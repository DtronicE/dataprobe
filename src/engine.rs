use anyhow::Result;
use blake3::Hasher;
use polars::prelude::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum RuleType {
    NotNull,
    Min(f64),
    Max(f64),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rule {
    pub column: String,
    pub rule_type: RuleType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub rules: Vec<Rule>,
}

/// Compute BLAKE3 content hash for specific columns across all rows in parallel
pub fn compute_content_hashes(df: &DataFrame, target_columns: &[String]) -> Result<Vec<String>> {
    let height = df.height();

    let mut columns_data: Vec<Vec<String>> = Vec::new();
    for col_name in target_columns {
        let series = df.column(col_name)?;
        let str_vec: Vec<String> = (0..height)
            .map(|i| series.get(i).map(|v| v.to_string()).unwrap_or_default())
            .collect();
        columns_data.push(str_vec);
    }

    let hashes: Vec<String> = (0..height)
        .into_par_iter()
        .map(|row_idx| {
            let mut hasher = Hasher::new();
            for col in &columns_data {
                hasher.update(col[row_idx].as_bytes());
                hasher.update(b"|");
            }
            hasher.finalize().to_hex().to_string()
        })
        .collect();

    Ok(hashes)
}

/// Automatically strip leading and trailing whitespace from all String columns
pub fn trim_string_columns(df: &mut DataFrame) -> Result<()> {
    let column_names: Vec<String> = df
        .schema()
        .iter_fields()
        .filter(|field| matches!(field.dtype(), DataType::String))
        .map(|field| field.name().to_string())
        .collect();

    for col_name in column_names {
        let series = df.column(&col_name)?;
        let ca = series.str()?;

        let trimmed_ca: StringChunked = ca.apply_into_string_amortized(|val, output| {
            output.push_str(val.trim());
        });

        df.replace(&col_name, trimmed_ca.into_series())?;
    }

    Ok(())
}

/// Profile numeric columns for statistical anomalies beyond a given standard deviation threshold
pub fn profile_anomalies(df: &DataFrame, col_name: &str, std_threshold: f64) -> Result<()> {
    let series = df.column(col_name)?;
    let ca = series.cast(&DataType::Float64)?;
    let f64_ca = ca.f64()?;

    let mean = f64_ca.mean().unwrap_or(0.0);
    let std = f64_ca.std(1).unwrap_or(0.0);

    println!("\n📊 Anomaly Profile for column '{}':", col_name);
    println!("  Mean: {:.2}, StdDev: {:.2}", mean, std);

    let lower_bound = mean - (std_threshold * std);
    let upper_bound = mean + (std_threshold * std);

    println!(
        "  Valid Range ({:.1}σ): [{:.2}, {:.2}]",
        std_threshold, lower_bound, upper_bound
    );

    let mut outlier_count = 0;
    for val in f64_ca.into_iter().flatten() {
        if val < lower_bound || val > upper_bound {
            outlier_count += 1;
        }
    }

    println!("  ⚠️ Outliers detected: {}", outlier_count);
    Ok(())
}

/// Execute JSON validation rules against dataset
pub fn validate_rules(df: &DataFrame, config: &ValidationConfig) -> Result<()> {
    println!("\n🛡️ Executing Data Validation Rules:");
    let mut total_passed = true;

    for rule in &config.rules {
        let series = match df.column(&rule.column) {
            Ok(s) => s,
            Err(_) => {
                println!("  ❌ Column '{}' not found in dataset!", rule.column);
                total_passed = false;
                continue;
            }
        };

        match &rule.rule_type {
            RuleType::NotNull => {
                let null_count = series.null_count();
                if null_count == 0 {
                    println!("  ✅ [NotNull] Column '{}' passed (0 nulls)", rule.column);
                } else {
                    println!(
                        "  ❌ [NotNull] Column '{}' failed ({} nulls found)",
                        rule.column, null_count
                    );
                    total_passed = false;
                }
            }
            RuleType::Min(min_val) => {
                let ca = series.cast(&DataType::Float64)?;
                let min_found = ca.f64()?.min().unwrap_or(f64::MIN);
                if min_found >= *min_val {
                    println!(
                        "  ✅ [Min >= {:.2}] Column '{}' passed (Min: {:.2})",
                        min_val, rule.column, min_found
                    );
                } else {
                    println!(
                        "  ❌ [Min >= {:.2}] Column '{}' failed (Violating value: {:.2})",
                        min_val, rule.column, min_found
                    );
                    total_passed = false;
                }
            }
            RuleType::Max(max_val) => {
                let ca = series.cast(&DataType::Float64)?;
                let max_found = ca.f64()?.max().unwrap_or(f64::MAX);
                if max_found <= *max_val {
                    println!(
                        "  ✅ [Max <= {:.2}] Column '{}' passed (Max: {:.2})",
                        max_val, rule.column, max_found
                    );
                } else {
                    println!(
                        "  ❌ [Max <= {:.2}] Column '{}' failed (Violating value: {:.2})",
                        max_val, rule.column, max_found
                    );
                    total_passed = false;
                }
            }
        }
    }

    if total_passed {
        println!("\n🎉 Dataset passed all validation checks!");
    } else {
        println!("\n⚠️ Dataset failed validation assertions.");
    }

    Ok(())
}
