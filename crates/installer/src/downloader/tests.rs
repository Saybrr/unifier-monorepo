//! Comprehensive unit tests for the downloader module

use super::*;
use crate::downloader::{
    core::{ErrorSeverity, FileOperation, ValidationType, IntoProgressCallback, NullProgressReporter, ConsoleProgressReporter, CompositeProgressReporter},
};
use crate::downloader::sources::DownloadSource;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::{tempdir, TempDir};
use base64;
use xxhash_rust;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use crate::downloader::core::{DownloadConfig, DownloadMetrics};


/// Helper struct to capture progress events during testing
#[derive(Debug, Default)]
struct ProgressCapture {
    events: Arc<Mutex<Vec<ProgressEvent>>>,
}

impl ProgressCapture {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_callback(&self) -> ProgressCallback {
        let events = self.events.clone();
        Arc::new(move |event| {
            events.lock().unwrap().push(event);
        })
    }

    fn get_events(&self) -> Vec<ProgressEvent> {
        self.events.lock().unwrap().clone()
    }

    fn count_events_of_type(&self, event_type: &str) -> usize {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| match event {
                ProgressEvent::DownloadStarted { .. } => event_type == "download_started",
                ProgressEvent::DownloadProgress { .. } => event_type == "download_progress",
                ProgressEvent::DownloadComplete { .. } => event_type == "download_complete",
                ProgressEvent::ValidationStarted { .. } => event_type == "validation_started",
                ProgressEvent::ValidationProgress { .. } => event_type == "validation_progress",
                ProgressEvent::ValidationComplete { .. } => event_type == "validation_complete",
                ProgressEvent::RetryAttempt { .. } => event_type == "retry_attempt",
                ProgressEvent::Warning { .. } => event_type == "warning",
                ProgressEvent::Error { .. } => event_type == "error",
            })
            .count()
    }
}

/// Create a temporary file with specific content
async fn create_test_file(content: &[u8]) -> (TempDir, PathBuf) {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test_file.txt");
    tokio::fs::write(&file_path, content).await.unwrap();
    (temp_dir, file_path)
}

/// Calculate xxHash64 of data and return as base64 string (matching Wabbajack format)
fn calculate_xxhash64_base64(data: &[u8]) -> String {
    let hash = xxhash_rust::xxh64::xxh64(data, 0);
    let bytes = hash.to_le_bytes();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
}

#[cfg(test)]
mod file_validation_tests {
    use super::*;

    #[tokio::test]
    async fn test_file_validation_xxhash64_success() {
        let test_data = b"Hello, World!";
        let expected_hash = calculate_xxhash64_base64(test_data);
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new(expected_hash, test_data.len() as u64);
        let progress = ProgressCapture::new();

        let result = validation
            .validate_file(&file_path, Some(progress.get_callback()))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(progress.count_events_of_type("validation_started") > 0);
        assert!(progress.count_events_of_type("validation_complete") > 0);
    }

    #[tokio::test]
    async fn test_file_validation_xxhash64_failure() {
        let test_data = b"Hello, World!";
        let wrong_hash = "AAAAAAAAAA8="; // Intentionally wrong base64 xxhash64
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new(wrong_hash.to_string(), test_data.len() as u64);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Validation should fail
    }

    #[tokio::test]
    async fn test_file_validation_with_size_success() {
        let test_data = b"Hello, World!";
        let expected_size = test_data.len() as u64;
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new(String::new(), expected_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }


    #[tokio::test]
    async fn test_file_validation_size_success() {
        let test_data = b"Hello, World!";
        let expected_size = test_data.len() as u64;
        let expected_hash = calculate_xxhash64_base64(test_data);
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new(expected_hash, expected_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_file_validation_size_failure() {
        let test_data = b"Hello, World!";
        let wrong_size = 999u64; // Definitely wrong
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new("dGVzdA==".to_string(), wrong_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::SizeMismatch { expected, actual, file: _, diff: _ } => {
                assert_eq!(expected, wrong_size);
                assert_eq!(actual, test_data.len() as u64);
            }
            _ => panic!("Expected SizeMismatch error"),
        }
    }

    #[tokio::test]
    async fn test_file_validation_multiple_hashes_success() {
        let test_data = b"Hello, World!";
        let expected_xxhash64_base64 = calculate_xxhash64_base64(test_data);
        let expected_size = test_data.len() as u64;

        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new(expected_xxhash64_base64, expected_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_file_validation_nonexistent_file() {
        let validation = FileValidation::new("AAAAAAAAAA8=".to_string(), 1024);
        let fake_path = PathBuf::from("nonexistent_file.txt");

        let result = validation.validate_file(&fake_path, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::FileSystem { .. } => {}
            _ => panic!("Expected FileSystem error"),
        }
    }
}

#[cfg(test)]
mod download_request_tests {
    use super::*;

    #[test]
    fn test_download_request_creation() {
        let request: DownloadRequest = DownloadRequest::new_http("https://example.com/file.txt", "/tmp", "file.txt", 1024, "dGVzdA==".to_string());

        // In the new architecture, we test via description since we can't directly access URLs from trait objects
        assert!(request.get_description().contains("https://example.com/file.txt"));
        assert_eq!(request.destination, PathBuf::from("/tmp"));
        assert_eq!(request.filename, "file.txt");
    }

    #[test]
    fn test_download_request_with_mirror() {
        // With trait objects, we need to create HttpSource directly to test mirrors
        use crate::downloader::sources::HttpSource;

        let http_source = HttpSource::new("https://example.com/file.txt")
            .with_mirror("https://mirror.example.com/file.txt");
        let request = DownloadRequest::new(DownloadSource::Http(http_source), "/tmp", "file.txt", 1024, "dGVzdA==".to_string());

        // Test that the request was created successfully
        assert!(request.get_description().contains("https://example.com/file.txt"));
    }

    #[test]
    fn test_download_request_with_validation() {
        let validation = FileValidation::new("dGVzdA==".to_string(), 1024);
        let request = DownloadRequest::new_http("https://example.com/file.txt", "/tmp", "file.txt", 1024, "dGVzdA==".to_string());

        assert_eq!(request.validation.xxhash64_base64, validation.xxhash64_base64);
    }

    #[test]
    fn test_download_request_get_filename_from_url() {
        // Test that get_filename returns the explicit filename passed to the constructor
        let request = DownloadRequest::new_http("https://example.com/path/file.txt", "/tmp", "file.txt", 1024, "dGVzdA==".to_string());
        let filename = request.get_filename().unwrap();
        assert_eq!(filename, "file.txt"); // Returns the explicit filename
    }

    #[test]
    fn test_download_request_get_filename_explicit() {
        let request = DownloadRequest::new_http("https://example.com/path/", "/tmp", "custom_name.txt", 1024, "dGVzdA==".to_string())
;
        let filename = request.get_filename().unwrap();
        assert_eq!(filename, "custom_name.txt");
    }

    #[test]
    fn test_download_request_get_filename_fallback() {
        let request = DownloadRequest::new_http("https://example.com/", "/tmp", "downloaded_file", 1024, "dGVzdA==".to_string());
        let filename = request.get_filename().unwrap();
        assert_eq!(filename, "downloaded_file");
    }
}

#[cfg(test)]
mod http_downloader_tests {
    use super::*;

    async fn setup_mock_server() -> MockServer {
        MockServer::start().await
    }

    #[tokio::test]
    async fn test_enhanced_downloader_creation() {
        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);

        // Test that the downloader can be created and has metrics
        assert!(downloader.metrics().successful_downloads.load(std::sync::atomic::Ordering::Relaxed) == 0);
        assert!(downloader.metrics().failed_downloads.load(std::sync::atomic::Ordering::Relaxed) == 0);
    }

    #[tokio::test]
    async fn test_http_downloader_successful_download() {
        let mock_server = setup_mock_server().await;
        let test_content = b"Hello, World!";

        Mock::given(method("HEAD"))
            .and(path("/test-file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("content-length", test_content.len().to_string())
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/test-file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(test_content)
                    .append_header("content-length", test_content.len().to_string())
            )
            .mount(&mock_server)
            .await;

        let temp_dir = tempdir().unwrap();
        let url = format!("{}/test-file.txt", mock_server.uri());

        let expected_hash = calculate_xxhash64_base64(test_content);
        let request = DownloadRequest::new_http(url, temp_dir.path(), "test-file.txt", test_content.len() as u64, expected_hash);

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);
        let progress = ProgressCapture::new();

        let result = downloader
            .download(request, Some(progress.get_callback()))
            .await;

        assert!(result.is_ok());
        match result.unwrap() {
            DownloadResult::Downloaded { size } => {
                assert_eq!(size, test_content.len() as u64);
            }
            _ => panic!("Expected Downloaded result"),
        }

        // Check that file was created
        let file_path = temp_dir.path().join("test-file.txt");
        assert!(file_path.exists());

        // Verify content
        let downloaded_content = tokio::fs::read(&file_path).await.unwrap();
        assert_eq!(downloaded_content, test_content);

        // Check progress events
        assert!(progress.count_events_of_type("download_started") > 0);
        assert!(progress.count_events_of_type("download_complete") > 0);
    }

    #[tokio::test]
    async fn test_http_downloader_file_already_exists() {
        let test_content = b"Hello, World!";
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("existing-file.txt");

        // Pre-create the file
        tokio::fs::write(&file_path, test_content).await.unwrap();

        let expected_hash = calculate_xxhash64_base64(test_content);
        let request = DownloadRequest::new_http("https://example.com/file.txt", temp_dir.path(), "existing-file.txt", test_content.len() as u64, expected_hash);

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);

        let result = downloader.download(request, None).await;

        assert!(result.is_ok());
        match result.unwrap() {
            DownloadResult::AlreadyExists { size } => {
                assert_eq!(size, test_content.len() as u64);
            }
            _ => panic!("Expected AlreadyExists result"),
        }
    }

    #[tokio::test]
    async fn test_http_downloader_validation_failure() {
        let mock_server = setup_mock_server().await;
        let test_content = b"Hello, World!";

        Mock::given(method("HEAD"))
            .and(path("/test-file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("content-length", test_content.len().to_string())
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/test-file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(test_content)
            )
            .mount(&mock_server)
            .await;

        let temp_dir = tempdir().unwrap();
        let url = format!("{}/test-file.txt", mock_server.uri());

        // Use wrong hash to force validation failure
        let request = DownloadRequest::new_http(url, temp_dir.path(), "test-file.txt", test_content.len() as u64, "AAAAAAAAAA8=".to_string());

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);

        let result = downloader.download(request, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::ValidationFailed { .. } => {}
            _ => panic!("Expected ValidationFailed error"),
        }

        // File should be cleaned up after validation failure
        let file_path = temp_dir.path().join("test-file.txt");
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_http_downloader_server_error() {
        let mock_server = setup_mock_server().await;

        Mock::given(method("HEAD"))
            .and(path("/error-file.txt"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/error-file.txt"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let temp_dir = tempdir().unwrap();
        let url = format!("{}/error-file.txt", mock_server.uri());

        let request = DownloadRequest::new_http(url, temp_dir.path(), "error-file.txt", 1024, "dGVzdA==".to_string());

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);

        let result = downloader.download(request, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::HttpRequest { .. } => {}
            _ => panic!("Expected HttpRequest error"),
        }
    }
}

#[cfg(test)]
mod enhanced_downloader_tests {
    use super::*;

    async fn setup_mock_server_with_content(content: &[u8]) -> (MockServer, String) {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/test-file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("content-length", content.len().to_string())
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/test-file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(content)
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/test-file.txt", mock_server.uri());
        (mock_server, url)
    }

    #[tokio::test]
    async fn test_enhanced_downloader_successful_download() {
        let test_content = b"Hello, Enhanced Downloader!";
        let (_mock_server, url) = setup_mock_server_with_content(test_content).await;

        let temp_dir = tempdir().unwrap();
        let expected_hash = calculate_xxhash64_base64(test_content);
        let request = DownloadRequest::new_http(url, temp_dir.path(), "enhanced-test.txt", test_content.len() as u64, expected_hash);

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);
        let progress = ProgressCapture::new();

        let result = downloader
            .download(request, Some(progress.get_callback()))
            .await;

        assert!(result.is_ok());
        match result.unwrap() {
            DownloadResult::Downloaded { size } => {
                assert_eq!(size, test_content.len() as u64);
            }
            _ => panic!("Expected Downloaded result"),
        }

        // Verify file content
        let file_path = temp_dir.path().join("enhanced-test.txt");
        let downloaded_content = tokio::fs::read(&file_path).await.unwrap();
        assert_eq!(downloaded_content, test_content);

        // Check progress events
        assert!(progress.count_events_of_type("download_started") > 0);
        assert!(progress.count_events_of_type("download_complete") > 0);
    }

    #[tokio::test]
    #[ignore] // TODO: Fix mirror fallback in new architecture
    async fn test_enhanced_downloader_mirror_fallback() {
        let test_content = b"Hello from mirror!";

        // Primary server that always fails
        let primary_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&primary_server)
            .await;
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&primary_server)
            .await;

        // Mirror server that works
        let (_mirror_server, mirror_url) = setup_mock_server_with_content(test_content).await;

        let temp_dir = tempdir().unwrap();
        let primary_url = format!("{}/test-file.txt", primary_server.uri());

        // Create HttpSource with mirror directly
        use crate::downloader::sources::HttpSource;
        let http_source = HttpSource::new(primary_url)
            .with_mirror(mirror_url);
        let request = DownloadRequest::new(DownloadSource::Http(http_source), temp_dir.path(), "test-file.txt", test_content.len() as u64, "dGVzdA==".to_string());

        let mut config = DownloadConfig::default();
        config.max_retries = 2; // Reduce retries for faster test

        let downloader = Downloader::new(config);
        let progress = ProgressCapture::new();

        let result = downloader
            .download(request, Some(progress.get_callback()))
            .await;

        assert!(result.is_ok());
        match result.unwrap() {
            DownloadResult::Downloaded { size } => {
                assert_eq!(size, test_content.len() as u64);
            }
            _ => panic!("Expected Downloaded result"),
        }

        // Verify content came from mirror
        let file_path = temp_dir.path().join("test-file.txt");
        let downloaded_content = tokio::fs::read(&file_path).await.unwrap();
        assert_eq!(downloaded_content, test_content);
    }

    #[tokio::test]
    async fn test_enhanced_downloader_batch_download() {
        let test_content_1 = b"File 1 content";
        let test_content_2 = b"File 2 content";

        let (_server1, url1) = setup_mock_server_with_content(test_content_1).await;
        let (_server2, url2) = setup_mock_server_with_content(test_content_2).await;

        let temp_dir = tempdir().unwrap();

        let expected_hash_1 = calculate_xxhash64_base64(test_content_1);
        let expected_hash_2 = calculate_xxhash64_base64(test_content_2);
        let requests = vec![
            DownloadRequest::new_http(url1, temp_dir.path(), "file1.txt", test_content_1.len() as u64, expected_hash_1),
            DownloadRequest::new_http(url2, temp_dir.path(), "file2.txt", test_content_2.len() as u64, expected_hash_2),
        ];

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);
        let progress = ProgressCapture::new();

        let results = downloader
            .download_batch(requests, Some(progress.get_callback()), 2)
            .await;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());

        // Verify both files were downloaded
        let file1_path = temp_dir.path().join("file1.txt");
        let file2_path = temp_dir.path().join("file2.txt");

        assert!(file1_path.exists());
        assert!(file2_path.exists());

        let content1 = tokio::fs::read(&file1_path).await.unwrap();
        let content2 = tokio::fs::read(&file2_path).await.unwrap();

        assert_eq!(content1, test_content_1);
        assert_eq!(content2, test_content_2);

        // Should have progress events for both downloads
        assert!(progress.count_events_of_type("download_complete") >= 2);
    }

    #[tokio::test]
    #[ignore] // TODO: Implement retry logic in new architecture
    async fn test_enhanced_downloader_max_retries_exceeded() {
        // Server that always fails
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let temp_dir = tempdir().unwrap();
        let url = format!("{}/test-file.txt", mock_server.uri());

        let request = DownloadRequest::new_http(url, temp_dir.path(), "test-file.txt", 1024, "dGVzdA==".to_string());

        let mut config = DownloadConfig::default();
        config.max_retries = 2; // Small number for faster test

        let downloader = Downloader::new(config);

        let result = downloader.download(request, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::MaxRetriesExceeded { .. } => {}
            _ => panic!("Expected MaxRetriesExceeded error"),
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_end_to_end_download_with_validation() {
        let test_content = b"Integration test content for validation";
        let expected_xxhash64_base64 = calculate_xxhash64_base64(test_content);
        let expected_size = test_content.len() as u64;

        let (_mock_server, url) = {
            let mock_server = MockServer::start().await;

            Mock::given(method("HEAD"))
                .and(path("/validated-file.txt"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .append_header("content-length", test_content.len().to_string())
                )
                .mount(&mock_server)
                .await;

            Mock::given(method("GET"))
                .and(path("/validated-file.txt"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_bytes(test_content)
                )
                .mount(&mock_server)
                .await;

            let url = format!("{}/validated-file.txt", mock_server.uri());
            (mock_server, url)
        };

        let temp_dir = tempdir().unwrap();

        let request = DownloadRequest::new_http(url, temp_dir.path(), "validated-file.txt", expected_size, expected_xxhash64_base64);

        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);
        let progress = ProgressCapture::new();

        let result = downloader
            .download(request, Some(progress.get_callback()))
            .await;

        assert!(result.is_ok());

        // Verify file exists and has correct content
        let file_path = temp_dir.path().join("validated-file.txt");
        assert!(file_path.exists());

        let downloaded_content = tokio::fs::read(&file_path).await.unwrap();
        assert_eq!(downloaded_content, test_content);

        // Check that all expected progress events occurred
        let events = progress.get_events();
        assert!(!events.is_empty());

        let has_download_started = events.iter().any(|e| matches!(e, ProgressEvent::DownloadStarted { .. }));
        let has_download_complete = events.iter().any(|e| matches!(e, ProgressEvent::DownloadComplete { .. }));
        let has_validation_complete = events.iter().any(|e| matches!(e, ProgressEvent::ValidationComplete { valid: true, .. }));

        assert!(has_download_started);
        assert!(has_download_complete);
        assert!(has_validation_complete);
    }
}

#[cfg(test)]
mod enhanced_error_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_error_severity_ordering() {
        assert!(ErrorSeverity::Low < ErrorSeverity::Medium);
        assert!(ErrorSeverity::Medium < ErrorSeverity::High);
        assert!(ErrorSeverity::High < ErrorSeverity::Critical);
    }

    #[test]
    fn test_error_categorization() {
        let timeout_error = DownloadError::NetworkTimeout {
            url: "http://example.com".to_string(),
            duration_secs: 30,
        };

        assert_eq!(timeout_error.category(), "network_timeout");
        assert_eq!(timeout_error.severity(), ErrorSeverity::Medium);
        assert!(timeout_error.is_recoverable());
    }

    #[test]
    fn test_validation_error_context() {
        let file_path = PathBuf::from("/test/file.txt");
        let validation_error = DownloadError::ValidationFailed {
            file: file_path.clone(),
            validation_type: ValidationType::XxHash64,
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
            suggestion: "Re-download the file as it may be corrupted".to_string(),
        };

        assert_eq!(validation_error.category(), "validation_failed");
        assert_eq!(validation_error.severity(), ErrorSeverity::High);
        assert!(!validation_error.is_recoverable());
        assert!(validation_error.suggestion().is_some());
    }

    #[test]
    fn test_size_mismatch_error() {
        let file_path = PathBuf::from("/test/file.txt");
        let expected = 1000u64;
        let actual = 800u64;

        let size_error = DownloadError::SizeMismatch {
            file: file_path,
            expected,
            actual,
            diff: actual as i64 - expected as i64,
        };

        let error_msg = format!("{}", size_error);
        assert!(error_msg.contains("expected 1000 bytes"));
        assert!(error_msg.contains("got 800 bytes"));
        assert!(error_msg.contains("difference: -200 bytes"));
    }

    #[test]
    fn test_network_timeout_error() {
        let timeout_error = DownloadError::NetworkTimeout {
            url: "http://slow-server.com/file.zip".to_string(),
            duration_secs: 60,
        };

        assert!(timeout_error.is_recoverable());
        assert_eq!(timeout_error.severity(), ErrorSeverity::Medium);
        assert!(timeout_error.suggestion().unwrap().contains("timeout"));
    }

    #[test]
    fn test_detailed_error_report() {
        let error = DownloadError::InsufficientSpace {
            required: 1_000_000_000,
            available: 500_000_000,
            shortage: 500_000_000,
            path: PathBuf::from("/downloads"),
        };

        let report = error.detailed_report();
        assert!(report.contains("Category: insufficient_space"));
        assert!(report.contains("Severity: Critical"));
        assert!(report.contains("Recoverable: false"));
        assert!(report.contains("Suggestion:"));
    }

    #[test]
    fn test_error_context_builder() {
        let context = ErrorContext::new()
            .with_url("http://example.com/file.zip")
            .with_file("/tmp/download")
            .with_operation(FileOperation::Write);

        assert!(context.url.is_some());
        assert!(context.file.is_some());
        assert!(context.operation.is_some());
    }
}

#[cfg(test)]
mod config_builder_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let config = DownloadConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.user_agent, "installer/0.1.0");
        assert!(config.allow_resume);
        assert_eq!(config.max_concurrent_validations, 4);
        assert!(config.async_validation);
        assert_eq!(config.validation_retries, 2);
    }

}
#[cfg(test)]
mod progress_reporter_tests {
    use super::*;

    #[test]
    fn test_null_progress_reporter() {
        let reporter = NullProgressReporter;

        // These should not panic and should do nothing
        reporter.on_download_started("http://example.com", Some(1000));
        reporter.on_download_progress("http://example.com", 500, Some(1000), 100.0);
        reporter.on_download_complete("http://example.com", 1000);
    }

    #[test]
    fn test_console_progress_reporter_creation() {
        let reporter = ConsoleProgressReporter::new(true);
        assert!(reporter.verbose);

        let reporter = ConsoleProgressReporter::new(false);
        assert!(!reporter.verbose);
    }

    #[test]
    fn test_composite_progress_reporter() {
        let mut composite = CompositeProgressReporter::new();
        composite = composite.add_reporter(NullProgressReporter);
        composite = composite.add_reporter(NullProgressReporter);

        // Should not panic when calling methods
        composite.on_download_started("http://example.com", Some(1000));
        composite.on_error("http://example.com", "Test error");
    }

    #[test]
    fn test_progress_reporter_into_callback() {
        let reporter = NullProgressReporter;
        let callback = reporter.into_callback();

        // Should not panic when called
        callback(ProgressEvent::DownloadStarted {
            url: "http://example.com".to_string(),
            total_size: Some(1000),
        });
    }
}

#[cfg(test)]
mod metrics_tests {
    use super::*;

    #[test]
    fn test_download_metrics_default() {
        let metrics = DownloadMetrics::default();
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.total_downloads, 0);
        assert_eq!(snapshot.successful_downloads, 0);
        assert_eq!(snapshot.failed_downloads, 0);
        assert_eq!(snapshot.total_bytes, 0);
        assert_eq!(snapshot.success_rate(), 0.0);
        assert_eq!(snapshot.average_size(), 0.0);
    }

    #[test]
    fn test_download_metrics_recording() {
        let metrics = DownloadMetrics::default();

        metrics.record_download_started();
        metrics.record_download_completed(1000);

        metrics.record_download_started();
        metrics.record_download_failed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_downloads, 2);
        assert_eq!(snapshot.successful_downloads, 1);
        assert_eq!(snapshot.failed_downloads, 1);
        assert_eq!(snapshot.total_bytes, 1000);
        assert_eq!(snapshot.success_rate(), 0.5);
        assert_eq!(snapshot.average_size(), 1000.0);
    }

    #[test]
    fn test_cache_hits() {
        let metrics = DownloadMetrics::default();

        metrics.record_cache_hit(500);
        metrics.record_cache_hit(300);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.cache_hits, 2);
        assert_eq!(snapshot.total_bytes, 800);
    }

    #[test]
    fn test_validation_failures() {
        let metrics = DownloadMetrics::default();

        metrics.record_validation_failed();
        metrics.record_validation_failed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.validation_failures, 2);
    }

    #[test]
    fn test_retries() {
        let metrics = DownloadMetrics::default();

        metrics.record_retry();
        metrics.record_retry();
        metrics.record_retry();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.retries_attempted, 3);
    }
}
#[cfg(test)]
mod new_enhanced_downloader_tests {
    use super::*;

    #[test]
    fn test_enhanced_downloader_creation() {
        let config = DownloadConfig::default();
        let downloader = Downloader::new(config);

        let metrics = downloader.metrics();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_downloads, 0);
    }

    // Registry-based tests removed - new architecture uses trait objects directly
}

#[cfg(test)]
mod enhanced_integration_tests {
    use super::*;

    async fn setup_mock_server() -> MockServer {
        MockServer::start().await
    }

    #[tokio::test]
    #[ignore] // TODO: Fix metrics recording in new architecture
    async fn test_complete_download_workflow_with_enhanced_features() {
        let mock_server = setup_mock_server().await;
        let test_content = b"Integration test content with enhanced features!";
        let expected_xxhash64_base64 = calculate_xxhash64_base64(test_content);

        // Set up mock responses
        Mock::given(method("HEAD"))
            .and(path("/enhanced-test.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("content-length", test_content.len().to_string())
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/enhanced-test.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(test_content)
                    .append_header("content-length", test_content.len().to_string())
            )
            .mount(&mock_server)
            .await;

        // Create enhanced configuration
        let config = DownloadConfig::default();

        let downloader = Downloader::new(config);
        let temp_dir = tempdir().unwrap();
        let url = format!("{}/enhanced-test.txt", mock_server.uri());

        // Create request with validation
        let request = DownloadRequest::new_http(url, temp_dir.path(), "enhanced-test.txt", test_content.len() as u64, expected_xxhash64_base64);

        // Use enhanced progress reporting
        let progress_reporter = ConsoleProgressReporter::new(false); // Non-verbose for tests
        let progress_callback = Some(progress_reporter.into_callback());

        // Perform download
        let result = downloader.download(request, progress_callback).await;

        assert!(result.is_ok());

        match result.unwrap() {
            DownloadResult::Downloaded { size } => {
                assert_eq!(size, test_content.len() as u64);
            }
            _ => panic!("Expected Downloaded result"),
        }

        // Check metrics
        let metrics = downloader.metrics();
        let snapshot = metrics.snapshot();
        assert!(snapshot.successful_downloads > 0);
        assert!(snapshot.total_bytes > 0);
        assert!(snapshot.success_rate() > 0.0);
    }
}
