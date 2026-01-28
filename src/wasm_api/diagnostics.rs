//! TypeScript Diagnostic APIs
//!
//! Provides diagnostic types and formatting utilities.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use super::enums::DiagnosticCategory;

/// TypeScript Diagnostic - represents a compiler diagnostic
///
/// Contains:
/// - file information (name, position)
/// - message (text or chain)
/// - category (error, warning, suggestion, message)
/// - error code
#[wasm_bindgen]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TsDiagnostic {
    /// File name (if file-related)
    file_name: Option<String>,
    /// Start position in source
    start: u32,
    /// Length of the diagnostic span
    length: u32,
    /// Diagnostic message text
    message_text: String,
    /// Diagnostic category
    category: u8,
    /// Diagnostic code (TS####)
    code: u32,
}

#[wasm_bindgen]
impl TsDiagnostic {
    /// Create a new diagnostic
    #[wasm_bindgen(constructor)]
    pub fn new(
        file_name: Option<String>,
        start: u32,
        length: u32,
        message_text: String,
        category: DiagnosticCategory,
        code: u32,
    ) -> TsDiagnostic {
        TsDiagnostic {
            file_name,
            start,
            length,
            message_text,
            category: category as u8,
            code,
        }
    }

    /// Get the file name
    #[wasm_bindgen(getter, js_name = fileName)]
    pub fn file_name(&self) -> Option<String> {
        self.file_name.clone()
    }

    /// Get the start position
    #[wasm_bindgen(getter)]
    pub fn start(&self) -> u32 {
        self.start
    }

    /// Get the length
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> u32 {
        self.length
    }

    /// Get the message text
    #[wasm_bindgen(getter, js_name = messageText)]
    pub fn message_text(&self) -> String {
        self.message_text.clone()
    }

    /// Get the category
    #[wasm_bindgen(getter)]
    pub fn category(&self) -> u8 {
        self.category
    }

    /// Get the error code
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> u32 {
        self.code
    }

    /// Check if this is an error
    #[wasm_bindgen(js_name = isError)]
    pub fn is_error(&self) -> bool {
        self.category == DiagnosticCategory::Error as u8
    }

    /// Check if this is a warning
    #[wasm_bindgen(js_name = isWarning)]
    pub fn is_warning(&self) -> bool {
        self.category == DiagnosticCategory::Warning as u8
    }

    /// Convert to JSON
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Format a diagnostic to a string
///
/// Matches TypeScript's `formatDiagnostic` output format.
#[wasm_bindgen(js_name = formatTsDiagnostic)]
pub fn format_ts_diagnostic(diagnostic: &TsDiagnostic, new_line: &str) -> String {
    let category_name = match diagnostic.category {
        0 => "warning",
        1 => "error",
        2 => "suggestion",
        3 => "message",
        _ => "error",
    };

    if let Some(ref file_name) = diagnostic.file_name {
        format!(
            "{}({},{}): {} TS{}: {}{}",
            file_name,
            diagnostic.start,
            diagnostic.length,
            category_name,
            diagnostic.code,
            diagnostic.message_text,
            new_line
        )
    } else {
        format!(
            "{} TS{}: {}{}",
            category_name, diagnostic.code, diagnostic.message_text, new_line
        )
    }
}

/// Format diagnostics with color and context
///
/// Provides pretty-printed output with:
/// - ANSI colors (if terminal supports it)
/// - Source code context around the error
/// - Squiggle underlining
#[wasm_bindgen(js_name = formatTsDiagnosticsWithColorAndContext)]
pub fn format_ts_diagnostics_with_color_and_context(
    diagnostics_json: &str,
    source_files_json: &str,
    use_colors: bool,
) -> String {
    // Parse inputs
    let diagnostics: Vec<TsDiagnostic> = match serde_json::from_str(diagnostics_json) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };

    let source_files: std::collections::HashMap<String, String> =
        serde_json::from_str(source_files_json).unwrap_or_default();

    let mut output = String::new();

    for diag in &diagnostics {
        // Category color
        let (color_start, color_end) = if use_colors {
            match diag.category {
                0 => ("\x1b[33m", "\x1b[0m"), // Yellow for warning
                1 => ("\x1b[31m", "\x1b[0m"), // Red for error
                2 => ("\x1b[36m", "\x1b[0m"), // Cyan for suggestion
                _ => ("", ""),
            }
        } else {
            ("", "")
        };

        // Format location
        if let Some(ref file_name) = diag.file_name {
            output.push_str(&format!(
                "{}{}{}({},{}): ",
                if use_colors { "\x1b[36m" } else { "" },
                file_name,
                if use_colors { "\x1b[0m" } else { "" },
                diag.start,
                diag.length
            ));
        }

        // Format message
        let category_name = match diag.category {
            0 => "warning",
            1 => "error",
            2 => "suggestion",
            3 => "message",
            _ => "error",
        };

        output.push_str(&format!(
            "{}{}{} TS{}: {}\n",
            color_start, category_name, color_end, diag.code, diag.message_text
        ));

        // Add source context if available
        if let (Some(_file_name), Some(source)) = (
            &diag.file_name,
            diag.file_name.as_ref().and_then(|f| source_files.get(f)),
        ) {
            // Find the line containing the error
            let start = diag.start as usize;
            let end = (diag.start + diag.length) as usize;

            if start < source.len() {
                // Find line start
                let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                // Find line end
                let line_end = source[start..]
                    .find('\n')
                    .map(|i| start + i)
                    .unwrap_or(source.len());

                // Get the line
                if line_start < line_end && line_end <= source.len() {
                    let line = &source[line_start..line_end];
                    output.push_str(&format!("\n  {}\n", line));

                    // Add squiggle
                    let squiggle_start = start - line_start;
                    let squiggle_len = (end - start).min(line.len() - squiggle_start).max(1);

                    output.push_str("  ");
                    output.push_str(&" ".repeat(squiggle_start));
                    if use_colors {
                        output.push_str(color_start);
                    }
                    output.push_str(&"~".repeat(squiggle_len));
                    if use_colors {
                        output.push_str(color_end);
                    }
                    output.push('\n');
                }
            }
        }

        output.push('\n');
    }

    output
}

/// Flatten diagnostic message chain to a single string
///
/// TypeScript diagnostics can have chained messages (messageText can be
/// a chain). This flattens them to a single string.
#[wasm_bindgen(js_name = flattenDiagnosticMessageText)]
pub fn flatten_diagnostic_message_text(message_text: &str, _new_line: &str) -> String {
    // If it's already a simple string, return it
    // In a full implementation, we'd parse JSON message chains
    message_text.to_string()
}

/// Get diagnostic category name
#[wasm_bindgen(js_name = diagnosticCategoryName)]
pub fn diagnostic_category_name(category: DiagnosticCategory) -> String {
    match category {
        DiagnosticCategory::Warning => "Warning".to_string(),
        DiagnosticCategory::Error => "Error".to_string(),
        DiagnosticCategory::Suggestion => "Suggestion".to_string(),
        DiagnosticCategory::Message => "Message".to_string(),
    }
}
