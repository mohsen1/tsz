//! TSC result structures
//!
//! Defines the structure of TSC cache entries and test results.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

/// File metadata for fast cache validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Last modified time in milliseconds
    pub mtime_ms: u64,
    /// File size in bytes
    pub size: u64,
    /// TypeScript version used to generate this cache entry.
    #[serde(default)]
    pub typescript_version: Option<String>,
}

/// TSC diagnostic result from cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TscResult {
    /// File metadata for cache validation
    pub metadata: FileMetadata,

    /// Error codes reported by TSC (sorted, unique)
    pub error_codes: Vec<u32>,

    /// Diagnostic fingerprints with location and normalized message details.
    ///
    /// This enables richer mismatch tracking than code-only comparisons.
    /// Defaults to empty for backward compatibility with older cache files.
    #[serde(default)]
    pub diagnostic_fingerprints: Vec<DiagnosticFingerprint>,
}

/// Stable diagnostic identity used for richer conformance comparisons.
///
/// `line` and `column` are 1-based when available, or 0 when unknown.
#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub struct DiagnosticFingerprint {
    pub code: u32,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message_key: String,
}

impl DiagnosticFingerprint {
    /// Build a fingerprint from raw diagnostic fields.
    pub fn new(code: u32, file: String, line: u32, column: u32, message: &str) -> Self {
        Self {
            code,
            file,
            line,
            column,
            message_key: Self::normalize_message_key(message),
        }
    }

    /// Best-effort message normalization to reduce noisy text differences.
    fn normalize_message_key(message: &str) -> String {
        let mut normalized = String::with_capacity(message.len());
        let mut prev_space = false;
        for ch in message.trim().chars() {
            if ch.is_whitespace() {
                if !prev_space {
                    normalized.push(' ');
                    prev_space = true;
                }
            } else {
                normalized.push(ch);
                prev_space = false;
            }
        }
        normalized
    }

    /// Human-readable compact key for summaries.
    pub fn display_key(&self) -> String {
        let file = if self.file.is_empty() {
            "<unknown>"
        } else {
            self.file.as_str()
        };
        format!(
            "TS{} {}:{}:{} {}",
            self.code, file, self.line, self.column, self.message_key
        )
    }
}

impl PartialEq for DiagnosticFingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.file == other.file
            && self.line == other.line
            && self.column == other.column
            && self.message_key == other.message_key
    }
}

impl Hash for DiagnosticFingerprint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.code.hash(state);
        self.file.hash(state);
        self.line.hash(state);
        self.column.hash(state);
        self.message_key.hash(state);
    }
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
        /// Missing diagnostic fingerprints (present in TSC but not tsz)
        missing_fingerprints: Vec<DiagnosticFingerprint>,
        /// Extra diagnostic fingerprints (present in tsz but not TSC)
        extra_fingerprints: Vec<DiagnosticFingerprint>,
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

/// Error frequency tracking for summaries
///
/// Uses DashMap for lock-free concurrent access from multiple workers.
#[derive(Debug, Default)]
pub struct ErrorFrequency {
    /// Map of error code -> (missing count, extra count)
    /// DashMap provides lock-free concurrent access
    pub frequencies: DashMap<u32, (usize, usize)>,
    /// Diagnostic fingerprint mismatch frequencies.
    pub fingerprint_frequencies: DashMap<DiagnosticFingerprint, (usize, usize)>,
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

    /// Record a missing fingerprint (thread-safe, no locking).
    pub fn record_missing_fingerprint(&self, fingerprint: DiagnosticFingerprint) {
        self.fingerprint_frequencies
            .entry(fingerprint)
            .and_modify(|(missing, _)| *missing += 1)
            .or_insert((1, 0));
    }

    /// Record an extra fingerprint (thread-safe, no locking).
    pub fn record_extra_fingerprint(&self, fingerprint: DiagnosticFingerprint) {
        self.fingerprint_frequencies
            .entry(fingerprint)
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

    /// Get top N fingerprint mismatches by total frequency.
    pub fn top_fingerprint_errors(&self, n: usize) -> Vec<(DiagnosticFingerprint, usize, usize)> {
        let mut errors: Vec<_> = self
            .fingerprint_frequencies
            .iter()
            .map(|entry| {
                let (fingerprint, &(missing, extra)) = entry.pair();
                (fingerprint.clone(), missing, extra)
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
