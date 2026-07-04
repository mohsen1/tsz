//! Reporter signature/tuple diagnostic surface construction scans.
//!
//! `error_reporter` files choose source locations, display policy, and message
//! shape. Raw diagnostic-only `CallSignature`, `ParamInfo`, `TupleElement`, and
//! `TypeParamInfo` construction belongs in `query_boundaries::diagnostics`.

use std::fs;
use std::path::{Path, PathBuf};

const ERROR_REPORTER_ROOT: &str = "src/error_reporter";
const DIAGNOSTICS_BOUNDARY: &str = "src/query_boundaries/diagnostics.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn collect_error_reporter_sources(dir: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {dir:?}: {err}")) {
        let path = entry.expect("failed to read directory entry").path();
        if path.is_dir() {
            collect_error_reporter_sources(&path, sources);
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs")
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("_tests.rs"))
        {
            sources.push(path);
        }
    }
}

fn relative_checker_path(path: &Path) -> String {
    path.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("source path should be under checker manifest dir")
        .to_string_lossy()
        .replace('\\', "/")
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

fn scan_for_patterns(path: &Path, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let relative = relative_checker_path(path);
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if has_token_pattern(line, pattern) {
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
fn reporter_signature_tuple_surfaces_route_raw_construction_through_diagnostics_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "CallSignature {",
        "CallSignature::new(",
        "ParamInfo {",
        "ParamInfo::required(",
        "ParamInfo::optional(",
        "ParamInfo::rest(",
        "ParamInfo::unnamed(",
        "TupleElement {",
        "TypeParamInfo {",
        "TypeParamInfo::simple(",
        ".type_param(",
        ".factory().type_param(",
        ".factory.type_param(",
        ".types.type_param(",
        ".ctx.types.type_param(",
    ];

    let mut sources = Vec::new();
    collect_error_reporter_sources(&checker_path(ERROR_REPORTER_ROOT), &mut sources);
    sources.sort();

    let mut violations = Vec::new();
    for source in sources {
        scan_for_patterns(&source, FORBIDDEN_PATTERNS, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "error_reporter diagnostic signature/tuple display construction must \
         route through query_boundaries::diagnostics:\n{}",
        violations.join("\n")
    );
}

#[test]
fn diagnostics_boundary_owns_reporter_signature_tuple_display_helpers() {
    let source = fs::read_to_string(checker_path(DIAGNOSTICS_BOUNDARY))
        .expect("failed to read query_boundaries/diagnostics.rs");
    for helper in [
        "call_signature_from_function_shape_for_display",
        "display_param_with_type",
        "display_tuple_element_with_type",
        "tuple_elements_with_unknown_fixed_display",
        "source_display_tuple_element",
        "instantiate_call_signature_for_display",
        "diagnostic_user_type_param",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::diagnostics must own `{helper}`"
        );
    }
}
