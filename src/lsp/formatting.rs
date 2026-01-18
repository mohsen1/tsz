//! Document Formatting implementation for LSP.
//!
//! Provides code formatting capabilities for TypeScript files.
//! Currently delegates to external formatters (prettier, eslint, etc.)
//! via command-line tools.

use crate::lsp::position::{Position, Range};
use std::io::Write;
use std::path::Path;
use std::process::Command;

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
}

impl Default for FormattingOptions {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
        }
    }
}

/// A text edit for formatting.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    /// The range to replace.
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
            .map(|_| true)
            .unwrap_or(false)
    }

    /// Check if eslint with fix is available.
    pub fn has_eslint_fix() -> bool {
        Command::new("eslint")
            .arg("--version")
            .output()
            .map(|_| true)
            .unwrap_or(false)
    }

    /// Format a document using the best available formatter.
    ///
    /// Returns a list of text edits to apply, or an error message.
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

        // No formatter available - return minimal formatting edits
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

        // Build prettier arguments
        let mut cmd = Command::new("prettier");
        cmd.arg("--stdin-filepath").arg(file_name);

        if options.insert_spaces {
            cmd.arg("--use-tabs").arg("false");
            cmd.arg(&format!("--tab-width={}", options.tab_size));
        } else {
            cmd.arg("--use-tabs").arg("true");
        }

        cmd.arg("--stdin");

        // Run prettier
        let output = cmd
            .current_dir(path.parent().unwrap_or(Path::new(".")))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn prettier: {}", e))?;

        // Write source to stdin
        output
            .stdin
            .as_ref()
            .ok_or("Failed to open stdin")?
            .write_all(source_text.as_bytes())
            .map_err(|e| format!("Failed to write to prettier stdin: {}", e))?;

        // Get output
        let result = output
            .wait_with_output()
            .map_err(|e| format!("Failed to read prettier output: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("Prettier failed: {}", stderr));
        }

        let formatted = String::from_utf8_lossy(&result.stdout);

        // Create a single edit replacing the entire document
        Ok(vec![TextEdit::new(
            Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
            formatted.to_string(),
        )])
    }

    /// Format using eslint with --fix.
    fn format_with_eslint(
        file_path: &str,
        _source_text: &str,
        _options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        // Run eslint with --fix
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
            // ESLint might exit with error but still output formatted code
            // Continue if we got stdout
            if result.stdout.is_empty() {
                return Err(format!("ESLint failed: {}", stderr));
            }
        }

        let formatted = String::from_utf8_lossy(&result.stdout);

        Ok(vec![TextEdit::new(
            Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
            formatted.to_string(),
        )])
    }

    /// Apply basic formatting when no external formatter is available.
    ///
    /// This handles:
    /// - Trimming trailing whitespace
    /// - Adding final newline if missing
    /// - Converting tabs to spaces (or vice versa)
    fn apply_basic_formatting(
        source_text: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let mut formatted_lines: Vec<String> = Vec::new();
        let lines: Vec<&str> = source_text.lines().collect();

        for line in lines.iter() {
            let mut processed = line.to_string();

            // Trim trailing whitespace if requested
            if options.trim_trailing_whitespace.unwrap_or(true) {
                processed = processed.trim_end().to_string();
            }

            // Convert tabs to spaces or vice versa
            if options.insert_spaces {
                // Tabs to spaces
                let tab_str = " ".repeat(options.tab_size as usize);
                processed = processed.replace('\t', &tab_str);
            } else {
                // Spaces to tabs (only for leading spaces)
                processed =
                    Self::convert_leading_spaces_to_tabs(&processed, options.tab_size as usize);
            }

            formatted_lines.push(processed);
        }

        // Add final newline if requested
        if options.insert_final_newline.unwrap_or(true) {
            formatted_lines.push(String::new());
        }

        let formatted = formatted_lines.join("\n");

        Ok(vec![TextEdit::new(
            Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
            formatted,
        )])
    }

    /// Convert leading spaces to tabs based on tab size.
    fn convert_leading_spaces_to_tabs(line: &str, tab_size: usize) -> String {
        let leading_spaces = line.chars().take_while(|&c| c == ' ').count();
        let leading_tabs = leading_spaces / tab_size;
        let remaining_spaces = leading_spaces % tab_size;

        let rest = &line[leading_spaces..];
        let tabs = "\t".repeat(leading_tabs);
        let spaces = " ".repeat(remaining_spaces);

        format!("{}{}{}", tabs, spaces, rest)
    }
}

#[cfg(test)]
mod formatting_tests {
    use super::*;

    #[test]
    fn test_formatting_options_default() {
        let options = FormattingOptions::default();
        assert_eq!(options.tab_size, 4);
        assert_eq!(options.insert_spaces, true);
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
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("let x = 1;\nlet y = 2;\n"));
    }

    #[test]
    fn test_basic_formatting_insert_final_newline() {
        let source = "let x = 1;";
        let options = FormattingOptions {
            insert_final_newline: Some(true),
            ..Default::default()
        };

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.ends_with('\n'));
    }

    #[test]
    fn test_basic_formatting_tabs_to_spaces() {
        let source = "\tlet x = 1;";
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..Default::default()
        };

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.starts_with("    let x = 1;"));
    }

    #[test]
    fn test_basic_formatting_spaces_to_tabs() {
        let source = "    let x = 1;";
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: false,
            ..Default::default()
        };

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.starts_with("\tlet x = 1;"));
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
        // 8 spaces with tab_size 4 should become 2 tabs
        let result =
            DocumentFormattingProvider::convert_leading_spaces_to_tabs("        let x = 1;", 4);
        assert_eq!(result, "\t\tlet x = 1;");

        // 6 spaces with tab_size 4 should become 1 tab + 2 spaces
        let result =
            DocumentFormattingProvider::convert_leading_spaces_to_tabs("      let x = 1;", 4);
        assert_eq!(result, "\t  let x = 1;");

        // 2 spaces with tab_size 4 should stay as spaces
        let result = DocumentFormattingProvider::convert_leading_spaces_to_tabs("  let x = 1;", 4);
        assert_eq!(result, "  let x = 1;");
    }

    #[test]
    fn test_basic_formatting_preserves_multiline() {
        let source = "function foo() {\n  return 1;\n}";
        let options = FormattingOptions::default();

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("function foo()"));
    }

    #[test]
    fn test_basic_formatting_empty_source() {
        let source = "";
        let options = FormattingOptions::default();

        let result = DocumentFormattingProvider::apply_basic_formatting(source, &options);
        assert!(result.is_ok());

        let edits = result.unwrap();
        assert_eq!(edits.len(), 1);
    }
}
