use colored::Colorize;
use rustc_hash::FxHashMap;
use std::path::{Component, Path, PathBuf};

use crate::locale;
use tsz::checker::diagnostics::{Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation};
use tsz::lsp::position::LineMap;

pub struct Reporter {
    pretty: bool,
    color: bool,
    cwd: Option<String>,
    sources: FxHashMap<String, String>,
    line_maps: FxHashMap<String, LineMap>,
}

impl Reporter {
    pub fn new(color: bool) -> Self {
        Self {
            pretty: color,
            color,
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned()),
            sources: FxHashMap::default(),
            line_maps: FxHashMap::default(),
        }
    }

    /// Set whether pretty mode is enabled (source snippets, colon-separated locations).
    /// By default, pretty mode matches the color setting.
    pub const fn set_pretty(&mut self, pretty: bool) {
        self.pretty = pretty;
    }

    /// Override the working directory used for computing relative paths.
    pub fn set_cwd(&mut self, cwd: &Path) {
        self.cwd = Some(cwd.to_string_lossy().into_owned());
    }

    /// Force color output regardless of TTY detection.
    /// Call this when `--pretty true` is explicitly passed so that ANSI codes
    /// are emitted even when piped (matching tsc v6 behavior).
    pub fn force_colors(enabled: bool) {
        colored::control::set_override(enabled);
    }

    /// Render all diagnostics, matching tsc output format exactly.
    pub fn render(&mut self, diagnostics: &[Diagnostic]) -> String {
        let mut out = String::new();

        if self.pretty {
            self.render_pretty(&mut out, diagnostics);
        } else {
            self.render_plain(&mut out, diagnostics);
        }

        out
    }

    /// Render diagnostics in non-pretty mode (--pretty false).
    /// Format: `file(line,col): error TScode: message`
    /// No source snippets, no summary line.
    fn render_plain(&mut self, out: &mut String, diagnostics: &[Diagnostic]) {
        for (index, diagnostic) in diagnostics.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            self.format_diagnostic_plain(out, diagnostic);
        }
        // tsc always ends non-pretty output with a newline
        if !diagnostics.is_empty() {
            out.push('\n');
        }
    }

    /// Render diagnostics in pretty mode (--pretty true / default with terminal).
    /// Format: `file:line:col - error TScode: message` + source snippet + summary.
    fn render_pretty(&mut self, out: &mut String, diagnostics: &[Diagnostic]) {
        for diagnostic in diagnostics {
            self.format_diagnostic_pretty(out, diagnostic);
            // Each diagnostic ends with a trailing blank line (tsc format)
            out.push('\n');
            out.push('\n');
        }

        // Summary line (preceded by extra blank line = two blank lines after last diagnostic)
        if !diagnostics.is_empty() {
            out.push('\n');
            self.format_summary(out, diagnostics);
        }
    }

    /// Format a single diagnostic in non-pretty mode.
    /// `file(line,col): error TScode: message`
    fn format_diagnostic_plain(&mut self, out: &mut String, diagnostic: &Diagnostic) {
        let file_display = self.relative_path(&diagnostic.file);

        if let Some((line, col)) = self.position_for(&diagnostic.file, diagnostic.start) {
            out.push_str(&format!("{file_display}({line},{col})"));
        } else if !diagnostic.file.is_empty() {
            out.push_str(&file_display);
        }

        out.push_str(": ");
        out.push_str(&self.format_category_label(diagnostic.category));
        if diagnostic.code != 0 {
            out.push(' ');
            out.push_str(&self.format_code_label(diagnostic.code));
        }
        out.push_str(": ");
        // Translate message using current locale if available
        let message = self.translate_message(diagnostic.code, &diagnostic.message_text);
        out.push_str(&message);

        // Non-pretty: related info is shown inline
        for related in &diagnostic.related_information {
            out.push('\n');
            self.format_related_plain(out, related);
        }
    }

    /// Format a single diagnostic in pretty mode.
    /// ```text
    /// file:line:col - error TScode: message
    ///
    /// {line_num} {source_line}
    /// {spaces}{tildes}
    /// ```
    fn format_diagnostic_pretty(&mut self, out: &mut String, diagnostic: &Diagnostic) {
        let file_display = self.relative_path(&diagnostic.file);

        // Header line: file:line:col - error TScode: message
        if let Some((line, col)) = self.position_for(&diagnostic.file, diagnostic.start) {
            if self.color {
                out.push_str(&file_display.bright_cyan().to_string());
                out.push(':');
                out.push_str(&line.to_string().bright_yellow().to_string());
                out.push(':');
                out.push_str(&col.to_string().bright_yellow().to_string());
            } else {
                out.push_str(&format!("{file_display}:{line}:{col}"));
            }
        } else if !diagnostic.file.is_empty() {
            if self.color {
                out.push_str(&file_display.bright_cyan().to_string());
            } else {
                out.push_str(&file_display);
            }
        }

        out.push_str(" - ");
        out.push_str(&self.format_category_label(diagnostic.category));

        if diagnostic.code != 0 {
            if self.color {
                out.push_str(
                    &format!(" TS{}: ", diagnostic.code)
                        .bright_black()
                        .to_string(),
                );
            } else {
                out.push_str(&format!(" TS{}: ", diagnostic.code));
            }
        } else {
            out.push_str(": ");
        }
        // Translate message using current locale if available
        let message = self.translate_message(diagnostic.code, &diagnostic.message_text);
        out.push_str(&message);

        // Source snippet
        if let Some(snippet) =
            self.format_snippet_pretty(&diagnostic.file, diagnostic.start, diagnostic.length, 0)
        {
            out.push('\n');
            out.push_str(&snippet);
        }

        // Related information
        for related in &diagnostic.related_information {
            out.push('\n');
            self.format_related_pretty(out, related);
        }
    }

    /// Format a source code snippet in pretty mode, matching tsc's format exactly.
    /// ```text
    /// {line_num} {source_line}
    /// {spaces}{tildes}
    /// ```
    /// The `indent` parameter adds leading spaces (used for related info: 4 spaces).
    fn format_snippet_pretty(
        &mut self,
        file: &str,
        start: u32,
        length: u32,
        indent: usize,
    ) -> Option<String> {
        if file.is_empty() || length == 0 {
            return None;
        }

        let (line_num, column) = self.position_for(file, start)?;
        let source = self.sources.get(file)?;

        // Get the line containing the error
        let lines: Vec<&str> = source.lines().collect();
        let line_idx = (line_num - 1) as usize;
        if line_idx >= lines.len() {
            return None;
        }

        let line_text = lines[line_idx];
        let indent_str = " ".repeat(indent);
        let line_num_str = line_num.to_string();
        let line_num_width = line_num_str.len();

        // Source line: {indent}{line_num} {source_line}
        let mut snippet = String::new();
        // Empty line before source (tsc always has a blank line between header and source)
        snippet.push('\n');

        if self.color {
            snippet.push_str(&indent_str);
            // tsc uses reverse video for line numbers
            snippet.push_str(&line_num_str.reversed().to_string());
            snippet.push(' ');
            snippet.push_str(line_text);
            snippet.push('\n');

            // Underline line
            snippet.push_str(&indent_str);
            snippet.push_str(&" ".repeat(line_num_width).reversed().to_string());
            snippet.push(' ');

            let underline = self.build_underline(line_text, column, length);
            snippet.push_str(&underline.bright_red().to_string());
        } else {
            snippet.push_str(&indent_str);
            snippet.push_str(&line_num_str);
            snippet.push(' ');
            snippet.push_str(line_text);
            snippet.push('\n');

            // Underline line: spaces matching line_num width + space + column offset + tildes
            snippet.push_str(&indent_str);
            snippet.push_str(&" ".repeat(line_num_width));
            snippet.push(' ');

            let underline = self.build_underline(line_text, column, length);
            snippet.push_str(&underline);
        }

        Some(snippet)
    }

    /// Build the underline string (spaces + tildes) for a given column and length.
    /// Column is 1-indexed. The underline aligns within the source line.
    fn build_underline(&self, line_text: &str, column: u32, length: u32) -> String {
        let mut underline = String::new();
        let col_0 = (column - 1) as usize;

        for (i, ch) in line_text.chars().enumerate() {
            if i < col_0 {
                // Before the error span - pad with spaces (tabs expand to spaces)
                if ch == '\t' {
                    underline.push_str("    ");
                } else {
                    underline.push(' ');
                }
            } else if i < col_0 + length as usize {
                // Within the error span
                if ch == '\t' {
                    underline.push_str("~~~~");
                } else {
                    underline.push('~');
                }
            } else {
                break;
            }
        }

        // If underline is empty but we have a length, show at least one ~
        if underline.trim().is_empty() && length > 0 {
            underline = " ".repeat(col_0) + "~";
        }

        underline
    }

    /// Format related information in non-pretty mode.
    /// tsc format: `  message` (no file/line/code prefix — just indented text).
    fn format_related_plain(&mut self, out: &mut String, related: &DiagnosticRelatedInformation) {
        out.push_str("  ");
        let message = self.translate_message(related.code, &related.message_text);
        out.push_str(&message);
    }

    /// Format related information in pretty mode.
    /// ```text
    ///   file:line:col
    ///     {line_num} {source_line}
    ///     {spaces}{tildes}
    ///     message
    /// ```
    fn format_related_pretty(&mut self, out: &mut String, related: &DiagnosticRelatedInformation) {
        let file_display = self.relative_path(&related.file);

        // Location line (2-space indent)
        out.push_str("  ");
        if let Some((line, col)) = self.position_for(&related.file, related.start) {
            if self.color {
                out.push_str(&file_display.bright_cyan().to_string());
                out.push(':');
                out.push_str(&line.to_string().bright_yellow().to_string());
                out.push(':');
                out.push_str(&col.to_string().bright_yellow().to_string());
            } else {
                out.push_str(&format!("{file_display}:{line}:{col}"));
            }
        } else if !related.file.is_empty() {
            if self.color {
                out.push_str(&file_display.bright_cyan().to_string());
            } else {
                out.push_str(&file_display);
            }
        }

        // Source snippet (4-space indent)
        if let Some(snippet) =
            self.format_snippet_pretty_related(&related.file, related.start, related.length)
        {
            out.push_str(&snippet);
        }

        // Message (4-space indent)
        out.push('\n');
        out.push_str("    ");
        let message = self.translate_message(related.code, &related.message_text);
        out.push_str(&message);
    }

    /// Format snippet for related info with 4-space indent and cyan underline.
    fn format_snippet_pretty_related(
        &mut self,
        file: &str,
        start: u32,
        length: u32,
    ) -> Option<String> {
        if file.is_empty() || length == 0 {
            return None;
        }

        let (line_num, column) = self.position_for(file, start)?;
        let source = self.sources.get(file)?;

        let lines: Vec<&str> = source.lines().collect();
        let line_idx = (line_num - 1) as usize;
        if line_idx >= lines.len() {
            return None;
        }

        let line_text = lines[line_idx];
        let line_num_str = line_num.to_string();
        let line_num_width = line_num_str.len();

        let mut snippet = String::new();
        snippet.push('\n');

        if self.color {
            snippet.push_str("    ");
            snippet.push_str(&line_num_str.reversed().to_string());
            snippet.push(' ');
            snippet.push_str(line_text);
            snippet.push('\n');

            snippet.push_str("    ");
            snippet.push_str(&" ".repeat(line_num_width).reversed().to_string());
            snippet.push(' ');

            // Related info uses cyan underline (not red)
            let underline = self.build_underline(line_text, column, length);
            snippet.push_str(&underline.bright_cyan().to_string());
        } else {
            snippet.push_str("    ");
            snippet.push_str(&line_num_str);
            snippet.push(' ');
            snippet.push_str(line_text);
            snippet.push('\n');

            snippet.push_str("    ");
            snippet.push_str(&" ".repeat(line_num_width));
            snippet.push(' ');

            let underline = self.build_underline(line_text, column, length);
            snippet.push_str(&underline);
        }

        Some(snippet)
    }

    /// Format the error summary line at the end of pretty output, matching tsc exactly.
    fn format_summary(&self, out: &mut String, diagnostics: &[Diagnostic]) {
        let error_count = diagnostics
            .iter()
            .filter(|d| d.category == DiagnosticCategory::Error)
            .count();

        if error_count == 0 {
            return;
        }

        // Collect unique files that have errors and find first error line per file
        let mut file_errors: Vec<(String, u32)> = Vec::new();
        let mut seen_files: FxHashMap<String, usize> = FxHashMap::default();

        for diag in diagnostics {
            if diag.category != DiagnosticCategory::Error {
                continue;
            }
            let file_display = self.relative_path(&diag.file);
            if let Some(&idx) = seen_files.get(&file_display) {
                // Update count for existing file entry
                file_errors[idx].1 += 1;
            } else {
                seen_files.insert(file_display.clone(), file_errors.len());
                file_errors.push((file_display, 1));
            }
        }

        // Find the first error line per file for the summary
        let mut first_error_lines: FxHashMap<String, u32> = FxHashMap::default();
        for diag in diagnostics {
            if diag.category != DiagnosticCategory::Error {
                continue;
            }
            let file_display = self.relative_path(&diag.file);
            if let std::collections::hash_map::Entry::Vacant(entry) =
                first_error_lines.entry(file_display.clone())
                && let Some((line, _)) = self.line_maps.get(&diag.file).and_then(|lm| {
                    let source = self.sources.get(&diag.file)?;
                    let pos = lm.offset_to_position(diag.start, source);
                    Some((pos.line + 1, pos.character + 1))
                })
            {
                entry.insert(line);
            }
        }

        let error_word = if error_count == 1 { "error" } else { "errors" };
        let unique_file_count = file_errors.len();

        if unique_file_count == 1 {
            let (ref file, _count) = file_errors[0];
            let first_line = first_error_lines.get(file).copied().unwrap_or(1);

            if error_count == 1 {
                // "Found 1 error in file:line\n\n" (tsc adds trailing blank line)
                if self.color {
                    out.push_str(&format!(
                        "Found 1 error in {}{}\n",
                        file,
                        format!(":{first_line}").bright_black()
                    ));
                } else {
                    out.push_str(&format!("Found 1 error in {file}:{first_line}\n"));
                }
            } else {
                // "Found N errors in the same file, starting at: file:line\n\n"
                if self.color {
                    out.push_str(&format!(
                        "Found {} errors in the same file, starting at: {}{}\n",
                        error_count,
                        file,
                        format!(":{first_line}").bright_black()
                    ));
                } else {
                    out.push_str(&format!(
                        "Found {error_count} errors in the same file, starting at: {file}:{first_line}\n"
                    ));
                }
            }
            // tsc adds a trailing blank line after single-file summaries
            out.push('\n');
        } else {
            // "Found N errors in M files." + file table (no trailing blank line)
            out.push_str(&format!(
                "Found {error_count} {error_word} in {unique_file_count} files."
            ));
            out.push('\n');
            out.push('\n');

            // "Errors  Files" table
            out.push_str("Errors  Files");

            for (file, count) in &file_errors {
                let first_line = first_error_lines.get(file).copied().unwrap_or(1);
                out.push('\n');
                if self.color {
                    out.push_str(&format!(
                        "{count:>6}  {file}{}",
                        format!(":{first_line}").bright_black()
                    ));
                } else {
                    out.push_str(&format!("{count:>6}  {file}:{first_line}"));
                }
            }
            out.push('\n');
        }
    }

    /// Get a file path relative to cwd (matching tsc behavior).
    /// Produces `../../../path` for files outside cwd, just like tsc v6.
    fn relative_path(&self, file: &str) -> String {
        if file.is_empty() {
            return file.to_string();
        }
        if let Some(ref cwd) = self.cwd {
            let file_path = Path::new(file);
            let cwd_path = Path::new(cwd);
            if let Some(rel) = Self::diff_paths(file_path, cwd_path) {
                return rel.to_string_lossy().into_owned();
            }
        }
        file.to_string()
    }

    /// Compute a relative path from `base` to `path`, similar to `pathdiff::diff_paths`.
    fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
        let path_components: Vec<Component<'_>> = path.components().collect();
        let base_components: Vec<Component<'_>> = base.components().collect();
        let common_len = path_components
            .iter()
            .zip(base_components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        if common_len == 0 && path.is_absolute() && base.is_absolute() {
            return None;
        }
        let mut result = PathBuf::new();
        for _ in common_len..base_components.len() {
            result.push("..");
        }
        for component in &path_components[common_len..] {
            result.push(component);
        }
        Some(result)
    }

    fn position_for(&mut self, file: &str, offset: u32) -> Option<(u32, u32)> {
        self.ensure_source(file)?;
        if !self.line_maps.contains_key(file) {
            let source = self.sources.get(file)?;
            let map = LineMap::build(source);
            self.line_maps.insert(file.to_string(), map);
        }

        let source = self.sources.get(file)?;
        let line_map = self.line_maps.get(file)?;
        let position = line_map.offset_to_position(offset, source);
        Some((position.line + 1, position.character + 1))
    }

    fn ensure_source(&mut self, file: &str) -> Option<()> {
        if !self.sources.contains_key(file) {
            let path = Path::new(file);
            let bytes = std::fs::read(path).ok()?;
            let contents = decode_source_bytes(&bytes)?;
            self.sources.insert(file.to_string(), contents);
        }
        Some(())
    }

    fn format_category_label(&self, category: DiagnosticCategory) -> String {
        let label = match category {
            DiagnosticCategory::Error => "error",
            DiagnosticCategory::Warning => "warning",
            DiagnosticCategory::Suggestion => "suggestion",
            DiagnosticCategory::Message => "message",
        };

        if !self.color {
            return label.to_string();
        }

        match category {
            DiagnosticCategory::Error => label.bright_red().to_string(),
            DiagnosticCategory::Warning => label.bright_yellow().bold().to_string(),
            DiagnosticCategory::Suggestion => label.blue().bold().to_string(),
            DiagnosticCategory::Message => label.bright_cyan().bold().to_string(),
        }
    }

    fn format_code_label(&self, code: u32) -> String {
        if code == 0 {
            return String::new();
        }

        let label = format!("TS{code}");
        if self.color {
            label.bright_blue().to_string()
        } else {
            label
        }
    }

    /// Translate a diagnostic message using the current locale.
    ///
    /// If a locale is set and has a translation for the given code, returns
    /// the translated message. Otherwise returns the original message.
    fn translate_message(&self, code: u32, message: &str) -> String {
        locale::translate(code, message)
    }
}

/// Decode raw file bytes to a UTF-8 string, handling UTF-16 BOM-encoded files.
///
/// TypeScript test files may be encoded as UTF-16 LE or UTF-16 BE with a BOM.
/// `std::fs::read_to_string` only handles UTF-8, so files with other encodings
/// would fail to load, causing the reporter to omit position info from diagnostics.
fn decode_source_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= 2 {
        // UTF-16 LE BOM
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            let u16_words: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&u16_words));
        }
        // UTF-16 BE BOM
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            let u16_words: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&u16_words));
        }
    }
    // UTF-8 (with or without BOM)
    String::from_utf8(bytes.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_utf8() {
        let text = "hello world";
        assert_eq!(
            decode_source_bytes(text.as_bytes()),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn decode_utf16_le_bom() {
        let text = "AB";
        let mut bytes = vec![0xFF, 0xFE]; // UTF-16 LE BOM
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        assert_eq!(decode_source_bytes(&bytes), Some("AB".to_string()));
    }

    #[test]
    fn decode_utf16_be_bom() {
        let text = "AB";
        let mut bytes = vec![0xFE, 0xFF]; // UTF-16 BE BOM
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_be_bytes());
        }
        assert_eq!(decode_source_bytes(&bytes), Some("AB".to_string()));
    }

    #[test]
    fn decode_invalid_utf8_returns_none() {
        let bytes = vec![0xFF, 0x00, 0x80]; // Invalid UTF-8 without BOM
        assert_eq!(decode_source_bytes(&bytes), None);
    }

    #[test]
    fn decode_utf16_le_multiline() {
        let text = "line1\nline2\nline3";
        let mut bytes = vec![0xFF, 0xFE];
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        let decoded = decode_source_bytes(&bytes).unwrap();
        assert_eq!(decoded.lines().count(), 3);
        assert_eq!(decoded, text);
    }
}
