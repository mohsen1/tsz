//! Emit/Transpilation API
//!
//! Provides TypeScript-compatible emit functionality for generating JavaScript output.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

use tsz::context::emit::EmitContext;
#[cfg(feature = "dts")]
use tsz::declaration_emitter::DeclarationEmitter;
use tsz::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
use tsz::lowering::LoweringPass;
use tsz::parser::{NodeArena, NodeIndex, ParserState, syntax_kind_ext};
use tsz_common::source_map::base64_encode;

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
    /// Module detection mode: "auto" (default), "force", or "legacy"
    #[serde(default)]
    pub module_detection: Option<String>,
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

        if let Some(ref detection) = self.module_detection {
            if detection.eq_ignore_ascii_case("force") {
                opts.module_detection_force = true;
            } else if detection.eq_ignore_ascii_case("legacy") {
                opts.module_detection_legacy = true;
            }
        }

        opts
    }
}

struct TranspileCompilation {
    #[cfg(feature = "dts")]
    arena: NodeArena,
    #[cfg(feature = "dts")]
    root_idx: NodeIndex,
    output_text: String,
    file_is_module: bool,
    source_map_text: Option<String>,
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
    source_map: Option<SourceMapRequest<'_>>,
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
    if let Some(req) = source_map {
        printer.enable_source_map(req.output_name, req.source_name);
    }
    printer.emit(root_idx);
    let output_text = printer.get_output().to_string();
    let source_map_text = if source_map.is_some() {
        printer.generate_source_map_json()
    } else {
        None
    };
    drop(printer);

    TranspileCompilation {
        #[cfg(feature = "dts")]
        arena,
        #[cfg(feature = "dts")]
        root_idx,
        output_text,
        file_is_module,
        source_map_text,
    }
}

#[derive(Clone, Copy)]
struct SourceMapRequest<'a> {
    /// Generated output file name (used as the source map's `file` field).
    output_name: &'a str,
    /// Source file name (used as the entry in the source map's `sources` field).
    source_name: &'a str,
}

/// Compute the JS output file name for a transpile source file name.
///
/// Mirrors tsc's transpileModule behavior, where the output extension follows
/// the source extension: `.ts`/`.tsx` -> `.js`, `.mts` -> `.mjs`, `.cts` -> `.cjs`.
/// The directory portion of the path is preserved.
fn js_output_name_for_source_map(file_name: &str) -> String {
    if let Some(stem) = file_name.strip_suffix(".mts") {
        return format!("{stem}.mjs");
    }
    if let Some(stem) = file_name.strip_suffix(".cts") {
        return format!("{stem}.cjs");
    }
    if let Some(stem) = file_name
        .strip_suffix(".tsx")
        .or_else(|| file_name.strip_suffix(".ts"))
    {
        return format!("{stem}.js");
    }
    format!("{file_name}.js")
}

/// Return just the basename of `path` (everything after the last `/` or `\`).
fn basename(path: &str) -> &str {
    let after_slash = path.rsplit_once('/').map_or(path, |(_, rest)| rest);
    after_slash
        .rsplit_once('\\')
        .map_or(after_slash, |(_, rest)| rest)
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

fn declaration_text_for_options(
    options: &TranspileOptions,
    compiled: &TranspileCompilation,
) -> Option<String> {
    if !options.declaration.unwrap_or(false) {
        return None;
    }

    emit_declaration_text(compiled)
}

#[cfg(feature = "dts")]
fn emit_declaration_text(compiled: &TranspileCompilation) -> Option<String> {
    let mut decl_emitter = DeclarationEmitter::new(&compiled.arena);
    Some(decl_emitter.emit(compiled.root_idx))
}

#[cfg(not(feature = "dts"))]
fn emit_declaration_text(_compiled: &TranspileCompilation) -> Option<String> {
    None
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
    let file_name = options.file_name().to_string();
    let want_external_map = options.source_map.unwrap_or(false);
    let want_inline_map = options.inline_source_map.unwrap_or(false);
    let want_any_map = want_external_map || want_inline_map;

    // tsc's transpileModule resolves the source map's `file` and the
    // `sourceMappingURL` comment to the output file's basename.
    let output_name = basename(&js_output_name_for_source_map(&file_name)).to_string();
    let source_name = basename(&file_name).to_string();

    let request = want_any_map.then_some(SourceMapRequest {
        output_name: &output_name,
        source_name: &source_name,
    });

    let compiled = compile_transpile_source(source, &file_name, printer_opts, request);
    // Generate declaration file if requested and the lean WASM build includes
    // the optional DTS emitter.
    let declaration_text = declaration_text_for_options(&options, &compiled);

    let mut output_text =
        preserve_empty_module_output(compiled.output_text, compiled.file_is_module, module_kind);

    // Append the sourceMappingURL comment when requested. `inlineSourceMap`
    // wins over the external `sourceMap` form (matches tsc behavior).
    let mut source_map_text: Option<String> = None;
    if want_any_map && let Some(map_json) = compiled.source_map_text {
        if want_inline_map {
            let encoded = base64_encode(map_json.as_bytes());
            output_text.push_str(&format!(
                "//# sourceMappingURL=data:application/json;base64,{encoded}\n"
            ));
        } else {
            output_text.push_str(&format!("//# sourceMappingURL={output_name}.map\n"));
            source_map_text = Some(map_json);
        }
    }

    // Build result
    let result = TranspileOutput {
        output_text,
        source_map_text,
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
    let compiled = compile_transpile_source(source, DEFAULT_TRANSPILE_FILE_NAME, opts, None);
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
