//! ThinEmitter - Emitter using ThinNodeArena
//!
//! This emitter uses the ThinNode architecture for cache-optimized AST access.
//! It works directly with ThinNodeArena instead of the old Node enum.
//!
//! # Architecture
//!
//! - Uses ThinNodeArena for AST access (16-byte nodes, 13x cache improvement)
//! - Dispatches based on ThinNode.kind (u16)
//! - Uses accessor methods to get typed node data
//!
//! # Module Organization
//!
//! The emitter is organized as a directory module:
//! - `mod.rs` - Core ThinPrinter struct, dispatch logic, and emit methods
//! - `expressions.rs` - Expression emission helpers
//! - `statements.rs` - Statement emission helpers
//! - `declarations.rs` - Declaration emission helpers
//! - `functions.rs` - Function emission helpers
//! - `types.rs` - Type emission helpers
//! - `jsx.rs` - JSX emission helpers
//!
//! Note: pub(super) fields and methods allow future submodules to access ThinPrinter internals.

// Allow dead code for:
// - Comment helpers (emit_leading_comments, emit_comments_in_gap, last_processed_pos): Infrastructure
//   for fine-grained comment emission that is partially implemented. Currently source file level
//   comment emission is done in emit_source_file, but node-level comment tracking is prepared.
// - TypeScript declaration emitters (emit_interface_declaration, emit_type_alias_declaration):
//   These emit .d.ts content and are intentionally skipped when emitting JavaScript output.
//   Kept for future declaration file generation support.
// - ES5 transform helpers (function_parameters_need_es5_transform, emit_extends_helper):
//   Infrastructure functions for ES5 downleveling. emit_extends_helper is replaced by
//   crate::transforms::helpers::emit_helpers() but kept as reference implementation.
// - Module emission helpers (emit_commonjs_preamble): Refactored into emit_source_file but
//   kept as standalone helper for potential future use.
#![allow(dead_code)]

use crate::emit_context::EmitContext;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::{ThinNode, ThinNodeArena};
use crate::scanner::SyntaxKind;
use crate::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use crate::transform_context::{IdentifierId, TransformContext, TransformDirective};
use crate::transforms::class_es5::ClassES5Emitter;
use crate::transforms::enum_es5::EnumES5Emitter;
use crate::transforms::namespace_es5::NamespaceES5Emitter;
use std::sync::Arc;

mod comment_helpers;
mod comments;
mod declarations;
mod es5_bindings;
mod es5_helpers;
mod es5_templates;
mod expressions;
mod functions;
mod helpers;
mod jsx;
mod literals;
mod module_emission;
mod module_wrapper;
mod statements;
mod template_literals;
mod types;

pub use comments::{
    CommentKind, CommentRange, get_leading_comment_ranges, get_trailing_comment_ranges,
};

// =============================================================================
// Emitter Options
// =============================================================================

/// ECMAScript target version.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScriptTarget {
    ES3 = 0,
    ES5 = 1,
    ES2015 = 2,
    ES2016 = 3,
    ES2017 = 4,
    ES2018 = 5,
    ES2019 = 6,
    ES2020 = 7,
    ES2021 = 8,
    ES2022 = 9,
    #[default]
    ESNext = 99,
}

/// Module system kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModuleKind {
    #[default]
    None = 0,
    CommonJS = 1,
    AMD = 2,
    UMD = 3,
    System = 4,
    ES2015 = 5,
    ES2020 = 6,
    ES2022 = 7,
    ESNext = 99,
    Node16 = 100,
    NodeNext = 199,
}

/// New line kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewLineKind {
    #[default]
    LineFeed = 0,
    CarriageReturnLineFeed = 1,
}

/// Printer configuration options.
#[derive(Clone, Debug)]
pub struct PrinterOptions {
    /// Remove comments from output
    pub remove_comments: bool,
    /// Target ECMAScript version
    pub target: ScriptTarget,
    /// Use single quotes for strings
    pub single_quote: bool,
    /// Omit trailing semicolons
    pub omit_trailing_semicolon: bool,
    /// Don't emit helpers
    pub no_emit_helpers: bool,
    /// Module kind
    pub module: ModuleKind,
    /// New line character
    pub new_line: NewLineKind,
}

impl Default for PrinterOptions {
    fn default() -> Self {
        PrinterOptions {
            remove_comments: false,
            target: ScriptTarget::ESNext,
            single_quote: false,
            omit_trailing_semicolon: false,
            no_emit_helpers: false,
            module: ModuleKind::None,
            new_line: NewLineKind::LineFeed,
        }
    }
}

#[derive(Default)]
struct ParamTransformPlan {
    params: Vec<ParamTransform>,
    rest: Option<RestParamTransform>,
}

impl ParamTransformPlan {
    fn has_transforms(&self) -> bool {
        !self.params.is_empty() || self.rest.is_some()
    }
}

struct ParamTransform {
    name: String,
    pattern: Option<NodeIndex>,
    initializer: Option<NodeIndex>,
}

struct RestParamTransform {
    name: String,
    pattern: Option<NodeIndex>,
    index: usize,
}

struct TemplateParts {
    cooked: Vec<String>,
    raw: Vec<String>,
    expressions: Vec<NodeIndex>,
}

enum EmitDirective {
    Identity,
    ES5Class {
        class_node: NodeIndex,
    },
    ES5ClassExpression {
        class_node: NodeIndex,
    },
    ES5Namespace {
        namespace_node: NodeIndex,
    },
    ES5Enum {
        enum_node: NodeIndex,
    },
    CommonJSExport {
        names: Arc<[IdentifierId]>,
        is_default: bool,
        inner: Box<EmitDirective>,
    },
    CommonJSExportDefaultExpr,
    CommonJSExportDefaultClassES5 {
        class_node: NodeIndex,
    },
    ES5ArrowFunction {
        arrow_node: NodeIndex,
        captures_this: bool,
    },
    ES5AsyncFunction {
        function_node: NodeIndex,
    },
    ES5ForOf {
        for_of_node: NodeIndex,
    },
    ES5ObjectLiteral {
        object_literal: NodeIndex,
    },
    ES5VariableDeclarationList {
        decl_list: NodeIndex,
    },
    ES5FunctionParameters {
        function_node: NodeIndex,
    },
    ES5TemplateLiteral,
    ModuleWrapper {
        format: crate::transform_context::ModuleFormat,
        dependencies: Arc<[String]>,
    },
    Chain(Vec<EmitDirective>),
}

// =============================================================================
// ThinPrinter
// =============================================================================

/// Printer that works with ThinNodeArena.
///
/// Uses SourceWriter for output generation (enables source map support).
/// Uses EmitContext for transform-specific state management.
/// Uses TransformContext for directive-based transforms (Phase 2 architecture).
/// Maximum recursion depth for emit operations to prevent infinite loops
const MAX_EMIT_DEPTH: u32 = 200;

pub struct ThinPrinter<'a> {
    /// The ThinNodeArena containing the AST.
    pub(super) arena: &'a ThinNodeArena,

    /// Source writer for output generation and source map tracking
    pub(super) writer: SourceWriter,

    /// Emit context holding options and transform state
    pub(super) ctx: EmitContext,

    /// Transform directives from lowering pass (optional, defaults to empty)
    pub(super) transforms: TransformContext,

    /// Emit `void 0` for missing initializers during recovery.
    pub(super) emit_missing_initializer_as_void_0: bool,

    /// Source text for detecting single-line constructs
    pub(super) source_text: Option<&'a str>,

    /// Source text for source map generation (kept separate from comment emission).
    pub(super) source_map_text: Option<&'a str>,

    /// Last processed position in source text for comment gap detection
    pub(super) last_processed_pos: u32,

    /// Pending source position for mapping the next write.
    pub(super) pending_source_pos: Option<SourcePosition>,

    /// Recursion depth counter to prevent infinite loops from malformed AST
    pub(super) emit_depth: u32,
}

impl<'a> ThinPrinter<'a> {
    /// Create a new ThinPrinter.
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        Self::with_options(arena, PrinterOptions::default())
    }

    /// Create a new ThinPrinter with pre-allocated output capacity
    /// This reduces allocations when the expected output size is known (e.g., ~1.5x source size)
    pub fn with_capacity(arena: &'a ThinNodeArena, capacity: usize) -> Self {
        Self::with_capacity_and_options(arena, capacity, PrinterOptions::default())
    }

    /// Create a new ThinPrinter with options.
    pub fn with_options(arena: &'a ThinNodeArena, options: PrinterOptions) -> Self {
        Self::with_capacity_and_options(arena, 1024, options)
    }

    /// Create a new ThinPrinter with pre-allocated capacity and options.
    pub fn with_capacity_and_options(
        arena: &'a ThinNodeArena,
        capacity: usize,
        options: PrinterOptions,
    ) -> Self {
        let mut writer = SourceWriter::with_capacity(capacity);
        writer.set_new_line_kind(options.new_line);

        // Create EmitContext from options (target controls ES5 vs ESNext)
        let ctx = EmitContext::with_options(options);

        ThinPrinter {
            arena,
            writer,
            ctx,
            transforms: TransformContext::new(), // Empty by default, can be set later
            emit_missing_initializer_as_void_0: false,
            source_text: None,
            source_map_text: None,
            last_processed_pos: 0,
            pending_source_pos: None,
            emit_depth: 0,
        }
    }

    /// Create a new ThinPrinter with transform directives.
    /// This is the Phase 2 constructor that accepts pre-computed transforms.
    pub fn with_transforms(arena: &'a ThinNodeArena, transforms: TransformContext) -> Self {
        let mut printer = Self::new(arena);
        printer.transforms = transforms;
        printer
    }

    /// Create a new ThinPrinter with transforms and options.
    pub fn with_transforms_and_options(
        arena: &'a ThinNodeArena,
        transforms: TransformContext,
        options: PrinterOptions,
    ) -> Self {
        let mut printer = Self::with_options(arena, options);
        printer.transforms = transforms;
        printer
    }

    /// Create a new ThinPrinter targeting ES5.
    pub fn new_es5(arena: &'a ThinNodeArena) -> Self {
        let options = PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        Self::with_options(arena, options)
    }

    /// Create a new ThinPrinter targeting ES6+.
    pub fn new_es6(arena: &'a ThinNodeArena) -> Self {
        let options = PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        };
        Self::with_options(arena, options)
    }

    /// Set whether to target ES5 (classes→IIFEs, arrows→functions).
    pub fn set_target_es5(&mut self, es5: bool) {
        self.ctx.target_es5 = es5;
    }

    /// Set the module kind (CommonJS, ESM, etc.).
    pub fn set_module_kind(&mut self, kind: ModuleKind) {
        self.ctx.options.module = kind;
    }

    /// Set auto-detect module mode. When enabled, the emitter will detect if
    /// the source file contains import/export statements and apply CommonJS
    /// transforms automatically.
    pub fn set_auto_detect_module(&mut self, enabled: bool) {
        self.ctx.auto_detect_module = enabled;
    }

    /// Set the source text (for detecting single-line constructs).
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
        let estimated = text.len().saturating_mul(3) / 2;
        self.writer.ensure_output_capacity(estimated);
    }

    /// Set source text for source map generation without enabling comment emission.
    pub fn set_source_map_text(&mut self, text: &'a str) {
        self.source_map_text = Some(text);
    }

    /// Enable source map generation and register the current source file.
    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.writer.enable_source_map(output_name.to_string());
        let content = self.source_text_for_map().map(|text| text.to_string());
        self.writer.add_source(source_name.to_string(), content);
    }

    /// Generate source map JSON (if enabled).
    pub fn generate_source_map_json(&mut self) -> Option<String> {
        self.writer.generate_source_map_json()
    }

    fn source_text_for_map(&self) -> Option<&'a str> {
        self.source_map_text.or(self.source_text)
    }

    fn queue_source_mapping(&mut self, node: &ThinNode) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        let Some(text) = self.source_text_for_map() else {
            self.pending_source_pos = None;
            return;
        };

        self.pending_source_pos = Some(source_position_from_offset(text, node.pos));
    }

    /// Check if a node spans a single line in the source.
    /// For blocks like `{ }`, we look for the closing `}` and check if there's a newline
    /// between the opening `{` and the first `}`.
    fn is_single_line(&self, node: &ThinNode) -> bool {
        if let Some(text) = self.source_text {
            let start = node.pos as usize;
            if start < text.len() {
                // Find the first closing brace after the opening
                // For a block, the source starts with `{` and we want to find the matching `}`
                let slice = &text[start..];
                if let Some(close_idx) = slice.find('}') {
                    // Check if there's a newline between `{` and `}`
                    let inner = &slice[..close_idx + 1];
                    return !inner.contains('\n');
                }
            }
        }
        // Default to multi-line if we can't determine
        false
    }

    /// Get the output.
    pub fn get_output(&self) -> &str {
        self.writer.get_output()
    }

    /// Take the output.
    pub fn take_output(self) -> String {
        self.writer.take_output()
    }

    // =========================================================================
    // Transform Application (Phase 2 Architecture)
    // =========================================================================

    fn emit_directive_from_transform(directive: &TransformDirective) -> EmitDirective {
        match directive {
            TransformDirective::Identity => EmitDirective::Identity,
            TransformDirective::ES5Class { class_node, .. } => EmitDirective::ES5Class {
                class_node: *class_node,
            },
            TransformDirective::ES5ClassExpression { class_node } => {
                EmitDirective::ES5ClassExpression {
                    class_node: *class_node,
                }
            }
            TransformDirective::ES5Namespace { namespace_node } => EmitDirective::ES5Namespace {
                namespace_node: *namespace_node,
            },
            TransformDirective::ES5Enum { enum_node } => EmitDirective::ES5Enum {
                enum_node: *enum_node,
            },
            TransformDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => EmitDirective::CommonJSExport {
                names: names.clone(),
                is_default: *is_default,
                inner: Box::new(Self::emit_directive_from_transform(inner.as_ref())),
            },
            TransformDirective::CommonJSExportDefaultExpr => {
                EmitDirective::CommonJSExportDefaultExpr
            }
            TransformDirective::CommonJSExportDefaultClassES5 { class_node } => {
                EmitDirective::CommonJSExportDefaultClassES5 {
                    class_node: *class_node,
                }
            }
            TransformDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
            } => EmitDirective::ES5ArrowFunction {
                arrow_node: *arrow_node,
                captures_this: *captures_this,
            },
            TransformDirective::ES5AsyncFunction { function_node } => {
                EmitDirective::ES5AsyncFunction {
                    function_node: *function_node,
                }
            }
            TransformDirective::ES5ForOf { for_of_node } => EmitDirective::ES5ForOf {
                for_of_node: *for_of_node,
            },
            TransformDirective::ES5ObjectLiteral { object_literal } => {
                EmitDirective::ES5ObjectLiteral {
                    object_literal: *object_literal,
                }
            }
            TransformDirective::ES5VariableDeclarationList { decl_list } => {
                EmitDirective::ES5VariableDeclarationList {
                    decl_list: *decl_list,
                }
            }
            TransformDirective::ES5FunctionParameters { function_node } => {
                EmitDirective::ES5FunctionParameters {
                    function_node: *function_node,
                }
            }
            TransformDirective::ES5TemplateLiteral { .. } => EmitDirective::ES5TemplateLiteral,
            TransformDirective::ModuleWrapper {
                format,
                dependencies,
            } => EmitDirective::ModuleWrapper {
                format: *format,
                dependencies: dependencies.clone(),
            },
            TransformDirective::Chain(directives) => {
                let mut flattened = Vec::new();
                Self::flatten_emit_chain(directives.as_slice(), &mut flattened);
                EmitDirective::Chain(flattened)
            }
        }
    }

    fn flatten_emit_chain(directives: &[TransformDirective], out: &mut Vec<EmitDirective>) {
        for directive in directives {
            match directive {
                TransformDirective::Chain(inner) => {
                    Self::flatten_emit_chain(inner.as_slice(), out);
                }
                other => out.push(Self::emit_directive_from_transform(other)),
            }
        }
    }

    /// Apply a transform directive to a node.
    /// This is called when a node has an entry in the TransformContext.
    fn apply_transform(&mut self, node: &ThinNode, idx: NodeIndex) {
        let Some(directive) = self.transforms.get(idx) else {
            // No transform, emit normally (should not happen if has_transform returned true)
            self.emit_node_default(node, idx);
            return;
        };

        let directive = Self::emit_directive_from_transform(directive);

        match directive {
            EmitDirective::Identity => {
                // No transformation needed, emit as-is
                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5Class { class_node } => {
                // Delegate to existing ClassES5Emitter
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
                let es5_output = es5_emitter.emit_class(class_node);
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(class_node);
            }

            EmitDirective::ES5Namespace { namespace_node } => {
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                let output = ns_emitter.emit_namespace(namespace_node);
                self.write(&output);
            }

            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                let output = enum_emitter.emit_enum(enum_node);
                self.write(&output);
            }

            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                let export_name = names.first().copied();
                self.emit_commonjs_export(names.as_ref(), is_default, |this| {
                    this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                });
            }

            EmitDirective::CommonJSExportDefaultExpr => {
                self.emit_commonjs_default_export_expr(node, idx);
            }

            EmitDirective::CommonJSExportDefaultClassES5 { class_node } => {
                self.emit_commonjs_default_export_class_es5(class_node);
            }

            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
            } => {
                if let Some(arrow_node) = self.arena.get(arrow_node) {
                    if let Some(func) = self.arena.get_function(arrow_node) {
                        self.emit_arrow_function_es5(arrow_node, func, captures_this);
                        return;
                    }
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(function_node) {
                    if let Some(func) = self.arena.get_function(func_node) {
                        let func_name = if !func.name.is_none() {
                            self.get_identifier_text_idx(func.name)
                        } else {
                            String::new()
                        };

                        self.emit_async_function_es5(func, &func_name, "this");
                        return;
                    }
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ForOf { for_of_node } => {
                if let Some(for_of_node) = self.arena.get(for_of_node) {
                    if let Some(for_in_of) = self.arena.get_for_in_of(for_of_node) {
                        if !for_in_of.await_modifier {
                            self.emit_for_of_statement_es5(for_in_of);
                            return;
                        }
                    }
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ObjectLiteral { object_literal } => {
                if let Some(literal_node) = self.arena.get(object_literal) {
                    if let Some(literal) = self.arena.get_literal_expr(literal_node) {
                        self.emit_object_literal_es5(&literal.elements.nodes);
                        return;
                    }
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5VariableDeclarationList { decl_list } => {
                if let Some(list_node) = self.arena.get(decl_list) {
                    self.emit_variable_declaration_list_es5(list_node);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                            return;
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node);
                            return;
                        }
                        _ => {}
                    }
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5TemplateLiteral => {
                if !self.emit_template_literal_es5(node, idx) {
                    self.emit_node_default(node, idx);
                }
            }

            EmitDirective::ModuleWrapper {
                format,
                dependencies,
            } => {
                if let Some(source) = self.arena.get_source_file(node) {
                    self.emit_module_wrapper(&format, dependencies.as_ref(), node, source);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::Chain(directives) => {
                self.emit_chained_directives(node, idx, directives.as_slice());
            }
        }
    }

    fn emit_commonjs_inner(
        &mut self,
        node: &ThinNode,
        idx: NodeIndex,
        inner: &EmitDirective,
        export_name: Option<IdentifierId>,
    ) {
        match inner {
            EmitDirective::ES5Class { class_node } => {
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
                let es5_output = es5_emitter.emit_class(*class_node);
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(*class_node);
            }
            EmitDirective::ES5Namespace { namespace_node } => {
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                let output = ns_emitter.emit_namespace(*namespace_node);
                self.write(&output);
            }
            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                let output = enum_emitter.emit_enum(*enum_node);
                self.write(&output);
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    if let Some(func) = self.arena.get_function(func_node) {
                        if !func.name.is_none() {
                            let func_name = self.get_identifier_text_idx(func.name);
                            self.emit_async_function_es5(func, &func_name, "this");
                        } else if let Some(export_name) = export_name {
                            if let Some(ident) = self.arena.identifiers.get(export_name as usize) {
                                self.emit_async_function_es5(func, &ident.escaped_text, "this");
                            } else {
                                self.emit_async_function_es5(func, "", "this");
                            }
                        } else {
                            self.emit_async_function_es5(func, "", "this");
                        }
                    }
                }
            }
            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
            } => {
                if let Some(arrow_node) = self.arena.get(*arrow_node) {
                    if let Some(func) = self.arena.get_function(arrow_node) {
                        self.emit_arrow_function_es5(arrow_node, func, *captures_this);
                    }
                }
            }
            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node);
                        }
                        _ => {}
                    }
                }
            }
            EmitDirective::Identity => {
                self.emit_node_default(node, idx);
            }
            EmitDirective::Chain(directives) => {
                self.emit_chained_directives(node, idx, directives.as_slice());
            }
            _ => {
                self.emit_node_default(node, idx);
            }
        }
    }

    fn emit_chained_directives(
        &mut self,
        node: &ThinNode,
        idx: NodeIndex,
        directives: &[EmitDirective],
    ) {
        if directives.is_empty() {
            self.emit_node_default(node, idx);
            return;
        }

        let last = directives.len() - 1;
        self.emit_chained_directive(node, idx, directives, last);
    }

    fn emit_chained_directive(
        &mut self,
        node: &ThinNode,
        idx: NodeIndex,
        directives: &[EmitDirective],
        index: usize,
    ) {
        let directive = &directives[index];
        match directive {
            EmitDirective::Identity => {
                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5Class { class_node } => {
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
                let es5_output = es5_emitter.emit_class(*class_node);
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(*class_node);
            }
            EmitDirective::ES5Namespace { namespace_node } => {
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                let output = ns_emitter.emit_namespace(*namespace_node);
                self.write(&output);
            }
            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                let output = enum_emitter.emit_enum(*enum_node);
                self.write(&output);
            }
            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                let export_name = names.first().copied();
                self.emit_commonjs_export(names.as_ref(), *is_default, |this| {
                    if index == 0 {
                        this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                    } else {
                        this.emit_chained_directive(node, idx, directives, index - 1);
                    }
                });
            }
            EmitDirective::CommonJSExportDefaultExpr => {
                self.emit_commonjs_default_export_assignment(|this| {
                    if index == 0 {
                        this.emit_commonjs_default_export_expr_inner(node, idx);
                    } else {
                        this.emit_chained_directive(node, idx, directives, index - 1);
                    }
                });
            }
            EmitDirective::CommonJSExportDefaultClassES5 { class_node } => {
                self.emit_commonjs_default_export_class_es5(*class_node);
            }
            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
            } => {
                if let Some(arrow_node) = self.arena.get(*arrow_node) {
                    if let Some(func) = self.arena.get_function(arrow_node) {
                        self.emit_arrow_function_es5(arrow_node, func, *captures_this);
                        return;
                    }
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    if let Some(func) = self.arena.get_function(func_node) {
                        let func_name = if !func.name.is_none() {
                            self.get_identifier_text_idx(func.name)
                        } else {
                            String::new()
                        };

                        self.emit_async_function_es5(func, &func_name, "this");
                        return;
                    }
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ForOf { for_of_node } => {
                if let Some(for_of_node) = self.arena.get(*for_of_node) {
                    if let Some(for_in_of) = self.arena.get_for_in_of(for_of_node) {
                        if !for_in_of.await_modifier {
                            self.emit_for_of_statement_es5(for_in_of);
                            return;
                        }
                    }
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ObjectLiteral { object_literal } => {
                if let Some(literal_node) = self.arena.get(*object_literal) {
                    if let Some(literal) = self.arena.get_literal_expr(literal_node) {
                        self.emit_object_literal_es5(&literal.elements.nodes);
                        return;
                    }
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5VariableDeclarationList { decl_list } => {
                if let Some(list_node) = self.arena.get(*decl_list) {
                    self.emit_variable_declaration_list_es5(list_node);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                            return;
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node);
                            return;
                        }
                        _ => {}
                    }
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5TemplateLiteral => {
                if self.emit_template_literal_es5(node, idx) {
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ModuleWrapper {
                format,
                dependencies,
            } => {
                if let Some(source) = self.arena.get_source_file(node) {
                    self.emit_module_wrapper(format, dependencies.as_ref(), node, source);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::Chain(nested) => {
                self.emit_chained_directives(node, idx, nested.as_slice());
            }
        }
    }

    fn emit_chained_previous(
        &mut self,
        node: &ThinNode,
        idx: NodeIndex,
        directives: &[EmitDirective],
        index: usize,
    ) {
        if index == 0 {
            self.emit_node_default(node, idx);
        } else {
            self.emit_chained_directive(node, idx, directives, index - 1);
        }
    }

    /// Emit a node using default logic (no transforms).
    /// This is the old emit_node logic extracted for reuse.
    fn emit_node_default(&mut self, node: &ThinNode, idx: NodeIndex) {
        // This will be populated by moving the match statement from emit_node
        // For now, just recursively call emit_node which will use the match
        // We'll refactor this properly in the next step
        let kind = node.kind;
        self.emit_node_by_kind(node, idx, kind);
    }

    // =========================================================================
    // Main Emit Method
    // =========================================================================

    /// Emit a node by index.
    pub fn emit(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(source) = self.arena.get_source_file(node) {
            if self.transforms.is_empty() {
                let format = match self.ctx.options.module {
                    ModuleKind::AMD => Some(crate::transform_context::ModuleFormat::AMD),
                    ModuleKind::UMD => Some(crate::transform_context::ModuleFormat::UMD),
                    ModuleKind::System => Some(crate::transform_context::ModuleFormat::System),
                    _ => None,
                };
                if let Some(format) = format {
                    if self.file_is_module(&source.statements) {
                        let dependencies =
                            self.collect_module_dependencies(&source.statements.nodes);
                        self.emit_module_wrapper(&format, &dependencies, node, source);
                        return;
                    }
                }
            }
        }

        self.emit_node(node, idx);
    }

    /// Emit a node in an expression context.
    /// If the node is an error/unknown node, emits `void 0` for parse error tolerance.
    pub fn emit_expression(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            self.write("void 0");
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            self.write("void 0");
            return;
        };

        // Check if this is an error/unknown node
        use crate::scanner::SyntaxKind;
        if node.kind == SyntaxKind::Unknown as u16 {
            self.write("void 0");
            return;
        }

        // Otherwise, emit normally
        self.emit_node(node, idx);
    }

    /// Emit a node.
    fn emit_node(&mut self, node: &ThinNode, idx: NodeIndex) {
        // Guard against infinite recursion from malformed AST
        self.emit_depth += 1;
        if self.emit_depth > MAX_EMIT_DEPTH {
            self.emit_depth -= 1;
            self.write("/* MAX_EMIT_DEPTH exceeded */");
            return;
        }

        // Phase 2 Architecture: Check transform directives first
        let has_transform = !self.transforms.is_empty()
            && Self::kind_may_have_transform(node.kind)
            && self.transforms.has_transform(idx);
        let previous_pending = self.pending_source_pos;

        self.queue_source_mapping(node);
        if has_transform {
            self.apply_transform(node, idx);
        } else {
            let kind = node.kind;
            self.emit_node_by_kind(node, idx, kind);
        }

        self.pending_source_pos = previous_pending;
        self.emit_depth -= 1;
    }

    fn kind_may_have_transform(kind: u16) -> bool {
        matches!(
            kind,
            k if k == syntax_kind_ext::SOURCE_FILE
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::MODULE_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Emit a node by kind using default logic (no transforms).
    /// This is the main dispatch method for emission.
    fn emit_node_by_kind(&mut self, node: &ThinNode, idx: NodeIndex, kind: u16) {
        match kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => {
                self.emit_identifier(node);
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                self.emit_type_parameter(node);
            }

            // Literals
            k if k == SyntaxKind::NumericLiteral as u16 => {
                self.emit_numeric_literal(node);
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.emit_string_literal(node);
            }
            k if k == SyntaxKind::TrueKeyword as u16 => {
                self.write("true");
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                self.write("false");
            }
            k if k == SyntaxKind::NullKeyword as u16 => {
                self.write("null");
            }

            // Binary expression
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.emit_binary_expression(node);
            }

            // Unary expressions
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.emit_prefix_unary(node);
            }
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                self.emit_postfix_unary(node);
            }

            // Call expression
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.emit_call_expression(node);
            }

            // New expression
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.emit_new_expression(node);
            }

            // Property access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.emit_property_access(node);
            }

            // Element access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.emit_element_access(node);
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.emit_parenthesized(node);
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.emit_type_assertion_expression(node);
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                self.emit_non_null_expression(node);
            }

            // Conditional expression
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.emit_conditional(node);
            }

            // Array literal
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.emit_array_literal(node);
            }

            // Object literal
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.emit_object_literal(node);
            }

            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                self.emit_arrow_function(node, idx);
            }

            // Function expression
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.emit_function_expression(node, idx);
            }

            // Function declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_declaration(node, idx);
            }

            // Variable declaration
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.emit_variable_declaration(node);
            }

            // Variable declaration list
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                self.emit_variable_declaration_list(node);
            }

            // Variable statement
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_statement(node);
            }

            // Expression statement
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.emit_expression_statement(node);
            }

            // Block
            k if k == syntax_kind_ext::BLOCK => {
                self.emit_block(node);
            }

            // If statement
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.emit_if_statement(node);
            }

            // While statement
            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.emit_while_statement(node);
            }

            // For statement
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.emit_for_statement(node);
            }

            // For-in statement
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => {
                self.emit_for_in_statement(node);
            }

            // For-of statement
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                self.emit_for_of_statement(node);
            }

            // Return statement
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                self.emit_return_statement(node);
            }

            // Class declaration
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_declaration(node, idx);
            }

            // Property assignment
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.emit_property_assignment(node);
            }

            // Shorthand property assignment
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                self.emit_shorthand_property(node);
            }

            // Parameter declaration
            k if k == syntax_kind_ext::PARAMETER => {
                self.emit_parameter(node);
            }

            // Type keywords (for type annotations)
            k if k == SyntaxKind::NumberKeyword as u16 => self.write("number"),
            k if k == SyntaxKind::StringKeyword as u16 => self.write("string"),
            k if k == SyntaxKind::BooleanKeyword as u16 => self.write("boolean"),
            k if k == SyntaxKind::VoidKeyword as u16 => self.write("void"),
            k if k == SyntaxKind::AnyKeyword as u16 => self.write("any"),
            k if k == SyntaxKind::NeverKeyword as u16 => self.write("never"),
            k if k == SyntaxKind::UnknownKeyword as u16 => self.write("unknown"),
            k if k == SyntaxKind::UndefinedKeyword as u16 => self.write("undefined"),
            k if k == SyntaxKind::ObjectKeyword as u16 => self.write("object"),
            k if k == SyntaxKind::SymbolKeyword as u16 => self.write("symbol"),
            k if k == SyntaxKind::BigIntKeyword as u16 => self.write("bigint"),

            // Type reference
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                self.emit_type_reference(node);
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                self.emit_array_type(node);
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                self.emit_union_type(node);
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                self.emit_intersection_type(node);
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                self.emit_tuple_type(node);
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                self.emit_function_type(node);
            }

            // Type literal
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                self.emit_type_literal(node);
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                self.emit_parenthesized_type(node);
            }

            // Empty statement
            k if k == syntax_kind_ext::EMPTY_STATEMENT => {
                self.write_semicolon();
            }

            // JSX
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                self.emit_jsx_element(node);
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.emit_jsx_self_closing_element(node);
            }
            k if k == syntax_kind_ext::JSX_OPENING_ELEMENT => {
                self.emit_jsx_opening_element(node);
            }
            k if k == syntax_kind_ext::JSX_CLOSING_ELEMENT => {
                self.emit_jsx_closing_element(node);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                self.emit_jsx_fragment(node);
            }
            k if k == syntax_kind_ext::JSX_OPENING_FRAGMENT => {
                self.write("<>");
            }
            k if k == syntax_kind_ext::JSX_CLOSING_FRAGMENT => {
                self.write("</>");
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTES => {
                self.emit_jsx_attributes(node);
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                self.emit_jsx_attribute(node);
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                self.emit_jsx_spread_attribute(node);
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                self.emit_jsx_expression(node);
            }
            k if k == SyntaxKind::JsxText as u16 => {
                self.emit_jsx_text(node);
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                self.emit_jsx_namespaced_name(node);
            }

            // Imports/Exports
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.emit_import_declaration(node);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.emit_import_equals_declaration(node);
            }
            k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                self.emit_import_clause(node);
            }
            k if k == syntax_kind_ext::NAMED_IMPORTS || k == syntax_kind_ext::NAMESPACE_IMPORT => {
                self.emit_named_imports(node);
            }
            k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                self.emit_import_specifier(node);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(node);
            }
            k if k == syntax_kind_ext::NAMED_EXPORTS => {
                self.emit_named_exports(node);
            }
            k if k == syntax_kind_ext::EXPORT_SPECIFIER => {
                self.emit_export_specifier(node);
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.emit_export_assignment(node);
            }

            // Additional statements
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                self.emit_throw_statement(node);
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.emit_try_statement(node);
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                self.emit_catch_clause(node);
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.emit_switch_statement(node);
            }
            k if k == syntax_kind_ext::CASE_CLAUSE => {
                self.emit_case_clause(node);
            }
            k if k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.emit_default_clause(node);
            }
            k if k == syntax_kind_ext::CASE_BLOCK => {
                self.emit_case_block(node);
            }
            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                self.emit_break_statement();
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                self.emit_continue_statement();
            }
            k if k == syntax_kind_ext::DO_STATEMENT => {
                self.emit_do_statement(node);
            }
            k if k == syntax_kind_ext::DEBUGGER_STATEMENT => {
                self.emit_debugger_statement();
            }

            // Declarations
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_declaration(node, idx);
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                self.emit_enum_member(node);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                // Interface declarations are TypeScript-only - skip for JavaScript
                // self.emit_interface_declaration(node);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                // Type alias declarations are TypeScript-only - skip for JavaScript
                // self.emit_type_alias_declaration(node);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(node, idx);
            }

            // Class members
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.emit_method_declaration(node);
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(node);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.emit_constructor_declaration(node);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.emit_get_accessor(node);
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.emit_set_accessor(node);
            }
            k if k == syntax_kind_ext::DECORATOR => {
                self.emit_decorator(node);
            }

            // Interface/type members (signatures)
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                self.emit_property_signature(node);
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                self.emit_method_signature(node);
            }
            k if k == syntax_kind_ext::CALL_SIGNATURE => {
                self.emit_call_signature(node);
            }
            k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                self.emit_construct_signature(node);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.emit_index_signature(node);
            }

            // Template literals
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.emit_tagged_template_expression(node, idx);
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.emit_template_expression(node);
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                self.emit_no_substitution_template(node);
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                self.emit_template_span(node);
            }
            k if k == SyntaxKind::TemplateHead as u16 => {
                self.emit_template_head(node);
            }
            k if k == SyntaxKind::TemplateMiddle as u16 => {
                self.emit_template_middle(node);
            }
            k if k == SyntaxKind::TemplateTail as u16 => {
                self.emit_template_tail(node);
            }

            // Yield/Await/Spread
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                self.emit_yield_expression(node);
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                self.emit_await_expression(node);
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                self.emit_spread_element(node);
            }

            // Source file
            k if k == syntax_kind_ext::SOURCE_FILE => {
                self.emit_source_file(node);
            }

            // Other tokens and keywords - emit their text
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // In ES5 mode inside an arrow function body, use _this instead of this
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this")
                } else {
                    self.write("this")
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),

            // Binding patterns (for destructuring)
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                // When emitting as-is (non-ES5 or for parameters), just emit the pattern
                self.emit_object_binding_pattern(node);
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.emit_array_binding_pattern(node);
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                self.emit_binding_element(node);
            }

            // Default: do nothing (or handle other cases as needed)
            _ => {}
        }
    }

    // =========================================================================
    // Yield and Await
    // =========================================================================

    fn emit_yield_expression(&mut self, node: &ThinNode) {
        // YieldExpression is stored with UnaryExprData (operand = expression, operator = asterisk flag)
        let Some(unary) = self.arena.get_unary_expr(node) else {
            self.write("yield");
            return;
        };

        self.write("yield");
        // Check if this is yield* (operator stores asterisk flag as SyntaxKind)
        if unary.operator == crate::scanner::SyntaxKind::AsteriskToken as u16 {
            self.write("*");
        }
        if !unary.operand.is_none() {
            self.write(" ");
            self.emit_expression(unary.operand);
        }
    }

    fn emit_await_expression(&mut self, node: &ThinNode) {
        // AwaitExpression is stored with UnaryExprData
        let Some(unary) = self.arena.get_unary_expr(node) else {
            self.write("await");
            return;
        };

        self.write("await ");
        self.emit_expression(unary.operand);
    }

    fn emit_spread_element(&mut self, node: &ThinNode) {
        let Some(spread) = self.arena.get_spread(node) else {
            self.write("...");
            return;
        };

        self.write("...");
        self.emit_expression(spread.expression);
    }

    // =========================================================================
    // Decorators
    // =========================================================================

    fn emit_decorator(&mut self, node: &ThinNode) {
        let Some(decorator) = self.arena.get_decorator(node) else {
            return;
        };

        self.write("@");
        self.emit(decorator.expression);
    }

    // =========================================================================
    // Source File
    // =========================================================================

    fn emit_source_file(&mut self, node: &ThinNode) {
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        // Auto-detect module: if enabled and file has imports/exports, switch to CommonJS
        if self.ctx.auto_detect_module && self.file_is_module(&source.statements) {
            self.ctx.options.module = ModuleKind::CommonJS;
        }

        // Detect export assignment (export =) to suppress other exports
        if self.has_export_assignment(&source.statements) {
            self.ctx.module_state.has_export_assignment = true;
        }

        // Extract and filter comments (strip compiler directives)
        let all_comments = if !self.ctx.options.remove_comments {
            if let Some(text) = self.source_text {
                crate::comments::get_comment_ranges(text)
                    .into_iter()
                    .filter(|c| {
                        // Filter out triple-slash directives (/// <reference ..., /// <amd ...)
                        // TypeScript strips these from JS output
                        let content = c.get_text(text);
                        !content.starts_with("/// <reference") && !content.starts_with("/// <amd")
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let mut comment_idx = 0;

        // CommonJS: Emit "use strict" FIRST (before comments and helpers)
        if self.ctx.is_commonjs() {
            self.write("\"use strict\";");
            self.write_line();
        }

        // Emit header comments AFTER "use strict" but BEFORE helpers
        let first_stmt_pos = source
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map(|n| n.pos)
            .unwrap_or(node.end);

        if let Some(text) = self.source_text {
            while comment_idx < all_comments.len() {
                let comment = &all_comments[comment_idx];
                if comment.end <= first_stmt_pos {
                    let comment_text = comment.get_text(text);
                    self.write(comment_text);
                    if comment.has_trailing_new_line {
                        self.write_line();
                    }
                    comment_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Emit runtime helpers (must come BEFORE __esModule marker)
        // Order: "use strict" → helpers → __esModule → exports init
        let mut helpers = crate::transforms::helpers::HelpersNeeded::default();

        // Detect CommonJS import/export helpers
        if self.ctx.is_commonjs() {
            self.detect_commonjs_helpers(&source.statements, &mut helpers);
        }

        let has_es5_transforms = self.has_es5_transforms();
        if has_es5_transforms {
            if self.transforms.helpers_populated() {
                let es5_helpers = self.transforms.helpers();
                helpers.extends |= es5_helpers.extends;
                helpers.values |= es5_helpers.values;
                helpers.rest |= es5_helpers.rest;
                helpers.awaiter |= es5_helpers.awaiter;
                helpers.generator |= es5_helpers.generator;
                helpers.make_template_object |= es5_helpers.make_template_object;
                helpers.class_private_field_get |= es5_helpers.class_private_field_get;
                helpers.class_private_field_set |= es5_helpers.class_private_field_set;
                helpers.decorate |= es5_helpers.decorate;
            } else {
                if self.needs_extends_helper(&source.statements) {
                    helpers.extends = true;
                }

                if self.needs_values_helper() {
                    helpers.values = true;
                }
                if self.needs_rest_helper() {
                    helpers.rest = true;
                }
                if self.needs_async_helpers() {
                    helpers.awaiter = true;
                    helpers.generator = true;
                }
                if self.needs_class_private_field_helpers() {
                    helpers.class_private_field_get = true;
                    helpers.class_private_field_set = true;
                }
            }
        } else if self.ctx.target_es5 {
            if self.needs_async_helpers() {
                helpers.awaiter = true;
                helpers.generator = true;
            }
        }

        // Emit all needed helpers
        let helpers_code = crate::transforms::helpers::emit_helpers(&helpers);
        if !helpers_code.is_empty() {
            self.write(&helpers_code);
            // emit_helpers() already adds newlines, no need to add more
        }

        if has_es5_transforms && helpers.make_template_object {
            let template_vars = self.collect_tagged_template_vars();
            if !template_vars.is_empty() {
                self.write("var ");
                self.write(&template_vars.join(", "));
                self.write(";");
                self.write_line();
            }
        }

        // CommonJS: Emit __esModule and exports initialization (AFTER helpers)
        if self.ctx.is_commonjs() {
            use crate::transforms::module_commonjs;

            // Emit __esModule if this is an ES module
            if self.should_emit_es_module_marker(&source.statements) {
                self.write("Object.defineProperty(exports, \"__esModule\", { value: true });");
                self.write_line();
            }

            // Collect and emit exports initialization
            let export_names =
                module_commonjs::collect_export_names(self.arena, &source.statements.nodes);
            if !export_names.is_empty() {
                for (i, name) in export_names.iter().enumerate() {
                    if i > 0 {
                        self.write(" = ");
                    }
                    self.write("exports.");
                    self.write(name);
                }
                self.write(" = void 0;");
                self.write_line();
            }
        }

        // Emit statements with their comments
        for &stmt_idx in &source.statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                // Emit any comments that appear before this statement
                if let Some(text) = self.source_text {
                    while comment_idx < all_comments.len() {
                        let comment = &all_comments[comment_idx];
                        if comment.end <= stmt_node.pos {
                            // This comment is before the statement, emit it
                            let comment_text = comment.get_text(text);
                            self.write(comment_text);
                            // Only add newline if the comment has a trailing newline
                            if comment.has_trailing_new_line {
                                self.write_line();
                            }
                            comment_idx += 1;
                        } else {
                            // This comment is after the statement start, stop
                            break;
                        }
                    }
                }
            }

            let before_len = self.writer.len();
            self.emit(stmt_idx);
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                self.write_line();
            }
        }

        // Emit remaining trailing comments at the end of file
        if let Some(text) = self.source_text {
            while comment_idx < all_comments.len() {
                let comment = &all_comments[comment_idx];
                let comment_text = comment.get_text(text);
                self.write(comment_text);
                if comment.has_trailing_new_line {
                    self.write_line();
                }
                comment_idx += 1;
            }
        }
    }

    // =========================================================================
    // Binding Patterns (Destructuring)
    // =========================================================================

    /// Emit an object binding pattern: { x, y }
    fn emit_object_binding_pattern(&mut self, node: &ThinNode) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        self.write("{ ");
        self.emit_comma_separated(&pattern.elements.nodes);
        self.write(" }");
    }

    /// Emit an array binding pattern: [x, y]
    fn emit_array_binding_pattern(&mut self, node: &ThinNode) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        self.write("[");
        self.emit_comma_separated(&pattern.elements.nodes);
        self.write("]");
    }

    /// Emit a binding element: x or x = default or propertyName: x
    fn emit_binding_element(&mut self, node: &ThinNode) {
        let Some(elem) = self.arena.get_binding_element(node) else {
            return;
        };

        // Rest element: ...x
        if elem.dot_dot_dot_token {
            self.write("...");
        }

        // propertyName: name  or just name
        if !elem.property_name.is_none() {
            self.emit(elem.property_name);
            self.write(": ");
        }

        self.emit(elem.name);

        // Default value: = expr
        if !elem.initializer.is_none() {
            self.write(" = ");
            self.emit(elem.initializer);
        }
    }

    /// Get the next temporary variable name (_a, _b, _c, etc.)
    fn get_temp_var_name(&mut self) -> String {
        let name = format!(
            "_{}",
            (b'a' + (self.ctx.destructuring_state.temp_var_counter % 26) as u8) as char
        );
        self.ctx.destructuring_state.temp_var_counter += 1;
        name
    }

    /// Check if a node is a binding pattern
    fn is_binding_pattern(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
    }
}

// =============================================================================
// Operator Text Helper
// =============================================================================

fn is_valid_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first == '$' || first.is_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric())
}

fn get_operator_text(op: u16) -> &'static str {
    match op {
        k if k == SyntaxKind::PlusToken as u16 => "+",
        k if k == SyntaxKind::MinusToken as u16 => "-",
        k if k == SyntaxKind::AsteriskToken as u16 => "*",
        k if k == SyntaxKind::SlashToken as u16 => "/",
        k if k == SyntaxKind::PercentToken as u16 => "%",
        k if k == SyntaxKind::AsteriskAsteriskToken as u16 => "**",
        k if k == SyntaxKind::PlusPlusToken as u16 => "++",
        k if k == SyntaxKind::MinusMinusToken as u16 => "--",
        k if k == SyntaxKind::LessThanToken as u16 => "<",
        k if k == SyntaxKind::GreaterThanToken as u16 => ">",
        k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
        k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
        k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
        k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
        k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
        k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
        k if k == SyntaxKind::EqualsToken as u16 => "=",
        k if k == SyntaxKind::PlusEqualsToken as u16 => "+=",
        k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
        k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=",
        k if k == SyntaxKind::SlashEqualsToken as u16 => "/=",
        k if k == SyntaxKind::PercentEqualsToken as u16 => "%=",
        k if k == SyntaxKind::AmpersandToken as u16 => "&",
        k if k == SyntaxKind::BarToken as u16 => "|",
        k if k == SyntaxKind::CaretToken as u16 => "^",
        k if k == SyntaxKind::TildeToken as u16 => "~",
        k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
        k if k == SyntaxKind::BarBarToken as u16 => "||",
        k if k == SyntaxKind::ExclamationToken as u16 => "!",
        k if k == SyntaxKind::QuestionQuestionToken as u16 => "??",
        k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<",
        k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => ">>>",
        _ => "",
    }
}

#[cfg(test)]
mod comment_tests {
    use super::*;

    #[test]
    fn test_trailing_comments_parsing() {
        let text = "constructor(public p3:any) {} // OK";
        //                                       ^
        //                                       position 29 (after the closing brace)
        let comments = get_trailing_comment_ranges(text, 29);
        assert_eq!(comments.len(), 1);
        assert_eq!(
            &text[comments[0].pos as usize..comments[0].end as usize],
            "// OK"
        );
    }

    #[test]
    fn test_trailing_comments_with_space() {
        let text = "} // OK\n";
        let comments = get_trailing_comment_ranges(text, 1); // after }
        assert_eq!(comments.len(), 1);
        assert_eq!(
            &text[comments[0].pos as usize..comments[0].end as usize],
            "// OK"
        );
    }
}
