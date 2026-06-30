//! Spelling-suggestion gateway scans.
//!
//! `find_similar_identifiers` walks the visible symbol universe and performs
//! spelling-distance work. Namespace diagnostics used to call it directly,
//! bypassing the memoized `(node, meaning)` gateway and the shared cap. Keep
//! production checker call sites routed through
//! `error_reporter::name_resolution::scan_similar_identifiers_for_meaning`.

use std::fs;
use std::path::{Path, PathBuf};

fn checker_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
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

fn rust_sources_under(dir: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    collect_rust_sources(dir, &mut sources);
    sources.sort();
    sources
}

#[test]
fn production_spelling_suggestions_use_memoized_gateway() {
    let src_root = checker_src_root();
    let mut violations = Vec::new();

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
            if !line.contains(".find_similar_identifiers(") {
                continue;
            }
            if relative_path == "src/error_reporter/name_resolution.rs" {
                continue;
            }
            violations.push(format!(
                "{}:{} calls find_similar_identifiers directly",
                relative_path,
                line_index + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "checker production code should route spelling scans through \
         scan_similar_identifiers_for_meaning so memoization and cap gating stay shared:\n{}",
        violations.join("\n")
    );
}
