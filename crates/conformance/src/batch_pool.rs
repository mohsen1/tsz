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
    pub async fn new(tsz_binary: &str, n: usize, max_compilations: usize) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(n);
        let mut workers = Vec::with_capacity(n);
        let mut compilation_counts = Vec::with_capacity(n);

        for i in 0..n {
            let worker = Self::spawn_worker(tsz_binary)?;
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
            *guard = Some(Self::spawn_worker(&self.tsz_binary)?);
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
        if self.max_compilations > 0 {
            let count = self.compilation_counts[idx].fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.max_compilations {
                if let Some(mut w) = guard.take() {
                    let _ = w.child.kill().await;
                }
                // Worker is now None; it will be respawned on next use.
            }
        }

        Ok(outcome)
    }

    fn spawn_worker(tsz_binary: &str) -> anyhow::Result<BatchWorker> {
        let mut child = Command::new(tsz_binary)
            .arg("--batch")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr is intentionally discarded: batch mode routes all diagnostics
            // through stdout (via Reporter), and panics are detected via EOF on stdout
            // which triggers crash recovery with automatic worker respawn.
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

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
