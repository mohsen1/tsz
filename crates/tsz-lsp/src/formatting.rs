//! Document Formatting implementation for LSP.
//!
//! External formatters (`Prettier`, `ESLint` `--fix`) handle real TypeScript /
//! JavaScript formatting. This module is a thin wrapper around them:
//!
//! 1. Try `Prettier` via stdin.
//! 2. Fall back to `ESLint` with `--fix-dry-run` via stdin.
//! 3. If neither is available, run a strictly conservative internal
//!    fallback that only performs **whitespace-only** cleanup:
//!    - trim trailing whitespace,
//!    - normalize the final newline.
//!
//! The internal fallback never rewrites code structure: it does not infer
//! indentation, does not insert or remove semicolons, does not change brace
//! spacing, and does not re-format statements. Structural formatting is
//! fundamentally syntax-sensitive (template literals, regex literals, JSX,
//! generics, conditional types, decorators, etc.), and correct handling
//! requires a real parser — which lives in `Prettier` / `ESLint`, not here.
//!
//! Format-on-key follows the same policy: in fallback mode it only trims
//! trailing whitespace; it does not re-indent or manipulate semicolons.

#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
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
    /// Trim trailing blank lines at the end of the file.
    #[serde(rename = "trimFinalNewlines")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_final_newlines: Option<bool>,
    /// Semicolons preference. Accepted by the public API for forward
    /// compatibility, but the internal fallback never inserts or removes
    /// semicolons — only external formatters implement this preference.
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
    pub const fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
}

/// Capability boundary for the internal fallback formatter.
///
/// The fallback is allowed to perform [`FallbackFormattingMode::WhitespaceOnly`]
/// operations. Anything else — re-indenting, semicolon adjustment, brace
/// spacing, member spacing, `as` spacing, etc. — is
/// [`FallbackFormattingMode::UnsupportedForStructuralFormatting`] and must be
/// produced by an external formatter. When a request would require structural
/// changes and no external formatter is available, prefer "no edits" over
/// "risky edits".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackFormattingMode {
    /// Whitespace-only cleanup. Safe without syntax awareness:
    /// - trim trailing whitespace,
    /// - normalize final newline.
    ///
    /// Preserves every non-whitespace character exactly.
    WhitespaceOnly,
    /// The caller requested a transformation that needs a real parser
    /// (indentation, semicolons, brace spacing, etc.). The internal fallback
    /// refuses and returns no edits.
    UnsupportedForStructuralFormatting,
}

/// Provider for document formatting.
pub struct DocumentFormattingProvider;

impl DocumentFormattingProvider {
    /// Check if prettier is available.
    pub fn has_prettier() -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Command::new("prettier")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    /// Check if eslint with fix is available.
    pub fn has_eslint_fix() -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Command::new("eslint")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    /// Format a document using the best available formatter.
    ///
    /// Tries `Prettier` first, then `ESLint` `--fix-dry-run`. If neither is
    /// available, falls back to [`apply_safe_whitespace_formatting`].
    ///
    /// All positions in returned edits are 0-based (LSP convention).
    ///
    /// [`apply_safe_whitespace_formatting`]: Self::apply_safe_whitespace_formatting
    pub fn format_document(
        file_path: &str,
        source_text: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        // External formatters (prettier, eslint) require process spawning,
        // which is not available on WASM.
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Try prettier first (most common for TypeScript)
            if Self::has_prettier() {
                return Self::format_with_prettier(file_path, source_text, options);
            }

            // Fall back to eslint with --fix
            if Self::has_eslint_fix() {
                return Self::format_with_eslint(file_path, source_text, options);
            }
        }

        // Suppress unused warning on WASM where file_path isn't needed
        let _ = file_path;

        // Conservative, whitespace-only fallback.
        Self::apply_safe_whitespace_formatting(source_text, options)
    }

    /// Format using prettier.
    #[cfg(not(target_arch = "wasm32"))]
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
            .current_dir(path.parent().unwrap_or_else(|| Path::new(".")))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn prettier: {e}"))?;

        output
            .stdin
            .as_ref()
            .ok_or("Failed to open stdin")?
            .write_all(source_text.as_bytes())
            .map_err(|e| format!("Failed to write to prettier stdin: {e}"))?;

        let result = output
            .wait_with_output()
            .map_err(|e| format!("Failed to read prettier output: {e}"))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("Prettier failed: {stderr}"));
        }

        let formatted = String::from_utf8_lossy(&result.stdout).to_string();

        Self::compute_line_edits(source_text, &formatted)
    }

    /// Format using eslint with stdin-based `--fix-dry-run`.
    ///
    /// Pipes the in-memory document buffer into `ESLint` via stdin so that
    /// formatting operates on the current editor buffer rather than whatever
    /// happens to be on disk. This matches the LSP contract that unsaved
    /// changes are the source of truth.
    #[cfg(not(target_arch = "wasm32"))]
    fn format_with_eslint(
        file_path: &str,
        source_text: &str,
        _options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let path = Path::new(file_path);

        let child = Command::new("eslint")
            .arg("--fix-dry-run")
            .arg("--stdin")
            .arg("--stdin-filename")
            .arg(file_path)
            .arg("--format")
            .arg("json")
            .current_dir(path.parent().unwrap_or_else(|| Path::new(".")))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn eslint: {e}"))?;

        child
            .stdin
            .as_ref()
            .ok_or("Failed to open eslint stdin")?
            .write_all(source_text.as_bytes())
            .map_err(|e| format!("Failed to write to eslint stdin: {e}"))?;

        let result = child
            .wait_with_output()
            .map_err(|e| format!("Failed to read eslint output: {e}"))?;

        // Exit code 0 = no problems, 1 = lint problems (fixable or not),
        // 2 = internal/configuration error. With `--fix-dry-run` we still
        // want to read stdout for codes 0 and 1.
        if result.status.code().is_some_and(|c| c >= 2) {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("ESLint failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&result.stdout);
        match Self::parse_eslint_fix_output(&stdout) {
            Ok(Some(formatted)) => Self::compute_line_edits(source_text, &formatted),
            Ok(None) => Ok(vec![]),
            Err(err) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                tracing::debug!(error = %err, stderr = %stderr, "eslint JSON parse failed");
                Err(err)
            }
        }
    }

    /// Parse `ESLint` `--format=json` output and return the `output` field
    /// from the first result, if any fixes were produced.
    ///
    /// Returns:
    /// - `Ok(Some(text))` when `ESLint` emitted a fixed version of the source.
    /// - `Ok(None)` when `ESLint` produced no fixes (empty stdout or no
    ///   `output` key on the result object).
    /// - `Err(msg)` when stdout is non-empty but not valid JSON.
    pub fn parse_eslint_fix_output(stdout: &str) -> Result<Option<String>, String> {
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let parsed: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| format!("Failed to parse eslint JSON output: {e}"))?;

        let output = parsed
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|result| result.get("output"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);

        Ok(output)
    }

    /// Compute a minimal set of text edits that transforms `original` into
    /// `formatted`.
    ///
    /// Strategy:
    /// - If the texts are equal, return no edits.
    /// - If they have the same line count and the same final-newline state,
    ///   emit one edit per changed line. Edits are returned in descending
    ///   order (bottom-to-top) so that applying them sequentially does not
    ///   invalidate subsequent ranges.
    /// - Otherwise, emit a single whole-document replacement. This is both
    ///   simpler and safer than reconstructing line-level diffs across
    ///   insertions/deletions.
    ///
    /// All positions are 0-based.
    pub fn compute_line_edits(original: &str, formatted: &str) -> Result<Vec<TextEdit>, String> {
        if original == formatted {
            return Ok(vec![]);
        }

        let orig_lines: Vec<&str> = original.lines().collect();
        let fmt_lines: Vec<&str> = formatted.lines().collect();
        let same_trailing_newline = original.ends_with('\n') == formatted.ends_with('\n');

        if orig_lines.len() == fmt_lines.len() && same_trailing_newline {
            let mut edits: Vec<TextEdit> = orig_lines
                .iter()
                .zip(fmt_lines.iter())
                .enumerate()
                .filter_map(|(i, (orig, fmt))| {
                    if orig == fmt {
                        return None;
                    }
                    Some(TextEdit::new(
                        Range::new(
                            Position::new(i as u32, 0),
                            Position::new(i as u32, orig.len() as u32),
                        ),
                        (*fmt).to_string(),
                    ))
                })
                .collect();

            // Emit edits from bottom-to-top so that consumers applying them
            // sequentially do not shift later ranges.
            edits.sort_by(|a, b| {
                b.range
                    .start
                    .line
                    .cmp(&a.range.start.line)
                    .then_with(|| b.range.start.character.cmp(&a.range.start.character))
            });
            return Ok(edits);
        }

        // Line counts differ or EOF-newline state differs: emit one
        // whole-document replacement. This keeps edits correct without
        // risking overlapping ranges.
        let end_position = document_end_position(original);
        Ok(vec![TextEdit::new(
            Range::new(Position::new(0, 0), end_position),
            formatted.to_string(),
        )])
    }

    /// Conservative, whitespace-only fallback formatter.
    ///
    /// Safe operations (no syntax awareness required):
    /// - trim trailing whitespace on each line (when
    ///   [`FormattingOptions::trim_trailing_whitespace`] is enabled),
    /// - normalize trailing blank lines at EOF (when
    ///   [`FormattingOptions::trim_final_newlines`] is enabled),
    /// - add a final newline at EOF (when
    ///   [`FormattingOptions::insert_final_newline`] is enabled).
    ///
    /// Anything that would require syntax awareness — re-indentation, brace
    /// spacing, semicolon normalization, `as` operator spacing, member
    /// spacing, collapsing `{}` etc. — is intentionally not performed. Those
    /// rewrites require an external formatter.
    ///
    /// This is the policy expressed by
    /// [`FallbackFormattingMode::WhitespaceOnly`].
    pub fn apply_safe_whitespace_formatting(
        source_text: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let formatted = Self::safe_whitespace_text(source_text, options);
        Self::compute_line_edits(source_text, &formatted)
    }

    /// Produce the whitespace-only normalized form of `source_text`.
    ///
    /// Preserves every non-whitespace byte exactly: code content, string
    /// literals, template literals, regex literals, decorators, JSX and
    /// conditional types are all untouched. Only trailing whitespace per
    /// line and end-of-file newlines are adjusted, according to `options`.
    pub fn safe_whitespace_text(source_text: &str, options: &FormattingOptions) -> String {
        let trim_trailing = options.trim_trailing_whitespace.unwrap_or(true);
        let trim_final_newlines = options.trim_final_newlines.unwrap_or(true);
        let insert_final_newline = options.insert_final_newline.unwrap_or(true);

        let had_trailing_newline = source_text.ends_with('\n');

        // Per-line trailing whitespace trim, preserving line contents.
        let mut lines: Vec<String> = source_text
            .split('\n')
            .map(|line| {
                // Strip a trailing '\r' so Windows line endings are normalized
                // consistently with the trim step on the payload below.
                let stripped = line.strip_suffix('\r').unwrap_or(line);
                if trim_trailing {
                    stripped.trim_end_matches([' ', '\t']).to_string()
                } else {
                    stripped.to_string()
                }
            })
            .collect();

        // `split('\n')` on a string ending in '\n' yields an extra empty
        // trailing element. Drop it; the trailing-newline flag below governs
        // whether one is re-emitted.
        if had_trailing_newline && lines.last().is_some_and(std::string::String::is_empty) {
            lines.pop();
        }

        if trim_final_newlines {
            while lines.last().is_some_and(std::string::String::is_empty) {
                lines.pop();
            }
        }

        let mut result = lines.join("\n");

        if result.is_empty() {
            // Don't synthesize a newline-only file from empty input.
            return String::new();
        }

        if insert_final_newline || (had_trailing_newline && !trim_final_newlines) {
            result.push('\n');
        }

        result
    }

    // =========================================================================
    // Range formatting
    // =========================================================================

    /// Format a specific range within a document.
    ///
    /// External formatters do not provide stable range-only formatting from
    /// stdin (`Prettier` and `ESLint` operate on whole files), so this method
    /// runs the conservative whitespace-only fallback and returns only the
    /// edits that intersect the requested line range. Lines outside the
    /// range are left untouched.
    pub fn format_range(
        source_text: &str,
        range: Range,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let lines: Vec<&str> = source_text.split('\n').collect();
        if lines.is_empty() {
            return Ok(vec![]);
        }
        let start_line = range.start.line as usize;
        let end_line = (range.end.line as usize).min(lines.len().saturating_sub(1));
        if start_line > end_line || start_line >= lines.len() {
            return Ok(vec![]);
        }

        // Apply whitespace-only normalization to the full document so that
        // `compute_line_edits` can still emit minimal per-line edits, then
        // keep only edits that intersect the requested line range.
        let range_options = FormattingOptions {
            // A range-format request must not force a final newline on the
            // whole document; leave EOF alone.
            insert_final_newline: Some(false),
            trim_final_newlines: Some(false),
            ..options.clone()
        };
        let formatted = Self::safe_whitespace_text(source_text, &range_options);
        let mut edits = Self::compute_line_edits(source_text, &formatted)?;
        edits.retain(|edit| {
            let edit_start = edit.range.start.line as usize;
            let edit_end = edit.range.end.line as usize;
            edit_end >= start_line && edit_start <= end_line
        });
        Ok(edits)
    }

    // =========================================================================
    // Format on key support
    // =========================================================================

    /// Handle format-on-key trigger.
    ///
    /// `key` is the character that was typed (e.g. `";"`, `"\n"`, `"}"`).
    /// `line` and `_offset` are the 0-based position after the key was typed.
    ///
    /// In fallback mode (no external formatter) this is strictly
    /// whitespace-safe:
    /// - `";"`: trims trailing whitespace on the current line.
    /// - `"\n"`: trims trailing whitespace on the previous line.
    /// - `"}"`: no edits. Re-indentation on close-brace requires a parser.
    /// - any other key: no edits.
    ///
    /// This intentionally does **not** remove double semicolons, insert
    /// indentation, or adjust brace placement — those would require syntax
    /// awareness. See [`FallbackFormattingMode`].
    pub fn format_on_key(
        source_text: &str,
        line: u32,
        _offset: u32,
        key: &str,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        match key {
            ";" => Self::trim_trailing_whitespace_on_line(source_text, line, options),
            "\n" => {
                if line == 0 {
                    return Ok(vec![]);
                }
                Self::trim_trailing_whitespace_on_line(source_text, line - 1, options)
            }
            _ => Ok(vec![]),
        }
    }

    /// Return an edit that trims trailing whitespace on `line`, or no edit
    /// if the line already has no trailing whitespace / trimming is disabled.
    fn trim_trailing_whitespace_on_line(
        source_text: &str,
        line: u32,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        if !options.trim_trailing_whitespace.unwrap_or(true) {
            return Ok(vec![]);
        }
        let lines: Vec<&str> = source_text.split('\n').collect();
        let line_idx = line as usize;
        let Some(current_line_with_cr) = lines.get(line_idx) else {
            return Ok(vec![]);
        };
        // Treat a CR at end of line as part of trailing whitespace; but
        // don't edit it here to avoid changing line-ending style.
        let current_line = current_line_with_cr
            .strip_suffix('\r')
            .unwrap_or(current_line_with_cr);
        let trimmed = current_line.trim_end_matches([' ', '\t']);
        if trimmed.len() == current_line.len() {
            return Ok(vec![]);
        }
        Ok(vec![TextEdit::new(
            Range::new(
                Position::new(line, trimmed.len() as u32),
                Position::new(line, current_line.len() as u32),
            ),
            String::new(),
        )])
    }
}

/// Compute the end position of a document (the position immediately past the
/// last character). Used to build a whole-document replacement range.
fn document_end_position(source: &str) -> Position {
    let mut line: u32 = 0;
    let mut character: u32 = 0;
    for ch in source.chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position::new(line, character)
}

#[cfg(test)]
#[path = "../tests/formatting_tests.rs"]
mod formatting_tests;
