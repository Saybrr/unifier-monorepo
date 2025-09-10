//! Enhanced error types for the downloader system with context and recovery information

use std::error::Error;
use std::path::PathBuf;
use thiserror::Error;

/// Comprehensive error types for the downloader with context and recovery information
#[derive(Error, Debug)]
pub enum DownloadError {
    /// HTTP-related errors with context
    #[error("HTTP request to '{url}' failed")]
    HttpRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// Network timeout with retry suggestion
    #[error("Request to '{url}' timed out after {duration_secs}s (try increasing timeout or check network)")]
    NetworkTimeout {
        url: String,
        duration_secs: u64,
    },

    /// File system I/O errors with file context
    #[error("File operation failed on '{path}'")]
    FileSystem {
        path: PathBuf,
        operation: FileOperation,
        #[source]
        source: std::io::Error,
    },

    /// URL parsing errors with helpful suggestions
    #[error("Invalid URL '{url}': {suggestion}")]
    InvalidUrl {
        url: String,
        suggestion: String,
        #[source]
        source: url::ParseError,
    },

    /// File validation errors with detailed context
    #[error("File validation failed for '{file}': {validation_type} mismatch")]
    ValidationFailed {
        file: PathBuf,
        validation_type: ValidationType,
        expected: String,
        actual: String,
        suggestion: String,
    },

    /// File size validation with helpful context
    #[error("File size mismatch for '{file}': expected {expected} bytes, got {actual} bytes (difference: {diff} bytes)")]
    SizeMismatch {
        file: PathBuf,
        expected: u64,
        actual: u64,
        diff: i64,
    },

    /// Unsupported URL schemes with alternatives
    #[error("Unsupported URL scheme in '{url}' (supported: {supported_schemes})")]
    UnsupportedUrl {
        url: String,
        scheme: String,
        supported_schemes: String,
    },

    /// Retry exhaustion with context
    #[error("Maximum retry attempts ({max_retries}) exceeded for '{url}' after {total_duration_secs}s")]
    MaxRetriesExceeded {
        url: String,
        max_retries: usize,
        total_duration_secs: u64,
        last_error: String,
    },

    /// Validation task execution errors
    #[error("Validation task failed for '{file}': {reason}")]
    ValidationTaskFailed {
        file: PathBuf,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Configuration errors
    #[error("Invalid configuration: {message}")]
    Configuration {
        message: String,
        field: Option<String>,
        suggestion: Option<String>,
    },

    /// Download cancelled by user or system
    #[error("Download cancelled: {reason}")]
    Cancelled {
        reason: String,
        url: Option<String>,
    },

    /// Insufficient disk space
    #[error("Insufficient disk space: need {required} bytes, available {available} bytes (short by {shortage} bytes)")]
    InsufficientSpace {
        required: u64,
        available: u64,
        shortage: u64,
        path: PathBuf,
    },

    /// Permission denied errors with suggestions
    #[error("Permission denied accessing '{path}': {suggestion}")]
    PermissionDenied {
        path: PathBuf,
        operation: FileOperation,
        suggestion: String,
        #[source]
        source: std::io::Error,
    },

    /// Legacy errors for backward compatibility
    #[error("Legacy error: {0}")]
    Legacy(String),
}

/// Types of file operations for error context
#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    Read,
    Write,
    Create,
    Delete,
    Move,
    Metadata,
    CreateDir,
}

impl std::fmt::Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::Read => write!(f, "reading"),
            FileOperation::Write => write!(f, "writing"),
            FileOperation::Create => write!(f, "creating"),
            FileOperation::Delete => write!(f, "deleting"),
            FileOperation::Move => write!(f, "moving"),
            FileOperation::Metadata => write!(f, "reading metadata"),
            FileOperation::CreateDir => write!(f, "creating directory"),
        }
    }
}

/// Types of validation for error context
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationType {
    Crc32,
    Md5,
    Sha256,
    Size,
}

impl std::fmt::Display for ValidationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationType::Crc32 => write!(f, "CRC32"),
            ValidationType::Md5 => write!(f, "MD5"),
            ValidationType::Sha256 => write!(f, "SHA256"),
            ValidationType::Size => write!(f, "file size"),
        }
    }
}

pub type Result<T> = std::result::Result<T, DownloadError>;

impl DownloadError {
    /// Check if error is recoverable (should retry)
    pub fn is_recoverable(&self) -> bool {
        match self {
            DownloadError::HttpRequest { source, .. } => {
                // Only retry on network-related HTTP errors, not client errors (4xx)
                source.status().map_or(true, |status| status.is_server_error() || status == 429)
            }
            DownloadError::NetworkTimeout { .. } => true,
            DownloadError::FileSystem { source, .. } => {
                // Retry on temporary file system issues
                matches!(source.kind(),
                    std::io::ErrorKind::Interrupted |
                    std::io::ErrorKind::TimedOut |
                    std::io::ErrorKind::WouldBlock
                )
            }
            DownloadError::MaxRetriesExceeded { .. } => false, // Already exhausted retries
            DownloadError::ValidationFailed { .. } => false,  // Data integrity issue
            DownloadError::SizeMismatch { .. } => false,       // Data integrity issue
            DownloadError::InvalidUrl { .. } => false,        // Configuration issue
            DownloadError::UnsupportedUrl { .. } => false,    // Configuration issue
            DownloadError::Configuration { .. } => false,     // Configuration issue
            DownloadError::PermissionDenied { .. } => false,  // System permission issue
            DownloadError::InsufficientSpace { .. } => false, // System resource issue
            DownloadError::Cancelled { .. } => false,         // Intentionally stopped
            DownloadError::ValidationTaskFailed { .. } => true, // Could be temporary
            DownloadError::Legacy(_) => false,                // Unknown legacy error
        }
    }

    /// Get error category for metrics and logging
    pub fn category(&self) -> &'static str {
        match self {
            DownloadError::HttpRequest { .. } => "http_request",
            DownloadError::NetworkTimeout { .. } => "network_timeout",
            DownloadError::FileSystem { .. } => "file_system",
            DownloadError::InvalidUrl { .. } => "invalid_url",
            DownloadError::ValidationFailed { .. } => "validation_failed",
            DownloadError::SizeMismatch { .. } => "size_mismatch",
            DownloadError::UnsupportedUrl { .. } => "unsupported_url",
            DownloadError::MaxRetriesExceeded { .. } => "max_retries_exceeded",
            DownloadError::ValidationTaskFailed { .. } => "validation_task_failed",
            DownloadError::Configuration { .. } => "configuration",
            DownloadError::Cancelled { .. } => "cancelled",
            DownloadError::InsufficientSpace { .. } => "insufficient_space",
            DownloadError::PermissionDenied { .. } => "permission_denied",
            DownloadError::Legacy(_) => "legacy",
        }
    }

    /// Get severity level for error prioritization
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            DownloadError::HttpRequest { .. } => ErrorSeverity::Medium,
            DownloadError::NetworkTimeout { .. } => ErrorSeverity::Medium,
            DownloadError::FileSystem { .. } => ErrorSeverity::High,
            DownloadError::InvalidUrl { .. } => ErrorSeverity::High,
            DownloadError::ValidationFailed { .. } => ErrorSeverity::High,
            DownloadError::SizeMismatch { .. } => ErrorSeverity::High,
            DownloadError::UnsupportedUrl { .. } => ErrorSeverity::High,
            DownloadError::MaxRetriesExceeded { .. } => ErrorSeverity::High,
            DownloadError::ValidationTaskFailed { .. } => ErrorSeverity::Medium,
            DownloadError::Configuration { .. } => ErrorSeverity::High,
            DownloadError::Cancelled { .. } => ErrorSeverity::Low,
            DownloadError::InsufficientSpace { .. } => ErrorSeverity::Critical,
            DownloadError::PermissionDenied { .. } => ErrorSeverity::Critical,
            DownloadError::Legacy(_) => ErrorSeverity::Medium,
        }
    }

    /// Get user-friendly suggestion for resolving the error
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            DownloadError::NetworkTimeout { .. } => {
                Some("Check your internet connection or try increasing the timeout value")
            }
            DownloadError::InvalidUrl { suggestion, .. } => Some(suggestion),
            DownloadError::ValidationFailed { suggestion, .. } => Some(suggestion),
            DownloadError::UnsupportedUrl { .. } => {
                Some("Use a supported URL scheme (http/https)")
            }
            DownloadError::InsufficientSpace { .. } => {
                Some("Free up disk space or choose a different download location")
            }
            DownloadError::PermissionDenied { suggestion, .. } => Some(suggestion),
            DownloadError::Configuration { suggestion, .. } => suggestion.as_deref(),
            _ => None,
        }
    }

    /// Create a detailed error report for debugging
    pub fn detailed_report(&self) -> String {
        let mut report = format!("Error: {}\n", self);
        report.push_str(&format!("Category: {}\n", self.category()));
        report.push_str(&format!("Severity: {:?}\n", self.severity()));
        report.push_str(&format!("Recoverable: {}\n", self.is_recoverable()));

        if let Some(suggestion) = self.suggestion() {
            report.push_str(&format!("Suggestion: {}\n", suggestion));
        }

        if let Some(source) = self.source() {
            report.push_str(&format!("Root cause: {}\n", source));
        }

        report
    }
}

/// Error severity levels for prioritization
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Error context helper for building rich errors
pub struct ErrorContext {
    pub url: Option<String>,
    pub file: Option<PathBuf>,
    pub operation: Option<FileOperation>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self {
            url: None,
            file: None,
            operation: None,
        }
    }

    pub fn with_url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_file<P: Into<PathBuf>>(mut self, file: P) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_operation(mut self, operation: FileOperation) -> Self {
        self.operation = Some(operation);
        self
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::new()
    }
}

// Enhanced error conversion implementations with context
impl From<reqwest::Error> for DownloadError {
    fn from(error: reqwest::Error) -> Self {
        let url = error.url().map(|u| u.to_string()).unwrap_or_else(|| "<unknown>".to_string());

        if error.is_timeout() {
            DownloadError::NetworkTimeout {
                url,
                duration_secs: 30, // Default timeout assumption
            }
        } else {
            DownloadError::HttpRequest {
                url,
                source: error,
            }
        }
    }
}

impl From<std::io::Error> for DownloadError {
    fn from(error: std::io::Error) -> Self {
        DownloadError::FileSystem {
            path: PathBuf::from("<unknown>"),
            operation: FileOperation::Read, // Default assumption
            source: error,
        }
    }
}

impl From<url::ParseError> for DownloadError {
    fn from(error: url::ParseError) -> Self {
        let suggestion = match error {
            url::ParseError::EmptyHost => "URL must have a valid hostname",
            url::ParseError::InvalidPort => "Port number must be between 1 and 65535",
            url::ParseError::InvalidIpv4Address => "Invalid IPv4 address format",
            url::ParseError::InvalidIpv6Address => "Invalid IPv6 address format",
            url::ParseError::RelativeUrlWithoutBase => "URL must be absolute (include http:// or https://)",
            _ => "Check URL format and try again",
        }.to_string();

        DownloadError::InvalidUrl {
            url: "<unparseable>".to_string(),
            suggestion,
            source: error,
        }
    }
}
