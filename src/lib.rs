pub mod engine;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[pyfunction]
fn trim_csv_whitespace(input_path: String, output_path: String) -> PyResult<()> {
    let mut df = polars::prelude::LazyCsvReader::new(input_path)
        .with_has_header(true)
        .finish()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?
        .collect()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    engine::trim_string_columns(&mut df)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let file = std::fs::File::create(output_path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    polars::prelude::CsvWriter::new(file)
        .finish(&mut df)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(())
}

#[cfg(feature = "python")]
#[pymodule]
fn dataprobe(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(trim_csv_whitespace, m)?)?;
    Ok(())
}
