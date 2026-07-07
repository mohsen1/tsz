//! Module augmentation surface construction boundary scans.
//!
//! `types/module_augmentation.rs` owns augmentation discovery, member merge
//! policy, definition-store publication, and recursive prototype routing.
//! Solver construction for augmented properties, application references,
//! object/index/callable surfaces, and fallback intersections belongs in
//! `query_boundaries::module_augmentation`.

use std::fs;
use std::path::{Path, PathBuf};

const MODULE_AUGMENTATION: &str = "src/types/module_augmentation.rs";
const MODULE_AUGMENTATION_BOUNDARY: &str = "src/query_boundaries/module_augmentation.rs";

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
fn module_augmentation_surfaces_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    let patterns = [
        "PropertyInfo {",
        "Visibility::Public",
        "ObjectShape {",
        "CallableShape {",
        ".ctx.types.factory().application(",
        "self.ctx.types.factory().application(",
        ".factory().application(",
        "factory.application(",
        "factory.object(",
        "factory.object_with",
        "factory.object_with_index(",
        "factory.callable(",
        "factory.intersection2(",
        ".types.object(",
        ".types.intersection2(",
    ];

    scan_source_for_patterns(MODULE_AUGMENTATION, &patterns, &mut violations);

    assert!(
        violations.is_empty(),
        "module augmentation surfaces must route solver construction through \
         query_boundaries::module_augmentation:\n{}",
        violations.join("\n")
    );
}

#[test]
fn module_augmentation_boundary_owns_surface_helpers() {
    let source = fs::read_to_string(checker_path(MODULE_AUGMENTATION_BOUNDARY))
        .expect("failed to read query_boundaries/module_augmentation.rs");

    for helper in [
        "augmentation_member_property",
        "augmentation_any_member_property",
        "self_reference_application_type",
        "augmented_object_type",
        "augmented_object_with_index_type",
        "augmented_callable_type",
        "other_target_with_augmentation_members",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::module_augmentation must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo {",
        "Visibility::Public",
        "ObjectShape {",
        "CallableShape {",
        "db.application(",
        "db.object_with_flags_and_symbol(",
        "db.object_with_index(",
        "db.callable(",
        "db.object(",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::module_augmentation should own `{construction_pattern}`"
        );
    }
}
