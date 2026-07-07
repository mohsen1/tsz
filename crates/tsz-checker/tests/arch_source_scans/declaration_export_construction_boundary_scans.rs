//! Declaration export construction boundary scans.
//!
//! Namespace/module declaration checkers collect AST and binder facts. Solver
//! property, object, callable, lazy-ref, intersection, and application
//! construction for export surfaces belongs in
//! `query_boundaries::declaration_exports`.

use std::fs;
use std::path::{Path, PathBuf};

const DECLARATION_EXPORT_CALLERS: &[&str] = &[
    "src/declarations/module_checker.rs",
    "src/declarations/namespace_checker.rs",
];
const DECLARATION_EXPORT_BOUNDARY: &str = "src/query_boundaries/declaration_exports.rs";
const COMMON_BOUNDARY: &str = "src/query_boundaries/common.rs";

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

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}(")) || source.contains(&format!("fn {name}<"))
}

#[test]
fn declaration_export_callers_route_solver_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "PropertyInfo {",
        "PropertyInfo::new(",
        "CallableShape {",
        "CallableShape::default()",
        ".factory().object(",
        ".ctx.types.factory().object(",
        "factory.object(",
        ".object_with_symbol(",
        ".factory().callable(",
        ".callable(",
        ".application(",
        ".lazy(",
        ".intersection2(",
    ];

    let mut violations = Vec::new();
    for caller in DECLARATION_EXPORT_CALLERS {
        scan_for_patterns(caller, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "declaration export-surface callers must route solver construction \
         through query_boundaries::declaration_exports:\n{}",
        violations.join("\n")
    );
}

#[test]
fn declaration_exports_boundary_owns_export_surface_construction_helpers() {
    let source = fs::read_to_string(checker_path(DECLARATION_EXPORT_BOUNDARY))
        .expect("failed to read query_boundaries/declaration_exports.rs");
    let common =
        fs::read_to_string(checker_path(COMMON_BOUNDARY)).expect("failed to read common.rs");

    for helper in [
        "declaration_export_property",
        "declaration_lazy_export_type",
        "module_export_augmented_type",
        "dynamic_import_module_object_type",
        "dynamic_import_promise_type",
        "empty_namespace_object_type",
        "namespace_object_placeholder_type",
        "namespace_object_type",
        "namespace_merged_constructor_callable_type",
        "namespace_merged_function_callable_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::declaration_exports must own `{helper}`"
        );
        assert!(
            !defines_fn(&common, helper),
            "query_boundaries::common must not define `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo {",
        "Visibility::Public",
        "db.lazy(",
        "db.intersection2(",
        "db.object(",
        "db.object_with_flags_and_symbol(",
        "db.application(",
        "db.callable(",
        "CallableShape {",
        "ObjectFlags::empty()",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::declaration_exports should own `{construction_pattern}`"
        );
    }
}
