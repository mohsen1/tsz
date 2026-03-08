//! Test file directive parser
//!
//! Parses @ directives from TypeScript test files using regex.
//! Supports: strict, target, module, filename, jsx, lib, noLib,
//! moduleResolution, noCheck, skip, typeScriptVersion, etc.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Compiled regex for parsing @ directives
/// Matches: // @key: value (captures entire rest of line as value)
static DIRECTIVE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*//\s*@(\w+)\s*:\s*([^\r\n]*)").unwrap());
static TS_DIRECTIVE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*//\s*@([\w-]+)\s*$").unwrap());

/// Parsed test directives
#[derive(Debug, Default, Clone)]
pub struct TestDirectives {
    /// Compiler options (strict, target, module, etc.)
    pub options: HashMap<String, String>,
    /// Key insertion order (preserves directive declaration order from the test file).
    /// Used to generate tsconfig.json with keys in the same order as the cache generator.
    pub option_order: Vec<String>,
    /// Additional files defined by @filename directives
    pub filenames: Vec<(String, String)>, // (filename, content)
}

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
    let mut directives = TestDirectives::default();
    let mut filenames = Vec::new();

    // Strip UTF-8 BOM if present — JavaScript's \s matches BOM (U+FEFF) but Rust's
    // regex \s does not, so without stripping, the first-line directive after a BOM
    // would be missed, causing hash mismatches with the Node.js cache generator.
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    // Split content into lines
    let lines: Vec<&str> = content.lines().collect();

    // Track current filename for multi-file tests
    let mut current_filename: Option<String> = None;
    let mut current_content: Vec<String> = Vec::new();

    for line in &lines {
        if let Some(cap) = DIRECTIVE_RE.captures(line) {
            let key = cap.get(1).unwrap().as_str();
            let value = cap.get(2).unwrap().as_str().trim();

            // Normalize key to lowercase for case-insensitive matching
            let key_lower = key.to_lowercase();

            if key_lower == "filename" {
                // Save previous file if exists
                if let Some(filename) = current_filename.take() {
                    filenames.push((filename, current_content.join("\n")));
                }
                // Start new file
                current_filename = Some(value.to_string());
                current_content = Vec::new();
            } else {
                if !directives.options.contains_key(&key_lower) {
                    directives.option_order.push(key_lower.clone());
                }
                directives.options.insert(key_lower, value.to_string());
            }
        } else if let Some(cap) = TS_DIRECTIVE_RE.captures(line) {
            let key = cap.get(1).unwrap().as_str();
            let key_lower = key.to_lowercase();

            let (mapped_key, value) = match key_lower.as_str() {
                "ts-check" => ("checkjs", "true"),
                "ts-nocheck" => ("checkjs", "false"),
                _ => continue,
            };

            if !directives.options.contains_key(mapped_key) {
                directives.option_order.push(mapped_key.to_string());
            }
            directives
                .options
                .insert(mapped_key.to_string(), value.to_string());
            if current_filename.is_some() {
                current_content.push(line.to_string());
            }
        } else {
            // Non-directive line - add to current file content
            current_content.push(line.to_string());
        }
    }

    // Don't forget the last file
    if let Some(filename) = current_filename {
        filenames.push((filename, current_content.join("\n")));
    }

    directives.filenames = filenames;

    Ok(ParsedTest { directives })
}

/// Check if test should be skipped based on directives
pub fn should_skip_test(directives: &TestDirectives) -> Option<&'static str> {
    // Check @skip (keys are already lowercase)
    if directives.options.contains_key("skip") {
        return Some("@skip");
    }

    // Check @noCheck (keys are already lowercase → "nocheck")
    if directives
        .options
        .get("nocheck")
        .is_some_and(|v| v == "true")
    {
        return Some("@noCheck");
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
    // The cache generator takes only the first comma-separated value for all
    // non-list options (see convert_options_to_tsconfig line 628).  The runner
    // must do the same to produce matching diagnostic sets.
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
}
