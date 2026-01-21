use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;

use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
};
use crate::lsp::position::LineMap;

pub struct Reporter {
    color: bool,
    sources: HashMap<String, String>,
    line_maps: HashMap<String, LineMap>,
}

impl Reporter {
    pub fn new(color: bool) -> Self {
        Reporter {
            color,
            sources: HashMap::new(),
            line_maps: HashMap::new(),
        }
    }

    pub fn render(&mut self, diagnostics: &[Diagnostic]) -> String {
        let mut out = String::new();
        for (index, diagnostic) in diagnostics.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(&self.format_diagnostic(diagnostic));
        }
        out
    }

    pub fn format_diagnostic(&mut self, diagnostic: &Diagnostic) -> String {
        let location = self.format_location(&diagnostic.file, diagnostic.start);
        let category = self.format_category(diagnostic.category);
        let code = self.format_code(diagnostic.code);

        let mut output = String::new();
        if let Some(location) = location {
            output.push_str(&location);
        } else if !diagnostic.file.is_empty() {
            output.push_str(&diagnostic.file);
        } else {
            output.push_str("<unknown>");
        }

        output.push_str(" - ");
        output.push_str(&category);
        if !code.is_empty() {
            output.push(' ');
            output.push_str(&code);
        }
        output.push_str(": ");
        output.push_str(&diagnostic.message_text);

        // Add source code snippet with error span underline (like tsc)
        if let Some(snippet) =
            self.format_snippet(&diagnostic.file, diagnostic.start, diagnostic.length)
        {
            output.push_str(&snippet);
        }

        if diagnostic.related_information.is_empty() {
            return output;
        }

        for related in &diagnostic.related_information {
            output.push('\n');
            output.push_str(&self.format_related(related));
            // Add source snippet for related information too
            if let Some(snippet) = self.format_snippet(&related.file, related.start, related.length)
            {
                output.push_str(&snippet);
            }
        }

        output
    }

    /// Format a source code snippet with error span underline, matching tsc's output format.
    /// Example:
    ///   2   let x: number = "string";
    ///         ~
    fn format_snippet(&mut self, file: &str, start: u32, length: u32) -> Option<String> {
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
        let end = start + length;

        // Build the underline
        // Start with spaces up to the error position
        let mut underline = String::new();
        for (i, ch) in line_text.chars().enumerate() {
            let offset = i as u32;
            if offset < column - 1 {
                if ch == '\t' {
                    underline.push_str("    "); // Tabs expand to 4 spaces
                } else {
                    underline.push(' ');
                }
            } else if offset >= column - 1 && offset < end {
                // Within the error span - use tildes (or ~ for single char)
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
        if underline.is_empty() && length > 0 {
            underline.push('~');
        }

        // Format: line number padded to 3 digits, then the source line, then the underline
        let mut snippet = String::new();
        snippet.push('\n');
        snippet.push_str(&format!("  {:>3}   {}", line_num, line_text));
        snippet.push('\n');

        // Add color to the underline in color mode
        let underline_display = if self.color {
            underline.red().to_string()
        } else {
            underline
        };
        snippet.push_str(&format!("       {}", underline_display));

        Some(snippet)
    }

    fn format_related(&mut self, related: &DiagnosticRelatedInformation) -> String {
        let location = self
            .format_location(&related.file, related.start)
            .unwrap_or_else(|| related.file.clone());
        let prefix = if self.color {
            "  Related".dimmed().to_string()
        } else {
            "  Related".to_string()
        };

        format!("{}: {} - {}", prefix, location, related.message_text)
    }

    fn format_location(&mut self, file: &str, offset: u32) -> Option<String> {
        if file.is_empty() {
            return None;
        }

        let (line, column) = self.position_for(file, offset)?;
        Some(format!("{}:{}:{}", file, line, column))
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

    fn format_category(&self, category: DiagnosticCategory) -> String {
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

    fn format_code(&self, code: u32) -> String {
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
