//! Triple-slash reference directive validation
//!
//! Validates /// <reference path="..." /> directives in TypeScript files.

use std::path::Path;

/// Extract triple-slash reference paths from source text
///
/// Returns a vector of (path, `line_number`) tuples for each reference directive found.
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

/// Extract path from a reference directive line
fn extract_quoted_path(line: &str) -> Option<String> {
    extract_quoted_attr(line, "path")
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
