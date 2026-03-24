//! Process pool for server-mode tsz compilation.
//!
//! Keeps N long-lived `tsz-server --protocol legacy` processes and multiplexes
//! tests across them via JSON on stdin/stdout. Crash and timeout recovery
//! ensure robustness — dead workers are automatically respawned.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::options_convert::directives_to_check_options;

/// A single long-lived `tsz-server --protocol legacy` worker process.
struct ServerWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// Outcome of a single server check request.
pub enum ServerOutcome {
    /// Normal completion — sorted, deduplicated error codes.
    Done(Vec<u32>),
    /// The worker process crashed (EOF on stdout).
    Crashed,
    /// The check exceeded the timeout.
    Timeout,
    /// The server returned an error message.
    Error(String),
}

/// Response from the server's legacy protocol.
#[derive(Deserialize)]
struct ServerResponse {
    #[allow(dead_code)]
    id: u64,
    codes: Option<Vec<i32>>,
    error: Option<String>,
}

/// Pool of `tsz-server --protocol legacy` worker processes.
pub struct ServerPool {
    workers: Vec<Mutex<Option<ServerWorker>>>,
    server_binary: String,
    /// Channel of available worker indices.
    available_tx: tokio::sync::mpsc::Sender<usize>,
    available_rx: Mutex<tokio::sync::mpsc::Receiver<usize>>,
    /// Maximum checks per worker before recycling (0 = no limit).
    max_checks: usize,
    /// Maximum RSS in bytes per worker before recycling (0 = no limit).
    max_rss_bytes: usize,
    /// Per-worker check counters.
    check_counts: Vec<AtomicUsize>,
    /// Global request ID counter.
    next_request_id: AtomicU64,
}

impl ServerPool {
    /// Create a new pool with `n` workers using the given server binary path.
    ///
    /// `max_checks` controls worker recycling: after a worker processes this
    /// many checks, it is killed and a fresh process is spawned on next use.
    /// Set to 0 to disable recycling.
    ///
    /// `max_rss_bytes` adds RSS-based recycling: after each check, the worker's
    /// resident memory is checked and it is recycled if it exceeds this threshold.
    /// Set to 0 to disable RSS-based recycling.
    pub async fn new(
        server_binary: &str,
        n: usize,
        max_checks: usize,
        max_rss_bytes: usize,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(n);
        let mut workers = Vec::with_capacity(n);
        let mut check_counts = Vec::with_capacity(n);

        for i in 0..n {
            let worker = Self::spawn_worker(server_binary).await?;
            workers.push(Mutex::new(Some(worker)));
            check_counts.push(AtomicUsize::new(0));
            tx.send(i).await.expect("channel should not be closed");
        }

        Ok(Self {
            workers,
            server_binary: server_binary.to_string(),
            available_tx: tx,
            available_rx: Mutex::new(rx),
            max_checks,
            max_rss_bytes,
            check_counts,
            next_request_id: AtomicU64::new(1),
        })
    }

    /// Check files using a pooled worker.
    ///
    /// Acquires an idle worker, sends the check request as JSON, reads the
    /// JSON response, and returns the worker to the pool.
    pub async fn check(
        &self,
        files: HashMap<String, String>,
        directives: &HashMap<String, String>,
        timeout: Duration,
    ) -> anyhow::Result<ServerOutcome> {
        // Acquire an available worker index
        let idx = {
            let mut rx = self.available_rx.lock().await;
            rx.recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("pool channel closed"))?
        };

        let result = self.check_on_worker(idx, files, directives, timeout).await;

        // Return worker to the pool
        let _ = self.available_tx.send(idx).await;

        result
    }

    async fn check_on_worker(
        &self,
        idx: usize,
        files: HashMap<String, String>,
        directives: &HashMap<String, String>,
        timeout: Duration,
    ) -> anyhow::Result<ServerOutcome> {
        let mut guard = self.workers[idx].lock().await;

        // If worker is dead (crashed or recycled), respawn
        if guard.is_none() {
            *guard = Some(Self::spawn_worker(&self.server_binary).await?);
            self.check_counts[idx].store(0, Ordering::Relaxed);
        }

        let worker = guard.as_mut().unwrap();

        // Build the JSON request
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let options = directives_to_check_options(directives);
        let request = json!({
            "type": "check",
            "id": request_id,
            "files": files,
            "options": options,
        });

        // Write request as a single JSON line
        let mut request_bytes = serde_json::to_vec(&request)?;
        request_bytes.push(b'\n');

        let write_result = worker.stdin.write_all(&request_bytes).await;
        if write_result.is_err() {
            *guard = None;
            return Ok(ServerOutcome::Crashed);
        }

        if worker.stdin.flush().await.is_err() {
            *guard = None;
            return Ok(ServerOutcome::Crashed);
        }

        // Read one JSON response line (with timeout)
        let read_future = read_response_line(&mut worker.stdout);

        let outcome = if timeout.is_zero() {
            match read_future.await {
                Ok(Some(resp)) => parse_response(resp),
                Ok(None) => {
                    *guard = None;
                    return Ok(ServerOutcome::Crashed);
                }
                Err(_) => {
                    *guard = None;
                    return Ok(ServerOutcome::Crashed);
                }
            }
        } else {
            match tokio::time::timeout(timeout, read_future).await {
                Ok(Ok(Some(resp))) => parse_response(resp),
                Ok(Ok(None)) => {
                    *guard = None;
                    return Ok(ServerOutcome::Crashed);
                }
                Ok(Err(_)) => {
                    *guard = None;
                    return Ok(ServerOutcome::Crashed);
                }
                Err(_) => {
                    // Timeout — kill the process
                    if let Some(mut w) = guard.take() {
                        let _ = w.child.kill().await;
                    }
                    return Ok(ServerOutcome::Timeout);
                }
            }
        };

        // Successful check — check if this worker should be recycled.
        let mut should_recycle = false;

        if self.max_checks > 0 {
            let count = self.check_counts[idx].fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.max_checks {
                should_recycle = true;
            }
        }

        // RSS-based recycling
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
            self.check_counts[idx].store(0, Ordering::Relaxed);
        }

        Ok(outcome)
    }

    async fn spawn_worker(server_binary: &str) -> anyhow::Result<ServerWorker> {
        let mut cmd = Command::new(server_binary);
        cmd.arg("--protocol")
            .arg("legacy")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        // If the binary exits immediately, surface this as a pool initialization failure.
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("server worker exited immediately with status: {status}");
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;

        Ok(ServerWorker {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }
}

/// Parse a deserialized server response into a `ServerOutcome`.
fn parse_response(resp: ServerResponse) -> ServerOutcome {
    if let Some(error) = resp.error {
        return ServerOutcome::Error(error);
    }

    let codes = resp.codes.unwrap_or_default();
    let mut result: Vec<u32> = codes
        .into_iter()
        .filter_map(|c| if c >= 0 { Some(c as u32) } else { None })
        .collect();
    result.sort_unstable();
    result.dedup();
    ServerOutcome::Done(result)
}

/// Read a single JSON response line from the worker's stdout.
/// Returns `Some(response)` on success, `None` on EOF (worker died).
async fn read_response_line(
    reader: &mut BufReader<ChildStdout>,
) -> std::io::Result<Option<ServerResponse>> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Ok(None);
    }

    match serde_json::from_str::<ServerResponse>(&line) {
        Ok(resp) => Ok(Some(resp)),
        Err(e) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid server response JSON: {e}"),
        )),
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
