//! JSDoc construction boundary scans.
//!
//! JSDoc resolution code parses source text and gathers facts. Raw solver
//! `FunctionShape`, `ObjectShape`, and indexed-object construction belongs in
//! `query_boundaries::jsdoc_construction`.

use std::fs;
use std::path::{Path, PathBuf};

const JSDOC_CONSTRUCTION_CALLERS: &[&str] = &[
    "src/jsdoc/resolution/type_construction.rs",
    "src/jsdoc/resolution/name_resolution.rs",
    "src/jsdoc/params_type_strings.rs",
];

const JSDOC_CONSTRUCTION_BOUNDARY: &str = "src/query_boundaries/jsdoc_construction.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
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
fn jsdoc_callers_route_solver_shape_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "FunctionShape",
        "ObjectShape",
        "IndexSignature",
        ".function(",
        ".object(",
        ".object_with_index(",
        ".callable(",
    ];

    let mut violations = Vec::new();
    for module in JSDOC_CONSTRUCTION_CALLERS {
        scan_for_patterns(module, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "JSDoc construction callers must route solver shape construction \
         through query_boundaries::jsdoc_construction:\n{}",
        violations.join("\n")
    );
}

#[test]
fn jsdoc_construction_boundary_owns_helpers_and_shape_literals() {
    let source = fs::read_to_string(checker_path(JSDOC_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/jsdoc_construction.rs");
    let common = fs::read_to_string(checker_path("src/query_boundaries/common.rs"))
        .expect("failed to read query_boundaries/common.rs");

    for helper in [
        "jsdoc_object_index_fact",
        "jsdoc_empty_object_type",
        "jsdoc_object_type",
        "jsdoc_object_index_type",
        "jsdoc_function_type",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::jsdoc_construction must own `{helper}`"
        );
        assert!(
            !common.contains(&format!("fn {helper}(")),
            "JSDoc construction helper `{helper}` must not drift into common.rs"
        );
    }

    for shape_pattern in ["FunctionShape {", "ObjectShape {", "IndexSignature {"] {
        assert!(
            source.contains(shape_pattern),
            "query_boundaries::jsdoc_construction should own `{shape_pattern}`"
        );
    }
}
