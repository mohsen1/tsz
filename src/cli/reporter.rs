use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;

use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
};
use crate::lsp::position::LineMap;

pub struct Reporter {
    pretty: bool,
    color: bool,
    cwd: Option<String>,
    sources: HashMap<String, String>,
    line_maps: HashMap<String, LineMap>,
}

impl Reporter {
    pub fn new(color: bool) -> Self {
        Reporter {
            pretty: color,
            color,
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned()),
            sources: HashMap::new(),
            line_maps: HashMap::new(),
        }
    }

    /// Set whether pretty mode is enabled (source snippets, colon-separated locations).
    /// By default, pretty mode matches the color setting.
    pub fn set_pretty(&mut self, pretty: bool) {
        self.pretty = pretty;
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
            out.push_str(&format!("{}({},{})", file_display, line, col));
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
        out.push_str(&diagnostic.message_text);

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
                out.push_str(&file_display.cyan().to_string());
                out.push(':');
                out.push_str(&line.to_string().yellow().to_string());
                out.push(':');
                out.push_str(&col.to_string().yellow().to_string());
            } else {
                out.push_str(&format!("{}:{}:{}", file_display, line, col));
            }
        } else if !diagnostic.file.is_empty() {
            if self.color {
                out.push_str(&file_display.cyan().to_string());
            } else {
                out.push_str(&file_display);
            }
        }

        out.push_str(" - ");
        out.push_str(&self.format_category_label(diagnostic.category));

        if diagnostic.code != 0 {
            if self.color {
                out.push_str(&format!(" TS{}: ", diagnostic.code).dimmed().to_string());
            } else {
                out.push_str(&format!(" TS{}: ", diagnostic.code));
            }
        } else {
            out.push_str(": ");
        }
        out.push_str(&diagnostic.message_text);

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
            snippet.push_str(&underline.red().to_string());
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
    fn format_related_plain(&mut self, out: &mut String, related: &DiagnosticRelatedInformation) {
        let file_display = self.relative_path(&related.file);
        if let Some((line, col)) = self.position_for(&related.file, related.start) {
            out.push_str(&format!("  {}({},{})", file_display, line, col));
        } else if !related.file.is_empty() {
            out.push_str(&format!("  {}", file_display));
        }
        out.push_str(": ");
        out.push_str(&related.message_text);
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
                out.push_str(&file_display.cyan().to_string());
                out.push(':');
                out.push_str(&line.to_string().yellow().to_string());
                out.push(':');
                out.push_str(&col.to_string().yellow().to_string());
            } else {
                out.push_str(&format!("{}:{}:{}", file_display, line, col));
            }
        } else if !related.file.is_empty() {
            if self.color {
                out.push_str(&file_display.cyan().to_string());
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
        out.push_str(&related.message_text);
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
            snippet.push_str(&underline.cyan().to_string());
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
        let mut seen_files: HashMap<String, usize> = HashMap::new();

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
        let mut first_error_lines: HashMap<String, u32> = HashMap::new();
        for diag in diagnostics {
            if diag.category != DiagnosticCategory::Error {
                continue;
            }
            let file_display = self.relative_path(&diag.file);
            if !first_error_lines.contains_key(&file_display) {
                if let Some((line, _)) = self.line_maps.get(&diag.file).and_then(|lm| {
                    let source = self.sources.get(&diag.file)?;
                    let pos = lm.offset_to_position(diag.start, source);
                    Some((pos.line + 1, pos.character + 1))
                }) {
                    first_error_lines.insert(file_display, line);
                }
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
                        format!(":{}", first_line).dimmed()
                    ));
                } else {
                    out.push_str(&format!("Found 1 error in {}:{}\n", file, first_line));
                }
            } else {
                // "Found N errors in the same file, starting at: file:line\n\n"
                if self.color {
                    out.push_str(&format!(
                        "Found {} errors in the same file, starting at: {}{}\n",
                        error_count,
                        file,
                        format!(":{}", first_line).dimmed()
                    ));
                } else {
                    out.push_str(&format!(
                        "Found {} errors in the same file, starting at: {}:{}\n",
                        error_count, file, first_line
                    ));
                }
            }
            // tsc adds a trailing blank line after single-file summaries
            out.push('\n');
        } else {
            // "Found N errors in M files." + file table (no trailing blank line)
            out.push_str(&format!(
                "Found {} {} in {} files.",
                error_count, error_word, unique_file_count
            ));
            out.push('\n');
            out.push('\n');

            // "Errors  Files" table
            out.push_str("Errors  Files");

            for (file, count) in &file_errors {
                let first_line = first_error_lines.get(file).copied().unwrap_or(1);
                out.push('\n');
                out.push_str(&format!("{:>6}  {}:{}", count, file, first_line));
            }
            out.push('\n');
        }
    }

    /// Get a file path relative to cwd (matching tsc behavior).
    fn relative_path(&self, file: &str) -> String {
        if file.is_empty() {
            return file.to_string();
        }

        if let Some(ref cwd) = self.cwd {
            let file_path = Path::new(file);
            let cwd_path = Path::new(cwd);
            if let Ok(relative) = file_path.strip_prefix(cwd_path) {
                return relative.to_string_lossy().into_owned();
            }
        }

        file.to_string()
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
            let contents = std::fs::read_to_string(path).ok()?;
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
            DiagnosticCategory::Error => label.red().bold().to_string(),
            DiagnosticCategory::Warning => label.yellow().bold().to_string(),
            DiagnosticCategory::Suggestion => label.blue().bold().to_string(),
            DiagnosticCategory::Message => label.cyan().bold().to_string(),
        }
    }

    fn format_code_label(&self, code: u32) -> String {
        if code == 0 {
            return String::new();
        }

        let label = format!("TS{}", code);
        if self.color {
            label.bright_blue().to_string()
        } else {
            label
        }
    }
}
