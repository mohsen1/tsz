//! Class/recovery surface construction boundary scans.
//!
//! Class recovery, final instance merge, JSDoc class templates, and constructor
//! heritage refresh code own traversal, lookup, sorting, and stale-signature
//! decisions. Raw callable/object/type-parameter construction belongs behind
//! class or signature query boundaries once those facts are known.

use std::fs;
use std::path::{Path, PathBuf};

const CLASS_RECOVERY: &str = "src/types/property_access_type/class_recovery.rs";
const INSTANCE_MERGE: &str = "src/types/class_type/instance_merge.rs";
const CLASS_HELPERS: &str = "src/types/class_type/helpers.rs";
const HERITAGE_IDENTITY: &str = "src/types/class_type/heritage_identity.rs";
const CLASS_BOUNDARY: &str = "src/query_boundaries/class_type.rs";
const SIGNATURE_BOUNDARY: &str = "src/query_boundaries/signature_building.rs";

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

fn scan(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
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

#[test]
fn class_recovery_surfaces_route_solver_construction_through_boundaries() {
    let mut violations = Vec::new();
    scan(
        CLASS_RECOVERY,
        &[
            "CallableShape {",
            ".factory().callable(",
            ".factory().union2(",
        ],
        &mut violations,
    );
    scan(
        INSTANCE_MERGE,
        &[
            "ObjectShape {",
            ".factory().object_with_index(",
            "mark_has_late_bound_members(",
            "mark_no_module_augmentation_lookup(",
        ],
        &mut violations,
    );
    scan(
        CLASS_HELPERS,
        &["TypeParamInfo {", ".factory().type_param("],
        &mut violations,
    );
    scan(
        HERITAGE_IDENTITY,
        &["CallableShape {", ".factory().callable("],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "class recovery surfaces must route raw solver construction through \
         query boundaries:\n{}",
        violations.join("\n")
    );
}

#[test]
fn class_recovery_boundaries_own_surface_helpers() {
    let class_boundary = fs::read_to_string(checker_path(CLASS_BOUNDARY))
        .expect("failed to read query_boundaries/class_type.rs");
    for helper in [
        "class_method_callable_type",
        "optional_class_member_type",
        "final_class_instance_type",
        "class_constructor_callable_with_construct_signatures_replaced",
    ] {
        assert!(
            defines_fn(&class_boundary, helper),
            "query_boundaries::class_type must own `{helper}`"
        );
    }

    let signature_boundary = fs::read_to_string(checker_path(SIGNATURE_BOUNDARY))
        .expect("failed to read query_boundaries/signature_building.rs");
    for helper in ["user_type_param_info", "user_type_param"] {
        assert!(
            defines_fn(&signature_boundary, helper),
            "query_boundaries::signature_building must own `{helper}`"
        );
    }
}
