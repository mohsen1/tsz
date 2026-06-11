//! Test file directive parser
//!
//! Thin adapter over the canonical directive parser in
//! `tsz_common::test_directives` (the single grammar shared by the
//! conformance, emit, fourslash, and checker test-harness paths).
//! Supports: strict, target, module, filename, jsx, lib, noLib,
//! moduleResolution, noCheck, skip, typeScriptVersion, etc.
//!
//! The parsing semantics here are cache-anchored: the checked-in tsc
//! result cache was generated through this parser, so option keys,
//! option order, and file-content splitting must stay byte-identical
//! between the cache generator and the runner.

use std::collections::HashMap;

pub use tsz_common::test_directives::TestDirectives;

/// Result of parsing a test file
#[derive(Debug, Clone)]
pub struct ParsedTest {
    /// Parsed directives
    pub directives: TestDirectives,
}

/// Parse @ directives from test file content
///
/// # Example
/// ```
/// use tsz_conformance::test_parser::parse_test_file;
/// let content = r#"
/// // @strict: true
/// // @target: es5
/// // @filename: file1.ts
/// function foo() {}
/// "#;
/// let parsed = parse_test_file(content).unwrap();
/// assert_eq!(parsed.directives.options.get("strict"), Some(&"true".to_string()));
/// ```
pub fn parse_test_file(content: &str) -> anyhow::Result<ParsedTest> {
    Ok(ParsedTest {
        directives: tsz_common::test_directives::parse_test_file(content),
    })
}

/// Check if test should be skipped based on directives
pub fn should_skip_test(directives: &TestDirectives) -> Option<&'static str> {
    // Check @skip (keys are already lowercase)
    if directives.options.contains_key("skip") {
        return Some("@skip");
    }

    None
}

/// Expand directives with comma-separated values into multiple option variants.
///
/// Currently returns a single variant using the first comma-separated value
/// for each non-list option.  This matches the cache generator behavior
/// (generate-tsc-cache.rs), which also takes only the first value via
/// `convert_options_to_tsconfig`.
///
/// Previously, "module", "moduleresolution", and "jsx" were expanded into
/// separate variants, but the cache generator was never updated to do the
/// same.  This caused false-positive diagnostics (e.g. TS5107 for
/// module=System, TS5095 for moduleResolution=bundler) because the runner
/// produced diagnostics from non-first variants that had no cache counterpart.
pub fn expand_option_variants(options: &HashMap<String, String>) -> Vec<HashMap<String, String>> {
    // The shared tsconfig converter takes only the first comma-separated value
    // for all non-list options. The runner must do the same to produce matching
    // diagnostic sets.
    //
    // Boolean options like "alwaysstrict" and "nolib" are also NOT expanded:
    // the cache generator passes the raw multi-value string (e.g. "true, false")
    // to convert_options_to_tsconfig, which takes the first comma-separated
    // value as a JSON string (not bool).  tsc then emits TS5024 for the
    // non-boolean value.  Expanding them here would convert each value to a
    // JSON bool, suppressing the TS5024 that the cache expects.
    vec![options.clone()]
}

/// Filter out option variants that are incompatible with moduleResolution rules.
///
/// Specifically, node16/nodenext moduleResolution requires module to match.
pub fn filter_incompatible_module_resolution_variants(
    variants: Vec<HashMap<String, String>>,
) -> Vec<HashMap<String, String>> {
    fn normalize_value(value: &str) -> String {
        value.trim().to_lowercase()
    }

    variants
        .into_iter()
        .filter(|options| {
            let module_resolution = options.get("moduleresolution").map(|v| normalize_value(v));
            let module = options.get("module").map(|v| normalize_value(v));

            match module_resolution.as_deref() {
                Some("node16") => module
                    .as_deref()
                    .is_none_or(|m| matches!(m, "node16" | "node18" | "node20")),
                Some("nodenext") => module.as_deref().is_none_or(|m| m == "nodenext"),
                // `bundler` requires `preserve`, `commonjs`, or ES2015+ — filter out
                // incompatible module values that would produce TS5095 errors the
                // cache never saw (the cache generator only tests the first
                // comma-separated value).
                Some("bundler") => module.as_deref().is_none_or(|m| {
                    matches!(
                        m,
                        "preserve"
                            | "commonjs"
                            | "es2015"
                            | "es6"
                            | "es2020"
                            | "es2022"
                            | "esnext"
                            | "node16"
                            | "node18"
                            | "node20"
                            | "nodenext"
                    )
                }),
                _ => true,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ts_check_directive() {
        let content = "// @ts-check\nconst x: any = 1;";
        let parsed = parse_test_file(content).unwrap();
        assert_eq!(
            parsed.directives.options.get("checkjs"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_parse_ts_nocheck_directive() {
        let content = "// @ts-nocheck\nconst x = 1;";
        let parsed = parse_test_file(content).unwrap();
        assert_eq!(
            parsed.directives.options.get("checkjs"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn test_parse_bom_prefixed_first_line_directive() {
        // JavaScript's \s matches BOM (U+FEFF) but a byte-level scan does
        // not, so the BOM must be stripped before the first-line directive
        // is recognized — otherwise hashes mismatch with the Node.js cache
        // generator.
        let content = "\u{FEFF}// @strict: true\nconst x = 1;";
        let parsed = parse_test_file(content).unwrap();
        assert_eq!(
            parsed.directives.options.get("strict"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_expand_option_variants_does_not_split_nolib() {
        let mut options = HashMap::new();
        options.insert("nolib".to_string(), "true,false".to_string());
        options.insert("module".to_string(), "esnext,commonjs".to_string());

        let variants = expand_option_variants(&options);

        assert_eq!(variants.len(), 1);
        assert!(variants
            .iter()
            .all(|v| v.get("nolib") == Some(&"true,false".to_string())));
    }

    #[test]
    fn test_parse_preserves_first_option_order_and_last_value() {
        let content = r#"
// @strict: true
// @target: es5
// @strict: false
// @ts-check
// @ts-nocheck
function foo() {}
"#;
        let parsed = parse_test_file(content).unwrap();
        assert_eq!(
            parsed.directives.option_order,
            vec![
                "strict".to_string(),
                "target".to_string(),
                "checkjs".to_string()
            ]
        );
        assert_eq!(
            parsed.directives.options.get("strict"),
            Some(&"false".to_string())
        );
        assert_eq!(
            parsed.directives.options.get("checkjs"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn test_should_skip_test_only_honors_skip() {
        let mut directives = TestDirectives::default();
        directives
            .options
            .insert("nocheck".to_string(), "true".to_string());
        assert_eq!(should_skip_test(&directives), None);

        directives
            .options
            .insert("skip".to_string(), "true".to_string());
        assert_eq!(should_skip_test(&directives), Some("@skip"));
    }

    #[test]
    fn test_filter_incompatible_module_resolution_variants_rejects_bundler_mismatch() {
        let mut accepted = HashMap::new();
        accepted.insert("moduleresolution".to_string(), " bundler ".to_string());
        accepted.insert("module".to_string(), " es2022 ".to_string());

        let mut rejected = HashMap::new();
        rejected.insert("moduleresolution".to_string(), "bundler".to_string());
        rejected.insert("module".to_string(), "node10".to_string());

        let filtered =
            filter_incompatible_module_resolution_variants(vec![accepted.clone(), rejected]);

        assert_eq!(filtered, vec![accepted]);
    }

    #[test]
    fn test_multi_file_split_matches_canonical_casing() {
        // The wrapper-side recognizers (symlink/link association,
        // materialization) must agree with this splitter; both now go
        // through the canonical parser, so any key casing splits files.
        let content =
            "// @Filename: a.ts\nexport const a = 1;\n// @FILENAME: b.ts\nexport const b = 2;\n";
        let parsed = parse_test_file(content).unwrap();
        let names: Vec<&str> = parsed
            .directives
            .filenames
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, vec!["a.ts", "b.ts"]);
    }
}
