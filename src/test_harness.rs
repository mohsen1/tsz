//! Test Harness Module - Infrastructure for unit and conformance tests
//!
//! This module provides utilities for:
//! - Running tests with timeouts to prevent hanging
//! - Shared test fixtures and setup helpers
//! - Conformance test runner integration
//! - Test result collection and reporting
//! - Isolated test execution with resource limits (via isolated_test_runner)
//!
//! # Basic Usage
//!
//! ```ignore
//! use test_harness::{run_with_timeout, default_test_timeout};
//!
//! let result = run_with_timeout(default_test_timeout(), || {
//!     assert_eq!(2 + 2, 4);
//! });
//! assert!(result.is_passed());
//! ```
//!
//! # Enhanced Usage with Resource Limits
//!
//! For tests that need stricter resource control:
//!
//! ```ignore
//! use test_harness::isolated::{run_enhanced_test, ResourceLimits};
//!
//! let result = run_enhanced_test(
//!     "my_test",
//!     Some(IsolatedTestConfig {
//!         limits: ResourceLimits::with_memory_mb(512),
//!         ..Default::default()
//!     }),
//!     || {
//!         // test code here
//!     }
//! );
//! ```

use std::panic::{self, AssertUnwindSafe};
use std::sync::OnceLock;

// Re-export the isolated_test_runner for enhanced test execution
pub use crate::isolated_test_runner::{
    EnhancedTestResult, IsolatedTestConfig, MemoryInfo, MonitoredTestResult, ResourceLimits,
    run_enhanced_test, run_isolated_test,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Default timeout (seconds) applied when no override is provided.
/// Use the environment variable `TSZ_TEST_TIMEOUT_SECS` to override at runtime.
pub const DEFAULT_TEST_TIMEOUT_SECS: u64 = 300;

/// Resolve the default test timeout, honoring `TSZ_TEST_TIMEOUT_SECS`.
fn resolve_default_timeout() -> Duration {
    static CACHE: OnceLock<Duration> = OnceLock::new();
    *CACHE.get_or_init(|| {
        let from_env = std::env::var("TSZ_TEST_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());
        Duration::from_secs(from_env.unwrap_or(DEFAULT_TEST_TIMEOUT_SECS))
    })
}

/// Default timeout for individual tests (resolved at runtime).
pub fn default_test_timeout() -> Duration {
    resolve_default_timeout()
}

/// Backwards-compatible constant using the default seconds (env override not applied).
pub const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(DEFAULT_TEST_TIMEOUT_SECS);

/// Timeout for parser tests (may take longer for complex inputs)
pub fn parser_test_timeout() -> Duration {
    Duration::from_secs(120)
}

/// Timeout for type checker tests (more complex analysis)
pub fn checker_test_timeout() -> Duration {
    Duration::from_secs(180)
}

/// Result of running a test with timeout
#[derive(Debug, Clone)]
pub enum TestResult {
    /// Test passed successfully
    Passed { duration: Duration },
    /// Test failed with an error message
    Failed { message: String, duration: Duration },
    /// Test timed out
    TimedOut { timeout: Duration },
    /// Test panicked with a message
    Panicked { message: String, duration: Duration },
}

impl TestResult {
    /// Check if the test passed
    pub fn is_passed(&self) -> bool {
        matches!(self, TestResult::Passed { .. })
    }

    /// Check if the test failed in any way
    pub fn is_failed(&self) -> bool {
        !self.is_passed()
    }

    /// Get the duration, if available
    pub fn duration(&self) -> Option<Duration> {
        match self {
            TestResult::Passed { duration }
            | TestResult::Failed { duration, .. }
            | TestResult::Panicked { duration, .. } => Some(*duration),
            TestResult::TimedOut { .. } => None,
        }
    }
}

/// Run a test function with a specified timeout.
/// Returns the result of the test, which may be a pass, fail, timeout, or panic.
///
/// # Example
/// ```ignore
/// use test_harness::{run_with_timeout, default_test_timeout};
///
/// let result = run_with_timeout(default_test_timeout(), || {
///     // Test code here
///     assert_eq!(2 + 2, 4);
/// });
/// assert!(result.is_passed());
/// ```
pub fn run_with_timeout<F>(timeout: Duration, test_fn: F) -> TestResult
where
    F: FnOnce() + Send + 'static,
{
    let start = Instant::now();
    let completed = Arc::new(AtomicBool::new(false));
    let completed_clone = completed.clone();

    let handle = thread::spawn(move || {
        let result = panic::catch_unwind(AssertUnwindSafe(test_fn));
        completed_clone.store(true, Ordering::SeqCst);
        result
    });

    // Wait for the thread with timeout
    let check_interval = Duration::from_millis(10);
    let deadline = start + timeout;

    loop {
        if completed.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() >= deadline {
            // Thread is still running after timeout
            // Note: We can't easily kill the thread in Rust, so we just report the timeout
            return TestResult::TimedOut { timeout };
        }
        thread::sleep(check_interval);
    }

    let duration = start.elapsed();

    match handle.join() {
        Ok(Ok(())) => TestResult::Passed { duration },
        Ok(Err(panic_info)) => {
            let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            TestResult::Panicked { message, duration }
        }
        Err(_) => TestResult::Panicked {
            message: "Thread panicked".to_string(),
            duration,
        },
    }
}

/// Run a test that returns a Result with timeout
pub fn run_result_with_timeout<F, E>(timeout: Duration, test_fn: F) -> TestResult
where
    F: FnOnce() -> Result<(), E> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let start = Instant::now();
    let completed = Arc::new(AtomicBool::new(false));
    let completed_clone = completed.clone();

    let handle = thread::spawn(move || {
        let result = panic::catch_unwind(AssertUnwindSafe(|| test_fn()));
        completed_clone.store(true, Ordering::SeqCst);
        result
    });

    // Wait for the thread with timeout
    let check_interval = Duration::from_millis(10);
    let deadline = start + timeout;

    loop {
        if completed.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() >= deadline {
            return TestResult::TimedOut { timeout };
        }
        thread::sleep(check_interval);
    }

    let duration = start.elapsed();

    match handle.join() {
        Ok(Ok(Ok(()))) => TestResult::Passed { duration },
        Ok(Ok(Err(e))) => TestResult::Failed {
            message: e.to_string(),
            duration,
        },
        Ok(Err(panic_info)) => {
            let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            TestResult::Panicked { message, duration }
        }
        Err(_) => TestResult::Panicked {
            message: "Thread panicked".to_string(),
            duration,
        },
    }
}

/// Test fixture for parser tests
pub struct ParserTestFixture {
    source: String,
    file_name: String,
}

impl ParserTestFixture {
    /// Create a new parser test fixture
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            file_name: "test.ts".to_string(),
        }
    }

    /// Create fixture with a custom file name
    pub fn with_file_name(source: impl Into<String>, file_name: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            file_name: file_name.into(),
        }
    }

    /// Get the source code
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the file name
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Parse the source and return the parser state
    pub fn parse(&self) -> crate::parser::ParserState {
        let mut parser = crate::parser::ParserState::new(
            self.file_name.clone(),
            self.source.clone(),
        );
        parser.parse_source_file();
        parser
    }

    /// Parse and bind the source, returning both parser and binder states
    pub fn parse_and_bind(&self) -> (crate::parser::ParserState, crate::binder::BinderState) {
        let mut parser = crate::parser::ParserState::new(
            self.file_name.clone(),
            self.source.clone(),
        );
        let root = parser.parse_source_file();

        let mut binder = crate::binder::BinderState::new();
        binder.bind_source_file(&parser.arena, root);

        (parser, binder)
    }
}

/// Collected test results for reporting
#[derive(Debug, Default)]
pub struct TestReport {
    pub passed: usize,
    pub failed: usize,
    pub timed_out: usize,
    pub panicked: usize,
    pub total_duration: Duration,
    pub failures: Vec<(String, TestResult)>,
}

impl TestReport {
    /// Add a test result to the report
    pub fn add(&mut self, name: impl Into<String>, result: TestResult) {
        match &result {
            TestResult::Passed { duration } => {
                self.passed += 1;
                self.total_duration += *duration;
            }
            TestResult::Failed { duration, .. } => {
                self.failed += 1;
                self.total_duration += *duration;
                self.failures.push((name.into(), result));
            }
            TestResult::TimedOut { timeout } => {
                self.timed_out += 1;
                self.total_duration += *timeout;
                self.failures.push((name.into(), result));
            }
            TestResult::Panicked { duration, .. } => {
                self.panicked += 1;
                self.total_duration += *duration;
                self.failures.push((name.into(), result));
            }
        }
    }

    /// Get the total number of tests
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.timed_out + self.panicked
    }

    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// Print a summary of the results
    pub fn print_summary(&self) {
        println!("\n=== Test Report ===");
        println!("Total:     {}", self.total());
        println!("Passed:    {}", self.passed);
        println!("Failed:    {}", self.failed);
        println!("Timed out: {}", self.timed_out);
        println!("Panicked:  {}", self.panicked);
        println!("Duration:  {:?}", self.total_duration);

        if !self.failures.is_empty() {
            println!("\nFailures:");
            for (name, result) in &self.failures {
                match result {
                    TestResult::Failed { message, .. } => {
                        println!("  FAIL {}: {}", name, message);
                    }
                    TestResult::TimedOut { timeout } => {
                        println!("  TIMEOUT {}: exceeded {:?}", name, timeout);
                    }
                    TestResult::Panicked { message, .. } => {
                        println!("  PANIC {}: {}", name, message);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Macro for running a test with a default timeout
#[macro_export]
macro_rules! test_with_timeout {
    ($name:ident, $timeout:expr, $body:block) => {
        #[test]
        fn $name() {
            let result = $crate::test_harness::run_with_timeout($timeout, || $body);
            match result {
                $crate::test_harness::TestResult::Passed { .. } => {}
                $crate::test_harness::TestResult::Failed { message, .. } => {
                    panic!("Test failed: {}", message);
                }
                $crate::test_harness::TestResult::TimedOut { timeout } => {
                    panic!("Test timed out after {:?}", timeout);
                }
                $crate::test_harness::TestResult::Panicked { message, .. } => {
                    panic!("Test panicked: {}", message);
                }
            }
        }
    };
    ($name:ident, $body:block) => {
        test_with_timeout!($name, $crate::test_harness::default_test_timeout(), $body);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_with_timeout_passes() {
        let result = run_with_timeout(Duration::from_secs(1), || {
            // Simple test that should pass
            assert_eq!(2 + 2, 4);
        });
        assert!(result.is_passed());
    }

    #[test]
    fn test_run_with_timeout_fails() {
        let result = run_with_timeout(Duration::from_secs(1), || {
            panic!("Intentional panic");
        });
        assert!(matches!(result, TestResult::Panicked { .. }));
    }

    #[test]
    fn test_run_with_timeout_timeout() {
        let result = run_with_timeout(Duration::from_millis(50), || {
            // This should timeout
            std::thread::sleep(Duration::from_secs(10));
        });
        assert!(matches!(result, TestResult::TimedOut { .. }));
    }

    #[test]
    fn test_parser_fixture() {
        let fixture = ParserTestFixture::new("const x = 1;");
        let parser = fixture.parse();
        // Just verify parsing doesn't panic
        assert!(!parser.get_arena().is_empty());
    }

    #[test]
    fn test_report() {
        let mut report = TestReport::default();
        report.add("test1", TestResult::Passed { duration: Duration::from_millis(100) });
        report.add("test2", TestResult::Failed {
            message: "assertion failed".into(),
            duration: Duration::from_millis(50)
        });

        assert_eq!(report.total(), 2);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        assert!(!report.all_passed());
    }
}

// =============================================================================
// Documentation Examples
// =============================================================================
//
// # Examples: Using the Enhanced Test Runner with Resource Limits
//
// This section demonstrates how to use the isolated test runner for
// tests that need better hang protection and memory limit enforcement.
//
// ## Basic Usage
//
// ```ignore
// use test_harness::{run_enhanced_test, IsolatedTestConfig};
//
// #[test]
// fn my_test() {
//     let result = run_enhanced_test(
//         "my_test",
//         None, // Use default config
//         || {
//             // Test code here
//             assert_eq!(2 + 2, 4);
//         }
//     );
//
//     assert!(result.is_passed());
// }
// ```
//
// ## Custom Memory Limits
//
// ```ignore
// use test_harness::{run_enhanced_test, IsolatedTestConfig, ResourceLimits};
//
// #[test]
// fn memory_intensive_test() {
//     let config = IsolatedTestConfig {
//         limits: ResourceLimits::with_memory_mb(1024), // 1 GB limit
//         ..Default::default()
//     };
//
//     let result = run_enhanced_test("memory_test", Some(config), || {
//         // Allocate up to 1GB of memory
//         let data = vec![0u8; 512 * 1024 * 1024]; // 512 MB
//         assert_eq!(data.len(), 512 * 1024 * 1024);
//     });
//
//     assert!(result.is_passed());
//
//     // Check memory usage
//     if let Some(mem_info) = result.memory_info {
//         println!("Peak memory: {} MB", mem_info.peak_bytes / 1024 / 1024);
//     }
// }
// ```
//
// ## Timeout Protection
//
// ```ignore
// use test_harness::{run_enhanced_test, IsolatedTestConfig, ResourceLimits};
// use std::time::Duration;
//
// #[test]
// fn slow_operation_test() {
//     let config = IsolatedTestConfig {
//         limits: ResourceLimits::with_timeout(Duration::from_secs(5)),
//         verbosity: 2, // Verbose output
//         ..Default::default()
//     };
//
//     let result = run_enhanced_test("slow_test", Some(config), || {
//         // This will timeout after 5 seconds
//         std::thread::sleep(Duration::from_secs(100));
//     });
//
//     // Test should be marked as timed out
//     assert!(!result.is_passed());
//     assert!(matches!(result.base, test_harness::TestResult::TimedOut { .. }));
// }
// ```
//
// ## Unlimited Memory for Known-High-Memory Tests
//
// ```ignore
// use test_harness::{run_enhanced_test, IsolatedTestConfig, ResourceLimits};
//
// #[test]
// fn large_dataset_test() {
//     let config = IsolatedTestConfig {
//         limits: ResourceLimits::unlimited_memory(),
//         ..Default::default()
//     };
//
//     let result = run_enhanced_test("large_data_test", Some(config), || {
//         // This test legitimately needs lots of memory
//         let large_vec = vec![0u64; 100_000_000];
//         assert_eq!(large_vec.len(), 100_000_000);
//     });
//
//     assert!(result.is_passed());
// }
// ```
//
// # Comparison: run_with_timeout vs run_enhanced_test
//
// | Feature | run_with_timeout | run_enhanced_test |
// |---------|------------------|-------------------|
// | Timeout detection | ✓ | ✓ |
// | Memory monitoring | ✗ | ✓ |
// | Memory limit enforcement | ✗ | ✓ |
// | Resource usage reporting | ✗ | ✓ |
// | Process isolation (future) | ✗ | Planned |
//
// # When to Use Each
//
// - **Use `run_with_timeout`** for simple, fast-running tests where you just need
//   basic timeout protection.
//
// - **Use `run_enhanced_test`** for:
//   - Tests that process large amounts of data
//   - Tests with complex control flow that might hang
//   - Integration tests that need memory monitoring
//   - Performance tests where you want to track resource usage
//
// # Integration with Existing Tests
//
// You can gradually migrate existing tests to use the enhanced runner:
//
// ```ignore
// // Before:
// #[test]
// fn test_parser() {
//     let result = run_with_timeout(PARSER_TEST_TIMEOUT, || {
//         parse_large_file();
//     });
//     assert!(result.is_passed());
// }
//
// // After:
// #[test]
// fn test_parser() {
//     let config = IsolatedTestConfig {
//         limits: ResourceLimits::with_memory_mb(256),
//         ..Default::default()
//     };
//
//     let result = run_enhanced_test(
//         "test_parser",
//         Some(config),
//         parse_large_file
//     );
//     assert!(result.is_passed());
// }
// ```
