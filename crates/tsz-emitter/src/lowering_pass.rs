//! Lowering Pass - Phase 1 of the Transform/Print Architecture
//!
//! This module implements the first phase of emission: analyzing the AST and
//! producing transform directives. The lowering pass walks the Node AST
//! and determines which nodes need transformation based on compiler options
//! (ES5 target, module format, etc.).
//!
//! # Architecture
//!
//! The lowering pass is a **read-only** traversal of the AST that produces
//! a `TransformContext` containing `TransformDirective`s for nodes that need
//! special handling during emission.
//!
//! ## Examples
//!
//! ### ES5 Class Transform
//!
//! When `target: ES5`, a `ClassDeclaration` needs transformation:
//!
//! ```typescript
//! class Point {
//!     constructor(x, y) { this.x = x; this.y = y; }
//! }
//! ```
//!
//! The lowering pass creates a `TransformDirective::ES5Class` for this node,
//! which the printer will use to emit an IIFE pattern instead of `class`.
//!
//! ### `CommonJS` Export
//!
//! When `module: CommonJS`, exported declarations need wrapping:
//!
//! ```typescript
//! export class Foo {}
//! ```
//!
//! The lowering pass creates a `TransformDirective::CommonJSExport` that
//! chains with any other transforms (like `ES5Class`).

#[path = "lowering_pass_helpers.rs"]
mod lowering_pass_helpers;

use crate::emit_context::EmitContext;
use crate::transform_context::{IdentifierId, ModuleFormat, TransformContext, TransformDirective};
use std::sync::Arc;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::{
    contains_arguments_reference, contains_this_reference, is_private_identifier,
};
use tsz_scanner::SyntaxKind;

/// Maximum recursion depth for AST traversal to prevent stack overflow
const MAX_AST_DEPTH: u32 = 500;

/// Maximum depth for qualified name recursion (A.B.C.D...)
const MAX_QUALIFIED_NAME_DEPTH: u32 = 100;

/// Maximum depth for binding pattern recursion ({a: {b: {c: ...}}})
const MAX_BINDING_PATTERN_DEPTH: u32 = 100;

/// Lowering pass - Phase 1 of emission
///
/// Walks the AST and produces transform directives based on compiler options.
pub struct LoweringPass<'a> {
    arena: &'a NodeArena,
    ctx: &'a EmitContext,
    transforms: TransformContext,
    commonjs_mode: bool,
    has_export_assignment: bool,
    /// Current recursion depth for stack overflow protection
    visit_depth: u32,
    /// Track declared names for namespace/class/enum/function merging detection
    declared_names: rustc_hash::FxHashSet<String>,
    /// Depth of arrow functions that capture 'this'
    /// When > 0, 'this' references should be substituted with '_this'
    this_capture_level: u32,
    /// Depth of arrow functions that capture 'arguments'
    /// When > 0, 'arguments' references should be substituted with '_arguments'
    arguments_capture_level: u32,
    /// Tracks if the current class declaration has an 'extends' clause
    current_class_is_derived: bool,
    /// Tracks if we are currently inside a constructor body
    in_constructor: bool,
    /// Tracks if we are inside a static class member
    in_static_context: bool,
    /// Current class alias name (e.g., "_a") for static members
    current_class_alias: Option<String>,
    /// True when visiting the left side of a destructuring assignment
    in_assignment_target: bool,
    /// True when inside a class body in ES5 mode.
    /// Arrow functions inside class members should NOT propagate _this capture
    /// to the enclosing scope because the class IIFE creates its own scope
    /// and `class_es5_ir` handles _this capture independently.
    in_es5_class: bool,
    /// Stack of enclosing non-arrow function body node indices.
    /// When an arrow function captures `this`, the top of this stack is the
    /// scope that needs `var _this = this;`.
    enclosing_function_bodies: Vec<NodeIndex>,
    /// Stack of capture variable names matching `enclosing_function_bodies`.
    /// Each entry is the name to use for `_this` capture in that scope
    /// (e.g., "_this" or "_`this_1`" if there's a collision with a user-defined `_this`).
    enclosing_capture_names: Vec<Arc<str>>,
}

impl<'a> LoweringPass<'a> {
    /// Create a new lowering pass
    pub fn new(arena: &'a NodeArena, ctx: &'a EmitContext) -> Self {
        LoweringPass {
            arena,
            ctx,
            transforms: TransformContext::new(),
            commonjs_mode: false,
            has_export_assignment: false,
            visit_depth: 0,
            declared_names: rustc_hash::FxHashSet::default(),
            this_capture_level: 0,
            arguments_capture_level: 0,
            current_class_is_derived: false,
            in_constructor: false,
            in_static_context: false,
            current_class_alias: None,
            in_assignment_target: false,
            in_es5_class: false,
            enclosing_function_bodies: Vec::new(),
            enclosing_capture_names: Vec::new(),
        }
    }

    /// Run the lowering pass on a source file and return the transform context
    pub fn run(mut self, source_file: NodeIndex) -> TransformContext {
        self.init_module_state(source_file);
        // Push source file as the top-level _this capture scope
        if self.ctx.target_es5 {
            let capture_name = self.compute_this_capture_name(source_file);
            self.enclosing_function_bodies.push(source_file);
            self.enclosing_capture_names.push(capture_name);
        }
        self.visit(source_file);
        if self.ctx.target_es5 {
            self.enclosing_function_bodies.pop();
            self.enclosing_capture_names.pop();
        }
        self.maybe_wrap_module(source_file);
        self.transforms.mark_helpers_populated();

        if tracing::enabled!(tracing::Level::DEBUG) {
            let arrow_captures = self
                .transforms
                .iter()
                .filter_map(|(idx, directive)| match directive {
                    TransformDirective::ES5ArrowFunction {
                        arrow_node: _,
                        captures_this,
                        captures_arguments: _,
                        class_alias: _,
                    } => Some((idx, *captures_this)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            tracing::debug!(
                "[lowering] source={} arrow directives: {arrow_captures:?}",
                source_file.0
            );
            if let Some(capture_name) = self.transforms.this_capture_name(source_file) {
                tracing::debug!(
                    "[lowering] source {} this capture: {capture_name}",
                    source_file.0
                );
            } else {
                tracing::debug!("[lowering] source {} no this capture scope", source_file.0);
            }
        }

        self.transforms
    }

    /// Visit a node and its children
    fn visit(&mut self, idx: NodeIndex) {
        // Stack overflow protection: limit recursion depth
        if self.visit_depth >= MAX_AST_DEPTH {
            return;
        }
        self.visit_depth += 1;

        let Some(node) = self.arena.get(idx) else {
            self.visit_depth -= 1;
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => self.visit_class_declaration(node, idx),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.visit_function_declaration(node, idx);
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.visit_function_expression(node, idx);
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.visit_arrow_function(node, idx),
            k if k == syntax_kind_ext::CONSTRUCTOR => self.visit_constructor(node, idx),
            k if k == syntax_kind_ext::CALL_EXPRESSION => self.visit_call_expression(node, idx),
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.visit_new_expression(node),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.visit_variable_statement(node, idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => self.visit_enum_declaration(node, idx),
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.visit_module_declaration(node, idx);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.visit_export_declaration(node, idx);
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.visit_import_declaration(node, idx);
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => self.visit_for_in_statement(node),
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => self.visit_for_of_statement(node, idx),
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // If we're inside a capturing arrow function, substitute 'this' with '_this'
                if self.this_capture_level > 0 {
                    let capture_name = self
                        .enclosing_capture_names
                        .last()
                        .cloned()
                        .unwrap_or_else(|| Arc::from("_this"));
                    self.transforms
                        .insert(idx, TransformDirective::SubstituteThis { capture_name });
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if self.this_capture_level > 0
                    && let Some(text) = self.get_identifier_text_ref(idx)
                    && text == "this"
                {
                    let capture_name = self
                        .enclosing_capture_names
                        .last()
                        .cloned()
                        .unwrap_or_else(|| Arc::from("_this"));
                    self.transforms
                        .insert(idx, TransformDirective::SubstituteThis { capture_name });
                }

                // Check if this is the 'arguments' identifier
                if self.arguments_capture_level > 0
                    && let Some(text) = self.get_identifier_text_ref(idx)
                    && text == "arguments"
                {
                    self.transforms
                        .insert(idx, TransformDirective::SubstituteArguments);
                }
            }
            _ => self.visit_children(idx),
        }

        self.visit_depth -= 1;
    }

    /// Visit all children of a node
    fn visit_children(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        self.visit(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.get_block_like(node) {
                    let statements = block.statements.nodes.clone();
                    for stmt in statements {
                        self.visit(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    for &decl_list in &var_stmt.declarations.nodes {
                        self.visit(decl_list);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(decl_list) = self.arena.get_variable(node) {
                    if self.ctx.target_es5 {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5VariableDeclarationList { decl_list: idx },
                        );

                        let need_downlevel_read = self.ctx.options.downlevel_iteration
                            && decl_list.declarations.nodes.iter().any(|&decl_idx| {
                                if let Some(decl_node) = self.arena.get(decl_idx) {
                                    if let Some(decl) =
                                        self.arena.get_variable_declaration(decl_node)
                                    {
                                        if decl.initializer.is_none() {
                                            return false;
                                        }

                                        self.arena.get(decl.name).is_some_and(|name_node| {
                                            name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                        })
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            });

                        if need_downlevel_read {
                            self.transforms.helpers_mut().read = true;
                        }
                    }
                    for &decl in &decl_list.declarations.nodes {
                        self.visit(decl);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    self.visit(decl.name);
                    if decl.initializer.is_some() {
                        self.visit(decl.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    self.visit(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(export_assign) = self.arena.get_export_assignment(node) {
                    self.visit(export_assign.expression);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.visit(call.expression);
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            self.visit(arg_idx);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    // If this is an assignment (=) with an array/object literal on the left,
                    // mark as assignment target so we don't treat it as spread-in-array-literal
                    let is_destructuring_assignment = bin.operator_token
                        == tsz_scanner::SyntaxKind::EqualsToken as u16
                        && self.arena.get(bin.left).is_some_and(|n| {
                            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    if is_destructuring_assignment {
                        let prev = self.in_assignment_target;
                        self.in_assignment_target = true;
                        self.visit(bin.left);
                        self.in_assignment_target = prev;
                    } else {
                        self.visit(bin.left);
                    }
                    self.visit(bin.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.visit(access.expression);
                    self.visit(access.name_or_argument);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    self.visit(prop.name);
                    self.visit(prop.initializer);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    if let Some(mods) = &prop.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(prop.name);
                    if prop.initializer.is_some() {
                        self.visit(prop.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    if self.ctx.target_es5
                        && let Some(mods) = &method.modifiers
                        && mods.nodes.iter().any(|&mod_idx| {
                            self.arena
                                .get(mod_idx)
                                .is_some_and(|n| n.kind == SyntaxKind::AsyncKeyword as u16)
                        })
                    {
                        self.mark_async_helpers();
                    }
                    if let Some(mods) = &method.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(method.name);
                    for &param_idx in &method.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if method.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                method.body,
                                Some(&method.parameters),
                            );
                            self.enclosing_function_bodies.push(method.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(method.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    if let Some(mods) = &ctor.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if ctor.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                ctor.body,
                                Some(&ctor.parameters),
                            );
                            self.enclosing_function_bodies.push(ctor.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(ctor.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    if let Some(mods) = &accessor.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(accessor.name);
                    for &param_idx in &accessor.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if accessor.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                accessor.body,
                                Some(&accessor.parameters),
                            );
                            self.enclosing_function_bodies.push(accessor.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(accessor.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if func.body.is_some() {
                        if self.ctx.target_es5 {
                            let cn = self.compute_this_capture_name_with_params(
                                func.body,
                                Some(&func.parameters),
                            );
                            self.enclosing_function_bodies.push(func.body);
                            self.enclosing_capture_names.push(cn);
                        }
                        self.visit(func.body);
                        if self.ctx.target_es5 {
                            self.enclosing_function_bodies.pop();
                            self.enclosing_capture_names.pop();
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class_data) = self.arena.get_class(node) {
                    if self.ctx.target_es5 {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ClassExpression { class_node: idx },
                        );
                        let heritage = self.get_extends_heritage(&class_data.heritage_clauses);
                        self.mark_class_helpers(idx, heritage);
                    }
                    if let Some(mods) = &class_data.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    for &member in &class_data.members.nodes {
                        self.visit(member);
                    }
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if let Some(mods) = &param.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }
                    self.visit(param.name);
                    if param.initializer.is_some() {
                        self.visit(param.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    if self.ctx.target_es5
                        && node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        && pattern.elements.nodes.iter().any(|&elem_idx| {
                            let Some(elem_node) = self.arena.get(elem_idx) else {
                                return false;
                            };
                            self.arena
                                .get_binding_element(elem_node)
                                .is_some_and(|elem| elem.dot_dot_dot_token)
                        })
                    {
                        self.transforms.helpers_mut().rest = true;
                    }
                    for &elem in &pattern.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    if elem.property_name.is_some() {
                        self.visit(elem.property_name);
                    }
                    self.visit(elem.name);
                    if elem.initializer.is_some() {
                        self.visit(elem.initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.visit(computed.expression);
                }
            }
            k if k == syntax_kind_ext::DECORATOR => {
                if let Some(decorator) = self.arena.get_decorator(node) {
                    self.visit(decorator.expression);
                }
                if self.ctx.options.legacy_decorators {
                    self.transforms.helpers_mut().decorate = true;
                }
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if self.ctx.target_es5 {
                    self.transforms.insert(
                        idx,
                        TransformDirective::ES5TemplateLiteral { template_node: idx },
                    );
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if self.ctx.target_es5 {
                    self.transforms.insert(
                        idx,
                        TransformDirective::ES5TemplateLiteral { template_node: idx },
                    );
                    self.transforms.helpers_mut().make_template_object = true;
                }
                if let Some(tagged) = self.arena.get_tagged_template(node) {
                    self.visit(tagged.tag);
                    self.visit(tagged.template);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if self.ctx.target_es5 {
                    self.transforms.insert(
                        idx,
                        TransformDirective::ES5TemplateLiteral { template_node: idx },
                    );
                }
                if let Some(template) = self.arena.get_template_expr(node) {
                    self.visit(template.head);
                    for &span_idx in &template.template_spans.nodes {
                        self.visit(span_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = self.arena.get_template_span(node) {
                    self.visit(span.expression);
                    self.visit(span.literal);
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if let Some(spread) = self.arena.get_spread(node) {
                    self.visit(spread.expression);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    self.visit(paren.expression);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    self.visit(unary.operand);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.visit(cond.condition);
                    self.visit(cond.when_true);
                    self.visit(cond.when_false);
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    // Skip transform if this is the left side of a destructuring assignment
                    if !self.in_assignment_target
                        && self.ctx.target_es5
                        && self.needs_es5_object_literal_transform(&lit.elements.nodes)
                    {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ObjectLiteral {
                                object_literal: idx,
                            },
                        );
                        // Mark __assign helper if object spread is detected
                        if lit
                            .elements
                            .nodes
                            .iter()
                            .any(|&idx| self.is_spread_element(idx))
                        {
                            self.transforms.helpers_mut().assign = true;
                        }
                    }

                    for &elem in &lit.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    // Add ES5ArrayLiteral directive if targeting ES5 and spread elements are present.
                    // Skip if this is the left side of a destructuring assignment
                    // (e.g., [...rest] = arr) since that's not a real array literal.
                    let has_spread = !self.in_assignment_target
                        && self.needs_es5_array_literal_transform(&lit.elements.nodes);
                    if self.ctx.target_es5 && has_spread {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ArrayLiteral { array_literal: idx },
                        );
                        // Flag that __spreadArray helper is needed
                        self.transforms.helpers_mut().spread_array = true;
                    }

                    for &elem in &lit.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.visit(if_stmt.expression);
                    self.visit(if_stmt.then_statement);
                    if if_stmt.else_statement.is_some() {
                        self.visit(if_stmt.else_statement);
                    }
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.visit(loop_data.initializer);
                    self.visit(loop_data.condition);
                    self.visit(loop_data.incrementor);
                    self.visit(loop_data.statement);
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.visit(ret.expression);
                }
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                if let Some(thr) = self.arena.get_return_statement(node)
                    && thr.expression.is_some()
                {
                    self.visit(thr.expression);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch) = self.arena.get_switch(node) {
                    self.visit(switch.expression);
                    self.visit(switch.case_block);
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    if clause.expression.is_some() {
                        self.visit(clause.expression);
                    }
                    for &stmt in &clause.statements.nodes {
                        self.visit(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.visit(try_stmt.try_block);
                    if try_stmt.catch_clause.is_some() {
                        self.visit(try_stmt.catch_clause);
                    }
                    if try_stmt.finally_block.is_some() {
                        self.visit(try_stmt.finally_block);
                    }
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch) = self.arena.get_catch_clause(node) {
                    if catch.variable_declaration.is_some() {
                        self.visit(catch.variable_declaration);
                    }
                    self.visit(catch.block);
                }
            }
            _ => {}
        }
    }

    fn visit_for_in_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };

        self.visit(for_in_of.initializer);
        self.visit(for_in_of.expression);
        self.visit(for_in_of.statement);
    }

    fn visit_for_of_statement(&mut self, node: &Node, idx: NodeIndex) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };
        let should_lower_for_of_sync = self.ctx.target_es5 && !for_in_of.await_modifier;
        let should_lower_for_await_of =
            for_in_of.await_modifier && !self.ctx.options.target.supports_es2018();

        if should_lower_for_of_sync || should_lower_for_await_of {
            self.transforms
                .insert(idx, TransformDirective::ES5ForOf { for_of_node: idx });
            if for_in_of.await_modifier {
                self.transforms.helpers_mut().async_values = true;
            } else if self.ctx.options.downlevel_iteration {
                self.transforms.helpers_mut().values = true;
            }
        }

        // Check if initializer contains destructuring pattern
        // For-of initializer can be VARIABLE_DECLARATION_LIST with binding patterns
        let init_has_binding_pattern =
            self.for_of_initializer_has_binding_pattern(for_in_of.initializer);

        if init_has_binding_pattern {
            // Mark __read helper when destructuring is used with downlevelIteration
            // TypeScript emits __read to convert iterator results to arrays for destructuring
            if self.ctx.target_es5 && self.ctx.options.downlevel_iteration {
                self.transforms.helpers_mut().read = true;
            }
            // Set in_assignment_target to prevent spread in destructuring from triggering __spreadArray
            let prev = self.in_assignment_target;
            self.in_assignment_target = true;
            self.visit(for_in_of.initializer);
            self.in_assignment_target = prev;
        } else {
            self.visit(for_in_of.initializer);
        }
        self.visit(for_in_of.expression);
        self.visit(for_in_of.statement);
    }

    /// Visit a class declaration
    fn visit_class_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_class_declaration(node, idx, false, false);
    }

    fn visit_enum_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_enum_declaration(node, idx, false);
    }

    fn visit_module_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_module_declaration(node, idx, false);
    }

    fn visit_import_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(import_decl) = self.arena.get_import_decl(node) else {
            return;
        };

        // Detect CommonJS helpers needed for imports
        if self.is_commonjs()
            && let Some(clause_node) = self.arena.get(import_decl.import_clause)
            && let Some(clause) = self.arena.get_import_clause(clause_node)
            && !clause.is_type_only
        {
            // Default import: import d from "mod" -> needs __importDefault
            if clause.name.is_some() {
                let helpers = self.transforms.helpers_mut();
                helpers.import_default = true;
            }

            // Namespace import: import * as ns from "mod" -> needs __importStar
            if let Some(bindings_node) = self.arena.get(clause.named_bindings) {
                // NAMESPACE_IMPORT = 275
                if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                    let helpers = self.transforms.helpers_mut();
                    helpers.import_star = true;
                    helpers.create_binding = true; // __importStar depends on __createBinding
                } else if let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                    && named_imports.name.is_some()
                    && named_imports.elements.nodes.is_empty()
                {
                    // "default" import with empty named imports (also needs importStar)
                    let helpers = self.transforms.helpers_mut();
                    helpers.import_star = true;
                    helpers.create_binding = true;
                } else if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                    let has_default_named_import =
                        named_imports.elements.nodes.iter().any(|&spec_idx| {
                            self.arena.get(spec_idx).is_some_and(|spec_node| {
                                self.arena.get_specifier(spec_node).is_some_and(|spec| {
                                    if spec.is_type_only {
                                        return false;
                                    }
                                    let import_name = if spec.property_name.is_some() {
                                        self.arena
                                            .get(spec.property_name)
                                            .and_then(|prop_node| {
                                                self.arena.get_identifier(prop_node)
                                            })
                                            .map(|id| id.escaped_text.as_str())
                                    } else {
                                        self.arena
                                            .get(spec.name)
                                            .and_then(|name_node| {
                                                self.arena.get_identifier(name_node)
                                            })
                                            .map(|id| id.escaped_text.as_str())
                                    };
                                    import_name == Some("default")
                                })
                            })
                        });
                    if has_default_named_import {
                        let helpers = self.transforms.helpers_mut();
                        helpers.import_default = true;
                    }
                }
            }
        }

        // Continue traversal
        if import_decl.import_clause.is_some() {
            self.visit(import_decl.import_clause);
        }
    }

    fn visit_export_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(export_decl) = self.arena.get_export_decl(node) else {
            return;
        };

        // Skip type-only exports
        if export_decl.is_type_only {
            return;
        }

        // Detect CommonJS helpers: export * from "mod"
        if self.is_commonjs()
            && export_decl.module_specifier.is_some()
            && export_decl.export_clause.is_none()
        {
            let helpers = self.transforms.helpers_mut();
            helpers.export_star = true;
            helpers.create_binding = true; // __exportStar depends on __createBinding
        }

        if export_decl.export_clause.is_none() {
            return;
        }

        if export_decl.is_default_export
            && self.is_commonjs()
            && let Some(export_node) = self.arena.get(export_decl.export_clause)
        {
            if export_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func) = self.arena.get_function(export_node)
            {
                let is_anonymous = {
                    let func_name = self.get_identifier_text_ref(func.name).unwrap_or("");
                    func_name == "function" || !Self::is_valid_identifier_name(func_name)
                };
                if is_anonymous {
                    let directive = self.commonjs_default_export_function_directive(
                        export_decl.export_clause,
                        func,
                    );
                    self.transforms.insert(export_decl.export_clause, directive);

                    if let Some(mods) = &func.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }

                    for &param_idx in &func.parameters.nodes {
                        self.visit(param_idx);
                    }

                    if func.body.is_some() {
                        self.visit(func.body);
                    }

                    return;
                }
            }

            if export_node.kind == syntax_kind_ext::CLASS_DECLARATION
                && let Some(class) = self.arena.get_class(export_node)
            {
                let is_anonymous = {
                    let class_name = self.get_identifier_text_ref(class.name).unwrap_or("");
                    !Self::is_valid_identifier_name(class_name)
                };
                if is_anonymous {
                    let directive = if self.ctx.target_es5 {
                        let heritage = self.get_extends_heritage(&class.heritage_clauses);
                        self.mark_class_helpers(export_decl.export_clause, heritage);
                        TransformDirective::CommonJSExportDefaultClassES5 {
                            class_node: export_decl.export_clause,
                        }
                    } else {
                        TransformDirective::CommonJSExportDefaultExpr
                    };
                    self.transforms.insert(export_decl.export_clause, directive);

                    if let Some(mods) = &class.modifiers {
                        for &mod_idx in &mods.nodes {
                            self.visit(mod_idx);
                        }
                    }

                    for &member_idx in &class.members.nodes {
                        self.visit(member_idx);
                    }

                    return;
                }
            }
        }

        if let Some(export_node) = self.arena.get(export_decl.export_clause) {
            if export_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                self.lower_class_declaration(
                    export_node,
                    export_decl.export_clause,
                    true,
                    export_decl.is_default_export,
                );
                return;
            }

            if export_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                self.lower_function_declaration(
                    export_node,
                    export_decl.export_clause,
                    true,
                    export_decl.is_default_export,
                );
                return;
            }

            if export_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                self.lower_variable_statement(export_node, export_decl.export_clause, true);
                return;
            }

            if export_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                self.lower_enum_declaration(export_node, export_decl.export_clause, true);
                return;
            }

            if export_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.lower_module_declaration(export_node, export_decl.export_clause, true);
                return;
            }
        }

        self.visit(export_decl.export_clause);
    }

    fn commonjs_default_export_function_directive(
        &mut self,
        function_node: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> TransformDirective {
        let mut directives = Vec::new();
        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_async_helpers();
                directives.push(TransformDirective::ES5AsyncFunction { function_node });
            } else if self.function_parameters_need_es5_transform(&func.parameters) {
                // Mark rest helper if parameters have rest
                if self.function_parameters_need_rest_helper(&func.parameters) {
                    self.transforms.helpers_mut().rest = true;
                }
                directives.push(TransformDirective::ES5FunctionParameters { function_node });
            }
        }

        directives.push(TransformDirective::CommonJSExportDefaultExpr);

        if directives.len() == 1 {
            directives
                .pop()
                .expect("commonjs default export directive should not be empty")
        } else {
            TransformDirective::Chain(directives)
        }
    }

    fn lower_class_declaration(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        force_export: bool,
        force_default: bool,
    ) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        if let Some(mods) = &class.modifiers {
            for &mod_idx in &mods.nodes {
                self.visit(mod_idx);
            }
        }

        // Skip ambient declarations (declare class)
        if self.has_declare_modifier(&class.modifiers) {
            return;
        }

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export || self.has_export_modifier(&class.modifiers));

        if force_export && self.is_commonjs() && !self.has_export_assignment {
            is_exported = true;
        }

        let is_default = if force_export {
            force_default
        } else {
            self.has_default_modifier(&class.modifiers)
        };

        // Get class name only if we might need it for exports.
        let class_name = if is_exported && class.name.is_some() {
            self.get_identifier_id(class.name)
        } else {
            None
        };

        // Track class name for namespace/class merging detection
        if let Some(name) = self.get_identifier_text_ref(class.name) {
            self.declared_names.insert(name.to_string());
        }

        let heritage = self.get_extends_heritage(&class.heritage_clauses);
        if self.ctx.target_es5 {
            self.mark_class_helpers(idx, heritage);
        }

        // Determine the base transform
        let base_directive = if self.ctx.target_es5 {
            // ES5 class transform
            TransformDirective::ES5Class {
                class_node: idx,
                heritage,
            }
        } else {
            // No transform needed for ES6+ targets
            TransformDirective::Identity
        };

        // Wrap with CommonJS export if needed
        let final_directive = if is_exported {
            if let Some(export_name) = class_name {
                let export_directive = TransformDirective::CommonJSExport {
                    names: Arc::from(vec![export_name]),
                    is_default,
                    inner: Box::new(TransformDirective::Identity),
                };

                match base_directive {
                    TransformDirective::Identity => export_directive,
                    other => TransformDirective::Chain(vec![other, export_directive]),
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        // Only register non-identity transforms
        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        // Save and set current_class_is_derived state for super detection
        let prev_is_derived = self.current_class_is_derived;
        self.current_class_is_derived = heritage.is_some();

        // Generate class alias for static members (e.g., "_a" for "Vector")
        let class_alias = if self.ctx.target_es5 {
            self.get_identifier_text_ref(class.name).map(|name| {
                // Generate a unique alias based on class name
                // For now, use the first letter + underscore pattern
                let first_char = name.chars().next().unwrap_or('_');
                format!("_{}", first_char.to_lowercase().collect::<String>())
            })
        } else {
            None
        };

        // Save previous static context
        let prev_in_static = self.in_static_context;
        let prev_class_alias = self.current_class_alias.take();

        // In ES5 mode, class members are emitted inside a class IIFE.
        // Arrow functions in property initializers/methods should NOT propagate
        // _this capture to the enclosing scope — the class_es5_ir handles
        // _this capture independently within the constructor/method bodies.
        let prev_in_es5_class = self.in_es5_class;
        let prev_capture_level = self.this_capture_level;
        let prev_args_capture_level = self.arguments_capture_level;
        if self.ctx.target_es5 {
            self.in_es5_class = true;
            self.this_capture_level = 0;
            self.arguments_capture_level = 0;
        }

        // Visit children (members) with static context tracking
        for &member_idx in &class.members.nodes {
            // Check if this member is static
            let is_static = self.is_static_member(member_idx);

            if is_static {
                self.in_static_context = true;
                self.current_class_alias = class_alias.clone();
            }

            self.visit(member_idx);

            if is_static {
                self.in_static_context = false;
                self.current_class_alias.take();
            }
        }

        // Restore previous state
        self.current_class_is_derived = prev_is_derived;
        self.in_static_context = prev_in_static;
        self.current_class_alias = prev_class_alias;

        // Restore _this capture state (undo the class barrier)
        if self.ctx.target_es5 {
            self.in_es5_class = prev_in_es5_class;
            self.this_capture_level = prev_capture_level;
            self.arguments_capture_level = prev_args_capture_level;
        }
    }

    fn lower_function_declaration(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        force_export: bool,
        force_default: bool,
    ) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Save and reset in_constructor state for nested function scope
        // Regular functions create a new scope, so in_constructor should be false inside them
        let prev_in_constructor = self.in_constructor;
        self.in_constructor = false;

        if let Some(mods) = &func.modifiers {
            for &mod_idx in &mods.nodes {
                self.visit(mod_idx);
            }
        }

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export || self.has_export_modifier(&func.modifiers));
        if force_export && self.is_commonjs() && !self.has_export_assignment {
            is_exported = true;
        }

        let is_default = if force_export {
            force_default
        } else {
            self.has_default_modifier(&func.modifiers)
        };

        let func_name = if is_exported && func.name.is_some() {
            self.get_identifier_id(func.name)
        } else {
            None
        };

        // Track function name for namespace/function merging detection
        if let Some(name) = self.get_identifier_text_ref(func.name) {
            self.declared_names.insert(name.to_string());
        }

        // Check if this is an async function needing lowering (target < ES2017)
        let base_directive = if self.ctx.needs_async_lowering && self.has_async_modifier(idx) {
            self.mark_async_helpers();
            TransformDirective::ES5AsyncFunction { function_node: idx }
        } else if self.ctx.target_es5
            && self.function_parameters_need_es5_transform(&func.parameters)
        {
            // Mark rest helper if parameters have rest
            if self.function_parameters_need_rest_helper(&func.parameters) {
                self.transforms.helpers_mut().rest = true;
            }
            TransformDirective::ES5FunctionParameters { function_node: idx }
        } else {
            TransformDirective::Identity
        };

        let final_directive = if is_exported {
            if let Some(export_name) = func_name {
                if is_default {
                    // Default exports need explicit exports.default = name;
                    let export_directive = TransformDirective::CommonJSExport {
                        names: Arc::from(vec![export_name]),
                        is_default,
                        inner: Box::new(TransformDirective::Identity),
                    };

                    match base_directive {
                        TransformDirective::Identity => export_directive,
                        other => TransformDirective::Chain(vec![other, export_directive]),
                    }
                } else {
                    // Named function exports: emit exports.f = f; after the declaration
                    let export_directive = TransformDirective::CommonJSExport {
                        names: Arc::from(vec![export_name]),
                        is_default: false,
                        inner: Box::new(TransformDirective::Identity),
                    };

                    match base_directive {
                        TransformDirective::Identity => export_directive,
                        other => TransformDirective::Chain(vec![other, export_directive]),
                    }
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        for &param_idx in &func.parameters.nodes {
            self.visit(param_idx);
        }

        if func.body.is_some() {
            // Track this function body as a potential _this capture scope
            if self.ctx.target_es5 {
                let cn =
                    self.compute_this_capture_name_with_params(func.body, Some(&func.parameters));
                self.enclosing_function_bodies.push(func.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(func.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore in_constructor state
        self.in_constructor = prev_in_constructor;
    }

    fn lower_enum_declaration(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(enum_decl) = self.arena.get_enum(node) else {
            return;
        };

        // Skip ambient and const enums (declare/const enums are erased)
        if self.has_declare_modifier(&enum_decl.modifiers)
            || self.has_const_modifier(&enum_decl.modifiers)
        {
            return;
        }

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export || self.has_export_modifier(&enum_decl.modifiers));
        if force_export && self.is_commonjs() && !self.has_export_assignment {
            is_exported = true;
        }

        let enum_name = if is_exported && enum_decl.name.is_some() {
            self.get_identifier_id(enum_decl.name)
        } else {
            None
        };

        // Track enum name for namespace/enum merging detection
        if let Some(name) = self.get_identifier_text_ref(enum_decl.name) {
            self.declared_names.insert(name.to_string());
        }

        let base_directive = if self.ctx.target_es5 {
            TransformDirective::ES5Enum { enum_node: idx }
        } else {
            TransformDirective::Identity
        };

        let final_directive = if is_exported {
            if let Some(export_name) = enum_name {
                let export_directive = TransformDirective::CommonJSExport {
                    names: Arc::from(vec![export_name]),
                    is_default: false,
                    inner: Box::new(TransformDirective::Identity),
                };

                match base_directive {
                    TransformDirective::Identity => export_directive,
                    other => TransformDirective::Chain(vec![other, export_directive]),
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        for &member_idx in &enum_decl.members.nodes {
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.visit(member.name);
                if member.initializer.is_some() {
                    self.visit(member.initializer);
                }
            }
        }
    }

    fn lower_module_declaration(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(module_decl) = self.arena.get_module(node) else {
            return;
        };

        // Skip ambient declarations (declare namespace/module)
        if self.has_declare_modifier(&module_decl.modifiers) {
            return;
        }

        // Get the namespace root name for merging detection
        let namespace_name = self.get_module_root_name_text(module_decl.name);

        // Check if this name has already been declared (class/enum/function/namespace)
        // If so, we should NOT emit 'var' for this namespace
        let should_declare_var = if let Some(ref name) = namespace_name {
            !self.declared_names.contains(name)
        } else {
            true
        };

        // Track this name as declared
        if let Some(name) = namespace_name {
            self.declared_names.insert(name);
        }

        let mut is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export || self.has_export_modifier(&module_decl.modifiers));
        if force_export && self.is_commonjs() && !self.has_export_assignment {
            is_exported = true;
        }

        let module_name = if is_exported {
            self.get_module_root_name(module_decl.name)
        } else {
            None
        };

        let base_directive = if self.ctx.target_es5 {
            TransformDirective::ES5Namespace {
                namespace_node: idx,
                should_declare_var,
            }
        } else {
            TransformDirective::Identity
        };

        let final_directive = if is_exported {
            if let Some(export_name) = module_name {
                let export_directive = TransformDirective::CommonJSExport {
                    names: Arc::from(vec![export_name]),
                    is_default: false,
                    inner: Box::new(TransformDirective::Identity),
                };

                match base_directive {
                    TransformDirective::Identity => export_directive,
                    other => TransformDirective::Chain(vec![other, export_directive]),
                }
            } else {
                base_directive
            }
        } else {
            base_directive
        };

        if !matches!(final_directive, TransformDirective::Identity) {
            self.transforms.insert(idx, final_directive);
        }

        // Recurse into namespace body to detect helpers needed by nested declarations
        // (e.g., classes with extends need __extends, async functions need __awaiter)
        self.visit_module_body(module_decl.body);
    }

    /// Recursively visit module/namespace body statements to detect helper requirements
    fn visit_module_body(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };

        if let Some(block_data) = self.arena.get_module_block(body_node) {
            if let Some(ref stmts) = block_data.statements {
                for &stmt_idx in &stmts.nodes {
                    self.visit(stmt_idx);
                }
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested namespace: `namespace A.B { ... }` — recurse into inner body
            if let Some(inner_module) = self.arena.get_module(body_node) {
                self.visit_module_body(inner_module.body);
            }
        }
    }

    /// Visit a function declaration
    fn visit_function_declaration(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_function_declaration(node, idx, false, false);
    }

    /// Visit an arrow function
    fn visit_arrow_function(&mut self, node: &Node, idx: NodeIndex) {
        let Some(arrow) = self.arena.get_function(node) else {
            return;
        };

        if self.ctx.target_es5 {
            let malformed_return_type = arrow.type_annotation.is_some()
                && self
                    .arena
                    .get(arrow.type_annotation)
                    .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);

            if self.is_recovery_malformed_arrow(node) || malformed_return_type {
                for &param_idx in &arrow.parameters.nodes {
                    self.visit(param_idx);
                }
                if arrow.body.is_some() {
                    self.visit(arrow.body);
                }
                return;
            }

            let captures_this = contains_this_reference(self.arena, idx);
            let captures_arguments = contains_arguments_reference(self.arena, idx);

            tracing::debug!(
                "[lowering][arrow] idx={} captures_this={captures_this} is_async={}",
                idx.0,
                arrow.is_async
            );

            // For static members, use class alias capture instead of IIFE
            let class_alias = if self.in_static_context && captures_this {
                self.current_class_alias.clone()
            } else {
                None
            };

            self.transforms.insert(
                idx,
                TransformDirective::ES5ArrowFunction {
                    arrow_node: idx,
                    captures_this,
                    captures_arguments,
                    class_alias: class_alias.map(std::convert::Into::into),
                },
            );

            if arrow.is_async {
                self.mark_async_helpers();
            }

            // If this arrow function captures 'this', increment the capture level
            // so that nested 'this' references get substituted.
            // Also mark the enclosing function body so the emitter inserts
            // `var _this = this;` at the start of that scope.
            // But NOT when inside an ES5 class — class_es5_ir handles _this
            // capture independently within constructor/method bodies.
            if captures_this {
                self.this_capture_level += 1;
                if !self.in_es5_class
                    && let Some(&enclosing_body) = self.enclosing_function_bodies.last()
                {
                    let capture_name = self
                        .enclosing_capture_names
                        .last()
                        .cloned()
                        .unwrap_or_else(|| Arc::from("_this"));
                    self.transforms
                        .mark_this_capture_scope(enclosing_body, capture_name);
                }
            }

            // If this arrow function captures 'arguments', increment the capture level
            // so that nested 'arguments' references get substituted
            if captures_arguments {
                self.arguments_capture_level += 1;
            }
        } else if self.ctx.needs_async_lowering && arrow.is_async {
            // ES2015/ES2016: arrow syntax is native but async needs lowering
            self.mark_async_helpers();
        }

        for &param_idx in &arrow.parameters.nodes {
            self.visit(param_idx);
        }

        if arrow.body.is_some() {
            self.visit(arrow.body);
        }

        // Restore capture level after visiting the arrow function body
        if self.ctx.target_es5 {
            let captures_this = contains_this_reference(self.arena, idx);
            if captures_this {
                self.this_capture_level -= 1;
            }

            let captures_arguments = contains_arguments_reference(self.arena, idx);
            if captures_arguments {
                self.arguments_capture_level -= 1;
            }
        }
    }

    fn is_recovery_malformed_arrow(&self, node: &Node) -> bool {
        let start = node.pos as usize;
        let end = node.end as usize;

        self.arena.source_files.iter().any(|sf| {
            if start < sf.text.len() && start < end {
                let window_start = start.saturating_sub(8);
                let window_end = (end + 8).min(sf.text.len());
                let slice = &sf.text[window_start..window_end];
                slice.contains("): =>") || slice.contains("):=>")
            } else {
                false
            }
        })
    }

    /// Visit a constructor declaration
    fn visit_constructor(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(ctor) = self.arena.get_constructor(node) else {
            return;
        };

        // Save previous state
        let prev_in_constructor = self.in_constructor;
        // Set new state - we're now inside a constructor
        self.in_constructor = true;

        // Visit children (modifiers, parameters, body)
        if let Some(mods) = &ctor.modifiers {
            for &mod_idx in &mods.nodes {
                self.visit(mod_idx);
            }
        }
        for &param_idx in &ctor.parameters.nodes {
            self.visit(param_idx);
        }
        if ctor.body.is_some() {
            if self.ctx.target_es5 {
                let cn = self.compute_this_capture_name(ctor.body);
                self.enclosing_function_bodies.push(ctor.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(ctor.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore state
        self.in_constructor = prev_in_constructor;
    }

    /// Visit a call expression and detect `super()` calls
    fn visit_call_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        // Check if this is a super() call
        let is_super_call = if let Some(expr_node) = self.arena.get(call.expression) {
            expr_node.kind == SyntaxKind::SuperKeyword as u16
        } else {
            false
        };

        // Emit directive if conditions met:
        // 1. This is a super(...) call
        // 2. Target is ES5
        // 3. We're inside a constructor
        // 4. The current class has a base class (is_derived)
        if is_super_call
            && self.ctx.target_es5
            && self.in_constructor
            && self.current_class_is_derived
        {
            self.transforms
                .insert(idx, TransformDirective::ES5SuperCall);
        }

        // Check if call has spread arguments and needs ES5 transformation
        if self.ctx.target_es5
            && !is_super_call
            && let Some(ref args) = call.arguments
        {
            let has_spread = args
                .nodes
                .iter()
                .any(|&arg_idx| self.is_spread_element(arg_idx));
            if has_spread {
                self.transforms
                    .insert(idx, TransformDirective::ES5CallSpread { call_expr: idx });
                // __spreadArray is only needed when spread arguments must be merged
                // with additional segments (not for plain foo(...args)).
                if self.call_spread_needs_spread_array(args.nodes.as_slice()) {
                    self.transforms.helpers_mut().spread_array = true;
                }
            }
        }

        // Continue traversal
        self.visit(call.expression);
        if let Some(ref args) = call.arguments {
            for &arg_idx in &args.nodes {
                self.visit(arg_idx);
            }
        }
    }

    /// Visit a new expression and traverse callee + arguments for nested transforms.
    fn visit_new_expression(&mut self, node: &Node) {
        let Some(new_expr) = self.arena.get_call_expr(node) else {
            return;
        };

        self.visit(new_expr.expression);
        if let Some(ref args) = new_expr.arguments {
            for &arg_idx in &args.nodes {
                self.visit(arg_idx);
            }
        }
    }

    /// Visit a variable statement
    fn visit_variable_statement(&mut self, node: &Node, idx: NodeIndex) {
        self.lower_variable_statement(node, idx, false);
    }

    fn lower_variable_statement(&mut self, node: &Node, idx: NodeIndex, force_export: bool) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };

        let is_exported = self.is_commonjs()
            && !self.has_export_assignment
            && (force_export || self.has_export_modifier(&var_stmt.modifiers));

        if is_exported {
            let export_names = self.collect_variable_names(&var_stmt.declarations);
            if !export_names.is_empty() {
                self.transforms.insert(
                    idx,
                    TransformDirective::CommonJSExport {
                        names: Arc::from(export_names),
                        is_default: false,
                        inner: Box::new(TransformDirective::Identity),
                    },
                );
            }
        }

        // Visit each declaration
        for &decl in &var_stmt.declarations.nodes {
            self.visit(decl);
        }
    }

    fn visit_function_expression(&mut self, node: &Node, idx: NodeIndex) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Save and reset in_constructor state for nested function scope
        let prev_in_constructor = self.in_constructor;
        self.in_constructor = false;

        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_async_helpers();
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5AsyncFunction { function_node: idx },
                );
            } else if self.function_parameters_need_es5_transform(&func.parameters) {
                // Mark rest helper if parameters have rest
                if self.function_parameters_need_rest_helper(&func.parameters) {
                    self.transforms.helpers_mut().rest = true;
                }
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5FunctionParameters { function_node: idx },
                );
            }
        }

        for &param_idx in &func.parameters.nodes {
            self.visit(param_idx);
        }

        if func.body.is_some() {
            // Track this function body as a potential _this capture scope
            if self.ctx.target_es5 {
                let cn =
                    self.compute_this_capture_name_with_params(func.body, Some(&func.parameters));
                self.enclosing_function_bodies.push(func.body);
                self.enclosing_capture_names.push(cn);
            }
            self.visit(func.body);
            if self.ctx.target_es5 {
                self.enclosing_function_bodies.pop();
                self.enclosing_capture_names.pop();
            }
        }

        // Restore in_constructor state
        self.in_constructor = prev_in_constructor;
    }
}

#[cfg(test)]
#[path = "../tests/lowering_pass.rs"]
mod tests;
