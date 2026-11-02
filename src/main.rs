mod engine;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use polars::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "dataprobe")]
#[command(about = "High-performance data audit and cleaning engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect dataset schema and missing value counts
    Audit {
        #[arg(short, long)]
        input: PathBuf,
    },
    /// Deduplicate rows using Polars exact row match
    Dedupe {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Deduplicate based on target content columns using parallel BLAKE3 hashing
    HashDedupe {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long, value_delimiter = ',')]
        columns: Vec<String>,
    },
    /// Sanitize dataset by trimming whitespace from all string fields
    Clean {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Profile statistical anomalies on a target numeric column
    Profile {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        column: String,
        #[arg(short, long, default_value_t = 2.0)]
        threshold: f64,
    },
    /// Validate dataset against JSON assertions
    Validate {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        rules: PathBuf,
    },
    /// Convert dataset between CSV, Parquet, and JSONL formats seamlessly
    Convert {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
}

/// Utility function to ingest datasets dynamically by file extension
fn read_dataset(path: &Path) -> Result<DataFrame> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "csv" => Ok(LazyCsvReader::new(path)
            .with_has_header(true)
            .finish()?
            .collect()?),
        "parquet" => Ok(LazyFrame::scan_parquet(path, Default::default())?.collect()?),
        "jsonl" | "json" => Ok(JsonReader::new(std::fs::File::open(path)?).finish()?),
        _ => Err(anyhow!("Unsupported input file extension: {}", extension)),
    }
}

/// Utility function to export dataframes dynamically by file extension
fn write_dataset(df: &mut DataFrame, path: &Path) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    let file = std::fs::File::create(path)?;

    match extension.as_str() {
        "csv" => {
            CsvWriter::new(file).finish(df)?;
        }
        "parquet" => {
            ParquetWriter::new(file).finish(df)?;
        }
        "jsonl" | "json" => {
            JsonWriter::new(file).finish(df)?;
        }
        _ => return Err(anyhow!("Unsupported output file extension: {}", extension)),
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Audit { input } => {
            println!("⚡ Auditing dataset: {:?}", input);
            let df = read_dataset(&input)?;

            println!("\n--- Dataset Shape ---");
            println!("Rows: {}, Columns: {}", df.height(), df.width());

            println!("\n--- Schema Overview ---");
            for field in df.schema().iter_fields() {
                println!("{}: {:?}", field.name(), field.dtype());
            }

            println!("\n--- Missing Values Audit ---");
            println!("{}", df.null_count());
        }

        Commands::Dedupe { input, output } => {
            println!("⚡ Deduplicating dataset: {:?}", input);
            let mut df = read_dataset(&input)?;

            let initial_rows = df.height();
            df = df.unique_stable(None, UniqueKeepStrategy::First, None)?;
            let final_rows = df.height();

            println!("✅ Removed {} duplicate rows.", initial_rows - final_rows);
            println!("📊 Clean dataset contains {} unique rows.", final_rows);

            write_dataset(&mut df, &output)?;
            println!("📁 Clean file saved to {:?}", output);
        }

        Commands::HashDedupe {
            input,
            output,
            columns,
        } => {
            println!(
                "⚡ Running BLAKE3 parallel hash deduplication on columns: {:?}",
                columns
            );
            let mut df = read_dataset(&input)?;

            let initial_rows = df.height();
            let hashes = engine::compute_content_hashes(&df, &columns)?;
            let hash_series = Column::new("dataprobe_hash".into(), hashes);
            df.with_column(hash_series)?;

            let column_subset: Vec<String> = vec!["dataprobe_hash".to_string()];
            df = df.unique_stable(Some(&column_subset), UniqueKeepStrategy::First, None)?;
            df.drop_in_place("dataprobe_hash")?;

            let final_rows = df.height();
            println!(
                "✅ Content Hash Dedupe complete. Removed {} duplicates.",
                initial_rows - final_rows
            );
            println!("📊 Clean dataset contains {} unique rows.", final_rows);

            write_dataset(&mut df, &output)?;
            println!("📁 Clean file saved to {:?}", output);
        }

        Commands::Clean { input, output } => {
            println!("⚡ Sanitizing dataset strings: {:?}", input);
            let mut df = read_dataset(&input)?;

            engine::trim_string_columns(&mut df)?;
            println!("✅ String whitespace trimming complete.");

            write_dataset(&mut df, &output)?;
            println!("📁 Cleaned file saved to {:?}", output);
        }

        Commands::Profile {
            input,
            column,
            threshold,
        } => {
            println!(
                "⚡ Profiling numeric column '{}' in dataset: {:?}",
                column, input
            );
            let df = read_dataset(&input)?;
            engine::profile_anomalies(&df, &column, threshold)?;
        }

        Commands::Validate { input, rules } => {
            println!(
                "⚡ Validating dataset {:?} against rules {:?}",
                input, rules
            );
            let df = read_dataset(&input)?;
            let rules_content = fs::read_to_string(&rules)?;
            let config: engine::ValidationConfig = serde_json::from_str(&rules_content)?;
            engine::validate_rules(&df, &config)?;
        }

        Commands::Convert { input, output } => {
            println!("⚡ Converting format from {:?} to {:?}", input, output);
            let mut df = read_dataset(&input)?;
            write_dataset(&mut df, &output)?;
            println!("✅ Converted successfully to {:?}", output);
        }
    }

    Ok(())
}
