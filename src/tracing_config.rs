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
    if !has_tsz_log && !has_rust_log {
        return;
    }

    let filter = build_filter();
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
