//! Shared helper for reading a process's Resident Set Size (RSS).
//!
//! Used by the batch and server worker pools to decide whether a worker
//! should be recycled to keep memory bounded. The implementation is
//! platform-specific:
//!
//! - Linux: read `/proc/{pid}/statm` and multiply the resident-page count by
//!   the runtime page size.
//! - macOS: shell out to `ps -o rss= -p {pid}` (returns RSS in KiB).
//!
//! On other platforms the function returns `None`.

/// Get the RSS (Resident Set Size) of a process in bytes.
/// Returns `None` if the RSS cannot be determined.
#[cfg(target_os = "linux")]
pub(crate) fn get_process_rss(pid: u32) -> Option<usize> {
    // On Linux, read /proc/{pid}/statm (page counts, space-separated).
    // Field 1 (index 1) is resident pages.
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let resident_pages: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = usize::try_from(page_size)
        .ok()
        .filter(|page_size| *page_size > 0)?;
    resident_pages.checked_mul(page_size)
}

/// Get the RSS (Resident Set Size) of a process in bytes.
/// Returns `None` if the RSS cannot be determined.
#[cfg(target_os = "macos")]
pub(crate) fn get_process_rss(pid: u32) -> Option<usize> {
    // On macOS, use `ps -o rss= -p {pid}` (returns RSS in KB).
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let rss_kb: usize = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    Some(rss_kb * 1024)
}

/// Get the RSS (Resident Set Size) of a process in bytes.
/// Returns `None` if the RSS cannot be determined.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) fn get_process_rss(pid: u32) -> Option<usize> {
    let _ = pid;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn get_process_rss_reports_current_process_memory_usage() {
        let rss = get_process_rss(std::process::id())
            .expect("current process RSS should be readable on supported platforms");
        assert!(rss > 0, "RSS should be positive, got {rss}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn get_process_rss_returns_none_for_nonexistent_pid() {
        // /proc/{pid}/statm only exists for live processes. Use a PID large
        // enough that it is overwhelmingly unlikely to map to a running
        // process during test execution, but still inside `pid_t` range.
        assert_eq!(get_process_rss(u32::MAX - 1), None);
    }
}
