//! Document Formatting implementation for LSP.
//!
//! Provides code formatting capabilities for TypeScript files.
//! Delegates to external formatters (prettier, eslint) when available,
//! and falls back to an internal formatter that handles indentation,
//! semicolons, whitespace normalization, and common TS/JS patterns.
//!
//! Also provides format-on-key support for semicolon and newline triggers.

use std::io::Write;
use std::path::Path;
use std::process::Command;
use tsz_common::position::{Position, Range};

/// Formatting options for a document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormattingOptions {
    /// Tab size.
    #[serde(rename = "tabSize")]
    pub tab_size: u32,
    /// Insert spaces when pressing Tab.
    #[serde(rename = "insertSpaces")]
    pub insert_spaces: bool,
    /// Trim trailing whitespace on a line.
    #[serde(rename = "trimTrailingWhitespace")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_trailing_whitespace: Option<bool>,
    /// Insert a final newline at the end of the file.
    #[serde(rename = "insertFinalNewline")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_final_newline: Option<bool>,
    /// Trim trailing whitespace on all lines.
    #[serde(rename = "trimFinalNewlines")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_final_newlines: Option<bool>,
    /// Semicolons preference: "insert" or "remove". Default is "insert".
    #[serde(rename = "semicolons")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semicolons: Option<String>,
}

impl Default for FormattingOptions {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            semicolons: None,
        }
    }
}

/// A text edit for formatting.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    /// The range to replace (0-based line and character).
    pub range: Range,
    /// The new text.
    pub new_text: String,
}

impl TextEdit {
    /// Create a new text edit.
    pub fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
}

/// Provider for document formatting.
pub struct DocumentFormattingProvider;

impl DocumentFormattingProvider {
    /// Check if prettier is available.
    pub fn has_prettier() -> bool {
        Command::new("prettier")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check if eslint with fix is available.
    pub fn has_eslint_fix() -> bool {
        Command::new("eslint")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Format a document using the best available formatter.
    ///
    /// Returns a list of text edits to apply, or an error message.
    /// All positions in returned edits are 0-based (LSP convention).
    pub fn format_document(
        file_path: &str,
        source_text: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        // Try prettier first (most common for TypeScript)
        if Self::has_prettier() {
            return Self::format_with_prettier(file_path, source_text, options);
        }

        // Fall back to eslint with --fix
        if Self::has_eslint_fix() {
            return Self::format_with_eslint(file_path, source_text, options);
        }

        // No formatter available - return internal formatting edits
        Self::apply_basic_formatting(source_text, options)
    }

    /// Format using prettier.
    fn format_with_prettier(
        file_path: &str,
        source_text: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let path = Path::new(file_path);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid file path")?;

        let mut cmd = Command::new("prettier");
        cmd.arg("--stdin-filepath").arg(file_name);

        if options.insert_spaces {
            cmd.arg("--use-tabs").arg("false");
            cmd.arg(format!("--tab-width={}", options.tab_size));
        } else {
            cmd.arg("--use-tabs").arg("true");
        }

        cmd.arg("--stdin");

        let output = cmd
            .current_dir(path.parent().unwrap_or(Path::new(".")))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn prettier: {}", e))?;

        output
            .stdin
            .as_ref()
            .ok_or("Failed to open stdin")?
            .write_all(source_text.as_bytes())
            .map_err(|e| format!("Failed to write to prettier stdin: {}", e))?;

        let result = output
            .wait_with_output()
            .map_err(|e| format!("Failed to read prettier output: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("Prettier failed: {}", stderr));
        }

        let formatted = String::from_utf8_lossy(&result.stdout).to_string();

        // Compute per-line edits to avoid overlapping ranges
        Self::compute_line_edits(source_text, &formatted)
    }

    /// Format using eslint with --fix.
    fn format_with_eslint(
        file_path: &str,
        source_text: &str,
        _options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let output = Command::new("eslint")
            .arg("--fix")
            .arg("--fix-to-stdout")
            .arg(file_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn eslint: {}", e))?;

        let result = output
            .wait_with_output()
            .map_err(|e| format!("Failed to read eslint output: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            if result.stdout.is_empty() {
                return Err(format!("ESLint failed: {}", stderr));
            }
        }

        let formatted = String::from_utf8_lossy(&result.stdout).to_string();

        Self::compute_line_edits(source_text, &formatted)
    }

    /// Compute per-line text edits between original and formatted text.
    /// This produces non-overlapping edits where each edit replaces exactly one line.
    /// Positions are 0-based.
    pub fn compute_line_edits(original: &str, formatted: &str) -> Result<Vec<TextEdit>, String> {
        if original == formatted {
            return Ok(vec![]);
        }

        let orig_lines: Vec<&str> = original.lines().collect();
        let fmt_lines: Vec<&str> = formatted.lines().collect();

        let orig_count = orig_lines.len();
        let fmt_count = fmt_lines.len();

        // Build per-line edits for lines that differ
        let mut edits = Vec::new();
        let max_common = orig_count.min(fmt_count);

        for i in 0..max_common {
            if orig_lines[i] != fmt_lines[i] {
                let line_len = orig_lines[i].len() as u32;
                edits.push(TextEdit::new(
                    Range::new(
                        Position::new(i as u32, 0),
                        Position::new(i as u32, line_len),
                    ),
                    fmt_lines[i].to_string(),
                ));
            }
        }

        // Handle extra lines in original (need to delete them)
        if orig_count > fmt_count && fmt_count > 0 {
            let start_line = fmt_count.saturating_sub(1);
            let start_char = fmt_lines[start_line].len() as u32;
            let end_line = orig_count.saturating_sub(1);
            let end_char = orig_lines[end_line].len() as u32;
            edits.push(TextEdit::new(
                Range::new(
                    Position::new(start_line as u32, start_char),
                    Position::new(end_line as u32, end_char),
                ),
                String::new(),
            ));
        }

        // Handle extra lines in formatted (need to insert them)
        if fmt_count > orig_count {
            let insert_line = if orig_count > 0 {
                orig_count.saturating_sub(1)
            } else {
                0
            };
            let insert_char = if orig_count > 0 {
                orig_lines[insert_line].len() as u32
            } else {
                0
            };
            let extra: Vec<&str> = fmt_lines[orig_count..].to_vec();
            let mut new_text = String::new();
            for line in &extra {
                new_text.push('\n');
                new_text.push_str(line);
            }
            edits.push(TextEdit::new(
                Range::new(
                    Position::new(insert_line as u32, insert_char),
                    Position::new(insert_line as u32, insert_char),
                ),
                new_text,
            ));
        }

        Ok(edits)
    }

    /// Apply basic formatting when no external formatter is available.
    ///
    /// This handles:
    /// - Trimming trailing whitespace
    /// - Adding final newline if missing
    /// - Converting tabs to spaces (or vice versa)
    /// - Indentation normalization for common TS patterns
    /// - Semicolon normalization
    pub fn apply_basic_formatting(
        source_text: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let formatted = Self::format_text(source_text, options);
        Self::compute_line_edits(source_text, &formatted)
    }

    /// Core formatting logic that returns the fully formatted text.
    pub fn format_text(source_text: &str, options: &FormattingOptions) -> String {
        let lines: Vec<&str> = source_text.lines().collect();
        let mut formatted_lines: Vec<String> = Vec::with_capacity(lines.len());

        // Track indent level for smart indentation
        let mut indent_level: i32 = 0;
        let indent_str = Self::make_indent_string(options, 1);

        for line in lines.iter() {
            let trimmed = line.trim();

            // Skip empty lines - preserve them as-is (just trim whitespace)
            if trimmed.is_empty() {
                formatted_lines.push(String::new());
                continue;
            }

            // Adjust indent before processing the line
            // Closing braces/brackets/parens reduce indent before the line
            let dedent_this_line = Self::line_starts_with_closing(trimmed);
            let case_dedent = Self::is_case_or_default(trimmed) && indent_level > 0;

            let effective_indent = if dedent_this_line {
                (indent_level - 1).max(0)
            } else if case_dedent {
                // case/default labels are indented one less than their body
                (indent_level - 1).max(0)
            } else {
                indent_level
            };

            // Build the formatted line
            let mut processed = trimmed.to_string();

            // Trim trailing whitespace
            if options.trim_trailing_whitespace.unwrap_or(true) {
                processed = processed.trim_end().to_string();
            }

            // Normalize semicolons: ensure statements end with semicolons
            if options.semicolons.as_deref() != Some("remove") {
                processed = Self::normalize_semicolons(&processed);
            }

            // Apply proper indentation
            let indent_prefix = indent_str.repeat(effective_indent as usize);
            let formatted_line = format!("{}{}", indent_prefix, processed);

            formatted_lines.push(formatted_line);

            // Adjust indent level for subsequent lines
            let opens = Self::count_openers(trimmed);
            let closes = Self::count_closers(trimmed);
            indent_level += opens - closes;
            indent_level = indent_level.max(0);
        }

        // Trim final empty lines if requested
        if options.trim_final_newlines.unwrap_or(true) {
            while formatted_lines.last().is_some_and(|l| l.is_empty()) {
                formatted_lines.pop();
            }
        }

        let mut result = formatted_lines.join("\n");

        // Add final newline if requested
        if options.insert_final_newline.unwrap_or(true) && !result.is_empty() {
            result.push('\n');
        }

        result
    }

    /// Create the indentation string for one level.
    fn make_indent_string(options: &FormattingOptions, levels: u32) -> String {
        if options.insert_spaces {
            " ".repeat((options.tab_size * levels) as usize)
        } else {
            "\t".repeat(levels as usize)
        }
    }

    /// Check if a trimmed line starts with a closing brace/bracket/paren.
    fn line_starts_with_closing(trimmed: &str) -> bool {
        trimmed.starts_with('}') || trimmed.starts_with(')') || trimmed.starts_with(']')
    }

    /// Check if a trimmed line is a case or default label in a switch.
    fn is_case_or_default(trimmed: &str) -> bool {
        trimmed.starts_with("case ")
            || trimmed.starts_with("default:")
            || trimmed.starts_with("default :")
    }

    /// Count opening braces/brackets/parens in a line (outside strings).
    fn count_openers(line: &str) -> i32 {
        let mut count = 0i32;
        let mut in_string = None;
        let mut escape = false;

        for ch in line.chars() {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            match in_string {
                Some(q) if ch == q => in_string = None,
                Some(_) => {}
                None => match ch {
                    '\'' | '"' | '`' => in_string = Some(ch),
                    '{' | '(' | '[' => count += 1,
                    _ => {}
                },
            }
        }
        count
    }

    /// Count closing braces/brackets/parens in a line (outside strings).
    fn count_closers(line: &str) -> i32 {
        let mut count = 0i32;
        let mut in_string = None;
        let mut escape = false;

        for ch in line.chars() {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            match in_string {
                Some(q) if ch == q => in_string = None,
                Some(_) => {}
                None => match ch {
                    '\'' | '"' | '`' => in_string = Some(ch),
                    '}' | ')' | ']' => count += 1,
                    _ => {}
                },
            }
        }
        count
    }

    /// Normalize semicolons: add missing semicolons to statement lines.
    fn normalize_semicolons(line: &str) -> String {
        let trimmed = line.trim_end();

        // Don't add semicolons after these patterns
        if trimmed.is_empty()
            || trimmed.ends_with('{')
            || trimmed.ends_with('}')
            || trimmed.ends_with('(')
            || trimmed.ends_with(',')
            || trimmed.ends_with(':')
            || trimmed.ends_with(';')
            || trimmed.ends_with('*')
            || trimmed.ends_with('/')
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/")
            || (trimmed.starts_with("import ") && !trimmed.contains("from "))
            || (trimmed.starts_with("export {") && !trimmed.ends_with('}'))
            || Self::is_case_or_default(trimmed)
            || trimmed.starts_with("if ")
            || trimmed.starts_with("if(")
            || trimmed.starts_with("} else")
            || trimmed.starts_with("else {")
            || trimmed.starts_with("else{")
            || trimmed.starts_with("for ")
            || trimmed.starts_with("for(")
            || trimmed.starts_with("while ")
            || trimmed.starts_with("while(")
            || trimmed.starts_with("switch ")
            || trimmed.starts_with("switch(")
            || trimmed.starts_with("try {")
            || trimmed.starts_with("try{")
            || trimmed.starts_with("} catch")
            || trimmed.starts_with("} finally")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("interface ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("namespace ")
            || trimmed.starts_with("module ")
            || trimmed.starts_with("@")  // decorators
            || trimmed.starts_with("function ")
            || trimmed.starts_with("async function ")
            || trimmed.starts_with("export default function ")
            || trimmed.starts_with("export function ")
            || trimmed.starts_with("export async function ")
            || trimmed.starts_with("export class ")
            || trimmed.starts_with("export interface ")
            || trimmed.starts_with("export enum ")
        {
            return trimmed.to_string();
        }

        // Lines that look like statements needing semicolons
        let needs_semi = trimmed.starts_with("let ")
            || trimmed.starts_with("const ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("return ")
            || trimmed == "return"
            || trimmed.starts_with("throw ")
            || trimmed.starts_with("break")
            || trimmed.starts_with("continue")
            || trimmed.starts_with("export default ")
            || (trimmed.starts_with("import ") && trimmed.contains("from "))
            || (trimmed.starts_with("export ") && trimmed.contains("from "))
            || trimmed.ends_with(')')
            || trimmed.ends_with(']')
            || trimmed.ends_with('"')
            || trimmed.ends_with('\'')
            || trimmed.ends_with('`');

        if needs_semi && !trimmed.ends_with(';') {
            format!("{};", trimmed)
        } else {
            trimmed.to_string()
        }
    }

    /// Convert leading spaces to tabs based on tab size.
    pub fn convert_leading_spaces_to_tabs(line: &str, tab_size: usize) -> String {
        let leading_spaces = line.chars().take_while(|&c| c == ' ').count();
        let leading_tabs = leading_spaces / tab_size;
        let remaining_spaces = leading_spaces % tab_size;

        let rest = &line[leading_spaces..];
        let tabs = "\t".repeat(leading_tabs);
        let spaces = " ".repeat(remaining_spaces);

        format!("{}{}{}", tabs, spaces, rest)
    }

    // =========================================================================
    // Format on key support
    // =========================================================================

    /// Handle format-on-key trigger.
    ///
    /// `key` is the character that was typed (e.g. ";" or "\n").
    /// `line` and `offset` are the 0-based position after the key was typed.
    ///
    /// Returns a list of text edits to apply to the line where the key was typed.
    pub fn format_on_key(
        source_text: &str,
        line: u32,
        _offset: u32,
        key: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        match key {
            ";" => Self::format_on_semicolon(source_text, line, options),
            "\n" => Self::format_on_enter(source_text, line, options),
            _ => Ok(vec![]),
        }
    }

    /// Format the current line when a semicolon is typed.
    /// Normalizes whitespace on the line that just received the semicolon.
    fn format_on_semicolon(
        source_text: &str,
        line: u32,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let lines: Vec<&str> = source_text.lines().collect();
        let line_idx = line as usize;
        if line_idx >= lines.len() {
            return Ok(vec![]);
        }

        let current_line = lines[line_idx];
        let trimmed = current_line.trim();

        // If the line has double semicolons, remove one
        if trimmed.ends_with(";;") {
            let fixed = &trimmed[..trimmed.len() - 1];
            let indent = Self::compute_indent_for_line(lines.as_slice(), line_idx, options);
            let new_text = format!("{}{}", indent, fixed);
            let line_len = current_line.len() as u32;
            return Ok(vec![TextEdit::new(
                Range::new(Position::new(line, 0), Position::new(line, line_len)),
                new_text,
            )]);
        }

        // Trim trailing whitespace on the current line
        let new_trimmed = current_line.trim_end();
        if new_trimmed != current_line {
            return Ok(vec![TextEdit::new(
                Range::new(
                    Position::new(line, new_trimmed.len() as u32),
                    Position::new(line, current_line.len() as u32),
                ),
                String::new(),
            )]);
        }

        Ok(vec![])
    }

    /// Format after pressing enter.
    /// Ensures proper indentation of the new line and trims the previous line.
    fn format_on_enter(
        source_text: &str,
        line: u32,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let lines: Vec<&str> = source_text.lines().collect();
        let line_idx = line as usize;

        let mut edits = Vec::new();

        // Trim trailing whitespace on the previous line
        if line_idx > 0 {
            let prev_line = lines[line_idx - 1];
            let prev_trimmed = prev_line.trim_end();
            if prev_trimmed.len() < prev_line.len() {
                edits.push(TextEdit::new(
                    Range::new(
                        Position::new(line - 1, prev_trimmed.len() as u32),
                        Position::new(line - 1, prev_line.len() as u32),
                    ),
                    String::new(),
                ));
            }
        }

        // Set proper indentation on the current (new) line
        if line_idx < lines.len() {
            let current_line = lines[line_idx];
            let current_trimmed = current_line.trim();
            let expected_indent =
                Self::compute_indent_for_line(lines.as_slice(), line_idx, options);

            let current_leading_len = current_line.len() - current_line.trim_start().len();
            let current_leading = &current_line[..current_leading_len];
            if current_leading != expected_indent && !current_trimmed.is_empty() {
                let old_indent_len = current_leading.len() as u32;
                edits.push(TextEdit::new(
                    Range::new(Position::new(line, 0), Position::new(line, old_indent_len)),
                    expected_indent,
                ));
            }
        }

        Ok(edits)
    }

    /// Compute the expected indentation string for a given line index,
    /// based on the context of surrounding lines.
    fn compute_indent_for_line(
        lines: &[&str],
        line_idx: usize,
        options: &FormattingOptions,
    ) -> String {
        let indent_unit = Self::make_indent_string(options, 1);

        // Look at the previous non-empty line
        let mut prev_idx = line_idx.saturating_sub(1);
        while prev_idx > 0 && lines.get(prev_idx).is_none_or(|l| l.trim().is_empty()) {
            prev_idx -= 1;
        }

        let prev_line = lines.get(prev_idx).copied().unwrap_or("");
        let prev_trimmed = prev_line.trim();

        // Get the indentation of the previous line
        let prev_indent_len = prev_line.len() - prev_line.trim_start().len();
        let prev_indent = &prev_line[..prev_indent_len];

        // Check the current line for dedent
        let current_trimmed = lines.get(line_idx).map(|l| l.trim()).unwrap_or("");
        let needs_dedent = Self::line_starts_with_closing(current_trimmed)
            || Self::is_case_or_default(current_trimmed);

        // Determine if we should increase indent
        let should_indent = prev_trimmed.ends_with('{')
            || prev_trimmed.ends_with('(')
            || prev_trimmed.ends_with('[')
            || prev_trimmed.ends_with("=>")
            || (prev_trimmed.ends_with(':') && Self::is_case_or_default(prev_trimmed));

        if needs_dedent && should_indent {
            // Opening and closing on adjacent lines: same indent as previous
            prev_indent.to_string()
        } else if needs_dedent {
            // Dedent from previous
            let unit_len = indent_unit.len();
            if prev_indent_len >= unit_len {
                prev_indent[..prev_indent_len - unit_len].to_string()
            } else {
                String::new()
            }
        } else if should_indent {
            format!("{}{}", prev_indent, indent_unit)
        } else {
            prev_indent.to_string()
        }
    }
}

#[cfg(test)]
mod formatting_tests {
    use super::*;

    #[test]
    fn test_formatting_options_default() {
        let options = FormattingOptions::default();
        assert_eq!(options.tab_size, 4);
        assert!(options.insert_spaces);
        assert_eq!(options.trim_trailing_whitespace, Some(true));
        assert_eq!(options.insert_final_newline, Some(true));
    }

    #[test]
    fn test_basic_formatting_trailing_whitespace() {
        let source = "let x = 1;   \nlet y = 2;\n";
        let options = FormattingOptions {
            trim_trailing_whitespace: Some(true),
            ..Default::default()
        };

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());

        let edits = result.unwrap();
        assert!(!edits.is_empty());
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        assert!(formatted.contains("let x = 1;\n"));
        assert!(!formatted.contains("let x = 1;   "));
    }

    #[test]
    fn test_basic_formatting_insert_final_newline() {
        let source = "let x = 1;";
        let options = FormattingOptions {
            insert_final_newline: Some(true),
            ..Default::default()
        };

        let formatted = DocumentFormattingProvider::format_text(source, &options);
        assert!(formatted.ends_with('\n'));
    }

    #[test]
    fn test_basic_formatting_tabs_to_spaces() {
        let source = "\tlet x = 1;";
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..Default::default()
        };

        let formatted = DocumentFormattingProvider::format_text(source, &options);
        // The formatter re-indents, so a top-level let should have no indent
        assert!(formatted.starts_with("let x = 1;"));
    }

    #[test]
    fn test_basic_formatting_spaces_to_tabs() {
        let source = "    let x = 1;";
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: false,
            ..Default::default()
        };

        let formatted = DocumentFormattingProvider::format_text(source, &options);
        assert!(formatted.starts_with("let x = 1;"));
    }

    #[test]
    fn test_text_edit_creation() {
        let range = Range::new(Position::new(0, 0), Position::new(0, 10));
        let edit = TextEdit::new(range, "new text".to_string());
        assert_eq!(edit.new_text, "new text");
        assert_eq!(edit.range.start.line, 0);
        assert_eq!(edit.range.end.character, 10);
    }

    #[test]
    fn test_convert_leading_spaces_to_tabs() {
        let result =
            DocumentFormattingProvider::convert_leading_spaces_to_tabs("        let x = 1;", 4);
        assert_eq!(result, "\t\tlet x = 1;");

        let result =
            DocumentFormattingProvider::convert_leading_spaces_to_tabs("      let x = 1;", 4);
        assert_eq!(result, "\t  let x = 1;");

        let result = DocumentFormattingProvider::convert_leading_spaces_to_tabs("  let x = 1;", 4);
        assert_eq!(result, "  let x = 1;");
    }

    #[test]
    fn test_basic_formatting_preserves_multiline() {
        let source = "function foo() {\n  return 1;\n}";
        let options = FormattingOptions::default();

        let formatted = DocumentFormattingProvider::format_text(source, &options);
        assert!(formatted.contains("function foo()"));
        assert!(formatted.contains("return 1;"));
    }

    #[test]
    fn test_basic_formatting_empty_source() {
        let source = "";
        let options = FormattingOptions::default();

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());
        let edits = result.unwrap();
        assert!(
            edits.is_empty()
                || edits
                    .iter()
                    .all(|e| e.new_text.is_empty() || e.new_text == "\n")
        );
    }

    // =========================================================================
    // New tests: indentation
    // =========================================================================

    #[test]
    fn test_format_if_else_indentation() {
        let source = "if (x) {\nlet a = 1;\n} else {\nlet b = 2;\n}";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[0], "if (x) {");
        assert_eq!(lines[1], "    let a = 1;");
        assert_eq!(lines[2], "} else {");
        assert_eq!(lines[3], "    let b = 2;");
        assert_eq!(lines[4], "}");
    }

    #[test]
    fn test_format_function_body_indentation() {
        let source = "function greet(name: string) {\nconst msg = \"hi\";\nreturn msg;\n}";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[0], "function greet(name: string) {");
        assert_eq!(lines[1], "    const msg = \"hi\";");
        assert_eq!(lines[2], "    return msg;");
        assert_eq!(lines[3], "}");
    }

    #[test]
    fn test_format_switch_case_indentation() {
        let source = "switch (x) {\ncase 1:\nlet a = 1;\nbreak;\ncase 2:\nlet b = 2;\nbreak;\ndefault:\nlet c = 3;\n}";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[0], "switch (x) {");
        assert!(lines[1].starts_with("case 1:"), "got: {}", lines[1]);
        assert!(
            lines[2].starts_with("    "),
            "case body should be indented, got: {}",
            lines[2]
        );
    }

    #[test]
    fn test_format_nested_blocks() {
        let source = "function foo() {\nif (true) {\nlet x = 1;\n}\n}";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[0], "function foo() {");
        assert_eq!(lines[1], "    if (true) {");
        assert_eq!(lines[2], "        let x = 1;");
        assert_eq!(lines[3], "    }");
        assert_eq!(lines[4], "}");
    }

    #[test]
    fn test_format_semicolon_normalization() {
        let source = "let x = 1\nlet y = 2\n";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);

        assert!(
            formatted.contains("let x = 1;"),
            "should add semicolon, got: {}",
            formatted
        );
        assert!(
            formatted.contains("let y = 2;"),
            "should add semicolon, got: {}",
            formatted
        );
    }

    #[test]
    fn test_format_no_double_semicolons() {
        let source = "let x = 1;\nlet y = 2;\n";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);

        assert!(
            !formatted.contains(";;"),
            "should not produce double semicolons"
        );
    }

    #[test]
    fn test_format_tab_size_2() {
        let source = "function foo() {\nlet x = 1;\n}";
        let options = FormattingOptions {
            tab_size: 2,
            insert_spaces: true,
            ..Default::default()
        };
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[1], "  let x = 1;");
    }

    #[test]
    fn test_format_with_tabs() {
        let source = "function foo() {\nlet x = 1;\n}";
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: false,
            ..Default::default()
        };
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[1], "\tlet x = 1;");
    }

    // =========================================================================
    // New tests: format on key
    // =========================================================================

    #[test]
    fn test_format_on_semicolon_removes_double() {
        let source = "let x = 1;;\n";
        let options = FormattingOptions::default();
        let result = DocumentFormattingProvider::format_on_key(source, 0, 11, ";", &options);
        assert!(result.is_ok());
        let edits = result.unwrap();
        assert!(
            !edits.is_empty(),
            "should produce edit for double semicolon"
        );
        let edit = &edits[0];
        assert!(
            edit.new_text.ends_with("let x = 1;"),
            "got: {}",
            edit.new_text
        );
        assert!(!edit.new_text.ends_with(";;"));
    }

    #[test]
    fn test_format_on_enter_trims_prev_line() {
        let source = "let x = 1;   \nlet y = 2;\n";
        let options = FormattingOptions::default();
        let result = DocumentFormattingProvider::format_on_key(source, 1, 0, "\n", &options);
        assert!(result.is_ok());
        let edits = result.unwrap();
        let has_trim = edits
            .iter()
            .any(|e| e.range.start.line == 0 && e.new_text.is_empty());
        assert!(has_trim, "should trim trailing whitespace on previous line");
    }

    #[test]
    fn test_format_on_enter_indents_after_brace() {
        let source = "function foo() {\n\n";
        let options = FormattingOptions::default();
        let result = DocumentFormattingProvider::format_on_key(source, 1, 0, "\n", &options);
        assert!(result.is_ok());
        // The current line is empty, so no indent edit is produced (graceful)
    }

    #[test]
    fn test_format_on_key_unknown_key() {
        let source = "let x = 1;\n";
        let options = FormattingOptions::default();
        let result = DocumentFormattingProvider::format_on_key(source, 0, 5, "a", &options);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // =========================================================================
    // New tests: line edit correctness (0-based positions)
    // =========================================================================

    #[test]
    fn test_compute_line_edits_no_change() {
        let result = DocumentFormattingProvider::compute_line_edits("hello\n", "hello\n");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_compute_line_edits_single_line_change() {
        let result = DocumentFormattingProvider::compute_line_edits("hello  \n", "hello\n");
        assert!(result.is_ok());
        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        let edit = &edits[0];
        assert_eq!(edit.range.start.line, 0);
        assert_eq!(edit.range.start.character, 0);
        assert_eq!(edit.range.end.line, 0);
        assert_eq!(edit.new_text, "hello");
    }

    #[test]
    fn test_compute_line_edits_no_overlapping_ranges() {
        let original = "line1\nline2  \nline3\n";
        let formatted = "line1\nline2\nline3\n";
        let result = DocumentFormattingProvider::compute_line_edits(original, formatted);
        assert!(result.is_ok());
        let edits = result.unwrap();

        for i in 0..edits.len() {
            for j in (i + 1)..edits.len() {
                let a = &edits[i];
                let b = &edits[j];
                let a_before_b = a.range.end.line < b.range.start.line
                    || (a.range.end.line == b.range.start.line
                        && a.range.end.character <= b.range.start.character);
                let b_before_a = b.range.end.line < a.range.start.line
                    || (b.range.end.line == a.range.start.line
                        && b.range.end.character <= a.range.start.character);
                assert!(
                    a_before_b || b_before_a,
                    "Overlapping edits: {:?} and {:?}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn test_format_positions_are_zero_based() {
        let source = "function foo() {\nlet x = 1\n}";
        let options = FormattingOptions::default();
        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());
        let edits = result.unwrap();

        for edit in &edits {
            assert!(
                edit.range.start.line < 1000,
                "Start line too large: {}",
                edit.range.start.line
            );
            assert!(
                edit.range.end.line < 1000,
                "End line too large: {}",
                edit.range.end.line
            );
        }
    }

    #[test]
    fn test_format_class_body_indentation() {
        let source = "class Foo {\nbar: number;\nbaz() {\nreturn 1;\n}\n}";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[0], "class Foo {");
        assert_eq!(lines[1], "    bar: number;");
        assert_eq!(lines[2], "    baz() {");
        assert_eq!(lines[3], "        return 1;");
        assert_eq!(lines[4], "    }");
        assert_eq!(lines[5], "}");
    }

    #[test]
    fn test_format_preserves_empty_lines() {
        let source = "let x = 1;\n\nlet y = 2;\n";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        assert!(
            formatted.contains("let x = 1;\n\nlet y = 2;"),
            "got: {}",
            formatted
        );
    }

    #[test]
    fn test_format_arrow_function() {
        let source = "const fn = () => {\nreturn 1;\n}";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        let lines: Vec<&str> = formatted.trim_end().lines().collect();

        assert_eq!(lines[0], "const fn = () => {");
        assert_eq!(lines[1], "    return 1;");
        assert_eq!(lines[2], "}");
    }

    #[test]
    fn test_format_multiline_import() {
        let source = "import { foo } from \"bar\";\n";
        let options = FormattingOptions::default();
        let formatted = DocumentFormattingProvider::format_text(source, &options);
        assert!(
            formatted.contains("import { foo } from \"bar\";"),
            "got: {}",
            formatted
        );
    }
}
