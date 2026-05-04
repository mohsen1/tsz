//! Emit/Transpilation API
//!
//! Provides TypeScript-compatible emit functionality for generating JavaScript output.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

use tsz::context::emit::EmitContext;
use tsz::declaration_emitter::DeclarationEmitter;
use tsz::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
use tsz::lowering::LoweringPass;
use tsz::parser::{NodeArena, NodeIndex, ParserState, syntax_kind_ext};

use super::options::{module_kind_from_u8, target_kind_from_u8};

const DEFAULT_TRANSPILE_FILE_NAME: &str = "module.ts";
const INVALID_TRANSPILE_OPTIONS_CODE: u32 = 0;
const DIAGNOSTIC_CATEGORY_ERROR: u8 = 1;

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
    /// Source file name used for parsing and diagnostics
    #[serde(default)]
    pub file_name: Option<String>,
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
    /// Downlevel iteration for for-of loops
    #[serde(default)]
    pub downlevel_iteration: Option<bool>,
}

impl TranspileOptions {
    fn file_name(&self) -> &str {
        self.file_name
            .as_deref()
            .filter(|file_name| !file_name.is_empty())
            .unwrap_or(DEFAULT_TRANSPILE_FILE_NAME)
    }

    fn to_printer_options(&self) -> PrinterOptions {
        let mut opts = PrinterOptions {
            target: target_kind_from_u8(self.target),
            module: module_kind_from_u8(self.module),
            ..Default::default()
        };

        opts.remove_comments = self.remove_comments.unwrap_or(false);
        opts.downlevel_iteration = self.downlevel_iteration.unwrap_or(false);

        opts
    }
}

struct TranspileCompilation {
    arena: NodeArena,
    root_idx: NodeIndex,
    output_text: String,
    file_is_module: bool,
}

fn invalid_options_output(error: serde_json::Error) -> String {
    let result = TranspileOutput {
        output_text: String::new(),
        source_map_text: None,
        declaration_text: None,
        diagnostics: vec![EmitDiagnostic {
            file: None,
            message: format!("Invalid transpile options JSON: {error}"),
            code: INVALID_TRANSPILE_OPTIONS_CODE,
            category: DIAGNOSTIC_CATEGORY_ERROR,
        }],
    };

    serialize_transpile_output(&result)
}

fn compile_transpile_source(
    source: &str,
    file_name: &str,
    printer_opts: PrinterOptions,
) -> TranspileCompilation {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root_idx = parser.parse_source_file();
    let arena = parser.into_arena();
    let file_is_module = source_file_has_module_syntax(&arena, root_idx);

    let mut ctx = EmitContext::with_options(printer_opts.clone());
    ctx.auto_detect_module = true;
    ctx.set_target(printer_opts.target);

    let transforms = LoweringPass::new(&arena, &ctx).run(root_idx);

    let mut printer = Printer::with_transforms_and_options(&arena, transforms, printer_opts);
    printer.set_target(ctx.options.target);
    printer.set_auto_detect_module(ctx.auto_detect_module);
    printer.set_source_text(source);
    printer.emit(root_idx);
    let output_text = printer.get_output().to_string();
    drop(printer);

    TranspileCompilation {
        arena,
        root_idx,
        output_text,
        file_is_module,
    }
}

fn source_file_has_module_syntax(arena: &NodeArena, root_idx: NodeIndex) -> bool {
    let Some(source_file) = arena.get_source_file_at(root_idx) else {
        return false;
    };

    source_file
        .statements
        .nodes
        .iter()
        .any(|&stmt_idx| statement_is_module_syntax(arena, stmt_idx))
}

fn statement_is_module_syntax(arena: &NodeArena, stmt_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(stmt_idx) else {
        return false;
    };

    matches!(
        node.kind,
        k if k == syntax_kind_ext::IMPORT_DECLARATION
            || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            || k == syntax_kind_ext::EXPORT_DECLARATION
            || k == syntax_kind_ext::EXPORT_ASSIGNMENT
            || k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
    )
}

fn preserve_empty_module_output(
    mut output: String,
    file_is_module: bool,
    module_kind: ModuleKind,
) -> String {
    if output.trim().is_empty() && file_is_module && !matches!(module_kind, ModuleKind::CommonJS) {
        return "export {};\n".to_string();
    }

    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

fn serialize_transpile_output(result: &TranspileOutput) -> String {
    serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string())
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
    let options: TranspileOptions = match serde_json::from_str(options_json) {
        Ok(options) => options,
        Err(error) => return invalid_options_output(error),
    };

    let printer_opts = options.to_printer_options();
    let module_kind = printer_opts.module;
    let compiled = compile_transpile_source(source, options.file_name(), printer_opts);
    let output_text =
        preserve_empty_module_output(compiled.output_text, compiled.file_is_module, module_kind);

    // Generate declaration file if requested
    let declaration_text = options.declaration.unwrap_or(false).then(|| {
        let mut decl_emitter = DeclarationEmitter::new(&compiled.arena);
        decl_emitter.emit(compiled.root_idx)
    });

    // Build result
    let result = TranspileOutput {
        output_text,
        source_map_text: None, // TODO: implement source maps
        declaration_text,
        diagnostics: Vec::new(),
    };

    serialize_transpile_output(&result)
}

/// Transpile TypeScript to JavaScript (simple version)
///
/// Returns just the JavaScript output string for quick use.
#[wasm_bindgen(js_name = transpile)]
pub fn transpile(source: &str, target: Option<u8>, module: Option<u8>) -> String {
    let opts = PrinterOptions {
        target: target_kind_from_u8(target),
        module: module_kind_from_u8(module),
        ..Default::default()
    };

    let module_kind = opts.module;
    let compiled = compile_transpile_source(source, DEFAULT_TRANSPILE_FILE_NAME, opts);
    preserve_empty_module_output(compiled.output_text, compiled.file_is_module, module_kind)
}

/// Emit a single file from an arena
pub(crate) fn emit_file(
    arena: &NodeArena,
    root_idx: NodeIndex,
    source_text: &str,
    target: ScriptTarget,
    module: ModuleKind,
) -> String {
    let opts = PrinterOptions {
        target,
        module,
        ..Default::default()
    };

    let mut ctx = EmitContext::with_options(opts.clone());
    ctx.auto_detect_module = true;
    ctx.set_target(target);

    let transforms = LoweringPass::new(arena, &ctx).run(root_idx);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, opts);
    printer.set_target(ctx.options.target);
    printer.set_auto_detect_module(ctx.auto_detect_module);
    printer.set_source_text(source_text);
    printer.emit(root_idx);

    printer.get_output().to_string()
}
