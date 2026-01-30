//! SourceWriter - Abstraction for writing emitter output with source map tracking
//!
//! This module separates the concerns of:
//! - Writing text to an output buffer
//! - Tracking line/column positions for source maps
//! - Managing indentation
//!
//! The emitter (Printer) delegates all text output to SourceWriter,
//! allowing for accurate source map generation and cleaner separation of concerns.

use crate::emitter::NewLineKind;
use crate::source_map::{Mapping, SourceMapGenerator};

/// A source position from the original AST
#[derive(Debug, Clone, Copy, Default)]
pub struct SourcePosition {
    /// Byte offset in original source (from Node.pos)
    pub pos: u32,
    /// Line number (0-indexed, computed from source text)
    pub line: u32,
    /// Column number (0-indexed, computed from source text)
    pub column: u32,
}

/// Writer that handles output generation and source map tracking.
///
/// This abstraction separates text generation from AST traversal,
/// enabling accurate source maps for all emitted code including transforms.
pub struct SourceWriter {
    /// Output buffer
    output: String,

    /// Current output line (0-indexed)
    line: u32,

    /// Current output column (0-indexed)
    column: u32,

    /// Current indentation level
    indent_level: u32,

    /// Whether we're at the start of a line (for lazy indentation)
    at_line_start: bool,

    /// Indentation string (e.g., "    " for 4 spaces)
    indent_str: String,

    /// New line string ("\n" or "\r\n")
    new_line: String,

    /// Optional source map generator
    source_map: Option<SourceMapGenerator>,

    /// Current source file index (for source maps)
    current_source_index: u32,
}

impl SourceWriter {
    /// Create a new SourceWriter with default settings
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    /// Create a SourceWriter with pre-allocated capacity
    /// This reduces allocations when the expected output size is known
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            output: String::with_capacity(capacity),
            line: 0,
            column: 0,
            indent_level: 0,
            at_line_start: true,
            indent_str: "    ".to_string(),
            new_line: "\n".to_string(),
            source_map: None,
            current_source_index: 0,
        }
    }

    /// Create a SourceWriter with source map generation enabled
    pub fn with_source_map(output_file: String) -> Self {
        let mut writer = Self::new();
        writer.source_map = Some(SourceMapGenerator::new(output_file));
        writer
    }

    /// Enable source map generation on an existing writer.
    pub fn enable_source_map(&mut self, output_file: String) {
        if self.source_map.is_none() {
            self.source_map = Some(SourceMapGenerator::new(output_file));
        }
    }

    /// Check if source map generation is enabled.
    pub fn has_source_map(&self) -> bool {
        self.source_map.is_some()
    }

    /// Set the indentation string
    pub fn set_indent_str(&mut self, indent: &str) {
        self.indent_str = indent.to_string();
    }

    /// Set the new line kind
    pub fn set_new_line_kind(&mut self, kind: NewLineKind) {
        self.new_line = match kind {
            NewLineKind::LineFeed => "\n".to_string(),
            NewLineKind::CarriageReturnLineFeed => "\r\n".to_string(),
        };
    }

    /// Add a source file to the source map and set it as current
    pub fn add_source(&mut self, source_name: String, content: Option<String>) -> u32 {
        if let Some(ref mut sm) = self.source_map {
            let idx = if let Some(c) = content {
                sm.add_source_with_content(source_name, c)
            } else {
                sm.add_source(source_name)
            };
            self.current_source_index = idx;
            idx
        } else {
            0
        }
    }

    // =========================================================================
    // Core Write Methods
    // =========================================================================

    /// Write text to output (syntax glue, no source mapping)
    pub fn write(&mut self, text: &str) {
        self.ensure_indent();
        self.raw_write(text);
    }

    /// Write text derived from a source node (maps to original position)
    pub fn write_node(&mut self, text: &str, source_pos: SourcePosition) {
        self.ensure_indent();

        // Add source map mapping before writing
        if let Some(ref mut sm) = self.source_map {
            sm.add_simple_mapping(
                self.line,
                self.column,
                self.current_source_index,
                source_pos.line,
                source_pos.column,
            );
        }

        self.raw_write(text);
    }

    /// Write an unsigned integer derived from a source node (maps to original position).
    pub fn write_node_usize(&mut self, value: usize, source_pos: SourcePosition) {
        self.ensure_indent();

        if let Some(ref mut sm) = self.source_map {
            sm.add_simple_mapping(
                self.line,
                self.column,
                self.current_source_index,
                source_pos.line,
                source_pos.column,
            );
        }

        self.raw_write_usize_digits(value);
    }

    /// Write text with a name reference (for identifiers)
    pub fn write_node_with_name(&mut self, text: &str, source_pos: SourcePosition, name: &str) {
        self.ensure_indent();

        if let Some(ref mut sm) = self.source_map {
            let name_idx = sm.add_name(name.to_string());
            sm.add_mapping(
                self.line,
                self.column,
                self.current_source_index,
                source_pos.line,
                source_pos.column,
                Some(name_idx),
            );
        }

        self.raw_write(text);
    }

    /// Write a single character
    pub fn write_char(&mut self, ch: char) {
        self.ensure_indent();
        self.raw_write_char(ch);
    }

    /// Write a newline
    pub fn write_line(&mut self) {
        self.output.push_str(&self.new_line);
        self.line += 1;
        self.column = 0;
        self.at_line_start = true;
    }

    /// Write a space
    pub fn write_space(&mut self) {
        self.write(" ");
    }

    /// Write an unsigned integer without allocating.
    pub fn write_usize(&mut self, value: usize) {
        self.ensure_indent();

        self.raw_write_usize_digits(value);
    }

    // =========================================================================
    // Indentation
    // =========================================================================

    /// Increase indentation level
    pub fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation level
    pub fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Get current indentation level
    pub fn indent_level(&self) -> u32 {
        self.indent_level
    }

    /// Set indentation level directly (for transforms that manage their own indentation)
    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Get the current indentation width in columns.
    pub fn indent_width(&self) -> u32 {
        self.indent_level
            .saturating_mul(self.indent_str.len() as u32)
    }

    // =========================================================================
    // Position Tracking
    // =========================================================================

    /// Get current output line (0-indexed)
    pub fn current_line(&self) -> u32 {
        self.line
    }

    /// Get current output column (0-indexed)
    pub fn current_column(&self) -> u32 {
        self.column
    }

    /// Get current source index for source map entries.
    pub fn current_source_index(&self) -> u32 {
        self.current_source_index
    }

    /// Check if we're at the start of a line
    pub fn is_at_line_start(&self) -> bool {
        self.at_line_start
    }

    // =========================================================================
    // Output Access
    // =========================================================================

    /// Get the output as a string slice
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Take ownership of the output string
    pub fn take_output(self) -> String {
        self.output
    }

    /// Get the output length in bytes
    pub fn len(&self) -> usize {
        self.output.len()
    }

    /// Get the output buffer capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.output.capacity()
    }

    /// Ensure the output buffer can hold at least `capacity` bytes without reallocating.
    pub fn ensure_output_capacity(&mut self, capacity: usize) {
        let current = self.output.capacity();
        if current < capacity {
            let len = self.output.len();
            if capacity > len {
                self.output.reserve(capacity - len);
            }
        }
    }

    /// Check if output is empty
    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }

    /// Take the source map generator (if any)
    pub fn take_source_map(self) -> Option<SourceMapGenerator> {
        self.source_map
    }

    /// Generate source map JSON (if source mapping is enabled)
    pub fn generate_source_map_json(&mut self) -> Option<String> {
        self.source_map.as_mut().map(|sm| sm.generate_json())
    }

    /// Add mappings with a base line/column offset. Column offset applies only to the first line.
    pub fn add_offset_mappings(&mut self, base_line: u32, base_column: u32, mappings: &[Mapping]) {
        let Some(ref mut sm) = self.source_map else {
            return;
        };

        for mapping in mappings {
            let line = base_line + mapping.generated_line;
            let column = if mapping.generated_line == 0 {
                base_column + mapping.generated_column
            } else {
                mapping.generated_column
            };
            sm.add_mapping(
                line,
                column,
                mapping.source_index,
                mapping.original_line,
                mapping.original_column,
                mapping.name_index,
            );
        }
    }

    /// Add mappings with a base line offset and a column offset applied to every line.
    pub fn add_mappings_with_line_column_offset(
        &mut self,
        base_line: u32,
        column_offset: u32,
        mappings: &[Mapping],
    ) {
        let Some(ref mut sm) = self.source_map else {
            return;
        };

        for mapping in mappings {
            let line = base_line + mapping.generated_line;
            let column = column_offset + mapping.generated_column;
            sm.add_mapping(
                line,
                column,
                mapping.source_index,
                mapping.original_line,
                mapping.original_column,
                mapping.name_index,
            );
        }
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Ensure indentation is written if we're at line start
    #[inline(always)]
    fn ensure_indent(&mut self) {
        if self.at_line_start && self.indent_level > 0 {
            for _ in 0..self.indent_level {
                self.output.push_str(&self.indent_str);
                self.column += self.indent_str.len() as u32;
            }
            self.at_line_start = false;
        } else if self.at_line_start {
            self.at_line_start = false;
        }
    }

    /// Raw write - updates position tracking, no indent handling
    /// Note: Column counting uses UTF-16 code units for source map compatibility
    ///
    /// Optimized using memchr for SIMD newline search and ASCII fast-path
    fn raw_write(&mut self, text: &str) {
        self.output.push_str(text);

        let bytes = text.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            match memchr::memchr(b'\n', &bytes[i..]) {
                Some(offset) => {
                    // Update column for text before newline
                    let segment_end = i + offset;
                    let segment = &text[i..segment_end];

                    if segment.is_ascii() {
                        // Fast path: ASCII strings have 1:1 byte-to-UTF16 mapping
                        self.column += segment.len() as u32;
                    } else {
                        // Slow path: Count UTF-16 code units properly
                        self.column += segment.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                    }

                    // Handle newline
                    self.line += 1;
                    self.column = 0;
                    i = segment_end + 1;
                }
                None => {
                    // No more newlines, just update column for remaining text
                    let segment = &text[i..];

                    if segment.is_ascii() {
                        self.column += segment.len() as u32;
                    } else {
                        self.column += segment.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                    }
                    break;
                }
            }
        }
    }

    fn raw_write_usize_digits(&mut self, mut value: usize) {
        if value == 0 {
            self.raw_write_char('0');
            return;
        }

        let mut buf = [0u8; 20];
        let mut i = buf.len();
        while value > 0 {
            let digit = (value % 10) as u8;
            i -= 1;
            buf[i] = b'0' + digit;
            value /= 10;
        }

        for &b in &buf[i..] {
            self.raw_write_char(b as char);
        }
    }

    /// Raw write single char - updates position tracking
    /// Note: Column counting uses UTF-16 code units for source map compatibility
    fn raw_write_char(&mut self, ch: char) {
        if ch == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            // UTF-16 code units: non-BMP characters (emojis etc.) count as 2
            self.column += ch.len_utf16() as u32;
        }
        self.output.push(ch);
    }
}

impl Default for SourceWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/source_writer_tests.rs"]
mod source_writer_tests;

// =============================================================================
// Helper Functions
// =============================================================================

/// Compute line and column from byte offset in source text
/// Note: Column counting uses UTF-16 code units for source map compatibility
pub fn compute_line_col(text: &str, pos: u32) -> (u32, u32) {
    let pos = pos as usize;
    if pos >= text.len() {
        // Return end of file position
        let line = text.matches('\n').count() as u32;
        let last_newline = text.rfind('\n').map(|i| i + 1).unwrap_or(0);
        // Count UTF-16 code units in the last line
        let col = text[last_newline..]
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum();
        return (line, col);
    }

    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in text.char_indices() {
        if i >= pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            // UTF-16 code units: non-BMP characters (emojis etc.) count as 2
            col += ch.len_utf16() as u32;
        }
    }

    (line, col)
}

/// Create a SourcePosition from a byte offset and source text
pub fn source_position_from_offset(text: &str, pos: u32) -> SourcePosition {
    let (line, column) = compute_line_col(text, pos);
    SourcePosition { pos, line, column }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_write() {
        let mut writer = SourceWriter::new();
        writer.write("hello");
        writer.write(" ");
        writer.write("world");
        assert_eq!(writer.get_output(), "hello world");
    }

    #[test]
    fn test_newline_tracking() {
        let mut writer = SourceWriter::new();
        writer.write("line 1");
        writer.write_line();
        writer.write("line 2");

        assert_eq!(writer.current_line(), 1);
        assert_eq!(writer.get_output(), "line 1\nline 2");
    }

    #[test]
    fn test_indentation() {
        let mut writer = SourceWriter::new();
        writer.write("start");
        writer.write_line();
        writer.increase_indent();
        writer.write("indented");
        writer.write_line();
        writer.decrease_indent();
        writer.write("back");

        assert_eq!(writer.get_output(), "start\n    indented\nback");
    }

    #[test]
    fn test_compute_line_col() {
        let text = "line1\nline2\nline3";

        assert_eq!(compute_line_col(text, 0), (0, 0)); // 'l' of line1
        assert_eq!(compute_line_col(text, 5), (0, 5)); // '\n' after line1
        assert_eq!(compute_line_col(text, 6), (1, 0)); // 'l' of line2
        assert_eq!(compute_line_col(text, 12), (2, 0)); // 'l' of line3
    }
}
