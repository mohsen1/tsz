//! Indexed-access key-space construction boundary scans.
//!
//! Indexed-access validation owns AST/source selection, relation policy, and
//! diagnostics. Solver construction for key spaces and indexed-access surfaces
//! belongs in `query_boundaries::indexed_access_key_space`.

use std::fs;
use std::path::{Path, PathBuf};

const INDEXED_ACCESS_HELPERS: &str =
    "src/types/type_checking/indexed_access/indexed_access_helpers.rs";
const DEFERRED_CONDITIONAL_INDEX: &str =
    "src/types/type_checking/indexed_access/deferred_conditional_index.rs";
const ERROR_CONTAGION: &str = "src/types/type_checking/indexed_access/error_contagion.rs";
const INDEXED_ACCESS_KEY_SPACE_BOUNDARY: &str = "src/query_boundaries/indexed_access_key_space.rs";

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
    source.contains(&format!("fn {name}("))
}

#[test]
fn indexed_access_key_space_construction_routes_through_boundary() {
    let forbidden = [
        ".factory().keyof(",
        ".factory().index_access(",
        ".factory().literal_number(",
        ".factory().literal_string_atom(",
        ".factory().union(",
        ".types.union(",
        ".types.union2(",
        "tsz_solver::utils::union_or_single(",
    ];
    let mut violations = Vec::new();

    for file in [
        INDEXED_ACCESS_HELPERS,
        DEFERRED_CONDITIONAL_INDEX,
        ERROR_CONTAGION,
    ] {
        scan_for_patterns(file, &forbidden, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "indexed-access key-space construction must route through \
         query_boundaries::indexed_access_key_space:\n{}",
        violations.join("\n")
    );
}

#[test]
fn indexed_access_key_space_boundary_owns_construction_helpers() {
    let source = fs::read_to_string(checker_path(INDEXED_ACCESS_KEY_SPACE_BOUNDARY))
        .expect("failed to read query_boundaries/indexed_access_key_space.rs");

    for helper in [
        "keyof_type",
        "indexed_access_type",
        "literal_number_key",
        "literal_string_key",
        "literal_key_union",
        "key_space_union",
        "string_or_number_key_space",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::indexed_access_key_space must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.keyof(",
        "db.index_access(",
        "db.literal_number(",
        "db.literal_string_atom(",
        "db.union(",
        "db.union2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::indexed_access_key_space should own `{construction_pattern}`"
        );
    }
}
