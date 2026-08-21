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
    #[serde(default)]
    pub mtime_ms: u64,
    /// File size in bytes
    #[serde(default)]
    pub size: u64,
    /// TypeScript version used to generate this cache entry.
    #[serde(default)]
    pub typescript_version: Option<String>,
    /// SHA-256 of the exact raw candidate bytes used by the oracle.
    #[serde(default)]
    pub source_sha256: String,
}

/// TSC diagnostic result from cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TscResult {
    /// File metadata for cache validation
    pub metadata: FileMetadata,

    /// Error codes reported by TSC in canonical order, with multiplicity.
    pub error_codes: Vec<u32>,

    /// Diagnostic fingerprints with location and byte-preserved message details.
    ///
    /// This enables richer mismatch tracking than code-only comparisons.
    /// Defaults to empty for backward compatibility with older cache files.
    #[serde(default)]
    pub diagnostic_fingerprints: Vec<DiagnosticFingerprint>,

    /// Whether every oracle diagnostic was parsed as a complete primary block,
    /// including its ordered continuation/related-information lines.
    ///
    /// Old caches default to `false`; every such entry, including an
    /// expected-clean row, is rejected before TSZ is invoked until the pinned
    /// oracle is regenerated.
    #[serde(default)]
    pub diagnostic_blocks_complete: bool,

    /// Exact ordinary process exit for each TS7-selected configuration, in
    /// selector order. Only 0, 1, and 2 are compiler outcomes; missing or
    /// other values are incomplete oracle evidence.
    #[serde(default)]
    pub ordinary_exit_statuses: Vec<u8>,
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
    /// Ordered, byte-preserving continuation lines owned by this primary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuations: Vec<String>,
}

impl DiagnosticFingerprint {
    /// Build a fingerprint from raw diagnostic fields.
    pub fn new(code: u32, file: String, line: u32, column: u32, message: &str) -> Self {
        Self {
            code,
            file,
            line,
            column,
            message_key: message.to_string(),
            continuations: Vec::new(),
        }
    }

    /// Human-readable compact key for summaries.
    pub fn display_key(&self) -> String {
        let file = if self.file.is_empty() {
            "<unknown>"
        } else {
            self.file.as_str()
        };
        let primary = format!(
            "TS{} {}:{}:{} {}",
            self.code, file, self.line, self.column, self.message_key
        );
        if self.continuations.is_empty() {
            primary
        } else {
            format!("{primary}\n{}", self.continuations.join("\n"))
        }
    }
}

impl PartialEq for DiagnosticFingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.file == other.file
            && self.line == other.line
            && self.column == other.column
            && self.message_key == other.message_key
            && self.continuations == other.continuations
    }
}

impl Hash for DiagnosticFingerprint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.code.hash(state);
        self.file.hash(state);
        self.line.hash(state);
        self.column.hash(state);
        self.message_key.hash(state);
        self.continuations.hash(state);
    }
}

/// Payload for a [`TestResult::Fail`] result.
///
/// Boxed inside the variant to keep the `TestResult` enum small; the `Fail`
/// variant carries eight `Vec`s and a `HashMap` that would otherwise dominate
/// the enum's stack size and force `clippy::large_enum_variant` suppressions
/// on every match arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResultFail {
    /// Expected error codes (from TSC)
    pub expected: Vec<u32>,
    /// Actual error codes (from tsz)
    pub actual: Vec<u32>,
    /// Missing error codes (present in TSC but not tsz)
    pub missing: Vec<u32>,
    /// Extra error codes (present in tsz but not TSC)
    pub extra: Vec<u32>,
    /// Missing diagnostic fingerprints (present in TSC but not tsz)
    pub missing_fingerprints: Vec<DiagnosticFingerprint>,
    /// Extra diagnostic fingerprints (present in tsz but not TSC)
    pub extra_fingerprints: Vec<DiagnosticFingerprint>,
    /// Full expected diagnostic fingerprints in canonical oracle order.
    pub expected_fingerprints: Vec<DiagnosticFingerprint>,
    /// Full raw TSZ diagnostic fingerprints in observed process order.
    pub actual_fingerprints: Vec<DiagnosticFingerprint>,
    /// Ordinary compiler exits, one per selected TS7 configuration.
    pub expected_exit_statuses: Vec<u8>,
    /// Fresh TSZ ordinary compiler exits, one per selected configuration.
    pub actual_exit_statuses: Vec<u8>,
    /// Resolved compiler options used
    pub options: std::collections::HashMap<String, String>,
    /// Known conformance debt reason. These are reported separately and are
    /// never counted as raw passes.
    pub known_failure: Option<&'static str>,
}

/// Stable reason codes for tests outside the active TypeScript oracle domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// No compiler-option configuration selected by the TypeScript 7 harness
    /// is supported by the pinned native compiler.
    TypeScript7Configuration,
    /// The authored invocation requests module-resolution trace output. The
    /// diagnostic conformance lane does not compare that second product yet,
    /// so the row must remain visible but cannot enter the runnable domain.
    TraceResolutionOutputNotCompared,
    /// TSZ could not complete a semantic operation required to decide the
    /// checked result. This is a capability nonclaim, not a compiler crash or
    /// a synthetic TypeScript diagnostic.
    SemanticIncomplete,
    /// The cached TypeScript result predates grouped diagnostic-block
    /// evidence, so exact message/continuation parity cannot be claimed.
    OracleDiagnosticEvidenceIncomplete,
}

impl UnsupportedReason {
    /// Machine-stable reason code emitted in per-test output and artifacts.
    pub const fn code(self) -> &'static str {
        match self {
            Self::TypeScript7Configuration => "typescript-7-unsupported-configuration",
            Self::TraceResolutionOutputNotCompared => "trace-resolution-output-not-compared",
            Self::SemanticIncomplete => "tsz-semantic-incomplete",
            Self::OracleDiagnosticEvidenceIncomplete => {
                "typescript-7-diagnostic-evidence-incomplete"
            }
        }
    }
}

/// Test comparison result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestResult {
    /// Test passed (results match)
    Pass,
    /// Test failed with specific mismatches; payload is boxed to keep the
    /// variant small.
    Fail(Box<TestResultFail>),
    /// Test was skipped (@noCheck, @skip, etc.)
    Skipped(&'static str),
    /// Test is outside the pinned oracle's supported configuration domain.
    Unsupported(UnsupportedReason),
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
    /// Number of paths selected for this invocation before execution starts.
    pub selected: AtomicUsize,
    pub total: AtomicUsize,
    pub passed: AtomicUsize,
    pub failed: AtomicUsize,
    pub skipped: AtomicUsize,
    pub unsupported: AtomicUsize,
    pub crashed: AtomicUsize,
    pub timeout: AtomicUsize,
    /// Failing tests that match an explicit known-debt entry.
    pub known_failures: AtomicUsize,
    /// Tests where error codes match exactly but fingerprints differ
    pub fingerprint_only: AtomicUsize,
}

impl TestStats {
    /// Every selected path must own exactly one terminal result.
    pub fn has_result_bijection(&self) -> bool {
        let total = self.total.load(Ordering::SeqCst);
        let partition = self.passed.load(Ordering::SeqCst)
            + self.failed.load(Ordering::SeqCst)
            + self.skipped.load(Ordering::SeqCst)
            + self.unsupported.load(Ordering::SeqCst)
            + self.crashed.load(Ordering::SeqCst)
            + self.timeout.load(Ordering::SeqCst);
        self.selected.load(Ordering::SeqCst) > 0
            && total == self.selected.load(Ordering::SeqCst)
            && partition == total
    }

    /// Failures, crashes, and timeouts are all terminal conformance failures.
    pub fn has_terminal_failure(&self) -> bool {
        self.failed.load(Ordering::SeqCst) > 0
            || self.crashed.load(Ordering::SeqCst) > 0
            || self.timeout.load(Ordering::SeqCst) > 0
    }

    /// Number of tests in the runnable oracle domain.
    pub fn runnable(&self) -> usize {
        let total = self.total.load(Ordering::SeqCst);
        let skipped = self.skipped.load(Ordering::SeqCst);
        let unsupported = self.unsupported.load(Ordering::SeqCst);
        total.saturating_sub(skipped.saturating_add(unsupported))
    }

    /// Backward-compatible name for the runnable oracle denominator.
    pub fn evaluated(&self) -> usize {
        self.runnable()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::cache_key;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    #[test]
    fn diagnostic_fingerprint_preserves_message_bytes_and_display_uses_unknown_file() {
        let fingerprint = DiagnosticFingerprint::new(
            2307,
            String::new(),
            12,
            4,
            "  Cannot   find\nmodule\t'foo'  ",
        );

        assert_eq!(fingerprint.message_key, "  Cannot   find\nmodule\t'foo'  ");
        assert_eq!(
            fingerprint.display_key(),
            "TS2307 <unknown>:12:4   Cannot   find\nmodule\t'foo'  "
        );
    }

    #[test]
    fn diagnostic_fingerprint_equality_preserves_message_whitespace() {
        let a = DiagnosticFingerprint::new(2322, "file.ts".to_string(), 1, 2, "one   two");
        let b = DiagnosticFingerprint::new(2322, "file.ts".to_string(), 1, 2, "one two");
        let c = DiagnosticFingerprint::new(2322, "file.ts".to_string(), 1, 2, "different");

        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn error_frequency_ranks_by_total_then_insertion_order_is_not_relied_on() {
        let freq = ErrorFrequency::default();

        freq.record_missing(2307);
        freq.record_extra(2307);
        freq.record_missing(2307);
        freq.record_extra(2322);
        freq.record_extra(2322);
        freq.record_missing(2353);

        let top = freq.top_errors(2);
        assert_eq!(top, vec![(2307, 2, 1), (2322, 0, 2)]);
    }

    #[test]
    fn fingerprint_frequency_tracks_missing_and_extra_counts() {
        let freq = ErrorFrequency::default();
        let fp = DiagnosticFingerprint::new(2345, "file.ts".to_string(), 3, 9, "mismatch");

        freq.record_missing_fingerprint(fp.clone());
        freq.record_extra_fingerprint(fp.clone());
        freq.record_extra_fingerprint(fp.clone());

        let top = freq.top_fingerprint_errors(1);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].0, fp);
        assert_eq!(top[0].1, 1);
        assert_eq!(top[0].2, 2);
    }

    #[test]
    fn test_stats_runnable_and_pass_rate_exclude_skips_and_unsupported() {
        let stats = TestStats::default();
        assert_eq!(stats.runnable(), 0);
        assert_eq!(stats.evaluated(), 0);
        assert_eq!(stats.pass_rate(), 0.0);

        stats.total.store(10, Ordering::SeqCst);
        stats.skipped.store(2, Ordering::SeqCst);
        stats.unsupported.store(1, Ordering::SeqCst);
        stats.passed.store(7, Ordering::SeqCst);

        assert_eq!(stats.runnable(), 7);
        assert_eq!(stats.evaluated(), 7);
        assert_eq!(stats.pass_rate(), 100.0);
    }

    #[test]
    fn result_bijection_and_terminal_failures_are_explicit() {
        let stats = TestStats::default();
        assert!(!stats.has_result_bijection());
        stats.selected.store(2, Ordering::SeqCst);
        stats.total.store(2, Ordering::SeqCst);
        stats.passed.store(1, Ordering::SeqCst);
        stats.crashed.store(1, Ordering::SeqCst);
        assert!(stats.has_result_bijection());
        assert!(stats.has_terminal_failure());

        stats.total.store(1, Ordering::SeqCst);
        assert!(!stats.has_result_bijection());
    }

    #[test]
    fn unsupported_reason_code_is_stable() {
        assert_eq!(
            UnsupportedReason::TypeScript7Configuration.code(),
            "typescript-7-unsupported-configuration"
        );
        assert_eq!(
            UnsupportedReason::TraceResolutionOutputNotCompared.code(),
            "trace-resolution-output-not-compared"
        );
        assert_eq!(
            UnsupportedReason::SemanticIncomplete.code(),
            "tsz-semantic-incomplete"
        );
        assert_eq!(
            UnsupportedReason::OracleDiagnosticEvidenceIncomplete.code(),
            "typescript-7-diagnostic-evidence-incomplete"
        );
    }

    #[test]
    fn tsc_result_deserializes_missing_fingerprints_as_empty() {
        let value = serde_json::json!({
            "metadata": { "mtime_ms": 1, "size": 2, "typescript_version": "5.4.0" },
            "error_codes": [2307, 2322]
        });

        let result: TscResult = serde_json::from_value(value).expect("valid TscResult JSON");
        assert!(result.diagnostic_fingerprints.is_empty());
        assert!(!result.diagnostic_blocks_complete);
        assert!(result.ordinary_exit_statuses.is_empty());
        assert_eq!(result.error_codes, vec![2307, 2322]);
        assert_eq!(result.metadata.typescript_version.as_deref(), Some("5.4.0"));
        assert!(result.metadata.source_sha256.is_empty());
    }

    #[test]
    fn cache_key_returns_empty_relative_path_for_root_match() {
        let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
        assert_eq!(cache_key(&test_dir, &test_dir), Some(String::new()));
    }
}
