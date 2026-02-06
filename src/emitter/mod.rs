//! Emitter - Emitter using NodeArena
//!
//! This emitter uses the Node architecture for cache-optimized AST access.
//! It works directly with NodeArena instead of the old Node enum.
//!
//! # Architecture
//!
//! - Uses NodeArena for AST access (16-byte nodes, 13x cache improvement)
//! - Dispatches based on Node.kind (u16)
//! - Uses accessor methods to get typed node data
//!
//! # Module Organization
//!
//! The emitter is organized as a directory module:
//! - `mod.rs` - Core Printer struct, dispatch logic, and emit methods
//! - `expressions.rs` - Expression emission helpers
//! - `statements.rs` - Statement emission helpers
//! - `declarations.rs` - Declaration emission helpers
//! - `functions.rs` - Function emission helpers
//! - `types.rs` - Type emission helpers
//! - `jsx.rs` - JSX emission helpers
//!
//! Note: pub(super) fields and methods allow future submodules to access Printer internals.

#![allow(clippy::print_stderr)]

use crate::emit_context::EmitContext;
use crate::parser::NodeIndex;
use crate::parser::node::{Node, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use crate::transform_context::{IdentifierId, TransformContext, TransformDirective};
use crate::transforms::{ClassES5Emitter, EnumES5Emitter, NamespaceES5Emitter};
use std::sync::Arc;

mod binding_patterns;
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
mod special_expressions;
mod statements;
mod template_literals;
pub mod type_printer;
mod types;

pub use comments::{
    CommentKind, CommentRange, get_leading_comment_ranges, get_trailing_comment_ranges,
};

// Re-export common types for backward compatibility
pub use crate::common::{ModuleKind, NewLineKind, ScriptTarget};

// =============================================================================
// Emitter Options
// =============================================================================

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
        should_declare_var: bool,
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
    ES5ArrayLiteral {
        array_literal: NodeIndex,
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
// Printer
// =============================================================================

/// Maximum recursion depth for emit to prevent infinite loops
const MAX_EMIT_RECURSION_DEPTH: u32 = 1000;

/// Printer that works with NodeArena.
///
/// Uses SourceWriter for output generation (enables source map support).
/// Uses EmitContext for transform-specific state management.
/// Uses TransformContext for directive-based transforms (Phase 2 architecture).
pub struct Printer<'a> {
    /// The NodeArena containing the AST.
    pub(super) arena: &'a NodeArena,

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

    /// Last processed position in source text for comment gap detection.
    ///
    /// This tracks the last source position that was processed, enabling detection
    /// of comment gaps between statements. Used by fine-grained comment emission
    /// to identify where comments should be inserted relative to emitted code.
    ///
    /// Note: Currently used for basic comment emission in emit_source_file.
    /// Future improvements could use this for more precise comment positioning.
    pub(super) last_processed_pos: u32,

    /// Pending source position for mapping the next write.
    pub(super) pending_source_pos: Option<SourcePosition>,

    /// Recursion depth counter to prevent infinite loops
    emit_recursion_depth: u32,

    /// All comments in the source file, collected once during emit_source_file.
    /// Used for distributing comments to blocks and other nested constructs.
    pub(super) all_comments: Vec<crate::comments::CommentRange>,

    /// Shared index into all_comments, monotonically advancing as comments are emitted.
    /// Used across emit_source_file and emit_block to prevent double-emission.
    pub(super) comment_emit_idx: usize,
}

impl<'a> Printer<'a> {
    /// Create a new Printer.
    pub fn new(arena: &'a NodeArena) -> Self {
        Self::with_options(arena, PrinterOptions::default())
    }

    /// Create a new Printer with pre-allocated output capacity
    /// This reduces allocations when the expected output size is known (e.g., ~1.5x source size)
    pub fn with_capacity(arena: &'a NodeArena, capacity: usize) -> Self {
        Self::with_capacity_and_options(arena, capacity, PrinterOptions::default())
    }

    /// Create a new Printer with options.
    pub fn with_options(arena: &'a NodeArena, options: PrinterOptions) -> Self {
        Self::with_capacity_and_options(arena, 1024, options)
    }

    /// Create a new Printer with pre-allocated capacity and options.
    pub fn with_capacity_and_options(
        arena: &'a NodeArena,
        capacity: usize,
        options: PrinterOptions,
    ) -> Self {
        let mut writer = SourceWriter::with_capacity(capacity);
        writer.set_new_line_kind(options.new_line);

        // Create EmitContext from options (target controls ES5 vs ESNext)
        let ctx = EmitContext::with_options(options);

        Printer {
            arena,
            writer,
            ctx,
            transforms: TransformContext::new(), // Empty by default, can be set later
            emit_missing_initializer_as_void_0: false,
            source_text: None,
            source_map_text: None,
            last_processed_pos: 0,
            pending_source_pos: None,
            emit_recursion_depth: 0,
            all_comments: Vec::new(),
            comment_emit_idx: 0,
        }
    }

    /// Create a new Printer with transform directives.
    /// This is the Phase 2 constructor that accepts pre-computed transforms.
    pub fn with_transforms(arena: &'a NodeArena, transforms: TransformContext) -> Self {
        let mut printer = Self::new(arena);
        printer.transforms = transforms;
        printer
    }

    /// Create a new Printer with transforms and options.
    pub fn with_transforms_and_options(
        arena: &'a NodeArena,
        transforms: TransformContext,
        options: PrinterOptions,
    ) -> Self {
        let mut printer = Self::with_options(arena, options);
        printer.transforms = transforms;
        printer
    }

    /// Create a new Printer targeting ES5.
    pub fn new_es5(arena: &'a NodeArena) -> Self {
        let mut options = PrinterOptions::default();
        options.target = ScriptTarget::ES5;
        Self::with_options(arena, options)
    }

    /// Create a new Printer targeting ES6+.
    pub fn new_es6(arena: &'a NodeArena) -> Self {
        let mut options = PrinterOptions::default();
        options.target = ScriptTarget::ES2015;
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

    fn queue_source_mapping(&mut self, node: &Node) {
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
    /// Finds the first `{` and last `}` within the node's source span and checks
    /// if there's a newline between them. Uses `rfind` to handle nested braces correctly.
    fn is_single_line(&self, node: &Node) -> bool {
        if let Some(text) = self.source_text {
            let start = node.pos as usize;
            let end = std::cmp::min(node.end as usize, text.len());
            if start < end {
                let slice = &text[start..end];
                // Find the first `{` and last `}` in the node's range
                if let Some(open) = slice.find('{') {
                    if let Some(close) = slice.rfind('}') {
                        if close > open {
                            let inner = &slice[open..close + 1];
                            return !inner.contains('\n');
                        }
                    }
                }
                // Fallback: check entire span for newlines
                return !slice.contains('\n');
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
            TransformDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => EmitDirective::ES5Namespace {
                namespace_node: *namespace_node,
                should_declare_var: *should_declare_var,
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
            TransformDirective::ES5ArrayLiteral { array_literal } => {
                EmitDirective::ES5ArrayLiteral {
                    array_literal: *array_literal,
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
    fn apply_transform(&mut self, node: &Node, idx: NodeIndex) {
        let Some(directive) = self.transforms.get(idx) else {
            // No transform, emit normally (should not happen if has_transform returned true)
            self.emit_node_default(node, idx);
            return;
        };

        let directive = Self::emit_directive_from_transform(directive);
        let debug_emit = std::env::var_os("TSZ_DEBUG_EMIT").is_some();

        match directive {
            EmitDirective::Identity => {
                // No transformation needed, emit as-is
                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5Class { class_node } => {
                if debug_emit {
                    println!(
                        "TSZ_DEBUG_EMIT: Printer ES5Class start (idx={}, class_node={})",
                        idx.0, class_node.0
                    );
                }
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
                if debug_emit {
                    println!(
                        "TSZ_DEBUG_EMIT: Printer ES5Class end (idx={}, class_node={}, output_len={})",
                        idx.0,
                        class_node.0,
                        es5_output.len()
                    );
                }
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

            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(should_declare_var);
                let output = ns_emitter.emit_namespace(namespace_node);
                self.write(output.trim_end_matches('\n'));
            }

            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                let output = enum_emitter.emit_enum(enum_node);
                self.write(output.trim_end_matches('\n'));
            }

            EmitDirective::CommonJSExport {
                names,
                is_default,
                inner,
            } => {
                // For exported variable declarations with no initializers (e.g.,
                // `export var x: number;`), skip entirely. The preamble
                // `exports.x = void 0;` already handles the forward declaration.
                let skip = node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    && self.arena.get_variable(node).is_some_and(|var_data| {
                        self.all_declarations_lack_initializer(&var_data.declarations)
                    });

                if !skip {
                    let export_name = names.first().copied();
                    self.emit_commonjs_export(names.as_ref(), is_default, |this| {
                        this.emit_commonjs_inner(node, idx, inner.as_ref(), export_name);
                    });
                }
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
                if let Some(arrow_node) = self.arena.get(arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(arrow_node, func, captures_this);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    let func_name = if !func.name.is_none() {
                        self.get_identifier_text_idx(func.name)
                    } else {
                        String::new()
                    };

                    self.emit_async_function_es5(func, &func_name, "this");
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ForOf { for_of_node } => {
                if let Some(for_of_node) = self.arena.get(for_of_node)
                    && let Some(for_in_of) = self.arena.get_for_in_of(for_of_node)
                    && !for_in_of.await_modifier
                {
                    self.emit_for_of_statement_es5(for_in_of);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ObjectLiteral { object_literal } => {
                if let Some(literal_node) = self.arena.get(object_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    self.emit_object_literal_es5(&literal.elements.nodes);
                    return;
                }

                self.emit_node_default(node, idx);
            }

            EmitDirective::ES5ArrayLiteral { array_literal } => {
                if let Some(literal_node) = self.arena.get(array_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    self.emit_array_literal_es5(&literal.elements.nodes);
                    return;
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
        node: &Node,
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
            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(*should_declare_var);
                let output = ns_emitter.emit_namespace(*namespace_node);
                self.write(output.trim_end_matches('\n'));
            }
            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                let output = enum_emitter.emit_enum(*enum_node);
                self.write(output.trim_end_matches('\n'));
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
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
            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
            } => {
                if let Some(arrow_node) = self.arena.get(*arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(arrow_node, func, *captures_this);
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
        node: &Node,
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
        node: &Node,
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
            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter.set_should_declare_var(*should_declare_var);
                let output = ns_emitter.emit_namespace(*namespace_node);
                self.write(output.trim_end_matches('\n'));
            }
            EmitDirective::ES5Enum { enum_node } => {
                let mut enum_emitter = EnumES5Emitter::new(self.arena);
                enum_emitter.set_indent_level(self.writer.indent_level());
                let output = enum_emitter.emit_enum(*enum_node);
                self.write(output.trim_end_matches('\n'));
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
                if let Some(arrow_node) = self.arena.get(*arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(arrow_node, func, *captures_this);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    let func_name = if !func.name.is_none() {
                        self.get_identifier_text_idx(func.name)
                    } else {
                        String::new()
                    };

                    self.emit_async_function_es5(func, &func_name, "this");
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ForOf { for_of_node } => {
                if let Some(for_of_node) = self.arena.get(*for_of_node)
                    && let Some(for_in_of) = self.arena.get_for_in_of(for_of_node)
                    && !for_in_of.await_modifier
                {
                    self.emit_for_of_statement_es5(for_in_of);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ObjectLiteral { object_literal } => {
                if let Some(literal_node) = self.arena.get(*object_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    self.emit_object_literal_es5(&literal.elements.nodes);
                    return;
                }

                self.emit_chained_previous(node, idx, directives, index);
            }
            EmitDirective::ES5ArrayLiteral { array_literal } => {
                if let Some(literal_node) = self.arena.get(*array_literal)
                    && let Some(literal) = self.arena.get_literal_expr(literal_node)
                {
                    self.emit_array_literal_es5(&literal.elements.nodes);
                    return;
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
        node: &Node,
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
    fn emit_node_default(&mut self, node: &Node, idx: NodeIndex) {
        // Emit the node without consulting transform directives.
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

        if let Some(source) = self.arena.get_source_file(node)
            && self.transforms.is_empty()
        {
            let format = match self.ctx.options.module {
                ModuleKind::AMD => Some(crate::transform_context::ModuleFormat::AMD),
                ModuleKind::UMD => Some(crate::transform_context::ModuleFormat::UMD),
                ModuleKind::System => Some(crate::transform_context::ModuleFormat::System),
                _ => None,
            };
            if let Some(format) = format
                && self.file_is_module(&source.statements)
            {
                let dependencies = self.collect_module_dependencies(&source.statements.nodes);
                self.emit_module_wrapper(&format, &dependencies, node, source);
                return;
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
    fn emit_node(&mut self, node: &Node, idx: NodeIndex) {
        // Recursion depth check to prevent infinite loops
        self.emit_recursion_depth += 1;
        if self.emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH {
            // Log a warning to stderr about the recursion limit being exceeded.
            // This helps developers identify problematic deeply nested ASTs.
            eprintln!(
                "Warning: emit recursion limit ({}) exceeded at node kind={} pos={}",
                MAX_EMIT_RECURSION_DEPTH, node.kind, node.pos
            );
            self.write("/* emit recursion limit exceeded */");
            self.emit_recursion_depth -= 1;
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
        self.emit_recursion_depth -= 1;
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
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Emit a node by kind using default logic (no transforms).
    /// This is the main dispatch method for emission.
    fn emit_node_by_kind(&mut self, node: &Node, idx: NodeIndex, kind: u16) {
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
                self.emit_break_statement(node);
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                self.emit_continue_statement(node);
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                self.emit_labeled_statement(node);
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
                // Interface declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_interface_declaration(node);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                // Type alias declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_type_alias_declaration(node);
                }
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
    // Source File
    // =========================================================================

    fn emit_source_file(&mut self, node: &Node) {
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        // Auto-detect module: if enabled and module is None (not explicitly set),
        // switch to CommonJS when file has imports/exports.
        // Do NOT override explicit module targets like ES2015/ESNext.
        if self.ctx.auto_detect_module
            && matches!(self.ctx.options.module, ModuleKind::None)
            && self.file_is_module(&source.statements)
        {
            self.ctx.options.module = ModuleKind::CommonJS;
        }

        // Detect export assignment (export =) to suppress other exports
        if self.has_export_assignment(&source.statements) {
            self.ctx.module_state.has_export_assignment = true;
        }

        // Extract comments. Triple-slash references (/// <reference ...>) are
        // preserved in output (TypeScript keeps them in JS emit).
        // Only AMD-specific directives (/// <amd ...) are stripped.
        // Store on self so nested blocks can also distribute comments.
        self.all_comments = if !self.ctx.options.remove_comments {
            if let Some(text) = self.source_text {
                crate::comments::get_comment_ranges(text)
                    .into_iter()
                    .filter(|c| {
                        let content = c.get_text(text);
                        !content.starts_with("/// <amd")
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        self.comment_emit_idx = 0;

        // Emit "use strict" FIRST (before comments and helpers)
        // TypeScript emits "use strict" when:
        // 1. Module is CommonJS/AMD/UMD
        // 2. alwaysStrict compiler option is enabled
        // 3. File is an ES module with target < ES2015
        // For now, we emit for CommonJS (most common case)
        // TODO: Add always_strict support to PrinterOptions
        let is_es_module = self.file_is_module(&source.statements);
        let is_commonjs_or_amd = matches!(
            self.ctx.options.module,
            ModuleKind::CommonJS | ModuleKind::AMD | ModuleKind::UMD
        );
        let target_before_es6 = !self.ctx.options.target.supports_es2015();

        // Emit for CommonJS/AMD/UMD OR (ES modules with target < ES6)
        if is_commonjs_or_amd || (is_es_module && target_before_es6) {
            self.write("\"use strict\";");
            self.write_line();
        }

        // Emit header comments AFTER "use strict" but BEFORE helpers.
        // Use skip_trivia_forward to find the actual token start since
        // node.pos may include leading trivia (where comments live).
        let first_stmt_pos = source
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map(|n| self.skip_trivia_forward(n.pos, n.end))
            .unwrap_or(node.end);

        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= first_stmt_pos {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    let comment_text =
                        crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    self.write(comment_text);
                    if c_trailing {
                        self.write_line();
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Emit runtime helpers (must come BEFORE __esModule marker)
        // Order: "use strict" → helpers → __esModule → exports init

        // Use helpers from TransformContext (populated during lowering pass)
        // This eliminates O(N) arena scans - all helpers are detected in Phase 1
        let helpers = if self.transforms.helpers_populated() {
            self.transforms.helpers().clone()
        } else {
            // Fallback for non-transforming emits (should be rare)
            // In normal operation, LoweringPass always marks helpers_populated = true
            crate::transforms::helpers::HelpersNeeded::default()
        };

        let has_es5_transforms = self.has_es5_transforms();

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
            // Function exports get direct assignment (hoisted), others get void 0
            let (func_exports, other_exports) = module_commonjs::collect_export_names_categorized(
                self.arena,
                &source.statements.nodes,
            );
            // Emit other exports first: exports.X = void 0;
            // TypeScript emits void 0 initialization before hoisted function exports
            if !other_exports.is_empty() {
                for (i, name) in other_exports.iter().enumerate() {
                    if i > 0 {
                        self.write(" = ");
                    }
                    self.write("exports.");
                    self.write(name);
                }
                self.write(" = void 0;");
                self.write_line();
            }
            // Emit function exports: exports.compile = compile;
            for name in &func_exports {
                self.write("exports.");
                self.write(name);
                self.write(" = ");
                self.write(name);
                self.write(";");
                self.write_line();
            }
        }

        // Emit statements with their leading comments.
        // In this parser, node.pos includes leading trivia (whitespace + comments).
        // Between-statement comments are part of the next node's leading trivia.
        // We find each statement's "actual token start" by scanning forward past
        // trivia, then emit all comments before that position.
        for &stmt_idx in &source.statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                // Find the actual start of the statement's first token by
                // scanning forward from node.pos past whitespace and comments
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);

                // Emit comments whose end position is at or before the actual token start
                if let Some(text) = self.source_text {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        if c_end <= actual_start {
                            let c_pos = self.all_comments[self.comment_emit_idx].pos;
                            let c_trailing =
                                self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                            let comment_text = crate::printer::safe_slice::slice(
                                text,
                                c_pos as usize,
                                c_end as usize,
                            );
                            self.write(comment_text);
                            if c_trailing {
                                self.write_line();
                            }
                            self.comment_emit_idx += 1;
                        } else {
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

            // Note: We do NOT skip inner comments here. The "emit comments before
            // statement" logic (above) uses actual_start which is computed by
            // skip_trivia_forward. Inner comments (inside function/class bodies)
            // have positions that are BEFORE the next top-level statement's actual
            // start, so they won't be emitted at the wrong level. They'll be
            // naturally consumed when we encounter the statement that contains them.
        }

        // Emit remaining trailing comments at the end of file
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_pos = self.all_comments[self.comment_emit_idx].pos;
                let c_end = self.all_comments[self.comment_emit_idx].end;
                let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                let comment_text =
                    crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                self.write(comment_text);
                if c_trailing {
                    self.write_line();
                }
                self.comment_emit_idx += 1;
            }
        }
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
        k if k == SyntaxKind::InstanceOfKeyword as u16 => "instanceof",
        k if k == SyntaxKind::InKeyword as u16 => "in",
        k if k == SyntaxKind::TypeOfKeyword as u16 => "typeof ",
        k if k == SyntaxKind::VoidKeyword as u16 => "void ",
        k if k == SyntaxKind::DeleteKeyword as u16 => "delete ",
        k if k == SyntaxKind::CommaToken as u16 => ",",
        k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**=",
        k if k == SyntaxKind::AmpersandEqualsToken as u16 => "&=",
        k if k == SyntaxKind::BarEqualsToken as u16 => "|=",
        k if k == SyntaxKind::CaretEqualsToken as u16 => "^=",
        k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<=",
        k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>=",
        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => ">>>=",
        k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => "&&=",
        k if k == SyntaxKind::BarBarEqualsToken as u16 => "||=",
        k if k == SyntaxKind::QuestionQuestionEqualsToken as u16 => "??=",
        _ => "",
    }
}

#[cfg(test)]
#[path = "tests/comment_tests.rs"]
mod comment_tests;
