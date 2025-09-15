# Installer Library

A comprehensive Rust library for downloading and validating files with advanced features like retry logic, progress tracking, and multiple validation methods.

## Features

- 🚀 **Multiple download sources**: HTTP/HTTPS with extensible support for other protocols
- 🔍 **File validation**: CRC32, MD5, and SHA256 hash verification with size checking
- 🔄 **Retry logic**: Configurable retry attempts with exponential backoff
- 🪞 **Mirror fallback**: Automatic fallback to mirror URLs on primary failure
- ⏯️ **Resume capability**: Resume interrupted downloads
- 📊 **Progress tracking**: Real-time progress events with speed calculation
- 📦 **Batch downloads**: Download multiple files concurrently with configurable limits
- ⚡ **Async/await**: Full async support with Tokio runtime

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
installer = { path = "path/to/installer" }
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use installer::{
    DownloadConfig, DownloadRequest, EnhancedDownloader,
    FileValidation, ProgressEvent
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> installer::Result<()> {
    // Create a download configuration
    let config = DownloadConfig::default();

    // Create the downloader
    let downloader = EnhancedDownloader::new(config);

    // Set up file validation (optional)
    let validation = FileValidation::new()
        .with_crc32(0x12345678)
        .with_expected_size(1024);

    // Create a download request
    let request = DownloadRequest::new(
        "https://example.com/file.zip",
        "/path/to/download/directory"
    )
    .with_filename("file.zip")
    .with_mirror_url("https://mirror.example.com/file.zip")
    .with_validation(validation);

    // Set up progress callback (optional)
    let progress_callback = Arc::new(|event: ProgressEvent| {
        match event {
            ProgressEvent::DownloadStarted { url, total_size } => {
                println!("Started downloading: {} ({:?} bytes)", url, total_size);
            }
            ProgressEvent::DownloadProgress { downloaded, total, speed_bps, .. } => {
                if let Some(total) = total {
                    let percent = (downloaded as f64 / total as f64) * 100.0;
                    println!("Progress: {:.1}% ({:.1} KB/s)", percent, speed_bps / 1024.0);
                }
            }
            ProgressEvent::DownloadComplete { final_size, .. } => {
                println!("Download complete: {} bytes", final_size);
            }
            _ => {}
        }
    });

    // Download the file
    let result = downloader.download(request, Some(progress_callback)).await?;
    println!("Download result: {:?}", result);

    Ok(())
}
```

## Advanced Usage

### Batch Downloads

```rust
let requests = vec![
    DownloadRequest::new("https://example.com/file1.zip", "/tmp"),
    DownloadRequest::new("https://example.com/file2.zip", "/tmp"),
    DownloadRequest::new("https://example.com/file3.zip", "/tmp"),
];

let results = downloader
    .download_batch(requests, Some(progress_callback), 3) // Max 3 concurrent downloads
    .await;

for (i, result) in results.iter().enumerate() {
    match result {
        Ok(download_result) => println!("File {} downloaded: {:?}", i, download_result),
        Err(e) => println!("File {} failed: {}", i, e),
    }
}
```

### File Validation

The library supports multiple validation methods:

```rust
let validation = FileValidation::new()
    .with_crc32(0x12345678)                              // CRC32 checksum
    .with_md5("d41d8cd98f00b204e9800998ecf8427e".to_string()) // MD5 hash
    .with_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()) // SHA256 hash
    .with_expected_size(1024);                           // Expected file size
```

### Custom Configuration

```rust
let config = DownloadConfig {
    max_retries: 5,                                 // Retry up to 5 times
    timeout: Duration::from_secs(60),               // 60 second timeout
    user_agent: "MyApp/1.0".to_string(),           // Custom user agent
    allow_resume: true,                             // Enable resume capability
    chunk_size: 16384,                              // 16KB chunks
};

let downloader = EnhancedDownloader::new(config);
```

### Progress Events

The library emits detailed progress events:

```rust
let progress_callback = Arc::new(|event: ProgressEvent| {
    match event {
        ProgressEvent::DownloadStarted { url, total_size } => {
            // Download started
        }
        ProgressEvent::DownloadProgress { url, downloaded, total, speed_bps } => {
            // Progress update with speed calculation
        }
        ProgressEvent::DownloadComplete { url, final_size } => {
            // Download completed successfully
        }
        ProgressEvent::ValidationStarted { file } => {
            // File validation started
        }
        ProgressEvent::ValidationProgress { file, progress } => {
            // Validation progress (0.0 to 1.0)
        }
        ProgressEvent::ValidationComplete { file, valid } => {
            // Validation completed
        }
        ProgressEvent::RetryAttempt { url, attempt, max_attempts } => {
            // Retry attempt notification
        }
        ProgressEvent::Error { url, error } => {
            // Error occurred
        }
    }
});
```

## Error Handling

The library provides detailed error information:

```rust
match downloader.download(request, None).await {
    Ok(result) => println!("Success: {:?}", result),
    Err(installer::DownloadError::HttpError(e)) => {
        println!("HTTP error: {}", e);
    }
    Err(installer::DownloadError::ValidationError { expected, actual }) => {
        println!("Validation failed: expected {}, got {}", expected, actual);
    }
    Err(installer::DownloadError::SizeMismatch { expected, actual }) => {
        println!("Size mismatch: expected {} bytes, got {}", expected, actual);
    }
    Err(e) => println!("Other error: {}", e),
}
```

## Examples

Run the included example:

```bash
cargo run --example download_example
```

## Testing

Run the test suite:

```bash
cargo test
```

The tests include:
- File validation tests for all hash types
- HTTP download simulation with mock servers
- Retry logic and mirror fallback testing
- Batch download testing
- Integration tests with real network conditions

## Architecture

The library is built around these core components:

- **`FileDownloader` trait**: Extensible interface for different download sources
- **`HttpDownloader`**: HTTP/HTTPS implementation with resume support
- **`DownloaderRegistry`**: Manages multiple downloader implementations
- **`EnhancedDownloader`**: High-level interface with retry and mirror support
- **`FileValidation`**: Comprehensive file integrity checking
- **Progress system**: Event-driven progress reporting

## License

This project is licensed under the MIT License - see the LICENSE file for details.
