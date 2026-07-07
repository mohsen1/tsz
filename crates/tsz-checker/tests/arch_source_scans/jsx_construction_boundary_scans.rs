//! JSX construction boundary scans.
//!
//! JSX checker modules gather JSX syntax, prop, and callback facts. Interning
//! object/function/callable solver types and rebuilding `FunctionShape`
//! literals belongs in `query_boundaries::checkers::jsx`.

use std::fs;
use std::path::{Path, PathBuf};

const JSX_CHECKER_ROOT: &str = "src/checkers/jsx";
const JSX_CONSTRUCTION_BOUNDARY: &str = "src/query_boundaries/checkers/jsx.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
    {
        let path = entry
            .unwrap_or_else(|err| panic!("failed to read entry under {}: {err}", dir.display()))
            .path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        // Return types and argument types may mention shape structs as
        // read-only data. This scan guards construction, not type signatures.
        if trimmed.contains("->") || trimmed.ends_with("&tsz_solver::FunctionShape,") {
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
}

#[test]
fn jsx_checkers_route_solver_shape_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".factory().object(",
        ".factory().function(",
        ".factory().callable(",
        "factory().object(",
        "factory().function(",
        "factory().callable(",
        ".factory.object(",
        ".factory.function(",
        ".factory.callable(",
        "FunctionShape {",
        "FunctionShape::new(",
        "CallableShape {",
        "ObjectShape {",
        "ParamInfo::required(",
    ];

    let root = checker_path(JSX_CHECKER_ROOT);
    let mut files = Vec::new();
    collect_rust_files(&root, &mut files);
    files.sort();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(manifest_dir)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to strip manifest dir {} from {}: {err}",
                    manifest_dir.display(),
                    file.display()
                )
            })
            .to_str()
            .expect("checker path is valid UTF-8");
        scan_for_patterns(relative, FORBIDDEN_PATTERNS, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "JSX checker modules must route solver shape construction through \
         query_boundaries::checkers::jsx:\n{}",
        violations.join("\n")
    );
}

#[test]
fn jsx_boundary_owns_construction_helpers() {
    let source = fs::read_to_string(checker_path(JSX_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/jsx.rs");
    for helper in [
        "object_type_from_properties",
        "empty_props_object_type",
        "props_param_type_or_empty",
        "function_type_from_shape",
        "function_type_from_parts",
        "single_required_param_function_type",
        "function_type_with_mapped_component_types",
        "function_type_without_this",
        "construct_signature_function_shape",
        "push_required_param",
        "synthetic_single_param_function_shape",
        "instantiate_function_shape_preserving_unresolved_params",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::checkers::jsx must own `{helper}`"
        );
    }
}
