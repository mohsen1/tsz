//! Index-signature resolver boundary scans (issue #14351).
//!
//! Production checker code should ask `query_boundaries::index_signature` for
//! index-signature presence/value information. The raw solver resolver is a
//! boundary implementation detail, not a call-site dependency.

use std::fs;
use std::path::{Path, PathBuf};

const RAW_RESOLVER_CONSTRUCTOR: &str = "IndexSignatureResolver::new";

/// Temporary overlap exceptions:
/// - `return_context_substitution.rs` is owned by active #15167.
/// - `types/utilities/core.rs` is owned by active #15182.
const TEMPORARY_ALLOWLIST: &[&str] = &[
    "src/types/computation/call_inference/return_context_substitution.rs",
    "src/types/utilities/core.rs",
];

fn checker_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_sources_under(dir: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    collect_rust_sources(dir, &mut sources);
    sources.sort();
    sources
}

fn collect_rust_sources(dir: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("failed to read source directory") {
        let path = entry.expect("failed to read source entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                collect_rust_sources(&path, sources);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            sources.push(path);
        }
    }
}

fn allowed_raw_resolver_constructor(relative_path: &str) -> bool {
    relative_path == "src/query_boundaries/index_signature.rs"
        || TEMPORARY_ALLOWLIST.contains(&relative_path)
}

#[test]
fn production_checker_index_signature_queries_use_boundary() {
    let mut violations = Vec::new();
    let src_root = checker_src_root();

    for path in rust_sources_under(&src_root) {
        let relative_path = format!(
            "src/{}",
            path.strip_prefix(&src_root)
                .expect("scanned path is under the src root")
                .to_string_lossy()
                .replace('\\', "/")
        );
        let source = fs::read_to_string(&path).expect("failed to read Rust source");
        for (line_index, line) in source.lines().enumerate() {
            if line.contains(RAW_RESOLVER_CONSTRUCTOR)
                && !allowed_raw_resolver_constructor(&relative_path)
            {
                violations.push(format!(
                    "{}:{} constructs the raw solver index-signature resolver",
                    relative_path,
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "checker index-signature queries must route through \
         query_boundaries::index_signature:\n{}",
        violations.join("\n")
    );
}
