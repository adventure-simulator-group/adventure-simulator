use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("required world-data source is missing: {0}")]
    MissingSource(PathBuf),
    #[error("failed to read {path}: {source}")]
    Csv { path: PathBuf, source: csv::Error },
    #[error("failed to read TIFF {path}: {source}")]
    Tiff {
        path: PathBuf,
        source: tiff::TiffError,
    },
    #[error("failed to read shapefile {path}: {source}")]
    Shapefile {
        path: PathBuf,
        source: shapefile::Error,
    },
    #[error("coordinate projection failed: {0}")]
    Projection(#[from] proj4rs::errors::Error),
    #[error("failed to parse JSON source {path}: {source}")]
    JsonSource {
        path: PathBuf,
        source: serde_json::Error,
    },
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
