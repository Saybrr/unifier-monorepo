//! Extension trait for DownloadRequest with convenience methods

use crate::DownloadRequest;

/// Extension trait providing convenient methods for DownloadRequest operations
pub trait DownloadRequestExt: Sized {
    /// Check if this download is fully automated (no user interaction or external dependencies)
    fn is_automated(&self) -> bool;

    /// Get a display-friendly description including size information
    fn display_description(&self) -> String;

    /// Check if this download is likely to be fast (size-based heuristic)
    fn is_likely_fast(&self) -> bool;

    /// Check if this download is likely to be slow (size-based heuristic)
    fn is_likely_slow(&self) -> bool;

    /// Get expected size in MB for display purposes
    fn expected_size_mb(&self) -> Option<f64>;
}

impl DownloadRequestExt for DownloadRequest {
    fn is_automated(&self) -> bool {
        !self.requires_user_interaction() && !self.requires_external_dependencies()
    }

    fn display_description(&self) -> String {
        let base_description = self.get_description();
        match self.expected_size_mb() {
            Some(size_mb) if size_mb < 1.0 => format!("{} ({:.1} KB)", base_description, size_mb * 1024.0),
            Some(size_mb) if size_mb < 1024.0 => format!("{} ({:.1} MB)", base_description, size_mb),
            Some(size_mb) => format!("{} ({:.1} GB)", base_description, size_mb / 1024.0),
            None => base_description,
        }
    }

    fn is_likely_fast(&self) -> bool {
        // Files under 10MB are considered "fast"
        self.expected_size < 10 * 1024 * 1024
    }

    fn is_likely_slow(&self) -> bool {
        // Files over 100MB are considered "slow"
        self.expected_size > 100 * 1024 * 1024
    }

    fn expected_size_mb(&self) -> Option<f64> {
        Some(self.expected_size as f64 / 1_048_576.0)
    }
}

/// Extension trait providing convenient filtering methods for collections of DownloadRequest
pub trait DownloadRequestIteratorExt: Iterator<Item = DownloadRequest> + Sized {
    /// Filter to only automated downloads (no user interaction or external dependencies)
    fn filter_automated(self) -> impl Iterator<Item = DownloadRequest> {
        self.filter(|request| request.is_automated())
    }

    /// Filter to only downloads that require user interaction
    fn filter_manual(self) -> impl Iterator<Item = DownloadRequest> {
        self.filter(|request| request.requires_user_interaction())
    }

    /// Filter to only downloads that require external dependencies
    fn filter_external_deps(self) -> impl Iterator<Item = DownloadRequest> {
        self.filter(|request| request.requires_external_dependencies())
    }

    /// Filter to only fast downloads (size-based heuristic)
    fn filter_fast(self) -> impl Iterator<Item = DownloadRequest> {
        self.filter(|request| request.is_likely_fast())
    }

    /// Filter to only slow downloads (size-based heuristic)
    fn filter_slow(self) -> impl Iterator<Item = DownloadRequest> {
        self.filter(|request| request.is_likely_slow())
    }

    /// Sort by expected file size (smallest first)
    fn sort_by_size_asc(self) -> impl Iterator<Item = DownloadRequest> {
        let mut requests: Vec<_> = self.collect();
        requests.sort_by_key(|req| req.expected_size);
        requests.into_iter()
    }

    /// Sort by expected file size (largest first)
    fn sort_by_size_desc(self) -> impl Iterator<Item = DownloadRequest> {
        let mut requests: Vec<_> = self.collect();
        requests.sort_by_key(|req| std::cmp::Reverse(req.expected_size));
        requests.into_iter()
    }

    /// Calculate total size in bytes for all requests
    fn total_size(&mut self) -> u64 {
        self.map(|req| req.expected_size).sum()
    }

    /// Calculate total size in MB for all requests
    fn total_size_mb(&mut self) -> f64 {
        self.total_size() as f64 / 1_048_576.0
    }
}

impl<I> DownloadRequestIteratorExt for I where I: Iterator<Item = DownloadRequest> {}

/// Convenience functions for working with Vec<DownloadRequest>
pub trait DownloadRequestVecExt {
    /// Get summary statistics about the requests
    fn summary_stats(&self) -> RequestSummaryStats;

    /// Partition requests into automated vs manual/external
    fn partition_by_automation(self) -> (Vec<DownloadRequest>, Vec<DownloadRequest>);
}

impl DownloadRequestVecExt for Vec<DownloadRequest> {
    fn summary_stats(&self) -> RequestSummaryStats {
        let mut automated = 0;
        let mut manual = 0;
        let mut external_deps = 0;
        let mut total_size = 0u64;
        let mut fast_count = 0;
        let mut slow_count = 0;

        for request in self {
            if request.is_automated() {
                automated += 1;
            } else if request.requires_user_interaction() {
                manual += 1;
            } else if request.requires_external_dependencies() {
                external_deps += 1;
            }

            total_size += request.expected_size;

            if request.is_likely_fast() {
                fast_count += 1;
            } else if request.is_likely_slow() {
                slow_count += 1;
            }
        }

        RequestSummaryStats {
            total_requests: self.len(),
            automated_requests: automated,
            manual_requests: manual,
            external_dep_requests: external_deps,
            total_size_bytes: total_size,
            fast_requests: fast_count,
            slow_requests: slow_count,
        }
    }

    fn partition_by_automation(self) -> (Vec<DownloadRequest>, Vec<DownloadRequest>) {
        self.into_iter().partition(|request| request.is_automated())
    }
}

/// Summary statistics about a collection of download requests
#[derive(Debug, Clone)]
pub struct RequestSummaryStats {
    pub total_requests: usize,
    pub automated_requests: usize,
    pub manual_requests: usize,
    pub external_dep_requests: usize,
    pub total_size_bytes: u64,
    pub fast_requests: usize,
    pub slow_requests: usize,
}

impl RequestSummaryStats {
    /// Get total size in MB
    pub fn total_size_mb(&self) -> f64 {
        self.total_size_bytes as f64 / 1_048_576.0
    }

    /// Get total size in GB
    pub fn total_size_gb(&self) -> f64 {
        self.total_size_mb() / 1024.0
    }

    /// Get percentage of automated requests
    pub fn automation_percentage(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.automated_requests as f64 / self.total_requests as f64) * 100.0
        }
    }
}
