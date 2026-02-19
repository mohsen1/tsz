//! Emitter - Emitter using `NodeArena`
//!
//! This emitter uses the Node architecture for cache-optimized AST access.
//! It works directly with `NodeArena` instead of the old Node enum.
//!
//! # Architecture
//!
//! - Uses `NodeArena` for AST access (16-byte nodes, 13x cache improvement)
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

use crate::emit_context::EmitContext;
use crate::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use crate::transform_context::{IdentifierId, TransformContext, TransformDirective};
use crate::transforms::{ClassES5Emitter, EnumES5Emitter, NamespaceES5Emitter};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::{debug, warn};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

mod binding_patterns;
mod comment_helpers;
mod comments;
mod declarations;
mod declarations_class_members;
mod es5_bindings;
mod es5_bindings_assignment;
mod es5_bindings_patterns;
mod es5_helpers;
mod es5_helpers_async;
mod es5_templates;
mod expressions;
mod expressions_literals;
mod functions;
mod helpers;
mod jsx;
mod literals;
mod module_emission;
mod module_wrapper;
mod special_expressions;
mod statements;
mod template_literals;
mod transform_dispatch;
pub mod type_printer;
mod types;

pub use comments::{
    CommentKind, CommentRange, get_leading_comment_ranges, get_trailing_comment_ranges,
};

// Re-export common types for backward compatibility
pub use tsz_common::common::{ModuleKind, NewLineKind, ScriptTarget};

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
    /// Downlevel iteration (for-of with full iterator protocol)
    pub downlevel_iteration: bool,
    /// Set of import specifier nodes that should be elided (type-only imports)
    pub type_only_nodes: Arc<FxHashSet<NodeIndex>>,
    /// Emit "use strict" for every source file
    pub always_strict: bool,
    /// Emit class fields using Object.defineProperty semantics when downleveling
    pub use_define_for_class_fields: bool,
}

impl Default for PrinterOptions {
    fn default() -> Self {
        Self {
            remove_comments: false,
            target: ScriptTarget::ESNext,
            single_quote: false,
            omit_trailing_semicolon: false,
            no_emit_helpers: false,
            module: ModuleKind::None,
            new_line: NewLineKind::LineFeed,
            downlevel_iteration: false,
            type_only_nodes: Arc::new(FxHashSet::default()),
            always_strict: false,
            use_define_for_class_fields: false,
        }
    }
}

#[derive(Default)]
struct ParamTransformPlan {
    params: Vec<ParamTransform>,
    rest: Option<RestParamTransform>,
}

#[derive(Default)]
struct TempScopeState {
    temp_var_counter: u32,
    generated_temp_names: FxHashSet<String>,
    first_for_of_emitted: bool,
    preallocated_temp_names: VecDeque<String>,
    preallocated_assignment_temps: VecDeque<String>,
    preallocated_logical_assignment_value_temps: VecDeque<String>,
    hoisted_assignment_value_temps: Vec<String>,
    hoisted_assignment_temps: Vec<String>,
}

impl ParamTransformPlan {
    const fn has_transforms(&self) -> bool {
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

// =============================================================================
// Printer
// =============================================================================

/// Maximum recursion depth for emit to prevent infinite loops
const MAX_EMIT_RECURSION_DEPTH: u32 = 1000;

/// Printer that works with `NodeArena`.
///
/// Uses `SourceWriter` for output generation (enables source map support).
/// Uses `EmitContext` for transform-specific state management.
/// Uses `TransformContext` for directive-based transforms (Phase 2 architecture).
pub struct Printer<'a> {
    /// The `NodeArena` containing the AST.
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

    /// Pending source position for mapping the next write.
    pub(super) pending_source_pos: Option<SourcePosition>,

    /// Recursion depth counter to prevent infinite loops
    emit_recursion_depth: u32,

    /// All comments in the source file, collected once during `emit_source_file`.
    /// Used for distributing comments to blocks and other nested constructs.
    pub(super) all_comments: Vec<tsz_common::comments::CommentRange>,

    /// Shared index into `all_comments`, monotonically advancing as comments are emitted.
    /// Used across `emit_source_file` and `emit_block` to prevent double-emission.
    pub(super) comment_emit_idx: usize,

    /// All identifier texts in the source file.
    /// Collected once at `emit_source_file` start for temp name collision detection.
    /// Mirrors TypeScript's `sourceFile.identifiers` used by `makeUniqueName`.
    pub(super) file_identifiers: FxHashSet<String>,

    /// Set of generated temp names (_a, _b, etc.) to avoid collisions.
    /// Tracks ALL generated temp names across destructuring and for-of lowering.
    pub(super) generated_temp_names: FxHashSet<String>,

    /// Stack for saving/restoring temp naming state when entering function scopes.
    temp_scope_stack: Vec<TempScopeState>,

    /// Whether the first for-of loop has been emitted (uses special `_i` index name).
    pub(super) first_for_of_emitted: bool,

    /// Whether we're inside a namespace IIFE (strip export/default modifiers from classes).
    pub(super) in_namespace_iife: bool,

    /// When set, the next enum emit should fold the namespace export into the IIFE closing.
    /// E.g., `(Color = A.Color || (A.Color = {}))` instead of `(Color || (Color = {}))`.
    pub(super) enum_namespace_export: Option<String>,

    /// Set to true when the next `MODULE_DECLARATION` emit should use parent namespace
    /// assignment in its IIFE closing. This is set by `emit_namespace_body_statements`
    /// when the module is wrapped in an `EXPORT_DECLARATION`.
    pub(super) namespace_export_inner: bool,

    /// Marker that the next block emission is a function body.
    pub(super) emitting_function_body_block: bool,

    /// The name of the current namespace we're emitting inside (if any).
    /// Used for nested exported namespaces to emit proper IIFE parameters.
    pub(super) current_namespace_name: Option<String>,

    /// Override name for anonymous default exports (e.g., "`default_1`").
    /// When set, class/function emitters use this instead of leaving the name blank.
    pub(super) anonymous_default_export_name: Option<String>,

    /// Names of namespaces already declared with `var name;` to avoid duplicates.
    pub(super) declared_namespace_names: FxHashSet<String>,

    /// Exported variable/function/class names in the current namespace IIFE.
    /// Used to qualify identifier references: `foo` → `ns.foo`.
    pub(super) namespace_exported_names: FxHashSet<String>,

    /// When true, suppress namespace identifier qualification (emitting a declaration name).
    pub(super) suppress_ns_qualification: bool,

    /// Pending class field initializers to inject into constructor body.
    /// Each entry is (`field_name`, `initializer_node_index`).
    pub(super) pending_class_field_inits: Vec<(String, NodeIndex)>,

    /// Temp names for assignment target values that need to be hoisted as `var _a, _b, ...;`.
    /// These are emitted on a separate declaration list before reference temps.
    pub(super) hoisted_assignment_value_temps: Vec<String>,

    /// Temp names for assignment target values that must be reserved before references.
    /// These are used by `make_unique_name_hoisted_value`.
    pub(super) preallocated_logical_assignment_value_temps: VecDeque<String>,

    /// Temp names for assignment target values that must be reserved before references.
    /// These are used by `make_unique_name_hoisted_assignment`.
    pub(super) preallocated_assignment_temps: VecDeque<String>,

    /// Temp variable names that need to be hoisted to the top of the current scope
    /// as `var _a, _b, ...;`. Used for assignment targets in helper expressions.
    pub(super) hoisted_assignment_temps: Vec<String>,

    /// Temp names reserved ahead-of-time and consumed before generating new names.
    pub(super) preallocated_temp_names: VecDeque<String>,

    /// Temp names for ES5 iterator-based for-of lowering that must be emitted
    /// as top-level `var` declarations (e.g., `e_1, _a, e_2, _b`).
    pub(super) hoisted_for_of_temps: Vec<String>,

    /// Pre-allocated return-temp names for iterator for-of nodes.
    /// This lets nested loops reserve their return temp before outer loop
    /// iterator/result temps, matching tsc temp ordering.
    pub(super) reserved_iterator_return_temps: FxHashMap<NodeIndex, String>,

    /// Current nesting depth for iterator for-of emission.
    pub(super) iterator_for_of_depth: usize,

    /// Current nesting depth for destructuring emission that should wrap spread inputs with `__read`.
    pub(super) destructuring_read_depth: u32,
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
            pending_source_pos: None,
            emit_recursion_depth: 0,
            all_comments: Vec::new(),
            comment_emit_idx: 0,
            file_identifiers: FxHashSet::default(),
            generated_temp_names: FxHashSet::default(),
            temp_scope_stack: Vec::new(),
            first_for_of_emitted: false,
            in_namespace_iife: false,
            enum_namespace_export: None,
            namespace_export_inner: false,
            emitting_function_body_block: false,
            current_namespace_name: None,
            anonymous_default_export_name: None,
            declared_namespace_names: FxHashSet::default(),
            namespace_exported_names: FxHashSet::default(),
            suppress_ns_qualification: false,
            pending_class_field_inits: Vec::new(),
            hoisted_assignment_value_temps: Vec::new(),
            preallocated_logical_assignment_value_temps: VecDeque::new(),
            preallocated_assignment_temps: VecDeque::new(),
            hoisted_assignment_temps: Vec::new(),
            preallocated_temp_names: VecDeque::new(),
            hoisted_for_of_temps: Vec::new(),
            reserved_iterator_return_temps: FxHashMap::default(),
            iterator_for_of_depth: 0,
            destructuring_read_depth: 0,
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
        let options = PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        Self::with_options(arena, options)
    }

    /// Create a new Printer targeting ES6+.
    pub fn new_es6(arena: &'a NodeArena) -> Self {
        let options = PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        };
        Self::with_options(arena, options)
    }

    /// Set whether to target ES5 behavior.
    ///
    /// This updates both the legacy `target_es5` bool and all derived
    /// per-version lowering gates in the shared context.
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.ctx.set_target_es5(es5);
    }

    /// Set the full script target.
    ///
    /// This keeps all derived feature gates synchronized, including `target_es5`.
    pub const fn set_target(&mut self, target: ScriptTarget) {
        self.ctx.set_target(target);
    }

    /// Set the module kind (`CommonJS`, ESM, etc.).
    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.ctx.options.module = kind;
    }

    /// Set auto-detect module mode. When enabled, the emitter will detect if
    /// the source file contains import/export statements and apply `CommonJS`
    /// transforms automatically.
    pub const fn set_auto_detect_module(&mut self, enabled: bool) {
        self.ctx.auto_detect_module = enabled;
    }

    /// Set the source text (for detecting single-line constructs).
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
        let estimated = text.len().saturating_mul(3) / 2;
        self.writer.ensure_output_capacity(estimated);
    }

    /// Enable declaration emit mode for `.d.ts` output.
    ///
    /// Declaration mode changes emission behavior in multiple nodes, such as:
    /// - Skipping JS-only constructs
    /// - Emitting `declare` signatures instead of values
    /// - Keeping type-only information
    pub const fn set_declaration_emit(&mut self, enabled: bool) {
        self.ctx.flags.in_declaration_emit = enabled;
    }

    /// Set source text for source map generation without enabling comment emission.
    pub const fn set_source_map_text(&mut self, text: &'a str) {
        self.source_map_text = Some(text);
    }

    /// Enable source map generation and register the current source file.
    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.writer.enable_source_map(output_name.to_string());
        let content = self
            .source_text_for_map()
            .map(std::string::ToString::to_string);
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
    /// if there's a newline between them. Uses depth counting to handle nested braces correctly.
    fn is_single_line(&self, node: &Node) -> bool {
        if let Some(text) = self.source_text {
            let actual_start = self.skip_trivia_forward(node.pos, node.end) as usize;
            // Use actual token end, not node.end which includes trailing trivia.
            // For example, `{ return x; }\n` has trailing newline in node.end,
            // but we want to check only `{ return x; }`.
            let token_end = self.find_token_end_before_trivia(node.pos, node.end);
            let end = std::cmp::min(token_end as usize, text.len());
            if actual_start < end {
                let slice = &text[actual_start..end];
                // Find the first `{` and its matching `}` using depth counting
                // to handle nested braces (e.g., `{ return new Line({ x: 0 }, p); }`)
                if let Some(open) = slice.find('{') {
                    let mut depth = 1;
                    let mut close = None;
                    for (i, ch) in slice[open + 1..].char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    close = Some(open + 1 + i);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(close) = close {
                        let inner = &slice[open..close + 1];
                        return !inner.contains('\n');
                    }
                }
                return !slice.contains('\n');
            }
        }
        false
    }

    /// Check if two nodes are on the same line in the source.
    fn are_on_same_line_in_source(
        &self,
        node1: tsz_parser::parser::NodeIndex,
        node2: tsz_parser::parser::NodeIndex,
    ) -> bool {
        if let Some(text) = self.source_text
            && let (Some(n1), Some(n2)) = (self.arena.get(node1), self.arena.get(node2))
        {
            let start = std::cmp::min(n1.end as usize, text.len());
            let end = std::cmp::min(n2.pos as usize, text.len());
            if start < end {
                // Check if there's a newline between the two nodes
                return !text[start..end].contains('\n');
            }
        }
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
                self.emit_module_wrapper(format, &dependencies, node, source, idx);
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
        use tsz_scanner::SyntaxKind;
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
            // Log a warning about the recursion limit being exceeded.
            // This helps developers identify problematic deeply nested ASTs.
            warn!(
                depth = MAX_EMIT_RECURSION_DEPTH,
                node_kind = node.kind,
                node_pos = node.pos,
                "Emit recursion limit exceeded"
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

    const fn kind_may_have_transform(kind: u16) -> bool {
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
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Emit a node by kind using default logic (no transforms).
    /// This is the main dispatch method for emission.
    fn emit_node_by_kind(&mut self, node: &Node, idx: NodeIndex, kind: u16) {
        match kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => {
                // Check for substitution directives on identifier nodes.
                if self.transforms.has_transform(idx) {
                    if let Some(directive) = self.transforms.get(idx) {
                        match directive {
                            TransformDirective::SubstituteArguments => self.write("arguments"),
                            TransformDirective::SubstituteThis { capture_name } => {
                                let name = std::sync::Arc::clone(capture_name);
                                self.write(&name);
                            }
                            _ => self.emit_identifier(node),
                        }
                    } else {
                        self.emit_identifier(node);
                    }
                } else {
                    self.emit_identifier(node);
                }
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => {
                // Private identifiers (#name) are emitted as-is for ES2022+ targets.
                // For ES5/ES2015 targets, they should be lowered by the class transform.
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
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
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => {
                self.emit_regex_literal(node);
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
                self.emit_block(node, idx);
            }

            // Class static block: `static { ... }`
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                self.write("static ");
                // The static block uses the same data as a Block node
                self.emit_block(node, idx);
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

            // Class expression (e.g., `return class extends Base { ... }`)
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
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

            // Spread assignment in object literal: `{ ...expr }` (ES2018+ native spread)
            // For pre-ES2018 targets this is handled by emit_object_literal_with_object_assign.
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                if let Some(spread) = self.arena.get_spread(node) {
                    self.write("...");
                    self.emit_expression(spread.expression);
                }
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
            k if k == syntax_kind_ext::NAMESPACE_EXPORT => {
                // `* as name` in `export * as name from "..."`
                if let Some(data) = self.arena.get_named_imports(node) {
                    self.write("* as ");
                    self.emit(data.name);
                }
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
                self.emit_debugger_statement(node);
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                self.emit_with_statement(node);
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
                } else {
                    // Skip comments belonging to erased declarations so they don't
                    // get emitted later by gap/before-pos comment handling.
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                // Type alias declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_type_alias_declaration(node);
                } else {
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(node, idx);
            }

            // Computed property name: [expr]
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.write("[");
                    self.emit(computed.expression);
                    // Map closing `]` to its source position.
                    // The expression's end points past the expression, so `]`
                    // is at the expression's end position (where the expression
                    // text ends and `]` begins).
                    if let Some(text) = self.source_text_for_map() {
                        let expr_end = self
                            .arena
                            .get(computed.expression)
                            .map_or(node.pos + 1, |e| e.end);
                        self.pending_source_pos = Some(source_position_from_offset(text, expr_end));
                    }
                    self.write("]");
                }
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
            k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => {
                self.write(";");
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
                // Call signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_call_signature(node);
                }
            }
            k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                // Construct signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_construct_signature(node);
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                // Index signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_index_signature(node);
                }
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
                self.emit_source_file(node, idx);
            }

            // Other tokens and keywords - emit their text
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // Check for SubstituteThis directive from lowering pass (Phase C)
                // Directive approach is now the only path (fallback removed)
                if let Some(TransformDirective::SubstituteThis { capture_name }) =
                    self.transforms.get(idx)
                {
                    let name = std::sync::Arc::clone(capture_name);
                    self.write(&name);
                } else {
                    self.write("this");
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

            // ExpressionWithTypeArguments / instantiation expression:
            // Strip type arguments and emit just the expression, wrapped in
            // parentheses to preserve semantics (e.g. `obj.fn<T>` → `(obj.fn)`).
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.arena.get_expr_type_args(node) {
                    self.write("(");
                    self.emit(data.expression);
                    self.write(")");
                }
            }

            // Default: do nothing (or handle other cases as needed)
            _ => {}
        }
    }

    // =========================================================================
    // Source File
    // =========================================================================

    fn emit_source_file(&mut self, node: &Node, source_idx: NodeIndex) {
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

        // Collect all identifiers in the file for temp name collision detection.
        // This mirrors TypeScript's `sourceFile.identifiers` used by `makeUniqueName`.
        self.file_identifiers.clear();
        for ident in &self.arena.identifiers {
            self.file_identifiers.insert(ident.escaped_text.clone());
        }
        self.generated_temp_names.clear();
        self.first_for_of_emitted = false;

        // Enter root scope for block-scoped variable tracking and `var` scope boundaries.
        // This ensures variables declared throughout the file are tracked for renaming.
        self.ctx.block_scope_state.enter_function_scope();

        // Extract comments. Triple-slash references (/// <reference ...>) are
        // preserved in output (TypeScript keeps them in JS emit).
        // Only AMD-specific directives (/// <amd ...) are stripped.
        // Store on self so nested blocks can also distribute comments.
        self.all_comments = if !self.ctx.options.remove_comments {
            if let Some(text) = self.source_text {
                tsz_common::comments::get_comment_ranges(text)
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

        // Filter out comments associated with erased declarations
        // (interfaces, type aliases). TSC strips both the declaration body
        // and its leading trivia (comments directly before it). However,
        // file-level comments before any declarations are preserved.
        // We use prev_end to track the previous statement's end position;
        // for the first statement, we use node.pos to preserve file-level comments.
        // Track position of first erased statement for header comment filtering.
        let mut first_erased_stmt_pos: Option<u32> = None;
        if !self.ctx.flags.in_declaration_emit && !self.all_comments.is_empty() {
            let mut erased_ranges: Vec<(u32, u32)> = Vec::new();
            let mut prev_end: Option<u32> = None;
            for &stmt_idx in &source.statements.nodes {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let stmt_token_end =
                        self.find_token_end_before_trivia(stmt_node.pos, stmt_node.end);
                    // Check if statement is erased in JS emit (type-only, ambient, etc.)
                    let mut is_erased = self.is_erased_statement(stmt_node);
                    // Also check if it's an export declaration wrapping an erased declaration
                    if !is_erased
                        && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                        && let Some(export) = self.arena.get_export_decl(stmt_node)
                        && let Some(inner_node) = self.arena.get(export.export_clause)
                        && self.is_erased_statement(inner_node)
                    {
                        is_erased = true;
                    }

                    if is_erased {
                        let range_start = if let Some(pe) = prev_end {
                            pe
                        } else {
                            // For the first erased statement, preserve file-level
                            // comments by starting the erased range at the statement
                            // itself. The header comment loop will filter out
                            // attached comments separately.
                            first_erased_stmt_pos = Some(stmt_node.pos);
                            stmt_node.pos
                        };
                        erased_ranges.push((range_start, stmt_token_end));
                    }
                    prev_end = Some(stmt_token_end);
                }
            }
            if !erased_ranges.is_empty() {
                self.all_comments.retain(|c| {
                    !erased_ranges
                        .iter()
                        .any(|&(start, end)| c.pos >= start && c.end <= end)
                });
            }
        }

        self.comment_emit_idx = 0;

        // Emit shebang line if present (must be the very first line of output)
        if let Some(text) = self.source_text
            && text.starts_with("#!")
        {
            if let Some(newline_pos) = text.find('\n') {
                self.write(text[..newline_pos].trim_end());
            } else {
                self.write(text.trim_end());
            }
            self.write_line();
        }

        // Emit "use strict" FIRST (before comments and helpers)
        // TypeScript emits "use strict" when:
        // 1. Module is CommonJS/AMD/UMD (always)
        // 2. alwaysStrict compiler option is enabled (for non-ES-module output)
        // But NOT when:
        // - The source already has "use strict" (avoid duplication)
        // - ES module output (ES2015/ESNext module kind) since ESM is implicitly strict
        let is_commonjs_or_amd = matches!(
            self.ctx.options.module,
            ModuleKind::CommonJS | ModuleKind::AMD | ModuleKind::UMD
        );
        let is_es_module_output = matches!(
            self.ctx.options.module,
            ModuleKind::ES2015
                | ModuleKind::ES2020
                | ModuleKind::ES2022
                | ModuleKind::ESNext
                | ModuleKind::Preserve
                | ModuleKind::Node16
                | ModuleKind::NodeNext
        );

        // Check if source already has "use strict" as a prologue directive.
        // Prologue directives are string literal expression statements that appear
        // BEFORE any non-string-literal statements. Once a non-string-literal
        // statement is seen, the prologue zone ends.
        // We must check the AST rather than raw text because there may be comments
        // before the prologue that would fool a text-based check.
        let source_has_use_strict = {
            let mut found = false;
            for &idx in &source.statements.nodes {
                let Some(stmt_node) = self.arena.get(idx) else {
                    break;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    break; // non-expression-statement ends the prologue zone
                }
                let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                    break;
                };
                let Some(expr_node) = self.arena.get(expr_stmt.expression) else {
                    break;
                };
                if expr_node.kind != tsz_scanner::SyntaxKind::StringLiteral as u16 {
                    break; // non-string-literal ends the prologue zone
                }
                // Check the literal text
                let is_use_strict = if let Some(lit) = self.arena.get_literal(expr_node) {
                    lit.text == "use strict"
                } else if let Some(text) = self.source_text {
                    let s = crate::printer::safe_slice::slice(
                        text,
                        expr_node.pos as usize,
                        expr_node.end as usize,
                    );
                    s == "\"use strict\"" || s == "'use strict'"
                } else {
                    false
                };
                if is_use_strict {
                    found = true;
                    break;
                }
                // Other string literal prologue — continue scanning
            }
            found
        };

        // TypeScript emits "use strict" when:
        // 1. CommonJS/AMD/UMD AND the file is actually an ES module (has import/export).
        //    Script files (no import/export) don't get "use strict".
        // 2. alwaysStrict is on AND the file is not already an ES module output.
        let is_file_module = self.file_is_module(&source.statements);
        let should_emit_use_strict = !source_has_use_strict
            && ((is_commonjs_or_amd && is_file_module)
                || (self.ctx.options.always_strict && !(is_es_module_output && is_file_module)));

        if should_emit_use_strict {
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
            .map_or(node.end, |n| self.skip_trivia_forward(n.pos, n.end));

        let mut deferred_header_comments: Vec<(String, bool)> = Vec::new();
        let is_commonjs = self.ctx.is_commonjs();
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= first_stmt_pos {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;

                    // Skip comments that are directly attached to an erased first
                    // statement (no blank line between comment and declaration).
                    // Detached comments (separated by blank line) are preserved.
                    if let Some(erased_pos) = first_erased_stmt_pos {
                        let between = &text[c_end as usize..erased_pos as usize];
                        let has_blank_line =
                            between.contains("\n\n") || between.contains("\r\n\r\n");
                        if !has_blank_line {
                            self.comment_emit_idx += 1;
                            continue;
                        }
                    }

                    let comment_text =
                        crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    // In CommonJS mode, "detached" comments (followed by a blank
                    // line before the next content) are file-level and go BEFORE
                    // the __esModule marker. "Attached" comments (no blank line
                    // after them) are deferred to AFTER the preamble.
                    let next_content_pos = self
                        .all_comments
                        .get(self.comment_emit_idx + 1)
                        .map_or(first_stmt_pos, |next_c| next_c.pos);
                    let between_after = &text[c_end as usize..next_content_pos as usize];
                    let is_detached =
                        between_after.contains("\n\n") || between_after.contains("\r\n\r\n");
                    if is_commonjs && !is_detached {
                        deferred_header_comments.push((comment_text.to_string(), c_trailing));
                    } else {
                        self.write_comment(comment_text);
                        if c_trailing {
                            self.write_line();
                        }
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

        // Emit all needed helpers (unless no_emit_helpers is set)
        if !self.ctx.options.no_emit_helpers {
            let helpers_code = crate::transforms::helpers::emit_helpers(&helpers);
            if !helpers_code.is_empty() {
                self.write(&helpers_code);
                // emit_helpers() already adds newlines, no need to add more
            }
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

        if !deferred_header_comments.is_empty() {
            for (comment, has_trailing_new_line) in &deferred_header_comments {
                self.write_comment(comment);
                if *has_trailing_new_line {
                    self.write_line();
                }
            }
        }

        // Emit `var _this = this;` for top-level arrow functions that capture `this`
        if let Some(capture_name) = self
            .transforms
            .this_capture_name(source_idx)
            .map(std::string::ToString::to_string)
        {
            self.write("var ");
            self.write(&capture_name);
            self.write(" = this;");
            self.write_line();
        } else {
            tracing::debug!("[emit] no top-level this capture for source {source_idx:?}");
        }

        // Save position for hoisted temp var declarations (assignment destructuring).
        // After emitting all statements, we'll insert `var _a, _b, ...;` here if needed.
        self.hoisted_assignment_temps.clear();
        self.hoisted_assignment_value_temps.clear();
        self.preallocated_logical_assignment_value_temps.clear();
        self.preallocated_assignment_temps.clear();
        self.hoisted_for_of_temps.clear();
        self.preallocated_temp_names.clear();
        self.reserved_iterator_return_temps.clear();
        self.iterator_for_of_depth = 0;

        self.prepare_logical_assignment_value_temps(source_idx);

        let hoisted_var_byte_offset = self.writer.len();
        let hoisted_var_line = self.writer.current_line();

        // Emit statements with their leading comments.
        // In this parser, node.pos includes leading trivia (whitespace + comments).
        // Between-statement comments are part of the next node's leading trivia.
        // We find each statement's "actual token start" by scanning forward past
        // trivia, then emit all comments before that position.
        let mut last_erased_stmt_end: Option<u32> = None;
        let mut last_erased_was_shorthand_module = false;
        let mut deferred_commonjs_export_equals: Vec<NodeIndex> = Vec::new();
        let mut has_runtime_module_syntax = false;
        let mut has_deferred_empty_export = false;
        for &stmt_idx in &source.statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                if self.ctx.is_commonjs()
                    && stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                    && self
                        .arena
                        .get_export_assignment(stmt_node)
                        .is_some_and(|ea| ea.is_export_equals)
                {
                    deferred_commonjs_export_equals.push(stmt_idx);
                    last_erased_stmt_end = None;
                    last_erased_was_shorthand_module = false;
                    continue;
                }

                // Defer `export {}` (empty named exports, no module specifier) to end
                // of file. TSC places these at the end as ESM markers.
                if is_es_module_output
                    && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export) = self.arena.get_export_decl(stmt_node)
                    && export.module_specifier.is_none()
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                    && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                    && let Some(named_exports) = self.arena.get_named_imports(clause_node)
                    && named_exports.elements.nodes.is_empty()
                {
                    has_deferred_empty_export = true;
                    has_runtime_module_syntax = true;
                    self.skip_comments_for_erased_node(stmt_node);
                    last_erased_stmt_end = None;
                    last_erased_was_shorthand_module = false;
                    continue;
                }

                // For erased declarations (type-only, ambient, etc.) in JS emit mode,
                // skip their leading comments entirely - they should not appear in output.
                let is_erased =
                    !self.ctx.flags.in_declaration_emit && self.is_erased_statement(stmt_node);

                // Skip empty statements (`;`) that follow an erased shorthand module
                // declaration (`declare module "foo";`). The shorthand module syntax
                // parses as MODULE_DECLARATION + EMPTY_STATEMENT, and the trailing
                // `;` should be erased along with the declaration.
                if !is_erased
                    && stmt_node.kind == syntax_kind_ext::EMPTY_STATEMENT
                    && last_erased_was_shorthand_module
                {
                    last_erased_was_shorthand_module = false;
                    continue;
                }

                // Track whether any non-erased module-indicating statement exists
                // (needed for `export {};` insertion at end of file)
                if !is_erased && !has_runtime_module_syntax {
                    let k = stmt_node.kind;
                    if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    {
                        has_runtime_module_syntax = true;
                    }
                }

                // Find the actual start of the statement's first token by
                // scanning forward from node.pos past whitespace only.
                // We preserve comments here - they're handled either as leading
                // comments (if truly before the statement) or by nested expression emitters.
                let actual_start = self.skip_whitespace_forward(stmt_node.pos, stmt_node.end);

                if is_erased {
                    // Skip erased declarations. Their leading comments were already
                    // filtered out of all_comments during initialization.
                    // Also consume trailing same-line comments for the erased statement
                    // (e.g., `declare var a: boolean; // comment` should be erased too).
                    // We use the end-of-line of the last token as the boundary:
                    //   - Comments on the same line as the last token → consume (erase)
                    //   - Comments on subsequent lines → keep for the next statement
                    let stmt_token_end =
                        self.find_token_end_before_trivia(stmt_node.pos, stmt_node.end);
                    let line_end = if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut pos = stmt_token_end as usize;
                        while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                            pos += 1;
                        }
                        pos as u32
                    } else {
                        stmt_token_end
                    };
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        if c_end <= line_end {
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                    last_erased_stmt_end = Some(line_end);
                    // Track if this is a shorthand module declaration (no body),
                    // so we can skip the trailing EMPTY_STATEMENT (`;`).
                    last_erased_was_shorthand_module = stmt_node.kind
                        == syntax_kind_ext::MODULE_DECLARATION
                        && self
                            .arena
                            .get_module(stmt_node)
                            .is_some_and(|m| m.body.is_none());
                    continue;
                }

                // Emit comments whose end position is at or before the actual token start.
                // These are truly "leading" comments for this statement.
                // Comments inside expressions (like call arguments) have positions AFTER
                // the statement's first token, so they won't be emitted here.
                let defer_for_of_comments = stmt_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                    && self.should_defer_for_of_comments(stmt_node);
                if !defer_for_of_comments && let Some(text) = self.source_text {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_pos = self.all_comments[self.comment_emit_idx].pos;
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        let c_trailing =
                            self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                        if let Some(erased_end) = last_erased_stmt_end
                            && c_end <= erased_end
                        {
                            // Comment was part of a recently erased declaration; discard it.
                            self.comment_emit_idx += 1;
                            continue;
                        }
                        // Only emit if this comment ends before the statement's first token
                        // AND hasn't been emitted by a nested expression emitter
                        if c_end <= actual_start {
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
                last_erased_stmt_end = None;
                last_erased_was_shorthand_module = false;
            }

            let before_len = self.writer.len();
            self.emit(stmt_idx);
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                // Emit trailing comments on the same line as the statement.
                // Use the next statement's pos as upper bound to avoid scanning
                // into the next statement's trivia (same pattern as emit_block_body).
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let stmts = &source.statements.nodes;
                    let stmt_i = stmts.iter().position(|&s| s == stmt_idx);
                    let next_pos = stmt_i.and_then(|i| {
                        stmts
                            .get(i + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map(|n| n.pos)
                    });
                    let upper_bound = next_pos.unwrap_or(stmt_node.end);
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments(token_end);
                }
                self.write_line();
            }

            // Note: We do NOT skip inner comments here. The "emit comments before
            // statement" logic (above) uses actual_start which is computed by
            // skip_trivia_forward. Inner comments (inside function/class bodies)
            // have positions that are BEFORE the next top-level statement's actual
            // start, so they won't be emitted at the wrong level. They'll be
            // naturally consumed when we encounter the statement that contains them.
        }

        // TypeScript emits CommonJS `export =` assignments after declaration output,
        // even when they appear earlier in source.
        for stmt_idx in deferred_commonjs_export_equals {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                self.write_line();
            }
        }

        // Emit deferred `export {}` at end of file (moved from its source position).
        if has_deferred_empty_export {
            self.write("export {};");
            self.write_line();
        }

        // When a file is an ES module but all import/export statements were erased
        // (all type-only), emit `export {};` to preserve module semantics.
        // This matches tsc behavior: the file must remain an ES module even if
        // all its import/export syntax was type-only and got stripped.
        if is_file_module && is_es_module_output && !has_runtime_module_syntax {
            self.write("export {};");
            self.write_line();
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

        // Insert hoisted temp declarations (for-of iterator lowering + assignment destructuring).
        let mut ref_vars = Vec::new();
        ref_vars.extend(self.hoisted_assignment_temps.iter().cloned());
        ref_vars.extend(self.hoisted_for_of_temps.iter().cloned());

        if !ref_vars.is_empty() {
            let var_decl = format!("var {};", ref_vars.join(", "));
            self.writer
                .insert_line_at(hoisted_var_byte_offset, hoisted_var_line, &var_decl);
        }

        if !self.hoisted_assignment_value_temps.is_empty() {
            let var_decl = format!("var {};", self.hoisted_assignment_value_temps.join(", "));
            self.writer
                .insert_line_at(hoisted_var_byte_offset, hoisted_var_line, &var_decl);
        }

        // Ensure output ends with a newline (matching tsc behavior)
        if !self.writer.is_at_line_start() {
            self.write_line();
        }

        // Exit root scope for block-scoped variable tracking
        self.ctx.block_scope_state.exit_scope();
    }
}

impl<'a> Printer<'a> {
    fn should_defer_for_of_comments(&self, node: &Node) -> bool {
        let for_of = match self.arena.get_for_in_of(node) {
            Some(for_of) => for_of,
            None => return false,
        };

        if for_of.await_modifier {
            return !self.ctx.options.target.supports_es2018();
        }

        self.ctx.target_es5 && self.ctx.options.downlevel_iteration
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

const fn get_operator_text(op: u16) -> &'static str {
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
