//! Test file directive parser
//!
//! Parses @ directives from TypeScript test files using regex.
//! Supports: strict, target, module, filename, jsx, lib, noLib,
//! moduleResolution, noCheck, skip, typeScriptVersion, etc.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Compiled regex for parsing @ directives
/// Matches: // @key: value
static DIRECTIVE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*//\s*@(\w+)\s*:\s*(\S+)").unwrap()
});

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
    /// Full file content
    pub content: String,
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

    // Split content into lines
    let lines: Vec<&str> = content.lines().collect();

    // Track current filename for multi-file tests
    let mut current_filename: Option<String> = None;
    let mut current_content: Vec<String> = Vec::new();

    for line in &lines {
        if let Some(cap) = DIRECTIVE_RE.captures(line) {
            let key = cap.get(1).unwrap().as_str();
            let value = cap.get(2).unwrap().as_str();

            if key.to_lowercase() == "filename" {
                // Save previous file if exists
                if let Some(filename) = current_filename.take() {
                    filenames.push((filename, current_content.join("\n")));
                }
                // Start new file
                current_filename = Some(value.to_string());
                current_content = Vec::new();
            } else {
                // Store as option
                directives.options.insert(key.to_string(), value.to_string());
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

    Ok(ParsedTest {
        directives,
        content: content.to_string(),
    })
}

/// Check if test should be skipped based on directives
pub fn should_skip_test(directives: &TestDirectives) -> Option<&'static str> {
    // Check @skip
    if directives.options.contains_key("skip") {
        return Some("@skip");
    }

    // Check @noCheck / @nocheck
    if directives.options.get("noCheck").map(|v| v == "true").unwrap_or(false)
        || directives.options.get("nocheck").map(|v| v == "true").unwrap_or(false)
    {
        return Some("@noCheck");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_directives() {
        let content = r#"
// @strict: true
// @target: es5
function foo() {}
"#;
        let parsed = parse_test_file(content).unwrap();
        assert_eq!(parsed.directives.options.get("strict"), Some(&"true".to_string()));
        assert_eq!(parsed.directives.options.get("target"), Some(&"es5".to_string()));
    }

    #[test]
    fn test_parse_multi_file() {
        let content = r#"
// @filename: file1.ts
export const x: number = 1;
// @filename: file2.ts
import { x } from './file1';
console.log(x);
"#;
        let parsed = parse_test_file(content).unwrap();
        assert_eq!(parsed.directives.filenames.len(), 2);
        assert_eq!(parsed.directives.filenames[0].0, "file1.ts");
        assert_eq!(parsed.directives.filenames[1].0, "file2.ts");
    }
}
