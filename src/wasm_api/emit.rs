//! Emit/Transpilation API
//!
//! Provides TypeScript-compatible emit functionality for generating JavaScript output.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::LoweringPass;
use crate::emit_context::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
use crate::parser::{NodeArena, NodeIndex, ParserState};

/// Emit result containing output files and diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmitResult {
    /// Whether emit was skipped
    pub emit_skipped: bool,
    /// Diagnostic messages from emit
    pub diagnostics: Vec<EmitDiagnostic>,
    /// Emitted files
    pub emitted_files: Vec<EmittedFile>,
}

/// A diagnostic from the emit process
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmitDiagnostic {
    pub file: Option<String>,
    pub message: String,
    pub code: u32,
    pub category: u8,
}

/// An emitted output file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmittedFile {
    /// Output file name
    pub name: String,
    /// Output content
    pub text: String,
    /// Whether this is a declaration file
    pub declaration: bool,
    /// Whether this is a source map
    pub source_map: bool,
}

/// Transpile options for single-file transpilation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranspileOptions {
    /// Target ECMAScript version
    #[serde(default)]
    pub target: Option<u8>,
    /// Module format
    #[serde(default)]
    pub module: Option<u8>,
    /// Generate source maps
    #[serde(default)]
    pub source_map: Option<bool>,
    /// Generate inline source maps
    #[serde(default)]
    pub inline_source_map: Option<bool>,
    /// Generate declaration files
    #[serde(default)]
    pub declaration: Option<bool>,
    /// Remove comments
    #[serde(default)]
    pub remove_comments: Option<bool>,
    /// JSX mode
    #[serde(default)]
    pub jsx: Option<u8>,
}

impl TranspileOptions {
    fn to_printer_options(&self) -> PrinterOptions {
        let mut opts = PrinterOptions::default();

        // Map target
        opts.target = match self.target.unwrap_or(1) {
            0 => ScriptTarget::ES3,
            1 => ScriptTarget::ES5,
            2 => ScriptTarget::ES2015,
            3 => ScriptTarget::ES2016,
            4 => ScriptTarget::ES2017,
            5 => ScriptTarget::ES2018,
            6 => ScriptTarget::ES2019,
            7 => ScriptTarget::ES2020,
            8 => ScriptTarget::ES2021,
            9 => ScriptTarget::ES2022,
            99 => ScriptTarget::ESNext,
            _ => ScriptTarget::ES5,
        };

        // Map module
        opts.module = match self.module.unwrap_or(0) {
            0 => ModuleKind::None,
            1 => ModuleKind::CommonJS,
            2 => ModuleKind::AMD,
            4 => ModuleKind::System,
            5 => ModuleKind::UMD,
            6 => ModuleKind::ES2015,
            7 => ModuleKind::ES2020,
            99 => ModuleKind::ESNext,
            100 => ModuleKind::Node16,
            199 => ModuleKind::NodeNext,
            _ => ModuleKind::None,
        };

        opts.remove_comments = self.remove_comments.unwrap_or(false);

        opts
    }
}

/// Transpile result for single-file transpilation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranspileOutput {
    /// Transpiled JavaScript output
    pub output_text: String,
    /// Source map (if requested)
    pub source_map_text: Option<String>,
    /// Declaration output (if requested)
    pub declaration_text: Option<String>,
    /// Diagnostics
    pub diagnostics: Vec<EmitDiagnostic>,
}

/// Transpile a single TypeScript file to JavaScript
///
/// This is a simplified API for quick transpilation without creating a full program.
#[wasm_bindgen(js_name = transpileModule)]
pub fn transpile_module(source: &str, options_json: &str) -> String {
    let options: TranspileOptions = serde_json::from_str(options_json).unwrap_or_default();

    // Parse the source
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root_idx = parser.parse_source_file();
    let arena = parser.into_arena();

    // Create emit context
    let printer_opts = options.to_printer_options();
    let mut ctx = EmitContext::with_options(printer_opts.clone());
    ctx.auto_detect_module = true;
    ctx.target_es5 = matches!(printer_opts.target, ScriptTarget::ES3 | ScriptTarget::ES5);

    // Run transforms
    let transforms = LoweringPass::new(&arena, &ctx).run(root_idx);

    // Emit
    let mut printer = Printer::with_transforms_and_options(&arena, transforms, printer_opts);
    printer.set_target_es5(ctx.target_es5);
    printer.set_auto_detect_module(ctx.auto_detect_module);
    printer.set_source_text(source);
    printer.emit(root_idx);

    let output_text = printer.get_output().to_string();

    // Build result
    let result = TranspileOutput {
        output_text,
        source_map_text: None,  // TODO: implement source maps
        declaration_text: None, // TODO: implement declarations
        diagnostics: Vec::new(),
    };

    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Transpile TypeScript to JavaScript (simple version)
///
/// Returns just the JavaScript output string for quick use.
#[wasm_bindgen(js_name = transpile)]
pub fn transpile(source: &str, target: Option<u8>, module: Option<u8>) -> String {
    // Parse the source
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root_idx = parser.parse_source_file();
    let arena = parser.into_arena();

    // Create emit context with specified options
    let mut opts = PrinterOptions::default();
    opts.target = match target.unwrap_or(1) {
        0 => ScriptTarget::ES3,
        1 => ScriptTarget::ES5,
        2 => ScriptTarget::ES2015,
        99 => ScriptTarget::ESNext,
        _ => ScriptTarget::ES5,
    };
    opts.module = match module.unwrap_or(0) {
        0 => ModuleKind::None,
        1 => ModuleKind::CommonJS,
        6 => ModuleKind::ES2015,
        99 => ModuleKind::ESNext,
        _ => ModuleKind::None,
    };

    let module_kind = opts.module;
    let mut ctx = EmitContext::with_options(opts.clone());
    ctx.auto_detect_module = true;
    ctx.target_es5 = matches!(opts.target, ScriptTarget::ES3 | ScriptTarget::ES5);

    // Run transforms
    let transforms = LoweringPass::new(&arena, &ctx).run(root_idx);

    // Emit
    let mut printer = Printer::with_transforms_and_options(&arena, transforms, opts);
    printer.set_target_es5(ctx.target_es5);
    printer.set_auto_detect_module(ctx.auto_detect_module);
    printer.set_source_text(source);
    printer.emit(root_idx);

    let mut output = printer.get_output().to_string();

    // For ES module files: if the source had import/export statements but the output
    // is empty (all imports were type-only), emit "export {};" to preserve module nature
    if output.trim().is_empty() && !matches!(module_kind, ModuleKind::CommonJS) {
        let has_module_syntax = source.contains("import ") || source.contains("export ");
        if has_module_syntax {
            return "export {};\n".to_string();
        }
    }

    // Ensure output ends with a newline (matches TypeScript behavior)
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

/// Emit a single file from an arena
pub(crate) fn emit_file(
    arena: &NodeArena,
    root_idx: NodeIndex,
    source_text: &str,
    target: ScriptTarget,
    module: ModuleKind,
) -> String {
    let mut opts = PrinterOptions::default();
    opts.target = target;
    opts.module = module;

    let mut ctx = EmitContext::with_options(opts.clone());
    ctx.auto_detect_module = true;
    ctx.target_es5 = matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5);

    let transforms = LoweringPass::new(arena, &ctx).run(root_idx);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, opts);
    printer.set_target_es5(ctx.target_es5);
    printer.set_auto_detect_module(ctx.auto_detect_module);
    printer.set_source_text(source_text);
    printer.emit(root_idx);

    printer.get_output().to_string()
}
