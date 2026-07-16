use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("required world-data source is missing: {0}")]
    MissingSource(PathBuf),
    #[error("failed to read {path}: {source}")]
    Csv { path: PathBuf, source: csv::Error },
    #[error("invalid {field} value {value:?} in {path}: {message}")]
    InvalidField {
        path: PathBuf,
        field: &'static str,
        value: String,
        message: String,
    },
    #[error("compiled world failed validation: {0}")]
    Validation(String),
    #[error("failed to serialize compiled world: {0}")]
    Json(#[from] serde_json::Error),
    #[error("world importer I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SpacetimeDB command failed while calling {reducer} with status {status}")]
    Spacetime { reducer: String, status: String },
}

pub type Result<T> = std::result::Result<T, Error>;
