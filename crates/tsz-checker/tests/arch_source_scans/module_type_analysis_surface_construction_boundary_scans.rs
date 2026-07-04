//! Module/namespace type-analysis surface construction boundary scans.
//!
//! `computed/mod.rs` and `computed/type_alias_variable_alias.rs` own module
//! resolution, export filtering, augmentation lookup, and display-name
//! publication. Solver construction for namespace export properties, namespace
//! object surfaces, and export-equals/namespace intersections belongs in
//! `query_boundaries::state::type_analysis`.

use std::fs;
use std::path::{Path, PathBuf};

const COMPUTED_MOD: &str = "src/state/type_analysis/computed/mod.rs";
const TYPE_ALIAS_VARIABLE_ALIAS: &str =
    "src/state/type_analysis/computed/type_alias_variable_alias.rs";
const TYPE_ALIAS_VARIABLE_ALIAS_HELPERS: &str =
    "src/state/type_analysis/computed/type_alias_variable_alias/helpers.rs";
const TYPE_ANALYSIS_BOUNDARY: &str = "src/query_boundaries/state/type_analysis.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn production_source_without_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn scan_source_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    for pattern in patterns {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn module_type_analysis_surfaces_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    let patterns = [
        "PropertyInfo {",
        "Visibility::Public",
        ".ctx.types.factory().object(",
        "self.ctx.types.factory().object(",
        ".ctx.types.factory().intersection2(",
        "self.ctx.types.factory().intersection2(",
        ".factory().object(",
        ".factory().intersection2(",
        ".types.object(",
        ".types.intersection2(",
        "factory.object(",
        "factory.intersection2(",
    ];

    for path in [
        COMPUTED_MOD,
        TYPE_ALIAS_VARIABLE_ALIAS,
        TYPE_ALIAS_VARIABLE_ALIAS_HELPERS,
    ] {
        scan_source_for_patterns(path, &patterns, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "module/namespace type-analysis surfaces must route solver construction \
         through query_boundaries::state::type_analysis:\n{}",
        violations.join("\n")
    );
}

#[test]
fn state_type_analysis_boundary_owns_module_surface_helpers() {
    let source = fs::read_to_string(checker_path(TYPE_ANALYSIS_BOUNDARY))
        .expect("failed to read query_boundaries/state/type_analysis.rs");

    for helper in [
        "namespace_export_property",
        "namespace_any_export_property",
        "namespace_object_type",
        "namespace_export_equals_intersection",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::state::type_analysis must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo {",
        "Visibility::Public",
        "db.object(",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::state::type_analysis should own `{construction_pattern}`"
        );
    }
}
