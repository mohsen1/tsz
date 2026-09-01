//! Test file directive parser
//!
//! Thin adapter over the canonical directive parser retained by the
//! conformance harness.
//! Supports: strict, target, module, filename, jsx, lib, noLib,
//! moduleResolution, noCheck, skip, typeScriptVersion, etc.
//!
//! The parsing semantics here are cache-anchored: the checked-in tsc
//! result cache was generated through this parser, so option keys,
//! option order, and file-content splitting must stay byte-identical
//! between the cache generator and the runner.

use std::collections::HashMap;
use std::path::Path;

use crate::tsc_results::UnsupportedReason;

pub use crate::test_directives::TestDirectives;

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
        directives: crate::test_directives::parse_test_file(content),
    })
}

/// Check if test should be skipped based on directives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestDisposition {
    Runnable,
    Unsupported(UnsupportedReason),
    Skipped(&'static str),
}

fn embedded_trace_resolution_enabled(directives: &TestDirectives) -> bool {
    // Treat every authored project config as observable. The pinned harness can
    // select a nested package config even when the local fixture mapper cannot
    // yet represent that project root. Narrowing this to the locally elected
    // config would silently erase the trace product for such rows.
    directives.filenames.iter().any(|(name, content)| {
        if !name.replace('\\', "/").ends_with("tsconfig.json") {
            return false;
        }
        crate::jsonc::parse_jsonc(content).ok().and_then(|config| {
            config
                .get("compilerOptions")
                .and_then(|options| options.get("traceResolution"))
                .and_then(serde_json::Value::as_bool)
        }) == Some(true)
    })
}

fn directive_trace_resolution_enabled(directives: &TestDirectives) -> bool {
    let Some(raw) = directives.options.get("traceresolution") else {
        return false;
    };
    let tokens = raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let excludes_true = tokens.iter().any(|token| {
        token
            .strip_prefix('-')
            .or_else(|| token.strip_prefix('!'))
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    });
    !excludes_true
        && tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("true") || *token == "*")
}

pub fn test_disposition(directives: &TestDirectives) -> TestDisposition {
    // Check @skip (keys are already lowercase)
    if directives.options.contains_key("skip") {
        return TestDisposition::Skipped("@skip");
    }

    if select_ts7_oracle_configurations(directives).is_err() {
        return TestDisposition::Unsupported(UnsupportedReason::TypeScript7Configuration);
    }

    if directive_trace_resolution_enabled(directives)
        || embedded_trace_resolution_enabled(directives)
    {
        return TestDisposition::Unsupported(UnsupportedReason::TraceResolutionOutputNotCompared);
    }

    TestDisposition::Runnable
}

pub fn test_disposition_at_path(path: &Path, directives: &TestDirectives) -> TestDisposition {
    // Checked first so the baseline stays host-agnostic — see `HOST_DIVERGENT_TESTS`.
    // A registered row wins even if it would also be TS7-unsupported.
    if let Some(reason) = host_divergent_skip_reason(path) {
        return TestDisposition::Skipped(reason);
    }
    let basename = path.file_name().and_then(|name| name.to_str());
    if basename.is_some_and(|name| TYPESCRIPT_7_SKIPPED_TESTS.contains(&name)) {
        return TestDisposition::Skipped("skipped by TypeScript 7 harness");
    }
    test_disposition(directives)
}

/// Stable reason emitted for a row excluded by the host-divergent registry.
const HOST_DIVERGENT_SKIP_REASON: &str = "host-divergent";

/// Suffix-match the forward-slash-normalized `path` against [`HOST_DIVERGENT_TESTS`],
/// returning [`HOST_DIVERGENT_SKIP_REASON`] on a hit. Suffix (not basename)
/// matching keeps look-alike siblings runnable; it is prefix-agnostic, so an
/// absolute discovery path and a repo-relative one both match.
fn host_divergent_skip_reason(path: &Path) -> Option<&'static str> {
    let normalized = crate::test_filter::normalized_path(path);
    HOST_DIVERGENT_TESTS
        .iter()
        .any(|suffix| normalized.ends_with(suffix))
        .then_some(HOST_DIVERGENT_SKIP_REASON)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedTypeScript7Configuration;

const LIST_OPTIONS: &[&str] = &[
    "lib",
    "types",
    "typeroots",
    "rootdirs",
    "modulesuffixes",
    "customconditions",
];

const HARNESS_ONLY_OPTIONS: &[&str] = &[
    "allownontsextensions",
    "baselinefile",
    "capturessuggestions",
    "currentdirectory",
    "filename",
    "fullemitpaths",
    "noimplicitreferences",
    "noerrortruncation",
    "notypesandsymbols",
    "suppressoutputpathcheck",
    "symlink",
    "link",
    "traceresolution",
    "usecasesensitivefilenames",
    "reportdiagnostics",
    "typescriptversion",
    "skip",
];

const TYPESCRIPT_7_SKIPPED_TESTS: &[&str] = &[
    "APILibCheck.ts",
    "APISample_Watch.ts",
    "APISample_WatchWithDefaults.ts",
    "APISample_WatchWithOwnWatchHost.ts",
    "APISample_compile.ts",
    "APISample_jsdoc.ts",
    "APISample_linter.ts",
    "APISample_parseConfig.ts",
    "APISample_transform.ts",
    "APISample_watcher.ts",
    "preserveUnusedImports.ts",
    "noCrashWithVerbatimModuleSyntaxAndImportsNotUsedAsValues.ts",
    "verbatimModuleSyntaxCompat.ts",
    "verbatimModuleSyntaxCompat2.ts",
    "verbatimModuleSyntaxCompat3.ts",
    "verbatimModuleSyntaxCompat4.ts",
    "preserveValueImports.ts",
    "preserveValueImports_importsNotUsedAsValues.ts",
    "preserveValueImports_errors.ts",
    "preserveValueImports_mixedImports.ts",
    "preserveValueImports_module.ts",
    "importsNotUsedAsValues_error.ts",
    "alwaysStrictNoImplicitUseStrict.ts",
    "nonPrimitiveIndexingWithForInSupressError.ts",
    "parameterInitializerBeforeDestructuringEmit.ts",
    "mappedTypeUnionConstraintInferences.ts",
    "lateBoundConstraintTypeChecksCorrectly.ts",
    "keyofDoesntContainSymbols.ts",
    "isolatedModulesOut.ts",
    "noStrictGenericChecks.ts",
    "noImplicitUseStrict_umd.ts",
    "noImplicitUseStrict_system.ts",
    "noImplicitUseStrict_es6.ts",
    "noImplicitUseStrict_commonjs.ts",
    "noImplicitUseStrict_amd.ts",
    "noImplicitAnyIndexingSuppressed.ts",
    "excessPropertyErrorsSuppressed.ts",
    "moduleNoneDynamicImport.ts",
    "moduleNoneErrors.ts",
    "moduleNoneOutFile.ts",
    "noErrorUsingImportExportModuleAugmentationInDeclarationFile1.ts",
    "noErrorUsingImportExportModuleAugmentationInDeclarationFile2.ts",
    "noErrorUsingImportExportModuleAugmentationInDeclarationFile3.ts",
    "requireOfJsonFileWithModuleEmitNone.ts",
    "requireOfJsonFileWithModuleNodeResolutionEmitNone.ts",
];

/// Conformance rows whose PASS/FAIL outcome is **host-deterministic** — stable on
/// one OS but divergent across systems because it hinges on OS-decided filesystem
/// semantics (case sensitivity, path canonicalization) rather than on `tsz`'s
/// type-checking. Excluding such a row on *every* host (a plain `Skipped`, out of
/// the runnable denominator) keeps the committed baseline host-agnostic, at the
/// cost of one diverging row of coverage.
///
/// Entries are sorted, unique `tests/cases`-relative suffixes. Add one only as a
/// last resort for a row proven host-deterministic (a stable divergence across
/// many samples, not flakiness), with the evidence recorded in the issue; a
/// divergence rooted in `tsz` itself is a bug to fix.
const HOST_DIVERGENT_TESTS: &[&str] = &[
    // Host-deterministic typings resolution: fails 16/16 on darwin-arm64,
    // passes on Linux CI, stable per host across many samples. The divergence is
    // OS filesystem case-sensitivity in the typings lookup, not a `tsz`
    // type-checking difference. See issue #17820.
    "conformance/typings/typingsLookup3.ts",
];

const TARGET_VALUES: &[&str] = &[
    "es5", "es6", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021", "es2022", "es2023",
    "es2024", "es2025", "esnext",
];
const MODULE_VALUES: &[&str] = &[
    "commonjs", "amd", "system", "umd", "es6", "es2020", "es2022", "esnext", "node16", "node18",
    "node20", "nodenext", "preserve",
];
const MODULE_RESOLUTION_VALUES: &[&str] = &["node16", "nodenext", "bundler", "classic", "node"];
const JSX_VALUES: &[&str] = &[
    "preserve",
    "react",
    "react-native",
    "react-jsx",
    "react-jsxdev",
];
const MODULE_DETECTION_VALUES: &[&str] = &["legacy", "auto", "force"];
const NEWLINE_VALUES: &[&str] = &["crlf", "lf"];

fn is_list_or_harness_option(key: &str) -> bool {
    LIST_OPTIONS.contains(&key) || HARNESS_ONLY_OPTIONS.contains(&key)
}

fn normalized_option_value(key: &str, value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    match (key, value.as_str()) {
        ("target" | "module", "es2015") => "es6".to_string(),
        ("moduleresolution", "node10") => "node".to_string(),
        _ => value,
    }
}

fn wildcard_values(key: &str) -> &'static [&'static str] {
    match key {
        "target" => TARGET_VALUES,
        "module" => MODULE_VALUES,
        "moduleresolution" => MODULE_RESOLUTION_VALUES,
        "jsx" => JSX_VALUES,
        "moduledetection" => MODULE_DETECTION_VALUES,
        "newline" => NEWLINE_VALUES,
        _ => &["true", "false"],
    }
}

fn is_enum_option(key: &str) -> bool {
    matches!(
        key,
        "target" | "module" | "moduleresolution" | "jsx" | "moduledetection" | "newline"
    )
}

fn is_varying_option(key: &str, raw: &str) -> bool {
    if is_enum_option(key) {
        return true;
    }
    raw.split(',').map(str::trim).all(|value| {
        value.is_empty()
            || value == "*"
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("false")
            || value
                .strip_prefix('-')
                .or_else(|| value.strip_prefix('!'))
                .is_some_and(|value| {
                    matches!(value.to_ascii_lowercase().as_str(), "true" | "false")
                })
    })
}

fn split_option_values(key: &str, raw: &str) -> Vec<String> {
    let tokens: Vec<_> = raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let exclusions: std::collections::HashSet<String> = tokens
        .iter()
        .filter_map(|token| token.strip_prefix('-').or_else(|| token.strip_prefix('!')))
        .map(|token| normalized_option_value(key, token))
        .collect();
    let has_wildcard = tokens.contains(&"*");
    let mut values: Vec<String> = tokens
        .iter()
        .filter(|token| **token != "*" && !token.starts_with('-') && !token.starts_with('!'))
        .map(|token| (*token).to_string())
        .collect();
    if has_wildcard {
        values.extend(
            wildcard_values(key)
                .iter()
                .map(|value| (*value).to_string()),
        );
    }

    let mut seen = std::collections::HashSet::new();
    values.retain(|value| {
        let normalized = normalized_option_value(key, value);
        let known = if is_enum_option(key) {
            wildcard_values(key)
                .iter()
                .any(|known| normalized_option_value(key, known) == normalized)
        } else {
            matches!(normalized.as_str(), "true" | "false")
        };
        known && !exclusions.contains(&normalized) && seen.insert(normalized)
    });
    values
}

fn option_value_supported(key: &str, value: &str) -> bool {
    let value = normalized_option_value(key, value);
    match key {
        "target" => value != "es3" && value != "es5",
        "module" => !matches!(value.as_str(), "amd" | "umd" | "system" | "none"),
        "moduleresolution" => !matches!(value.as_str(), "classic" | "node"),
        "esmoduleinterop" | "allowsyntheticdefaultimports" | "alwaysstrict" => value != "false",
        "baseurl" | "outfile" => value.is_empty(),
        _ => true,
    }
}

fn ordered_option_keys(directives: &TestDirectives) -> Vec<String> {
    let mut keys = directives.option_order.clone();
    let mut remaining: Vec<_> = directives
        .options
        .keys()
        .filter(|key| !keys.contains(key))
        .cloned()
        .collect();
    remaining.sort();
    keys.extend(remaining);
    keys
}

/// Expand the compiler configurations that TypeScript 7's native harness
/// would run, then drop configurations rejected by its unsupported-option
/// policy. Cache generation and execution must consume this same set.
pub fn select_ts7_oracle_configurations(
    directives: &TestDirectives,
) -> Result<Vec<HashMap<String, String>>, UnsupportedTypeScript7Configuration> {
    let keys = ordered_option_keys(directives);
    let mut configurations = vec![HashMap::new()];
    for key in keys {
        let Some(raw) = directives.options.get(&key) else {
            continue;
        };
        let values = if is_list_or_harness_option(&key) || !is_varying_option(&key, raw) {
            vec![raw.clone()]
        } else {
            split_option_values(&key, raw)
        };
        if values.is_empty() {
            if raw.trim().is_empty() {
                continue;
            }
            return Err(UnsupportedTypeScript7Configuration);
        }
        let mut next = Vec::new();
        for configuration in &configurations {
            for value in &values {
                let mut candidate = configuration.clone();
                candidate.insert(key.clone(), value.clone());
                next.push(candidate);
            }
        }
        if next.len() > 25 {
            return Err(UnsupportedTypeScript7Configuration);
        }
        configurations = next;
    }

    configurations.retain(|options| {
        options.iter().all(|(key, value)| {
            is_list_or_harness_option(key) || option_value_supported(key, value)
        })
    });
    if configurations.is_empty() {
        Err(UnsupportedTypeScript7Configuration)
    } else {
        Ok(configurations)
    }
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
    fn selector_skips_removed_target_and_normalizes_boolean_variants() {
        let directives = TestDirectives {
            options: HashMap::from([
                ("target".to_string(), "es5, es2015".to_string()),
                ("esmoduleinterop".to_string(), "false, true".to_string()),
            ]),
            option_order: vec!["target".to_string(), "esmoduleinterop".to_string()],
            filenames: Vec::new(),
        };

        let selected = select_ts7_oracle_configurations(&directives).expect("valid variants");
        assert_eq!(selected.len(), 1);
        let selected = &selected[0];
        assert_eq!(selected.get("target").map(String::as_str), Some("es2015"));
        assert_eq!(
            selected.get("esmoduleinterop").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn selector_expands_wildcard_exclusions_in_source_order() {
        let directives = TestDirectives {
            options: HashMap::from([("target".to_string(), "*, -es3".to_string())]),
            option_order: vec!["target".to_string()],
            filenames: Vec::new(),
        };
        let selected = select_ts7_oracle_configurations(&directives).expect("valid variants");
        let selected = &selected[0];
        assert_eq!(selected.get("target").map(String::as_str), Some("es6"));
    }

    #[test]
    fn selector_keeps_every_supported_resolution_variant() {
        let directives = TestDirectives {
            options: HashMap::from([(
                "moduleresolution".to_string(),
                "classic, node16, nodenext, bundler".to_string(),
            )]),
            option_order: vec!["moduleresolution".to_string()],
            filenames: Vec::new(),
        };
        let selected = select_ts7_oracle_configurations(&directives).expect("valid variants");
        assert_eq!(
            selected
                .iter()
                .filter_map(|options| options.get("moduleresolution").map(String::as_str))
                .collect::<Vec<_>>(),
            vec!["node16", "nodenext", "bundler"]
        );
    }

    #[test]
    fn selector_rejects_removed_only_configuration() {
        let directives = TestDirectives {
            options: HashMap::from([("target".to_string(), "es5".to_string())]),
            option_order: vec!["target".to_string()],
            filenames: Vec::new(),
        };
        assert_eq!(
            select_ts7_oracle_configurations(&directives),
            Err(UnsupportedTypeScript7Configuration)
        );
        assert_eq!(
            test_disposition(&directives),
            TestDisposition::Unsupported(UnsupportedReason::TypeScript7Configuration)
        );
    }

    #[test]
    fn selector_leaves_virtual_config_diagnostics_to_the_compiler() {
        let directives = TestDirectives {
            options: HashMap::new(),
            option_order: Vec::new(),
            filenames: vec![(
                "tsconfig.base.json".to_string(),
                r#"{"compilerOptions":{"baseUrl":"."}}"#.to_string(),
            )],
        };
        assert_eq!(
            select_ts7_oracle_configurations(&directives)
                .expect("embedded config is an oracle input")
                .len(),
            1
        );
    }

    #[test]
    fn selector_keeps_incompatible_pairs_as_diagnostic_witnesses() {
        let directives = TestDirectives {
            options: HashMap::from([
                ("module".to_string(), "node16, esnext".to_string()),
                ("moduleresolution".to_string(), "bundler".to_string()),
            ]),
            option_order: vec!["module".to_string(), "moduleresolution".to_string()],
            filenames: Vec::new(),
        };
        let selected = select_ts7_oracle_configurations(&directives).expect("valid variants");
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected[0].get("module").map(String::as_str),
            Some("node16")
        );
        assert_eq!(
            selected[1].get("module").map(String::as_str),
            Some("esnext")
        );
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
    fn test_disposition_honors_explicit_and_ts7_unsupported_skips() {
        let mut directives = TestDirectives::default();
        directives
            .options
            .insert("nocheck".to_string(), "true".to_string());
        assert_eq!(test_disposition(&directives), TestDisposition::Runnable);

        directives
            .options
            .insert("skip".to_string(), "true".to_string());
        assert_eq!(
            test_disposition(&directives),
            TestDisposition::Skipped("@skip")
        );
    }

    #[test]
    fn trace_resolution_products_are_visible_but_not_diagnostic_claims() {
        let direct =
            parse_test_file("// @traceResolution: true\n// @filename: input.ts\nimport 'pkg';\n")
                .expect("direct trace fixture");
        assert_eq!(
            test_disposition(&direct.directives),
            TestDisposition::Unsupported(UnsupportedReason::TraceResolutionOutputNotCompared)
        );

        let embedded = parse_test_file(
            "// @filename: tsconfig.json\n{\"compilerOptions\":{\"traceResolution\":true}}\n// @filename: input.ts\nimport 'pkg';\n",
        )
        .expect("embedded trace fixture");
        assert_eq!(
            test_disposition(&embedded.directives),
            TestDisposition::Unsupported(UnsupportedReason::TraceResolutionOutputNotCompared)
        );

        let embedded_jsonc = parse_test_file(
            "// @filename: tsconfig.json\n{\n // comment\n \"compilerOptions\": { \"traceResolution\": true, },\n}\n// @filename: input.ts\nimport 'pkg';\n",
        )
        .unwrap();
        assert_eq!(
            test_disposition(&embedded_jsonc.directives),
            TestDisposition::Unsupported(UnsupportedReason::TraceResolutionOutputNotCompared)
        );

        for source in [
            "// @traceResolution: false\nconst value = 1;\n",
            "// @traceResolution: *, -true\nconst value = 1;\n",
            "// @traceResolution: *, -TRUE\nconst value = 1;\n",
            "// @filename: tsconfig.json\n{\"compilerOptions\":{\"traceResolution\":false}}\n// @filename: input.ts\nconst value = 1;\n",
            "// @filename: tsconfig.json\n{\"compilerOptions\":{\"traceResolution\":\"true\"}}\n// @filename: input.ts\nconst value = 1;\n",
        ] {
            let parsed = parse_test_file(source).expect("non-tracing fixture");
            assert_eq!(
                test_disposition(&parsed.directives),
                TestDisposition::Runnable
            );
        }
    }

    #[test]
    fn explicit_skip_and_ts7_configuration_precede_trace_nonclaim() {
        let skipped =
            parse_test_file("// @skip: tracked\n// @traceResolution: true\nconst value = 1;\n")
                .expect("skipped trace fixture");
        assert_eq!(
            test_disposition(&skipped.directives),
            TestDisposition::Skipped("@skip")
        );

        let removed =
            parse_test_file("// @target: es5\n// @traceResolution: true\nconst value = 1;\n")
                .expect("removed target fixture");
        assert_eq!(
            test_disposition(&removed.directives),
            TestDisposition::Unsupported(UnsupportedReason::TypeScript7Configuration)
        );
    }

    #[test]
    fn path_skip_matches_the_native_ts7_runner_registry() {
        let directives = TestDirectives::default();
        assert_eq!(
            test_disposition_at_path(Path::new("compiler/preserveValueImports.ts"), &directives),
            TestDisposition::Skipped("skipped by TypeScript 7 harness")
        );
        assert_eq!(
            test_disposition_at_path(Path::new("compiler/ordinary.ts"), &directives),
            TestDisposition::Runnable
        );
    }

    #[test]
    fn host_divergent_registry_skips_only_registered_rows() {
        let directives = TestDirectives::default();
        // Repo-relative, absolute POSIX, and Windows-separator spellings of a
        // registered row all resolve to the skip; basename-adjacent siblings that
        // are not registered stay runnable (suffix matching must not sweep in
        // look-alikes).
        let cases = [
            (
                "TypeScript/tests/cases/conformance/typings/typingsLookup3.ts",
                TestDisposition::Skipped(HOST_DIVERGENT_SKIP_REASON),
            ),
            (
                "/home/runner/tsz/TypeScript/tests/cases/conformance/typings/typingsLookup3.ts",
                TestDisposition::Skipped(HOST_DIVERGENT_SKIP_REASON),
            ),
            (
                r"C:\src\TypeScript\tests\cases\conformance\typings\typingsLookup3.ts",
                TestDisposition::Skipped(HOST_DIVERGENT_SKIP_REASON),
            ),
            (
                "TypeScript/tests/cases/conformance/typings/typingsLookup30.ts",
                TestDisposition::Runnable,
            ),
            (
                "TypeScript/tests/cases/conformance/typings/typingsLookup.ts",
                TestDisposition::Runnable,
            ),
        ];
        for (spelling, expected) in cases {
            assert_eq!(
                test_disposition_at_path(Path::new(spelling), &directives),
                expected,
                "unexpected disposition for {spelling}"
            );
        }
    }

    #[test]
    fn host_divergent_registry_entries_are_canonical_and_sorted() {
        for entry in HOST_DIVERGENT_TESTS {
            assert!(!entry.contains('\\'), "{entry:?} must use forward slashes");
            assert!(
                entry.contains('/'),
                "{entry:?} must include its directory prefix so basenames cannot collide"
            );
        }
        let mut sorted = HOST_DIVERGENT_TESTS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.as_slice(),
            HOST_DIVERGENT_TESTS,
            "HOST_DIVERGENT_TESTS must stay sorted and free of duplicates"
        );
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
