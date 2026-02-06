//! TSC result structures
//!
//! Defines the structure of TSC cache entries and test results.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

/// File metadata for fast cache validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Last modified time in milliseconds
    pub mtime_ms: u64,
    /// File size in bytes
    pub size: u64,
}

/// TSC diagnostic result from cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TscResult {
    /// File metadata for cache validation
    pub metadata: FileMetadata,

    /// Error codes reported by TSC (sorted, unique)
    pub error_codes: Vec<u32>,
}

/// Test comparison result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestResult {
    /// Test passed (results match)
    Pass,
    /// Test failed with specific mismatches
    Fail {
        /// Expected error codes (from TSC)
        expected: Vec<u32>,
        /// Actual error codes (from tsz)
        actual: Vec<u32>,
        /// Missing error codes (present in TSC but not tsz)
        missing: Vec<u32>,
        /// Extra error codes (present in tsz but not TSC)
        extra: Vec<u32>,
        /// Resolved compiler options used
        options: std::collections::HashMap<String, String>,
    },
    /// Test was skipped (@noCheck, @skip, etc.)
    Skipped(&'static str),
    /// Compiler crashed
    Crashed,
    /// Test timed out
    Timeout,
}

impl TestResult {
    /// Check if test passed
    pub fn is_pass(&self) -> bool {
        matches!(self, TestResult::Pass)
    }

    /// Check if test was skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self, TestResult::Skipped(_))
    }

    /// Check if test crashed
    pub fn is_crashed(&self) -> bool {
        matches!(self, TestResult::Crashed)
    }

    /// Check if test timed out
    pub fn is_timeout(&self) -> bool {
        matches!(self, TestResult::Timeout)
    }
}

/// Error frequency tracking for summaries
///
/// Uses DashMap for lock-free concurrent access from multiple workers.
#[derive(Debug, Default)]
pub struct ErrorFrequency {
    /// Map of error code -> (missing count, extra count)
    /// DashMap provides lock-free concurrent access
    pub frequencies: DashMap<u32, (usize, usize)>,
}

impl ErrorFrequency {
    /// Record a missing error (thread-safe, no locking)
    pub fn record_missing(&self, code: u32) {
        self.frequencies
            .entry(code)
            .and_modify(|(missing, _)| *missing += 1)
            .or_insert((1, 0));
    }

    /// Record an extra error (thread-safe, no locking)
    pub fn record_extra(&self, code: u32) {
        self.frequencies
            .entry(code)
            .and_modify(|(_, extra)| *extra += 1)
            .or_insert((0, 1));
    }

    /// Get top N error codes by total frequency
    pub fn top_errors(&self, n: usize) -> Vec<(u32, usize, usize)> {
        let mut errors: Vec<_> = self
            .frequencies
            .iter()
            .map(|entry| {
                let (&code, &(missing, extra)) = entry.pair();
                (code, missing, extra)
            })
            .collect();

        errors.sort_by_key(|(_, missing, extra)| *extra + *missing);
        errors.reverse();
        errors.into_iter().take(n).collect()
    }
}

/// Statistics for test run
#[derive(Debug, Default)]
pub struct TestStats {
    pub total: AtomicUsize,
    pub passed: AtomicUsize,
    pub failed: AtomicUsize,
    pub skipped: AtomicUsize,
    pub crashed: AtomicUsize,
    pub timeout: AtomicUsize,
}

impl TestStats {
    /// Number of tests actually evaluated (total minus skipped)
    pub fn evaluated(&self) -> usize {
        let total = self.total.load(Ordering::SeqCst);
        let skipped = self.skipped.load(Ordering::SeqCst);
        total.saturating_sub(skipped)
    }

    pub fn pass_rate(&self) -> f64 {
        let evaluated = self.evaluated();
        let passed = self.passed.load(Ordering::SeqCst);
        if evaluated == 0 {
            0.0
        } else {
            (passed as f64 / evaluated as f64) * 100.0
        }
    }
}
