//! Pattern Matching tests for `module_resolver`.
//!
//! Tests for the pattern-matching primitives that drive
//! `package.json#exports` and `#imports` resolution:
//!
//! - `match_export_pattern` / `match_imports_pattern` (exact + wildcard)
//! - `parse_types_versions_pattern` (TypeScript `typesVersions` selector;
//!   the resolver inlines the matching loop, so the unit-test surface lives
//!   in `resolution::helpers` next to the parser itself)
//! - `apply_wildcard_substitution` / `substitute_wildcard_in_exports`

use crate::module_resolver_helpers::*;

#[test]
fn test_match_export_pattern_exact() {
    assert_eq!(match_export_pattern("./lib", "./lib"), Some(String::new()));
    assert_eq!(match_export_pattern("./lib", "./src"), None);
}

#[test]
fn test_match_export_pattern_wildcard() {
    assert_eq!(
        match_export_pattern("./*", "./foo"),
        Some("foo".to_string())
    );
    assert_eq!(
        match_export_pattern("./lib/*", "./lib/utils"),
        Some("utils".to_string())
    );
    assert_eq!(match_export_pattern("./lib/*", "./src/utils"), None);
}

#[test]
fn test_match_export_pattern_directory() {
    // "./" pattern matches any subpath starting with "./"
    assert_eq!(
        match_export_pattern("./", "./index.js"),
        Some("index.js".to_string())
    );
    assert_eq!(
        match_export_pattern("./", "./other"),
        Some("other".to_string())
    );
    // Exact match still works
    assert_eq!(match_export_pattern("./", "./"), Some(String::new()));
    // Non-matching subpaths
    assert_eq!(match_export_pattern("./lib/", "./src/utils"), None);
}

#[test]
fn test_match_imports_pattern_exact() {
    assert_eq!(
        match_imports_pattern("#utils", "#utils"),
        Some(String::new())
    );
    assert_eq!(match_imports_pattern("#utils", "#other"), None);
}

#[test]
fn test_match_imports_pattern_wildcard() {
    assert_eq!(
        match_imports_pattern("#utils/*", "#utils/foo"),
        Some("foo".to_string())
    );
    assert_eq!(
        match_imports_pattern("#internal/*", "#internal/helpers/bar"),
        Some("helpers/bar".to_string())
    );
    assert_eq!(match_imports_pattern("#utils/*", "#other/foo"), None);
}

#[test]
fn test_apply_wildcard_substitution() {
    // `*`-pattern key (is_directory_match = false): replace `*` in target.
    assert_eq!(
        apply_wildcard_substitution("./lib/*.js", "utils", false),
        "./lib/utils.js"
    );
    // No `*` and not a directory match: target unchanged.
    assert_eq!(
        apply_wildcard_substitution("./dist/index.js", "ignored", false),
        "./dist/index.js"
    );
}

#[test]
fn test_substitute_wildcard_in_exports_string() {
    let value = PackageExports::String("./*.cjs".to_string());
    let result = substitute_wildcard_in_exports(&value, "index", false);
    assert!(matches!(result, PackageExports::String(s) if s == "./index.cjs"));
}

#[test]
fn test_substitute_wildcard_in_exports_conditional() {
    let value = PackageExports::Conditional(vec![
        (
            "import".to_string(),
            PackageExports::String("./*.mjs".to_string()),
        ),
        (
            "default".to_string(),
            PackageExports::String("./*.cjs".to_string()),
        ),
    ]);
    let result = substitute_wildcard_in_exports(&value, "foo", false);
    match result {
        PackageExports::Conditional(entries) => {
            assert_eq!(entries.len(), 2);
            assert!(matches!(&entries[0].1, PackageExports::String(s) if s == "./foo.mjs"));
            assert!(matches!(&entries[1].1, PackageExports::String(s) if s == "./foo.cjs"));
        }
        _ => panic!("Expected Conditional"),
    }
}

#[test]
fn test_substitute_wildcard_in_exports_no_wildcard() {
    let value = PackageExports::String("./index.js".to_string());
    let result = substitute_wildcard_in_exports(&value, "anything", false);
    assert!(matches!(result, PackageExports::String(s) if s == "./index.js"));
}

#[test]
fn test_substitute_wildcard_in_exports_directory_target() {
    // Directory match: "./" target with "./index.js" wildcard → "./index.js".
    let value = PackageExports::String("./".to_string());
    let result = substitute_wildcard_in_exports(&value, "index.js", true);
    assert!(matches!(result, PackageExports::String(s) if s == "./index.js"));
}

#[test]
fn test_substitute_wildcard_in_exports_directory_empty_wildcard() {
    // Directory match with empty wildcard preserves the trailing slash.
    let value = PackageExports::String("./".to_string());
    let result = substitute_wildcard_in_exports(&value, "", true);
    assert!(matches!(result, PackageExports::String(s) if s == "./"));
}

#[test]
fn test_apply_wildcard_substitution_directory_target() {
    // Directory match: target ending in `/` gets the wildcard appended.
    assert_eq!(
        apply_wildcard_substitution("./lib/", "utils", true),
        "./lib/utils"
    );
}

#[test]
fn test_apply_wildcard_substitution_star_pattern_with_dir_target() {
    // `*`-pattern key (is_directory_match = false) mapping to a `/`-ending
    // target without `*`: target must remain unchanged. Without this, a
    // package like `{ \"./*\": { \"types\": \"./types/\" } }` would resolve
    // `pkg/foo` to `./types/foo` instead of `./types/`, diverging from
    // Node.js (Devin review on #1915).
    assert_eq!(
        apply_wildcard_substitution("./types/", "foo", false),
        "./types/"
    );
}

#[test]
fn test_export_pattern_specificity_prefix_and_suffix() {
    // Primary sort: longer prefix wins.
    assert!(export_pattern_specificity("./abc/*") > export_pattern_specificity("./*"));
    // Secondary sort: equal prefix, longer suffix wins.
    assert!(export_pattern_specificity("./*-b") > export_pattern_specificity("./*"));
    // Equal specificity.
    assert_eq!(
        export_pattern_specificity("./*-a"),
        export_pattern_specificity("./*-b")
    );
    // `(base_length, is_pattern, total_length)` per Node `PATTERN_KEY_COMPARE`.
    // No wildcard: base = full length, not a pattern.
    assert_eq!(export_pattern_specificity("./lib"), (5, 0, 5));
    // Wildcard: base = indexOf('*') + 1, flagged as a pattern.
    assert_eq!(export_pattern_specificity("./lib/*"), (7, 1, 7));
    assert_eq!(export_pattern_specificity("./*-suffix"), (3, 1, 10));
    // Longer base beats longer suffix.
    // "./abc/*"   → base 7
    // "./*-suffix" → base 3
    assert!(export_pattern_specificity("./abc/*") > export_pattern_specificity("./*-suffix"));
    // At equal base length a wildcard key outranks a directory/exact key, so
    // `"./*"` (base 3) beats `"./"` (base 2) — and even ties on base are broken
    // toward the pattern: `"./x"` exact (base 3, not a pattern) loses to a
    // base-3 wildcard.
    assert!(export_pattern_specificity("./*") > export_pattern_specificity("./"));
    assert!(export_pattern_specificity("./*") > export_pattern_specificity("./x"));
}
