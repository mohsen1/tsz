//! CommonJS/JSON export-surface construction boundary scans.
//!
//! `exports_detection.rs` owns JSON parsing, module-mode decisions, and
//! current-file CommonJS export-name discovery. Solver construction for the
//! resulting JSON value and namespace surfaces belongs in
//! `query_boundaries::js_exports`.

use std::fs;
use std::path::Path;

const EXPORTS_DETECTION: &str = "src/state/type_analysis/computed_commonjs/exports_detection.rs";
const JS_EXPORTS_BOUNDARY: &str = "src/query_boundaries/js_exports.rs";

fn checker_path(relative: &str) -> std::path::PathBuf {
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
fn commonjs_json_export_surface_construction_routes_through_boundary() {
    let source = fs::read_to_string(checker_path(EXPORTS_DETECTION))
        .expect("failed to read computed_commonjs/exports_detection.rs");
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    let forbidden = [
        "PropertyInfo {",
        "Visibility::Public",
        ".factory().object(",
        ".factory().array(",
        ".factory().union(",
        ".factory().union2(",
        ".types.object(",
        ".types.array(",
        ".types.union(",
        "fn json_value_type(",
        "fn json_array_type(",
        "fn json_object_type(",
        "fn json_array_object_property_order(",
        "JsExportSurface {",
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
        "CommonJS/JSON export-surface construction must route through \
         query_boundaries::js_exports, found: {}",
        violations.join(", ")
    );
}

#[test]
fn js_exports_boundary_owns_commonjs_json_surface_helpers() {
    let source = fs::read_to_string(checker_path(JS_EXPORTS_BOUNDARY))
        .expect("failed to read query_boundaries/js_exports.rs");

    for helper in [
        "json_module_union",
        "json_module_array_type",
        "json_module_object_type",
        "json_module_object_property",
        "json_module_missing_property",
        "json_module_value_type",
        "json_esm_namespace_type",
        "commonjs_json_namespace_type",
        "commonjs_export_surface_can_merge_named_exports",
        "current_file_commonjs_namespace_type",
        "commonjs_namespace_any_property",
        "commonjs_empty_namespace_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::js_exports must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo {",
        "db.union(",
        "db.array(",
        "db.object(",
        "Visibility::Public",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::js_exports should own `{construction_pattern}`"
        );
    }
}
