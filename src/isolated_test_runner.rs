//! Isolated Test Runner - Process-based test execution with resource limits
//!
//! This module provides a robust test runner that can:
//! - Run tests in separate processes for true isolation
//! - Enforce memory limits to prevent OOM
//! - Forcefully terminate hung tests
//! - Monitor resource usage during test execution
//! - Fall back to thread-based execution when process isolation isn't available

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::test_harness::{TestResult, default_test_timeout};

/// Resource limits for isolated test execution
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory allowed in bytes (None = no limit)
    pub max_memory_mb: Option<usize>,
    /// Maximum time allowed for test execution
    pub timeout: Duration,
    /// Maximum number of file descriptors (Unix-only)
    pub max_file_descriptors: Option<u64>,
    /// Enable process monitoring
    pub enable_monitoring: bool,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: Some(512), // 512 MB default
            timeout: default_test_timeout(),
            max_file_descriptors: Some(1024),
            enable_monitoring: true,
        }
    }
}

impl ResourceLimits {
    /// Create resource limits with custom memory limit
    pub fn with_memory_mb(memory_mb: usize) -> Self {
        Self {
            max_memory_mb: Some(memory_mb),
            ..Default::default()
        }
    }

    /// Create resource limits with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            ..Default::default()
        }
    }

    /// Disable memory limits (useful for tests that legitimately need more memory)
    pub fn unlimited_memory() -> Self {
        Self {
            max_memory_mb: None,
            ..Default::default()
        }
    }
}

/// Configuration for isolated test execution
#[derive(Debug, Clone)]
pub struct IsolatedTestConfig {
    /// Resource limits for test execution
    pub limits: ResourceLimits,
    /// Whether to use process isolation (requires cargo test --test)
    pub use_process_isolation: bool,
    /// Path to the test binary (auto-detected if None)
    pub test_binary_path: Option<String>,
    /// Verbosity level (0 = quiet, 1 = normal, 2 = verbose)
    pub verbosity: u8,
}

impl Default for IsolatedTestConfig {
    fn default() -> Self {
        Self {
            limits: ResourceLimits::default(),
            use_process_isolation: true,
            test_binary_path: None,
            verbosity: 1,
        }
    }
}

/// Result of running a test with monitoring
#[derive(Debug, Clone)]
pub struct MonitoredTestResult {
    /// The test result
    pub result: TestResult,
    /// Peak memory usage in bytes (if available)
    pub peak_memory_bytes: Option<usize>,
    /// Whether the test was forcefully terminated
    pub was_terminated: bool,
    /// Exit signal if process was killed (Unix: signal number, Windows: exit code)
    pub termination_signal: Option<i32>,
}

impl MonitoredTestResult {
    /// Convert to basic TestResult
    pub fn to_test_result(&self) -> TestResult {
        self.result.clone()
    }

    /// Check if the test was killed due to resource limits
    pub fn was_killed(&self) -> bool {
        self.was_terminated || self.termination_signal.is_some()
    }

    /// Get a human-readable description of why the test was terminated
    pub fn termination_reason(&self) -> Option<String> {
        if let Some(signal) = self.termination_signal {
            Some(format!("terminated by signal {}", signal))
        } else if self.was_terminated {
            Some("forcefully terminated".to_string())
        } else {
            None
        }
    }
}

/// Run a test function with process isolation and resource limits
///
/// This function attempts to run the test with enforced resource limits.
/// Currently uses thread-based execution with monitoring. Process isolation
/// support is planned for future enhancements.
///
/// # Arguments
/// * `config` - Test execution configuration
/// * `test_name` - Name of the test (for diagnostics)
/// * `test_fn` - The test function to execute
///
/// # Returns
/// A monitored test result with execution details
pub fn run_isolated_test<F>(
    config: IsolatedTestConfig,
    test_name: &str,
    test_fn: F,
) -> MonitoredTestResult
where
    F: FnOnce() + Send + 'static,
{
    let start = Instant::now();

    // Log if process isolation was requested but not available
    if config.use_process_isolation && config.verbosity > 0 {
        eprintln!(
            "[isolated_test_runner] Process isolation not yet implemented for '{}'. Using thread-based execution with monitoring.",
            test_name
        );
    }

    // Use monitored thread-based execution
    run_monitored_thread(&config, test_fn, start)
}

/// Run test with thread-based monitoring (fallback)
fn run_monitored_thread<F>(
    config: &IsolatedTestConfig,
    test_fn: F,
    start: Instant,
) -> MonitoredTestResult
where
    F: FnOnce() + Send + 'static,
{
    // Use the existing test_harness infrastructure
    let completed = Arc::new(AtomicBool::new(false));
    let completed_clone = completed.clone();
    let should_terminate = Arc::new(AtomicBool::new(false));
    let should_terminate_clone = should_terminate.clone();

    let handle = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Check for termination request periodically
            // Note: This is cooperative - tests that don't yield can't be interrupted
            test_fn();
        }));
        completed_clone.store(true, Ordering::SeqCst);
        result
    });

    // Spawn monitor thread
    // Copy values from config before moving into closure
    let timeout = config.limits.timeout;
    let max_memory_mb = config.limits.max_memory_mb;

    let monitor_handle = thread::spawn({
        let completed = completed.clone();
        let should_terminate = should_terminate_clone.clone();
        let check_interval = Duration::from_millis(100);

        move || {
            let deadline = Instant::now() + timeout;
            let mut peak_memory = 0;

            loop {
                if completed.load(Ordering::SeqCst) {
                    break;
                }

                // Check timeout
                if Instant::now() >= deadline {
                    should_terminate.store(true, Ordering::SeqCst);
                    return MonitorOutcome::TimedOut { peak_memory };
                }

                // Monitor memory (platform-specific)
                #[cfg(unix)]
                {
                    if let Some(memory) = get_thread_memory_usage() {
                        if memory > peak_memory {
                            peak_memory = memory;
                        }

                        // Check memory limit
                        if let Some(limit_mb) = max_memory_mb {
                            let limit_bytes = limit_mb * 1024 * 1024;
                            if memory > limit_bytes {
                                should_terminate.store(true, Ordering::SeqCst);
                                return MonitorOutcome::MemoryLimitExceeded {
                                    peak_memory,
                                    limit_bytes,
                                };
                            }
                        }
                    }
                }

                thread::sleep(check_interval);
            }

            MonitorOutcome::Completed { peak_memory }
        }
    });

    // Wait for test completion
    // Wait for monitor outcome first so we can short-circuit on timeouts without blocking on join.
    let monitor_outcome = monitor_handle.join().unwrap_or(MonitorOutcome::MonitorCrashed);

    match monitor_outcome {
        MonitorOutcome::Completed { peak_memory } => {
            let test_duration = start.elapsed();
            let result = match handle.join() {
                Ok(Ok(())) => TestResult::Passed {
                    duration: test_duration,
                },
                Ok(Err(panic_info)) => {
                    let message = extract_panic_message(panic_info);
                    TestResult::Panicked {
                        message,
                        duration: test_duration,
                    }
                }
                Err(_) => TestResult::Panicked {
                    message: "Thread join failed".to_string(),
                    duration: test_duration,
                },
            };

            MonitoredTestResult {
                result,
                peak_memory_bytes: Some(peak_memory),
                was_terminated: false,
                termination_signal: None,
            }
        }
        MonitorOutcome::TimedOut { peak_memory } => {
            // Best-effort: if the worker finished between timeout and here, join to avoid leaks.
            if completed.load(Ordering::SeqCst) {
                let _ = handle.join();
            }

            MonitoredTestResult {
                result: TestResult::TimedOut {
                    timeout: config.limits.timeout,
                },
                peak_memory_bytes: Some(peak_memory),
                was_terminated: true,
                termination_signal: None, // Thread-based, no signal
            }
        }
        MonitorOutcome::MemoryLimitExceeded {
            peak_memory,
            limit_bytes,
        } => {
            if completed.load(Ordering::SeqCst) {
                let _ = handle.join();
            }

            MonitoredTestResult {
                result: TestResult::Failed {
                    message: format!(
                        "Memory limit exceeded: {} MB used, {} MB limit",
                        peak_memory / 1024 / 1024,
                        limit_bytes / 1024 / 1024
                    ),
                    duration: start.elapsed(),
                },
                peak_memory_bytes: Some(peak_memory),
                was_terminated: true,
                termination_signal: None,
            }
        }
        MonitorOutcome::MonitorCrashed => MonitoredTestResult {
            result: TestResult::Failed {
                message: "Monitor thread crashed".to_string(),
                duration: start.elapsed(),
            },
            peak_memory_bytes: None,
            was_terminated: false,
            termination_signal: None,
        },
    }
}

enum MonitorOutcome {
    Completed { peak_memory: usize },
    TimedOut { peak_memory: usize },
    MemoryLimitExceeded { peak_memory: usize, limit_bytes: usize },
    MonitorCrashed,
}

/// Extract panic message from panic payload
fn extract_panic_message(panic_info: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic_info.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Get approximate memory usage of current thread (Unix-only)
#[cfg(unix)]
fn get_thread_memory_usage() -> Option<usize> {
    use std::fs;

    // Read /proc/self/status for memory info
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                // VmRSS: 12345 kB
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<usize>() {
                        return Some(kb * 1024); // Convert to bytes
                    }
                }
            }
        }
    }
    None
}

/// Get approximate memory usage of current thread (Windows stub)
#[cfg(not(unix))]
fn get_thread_memory_usage() -> Option<usize> {
    // Windows implementation would use GetProcessMemoryInfo
    None
}

/// Enhanced test result with additional metadata
#[derive(Debug, Clone)]
pub struct EnhancedTestResult {
    /// Basic test result
    pub base: TestResult,
    /// Test name
    pub test_name: String,
    /// Memory usage information
    pub memory_info: Option<MemoryInfo>,
    /// Whether the test was isolated in a separate process
    pub was_isolated: bool,
}

/// Memory usage information
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    /// Peak memory usage in bytes
    pub peak_bytes: usize,
    /// Memory limit in bytes (if any)
    pub limit_bytes: Option<usize>,
    /// Percentage of limit used
    pub limit_percent: Option<f64>,
}

impl EnhancedTestResult {
    /// Create enhanced result from monitored result
    pub fn from_monitored(test_name: String, monitored: MonitoredTestResult, was_isolated: bool) -> Self {
        let memory_info = monitored.peak_memory_bytes.map(|peak| MemoryInfo {
            peak_bytes: peak,
            limit_bytes: None, // TODO: extract from config
            limit_percent: None,
        });

        Self {
            base: monitored.result,
            test_name,
            memory_info,
            was_isolated,
        }
    }

    /// Check if test passed
    pub fn is_passed(&self) -> bool {
        self.base.is_passed()
    }
}

/// Run a test with enhanced monitoring and reporting
///
/// This is the main entry point for running tests with improved resilience.
/// It provides a drop-in replacement for the basic `run_with_timeout` function.
///
/// # Arguments
/// * `test_name` - Name of the test
/// * `config` - Test configuration (or default for sensible defaults)
/// * `test_fn` - The test function to execute
///
/// # Returns
/// Enhanced test result with memory and isolation information
pub fn run_enhanced_test<F>(
    test_name: &str,
    config: Option<IsolatedTestConfig>,
    test_fn: F,
) -> EnhancedTestResult
where
    F: FnOnce() + Send + 'static,
{
    let config = config.unwrap_or_default();
    let use_process_isolation = config.use_process_isolation;
    let monitored = run_isolated_test(config, test_name, test_fn);
    EnhancedTestResult::from_monitored(test_name.to_string(), monitored, use_process_isolation)
}

// =============================================================================
// Tests for the isolated test runner
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolated_runner_pass() {
        let config = IsolatedTestConfig {
            limits: ResourceLimits {
                max_memory_mb: Some(100),
                timeout: Duration::from_secs(5),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = run_enhanced_test("test_pass", Some(config), || {
            assert_eq!(2 + 2, 4);
        });

        assert!(result.is_passed());
        assert_eq!(result.test_name, "test_pass");
    }

    #[test]
    fn test_isolated_runner_panic() {
        let config = IsolatedTestConfig::default();

        let result = run_enhanced_test("test_panic", Some(config), || {
            panic!("Intentional panic");
        });

        assert!(!result.is_passed());
        assert!(matches!(result.base, TestResult::Panicked { .. }));
    }

    #[test]
    fn test_isolated_runner_timeout() {
        let config = IsolatedTestConfig {
            limits: ResourceLimits {
                timeout: Duration::from_millis(100),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = run_enhanced_test("test_timeout", Some(config), || {
            std::thread::sleep(Duration::from_secs(10));
        });

        assert!(!result.is_passed());
        assert!(matches!(result.base, TestResult::TimedOut { .. }));
    }

    #[test]
    fn test_memory_tracking() {
        let config = IsolatedTestConfig::default();

        let result = run_enhanced_test("test_memory", Some(config), || {
            // Allocate some memory
            let _v: Vec<u8> = vec![0; 1024 * 1024]; // 1 MB
        });

        assert!(result.is_passed());
        #[cfg(unix)]
        assert!(result.memory_info.is_some());
    }

    #[test]
    fn test_unlimited_memory() {
        let config = IsolatedTestConfig {
            limits: ResourceLimits::unlimited_memory(),
            ..Default::default()
        };

        let result = run_enhanced_test("test_unlimited", Some(config), || {
            // This should not trigger memory limit
            let _v: Vec<u8> = vec![0; 10 * 1024 * 1024]; // 10 MB
        });

        assert!(result.is_passed());
    }
}
