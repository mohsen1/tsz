//! CommonJS resolution export-surface construction boundary scans.
//!
//! `exports_resolution.rs` owns CommonJS AST recognition, symbol lookup, and
//! module-resolution policy. Solver construction for descriptor properties,
//! object overlays, callable constructor upgrades, and imported module value
//! surfaces belongs in `query_boundaries::js_exports`.

use std::fs;
use std::path::{Path, PathBuf};

const EXPORTS_RESOLUTION: &str = "src/state/type_analysis/computed_commonjs/exports_resolution.rs";
const JS_EXPORTS_BOUNDARY: &str = "src/query_boundaries/js_exports.rs";

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

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn commonjs_resolution_export_surface_construction_routes_through_boundary() {
    let source = fs::read_to_string(checker_path(EXPORTS_RESOLUTION))
        .expect("failed to read computed_commonjs/exports_resolution.rs");
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    let forbidden = [
        "PropertyInfo {",
        "Visibility::Public",
        "ObjectShape {",
        "FunctionShape::new",
        "CallSignature {",
        "CallableShape {",
        ".factory().function(",
        ".factory().callable(",
        ".factory().object",
        ".factory().intersection2(",
        ".types.object(",
        ".types.intersection2(",
        ".to_type_id_with_display_name(",
    ];

    let mut violations = Vec::new();
    for pattern in forbidden {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(pattern);
        }
    }

    assert!(
        violations.is_empty(),
        "CommonJS resolution export-surface construction must route through \
         query_boundaries::js_exports, found: {}",
        violations.join(", ")
    );
}

#[test]
fn js_exports_boundary_owns_commonjs_resolution_surface_helpers() {
    let source = fs::read_to_string(checker_path(JS_EXPORTS_BOUNDARY))
        .expect("failed to read query_boundaries/js_exports.rs");

    for helper in [
        "commonjs_namespace_export_property",
        "commonjs_define_property_setter_contextual_function_type",
        "commonjs_define_property_descriptor_property",
        "commonjs_type_with_define_property_members",
        "commonjs_export_constructor_type_with_instance",
        "commonjs_export_surface_type_with_display_name",
        "commonjs_imported_module_value_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::js_exports must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo {",
        "Visibility::Public",
        "FunctionShape::new",
        "CallSignature {",
        "CallableShape {",
        "ObjectShape {",
        "db.function(",
        "db.callable(",
        "db.object(",
        "db.object_with_index(",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::js_exports should own `{construction_pattern}`"
        );
    }
}
