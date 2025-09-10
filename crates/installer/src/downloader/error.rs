//! Error types for the downloader system

use thiserror::Error;

/// Custom error types for the downloader
#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("URL parse error: {0}")]
    UrlError(#[from] url::ParseError),

    #[error("File validation failed: expected {expected}, got {actual}")]
    ValidationError { expected: String, actual: String },

    #[error("Unsupported URL: {0}")]
    UnsupportedUrl(String),

    #[error("Download timeout")]
    Timeout,

    #[error("Maximum retry attempts exceeded")]
    MaxRetriesExceeded,

    #[error("File size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },

    #[error("Validation task failed: {0}")]
    ValidationTaskError(String),

    #[error("Hex decode error: {0}")]
    HexError(#[from] hex::FromHexError),
}

pub type Result<T> = std::result::Result<T, DownloadError>;

impl DownloadError {
    /// Check if error is recoverable (should retry)
    pub fn is_recoverable(&self) -> bool {
        matches!(self,
            DownloadError::HttpError(_) |
            DownloadError::IoError(_) |
            DownloadError::Timeout
        )
    }

    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            DownloadError::HttpError(_) => "http",
            DownloadError::IoError(_) => "io",
            DownloadError::UrlError(_) => "url",
            DownloadError::ValidationError { .. } => "validation",
            DownloadError::UnsupportedUrl(_) => "unsupported_url",
            DownloadError::Timeout => "timeout",
            DownloadError::MaxRetriesExceeded => "max_retries",
            DownloadError::SizeMismatch { .. } => "size_mismatch",
            DownloadError::ValidationTaskError(_) => "validation_task",
            DownloadError::HexError(_) => "hex",
        }
    }
}
