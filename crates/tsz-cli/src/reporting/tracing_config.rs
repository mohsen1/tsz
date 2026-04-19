//! Tracing configuration for debugging conformance failures.
//!
//! Supports three output formats controlled by `TSZ_LOG_FORMAT`:
//!
//! - `text` (default): Standard `tracing-subscriber` flat output
//! - `tree`: Hierarchical indented output via `tracing-tree` — easy to read,
//!   great for pasting into conversations
//! - `json`: One JSON object per span/event — machine-readable, also pasteable
//!
//! ## Quick start
//!
//! ```bash
//! # Human-readable tree (recommended for debugging conformance)
//! TSZ_LOG=debug TSZ_LOG_FORMAT=tree tsz file.ts
//!
//! # JSON (for tooling or sharing full traces)
//! TSZ_LOG=debug TSZ_LOG_FORMAT=json tsz file.ts
//!
//! # Plain text (classic fmt subscriber)
//! TSZ_LOG=debug tsz file.ts
//!
//! # Fine-grained filtering
//! TSZ_LOG="wasm::checker=debug,wasm::solver=trace" TSZ_LOG_FORMAT=tree tsz file.ts
//! ```
//!
//! The subscriber is only initialised when `TSZ_LOG` (or `RUST_LOG`) is set,
//! so there is zero overhead in normal builds.
//! `TSZ_PERF` also enables a minimal default perf filter (`wasm::perf=info`).

use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry, fmt};

/// Tracing output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Standard flat text lines (default).
    Text,
    /// Hierarchical indented tree via `tracing-tree`.
    Tree,
    /// Newline-delimited JSON objects.
    Json,
}

impl LogFormat {
    /// Parse from the `TSZ_LOG_FORMAT` environment variable.
    fn from_env() -> Self {
        match std::env::var("TSZ_LOG_FORMAT")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "tree" => Self::Tree,
            "json" => Self::Json,
            _ => Self::Text,
        }
    }
}

/// Build an `EnvFilter` from `TSZ_LOG`, falling back to `RUST_LOG`.
///
/// `TSZ_LOG` takes precedence when both are set. Values use the same
/// syntax as `RUST_LOG` (e.g. `debug`, `wasm::checker=trace`).
fn build_filter() -> EnvFilter {
    if let Ok(val) = std::env::var("TSZ_LOG") {
        EnvFilter::builder().parse_lossy(val)
    } else {
        // RUST_LOG is set (caller already checked).  Use it as-is.
        EnvFilter::from_default_env()
    }
}

/// Initialise the global tracing subscriber.
///
/// Does nothing when neither `TSZ_LOG` nor `RUST_LOG` is set, keeping startup
/// cost at zero for normal usage.
///
/// All output goes to stderr so it never interferes with stdout
/// (compiler diagnostics, `--showConfig`, or LSP JSON-RPC).
pub fn init_tracing() {
    // Only pay for tracing when explicitly requested.
    let has_tsz_log = std::env::var("TSZ_LOG").is_ok();
    let has_rust_log = std::env::var("RUST_LOG").is_ok();
    let has_perf = std::env::var_os("TSZ_PERF").is_some();
    if !has_tsz_log && !has_rust_log && !has_perf {
        return;
    }

    let filter = if has_tsz_log || has_rust_log {
        build_filter()
    } else {
        EnvFilter::builder().parse_lossy("wasm::perf=info")
    };
    let format = LogFormat::from_env();

    match format {
        LogFormat::Tree => {
            let tree_layer = tracing_tree::HierarchicalLayer::default()
                .with_indent_amount(2)
                .with_indent_lines(true)
                .with_deferred_spans(true)
                .with_span_retrace(true)
                .with_targets(true);

            Registry::default().with(filter).with(tree_layer).init();
        }
        LogFormat::Json => {
            let json_layer = fmt::layer().json().with_writer(std::io::stderr);

            Registry::default().with(filter).with(json_layer).init();
        }
        LogFormat::Text => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .init();
        }
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct TestEnv {
        previous: Vec<(&'static str, Option<OsString>)>,
    }

    impl TestEnv {
        fn new() -> Self {
            Self {
                previous: Vec::new(),
            }
        }

        fn remember(&mut self, key: &'static str) {
            if self.previous.iter().any(|(existing, _)| *existing == key) {
                return;
            }
            self.previous.push((key, env::var_os(key)));
        }

        fn set(&mut self, key: &'static str, value: &str) {
            self.remember(key);
            unsafe { env::set_var(key, value) };
        }

        fn unset(&mut self, key: &'static str) {
            self.remember(key);
            unsafe { env::remove_var(key) };
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            for (key, value) in self.previous.drain(..).rev() {
                match value {
                    Some(value) => unsafe { env::set_var(key, value) },
                    None => unsafe { env::remove_var(key) },
                }
            }
        }
    }

    #[test]
    fn log_format_from_env_defaults_to_text_and_normalizes_case() {
        let _guard = env_lock().lock().unwrap();
        let mut env = TestEnv::new();

        env.unset("TSZ_LOG_FORMAT");
        assert_eq!(LogFormat::from_env(), LogFormat::Text);

        env.set("TSZ_LOG_FORMAT", "TrEe");
        assert_eq!(LogFormat::from_env(), LogFormat::Tree);

        env.set("TSZ_LOG_FORMAT", "JSON");
        assert_eq!(LogFormat::from_env(), LogFormat::Json);
    }

    #[test]
    fn build_filter_prefers_tsz_log_over_rust_log() {
        let _guard = env_lock().lock().unwrap();
        let mut env = TestEnv::new();

        env.set("TSZ_LOG", "wasm::checker=trace");
        env.set("RUST_LOG", "wasm::solver=debug");

        let filter = build_filter();
        let expected = EnvFilter::builder().parse_lossy("wasm::checker=trace");

        assert_eq!(filter.to_string(), expected.to_string());
    }

    #[test]
    fn build_filter_falls_back_to_rust_log_when_tsz_log_is_missing() {
        let _guard = env_lock().lock().unwrap();
        let mut env = TestEnv::new();

        env.unset("TSZ_LOG");
        env.set("RUST_LOG", "wasm::solver=debug");

        let filter = build_filter();
        let expected = EnvFilter::from_default_env();

        assert_eq!(filter.to_string(), expected.to_string());
    }
}
