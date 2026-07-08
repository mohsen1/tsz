//! Object flag boundary scans (issue #14351).
//!
//! Checker code may construct solver object surfaces, but semantic flag policy
//! should stay behind named construction/query helpers. Raw `ObjectFlags`
//! traffic outside the named construction/augmentation boundaries turns enum
//! namespace, const-enum, and augmentation-opt-out decisions back into ad hoc
//! checker branches.

use std::fs;
use std::path::{Path, PathBuf};

const OBJECT_FLAG_BOUNDARIES: &[&str] = &[
    "src/query_boundaries/type_construction.rs",
    "src/query_boundaries/lib_augmentations.rs",
    // Named per-surface construction boundaries. Their companion scans
    // (declaration_export/interface_merge/js_constructor/object_literal
    // *_construction_boundary_scans) pin the flag policy they own.
    "src/query_boundaries/declaration_exports.rs",
    "src/query_boundaries/interface_merge.rs",
    "src/query_boundaries/type_computation/complex.rs",
    "src/query_boundaries/type_computation/object_literals.rs",
];

const OBJECT_FLAG_FACTORY_BOUNDARIES: &[&str] = &[
    "src/query_boundaries/type_construction.rs",
    // Named per-surface construction boundaries required by their companion
    // scans to own `db.object_with_flags_and_symbol(` for their surfaces.
    "src/query_boundaries/binding_patterns.rs",
    "src/query_boundaries/declaration_exports.rs",
    "src/query_boundaries/interface_merge.rs",
    "src/query_boundaries/module_augmentation.rs",
    "src/query_boundaries/type_computation/complex.rs",
    "src/query_boundaries/type_computation/object_literals.rs",
];

const RAW_OBJECT_FLAG_PATTERNS: &[&str] = &[
    "ObjectFlags::",
    "tsz_solver::ObjectFlags",
    "common::ObjectFlags",
    "query_boundaries::common::ObjectFlags",
];

const RAW_OBJECT_FLAG_FACTORY_PATTERNS: &[&str] = &["object_with_flags_and_symbol("];

fn checker_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn production_object_shape_rebuilds_avoid_raw_flag_factory() {
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
        if OBJECT_FLAG_FACTORY_BOUNDARIES.contains(&relative_path.as_str()) {
            continue;
        }

        let source = fs::read_to_string(&path).expect("failed to read Rust source");
        for (line_index, line) in source.lines().enumerate() {
            if is_comment_or_doc_line(line) {
                continue;
            }
            for pattern in RAW_OBJECT_FLAG_FACTORY_PATTERNS {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{}:{} calls raw object flag factory `{}`",
                        relative_path,
                        line_index + 1,
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "checker production code should rebuild object metadata through named \
         factory/query-boundary helpers instead of passing raw flags:\n{}",
        violations.join("\n")
    );
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

fn is_comment_or_doc_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("//!")
}

#[test]
fn production_object_flags_use_type_construction_boundary() {
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
        if OBJECT_FLAG_BOUNDARIES.contains(&relative_path.as_str()) {
            continue;
        }

        let source = fs::read_to_string(&path).expect("failed to read Rust source");
        for (line_index, line) in source.lines().enumerate() {
            if is_comment_or_doc_line(line) {
                continue;
            }
            for pattern in RAW_OBJECT_FLAG_PATTERNS {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{}:{} contains raw object-flag traffic `{}`",
                        relative_path,
                        line_index + 1,
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "checker production code should route object flag decisions through \
         named query-boundary helpers:\n{}",
        violations.join("\n")
    );
}
