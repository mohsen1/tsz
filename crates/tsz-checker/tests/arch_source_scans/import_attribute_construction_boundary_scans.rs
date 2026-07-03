//! Import attribute construction boundary scans.
//!
//! Static and dynamic import checkers gather syntax facts and issue grammar or
//! assignability diagnostics. Solver property/object construction for
//! `ImportAttributes` and `ImportCallOptions` belongs in
//! `query_boundaries::import_attributes`.

use std::fs;
use std::path::{Path, PathBuf};

const IMPORT_ATTRIBUTE_CALLERS: &[&str] = &[
    "src/declarations/import/declaration_attributes.rs",
    "src/declarations/dynamic_import_checker.rs",
];
const IMPORT_ATTRIBUTE_BOUNDARY: &str = "src/query_boundaries/import_attributes.rs";
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
fn import_attribute_callers_route_solver_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "PropertyInfo {",
        "PropertyInfo::new(",
        "PropertyInfo::opt(",
        ".factory().object(",
        ".factory().literal_string(",
    ];

    let mut violations = Vec::new();
    for caller in IMPORT_ATTRIBUTE_CALLERS {
        scan_for_patterns(caller, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "import attribute/options callers must route solver construction \
         through query_boundaries::import_attributes:\n{}",
        violations.join("\n")
    );
}

#[test]
fn import_attributes_boundary_owns_import_attribute_construction_helpers() {
    let source = fs::read_to_string(checker_path(IMPORT_ATTRIBUTE_BOUNDARY))
        .expect("failed to read query_boundaries/import_attributes.rs");
    let common =
        fs::read_to_string(checker_path(COMMON_BOUNDARY)).expect("failed to read common.rs");

    for helper in [
        "import_attribute_literal_string_type",
        "import_attribute_property",
        "optional_import_option_property",
        "import_attribute_object_type",
        "import_call_options_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::import_attributes must own `{helper}`"
        );
        assert!(
            !defines_fn(&common, helper),
            "query_boundaries::common must not define `{helper}`"
        );
    }

    for construction_pattern in [
        "db.literal_string(",
        "PropertyInfo::new(",
        "PropertyInfo::opt(",
        "db.object(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::import_attributes should own `{construction_pattern}`"
        );
    }
}
