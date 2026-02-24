//! Printer - A clean, safe AST-to-JavaScript printer
//!
//! This module provides a high-level interface for converting TypeScript/JavaScript AST
//! to JavaScript output. It separates concerns:
//!
//! - **Printing**: AST nodes → JavaScript text (this module)
//! - **Transformation**: TypeScript features → ES5/ES6 (emitter internals)
//! - **Output**: Text generation and source maps (`source_writer`)
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

use crate::context::emit::EmitContext;
use crate::context::transform::TransformContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions, ScriptTarget};
use crate::lowering::LoweringPass;
use std::io::{self, Write};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

// =============================================================================
// Print Options
// =============================================================================

/// Options for printing AST to JavaScript.
///
/// This is a simplified options struct that covers the most common use cases.
/// For advanced options, use `emitter::Printer` directly with `PrinterOptions`.
#[derive(Clone, Debug, Default)]
pub struct PrintOptions {
    /// Target ECMAScript version (`ESNext` by default)
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

    /// Create options for `CommonJS` module output
    pub fn commonjs() -> Self {
        Self {
            module: ModuleKind::CommonJS,
            ..Default::default()
        }
    }

    /// Create options for ES5 `CommonJS` output
    pub fn es5_commonjs() -> Self {
        Self {
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            ..Default::default()
        }
    }

    /// Convert to internal `PrinterOptions`
    fn to_printer_options(&self) -> PrinterOptions {
        PrinterOptions {
            target: self.target,
            module: self.module,
            remove_comments: self.remove_comments,
            single_quote: self.single_quote,
            downlevel_iteration: false,
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
    /// Create a new `PrintResult` with just code
    pub const fn new(code: String) -> Self {
        Self {
            code,
            source_map: None,
        }
    }

    /// Create a new `PrintResult` with code and source map
    pub const fn with_source_map(code: String, source_map: String) -> Self {
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
        self.inner.set_target(self.options.target);

        // Configure module settings
        self.inner.set_module_kind(self.options.module);

        // Extract source text from arena if not already set (for comment preservation)
        if let Some(node) = self.inner.arena.get(root)
            && let Some(source_file) = self.inner.arena.get_source_file(node)
        {
            let file_name = source_file.file_name.to_ascii_lowercase();
            let is_js_source = file_name.ends_with(".js")
                || file_name.ends_with(".jsx")
                || file_name.ends_with(".mjs")
                || file_name.ends_with(".cjs");
            self.inner.set_current_root_js_source(is_js_source);

            if self.inner.source_text.is_none() {
                // SAFETY: The source text lives as long as the arena, and we hold
                // a reference to the arena for the lifetime of the Printer.
                // Route through `set_source_text` so writer preallocation stays in sync
                // with other source-text-aware constructor paths.
                let text_ref: &'a str = &source_file.text;
                self.inner.set_source_text(text_ref);
            }
        }

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
/// Use this when you want ES5 or `CommonJS` output with proper transforms.
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
#[path = "../../tests/printer.rs"]
mod tests;
