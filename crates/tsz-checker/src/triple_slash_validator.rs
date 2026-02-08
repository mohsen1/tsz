//! Triple-slash reference directive validation
//!
//! Validates /// <reference path="..." /> directives in TypeScript files.

use std::path::Path;

/// Extract triple-slash reference paths from source text
///
/// Returns a vector of (path, line_number) tuples for each reference directive found.
pub fn extract_reference_paths(source: &str) -> Vec<(String, usize)> {
    let mut references = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Check if line starts with ///
        if !trimmed.starts_with("///") {
            continue;
        }

        // Check if it contains <reference path=
        if !trimmed.contains("<reference") || !trimmed.contains("path=") {
            continue;
        }

        // Extract the path value between quotes
        if let Some(path) = extract_quoted_path(trimmed) {
            references.push((path, line_num));
        }
    }

    references
}

/// Extract path from a reference directive line
fn extract_quoted_path(line: &str) -> Option<String> {
    // Find "path" followed by optional whitespace, then "="
    let path_idx = line.find("path")?;
    let after_path = &line[path_idx + 4..];

    // Find the equals sign (may have whitespace before it)
    let eq_idx = after_path.find('=')?;
    let after_equals = &after_path[eq_idx + 1..];

    // Find opening quote (skip whitespace)
    let first_char = after_equals.trim_start().chars().next()?;
    if first_char != '"' && first_char != '\'' {
        return None;
    }

    let quote_char = first_char;
    let after_open_quote = &after_equals[after_equals.find(quote_char)? + 1..];

    // Find closing quote
    let end_pos = after_open_quote.find(quote_char)?;
    Some(after_open_quote[..end_pos].to_string())
}

/// Check if a referenced file exists relative to the source file
///
/// Returns true if the file exists, false otherwise.
/// Follows TypeScript's resolution strategy:
/// 1. Try exact path first
/// 2. If no extension or not found, try .ts, .tsx, .d.ts extensions
pub fn validate_reference_path(source_file: &Path, reference_path: &str) -> bool {
    if let Some(parent) = source_file.parent() {
        let base_path = parent.join(reference_path);

        // Try exact path first
        if base_path.exists() {
            return true;
        }

        // If the path already has an extension, don't try others
        if reference_path.contains('.') {
            return false;
        }

        // Try TypeScript extensions in order: .ts, .tsx, .d.ts
        let extensions = [".ts", ".tsx", ".d.ts"];
        for ext in &extensions {
            let path_with_ext = parent.join(format!("{}{}", reference_path, ext));
            if path_with_ext.exists() {
                return true;
            }
        }

        false
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_reference_paths() {
        let source = r#"
/// <reference path="./types.d.ts" />
/// <reference path='./other.ts' />
const x = 1;
"#;
        let refs = extract_reference_paths(source);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "./types.d.ts");
        assert_eq!(refs[1].0, "./other.ts");
    }

    #[test]
    fn test_extract_no_references() {
        let source = "const x = 1;\n// regular comment\n";
        let refs = extract_reference_paths(source);
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_extract_quoted_path() {
        assert_eq!(
            extract_quoted_path(r#"path="./file.ts""#),
            Some("./file.ts".to_string())
        );
        assert_eq!(
            extract_quoted_path(r#"path='./file.ts'"#),
            Some("./file.ts".to_string())
        );
        assert_eq!(
            extract_quoted_path(r#"  path  =  "./file.ts"  "#),
            Some("./file.ts".to_string())
        );
    }

    #[test]
    fn test_validate_extensionless_references() {
        use std::fs;

        // Create a temporary directory with test files
        let temp_dir = std::env::temp_dir().join("tsz_test_refs");
        let _ = fs::create_dir_all(&temp_dir);

        // Create test files
        let a_ts = temp_dir.join("a.ts");
        let b_dts = temp_dir.join("b.d.ts");
        let c_ts = temp_dir.join("c.ts");

        fs::write(&a_ts, "var aa = 1;").unwrap();
        fs::write(&b_dts, "declare var bb: number;").unwrap();
        fs::write(&c_ts, "var cc = 1;").unwrap();

        let source_file = temp_dir.join("t.ts");

        // Test extension-less references
        assert!(
            validate_reference_path(&source_file, "a"),
            "Should find a.ts"
        );
        assert!(
            validate_reference_path(&source_file, "b"),
            "Should find b.d.ts"
        );
        assert!(
            validate_reference_path(&source_file, "c"),
            "Should find c.ts"
        );
        assert!(
            !validate_reference_path(&source_file, "missing"),
            "Should not find missing file"
        );

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
