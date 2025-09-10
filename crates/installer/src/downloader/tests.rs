//! Comprehensive unit tests for the downloader module

use super::*;
use std::sync::{Arc, Mutex};
use tempfile::{tempdir, TempDir};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

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

/// Calculate CRC32 of data
fn calculate_crc32(data: &[u8]) -> u32 {
    let mut hasher = Crc32Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// Calculate MD5 hash of data
fn calculate_md5(data: &[u8]) -> String {
    let mut hasher = md5::Context::new();
    hasher.consume(data);
    format!("{:x}", hasher.compute())
}

/// Calculate SHA256 hash of data
fn calculate_sha256(data: &[u8]) -> String {
    use digest::Digest;
    use sha2::Sha256;
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod file_validation_tests {
    use super::*;

    #[tokio::test]
    async fn test_file_validation_crc32_success() {
        let test_data = b"Hello, World!";
        let expected_crc32 = calculate_crc32(test_data);
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_crc32(expected_crc32);
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
    async fn test_file_validation_crc32_failure() {
        let test_data = b"Hello, World!";
        let wrong_crc32 = 0x12345678; // Definitely wrong
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_crc32(wrong_crc32);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Validation should fail
    }

    #[tokio::test]
    async fn test_file_validation_md5_success() {
        let test_data = b"Hello, World!";
        let expected_md5 = calculate_md5(test_data);
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_md5(expected_md5);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_file_validation_md5_failure() {
        let test_data = b"Hello, World!";
        let wrong_md5 = "wrong_hash".to_string();
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_md5(wrong_md5);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Validation should fail
    }

    #[tokio::test]
    async fn test_file_validation_sha256_success() {
        let test_data = b"Hello, World!";
        let expected_sha256 = calculate_sha256(test_data);
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_sha256(expected_sha256);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_file_validation_size_success() {
        let test_data = b"Hello, World!";
        let expected_size = test_data.len() as u64;
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_expected_size(expected_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_file_validation_size_failure() {
        let test_data = b"Hello, World!";
        let wrong_size = 999u64; // Definitely wrong
        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new().with_expected_size(wrong_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::SizeMismatch { expected, actual } => {
                assert_eq!(expected, wrong_size);
                assert_eq!(actual, test_data.len() as u64);
            }
            _ => panic!("Expected SizeMismatch error"),
        }
    }

    #[tokio::test]
    async fn test_file_validation_multiple_hashes_success() {
        let test_data = b"Hello, World!";
        let expected_crc32 = calculate_crc32(test_data);
        let expected_md5 = calculate_md5(test_data);
        let expected_sha256 = calculate_sha256(test_data);
        let expected_size = test_data.len() as u64;

        let (_temp_dir, file_path) = create_test_file(test_data).await;

        let validation = FileValidation::new()
            .with_crc32(expected_crc32)
            .with_md5(expected_md5)
            .with_sha256(expected_sha256)
            .with_expected_size(expected_size);

        let result = validation.validate_file(&file_path, None).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_file_validation_nonexistent_file() {
        let validation = FileValidation::new().with_crc32(0x12345678);
        let fake_path = PathBuf::from("nonexistent_file.txt");

        let result = validation.validate_file(&fake_path, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::IoError(_) => {}
            _ => panic!("Expected IoError"),
        }
    }
}

#[cfg(test)]
mod download_request_tests {
    use super::*;

    #[test]
    fn test_download_request_creation() {
        let request = DownloadRequest::new("https://example.com/file.txt", "/tmp");

        assert_eq!(request.url, "https://example.com/file.txt");
        assert_eq!(request.destination, PathBuf::from("/tmp"));
        assert!(request.mirror_url.is_none());
        assert!(request.filename.is_none());
    }

    #[test]
    fn test_download_request_with_mirror() {
        let request = DownloadRequest::new("https://example.com/file.txt", "/tmp")
            .with_mirror_url("https://mirror.example.com/file.txt");

        assert_eq!(
            request.mirror_url,
            Some("https://mirror.example.com/file.txt".to_string())
        );
    }

    #[test]
    fn test_download_request_with_validation() {
        let validation = FileValidation::new().with_crc32(0x12345678);
        let request = DownloadRequest::new("https://example.com/file.txt", "/tmp")
            .with_validation(validation.clone());

        assert_eq!(request.validation.crc32, validation.crc32);
    }

    #[test]
    fn test_download_request_get_filename_from_url() {
        let request = DownloadRequest::new("https://example.com/path/file.txt", "/tmp");
        let filename = request.get_filename().unwrap();
        assert_eq!(filename, "file.txt");
    }

    #[test]
    fn test_download_request_get_filename_explicit() {
        let request = DownloadRequest::new("https://example.com/path/", "/tmp")
            .with_filename("custom_name.txt");
        let filename = request.get_filename().unwrap();
        assert_eq!(filename, "custom_name.txt");
    }

    #[test]
    fn test_download_request_get_filename_fallback() {
        let request = DownloadRequest::new("https://example.com/", "/tmp");
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
    async fn test_http_downloader_supports_url() {
        let config = DownloadConfig::default();
        let downloader = HttpDownloader::new(config);

        assert!(downloader.supports_url("http://example.com/file.txt"));
        assert!(downloader.supports_url("https://example.com/file.txt"));
        assert!(!downloader.supports_url("ftp://example.com/file.txt"));
        assert!(!downloader.supports_url("file:///local/file.txt"));
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

        let request = DownloadRequest::new(url, temp_dir.path())
            .with_filename("test-file.txt");

        let config = DownloadConfig::default();
        let downloader = HttpDownloader::new(config);
        let progress = ProgressCapture::new();

        let result = downloader
            .download(&request, Some(progress.get_callback()))
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

        let request = DownloadRequest::new("https://example.com/file.txt", temp_dir.path())
            .with_filename("existing-file.txt");

        let config = DownloadConfig::default();
        let downloader = HttpDownloader::new(config);

        let result = downloader.download(&request, None).await;

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

        // Use wrong CRC32 to force validation failure
        let validation = FileValidation::new().with_crc32(0x12345678);
        let request = DownloadRequest::new(url, temp_dir.path())
            .with_filename("test-file.txt")
            .with_validation(validation);

        let config = DownloadConfig::default();
        let downloader = HttpDownloader::new(config);

        let result = downloader.download(&request, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::ValidationError { .. } => {}
            _ => panic!("Expected ValidationError"),
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

        let request = DownloadRequest::new(url, temp_dir.path())
            .with_filename("error-file.txt");

        let config = DownloadConfig::default();
        let downloader = HttpDownloader::new(config);

        let result = downloader.download(&request, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::HttpError(_) => {}
            _ => panic!("Expected HttpError"),
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
        let request = DownloadRequest::new(url, temp_dir.path())
            .with_filename("test-file.txt");

        let config = DownloadConfig::default();
        let downloader = EnhancedDownloader::new(config);
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
        let file_path = temp_dir.path().join("test-file.txt");
        let downloaded_content = tokio::fs::read(&file_path).await.unwrap();
        assert_eq!(downloaded_content, test_content);

        // Check progress events
        assert!(progress.count_events_of_type("download_started") > 0);
        assert!(progress.count_events_of_type("download_complete") > 0);
    }

    #[tokio::test]
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

        let request = DownloadRequest::new(primary_url, temp_dir.path())
            .with_mirror_url(mirror_url)
            .with_filename("test-file.txt");

        let mut config = DownloadConfig::default();
        config.max_retries = 2; // Reduce retries for faster test

        let downloader = EnhancedDownloader::new(config);
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

        let requests = vec![
            DownloadRequest::new(url1, temp_dir.path()).with_filename("file1.txt"),
            DownloadRequest::new(url2, temp_dir.path()).with_filename("file2.txt"),
        ];

        let config = DownloadConfig::default();
        let downloader = EnhancedDownloader::new(config);
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

        let request = DownloadRequest::new(url, temp_dir.path())
            .with_filename("test-file.txt");

        let mut config = DownloadConfig::default();
        config.max_retries = 2; // Small number for faster test

        let downloader = EnhancedDownloader::new(config);

        let result = downloader.download(request, None).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DownloadError::MaxRetriesExceeded => {}
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
        let expected_crc32 = calculate_crc32(test_content);
        let expected_md5 = calculate_md5(test_content);
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

        let validation = FileValidation::new()
            .with_crc32(expected_crc32)
            .with_md5(expected_md5)
            .with_expected_size(expected_size);

        let request = DownloadRequest::new(url, temp_dir.path())
            .with_filename("validated-file.txt")
            .with_validation(validation);

        let config = DownloadConfig::default();
        let downloader = EnhancedDownloader::new(config);
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
