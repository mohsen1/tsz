//! Noncanonical performance pool for server-mode tsz compilation.
//!
//! Deliberately not linked into `tsz-conformance`: pooled protocol responses
//! cannot attribute raw stderr and an ordinary exit to one case. This transport
//! may be benchmarked, but it can never score parity or write canonical artifacts.
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
use crate::process_rss::get_process_rss;
use crate::tsz_wrapper::SemanticCompletion;

/// A single long-lived `tsz-server --protocol legacy` worker process.
struct ServerWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// Outcome of a single server check request.
pub enum ServerOutcome {
    /// Normal completion — diagnostic codes in server response order.
    Done {
        codes: Vec<u32>,
        semantic_completion: SemanticCompletion,
    },
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
    codes: Option<Vec<i32>>,
    error: Option<String>,
    /// Missing/unknown completion is a capability nonclaim, never success.
    #[serde(default)]
    semantic_completion: SemanticCompletion,
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
        let request = check_request(request_id, files, directives);

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

    let Some(codes) = resp.codes else {
        return ServerOutcome::Error("server response is missing diagnostic codes".to_string());
    };
    if codes.iter().any(|code| *code < 0) {
        return ServerOutcome::Error(
            "server response contains a negative diagnostic code".to_string(),
        );
    }
    let result = codes.into_iter().map(|code| code as u32).collect();
    ServerOutcome::Done {
        codes: result,
        semantic_completion: resp.semantic_completion,
    }
}

fn check_request(
    request_id: u64,
    files: HashMap<String, String>,
    directives: &HashMap<String, String>,
) -> serde_json::Value {
    let mut files = files
        .into_iter()
        .map(|(path, content)| json!({"path": path, "content": content}))
        .collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    json!({
        "type": "check",
        "id": request_id,
        "files": files,
        "options": directives_to_check_options(directives),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_response_preserves_completion_and_ignores_legacy_id_field() {
        let resp: ServerResponse = serde_json::from_str(
            r#"{"id":7,"codes":[2322,2322],"error":null,"semantic_completion":"complete"}"#,
        )
        .expect("legacy server response should deserialize");

        match parse_response(resp) {
            ServerOutcome::Done {
                codes,
                semantic_completion,
            } => {
                assert_eq!(codes, vec![2322, 2322]);
                assert_eq!(semantic_completion, SemanticCompletion::Complete);
            }
            _ => panic!("expected normal server response"),
        }
    }

    #[test]
    fn server_response_rejects_negative_codes_instead_of_dropping_them() {
        let response: ServerResponse =
            serde_json::from_str(r#"{"codes":[-1],"semantic_completion":"complete"}"#).unwrap();
        match parse_response(response) {
            ServerOutcome::Error(message) => {
                assert_eq!(
                    message,
                    "server response contains a negative diagnostic code"
                );
            }
            _ => panic!("negative server codes must not become empty success"),
        }
    }

    #[test]
    fn server_response_missing_or_unknown_completion_fails_closed() {
        for body in [
            r#"{"codes":[]}"#,
            r#"{"codes":[],"semantic_completion":"future-verdict"}"#,
        ] {
            let response: ServerResponse = serde_json::from_str(body).unwrap();
            match parse_response(response) {
                ServerOutcome::Done {
                    codes,
                    semantic_completion,
                } => {
                    assert!(codes.is_empty());
                    assert_eq!(semantic_completion, SemanticCompletion::Incomplete);
                }
                _ => panic!("missing completion must be a semantic nonclaim"),
            }
        }
    }

    #[test]
    fn server_response_missing_codes_is_not_empty_success() {
        let response: ServerResponse =
            serde_json::from_str(r#"{"semantic_completion":"complete"}"#).unwrap();
        match parse_response(response) {
            ServerOutcome::Error(message) => {
                assert_eq!(message, "server response is missing diagnostic codes");
            }
            _ => panic!("missing diagnostic codes must not become empty success"),
        }
    }

    #[test]
    fn server_request_uses_the_legacy_array_shape_in_stable_path_order() {
        let request = check_request(
            9,
            HashMap::from([
                ("z.ts".to_string(), "const z=1;".to_string()),
                ("a.ts".to_string(), "const a=1;".to_string()),
            ]),
            &HashMap::from([("strict".to_string(), "true".to_string())]),
        );

        assert_eq!(request["id"], 9);
        assert_eq!(request["files"][0]["path"], "a.ts");
        assert_eq!(request["files"][0]["content"], "const a=1;");
        assert_eq!(request["files"][1]["path"], "z.ts");
        assert_eq!(request["options"]["strict"], true);
    }
}
