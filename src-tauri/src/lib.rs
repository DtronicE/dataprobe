use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Serialize, Deserialize)]
pub struct ExtensionCount {
    pub extension: String,
    pub count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DatasetInfo {
    pub file_name: String,
    pub file_path: String,
    pub format: String,
    pub row_count: usize,
    pub column_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct CleanOptions {
    pub remove_duplicates: bool,
    pub drop_null_rows: bool,
    pub drop_null_cols: bool,
    pub trim_whitespace: bool,
    pub fill_nulls: bool,
    pub fill_value: String,
}

#[derive(Serialize, Deserialize)]
pub struct CleanResult {
    pub original_rows: usize,
    pub cleaned_rows: usize,
    pub original_cols: usize,
    pub cleaned_cols: usize,
    pub duplicates_removed: usize,
    pub memory_reduced_mb: f64,
    pub completeness_pct: f64,
    pub output_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct BatchResult {
    pub files_processed: usize,
    pub total_memory_reduced_mb: f64,
    pub total_rows_cleaned: usize,
}

#[derive(Serialize, Deserialize)]
pub struct PreviewData {
    pub original_rows: usize,
    pub original_cols: usize,
    pub cleaned_rows: usize,
    pub cleaned_cols: usize,
    pub original_headers: Vec<String>,
    pub cleaned_headers: Vec<String>,
    pub original_preview: Vec<Vec<String>>,
    pub cleaned_preview: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
pub struct ScanResult {
    pub path: String,
    pub status: String,
    pub files_scanned: usize,
    pub total_bytes: u64,
    pub execution_time_ms: u64,
    pub top_extensions: Vec<ExtensionCount>,
    pub datasets: Vec<DatasetInfo>,
}

fn read_lazy_frame(path: &Path) -> Option<LazyFrame> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let path_str = path.to_str()?;

    match ext.as_str() {
        "csv" => LazyCsvReader::new(path_str)
            .with_has_header(true)
            .finish()
            .ok(),
        "parquet" => LazyFrame::scan_parquet(path_str, ScanArgsParquet::default()).ok(),
        "json" => {
            let file = fs::File::open(path).ok()?;
            let df = JsonReader::new(file).finish().ok()?;
            Some(df.lazy())
        }
        _ => None,
    }
}

fn inspect_dataset(path: &Path) -> Option<(usize, usize, String)> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let lf = read_lazy_frame(path)?;
    let df = lf.collect().ok()?;
    Some((df.height(), df.width(), ext))
}

fn scan_directory_recursive(
    dir: &Path,
    file_count: &mut usize,
    total_bytes: &mut u64,
    ext_map: &mut HashMap<String, usize>,
    datasets: &mut Vec<DatasetInfo>,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory_recursive(&path, file_count, total_bytes, ext_map, datasets);
            } else if path.is_file() {
                *file_count += 1;
                if let Ok(metadata) = entry.metadata() {
                    *total_bytes += metadata.len();
                }

                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    *ext_map.entry(ext_lower.clone()).or_insert(0) += 1;

                    if (ext_lower == "csv" || ext_lower == "parquet" || ext_lower == "json")
                        && datasets.len() < 15
                    {
                        if let Some((rows, cols, fmt)) = inspect_dataset(&path) {
                            datasets.push(DatasetInfo {
                                file_name: path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string(),
                                file_path: path.to_string_lossy().to_string(),
                                format: fmt,
                                row_count: rows,
                                column_count: cols,
                            });
                        }
                    }
                } else {
                    *ext_map.entry("no extension".to_string()).or_insert(0) += 1;
                }
            }
        }
    }
}

fn apply_cleaning_pipeline(mut lf: LazyFrame, options: &CleanOptions) -> Result<LazyFrame, String> {
    if options.remove_duplicates {
        lf = lf.unique(None, UniqueKeepStrategy::First);
    }

    if options.drop_null_rows {
        lf = lf.drop_nulls(None);
    }

    if options.trim_whitespace {
        let schema = lf.collect_schema().map_err(|e| e.to_string())?;
        for (name, dtype) in schema.iter() {
            if dtype == &DataType::String {
                let col_name = name.as_str();
                lf = lf.with_column(col(col_name).str().strip_chars(lit(NULL)).alias(col_name));
            }
        }
    }

    if options.fill_nulls {
        let schema = lf.collect_schema().map_err(|e| e.to_string())?;
        for (name, dtype) in schema.iter() {
            let col_name = name.as_str();
            match dtype {
                DataType::String => {
                    lf = lf.with_column(
                        col(col_name)
                            .fill_null(lit(options.fill_value.clone()))
                            .alias(col_name),
                    );
                }
                DataType::Int64 | DataType::Int32 | DataType::Float64 | DataType::Float32 => {
                    if let Ok(val) = options.fill_value.parse::<f64>() {
                        lf = lf.with_column(col(col_name).fill_null(lit(val)).alias(col_name));
                    }
                }
                _ => {}
            }
        }
    }

    Ok(lf)
}

fn dataframe_to_grid(df: &DataFrame, max_rows: usize) -> (Vec<String>, Vec<Vec<String>>) {
    let headers: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let height = df.height().min(max_rows);
    let mut grid = Vec::new();

    for r in 0..height {
        let mut row = Vec::new();
        for col_name in &headers {
            if let Ok(series) = df.column(col_name) {
                if let Ok(val) = series.get(r) {
                    row.push(format!("{}", val));
                } else {
                    row.push("null".to_string());
                }
            } else {
                row.push("null".to_string());
            }
        }
        grid.push(row);
    }

    (headers, grid)
}

#[tauri::command]
async fn run_dataprobe_scan(path: String) -> Result<ScanResult, String> {
    let target_path = Path::new(&path);
    if !target_path.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    let start_time = Instant::now();
    let mut file_count = 0;
    let mut total_bytes = 0;
    let mut ext_map: HashMap<String, usize> = HashMap::new();
    let mut datasets: Vec<DatasetInfo> = Vec::new();

    if target_path.is_dir() {
        scan_directory_recursive(
            target_path,
            &mut file_count,
            &mut total_bytes,
            &mut ext_map,
            &mut datasets,
        );
    } else {
        file_count = 1;
        if let Ok(metadata) = target_path.metadata() {
            total_bytes = metadata.len();
        }
        if let Some(ext) = target_path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            ext_map.insert(ext_lower.clone(), 1);
            if let Some((rows, cols, fmt)) = inspect_dataset(target_path) {
                datasets.push(DatasetInfo {
                    file_name: target_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    file_path: target_path.to_string_lossy().to_string(),
                    format: fmt,
                    row_count: rows,
                    column_count: cols,
                });
            }
        }
    }

    let mut top_extensions: Vec<ExtensionCount> = ext_map
        .into_iter()
        .map(|(extension, count)| ExtensionCount { extension, count })
        .collect();

    top_extensions.sort_by(|a, b| b.count.cmp(&a.count));
    top_extensions.truncate(5);

    let duration = start_time.elapsed().as_millis() as u64;

    Ok(ScanResult {
        path: path.clone(),
        status: "Success".to_string(),
        files_scanned: file_count,
        total_bytes,
        execution_time_ms: duration,
        top_extensions,
        datasets,
    })
}

#[tauri::command]
async fn preview_clean_dataset(
    file_path: String,
    options: CleanOptions,
) -> Result<PreviewData, String> {
    let path = Path::new(&file_path);
    let lf = read_lazy_frame(path).ok_or("Failed to parse dataset")?;

    let original_df = lf.clone().collect().map_err(|e| e.to_string())?;
    let orig_rows = original_df.height();
    let orig_cols = original_df.width();

    let cleaned_lf = apply_cleaning_pipeline(lf, &options)?;
    let mut cleaned_df = cleaned_lf.collect().map_err(|e| e.to_string())?;

    if options.drop_null_cols {
        let drop_cols: Vec<String> = cleaned_df
            .get_columns()
            .iter()
            .filter(|s| s.null_count() == cleaned_df.height() && cleaned_df.height() > 0)
            .map(|s| s.name().to_string())
            .collect();
        for col_name in drop_cols {
            let _ = cleaned_df.drop_in_place(&col_name);
        }
    }

    let (orig_headers, orig_grid) = dataframe_to_grid(&original_df, 6);
    let (clean_headers, clean_grid) = dataframe_to_grid(&cleaned_df, 6);

    Ok(PreviewData {
        original_rows: orig_rows,
        original_cols: orig_cols,
        cleaned_rows: cleaned_df.height(),
        cleaned_cols: cleaned_df.width(),
        original_headers: orig_headers,
        cleaned_headers: clean_headers,
        original_preview: orig_grid,
        cleaned_preview: clean_grid,
    })
}

#[tauri::command]
async fn clean_and_save_dataset(
    file_path: String,
    output_path: String,
    options: CleanOptions,
) -> Result<CleanResult, String> {
    let path = Path::new(&file_path);
    let original_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let lf = read_lazy_frame(path).ok_or("Failed to read dataset")?;
    let original_df = lf.clone().collect().map_err(|e| e.to_string())?;
    let orig_rows = original_df.height();
    let orig_cols = original_df.width();

    let cleaned_lf = apply_cleaning_pipeline(lf, &options)?;
    let mut cleaned_df = cleaned_lf.collect().map_err(|e| e.to_string())?;

    if options.drop_null_cols {
        let drop_cols: Vec<String> = cleaned_df
            .get_columns()
            .iter()
            .filter(|s| s.null_count() == cleaned_df.height() && cleaned_df.height() > 0)
            .map(|s| s.name().to_string())
            .collect();
        for col_name in drop_cols {
            let _ = cleaned_df.drop_in_place(&col_name);
        }
    }

    let out_ext = Path::new(&output_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("csv")
        .to_lowercase();

    let mut out_file = fs::File::create(&output_path).map_err(|e| e.to_string())?;

    match out_ext.as_str() {
        "parquet" => {
            ParquetWriter::new(&mut out_file)
                .finish(&mut cleaned_df)
                .map_err(|e| e.to_string())?;
        }
        "json" => {
            JsonWriter::new(&mut out_file)
                .finish(&mut cleaned_df)
                .map_err(|e| e.to_string())?;
        }
        _ => {
            CsvWriter::new(&mut out_file)
                .include_header(true)
                .finish(&mut cleaned_df)
                .map_err(|e| e.to_string())?;
        }
    }

    let new_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    let memory_reduced_mb = ((original_size as f64 - new_size as f64) / 1_048_576.0).max(0.0);

    let total_cells = (cleaned_df.height() * cleaned_df.width()) as f64;
    let mut null_cells = 0;
    for col in cleaned_df.get_columns() {
        null_cells += col.null_count();
    }
    let completeness_pct = if total_cells > 0.0 {
        ((total_cells - null_cells as f64) / total_cells) * 100.0
    } else {
        100.0
    };

    let duplicates_removed = if orig_rows >= cleaned_df.height() {
        orig_rows - cleaned_df.height()
    } else {
        0
    };

    Ok(CleanResult {
        original_rows: orig_rows,
        cleaned_rows: cleaned_df.height(),
        original_cols: orig_cols,
        cleaned_cols: cleaned_df.width(),
        duplicates_removed,
        memory_reduced_mb,
        completeness_pct,
        output_path,
    })
}

#[tauri::command]
async fn clean_batch_datasets(
    file_paths: Vec<String>,
    output_dir: String,
    target_format: String,
    options: CleanOptions,
) -> Result<BatchResult, String> {
    let mut files_processed = 0;
    let mut total_memory_reduced_mb = 0.0;
    let mut total_rows_cleaned = 0;

    let target_dir_path = Path::new(&output_dir);
    if !target_dir_path.exists() {
        fs::create_dir_all(target_dir_path)
            .map_err(|e| format!("Failed to create output dir: {}", e))?;
    }

    for file_path in file_paths {
        let path = Path::new(&file_path);
        if !path.exists() {
            continue;
        }

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cleaned_dataset");
        let out_filename = format!("cleaned_{}.{}", stem, target_format.to_lowercase());
        let out_path = target_dir_path.join(out_filename);
        let out_path_str = out_path.to_string_lossy().to_string();

        if let Ok(res) = clean_and_save_dataset(
            file_path,
            out_path_str,
            CleanOptions {
                remove_duplicates: options.remove_duplicates,
                drop_null_rows: options.drop_null_rows,
                drop_null_cols: options.drop_null_cols,
                trim_whitespace: options.trim_whitespace,
                fill_nulls: options.fill_nulls,
                fill_value: options.fill_value.clone(),
            },
        )
        .await
        {
            files_processed += 1;
            total_memory_reduced_mb += res.memory_reduced_mb;
            total_rows_cleaned += res.cleaned_rows;
        }
    }

    Ok(BatchResult {
        files_processed,
        total_memory_reduced_mb,
        total_rows_cleaned,
    })
}

#[tauri::command]
async fn export_scan_csv(save_path: String, content: String) -> Result<(), String> {
    fs::write(&save_path, content).map_err(|e| format!("Failed to write CSV: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            run_dataprobe_scan,
            export_scan_csv,
            preview_clean_dataset,
            clean_and_save_dataset,
            clean_batch_datasets
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
