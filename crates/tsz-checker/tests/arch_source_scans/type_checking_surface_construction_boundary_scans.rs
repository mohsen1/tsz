//! Type-checking surface construction boundary scans.
//!
//! Type-checking modules own AST traversal, environment setup, relation
//! requests, and diagnostics. Solver construction for type-checking helper
//! surfaces belongs in `query_boundaries::type_checking`.

use std::fs;
use std::path::{Path, PathBuf};

const TYPE_CHECKING_ROOT: &str = "src/types/type_checking";
const TYPE_CHECKING_BOUNDARY: &str = "src/query_boundaries/type_checking.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn collect_rs_files(relative: &str, files: &mut Vec<PathBuf>) {
    let path = checker_path(relative);
    let entries = fs::read_dir(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|err| panic!("failed to read entry in {relative}: {err}"));
        let path = entry.path();
        if path.is_dir() {
            let nested = path
                .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
                .expect("path should be under checker manifest dir")
                .to_string_lossy()
                .into_owned();
            collect_rs_files(&nested, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn relative_path(path: &Path) -> String {
    path.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("path should be under checker manifest dir")
        .to_string_lossy()
        .into_owned()
}

fn scan_file(path: &Path, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let relative = relative_path(path);
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.contains("->") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_index + 1
                ));
            }
        }
    }

    let compact_source = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(|line| line.chars())
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    for pattern in patterns {
        let compact_pattern = pattern
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        if compact_source.contains(&compact_pattern) {
            violations.push(format!(
                "{relative} contains split or inline `{pattern}` construction"
            ));
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn type_checking_surface_construction_routes_through_boundary() {
    let forbidden = [
        ".factory().function(",
        ".factory().union(",
        ".factory().type_param(",
        ".factory().index_access(",
        ".literal_number(",
        ".types.union(",
        ".types.union2(",
        ".types.index_access(",
        "FunctionShape {",
        "ParamInfo {",
        "TypeParamInfo {",
        "TypeParamOrigin::User",
    ];
    let mut files = Vec::new();
    let mut violations = Vec::new();
    collect_rs_files(TYPE_CHECKING_ROOT, &mut files);

    for file in files {
        scan_file(&file, &forbidden, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "type-checking helper surface construction must route through \
         query_boundaries::type_checking:\n{}",
        violations.join("\n")
    );
}

#[test]
fn type_checking_boundary_owns_surface_construction_helpers() {
    let source = fs::read_to_string(checker_path(TYPE_CHECKING_BOUNDARY))
        .expect("failed to read query_boundaries/type_checking.rs");

    for helper in [
        "type_checking_union",
        "type_checking_index_access",
        "type_checking_literal_number",
        "user_type_param_info",
        "user_type_param",
        "param_info",
        "global_function_fallback_type",
        "method_function_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_checking must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.union(",
        "db.index_access(",
        "db.literal_number(",
        "db.type_param(",
        "db.function(",
        "TypeParamInfo {",
        "ParamInfo {",
        "FunctionShape {",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_checking should own `{construction_pattern}`"
        );
    }
}
