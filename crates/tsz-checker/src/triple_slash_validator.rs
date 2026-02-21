//! Triple-slash reference directive validation
//!
//! Validates /// <reference path="..." /> directives in TypeScript files.

use std::path::Path;

/// Extract triple-slash reference paths from source text
///
/// Returns a vector of (path, `line_number`, `quote_offset`) tuples for each reference directive found.
/// `quote_offset` is the byte offset of the value start (after the opening quote) within the original (untrimmed) line.
pub fn extract_reference_paths(source: &str) -> Vec<(String, usize, usize)> {
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

        // Extract the path value between quotes, with offset in the original line
        if let Some((path, offset)) = extract_quoted_path_with_offset(line) {
            references.push((path, line_num, offset));
        }
    }

    references
}

/// Extract `/// <reference types="..." />` directives from source text.
///
/// Returns a vector of (`type_name`, `resolution_mode`, `line_number`) tuples.
/// `resolution_mode` is `Some("import")` or `Some("require")` if specified.
pub fn extract_reference_types(source: &str) -> Vec<(String, Option<String>, usize)> {
    let mut references = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if !trimmed.starts_with("///") {
            continue;
        }

        if !trimmed.contains("<reference") || !trimmed.contains("types=") {
            continue;
        }

        if let Some(name) = extract_quoted_attr(trimmed, "types") {
            let resolution_mode = extract_quoted_attr(trimmed, "resolution-mode");
            references.push((name, resolution_mode, line_num));
        }
    }

    references
}

/// Extract `/// <amd-module name="..." />` directives from source text.
///
/// Returns a vector of (`module_name`, `line_number`) tuples for each amd-module directive found.
/// Used to detect multiple AMD module name assignments (TS2458).
pub fn extract_amd_module_names(source: &str) -> Vec<(String, usize)> {
    let mut amd_modules = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Check if line starts with ///
        if !trimmed.starts_with("///") {
            continue;
        }

        // Check if it contains <amd-module name=
        if !trimmed.contains("<amd-module") || !trimmed.contains("name=") {
            continue;
        }

        // Extract the name value between quotes
        if let Some(name) = extract_quoted_attr(trimmed, "name") {
            amd_modules.push((name, line_num));
        }
    }

    amd_modules
}

/// Extract the value of a named attribute from a reference directive line.
fn extract_quoted_attr(line: &str, attr: &str) -> Option<String> {
    let idx = line.find(attr)?;
    let after_attr = &line[idx + attr.len()..];

    let eq_idx = after_attr.find('=')?;
    let after_equals = &after_attr[eq_idx + 1..];

    let first_char = after_equals.trim_start().chars().next()?;
    if first_char != '"' && first_char != '\'' {
        return None;
    }

    let quote_char = first_char;
    let after_open_quote = &after_equals[after_equals.find(quote_char)? + 1..];

    let end_pos = after_open_quote.find(quote_char)?;
    Some(after_open_quote[..end_pos].to_string())
}

/// Extract path with byte offset of the value start within the line
fn extract_quoted_path_with_offset(line: &str) -> Option<(String, usize)> {
    extract_quoted_attr_with_offset(line, "path")
}

/// Extract the value and byte offset of the value start (after the opening quote).
fn extract_quoted_attr_with_offset(line: &str, attr: &str) -> Option<(String, usize)> {
    let attr_idx = line.find(attr)?;
    let after_attr = &line[attr_idx + attr.len()..];

    let eq_idx = after_attr.find('=')?;
    let after_equals = &after_attr[eq_idx + 1..];
    let trimmed = after_equals.trim_start();

    let first_char = trimmed.chars().next()?;
    if first_char != '"' && first_char != '\'' {
        return None;
    }

    // Byte offset of the character after the opening quote (the value start)
    let value_offset =
        attr_idx + attr.len() + eq_idx + 1 + (after_equals.len() - trimmed.len()) + 1;

    let after_open_quote = &trimmed[1..];
    let end_pos = after_open_quote.find(first_char)?;
    Some((after_open_quote[..end_pos].to_string(), value_offset))
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
            let path_with_ext = parent.join(format!("{reference_path}{ext}"));
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
#[path = "../tests/triple_slash_validator.rs"]
mod tests;
