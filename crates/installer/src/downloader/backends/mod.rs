//! Backend downloader implementations
//!
//! This module contains specific implementations of the FileDownloader trait.
//! Each backend handles a different protocol or source type.
//!
//! Currently supported:
//! - HTTP/HTTPS downloads with resume support
//!
//! Future backends might include:
//! - FTP downloads
//! - Google Drive downloads
//! - Cloud storage downloads (S3, Azure, etc.)

pub mod http;

// Re-export main implementations
pub use http::HttpDownloader;
