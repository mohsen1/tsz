//! IR Printer - Emits JavaScript strings from IR nodes
//!
//! This module handles all string emission from the IR. Transforms produce IR nodes,
//! and this printer converts them to JavaScript strings.
//!
//! # Example
//!
//! ```ignore
//! use crate::transforms::ir::IRNode;
//! use crate::transforms::ir_printer::IRPrinter;
//!
//! let ir = IRNode::func_decl("foo", vec![], vec![
//!     IRNode::ret(Some(IRNode::number("42")))
//! ]);
//!
//! let mut printer = IRPrinter::new();
//! let output = printer.emit(&ir);
//! // output: "function foo() {\n    return 42;\n}"
//! ```

use std::borrow::Cow;
use std::fmt::Write;

#[path = "ir_printer_class_emit.rs"]
mod ir_printer_class_emit;
#[path = "ir_printer_generator_state.rs"]
mod ir_printer_generator_state;
#[path = "ir_printer_helpers.rs"]
mod ir_printer_helpers;
#[path = "ir_printer_namespace.rs"]
mod ir_printer_namespace;
use ir_printer_namespace::NamespaceIifeContext;

use crate::context::transform::TransformContext;
use crate::emitter::{Printer as AstPrinter, PrinterOptions};
use crate::transforms::ir::{
    EnumMember, EnumMemberValue, IRMethodName, IRNode, IRParam, IRProperty, IRPropertyKey,
    IRPropertyKind, IRSwitchCase,
};
use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::syntax_kind_ext;

/// IR Printer - converts IR nodes to JavaScript strings
pub struct IRPrinter<'a> {
    output: String,
    indent_level: u32,
    indent_str: &'static str,
    /// Optional arena for handling `ASTRef` nodes
    arena: Option<&'a NodeArena>,
    /// Source text for emitting `ASTRef` nodes
    source_text: Option<&'a str>,
    /// Optional transform directives for `ASTRef` nodes
    transforms: Option<TransformContext>,
    /// Avoid duplicate trailing comments when a sequence explicitly carries one.
    suppress_function_trailing_extraction: bool,
    /// Tracks when the last emitted IR node wrote a trailing line comment.
    last_emit_ended_with_line_comment: bool,
    /// Source range end for nested AST arrow comments that should be left for
    /// an IR-owned semicolon/trailing-comment site.
    ast_arrow_comment_defer_end: Option<u32>,
    /// Name of the current ES5 class IIFE constructor, used to force constructor
    /// empty-body formatting without affecting nested function declarations.
    current_class_iife_name: Option<String>,
    /// When true, the next `FunctionExpr` emit will force multiline for empty bodies.
    /// Set by `CallExpr` when emitting an IIFE callee.
    force_iife_multiline_empty: bool,
    /// When true, we are inside a namespace IIFE body.
    /// Nested namespace variable declarations use `let` instead of `var` in ES2015+ targets.
    in_namespace_iife_body: bool,
    /// When true, the target is ES5 and `let`/`const` should not be emitted.
    target_es5: bool,
    /// When true, comments like `/** @class */` are suppressed in output.
    remove_comments: bool,
    /// CommonJS `tslib` binding used to prefix runtime helper calls for importHelpers.
    tslib_prefix: bool,
    tslib_import_binding: String,
    commonjs_import_substitutions: rustc_hash::FxHashMap<String, String>,
    system_import_meta: bool,
    pub(crate) base_printer_options: Option<PrinterOptions>,
    generator_state_name: &'static str,
    /// Outer names (e.g. a class-expression alias) excluded from generator state
    /// variable selection.  Treated as already-allocated hoisted vars so the
    /// state-name picker skips past them.
    outer_reserved_for_generator_state: Vec<String>,
    namespace_ast_name: Option<String>,
    namespace_ast_exported_names: rustc_hash::FxHashSet<String>,
    block_scope_shadowed_names: Vec<String>,
    block_scope_reserved_names: Vec<String>,
    /// Deferred CommonJS `exports.<name> = <name>;` assignment for a top-level
    /// `export class` lowered to an ES5 IIFE. When set, the assignment is
    /// emitted immediately after the class IIFE statement and BEFORE any
    /// trailing computed-property-name side-effect statements, mirroring the
    /// ES2015+ class export ordering in `emit_es6.rs`.
    pending_commonjs_class_export_name: Option<String>,
}

impl<'a> IRPrinter<'a> {
    /// Check if a generator switch case should stay on the `case N:` line.
    fn is_generator_inline_case_statement(node: &IRNode) -> bool {
        match node {
            IRNode::ThrowStatement(expr) => Self::is_generator_inline_throw_expression(expr),
            IRNode::ReturnStatement(Some(expr)) => {
                matches!(expr.as_ref(), IRNode::GeneratorOp { .. })
            }
            _ => false,
        }
    }

    fn is_generator_break_return(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ReturnStatement(Some(expr))
                if matches!(expr.as_ref(), IRNode::GeneratorOp { opcode: 3, .. })
        )
    }

    fn is_generator_sent_assignment(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ExpressionStatement(expr)
                if matches!(
                    expr.as_ref(),
                    IRNode::BinaryExpr { right, .. } if matches!(right.as_ref(), IRNode::GeneratorSent)
                )
        )
    }

    const fn is_generator_inline_throw_expression(expr: &IRNode) -> bool {
        matches!(
            expr,
            IRNode::Identifier(_) | IRNode::CallExpr { .. } | IRNode::GeneratorSent
        )
    }

    pub(crate) fn emit_es5_class_expression(
        &mut self,
        name: &str,
        base_class: Option<&IRNode>,
        super_param: Option<&str>,
        body: &[IRNode],
    ) {
        if !self.remove_comments {
            self.write("/** @class */ ");
        }
        self.write("(function (");
        if base_class.is_some() {
            self.write(super_param.unwrap_or("_super"));
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        let prev_iife_name = self.current_class_iife_name.replace(name.to_string());
        for stmt in body {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }
        self.current_class_iife_name = prev_iife_name;

        self.decrease_indent();
        self.write_indent();
        self.write("}(");
        if let Some(base) = base_class {
            self.emit_node(base);
        }
        self.write("))");
    }

    fn emit_static_block_iife_expression(&mut self, statements: &[IRNode]) {
        self.write("(function () {");
        if statements.is_empty() {
            self.write(" })()");
            return;
        }

        self.write_line();
        self.increase_indent();
        for stmt in statements {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }
        self.decrease_indent();
        self.write_indent();
        self.write("})()");
    }

    fn extract_trailing_comment_from_function(&self, function: &IRNode) -> Option<String> {
        let source_text = self.source_text?;
        let (body_start, body_end) = match function {
            IRNode::FunctionExpr {
                body_source_range: Some((body_start, body_end)),
                ..
            }
            | IRNode::FunctionDecl {
                body_source_range: Some((body_start, body_end)),
                ..
            } => (*body_start, *body_end),
            _ => return None,
        };
        let bytes = source_text.as_bytes();
        let start = body_start as usize;
        let end = (body_end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        let open_brace = bytes[start..end].iter().position(|&byte| byte == b'{')?;
        let mut depth = 1usize;
        let mut close_brace = None;
        for offset in open_brace + 1..end - start {
            match bytes[start + offset] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close_brace = Some(start + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_brace = close_brace?;
        let comments = crate::emitter::get_trailing_comment_ranges(source_text, close_brace + 1);
        if comments.is_empty() {
            return None;
        }

        Some(
            comments
                .iter()
                .map(|comment| source_text[comment.pos as usize..comment.end as usize].to_string())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    fn should_indent_sequence_child(node: &IRNode) -> bool {
        match node {
            IRNode::NamespaceIIFE {
                skip_sequence_indent,
                ..
            } => !skip_sequence_indent,
            _ => true,
        }
    }

    fn generator_state_name_for_function_body(body: &[IRNode]) -> Option<&'static str> {
        if !body
            .iter()
            .any(|node| matches!(node, IRNode::GeneratorBody { .. }))
        {
            return None;
        }

        let mut hoisted_vars = Vec::new();
        for stmt in body {
            match stmt {
                IRNode::VarDeclList(decls) => {
                    for decl in decls {
                        if let IRNode::VarDecl { name, .. } = decl {
                            hoisted_vars.push(name.as_ref());
                        }
                    }
                }
                IRNode::VarDecl { name, .. } => hoisted_vars.push(name.as_ref()),
                IRNode::GeneratorBody { .. } => break,
                _ => {}
            }
        }

        (!hoisted_vars.is_empty()).then(|| Self::generator_state_name_for_hoisted(&hoisted_vars))
    }

    fn is_noop_statement(node: &IRNode) -> bool {
        match node {
            IRNode::Sequence(nodes) if nodes.is_empty() => true,
            IRNode::EmptyStatement => true,
            IRNode::Raw(text) => text.trim().is_empty(),
            _ => false,
        }
    }

    fn write_embedded_output(&mut self, output: &str) {
        let mut lines = output.split('\n');
        if let Some(first) = lines.next() {
            self.write(first);
        }
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                self.write_indent();
                self.write(line);
            }
        }
    }

    /// Create a new IR printer
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: None,
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            last_emit_ended_with_line_comment: false,
            ast_arrow_comment_defer_end: None,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: rustc_hash::FxHashMap::default(),
            system_import_meta: false,
            base_printer_options: None,
            generator_state_name: "_a",
            outer_reserved_for_generator_state: Vec::new(),
            namespace_ast_name: None,
            namespace_ast_exported_names: rustc_hash::FxHashSet::default(),
            block_scope_shadowed_names: Vec::new(),
            block_scope_reserved_names: Vec::new(),
            pending_commonjs_class_export_name: None,
        }
    }

    /// Create an IR printer with an arena for `ASTRef` handling
    pub fn with_arena(arena: &'a NodeArena) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            last_emit_ended_with_line_comment: false,
            ast_arrow_comment_defer_end: None,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: rustc_hash::FxHashMap::default(),
            system_import_meta: false,
            base_printer_options: None,
            generator_state_name: "_a",
            outer_reserved_for_generator_state: Vec::new(),
            namespace_ast_name: None,
            namespace_ast_exported_names: rustc_hash::FxHashSet::default(),
            block_scope_shadowed_names: Vec::new(),
            block_scope_reserved_names: Vec::new(),
            pending_commonjs_class_export_name: None,
        }
    }

    /// Create an IR printer with both arena and source text for `ASTRef` emission
    pub fn with_arena_and_source(arena: &'a NodeArena, source_text: &'a str) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: Some(source_text),
            transforms: None,
            suppress_function_trailing_extraction: false,
            last_emit_ended_with_line_comment: false,
            ast_arrow_comment_defer_end: None,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: rustc_hash::FxHashMap::default(),
            system_import_meta: false,
            base_printer_options: None,
            generator_state_name: "_a",
            outer_reserved_for_generator_state: Vec::new(),
            namespace_ast_name: None,
            namespace_ast_exported_names: rustc_hash::FxHashSet::default(),
            block_scope_shadowed_names: Vec::new(),
            block_scope_reserved_names: Vec::new(),
            pending_commonjs_class_export_name: None,
        }
    }

    /// Schedule a deferred CommonJS `exports.<name> = <name>;` assignment for a
    /// top-level `export class` lowered to an ES5 IIFE. Emitted right after the
    /// class IIFE statement, before any trailing computed-property side effects.
    pub fn set_pending_commonjs_class_export_name(&mut self, name: Option<String>) {
        self.pending_commonjs_class_export_name = name;
    }

    /// Consume the scheduled CommonJS class export name, clearing it so a single
    /// IIFE statement emits the assignment exactly once.
    pub(super) const fn take_pending_commonjs_class_export_name(&mut self) -> Option<String> {
        self.pending_commonjs_class_export_name.take()
    }

    /// Set transform directives for `ASTRef` emission
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Enable `tslib_1.` prefix for runtime helper calls (importHelpers + CJS).
    pub const fn set_tslib_prefix(&mut self, enable: bool) {
        self.tslib_prefix = enable;
    }

    pub fn set_tslib_import_binding(&mut self, binding: String) {
        self.tslib_import_binding = binding;
    }

    pub fn set_commonjs_import_substitutions(
        &mut self,
        subs: rustc_hash::FxHashMap<String, String>,
    ) {
        self.commonjs_import_substitutions = subs;
    }

    pub const fn set_system_import_meta(&mut self, enabled: bool) {
        self.system_import_meta = enabled;
    }

    pub fn set_namespace_ast_qualification(
        &mut self,
        namespace: String,
        names: std::collections::HashSet<String>,
    ) {
        self.namespace_ast_name = Some(namespace);
        self.namespace_ast_exported_names = names.into_iter().collect();
    }

    pub fn set_block_scope_shadowed_names(&mut self, names: Vec<String>) {
        self.block_scope_shadowed_names = names;
    }

    pub fn set_block_scope_reserved_names(&mut self, names: Vec<String>) {
        self.block_scope_reserved_names = names;
    }

    pub fn block_scope_reserved_names(&self) -> Vec<String> {
        let mut names = self.block_scope_reserved_names.clone();
        names.sort();
        names.dedup();
        names
    }

    fn merge_ast_printer_block_scope_reserved_names(&mut self, printer: &AstPrinter<'a>) {
        self.block_scope_reserved_names
            .extend(printer.block_scope_reserved_names());
        self.block_scope_reserved_names.sort();
        self.block_scope_reserved_names.dedup();
    }

    fn configure_ast_printer_namespace(&self, printer: &mut AstPrinter<'a>) {
        if let Some(namespace) = self.namespace_ast_name.clone() {
            printer.in_namespace_iife = true;
            printer.current_namespace_name = Some(namespace);
            printer.namespace_exported_names = self.namespace_ast_exported_names.clone();
        }
    }

    /// Build a nested `AstPrinter` that inherits this IR printer's transforms,
    /// printer options, and source text. Callers that need namespace
    /// qualification on the embedded output must invoke
    /// `configure_ast_printer_namespace` themselves; keeping it opt-in avoids
    /// silently changing emission for arms (e.g. `ASTRefWithGeneratorThis`)
    /// that historically ran without namespace context.
    fn build_nested_ast_printer(&self, arena: &'a NodeArena) -> AstPrinter<'a> {
        let transforms = self.transforms.clone().unwrap_or_default();
        let mut printer = AstPrinter::with_transforms_and_options(
            arena,
            transforms,
            self.make_ast_printer_options(),
        );
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        printer.seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
        printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
        printer
    }

    /// Write a runtime helper name, prefixing with `tslib_1.` when `tslib_prefix` is active.
    fn write_helper(&mut self, name: &str) {
        if self.tslib_prefix {
            self.output.push_str(&self.tslib_import_binding);
            self.output.push('.');
        }
        self.output.push_str(name);
    }

    /// Set the source text for `ASTRef` emission
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Set the indentation level
    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Mark this printer as targeting ES5 (disables `let`/`const` emission).
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.target_es5 = es5;
    }

    pub const fn set_generator_state_name(&mut self, name: &'static str) {
        self.generator_state_name = name;
    }

    /// Set names that must not be chosen as the `__generator` state variable.
    pub fn set_outer_reserved_for_generator_state(&mut self, names: Vec<String>) {
        self.outer_reserved_for_generator_state = names;
    }

    /// When true, suppress comment annotations like `/** @class */` in output.
    pub const fn set_remove_comments(&mut self, remove: bool) {
        self.remove_comments = remove;
    }

    pub fn set_base_printer_options(&mut self, options: PrinterOptions) {
        self.base_printer_options = Some(options);
    }

    fn make_ast_printer_options(&self) -> PrinterOptions {
        if let Some(ref base) = self.base_printer_options {
            let mut opts = base.clone();
            if self.target_es5 {
                opts.target = crate::emitter::ScriptTarget::ES5;
            }
            opts
        } else {
            PrinterOptions {
                target: if self.target_es5 {
                    crate::emitter::ScriptTarget::ES5
                } else {
                    PrinterOptions::default().target
                },
                ..PrinterOptions::default()
            }
        }
    }

    /// Get the output
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Take the output
    pub fn take_output(self) -> String {
        self.output
    }

    /// Emit an IR node to a string
    pub fn emit(&mut self, node: &IRNode) -> &str {
        // For top-level Sequences, add newlines between statements
        if let IRNode::Sequence(nodes) = node {
            let mut i = 0;
            while i < nodes.len() {
                if i > 0 {
                    self.write_line();
                    if Self::should_indent_sequence_child(&nodes[i]) {
                        self.write_indent();
                    }
                }
                if i + 1 < nodes.len()
                    && let Some((enum_name, members, namespace)) =
                        Self::enum_with_matching_namespace_export(&nodes[i], &nodes[i + 1])
                {
                    self.emit_namespace_bound_enum_iife(enum_name, members, namespace);
                    i += 2;
                    continue;
                }
                let suppress_for_this_node = i + 1 < nodes.len()
                    && matches!(&nodes[i], IRNode::FunctionDecl { .. })
                    && matches!(&nodes[i + 1], IRNode::TrailingComment(_));
                let prev_suppress = self.suppress_function_trailing_extraction;
                self.suppress_function_trailing_extraction = suppress_for_this_node;
                self.emit_node(&nodes[i]);
                self.suppress_function_trailing_extraction = prev_suppress;
                i += 1;
            }
        } else {
            self.emit_node(node);
        }
        &self.output
    }

    /// Emit an IR node and return the output
    pub fn emit_to_string(node: &IRNode) -> String {
        let mut printer = Self::new();
        printer.emit(node);
        printer.output
    }

    /// Check whether a property access on `node` needs `..` instead of `.`.
    /// Plain decimal integer literals need `..` because `0.x` would be
    /// parsed as the float `0.` followed by identifier `x`.
    fn ir_node_needs_double_dot(node: &IRNode) -> bool {
        match node {
            IRNode::NumericLiteral(n) => {
                let num_text = n.trim();
                let is_prefixed = num_text.starts_with("0x")
                    || num_text.starts_with("0X")
                    || num_text.starts_with("0o")
                    || num_text.starts_with("0O")
                    || num_text.starts_with("0b")
                    || num_text.starts_with("0B");
                !is_prefixed
                    && !num_text.contains('.')
                    && !num_text.contains('e')
                    && !num_text.contains('E')
            }
            // Other expressions (including parenthesized) never need double-dot
            // because the closing paren already disambiguates: `(1).foo` is valid JS.
            _ => false,
        }
    }

    fn emit_sent_aware(&mut self, node: &IRNode) {
        if matches!(node, IRNode::GeneratorSent) {
            self.write("(");
            self.emit_node(node);
            self.write(")");
        } else {
            self.emit_node(node);
        }
    }

    fn emit_node(&mut self, node: &IRNode) {
        match node {
            // Literals
            IRNode::NumericLiteral(n) => self.write(n),
            IRNode::StringLiteral(s) => {
                self.write("\"");
                self.write_escaped(s);
                self.write("\"");
            }
            IRNode::RawStringLiteral(s) => {
                self.write("\"");
                self.write(s);
                self.write("\"");
            }
            IRNode::BooleanLiteral(b) => {
                self.write(if *b { "true" } else { "false" });
            }
            IRNode::NullLiteral => self.write("null"),
            IRNode::Undefined => self.write("void 0"),

            // Identifiers
            IRNode::Identifier(name) => {
                if let Some(subst) = self.commonjs_import_substitutions.get(name.as_ref()) {
                    let subst = subst.clone();
                    self.write(&subst);
                } else {
                    self.write(name);
                }
            }
            IRNode::RuntimeHelper(name) => {
                self.write_helper(name);
            }
            IRNode::This { captured } => {
                self.write(if *captured { "_this" } else { "this" });
            }
            IRNode::Super => self.write("super"),
            IRNode::ImportMeta => {
                self.write(if self.system_import_meta {
                    "context_1.meta"
                } else {
                    "import.meta"
                });
            }

            // Expressions
            IRNode::BinaryExpr {
                left,
                operator,
                right,
            } => {
                if *operator == "," {
                    self.emit_node(left);
                    self.write(", ");
                    self.emit_node(right);
                } else {
                    self.emit_sent_aware(left);
                    self.write(" ");
                    self.write(operator);
                    self.write(" ");
                    // Plain assignment operators (`=`, `+=`, etc.) don't need
                    // disambiguating parens around `_a.sent()` on the RHS — the
                    // call-expression precedence is unambiguous in that
                    // position. tsc emits `y = _a.sent();` without parens.
                    let is_assign = matches!(
                        operator.as_ref(),
                        "=" | "+="
                            | "-="
                            | "*="
                            | "/="
                            | "%="
                            | "**="
                            | "<<="
                            | ">>="
                            | ">>>="
                            | "&="
                            | "|="
                            | "^="
                            | "&&="
                            | "||="
                            | "??="
                    );
                    if is_assign {
                        self.emit_node(right);
                        if !self.remove_comments
                            && let IRNode::FunctionExpr { .. } = right.as_ref()
                            && let Some(comment) =
                                self.extract_trailing_comment_from_function(right)
                        {
                            self.write(" ");
                            self.write(&comment);
                            self.last_emit_ended_with_line_comment =
                                comment.trim_start().starts_with("//");
                        }
                    } else {
                        // `_a.sent()` does not need parentheses as the right operand of an
                        // arithmetic or comparison binary expression. tsc emits e.g.
                        // `_a + _b.sent()` without wrapping `_b.sent()` in parentheses.
                        // Parentheses are only needed when the call result is immediately
                        // used as an object for member access, which is handled at the
                        // PropertyAccess / ElementAccess emit sites via emit_sent_aware.
                        self.emit_node(right);
                    }
                }
            }
            IRNode::PrefixUnaryExpr { operator, operand } => {
                self.write(operator);
                self.emit_node(operand);
            }
            IRNode::PostfixUnaryExpr { operand, operator } => {
                self.emit_node(operand);
                self.write(operator);
            }
            IRNode::CallExpr { callee, arguments } => {
                // Check if this is an IIFE (immediately invoked function expression)
                // IIFEs should be wrapped in parentheses: (function() { ... })(args)
                let is_iife = matches!(&**callee, IRNode::FunctionExpr { .. });
                if is_iife {
                    self.write("(");
                    // IIFE function bodies should use multiline for empty bodies,
                    // matching TSC's behavior for synthetic code wrappers.
                    self.force_iife_multiline_empty = true;
                }
                self.emit_sent_aware(callee);
                self.force_iife_multiline_empty = false;
                if is_iife {
                    self.write(")");
                }
                self.write("(");
                self.emit_comma_separated(arguments);
                self.write(")");
            }
            IRNode::NewExpr {
                callee,
                arguments,
                explicit_arguments,
            } => {
                self.write("new ");
                self.emit_node(callee);
                if *explicit_arguments {
                    self.write("(");
                    self.emit_comma_separated(arguments);
                    self.write(")");
                }
            }
            IRNode::PropertyAccess { object, property } => {
                self.emit_sent_aware(object);
                if Self::ir_node_needs_double_dot(object) {
                    self.write("..");
                } else {
                    self.write(".");
                }
                self.write(property);
            }
            IRNode::ElementAccess { object, index } => {
                self.emit_node(object);
                self.write("[");
                self.emit_node(index);
                self.write("]");
            }
            IRNode::ConditionalExpr {
                condition,
                when_true,
                when_false,
            } => {
                self.emit_node(condition);
                self.write(" ? ");
                self.emit_node(when_true);
                self.write(" : ");
                self.emit_node(when_false);
            }
            IRNode::Parenthesized(expr) => {
                self.write("(");
                self.emit_node(expr);
                self.write(")");
            }
            IRNode::CommaExpr(exprs) => {
                self.write("(");
                self.emit_comma_separated(exprs);
                self.write(")");
            }
            IRNode::CommaExprMultiline(exprs) => {
                // Multiline comma expression for ES5 computed property lowering:
                // (_a = {},
                //     _a[key] = value,
                //     _a)
                self.write("(");
                self.indent_level += 1;
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        if self.last_emit_ended_with_line_comment {
                            self.write_line();
                            self.write_indent_level(self.indent_level.saturating_sub(1));
                        }
                        self.last_emit_ended_with_line_comment = false;
                        self.write(",");
                        self.write_line();
                        self.write_indent();
                    }
                    self.last_emit_ended_with_line_comment = false;
                    self.emit_node(expr);
                }
                self.indent_level -= 1;
                self.write(")");
            }
            IRNode::CommaExprMultilineFlat(exprs) => {
                self.write("(");
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        if self.last_emit_ended_with_line_comment {
                            self.write_line();
                            self.write_indent();
                        }
                        self.last_emit_ended_with_line_comment = false;
                        self.write(",");
                        self.write_line();
                        self.write_indent();
                    }
                    self.last_emit_ended_with_line_comment = false;
                    self.emit_node(expr);
                }
                self.write(")");
            }
            IRNode::ArrayLiteral(elements) => {
                self.write("[");
                self.emit_comma_separated(elements);
                self.write("]");
            }
            IRNode::SpreadElement(expr) => {
                self.write("...");
                self.emit_node(expr);
            }
            IRNode::ObjectLiteral {
                properties,
                source_range,
                extra_indent,
            } => {
                if properties.is_empty() {
                    self.write("{}");
                    return;
                }

                // Check if the object was multiline in source
                let is_multiline = if let Some((pos, end)) = source_range {
                    !self.is_single_line_range(*pos, *end)
                } else {
                    false
                };
                let has_source_trailing_comma = source_range.is_some_and(|(pos, end)| {
                    self.object_literal_source_has_trailing_comma(pos, end)
                });

                if is_multiline {
                    // Multiline format
                    self.write("{");
                    self.write_line();
                    let extra_indent = u32::from(*extra_indent);
                    self.indent_level += 1 + extra_indent;
                    for (i, prop) in properties.iter().enumerate() {
                        self.write_indent();
                        self.emit_property(prop);
                        if i < properties.len() - 1 || has_source_trailing_comma {
                            self.write(",");
                        }
                        self.write_line();
                    }
                    self.indent_level -= 1;
                    self.write_indent();
                    self.write("}");
                    self.indent_level -= extra_indent;
                } else {
                    // Single-line format
                    self.write("{ ");
                    for (i, prop) in properties.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.emit_property(prop);
                    }
                    self.write(" }");
                }
            }
            IRNode::FunctionExpr {
                name,
                parameters,
                body,
                is_expression_body,
                body_source_range,
            } => {
                self.write("function ");
                if let Some(n) = name {
                    self.write(n);
                }
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                let has_defaults = parameters.iter().any(|p| p.default_value.is_some());
                let is_source_single_line = self.is_body_source_single_line(*body_source_range);

                // Single-line function body: { return expr; }
                // Applies to:
                // 1. Arrow-to-function conversions (is_expression_body)
                // 2. Functions that were single-line in source (is_source_single_line)
                // tsc never collapses multi-line function bodies to single line,
                // so we should NOT use heuristics to guess single-line for generated code.
                let should_emit_single_line = *is_expression_body || is_source_single_line;

                let has_rest_to_lower = self.target_es5 && parameters.iter().any(|p| p.rest);
                let has_new_target_capture = body
                    .first()
                    .is_some_and(|node| matches!(node, IRNode::NewTargetCapture { .. }));
                if !has_defaults
                    && !has_rest_to_lower
                    && !has_new_target_capture
                    && should_emit_single_line
                    && body.len() == 1
                    && match &body[0] {
                        IRNode::ReturnStatement(Some(expr)) => {
                            self.write("{ return ");
                            self.emit_node(expr);
                            self.write("; }");
                            true
                        }
                        IRNode::ExpressionStatement(expr) => {
                            self.write("{ ");
                            self.emit_node(expr);
                            self.write("; }");
                            true
                        }
                        IRNode::AwaiterCall { .. } => {
                            self.write("{ ");
                            self.emit_node(&body[0]);
                            self.write(" }");
                            true
                        }
                        _ => false,
                    }
                {
                    return;
                }
                if !has_defaults
                    && !has_rest_to_lower
                    && !has_new_target_capture
                    && should_emit_single_line
                    && body.len() == 2
                    && matches!(body[0], IRNode::VarDeclList(_))
                    && matches!(
                        body[1],
                        IRNode::ReturnStatement(_) | IRNode::ExpressionStatement(_)
                    )
                {
                    self.write("{ ");
                    self.emit_node(&body[0]);
                    self.write(" ");
                    self.emit_node(&body[1]);
                    self.write(" }");
                    return;
                }
                let force_multiline_empty = self.force_iife_multiline_empty
                    || matches!(name, Some(n) if self.current_class_iife_name.as_deref() == Some(&**n));
                let previous_generator_state_name = self.generator_state_name;
                if let Some(generator_state_name) =
                    Self::generator_state_name_for_function_body(body)
                {
                    self.generator_state_name = generator_state_name;
                }
                self.emit_function_body_with_defaults(
                    parameters,
                    body,
                    *body_source_range,
                    force_multiline_empty,
                );
                self.generator_state_name = previous_generator_state_name;
            }
            IRNode::LogicalOr { left, right } => {
                self.emit_node(left);
                self.write(" || ");
                // Wrap assignment expressions in parens for correctness: E || (E = {})
                let needs_parens = matches!(
                    &**right,
                    IRNode::BinaryExpr { operator, .. } if operator == "="
                );
                if needs_parens {
                    self.write("(");
                }
                self.emit_node(right);
                if needs_parens {
                    self.write(")");
                }
            }
            IRNode::LogicalAnd { left, right } => {
                self.emit_node(left);
                self.write(" && ");
                self.emit_node(right);
            }

            // Statements
            IRNode::HoistedVarGroupBreak => {}
            IRNode::VarDecl { name, initializer } => {
                self.write("var ");
                self.write(name);
                if let Some(init) = initializer {
                    self.write(" = ");
                    self.emit_node(init);
                }
                self.write(";");
            }
            IRNode::VarDeclList(decls) => {
                self.write("var ");
                for (i, decl) in decls.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if let IRNode::VarDecl { name, initializer } = decl {
                        self.write(name);
                        if let Some(init) = initializer {
                            self.write(" = ");
                            self.emit_node(init);
                        }
                    } else {
                        self.emit_node(decl);
                    }
                }
                self.write(";");
            }
            IRNode::NewTargetCapture { initializer } => {
                self.write("var _newTarget = ");
                self.emit_node(initializer);
                self.write(";");
            }
            IRNode::ExpressionStatement(expr) => {
                if let IRNode::CommaExprMultiline(exprs) = expr.as_ref() {
                    self.indent_level += 1;
                    for (i, expr) in exprs.iter().enumerate() {
                        if i > 0 {
                            if self.last_emit_ended_with_line_comment {
                                self.write_line();
                                self.write_indent_level(self.indent_level.saturating_sub(1));
                            }
                            self.last_emit_ended_with_line_comment = false;
                            self.write(",");
                            self.write_line();
                            self.write_indent();
                        }
                        self.last_emit_ended_with_line_comment = false;
                        self.emit_node(expr);
                    }
                    self.indent_level -= 1;
                    self.write(";");
                    return;
                }

                // Wrap function/object expressions in parens when in statement
                // position to prevent declaration/block ambiguity.
                let needs_paren = matches!(
                    expr.as_ref(),
                    IRNode::FunctionExpr { .. } | IRNode::ObjectLiteral { .. }
                );
                if needs_paren {
                    self.write("(");
                }
                let prev_ast_arrow_comment_defer_end = self.ast_arrow_comment_defer_end;
                self.ast_arrow_comment_defer_end =
                    self.source_text.map(|source| source.len() as u32);
                self.emit_node(expr);
                self.ast_arrow_comment_defer_end = prev_ast_arrow_comment_defer_end;
                if needs_paren {
                    self.write(")");
                }
                self.write(";");
            }
            IRNode::ReturnStatement(expr) => {
                self.write("return");
                if let Some(e) = expr {
                    self.write(" ");
                    if let IRNode::ObjectLiteral {
                        properties,
                        source_range: None,
                        extra_indent: 0,
                    } = &**e
                        && Self::is_done_value_object_literal(properties)
                    {
                        self.emit_object_literal_multiline(properties);
                    } else {
                        self.emit_node(e);
                    }
                }
                self.write(";");
            }
            IRNode::IfStatement {
                condition,
                then_branch,
                else_branch,
            } => {
                self.write("if (");
                self.emit_node(condition);
                self.write(")");
                if Self::is_generator_break_return(then_branch) {
                    self.write_line();
                    self.increase_indent();
                    self.write_indent();
                    self.emit_node(then_branch);
                    self.decrease_indent();
                } else {
                    self.write(" ");
                    self.emit_node(then_branch);
                }
                if let Some(else_br) = else_branch {
                    self.write_line();
                    self.write_indent();
                    self.write("else");
                    match else_br.as_ref() {
                        IRNode::Block(_) | IRNode::IfStatement { .. } => {
                            self.write(" ");
                            self.emit_node(else_br);
                        }
                        _ => {
                            self.write_line();
                            self.increase_indent();
                            self.write_indent();
                            self.emit_node(else_br);
                            self.decrease_indent();
                        }
                    }
                }
            }
            IRNode::Block(stmts) => {
                self.emit_block(stmts);
            }
            IRNode::EmptyStatement => {
                self.write(";");
            }
            IRNode::SwitchStatement { expression, cases } => {
                self.write("switch (");
                self.emit_node(expression);
                self.write(") {");
                self.write_line();
                self.increase_indent();
                for case in cases {
                    self.emit_switch_case(case);
                }
                self.decrease_indent();
                self.write_indent();
                self.write("}");
            }
            IRNode::ForStatement {
                initializer,
                condition,
                incrementor,
                body,
            } => {
                self.write("for (");
                if let Some(init) = initializer {
                    self.emit_for_initializer(init);
                }
                self.write(";");
                if let Some(cond) = condition {
                    self.write(" ");
                    self.emit_node(cond);
                }
                self.write(";");
                if let Some(incr) = incrementor {
                    self.write(" ");
                    self.emit_node(incr);
                }
                self.write(") ");
                self.emit_node(body);
            }
            IRNode::ForInOfStatement {
                kind,
                initializer,
                expression,
                body,
                multiline_body,
            } => {
                self.write("for (");
                self.emit_node(initializer);
                self.write(" ");
                self.write(kind);
                self.write(" ");
                self.emit_node(expression);
                self.write(") ");
                if *multiline_body && !matches!(&**body, IRNode::Block(_)) {
                    self.write_line();
                    self.increase_indent();
                    self.write_indent();
                    self.emit_node(body);
                    self.decrease_indent();
                } else {
                    self.emit_node(body);
                }
            }
            IRNode::WhileStatement { condition, body } => {
                self.write("while (");
                self.emit_node(condition);
                self.write(") ");
                self.emit_node(body);
            }
            IRNode::DoWhileStatement { body, condition } => {
                self.write("do ");
                self.emit_node(body);
                self.write(" while (");
                self.emit_node(condition);
                self.write(");");
            }
            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            } => {
                self.write("try ");
                self.emit_node(try_block);
                if let Some(catch) = catch_clause {
                    self.write_line();
                    self.write_indent();
                    self.write("catch");
                    if let Some(param) = &catch.param {
                        self.write(" (");
                        self.write(param);
                        self.write(")");
                    }
                    self.write(" ");
                    if catch.single_line {
                        self.emit_block_single_line(&catch.body);
                    } else {
                        self.emit_block(&catch.body);
                    }
                }
                if let Some(finally) = finally_block {
                    self.write_line();
                    self.write_indent();
                    self.write("finally ");
                    self.emit_node(finally);
                }
            }
            IRNode::ThrowStatement(expr) => {
                self.write("throw ");
                self.emit_node(expr);
                if self.is_recovered_empty_property_access(expr) {
                    self.write_line();
                    self.write_indent();
                    self.write(";");
                } else {
                    self.write(";");
                }
            }
            IRNode::BreakStatement(label) => {
                self.write("break");
                if let Some(l) = label {
                    self.write(" ");
                    self.write(l);
                }
                self.write(";");
            }
            IRNode::ContinueStatement(label) => {
                self.write("continue");
                if let Some(l) = label {
                    self.write(" ");
                    self.write(l);
                }
                self.write(";");
            }
            IRNode::LabeledStatement { label, statement } => {
                self.write(label);
                self.write(": ");
                self.emit_node(statement);
            }

            // Declarations
            IRNode::FunctionDecl {
                name,
                parameters,
                body,
                body_source_range,
                leading_comment,
            } => {
                // Emit leading JSDoc/block comment if present (e.g., constructor comment)
                if !self.remove_comments
                    && let Some(comment) = leading_comment
                {
                    self.emit_multiline_comment(comment);
                    self.write_line();
                    self.write_indent();
                }
                self.write("function ");
                self.write(name);
                self.write("(");
                self.emit_parameters(parameters);
                self.write(") ");
                let force_multiline_empty =
                    self.current_class_iife_name.as_deref() == Some(&**name);
                let previous_generator_state_name = self.generator_state_name;
                if let Some(generator_state_name) =
                    Self::generator_state_name_for_function_body(body)
                {
                    self.generator_state_name = generator_state_name;
                }
                self.emit_function_body_with_defaults(
                    parameters,
                    body,
                    *body_source_range,
                    force_multiline_empty,
                );
                self.generator_state_name = previous_generator_state_name;
                if !self.remove_comments
                    && !self.suppress_function_trailing_extraction
                    && let Some(comment) = self.extract_trailing_comment_from_function(node)
                {
                    self.write(" ");
                    self.write(&comment);
                }
            }

            // ES5 Class Transform Specific
            IRNode::ES5ClassIIFE { .. } => self.emit_es5_class_iife_node(node),
            IRNode::ES5ClassAssignment { .. } => self.emit_es5_class_assignment_node(node),
            IRNode::StaticBlockIIFE { statements } => {
                // (function () { ...statements... })();
                self.emit_static_block_iife_expression(statements);
                self.write(";");
            }
            IRNode::ExtendsHelper {
                class_name,
                super_name,
            } => {
                self.write_helper("__extends");
                self.write("(");
                self.write(class_name);
                self.write(", ");
                self.write(super_name);
                self.write(");");
            }
            IRNode::ES5ClassApply {
                factory,
                base_class,
            } => {
                if !self.remove_comments {
                    self.write("/** @class */ ");
                }
                self.write("(");
                self.emit_node(factory);
                self.write(".apply(void 0, [(");
                self.emit_node(base_class);
                self.write(")]))");
            }
            IRNode::PrototypeMethod { .. } => self.emit_prototype_method_node(node),
            IRNode::StaticMethod { .. } => self.emit_static_method_node(node),
            IRNode::DefineProperty { .. } => self.emit_define_property_node(node),

            // Async Transform Specific
            IRNode::AwaiterCall { .. } => self.emit_awaiter_call_node(node),
            IRNode::GeneratorBody { .. } => self.emit_generator_body_node(node),
            IRNode::GeneratorOp {
                opcode,
                value,
                comment,
            } => {
                self.write("[");
                self.write(&opcode.to_string());
                if let Some(cmt) = comment {
                    self.write(" /*");
                    self.write(cmt);
                    self.write("*/");
                }
                if let Some(val) = value {
                    self.write(", ");
                    self.emit_node(val);
                }
                self.write("]");
            }
            IRNode::GeneratorSent => {
                self.write(self.generator_state_name);
                self.write(".sent()");
            }
            IRNode::GeneratorLabel => {
                self.write(self.generator_state_name);
                self.write(".label");
            }
            IRNode::GeneratorTryPush {
                start_label,
                catch_label,
                finally_label,
                end_label,
            } => {
                self.write(self.generator_state_name);
                self.write(".trys.push([");
                self.write(&start_label.to_string());
                self.write(", ");
                self.write(&catch_label.to_string());
                self.write(", ");
                self.write(&finally_label.to_string());
                self.write(", ");
                self.write(&end_label.to_string());
                self.write("]);");
            }
            IRNode::GeneratorTryPushFinally {
                start_label,
                finally_label,
                end_label,
            } => {
                self.write(self.generator_state_name);
                self.write(".trys.push([");
                self.write(&start_label.to_string());
                self.write(", , ");
                self.write(&finally_label.to_string());
                self.write(", ");
                self.write(&end_label.to_string());
                self.write("]);");
            }
            IRNode::GeneratorTryPushCatch {
                start_label,
                catch_label,
                end_label,
            } => {
                self.write(self.generator_state_name);
                self.write(".trys.push([");
                self.write(&start_label.to_string());
                self.write(", ");
                self.write(&catch_label.to_string());
                self.write(", , ");
                self.write(&end_label.to_string());
                self.write("]);");
            }

            IRNode::IfBreak {
                condition,
                target_label,
            } => {
                self.write("if (");
                self.emit_node(condition);
                self.write(") return [3 /*break*/, ");
                self.write(&target_label.to_string());
                self.write("];");
            }

            // Private Field Helpers
            IRNode::PrivateFieldGet {
                receiver,
                weakmap_name,
            } => {
                self.write_helper("__classPrivateFieldGet");
                self.write("(");
                self.emit_node(receiver);
                self.write(", ");
                self.write(weakmap_name);
                self.write(", \"f\")");
            }
            IRNode::PrivateStaticFieldGet {
                receiver,
                state,
                storage_name,
            } => {
                self.write_helper("__classPrivateFieldGet");
                self.write("(");
                self.emit_node(receiver);
                self.write(", ");
                self.emit_node(state);
                self.write(", \"f\", ");
                self.write(storage_name);
                self.write(")");
            }
            IRNode::PrivateFieldSet {
                receiver,
                weakmap_name,
                value,
            } => {
                self.write_helper("__classPrivateFieldSet");
                self.write("(");
                self.emit_node(receiver);
                self.write(", ");
                self.write(weakmap_name);
                self.write(", ");
                self.emit_node(value);
                self.write(", \"f\")");
            }
            IRNode::PrivateStaticFieldSet {
                receiver,
                state,
                storage_name,
                value,
            } => {
                self.write_helper("__classPrivateFieldSet");
                self.write("(");
                self.emit_node(receiver);
                self.write(", ");
                self.emit_node(state);
                self.write(", ");
                self.emit_node(value);
                self.write(", \"f\", ");
                self.write(storage_name);
                self.write(")");
            }
            IRNode::PrivateFieldIn { weakmap_name, obj } => {
                self.write_helper("__classPrivateFieldIn");
                self.write("(");
                self.write(weakmap_name);
                self.write(", ");
                self.emit_node(obj);
                self.write(")");
            }
            IRNode::WeakMapSet {
                weakmap_name,
                key,
                value,
            } => {
                self.write(weakmap_name);
                self.write(".set(");
                self.emit_node(key);
                self.write(", ");
                self.emit_node(value);
                self.write(")");
            }

            // Special
            IRNode::Raw(s) => {
                // Comments stored as Raw nodes bypass IRNode::Comment guards.
                // Detect and suppress them when removeComments is enabled.
                if self.remove_comments {
                    let t = s.trim_start();
                    if t.starts_with("//") || t.starts_with("/*") {
                        // Skip comment-like Raw node
                    } else {
                        self.write(s);
                    }
                } else {
                    self.write(s);
                }
            }
            IRNode::Comment { text, is_block } => {
                if !self.remove_comments {
                    if *is_block {
                        self.write("/*");
                        self.write(text);
                        self.write("*/");
                    } else {
                        self.write("// ");
                        self.write(text);
                    }
                }
            }
            IRNode::TrailingComment(text) => {
                // When encountered outside the body loop, just emit the text.
                // Inside the body loop, this is consumed by peek-ahead logic.
                if !self.remove_comments {
                    self.write(" ");
                    self.write(text);
                }
            }
            IRNode::Sequence(nodes) => {
                let mut i = 0;
                while i < nodes.len() {
                    if matches!(&nodes[i], IRNode::TrailingComment(_)) {
                        i += 1;
                        continue;
                    }
                    // Skip comment nodes when removeComments is enabled
                    if self.remove_comments
                        && (matches!(&nodes[i], IRNode::Comment { .. })
                            || matches!(&nodes[i], IRNode::Raw(s) if s.trim_start().starts_with("//") || s.trim_start().starts_with("/*")))
                    {
                        i += 1;
                        continue;
                    }
                    if i + 1 < nodes.len()
                        && let Some((enum_name, members, namespace)) =
                            Self::enum_with_matching_namespace_export(&nodes[i], &nodes[i + 1])
                    {
                        self.emit_namespace_bound_enum_iife(enum_name, members, namespace);
                        i += 2;
                        if i < nodes.len() {
                            self.write_line();
                            if Self::should_indent_sequence_child(&nodes[i]) {
                                self.write_indent();
                            }
                        }
                        continue;
                    }

                    let suppress_for_this_node = i + 1 < nodes.len()
                        && matches!(&nodes[i], IRNode::FunctionDecl { .. })
                        && matches!(&nodes[i + 1], IRNode::TrailingComment(_));
                    let prev_suppress = self.suppress_function_trailing_extraction;
                    self.suppress_function_trailing_extraction = suppress_for_this_node;
                    self.emit_node(&nodes[i]);
                    self.suppress_function_trailing_extraction = prev_suppress;
                    if i + 1 < nodes.len()
                        && let IRNode::TrailingComment(text) = &nodes[i + 1]
                    {
                        if !self.remove_comments {
                            self.write(" ");
                            self.write(text);
                        }
                        i += 1;
                    }

                    if i < nodes.len() - 1 {
                        self.write_line();
                        if Self::should_indent_sequence_child(&nodes[i + 1]) {
                            self.write_indent();
                        }
                    }
                    i += 1;
                }
            }
            IRNode::WithStatement { expression, body } => {
                self.write("with (");
                self.emit_node(expression);
                self.write(") ");
                self.emit_node(body);
            }
            IRNode::ASTRef(_) => self.emit_ast_ref_node(node),

            IRNode::ASTRefWithGeneratorThis {
                node,
                generator_this,
            } => {
                if let Some(arena) = self.arena {
                    let mut printer = self.build_nested_ast_printer(arena);
                    printer.emit_expression(*node);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    let output = printer.get_output();
                    let rewritten = output.replacen(
                        "__generator(this,",
                        &format!("__generator({generator_this},"),
                        1,
                    );
                    self.write_embedded_output(&Self::rename_colliding_outer_generator_state(
                        &rewritten,
                        generator_this,
                    ));
                    return;
                }
                self.write("undefined");
            }

            IRNode::ASTRefWithCapturedClassHeritageThis(idx) => {
                if let Some(arena) = self.arena {
                    let mut printer = self.build_nested_ast_printer(arena);
                    printer.set_es5_class_expression_extends_this_captured(true);
                    printer.emit_expression(*idx);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    let output = printer.get_output();
                    if !output.trim().is_empty() {
                        self.write_embedded_output(output.trim());
                        return;
                    }
                }
                self.write("undefined");
            }

            IRNode::ASTRefRange(idx, max_end) => {
                // Like ASTRef but with a constrained end position.
                // Used when a statement's node.end extends into a parent block's closing brace.
                if let Some(arena) = self.arena {
                    let mut printer = self.build_nested_ast_printer(arena);
                    self.configure_ast_printer_namespace(&mut printer);
                    printer.emit(*idx);
                    self.merge_ast_printer_block_scope_reserved_names(&printer);
                    let trimmed = printer.get_output().trim();
                    if !trimmed.is_empty() {
                        self.write_embedded_output(trimmed);
                        return;
                    }
                }

                if let Some(arena) = self.arena
                    && let Some(text) = self.source_text
                    && let Some(node) = arena.get(*idx)
                {
                    let start = node.pos as usize;
                    let end = std::cmp::min(*max_end as usize, text.len());
                    let end = std::cmp::min(end, node.end as usize);
                    if start < end {
                        let raw = &text[start..end];
                        let trimmed = raw.trim();
                        if !trimmed.is_empty() {
                            self.write(trimmed);
                            return;
                        }
                    }
                }
                self.write("undefined");
            }

            // CommonJS Module Transform Specific
            IRNode::UseStrict => {
                self.write("\"use strict\";");
            }
            IRNode::EsesModuleMarker => {
                self.write("Object.defineProperty(exports, \"__esModule\", { value: true });");
            }
            IRNode::ExportInit { name } => {
                self.write("exports.");
                self.write(name);
                self.write(" = void 0;");
            }
            IRNode::RequireStatement {
                var_name,
                module_spec,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = require(\"");
                self.write(module_spec);
                self.write("\");");
            }
            IRNode::DefaultImport {
                var_name,
                module_var,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = ");
                self.write(module_var);
                self.write(".default;");
            }
            IRNode::NamespaceImport {
                var_name,
                module_var,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = __importStar(");
                self.write(module_var);
                self.write(");");
            }
            IRNode::NamedImport {
                var_name,
                module_var,
                import_name,
            } => {
                self.write("var ");
                self.write(var_name);
                self.write(" = ");
                self.write(module_var);
                self.write(".");
                self.write(import_name);
                self.write(";");
            }
            IRNode::ExportAssignment { name } => {
                self.write("exports.");
                self.write(name);
                self.write(" = ");
                self.write(name);
                self.write(";");
            }
            IRNode::ReExportProperty {
                export_name,
                module_var,
                import_name,
            } => {
                self.write("Object.defineProperty(exports, \"");
                self.write(export_name);
                self.write("\", { enumerable: true, get: function () { return ");
                self.write(module_var);
                self.write(".");
                self.write(import_name);
                self.write("; } });");
            }

            // Enum Transform Specific
            IRNode::EnumIIFE {
                name,
                members,
                namespace_export,
                invalid_namespace_static,
            } => {
                if let Some(ns) = namespace_export {
                    self.emit_namespace_bound_enum_iife(name, members, ns);
                } else {
                    // var E; (top-level) or let E; (inside namespace/function body at ES2015+)
                    let keyword = if self.in_namespace_iife_body && !self.target_es5 {
                        "let"
                    } else {
                        "var"
                    };
                    if *invalid_namespace_static && self.in_namespace_iife_body && !self.target_es5
                    {
                        self.write("static ");
                    }
                    self.write(keyword);
                    self.write(" ");
                    self.write(name);
                    self.write(";");
                    self.write_line();
                    self.write_indent();
                    self.write("(function (");
                    self.write(name);
                    self.write(") {");
                    self.write_line();
                    self.increase_indent();

                    // Emit members
                    for member in members {
                        self.write_indent();
                        self.emit_enum_member(name, member);
                        self.write_line();
                    }

                    self.decrease_indent();
                    self.write_indent();
                    self.write("})(");
                    self.write(name);
                    self.write(" || (");
                    self.write(name);
                    self.write(" = {}));");
                }
            }

            // Namespace Transform Specific
            IRNode::NamespaceIIFE {
                name: _name,
                name_parts,
                body,
                is_exported,
                attach_to_exports,
                commonjs_export_names,
                system_export_names,
                should_declare_var,
                parent_name,
                param_name,
                default_export_merge,
                skip_sequence_indent: _,
                trailing_comment,
                invalid_namespace_static,
            } => {
                self.emit_namespace_iife(
                    name_parts,
                    0,
                    body,
                    NamespaceIifeContext {
                        is_exported: *is_exported,
                        attach_to_exports: *attach_to_exports,
                        commonjs_export_names,
                        system_export_names,
                        should_declare_var: *should_declare_var,
                        default_export_merge: *default_export_merge,
                        parent_name: parent_name.as_deref(),
                        param_name: param_name.as_deref(),
                        invalid_static_declaration: *invalid_namespace_static,
                    },
                );
                if !self.remove_comments
                    && let Some(comment) = trailing_comment
                {
                    self.write(" ");
                    self.write(comment);
                }
            }
            IRNode::NamespaceExport {
                namespace,
                name,
                value,
            } => {
                self.write(namespace);
                self.write(".");
                self.write(name);
                self.write(" = ");
                self.emit_node(value);
                self.write(";");
            }
        }
    }

    fn is_recovered_empty_property_access(&self, node: &IRNode) -> bool {
        match node {
            IRNode::PropertyAccess { property, .. } => property.is_empty(),
            IRNode::Raw(text) => text.trim_end().ends_with('.'),
            IRNode::ASTRef(idx) => self.arena.is_some_and(|arena| {
                arena.get(*idx).is_some_and(|node| {
                    if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && arena.get_access_expr(node).is_some_and(|access| {
                            arena.is_missing_recovery_identifier(access.name_or_argument)
                        })
                    {
                        return true;
                    }

                    self.source_text.is_some_and(|source_text| {
                        let start = (node.pos as usize).min(source_text.len());
                        let end = (node.end as usize).min(source_text.len());
                        if start < end {
                            let span = &source_text[start..end];
                            span.trim_end().ends_with('.')
                                || Self::source_span_has_recovered_property_boundary(span)
                                || Self::source_has_recovered_property_boundary(source_text, end)
                        } else {
                            Self::source_has_recovered_property_boundary(source_text, end)
                        }
                    })
                })
            }),
            _ => false,
        }
    }

    fn source_has_recovered_property_boundary(source_text: &str, end: usize) -> bool {
        let bytes = source_text.as_bytes();
        if bytes.get(end) != Some(&b'.') {
            return false;
        }

        let mut i = end + 1;
        while let Some(byte) = bytes.get(i) {
            match byte {
                b' ' | b'\t' | b'\r' => i += 1,
                b'\n' | b';' | b'}' => return true,
                _ => return false,
            }
        }
        true
    }

    fn source_span_has_recovered_property_boundary(span: &str) -> bool {
        let bytes = span.as_bytes();
        for dot in bytes
            .iter()
            .enumerate()
            .filter_map(|(i, byte)| (*byte == b'.').then_some(i))
        {
            let mut i = dot + 1;
            while let Some(byte) = bytes.get(i) {
                match byte {
                    b' ' | b'\t' | b'\r' => i += 1,
                    b'\n' => {
                        i += 1;
                        while matches!(bytes.get(i), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                            i += 1;
                        }
                        return bytes.get(i).is_none_or(|byte| matches!(byte, b';' | b'}'));
                    }
                    b';' | b'}' => return true,
                    _ => break,
                }
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "../../tests/ir_printer.rs"]
mod tests;
