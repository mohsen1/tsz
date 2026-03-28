//! Process pool for batch-mode tsz compilation.
//!
//! Keeps N long-lived `tsz --batch` processes and multiplexes tests across them
//! via stdin/stdout with a sentinel-line protocol. Crash and timeout recovery
//! ensure robustness — dead workers are automatically respawned.

use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// Sentinel line printed by `tsz --batch` after each compilation.
const BATCH_SENTINEL: &str = "---TSZ-BATCH-DONE---";

/// A single long-lived `tsz --batch` worker process.
struct BatchWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// Outcome of a single batch compilation request.
pub enum BatchOutcome {
    /// Normal completion — collected output lines (may be empty for no-error case).
    Done(String),
    /// The worker process crashed (EOF on stdout before sentinel).
    Crashed,
    /// The compilation exceeded the timeout.
    Timeout,
}

/// Pool of `tsz --batch` worker processes.
pub struct ProcessPool {
    workers: Vec<Mutex<Option<BatchWorker>>>,
    tsz_binary: String,
    /// Channel of available worker indices.
    available_tx: tokio::sync::mpsc::Sender<usize>,
    available_rx: Mutex<tokio::sync::mpsc::Receiver<usize>>,
    /// Maximum compilations per worker before recycling (0 = no limit).
    max_compilations: usize,
    /// Maximum RSS in bytes per worker before recycling (0 = no limit).
    max_rss_bytes: usize,
    /// Per-worker compilation counters.
    compilation_counts: Vec<AtomicUsize>,
}

impl ProcessPool {
    /// Create a new pool with `n` workers using the given tsz binary path.
    ///
    /// `max_compilations` controls worker recycling: after a worker processes this
    /// many compilations, it is killed and a fresh process is spawned on next use.
    /// This returns all process memory to the OS, preventing unbounded RSS growth
    /// from arena/cache accumulation and malloc fragmentation in long-lived workers.
    /// Set to 0 to disable recycling.
    ///
    /// `max_rss_bytes` adds RSS-based recycling: after each compilation, the worker's
    /// resident memory is checked and it is recycled if it exceeds this threshold.
    /// Set to 0 to disable RSS-based recycling.
    pub async fn new(
        tsz_binary: &str,
        n: usize,
        max_compilations: usize,
        max_rss_bytes: usize,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(n);
        let mut workers = Vec::with_capacity(n);
        let mut compilation_counts = Vec::with_capacity(n);

        for i in 0..n {
            let worker = Self::spawn_worker_with_mem_limit(tsz_binary, max_rss_bytes)?;
            workers.push(Mutex::new(Some(worker)));
            compilation_counts.push(AtomicUsize::new(0));
            tx.send(i).await.expect("channel should not be closed");
        }

        Ok(Self {
            workers,
            tsz_binary: tsz_binary.to_string(),
            available_tx: tx,
            available_rx: Mutex::new(rx),
            max_compilations,
            max_rss_bytes,
            compilation_counts,
        })
    }

    /// Compile a project directory using a pooled worker.
    ///
    /// Acquires an idle worker, sends the project path, reads output until the
    /// sentinel line, and returns the worker to the pool.
    pub async fn compile(
        &self,
        project_dir: &Path,
        timeout: Duration,
    ) -> anyhow::Result<BatchOutcome> {
        // Acquire an available worker index
        let idx = {
            let mut rx = self.available_rx.lock().await;
            rx.recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("pool channel closed"))?
        };

        let result = self.compile_on_worker(idx, project_dir, timeout).await;

        // Return worker to the pool
        let _ = self.available_tx.send(idx).await;

        result
    }

    async fn compile_on_worker(
        &self,
        idx: usize,
        project_dir: &Path,
        timeout: Duration,
    ) -> anyhow::Result<BatchOutcome> {
        let mut guard = self.workers[idx].lock().await;

        // If worker is dead (crashed or recycled), respawn
        if guard.is_none() {
            *guard = Some(Self::spawn_worker_with_mem_limit(
                &self.tsz_binary,
                self.max_rss_bytes,
            )?);
            self.compilation_counts[idx].store(0, Ordering::Relaxed);
        }

        let worker = guard.as_mut().unwrap();

        // Write project directory path to stdin
        let dir_str = project_dir.to_string_lossy();
        let write_result = worker
            .stdin
            .write_all(format!("{dir_str}\n").as_bytes())
            .await;

        if write_result.is_err() {
            // Worker stdin broken — process likely dead
            *guard = None;
            return Ok(BatchOutcome::Crashed);
        }

        if worker.stdin.flush().await.is_err() {
            *guard = None;
            return Ok(BatchOutcome::Crashed);
        }

        // Read lines until sentinel (with timeout)
        let read_future = read_until_sentinel(&mut worker.stdout);

        let outcome = if timeout.is_zero() {
            match read_future.await {
                Ok(Some(output)) => BatchOutcome::Done(output),
                Ok(None) => {
                    // EOF — process died
                    *guard = None;
                    return Ok(BatchOutcome::Crashed);
                }
                Err(_) => {
                    *guard = None;
                    return Ok(BatchOutcome::Crashed);
                }
            }
        } else {
            match tokio::time::timeout(timeout, read_future).await {
                Ok(Ok(Some(output))) => BatchOutcome::Done(output),
                Ok(Ok(None)) => {
                    // EOF — process died
                    *guard = None;
                    return Ok(BatchOutcome::Crashed);
                }
                Ok(Err(_)) => {
                    *guard = None;
                    return Ok(BatchOutcome::Crashed);
                }
                Err(_) => {
                    // Timeout — kill the process
                    if let Some(mut w) = guard.take() {
                        let _ = w.child.kill().await;
                    }
                    return Ok(BatchOutcome::Timeout);
                }
            }
        };

        // Successful compilation — check if this worker should be recycled.
        // Recycling kills the process so the OS reclaims all memory, preventing
        // unbounded RSS growth from global caches and malloc fragmentation.
        let mut should_recycle = false;

        if self.max_compilations > 0 {
            let count = self.compilation_counts[idx].fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.max_compilations {
                should_recycle = true;
            }
        }

        // RSS-based recycling: check worker memory after each compilation.
        // Some tests (JSX, JSDoc, large multi-file) can cause a single worker
        // to allocate 2-3GB. With 2 workers on a 7GB runner, this OOM-kills
        // the entire process tree.
        if !should_recycle && self.max_rss_bytes > 0 {
            if let Some(ref w) = *guard {
                if let Some(pid) = w.child.id() {
                    if let Some(rss) = get_process_rss(pid) {
                        if rss > self.max_rss_bytes {
                            should_recycle = true;
                        }
                    }
                }
            }
        }

        if should_recycle {
            if let Some(mut w) = guard.take() {
                let _ = w.child.kill().await;
            }
            self.compilation_counts[idx].store(0, Ordering::Relaxed);
            // Worker is now None; it will be respawned on next use.
        }

        Ok(outcome)
    }

    fn spawn_worker_with_mem_limit(
        tsz_binary: &str,
        #[allow(unused_variables)] max_rss_bytes: usize,
    ) -> anyhow::Result<BatchWorker> {
        let mut cmd = Command::new(tsz_binary);
        cmd.arg("--batch")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr is intentionally discarded: batch mode routes all diagnostics
            // through stdout (via Reporter), and panics are detected via EOF on stdout
            // which triggers crash recovery with automatic worker respawn.
            .stderr(Stdio::null())
            .kill_on_drop(true);

        // Limit the worker's virtual address space so runaway recursion
        // (e.g., stack overflow in inferThis.ts) hits ENOMEM instead of
        // consuming all RAM and triggering the OOM killer on the entire
        // runner. The process crashes with SIGABRT, which the pool detects
        // as a crash and respawns cleanly.
        //
        // RLIMIT_AS limits virtual address space, which is typically much
        // larger than RSS due to memory-mapped files, thread stacks, and
        // other virtual regions. Use a 4x multiplier over the RSS limit
        // to avoid false positives while still catching runaway allocation.
        // RSS-based recycling (checked after each compilation) handles the
        // normal memory management; RLIMIT_AS is a safety net for crashes.
        #[cfg(target_os = "linux")]
        if max_rss_bytes > 0 {
            let limit = (max_rss_bytes as u64).saturating_mul(4);
            unsafe {
                cmd.pre_exec(move || {
                    let rlim = libc::rlimit {
                        rlim_cur: limit,
                        rlim_max: limit,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let mut child = cmd.spawn()?;

        // If the binary doesn't support batch mode, it can exit immediately.
        // Surface this as pool initialization failure so the runner falls back
        // to subprocess mode instead of reporting per-test crashes.
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("batch worker exited immediately with status: {status}");
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;

        Ok(BatchWorker {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }
}

/// Get the RSS (Resident Set Size) of a process in bytes.
/// Returns `None` if the RSS cannot be determined.
fn get_process_rss(pid: u32) -> Option<usize> {
    // On Linux, read /proc/{pid}/statm (page counts, space-separated).
    // Field 1 (index 1) is resident pages.
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        let resident_pages: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
        let page_size = 4096; // standard on x86_64 Linux
        return Some(resident_pages * page_size);
    }

    // On macOS, use `ps -o rss= -p {pid}` (returns RSS in KB).
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let rss_kb: usize = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .ok()?;
        return Some(rss_kb * 1024);
    }

    #[allow(unreachable_code)]
    None
}

/// Read lines from the worker's stdout until the sentinel line is found.
/// Returns `Some(output)` on success, `None` on EOF (worker died).
async fn read_until_sentinel(
    reader: &mut BufReader<ChildStdout>,
) -> std::io::Result<Option<String>> {
    let mut output = String::new();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // EOF — process exited
            return Ok(None);
        }

        let trimmed = line.trim_end();
        if trimmed == BATCH_SENTINEL {
            return Ok(Some(output));
        }

        output.push_str(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_process_rss_reports_current_process_memory_usage() {
        let rss = get_process_rss(std::process::id())
            .expect("current process RSS should be readable on supported platforms");
        assert!(rss > 0, "RSS should be positive, got {rss}");
    }

    #[tokio::test]
    async fn read_until_sentinel_collects_all_output_before_marker() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "printf '%s\\n%s\\n%s\\n' first second '{BATCH_SENTINEL}'"
            ))
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn sentinel process");
        let stdout = child.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);

        let output = read_until_sentinel(&mut reader)
            .await
            .expect("read until sentinel");

        assert_eq!(output.as_deref(), Some("first\nsecond\n"));
        let status = child.wait().await.expect("wait for child");
        assert!(status.success(), "child should exit successfully: {status}");
    }

    #[tokio::test]
    async fn read_until_sentinel_returns_none_when_process_ends_early() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("printf '%s\\n' partial")
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn eof process");
        let stdout = child.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);

        let output = read_until_sentinel(&mut reader)
            .await
            .expect("read before eof");

        assert_eq!(output, None);
        let status = child.wait().await.expect("wait for child");
        assert!(status.success(), "child should exit successfully: {status}");
    }
}
