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

/// Parsed test directives
#[derive(Debug, Default, Clone)]
pub struct TestDirectives {
    /// Compiler options (strict, target, module, etc.)
    pub options: HashMap<String, String>,
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
/// let content = r#"
/// // @strict: true
/// // @target: es5
/// // @filename: file1.ts
/// function foo() {}
/// "#;
/// let parsed = parse_test_file(content)?;
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
                directives.options.insert(key_lower, value.to_string());
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
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        return Some("@noCheck");
    }

    None
}

/// Expand directives with comma-separated values into multiple option variants.
///
/// Some harness directives (e.g. module, moduleResolution) represent multiple runs.
pub fn expand_option_variants(options: &HashMap<String, String>) -> Vec<HashMap<String, String>> {
    const MULTI_VALUE_KEYS: &[&str] = &["module", "moduleresolution", "target", "jsx"];

    let mut variants = vec![options.clone()];
    for key in MULTI_VALUE_KEYS {
        let Some(value) = options.get(*key) else {
            continue;
        };
        let values: Vec<String> = value
            .split(',')
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
            .collect();
        if values.len() <= 1 {
            if let Some(v) = values.first() {
                for variant in &mut variants {
                    variant.insert((*key).to_string(), v.clone());
                }
            }
            continue;
        }

        let mut next_variants = Vec::new();
        for variant in &variants {
            for v in &values {
                let mut next = variant.clone();
                next.insert((*key).to_string(), v.clone());
                next_variants.push(next);
            }
        }
        variants = next_variants;
    }

    variants
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
                    .map(|m| matches!(m, "node16" | "node18" | "node20"))
                    .unwrap_or(true),
                Some("nodenext") => module.as_deref().map(|m| m == "nodenext").unwrap_or(true),
                _ => true,
            }
        })
        .collect()
}
