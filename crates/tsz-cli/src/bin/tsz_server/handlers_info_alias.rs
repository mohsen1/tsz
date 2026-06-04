use std::collections::BTreeSet;

use std::io::Write;

use std::path::{Path, PathBuf};

use std::process::{Command, Stdio};

use super::Server;

use tsz::lsp::definition::GoToDefinition;

use tsz::lsp::hover::HoverProvider;

use tsz::lsp::position::LineMap;

use tsz::parser::node::NodeAccess;

use tsz_scanner::SyntaxKind;

use tsz_solver::construction::TypeInterner;

use super::handlers_info::ParsedFileContext;

type LocationKey = (String, u32, u32, u32, u32);

type LocationKeySet = rustc_hash::FxHashSet<LocationKey>;

include!("handlers_info_alias_parts/part1.rs");
include!("handlers_info_alias_parts/part2.rs");

/// Long-running Node.js subprocess that delegates to the real `tsc`
/// `LanguageService`. The first request pays the TypeScript-module load
/// cost; subsequent requests reuse the loaded runtime, turning ~1–2 s
/// per-operation cold starts into tens of milliseconds.
pub(crate) struct NativeTsWorker {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl NativeTsWorker {
    pub(crate) fn spawn() -> Option<Self> {
        let script = Self::loop_script()?;
        let mut child = std::process::Command::new("node")
            .arg("-e")
            .arg(&script)
            .env("TSZ_NATIVE_TS_PERSISTENT", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        let stdin = child.stdin.take()?;
        let stdout = std::io::BufReader::new(child.stdout.take()?);
        Some(Self {
            child,
            stdin,
            stdout,
        })
    }

    /// Synchronous request/response roundtrip against the worker.
    /// Returns `None` if the worker isn't healthy or the response is
    /// malformed; the caller should then fall back to spawning a fresh
    /// subprocess via the legacy single-shot path.
    pub(crate) fn request(
        &mut self,
        _script: &str,
        payload: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        use std::io::{BufRead, Write};
        if self.child.try_wait().ok().flatten().is_some() {
            return None;
        }
        let mut line = serde_json::to_vec(payload).ok()?;
        line.push(b'\n');
        self.stdin.write_all(&line).ok()?;
        self.stdin.flush().ok()?;
        let mut response = Vec::new();
        self.stdout.read_until(b'\n', &mut response).ok()?;
        if response.ends_with(b"\n") {
            response.pop();
        }
        if response.is_empty() {
            return None;
        }
        serde_json::from_slice(&response).ok()
    }

    /// Extracts the embedded Node.js worker script. Shared with the
    /// single-shot path so that we don't drift between the two modes.
    fn loop_script() -> Option<String> {
        // The worker script is stored inline in `try_native_typescript_operation`
        // as the `SCRIPT` constant. We reach it via a tiny dummy Server
        // instance at spawn time — but since that would be circular, we
        // instead embed a small prelude that triggers the TypeScript-module
        // load and loops. For now, reuse the full script source via
        // `include_str!` if we split it out; fall back to re-emitting the
        // known loop harness.
        Some(include_str!("native_ts_worker.js").to_string())
    }
}

impl Drop for NativeTsWorker {
    fn drop(&mut self) {
        // Closing stdin lets the child exit cleanly on its next read.
        // If the worker is already gone, kill() is a no-op.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::Server;

    #[test]
    fn import_statement_context_span_accepts_export_specifier_lines() {
        let source = "const foo = 1;\nexport { foo as \"__<alias>\" };\n";
        let anchor = source
            .find("__<alias>")
            .expect("expected alias literal in source") as u32;
        let span = Server::import_statement_context_span(source, anchor)
            .expect("expected context span for export specifier line");
        let line = &source[span.0 as usize..span.1 as usize];
        assert!(
            line.trim_start().starts_with("export "),
            "expected export statement context, got: {line:?}"
        );
    }
}
