//! Signature-builder surface construction scans.
//!
//! `checkers/signature_builder.rs` walks syntax and manages checker scopes.
//! Raw solver signature, parameter, type-parameter, and predicate records belong
//! in `query_boundaries::signature_building`.

use std::fs;
use std::path::{Path, PathBuf};

const SIGNATURE_BUILDER: &str = "src/checkers/signature_builder.rs";
const SIGNATURE_BOUNDARY: &str = "src/query_boundaries/signature_building.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

const fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn has_token_pattern(line: &str, pattern: &str) -> bool {
    line.match_indices(pattern).any(|(index, _)| {
        line[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_identifier_char(ch))
    })
}

fn allowed_signature_return_type(line: &str, pattern: &str) -> bool {
    pattern == "CallSignature {"
        && (line.contains("-> CallSignature {") || line.contains("-> tsz_solver::CallSignature {"))
}

fn scan_for_patterns(path: &Path, patterns: &[&str]) -> Vec<String> {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if has_token_pattern(line, pattern) && !allowed_signature_return_type(line, pattern) {
                violations.push(format!(
                    "{}:{} contains `{pattern}`",
                    SIGNATURE_BUILDER,
                    line_index + 1
                ));
            }
        }
    }
    violations
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn signature_builder_routes_raw_signature_surfaces_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "CallSignature {",
        "CallSignature::new(",
        "ParamInfo {",
        "ParamInfo::required(",
        "ParamInfo::optional(",
        "ParamInfo::rest(",
        "ParamInfo::unnamed(",
        "TypeParamInfo {",
        "TypeParamInfo::simple(",
        "TypePredicate {",
        ".type_param(",
    ];

    let violations = scan_for_patterns(&checker_path(SIGNATURE_BUILDER), FORBIDDEN_PATTERNS);
    assert!(
        violations.is_empty(),
        "signature_builder raw solver surface construction must route through \
         query_boundaries::signature_building:\n{}",
        violations.join("\n")
    );
}

#[test]
fn signature_building_boundary_owns_signature_surface_helpers() {
    let source = fs::read_to_string(checker_path(SIGNATURE_BOUNDARY))
        .expect("failed to read query_boundaries/signature_building.rs");
    for helper in [
        "user_type_param_info",
        "user_type_param",
        "param_info",
        "call_signature",
        "type_predicate",
        "instantiate_signature",
        "partially_instantiate_signature",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::signature_building must own `{helper}`"
        );
    }
}
