//! Document Formatting implementation for LSP.
//!
//! Provides code formatting capabilities for TypeScript files.
//! Delegates to external formatters (prettier, eslint) when available,
//! and falls back to an internal formatter that handles indentation,
//! semicolons, whitespace normalization, and common TS/JS patterns.
//!
//! Also provides format-on-key support for semicolon and newline triggers.

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
    pub const fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
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
    /// Returns a list of text edits to apply, or an error message.
    /// All positions in returned edits are 0-based (LSP convention).
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

        // No external formatter available - return internal formatting edits
        Self::apply_basic_formatting(source_text, options)
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

        // Compute per-line edits to avoid overlapping ranges
        Self::compute_line_edits(source_text, &formatted)
    }

    /// Format using eslint with --fix.
    #[cfg(not(target_arch = "wasm32"))]
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
            .map_err(|e| format!("Failed to spawn eslint: {e}"))?;

        let result = output
            .wait_with_output()
            .map_err(|e| format!("Failed to read eslint output: {e}"))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            if result.stdout.is_empty() {
                return Err(format!("ESLint failed: {stderr}"));
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

        // Emit edits from bottom-to-top so consumers that apply them
        // sequentially do not invalidate later ranges.
        edits.sort_by(|a, b| {
            b.range
                .start
                .line
                .cmp(&a.range.start.line)
                .then_with(|| b.range.start.character.cmp(&a.range.start.character))
        });

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
        let mut line_index = 0usize;
        while line_index < lines.len() {
            let line = lines[line_index];
            let trimmed = line.trim();

            // Skip empty lines - preserve them as-is (just trim whitespace)
            if trimmed.is_empty() {
                formatted_lines.push(String::new());
                line_index += 1;
                continue;
            }

            let mut structural_line = trimmed.to_string();
            if line_index + 1 < lines.len()
                && Self::can_precede_empty_block(trimmed)
                && lines[line_index + 1].trim() == "{}"
            {
                // Match tsserver-style formatting for compact empty blocks/bodies.
                structural_line = format!("{} {{ }}", Self::normalize_member_spacing(trimmed));
            }

            // Adjust indent before processing the line
            // Closing braces/brackets/parens reduce indent before the line
            let dedent_this_line = Self::line_starts_with_closing(&structural_line);
            let case_dedent = Self::is_case_or_default(&structural_line) && indent_level > 0;

            let effective_indent = if dedent_this_line {
                (indent_level - 1).max(0)
            } else if case_dedent {
                // case/default labels are indented one less than their body
                (indent_level - 1).max(0)
            } else {
                indent_level
            };

            // Build the formatted line
            let mut processed = structural_line.clone();

            // Trim trailing whitespace
            if options.trim_trailing_whitespace.unwrap_or(true) {
                processed = processed.trim_end().to_string();
            }

            // Normalize semicolons: ensure statements end with semicolons
            if options.semicolons.as_deref() != Some("remove") {
                processed = Self::normalize_semicolons(&processed);
            }
            processed = Self::normalize_member_spacing(&processed);
            processed = Self::normalize_as_operator_spacing(&processed);

            // Apply proper indentation
            let indent_prefix = indent_str.repeat(effective_indent as usize);
            let formatted_line = format!("{indent_prefix}{processed}");

            formatted_lines.push(formatted_line);

            // Adjust indent level for subsequent lines
            let opens = Self::count_openers(&structural_line);
            let closes = Self::count_closers(&structural_line);
            indent_level += opens - closes;
            indent_level = indent_level.max(0);

            if structural_line.ends_with(" { }") && line_index + 1 < lines.len() {
                line_index += 2;
            } else {
                line_index += 1;
            }
        }

        // Trim final empty lines if requested
        if options.trim_final_newlines.unwrap_or(true) {
            while formatted_lines
                .last()
                .is_some_and(std::string::String::is_empty)
            {
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
            || Self::looks_like_method_signature(trimmed)
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
            format!("{trimmed};")
        } else {
            trimmed.to_string()
        }
    }

    /// Normalize spacing around the `as` type-assertion operator.
    fn normalize_as_operator_spacing(line: &str) -> String {
        if !line.contains("as") {
            return line.to_string();
        }

        let mut out = String::with_capacity(line.len());
        let mut i = 0usize;
        while i < line.len() {
            let Some(ch) = line[i..].chars().next() else {
                break;
            };
            if !ch.is_whitespace() {
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }

            let ws_start = i;
            while let Some(next) = line[i..].chars().next() {
                if !next.is_whitespace() {
                    break;
                }
                i += next.len_utf8();
            }

            if line[i..].starts_with("as") {
                let as_end = i + 2;
                let mut after_as = as_end;
                while let Some(next) = line[after_as..].chars().next() {
                    if !next.is_whitespace() {
                        break;
                    }
                    after_as += next.len_utf8();
                }
                if after_as > as_end {
                    if !out.ends_with(' ') && !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str("as");
                    if after_as < line.len() {
                        out.push(' ');
                    }
                    i = after_as;
                    continue;
                }
            }

            out.push_str(&line[ws_start..i]);
        }

        out
    }

    fn looks_like_method_signature(trimmed: &str) -> bool {
        if !trimmed.ends_with(')') || !trimmed.contains('(') {
            return false;
        }
        trimmed.starts_with("public ")
            || trimmed.starts_with("private ")
            || trimmed.starts_with("protected ")
            || trimmed.starts_with("readonly ")
    }

    /// Returns true if a line can precede `{}` and should be merged into `... { }`.
    fn can_precede_empty_block(trimmed: &str) -> bool {
        // Method/function signatures ending with ')'
        if trimmed.ends_with(')') {
            return true;
        }
        // Generic signatures ending with '>'
        if trimmed.ends_with('>') {
            return true;
        }
        // Standalone keywords: else, do, try, finally
        if matches!(trimmed, "else" | "do" | "try" | "finally") {
            return true;
        }
        // catch(...) is covered by ends_with ')' above
        // Declarations: class/interface/enum/namespace/module (line ends with identifier)
        if trimmed.starts_with("class ")
            || trimmed.starts_with("interface ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("namespace ")
            || trimmed.starts_with("module ")
            || trimmed.starts_with("abstract class ")
            || trimmed.starts_with("export class ")
            || trimmed.starts_with("export interface ")
            || trimmed.starts_with("export enum ")
            || trimmed.starts_with("export default class")
            || trimmed.starts_with("declare class ")
            || trimmed.starts_with("declare interface ")
            || trimmed.starts_with("declare enum ")
            || trimmed.starts_with("declare namespace ")
            || trimmed.starts_with("declare module ")
        {
            return true;
        }
        false
    }

    fn normalize_member_spacing(line: &str) -> String {
        // Normalize multiple whitespace to single space, respecting strings.
        let mut out = Self::collapse_whitespace(line);
        // After collapsing, `( )` → `()`
        out = out.replace("( )", "()");
        // Remove space before semicolon: `foo ;` → `foo;`
        out = out.replace(" ;", ";");
        // Remove space before comma: `foo ,` → `foo,`
        out = out.replace(" ,", ",");
        // Remove space after `@` in decorators: `@ decorator` → `@decorator`
        if out.starts_with("@ ") {
            out = format!("@{}", out[2..].trim_start());
        }
        // Ensure space inside empty braces: `{}` → `{ }`
        out = out.replace("{}", "{ }");
        // Ensure space before opening brace (not at start of line): `foo{` → `foo {`
        out = Self::ensure_space_before_brace(&out);
        out
    }

    /// Ensure there's a space before `{` when preceded by non-whitespace,
    /// but not after `$` (template literal `${`).
    fn ensure_space_before_brace(line: &str) -> String {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut result = Vec::with_capacity(len + 4);
        let mut in_string: Option<u8> = None;
        let mut i = 0;

        while i < len {
            let ch = bytes[i];

            // Track string state
            if in_string.is_none() && (ch == b'\'' || ch == b'"' || ch == b'`') {
                in_string = Some(ch);
                result.push(ch);
                i += 1;
                continue;
            }
            if let Some(q) = in_string {
                if ch == b'\\' && i + 1 < len {
                    result.push(ch);
                    result.push(bytes[i + 1]);
                    i += 2;
                    continue;
                }
                if ch == q {
                    in_string = None;
                }
                result.push(ch);
                i += 1;
                continue;
            }

            if ch == b'{' && i > 0 {
                let prev = bytes[i - 1];
                // Add space before `{` if preceded by non-whitespace
                // but NOT after `$` (template literal `${`)
                if prev != b' ' && prev != b'\t' && prev != b'$' && prev != b'(' {
                    result.push(b' ');
                }
            }

            result.push(ch);
            i += 1;
        }

        String::from_utf8(result).unwrap_or_else(|_| line.to_string())
    }

    /// Collapse runs of whitespace to single spaces, but preserve whitespace
    /// inside string literals (single, double, backtick quotes).
    fn collapse_whitespace(line: &str) -> String {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut result = Vec::with_capacity(len);
        let mut i = 0;
        let mut in_whitespace_run = false;

        while i < len {
            let ch = bytes[i];

            // Handle string literals — copy them verbatim
            if ch == b'\'' || ch == b'"' || ch == b'`' {
                if in_whitespace_run {
                    result.push(b' ');
                    in_whitespace_run = false;
                }
                result.push(ch);
                i += 1;
                // Scan to matching close quote
                while i < len {
                    let c = bytes[i];
                    result.push(c);
                    if c == b'\\' && i + 1 < len {
                        // Escape sequence — copy next char too
                        i += 1;
                        result.push(bytes[i]);
                    } else if c == ch {
                        break;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            if ch == b' ' || ch == b'\t' {
                in_whitespace_run = true;
                i += 1;
            } else {
                if in_whitespace_run {
                    result.push(b' ');
                    in_whitespace_run = false;
                }
                result.push(ch);
                i += 1;
            }
        }

        // Don't add trailing space
        String::from_utf8(result).unwrap_or_else(|_| line.to_string())
    }

    /// Convert leading spaces to tabs based on tab size.
    pub fn convert_leading_spaces_to_tabs(line: &str, tab_size: usize) -> String {
        let leading_spaces = line.chars().take_while(|&c| c == ' ').count();
        let leading_tabs = leading_spaces / tab_size;
        let remaining_spaces = leading_spaces % tab_size;

        let rest = &line[leading_spaces..];
        let tabs = "\t".repeat(leading_tabs);
        let spaces = " ".repeat(remaining_spaces);

        format!("{tabs}{spaces}{rest}")
    }

    // =========================================================================
    // Range formatting
    // =========================================================================

    /// Format a specific range within a document.
    ///
    /// This implements the LSP `textDocument/rangeFormatting` request.
    /// Only lines that fall within the given range are reformatted;
    /// surrounding lines are left untouched.
    pub fn format_range(
        source_text: &str,
        range: Range,
        options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, String> {
        let lines: Vec<&str> = source_text.lines().collect();
        let start_line = range.start.line as usize;
        let end_line = (range.end.line as usize).min(lines.len().saturating_sub(1));

        if start_line > end_line || start_line >= lines.len() {
            return Ok(vec![]);
        }

        // Extract the range of lines and format them
        let range_text: String = lines[start_line..=end_line].join("\n");
        let formatted = Self::format_text(
            &range_text,
            &FormattingOptions {
                // Don't add final newline for range formatting
                insert_final_newline: Some(false),
                trim_final_newlines: Some(false),
                ..options.clone()
            },
        );

        let formatted_lines: Vec<&str> = formatted.lines().collect();

        let mut edits = Vec::new();
        let max_lines = lines[start_line..=end_line]
            .len()
            .min(formatted_lines.len());

        for i in 0..max_lines {
            let orig_line = lines[start_line + i];
            let fmt_line = formatted_lines.get(i).copied().unwrap_or("");
            if orig_line != fmt_line {
                edits.push(TextEdit::new(
                    Range::new(
                        Position::new((start_line + i) as u32, 0),
                        Position::new((start_line + i) as u32, orig_line.len() as u32),
                    ),
                    fmt_line.to_string(),
                ));
            }
        }

        // Handle line count differences
        let orig_count = end_line - start_line + 1;
        if formatted_lines.len() < orig_count {
            // Remove extra lines
            let last_fmt = formatted_lines.len().saturating_sub(1);
            let last_fmt_len = formatted_lines.last().map_or(0, |l| l.len()) as u32;
            edits.push(TextEdit::new(
                Range::new(
                    Position::new((start_line + last_fmt) as u32, last_fmt_len),
                    Position::new(end_line as u32, lines[end_line].len() as u32),
                ),
                String::new(),
            ));
        } else if formatted_lines.len() > orig_count {
            // Insert extra lines
            let extra: Vec<&str> = formatted_lines[orig_count..].to_vec();
            let mut new_text = String::new();
            for line in &extra {
                new_text.push('\n');
                new_text.push_str(line);
            }
            let end_char = lines[end_line].len() as u32;
            edits.push(TextEdit::new(
                Range::new(
                    Position::new(end_line as u32, end_char),
                    Position::new(end_line as u32, end_char),
                ),
                new_text,
            ));
        }

        Ok(edits)
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
            "}" => Self::format_on_closing_brace(source_text, line, options),
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
            let new_text = format!("{indent}{fixed}");
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

    /// Format after typing a closing brace `}`.
    /// Re-indents the current line to match the corresponding opening brace.
    fn format_on_closing_brace(
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

        // Only apply if the line is just a closing brace (with possible whitespace)
        if !trimmed.starts_with('}') {
            return Ok(vec![]);
        }

        let expected_indent = Self::compute_indent_for_line(lines.as_slice(), line_idx, options);
        let new_text = format!("{expected_indent}{trimmed}");

        if new_text != current_line {
            Ok(vec![TextEdit::new(
                Range::new(
                    Position::new(line, 0),
                    Position::new(line, current_line.len() as u32),
                ),
                new_text,
            )])
        } else {
            Ok(vec![])
        }
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
        let current_trimmed = lines.get(line_idx).map_or("", |l| l.trim());
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
            format!("{prev_indent}{indent_unit}")
        } else {
            prev_indent.to_string()
        }
    }
}

#[cfg(test)]
#[path = "../tests/formatting_tests.rs"]
mod formatting_tests;
