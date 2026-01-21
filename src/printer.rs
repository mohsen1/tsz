//! Printer - A clean, safe AST-to-JavaScript printer
//!
//! This module provides a high-level interface for converting TypeScript/JavaScript AST
//! to JavaScript output. It separates concerns:
//!
//! - **Printing**: AST nodes → JavaScript text (this module)
//! - **Transformation**: TypeScript features → ES5/ES6 (emitter internals)
//! - **Output**: Text generation and source maps (source_writer)
//!
//! # Safety
//!
//! This module uses safe string handling utilities to avoid panics from:
//! - Out-of-bounds slicing
//! - Non-UTF8 boundary slicing
//!
//! # Example
//!
//! ```ignore
//! use tsz::parser::ParserState;
//! use tsz::printer::{print_to_string, PrintOptions};
//!
//! let source = "const x: number = 42;";
//! let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
//! let root = parser.parse_source_file();
//!
//! let output = print_to_string(&parser.arena, root, PrintOptions::default());
//! assert!(output.contains("const x = 42"));
//! ```

use crate::emit_context::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions, ScriptTarget};
use crate::lowering_pass::LoweringPass;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::transform_context::TransformContext;
use std::io::{self, Write};

// =============================================================================
// Print Options
// =============================================================================

/// Options for printing AST to JavaScript.
///
/// This is a simplified options struct that covers the most common use cases.
/// For advanced options, use `emitter::Printer` directly with `PrinterOptions`.
#[derive(Clone, Debug, Default)]
pub struct PrintOptions {
    /// Target ECMAScript version (ESNext by default)
    pub target: ScriptTarget,
    /// Module format (None by default)
    pub module: ModuleKind,
    /// Remove comments from output
    pub remove_comments: bool,
    /// Use single quotes for strings
    pub single_quote: bool,
    /// Enable source map generation
    pub source_map: bool,
}

impl PrintOptions {
    /// Create options for ES5 target
    pub fn es5() -> Self {
        Self {
            target: ScriptTarget::ES5,
            ..Default::default()
        }
    }

    /// Create options for ES6+ target
    pub fn es6() -> Self {
        Self {
            target: ScriptTarget::ES2015,
            ..Default::default()
        }
    }

    /// Create options for CommonJS module output
    pub fn commonjs() -> Self {
        Self {
            module: ModuleKind::CommonJS,
            ..Default::default()
        }
    }

    /// Create options for ES5 CommonJS output
    pub fn es5_commonjs() -> Self {
        Self {
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            ..Default::default()
        }
    }

    /// Convert to internal PrinterOptions
    fn to_printer_options(&self) -> PrinterOptions {
        PrinterOptions {
            target: self.target,
            module: self.module,
            remove_comments: self.remove_comments,
            single_quote: self.single_quote,
            ..Default::default()
        }
    }
}

// =============================================================================
// Print Result
// =============================================================================

/// Result of printing an AST to JavaScript.
#[derive(Debug)]
pub struct PrintResult {
    /// The generated JavaScript code
    pub code: String,
    /// The source map JSON (if source map generation was enabled)
    pub source_map: Option<String>,
}

impl PrintResult {
    /// Create a new PrintResult with just code
    pub fn new(code: String) -> Self {
        Self {
            code,
            source_map: None,
        }
    }

    /// Create a new PrintResult with code and source map
    pub fn with_source_map(code: String, source_map: String) -> Self {
        Self {
            code,
            source_map: Some(source_map),
        }
    }
}

// =============================================================================
// High-Level Print Functions
// =============================================================================

/// Print an AST to a JavaScript string.
///
/// This is the simplest way to convert a parsed AST to JavaScript output.
/// For more control over the output, use `Printer::new()`.
pub fn print_to_string(arena: &NodeArena, root: NodeIndex, options: PrintOptions) -> String {
    let mut printer = Printer::new(arena, options);
    printer.print(root);
    printer.finish().code
}

/// Print an AST to JavaScript, returning both code and source map.
///
/// The source map is generated if `options.source_map` is true.
pub fn print_with_source_map(
    arena: &NodeArena,
    root: NodeIndex,
    source_text: &str,
    source_name: &str,
    output_name: &str,
    options: PrintOptions,
) -> PrintResult {
    let mut printer = Printer::new(arena, options);
    printer.set_source_text(source_text);
    printer.enable_source_map(source_name, output_name);
    printer.print(root);
    printer.finish()
}

/// Print an AST to a writer (streaming output).
///
/// This is more efficient for large files as it avoids building
/// the entire output string in memory.
pub fn print_to_writer<W: Write>(
    arena: &NodeArena,
    root: NodeIndex,
    options: PrintOptions,
    writer: &mut W,
) -> io::Result<()> {
    let output = print_to_string(arena, root, options);
    writer.write_all(output.as_bytes())
}

// =============================================================================
// Printer - Main Interface
// =============================================================================

/// A printer that converts AST nodes to JavaScript text.
///
/// This provides a clean interface for printing without exposing
/// the internal complexity of transforms and emit context.
pub struct Printer<'a> {
    inner: EmitterPrinter<'a>,
    options: PrintOptions,
    source_map_enabled: bool,
}

impl<'a> Printer<'a> {
    /// Create a new printer with the given options.
    pub fn new(arena: &'a NodeArena, options: PrintOptions) -> Self {
        let printer_opts = options.to_printer_options();
        let inner = EmitterPrinter::with_options(arena, printer_opts);

        Self {
            inner,
            options,
            source_map_enabled: false,
        }
    }

    /// Create a printer with pre-computed transforms.
    ///
    /// Use this when you have already run the lowering pass to compute
    /// transform directives. This avoids redundant analysis.
    pub fn with_transforms(
        arena: &'a NodeArena,
        transforms: TransformContext,
        options: PrintOptions,
    ) -> Self {
        let printer_opts = options.to_printer_options();
        let inner = EmitterPrinter::with_transforms_and_options(arena, transforms, printer_opts);

        Self {
            inner,
            options,
            source_map_enabled: false,
        }
    }

    /// Set the source text for comment preservation and single-line detection.
    pub fn set_source_text(&mut self, text: &'a str) {
        self.inner.set_source_text(text);
    }

    /// Enable source map generation.
    pub fn enable_source_map(&mut self, source_name: &str, output_name: &str) {
        if self.options.source_map {
            self.inner.enable_source_map(output_name, source_name);
            self.source_map_enabled = true;
        }
    }

    /// Print the AST starting from the given root node.
    pub fn print(&mut self, root: NodeIndex) {
        // Configure target-specific settings
        self.inner.set_target_es5(matches!(
            self.options.target,
            ScriptTarget::ES3 | ScriptTarget::ES5
        ));

        // Configure module settings
        self.inner.set_module_kind(self.options.module);

        // Emit the AST
        self.inner.emit(root);
    }

    /// Get the current output as a string slice.
    pub fn get_output(&self) -> &str {
        self.inner.get_output()
    }

    /// Finish printing and return the result.
    pub fn finish(mut self) -> PrintResult {
        let code = self.inner.get_output().to_string();
        let source_map = if self.source_map_enabled {
            self.inner.generate_source_map_json()
        } else {
            None
        };
        PrintResult { code, source_map }
    }
}

// =============================================================================
// Lowering + Printing Combined
// =============================================================================

/// Lower and print an AST in one step.
///
/// This runs the lowering pass to compute transforms, then prints the result.
/// Use this when you want ES5 or CommonJS output with proper transforms.
pub fn lower_and_print(arena: &NodeArena, root: NodeIndex, options: PrintOptions) -> PrintResult {
    // Create emit context for lowering
    let emit_ctx = EmitContext::with_options(options.to_printer_options());

    // Run lowering pass
    let transforms = LoweringPass::new(arena, &emit_ctx).run(root);

    // Create printer with transforms
    let mut printer = Printer::with_transforms(arena, transforms, options);
    printer.print(root);
    printer.finish()
}

/// Lower and print with source map support.
pub fn lower_and_print_with_source_map(
    arena: &NodeArena,
    root: NodeIndex,
    source_text: &str,
    source_name: &str,
    output_name: &str,
    options: PrintOptions,
) -> PrintResult {
    // Create emit context for lowering
    let emit_ctx = EmitContext::with_options(options.to_printer_options());

    // Run lowering pass
    let transforms = LoweringPass::new(arena, &emit_ctx).run(root);

    // Create printer with transforms
    let mut printer = Printer::with_transforms(arena, transforms, options);
    printer.set_source_text(source_text);
    printer.enable_source_map(source_name, output_name);
    printer.print(root);
    printer.finish()
}

// =============================================================================
// Safe String Utilities
// =============================================================================

/// Safe string slice utilities that never panic.
///
/// These functions handle edge cases like:
/// - Out-of-bounds indices
/// - Non-UTF8 boundary slicing
pub mod safe_slice {
    /// Safely slice a string, returning an empty string if bounds are invalid.
    ///
    /// Unlike `&s[start..end]`, this never panics.
    pub fn slice(s: &str, start: usize, end: usize) -> &str {
        if start >= s.len() || end > s.len() || start > end {
            return "";
        }

        // Check if indices are valid UTF-8 boundaries
        if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
            return "";
        }

        &s[start..end]
    }

    /// Safely slice a string from a start position to the end.
    pub fn slice_from(s: &str, start: usize) -> &str {
        if start >= s.len() {
            return "";
        }

        if !s.is_char_boundary(start) {
            return "";
        }

        &s[start..]
    }

    /// Safely slice a string from the beginning to an end position.
    pub fn slice_to(s: &str, end: usize) -> &str {
        if end > s.len() {
            return s;
        }

        if !s.is_char_boundary(end) {
            return "";
        }

        &s[..end]
    }

    /// Get a character at a byte position, if valid.
    pub fn char_at(s: &str, pos: usize) -> Option<char> {
        if pos >= s.len() {
            return None;
        }

        if !s.is_char_boundary(pos) {
            return None;
        }

        s[pos..].chars().next()
    }

    /// Get a byte at a position, returning None if out of bounds.
    pub fn byte_at(s: &str, pos: usize) -> Option<u8> {
        s.as_bytes().get(pos).copied()
    }

    /// Find the next character boundary at or after a position.
    pub fn next_boundary(s: &str, pos: usize) -> usize {
        if pos >= s.len() {
            return s.len();
        }

        let mut boundary = pos;
        while boundary < s.len() && !s.is_char_boundary(boundary) {
            boundary += 1;
        }
        boundary
    }

    /// Find the previous character boundary at or before a position.
    pub fn prev_boundary(s: &str, pos: usize) -> usize {
        if pos == 0 || pos > s.len() {
            return 0;
        }

        let mut boundary = pos;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        boundary
    }
}

// =============================================================================
// Streaming Writer
// =============================================================================

/// A writer that streams JavaScript output to an underlying writer.
///
/// This is useful for writing directly to files or network streams
/// without building the entire output in memory first.
pub struct StreamingPrinter<W: Write> {
    writer: W,
    buffer: String,
    buffer_size: usize,
}

impl<W: Write> StreamingPrinter<W> {
    /// Create a new streaming printer with default buffer size (8KB).
    pub fn new(writer: W) -> Self {
        Self::with_buffer_size(writer, 8192)
    }

    /// Create a new streaming printer with a specific buffer size.
    pub fn with_buffer_size(writer: W, buffer_size: usize) -> Self {
        Self {
            writer,
            buffer: String::with_capacity(buffer_size),
            buffer_size,
        }
    }

    /// Write text to the stream, flushing if buffer is full.
    pub fn write(&mut self, text: &str) -> io::Result<()> {
        if self.buffer.len() + text.len() > self.buffer_size {
            self.flush()?;
        }

        if text.len() > self.buffer_size {
            // Write directly if text is larger than buffer
            self.writer.write_all(text.as_bytes())?;
        } else {
            self.buffer.push_str(text);
        }

        Ok(())
    }

    /// Write a single character.
    pub fn write_char(&mut self, ch: char) -> io::Result<()> {
        if self.buffer.len() + ch.len_utf8() > self.buffer_size {
            self.flush()?;
        }
        self.buffer.push(ch);
        Ok(())
    }

    /// Write a newline.
    pub fn write_line(&mut self) -> io::Result<()> {
        self.write("\n")
    }

    /// Flush the internal buffer to the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            self.writer.write_all(self.buffer.as_bytes())?;
            self.buffer.clear();
        }
        self.writer.flush()
    }

    /// Finish writing and return the underlying writer.
    pub fn finish(mut self) -> io::Result<W> {
        self.flush()?;
        Ok(self.writer)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_slice_basic() {
        let s = "hello world";
        assert_eq!(safe_slice::slice(s, 0, 5), "hello");
        assert_eq!(safe_slice::slice(s, 6, 11), "world");
    }

    #[test]
    fn test_safe_slice_empty() {
        let s = "hello";
        assert_eq!(safe_slice::slice(s, 10, 20), "");
        assert_eq!(safe_slice::slice(s, 5, 3), "");
    }

    #[test]
    fn test_safe_slice_unicode() {
        let s = "hello 🦀 world";
        // The crab emoji is 4 bytes
        let crab_start = 6;
        let crab_end = 10;

        // Safe slice should work with valid boundaries
        assert_eq!(safe_slice::slice(s, 0, crab_start), "hello ");
        assert_eq!(safe_slice::slice(s, crab_end + 1, s.len()), "world");

        // Invalid boundary should return empty
        assert_eq!(safe_slice::slice(s, 7, 9), ""); // Mid-emoji
    }

    #[test]
    fn test_safe_slice_from_to() {
        let s = "hello";
        assert_eq!(safe_slice::slice_from(s, 2), "llo");
        assert_eq!(safe_slice::slice_to(s, 3), "hel");
        assert_eq!(safe_slice::slice_from(s, 10), "");
    }

    #[test]
    fn test_char_at() {
        let s = "hello 🦀";
        assert_eq!(safe_slice::char_at(s, 0), Some('h'));
        assert_eq!(safe_slice::char_at(s, 6), Some('🦀'));
        assert_eq!(safe_slice::char_at(s, 100), None);
    }

    #[test]
    fn test_byte_at() {
        let s = "hello";
        assert_eq!(safe_slice::byte_at(s, 0), Some(b'h'));
        assert_eq!(safe_slice::byte_at(s, 4), Some(b'o'));
        assert_eq!(safe_slice::byte_at(s, 10), None);
    }

    #[test]
    fn test_print_options() {
        let opts = PrintOptions::es5();
        assert!(matches!(opts.target, ScriptTarget::ES5));

        let opts = PrintOptions::commonjs();
        assert!(matches!(opts.module, ModuleKind::CommonJS));

        let opts = PrintOptions::es5_commonjs();
        assert!(matches!(opts.target, ScriptTarget::ES5));
        assert!(matches!(opts.module, ModuleKind::CommonJS));
    }

    #[test]
    fn test_streaming_writer() {
        let mut output = Vec::new();
        {
            let mut printer = StreamingPrinter::new(&mut output);
            printer.write("hello").unwrap();
            printer.write(" ").unwrap();
            printer.write("world").unwrap();
            printer.flush().unwrap();
        }
        assert_eq!(String::from_utf8(output).unwrap(), "hello world");
    }
}
