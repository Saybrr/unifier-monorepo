//! Backend downloader implementations
//!
//! This module contains specific implementations of the FileDownloader trait.
//! Each backend handles a different protocol or source type.
//!
//! Currently supported:
//! - HTTP/HTTPS downloads with resume support
//! - WabbajackCDN chunked downloads
//!
//! Future backends might include:
//! - FTP downloads
//! - Google Drive downloads
//! - Cloud storage downloads (S3, Azure, etc.)

pub mod http;
pub mod wabbajack_cdn;

// Re-export main implementations
pub use http::HttpDownloader;
pub use wabbajack_cdn::WabbajackCDNDownloader;
