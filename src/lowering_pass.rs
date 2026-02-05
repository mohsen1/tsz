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
//! When `target: ES5`, a ClassDeclaration needs transformation:
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
//! ### CommonJS Export
//!
//! When `module: CommonJS`, exported declarations need wrapping:
//!
//! ```typescript
//! export class Foo {}
//! ```
//!
//! The lowering pass creates a `TransformDirective::CommonJSExport` that
//! chains with any other transforms (like ES5Class).

use crate::common::ModuleKind;
use crate::emit_context::EmitContext;
use crate::parser::node::{Node, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::syntax::transform_utils::{contains_this_reference, is_private_identifier};
use crate::transform_context::{IdentifierId, ModuleFormat, TransformContext, TransformDirective};
use std::sync::Arc;

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
        }
    }

    /// Run the lowering pass on a source file and return the transform context
    pub fn run(mut self, source_file: NodeIndex) -> TransformContext {
        self.init_module_state(source_file);
        self.visit(source_file);
        self.maybe_wrap_module(source_file);
        self.transforms.mark_helpers_populated();
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
                self.visit_function_declaration(node, idx)
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.visit_function_expression(node, idx)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.visit_arrow_function(node, idx),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.visit_variable_statement(node, idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => self.visit_enum_declaration(node, idx),
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.visit_module_declaration(node, idx)
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.visit_export_declaration(node, idx)
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => self.visit_for_in_statement(node),
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => self.visit_for_of_statement(node, idx),
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
                    }
                    for &decl in &decl_list.declarations.nodes {
                        self.visit(decl);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    self.visit(decl.name);
                    if !decl.initializer.is_none() {
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
                    self.visit(bin.left);
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
                    if !prop.initializer.is_none() {
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
                                .map(|n| n.kind == SyntaxKind::AsyncKeyword as u16)
                                .unwrap_or(false)
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
                    if !method.body.is_none() {
                        self.visit(method.body);
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
                    if !ctor.body.is_none() {
                        self.visit(ctor.body);
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
                    if !accessor.body.is_none() {
                        self.visit(accessor.body);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        self.visit(param_idx);
                    }
                    if !func.body.is_none() {
                        self.visit(func.body);
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
                    if !param.initializer.is_none() {
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
                                .map(|elem| elem.dot_dot_dot_token)
                                .unwrap_or(false)
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
                    if !elem.property_name.is_none() {
                        self.visit(elem.property_name);
                    }
                    self.visit(elem.name);
                    if !elem.initializer.is_none() {
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
                // When targeting ES5, decorators need the __decorate helper
                if self.ctx.target_es5 {
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
                    if self.ctx.target_es5
                        && self.needs_es5_object_literal_transform(&lit.elements.nodes)
                    {
                        self.transforms.insert(
                            idx,
                            TransformDirective::ES5ObjectLiteral {
                                object_literal: idx,
                            },
                        );
                    }

                    for &elem in &lit.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    for &elem in &lit.elements.nodes {
                        self.visit(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.visit(if_stmt.expression);
                    self.visit(if_stmt.then_statement);
                    if !if_stmt.else_statement.is_none() {
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
                    && !ret.expression.is_none()
                {
                    self.visit(ret.expression);
                }
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                if let Some(thr) = self.arena.get_return_statement(node)
                    && !thr.expression.is_none()
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
                    if !clause.expression.is_none() {
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
                    if !try_stmt.catch_clause.is_none() {
                        self.visit(try_stmt.catch_clause);
                    }
                    if !try_stmt.finally_block.is_none() {
                        self.visit(try_stmt.finally_block);
                    }
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch) = self.arena.get_catch_clause(node) {
                    if !catch.variable_declaration.is_none() {
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

        if self.ctx.target_es5 && !for_in_of.await_modifier {
            self.transforms
                .insert(idx, TransformDirective::ES5ForOf { for_of_node: idx });
            // Note: simple array-indexing pattern doesn't need __values helper
            // __values is only needed with --downlevelIteration
        }

        self.visit(for_in_of.initializer);
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

    fn visit_export_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(export_decl) = self.arena.get_export_decl(node) else {
            return;
        };

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

                    if !func.body.is_none() {
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
        func: &crate::parser::node::FunctionData,
    ) -> TransformDirective {
        let mut directives = Vec::new();
        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_async_helpers();
                directives.push(TransformDirective::ES5AsyncFunction { function_node });
            } else if self.function_parameters_need_es5_transform(&func.parameters) {
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
        let class_name = if is_exported && !class.name.is_none() {
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

        // Visit children (members)
        for &member_idx in &class.members.nodes {
            self.visit(member_idx);
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

        let func_name = if is_exported && !func.name.is_none() {
            self.get_identifier_id(func.name)
        } else {
            None
        };

        // Track function name for namespace/function merging detection
        if let Some(name) = self.get_identifier_text_ref(func.name) {
            self.declared_names.insert(name.to_string());
        }

        // Check if this is an async function targeting ES5
        let base_directive = if self.ctx.target_es5 && self.has_async_modifier(idx) {
            self.mark_async_helpers();
            TransformDirective::ES5AsyncFunction { function_node: idx }
        } else if self.ctx.target_es5
            && self.function_parameters_need_es5_transform(&func.parameters)
        {
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

        if !func.body.is_none() {
            self.visit(func.body);
        }
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

        let enum_name = if is_exported && !enum_decl.name.is_none() {
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
                if !member.initializer.is_none() {
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
            let captures_this = contains_this_reference(self.arena, idx);

            self.transforms.insert(
                idx,
                TransformDirective::ES5ArrowFunction {
                    arrow_node: idx,
                    captures_this,
                },
            );

            if arrow.is_async {
                self.mark_async_helpers();
            }
        }

        for &param_idx in &arrow.parameters.nodes {
            self.visit(param_idx);
        }

        if !arrow.body.is_none() {
            self.visit(arrow.body);
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

        if self.ctx.target_es5 {
            if func.is_async {
                self.mark_async_helpers();
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5AsyncFunction { function_node: idx },
                );
            } else if self.function_parameters_need_es5_transform(&func.parameters) {
                self.transforms.insert(
                    idx,
                    TransformDirective::ES5FunctionParameters { function_node: idx },
                );
            }
        }

        for &param_idx in &func.parameters.nodes {
            self.visit(param_idx);
        }

        if !func.body.is_none() {
            self.visit(func.body);
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    fn init_module_state(&mut self, source_file: NodeIndex) {
        let Some(node) = self.arena.get(source_file) else {
            return;
        };
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        self.has_export_assignment = self.contains_export_assignment(&source.statements);
        self.commonjs_mode = if self.ctx.is_commonjs() {
            true
        } else if self.ctx.auto_detect_module && matches!(self.ctx.options.module, ModuleKind::None)
        {
            self.file_is_module(&source.statements)
        } else {
            false
        };
    }

    fn is_commonjs(&self) -> bool {
        self.commonjs_mode
    }

    /// Check if a modifier list contains the 'declare' keyword
    fn has_declare_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .map(|n| n.kind == SyntaxKind::DeclareKeyword as u16)
                .unwrap_or(false)
        })
    }

    /// Check if a modifier list contains the 'const' keyword
    fn has_const_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .map(|n| n.kind == SyntaxKind::ConstKeyword as u16)
                .unwrap_or(false)
        })
    }

    /// Check if a modifier list contains the 'export' keyword
    fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .map(|n| n.kind == SyntaxKind::ExportKeyword as u16)
                .unwrap_or(false)
        })
    }

    /// Check if a modifier list contains the 'default' keyword
    fn has_default_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .map(|n| n.kind == SyntaxKind::DefaultKeyword as u16)
                .unwrap_or(false)
        })
    }

    fn get_extends_heritage(&self, heritage_clauses: &Option<NodeList>) -> Option<NodeIndex> {
        let clauses = heritage_clauses.as_ref()?;

        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage(clause_node)?;
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return Some(clause_idx);
            }
        }

        None
    }

    /// Check if a function has the 'async' modifier
    fn has_async_modifier(&self, func_idx: NodeIndex) -> bool {
        let Some(func_node) = self.arena.get(func_idx) else {
            return false;
        };

        let Some(func) = self.arena.get_function(func_node) else {
            return false;
        };

        if func.is_async {
            return true;
        }

        let Some(mods) = &func.modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .map(|n| n.kind == SyntaxKind::AsyncKeyword as u16)
                .unwrap_or(false)
        })
    }

    fn mark_async_helpers(&mut self) {
        let helpers = self.transforms.helpers_mut();
        helpers.awaiter = true;
        helpers.generator = true;
    }

    fn mark_class_helpers(&mut self, class_node: NodeIndex, heritage: Option<NodeIndex>) {
        if heritage.is_some() {
            self.transforms.helpers_mut().extends = true;
        }

        let Some(class_node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        if self.class_has_private_members(class_data) {
            let helpers = self.transforms.helpers_mut();
            helpers.class_private_field_get = true;
            helpers.class_private_field_set = true;
        }
    }

    fn class_has_private_members(&self, class_data: &crate::parser::node::ClassData) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(member_node)
                        && is_private_identifier(self.arena, prop.name)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(member_node)
                        && is_private_identifier(self.arena, method.name)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.arena.get_accessor(member_node)
                        && is_private_identifier(self.arena, accessor.name)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    fn needs_es5_object_literal_transform(&self, elements: &[NodeIndex]) -> bool {
        elements.iter().any(|&idx| {
            if self.is_computed_property_member(idx) || self.is_spread_element(idx) {
                return true;
            }

            let Some(node) = self.arena.get(idx) else {
                return false;
            };

            node.kind == syntax_kind_ext::METHOD_DECLARATION
                || node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
        })
    }

    fn function_parameters_need_es5_transform(&self, params: &NodeList) -> bool {
        params.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            param.dot_dot_dot_token
                || !param.initializer.is_none()
                || self.is_binding_pattern_idx(param.name)
        })
    }

    fn is_binding_pattern_idx(&self, idx: NodeIndex) -> bool {
        self.arena
            .get(idx)
            .map(|node| {
                node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            })
            .unwrap_or(false)
    }

    fn is_computed_property_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        let name_idx = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.arena.get_property_assignment(node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(node).map(|a| a.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        }

        false
    }

    fn is_spread_element(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            || node.kind == syntax_kind_ext::SPREAD_ELEMENT
    }

    fn get_identifier_id(&self, idx: NodeIndex) -> Option<IdentifierId> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        Some(node.data_index)
    }

    fn get_identifier_text_ref(&self, idx: NodeIndex) -> Option<&str> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(node)?;
        Some(&ident.escaped_text)
    }

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

    fn get_module_root_name(&self, name_idx: NodeIndex) -> Option<IdentifierId> {
        self.get_module_root_name_inner(name_idx, 0)
    }

    fn get_module_root_name_inner(&self, name_idx: NodeIndex, depth: u32) -> Option<IdentifierId> {
        // Stack overflow protection for qualified names
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return None;
        }

        if name_idx.is_none() {
            return None;
        }

        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(node.data_index);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.qualified_names.get(node.data_index as usize)
        {
            return self.get_module_root_name_inner(qn.left, depth + 1);
        }

        None
    }

    /// Get the root name of a module as a String for merging detection
    fn get_module_root_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let id = self.get_module_root_name(name_idx)?;
        let ident = self.arena.identifiers.get(id as usize)?;
        Some(ident.escaped_text.clone())
    }

    fn get_block_like(&self, node: &Node) -> Option<&crate::parser::node::BlockData> {
        if node.kind == syntax_kind_ext::BLOCK || node.kind == syntax_kind_ext::CASE_BLOCK {
            self.arena.blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    fn collect_variable_names(&self, declarations: &NodeList) -> Vec<IdentifierId> {
        let mut names = Vec::new();
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_names(decl.name, &mut names);
            }
        }
        names
    }

    fn collect_binding_names(&self, name_idx: NodeIndex, names: &mut Vec<IdentifierId>) {
        self.collect_binding_names_inner(name_idx, names, 0);
    }

    fn collect_binding_names_inner(
        &self,
        name_idx: NodeIndex,
        names: &mut Vec<IdentifierId>,
        depth: u32,
    ) {
        // Stack overflow protection for deeply nested binding patterns
        if depth >= MAX_BINDING_PATTERN_DEPTH {
            return;
        }

        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            names.push(node.data_index);
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_names_from_element_inner(elem_idx, names, depth + 1);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    self.collect_binding_names_inner(elem.name, names, depth + 1);
                }
            }
            _ => {}
        }
    }

    fn collect_binding_names_from_element_inner(
        &self,
        elem_idx: NodeIndex,
        names: &mut Vec<IdentifierId>,
        depth: u32,
    ) {
        // Stack overflow protection
        if depth >= MAX_BINDING_PATTERN_DEPTH {
            return;
        }

        if elem_idx.is_none() {
            return;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(elem) = self.arena.get_binding_element(elem_node) {
            self.collect_binding_names_inner(elem.name, names, depth + 1);
        }
    }

    fn maybe_wrap_module(&mut self, source_file: NodeIndex) {
        let format = match self.ctx.options.module {
            ModuleKind::AMD => ModuleFormat::AMD,
            ModuleKind::System => ModuleFormat::System,
            ModuleKind::UMD => ModuleFormat::UMD,
            _ => return,
        };

        let Some(node) = self.arena.get(source_file) else {
            return;
        };
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        if !self.file_is_module(&source.statements) {
            return;
        }

        let dependencies = Arc::from(self.collect_module_dependencies(&source.statements.nodes));
        self.transforms.insert(
            source_file,
            TransformDirective::ModuleWrapper {
                format,
                dependencies,
            },
        );
    }

    fn file_is_module(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                    {
                        if let Some(import_decl) = self.arena.get_import_decl(node)
                            && self.import_has_runtime_dependency(import_decl)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        if let Some(export_decl) = self.arena.get_export_decl(node)
                            && self.export_decl_has_runtime_value(export_decl)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node)
                            && self.has_export_modifier(&var_stmt.modifiers)
                            && !self.has_declare_modifier(&var_stmt.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node)
                            && self.has_export_modifier(&func.modifiers)
                            && !self.has_declare_modifier(&func.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node)
                            && self.has_export_modifier(&class.modifiers)
                            && !self.has_declare_modifier(&class.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node)
                            && self.has_export_modifier(&enum_decl.modifiers)
                            && !self.has_declare_modifier(&enum_decl.modifiers)
                            && !self.has_const_modifier(&enum_decl.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node)
                            && self.has_export_modifier(&module.modifiers)
                            && !self.has_declare_modifier(&module.modifiers)
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    fn contains_export_assignment(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            {
                return true;
            }
        }
        false
    }

    fn collect_module_dependencies(&self, statements: &[NodeIndex]) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_has_runtime_dependency(import_decl) {
                        continue;
                    }
                    if let Some(text) = self.get_module_specifier_text(import_decl.module_specifier)
                        && !deps.contains(&text)
                    {
                        deps.push(text);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(node)
            {
                if !self.export_has_runtime_dependency(export_decl) {
                    continue;
                }
                if let Some(text) = self.get_module_specifier_text(export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
            }
        }

        deps
    }

    fn import_has_runtime_dependency(
        &self,
        import_decl: &crate::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            return self.import_equals_has_external_module(import_decl.module_specifier);
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };

        if clause.is_type_only {
            return false;
        }

        if !clause.name.is_none() {
            return true;
        }

        if clause.named_bindings.is_none() {
            return false;
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };

        let Some(named) = self.arena.get_named_imports(bindings_node) else {
            return true;
        };

        if !named.name.is_none() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    fn import_equals_has_external_module(&self, module_specifier: NodeIndex) -> bool {
        if module_specifier.is_none() {
            return false;
        }

        let Some(node) = self.arena.get(module_specifier) else {
            return false;
        };

        node.kind == SyntaxKind::StringLiteral as u16
    }

    fn export_decl_has_runtime_value(
        &self,
        export_decl: &crate::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.is_default_export {
            return true;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return false;
        };

        if let Some(named) = self.arena.get_named_imports(clause_node) {
            if !named.name.is_none() {
                return true;
            }

            if named.elements.nodes.is_empty() {
                return true;
            }

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                if let Some(spec) = self.arena.get_specifier(spec_node)
                    && !spec.is_type_only
                {
                    return true;
                }
            }

            return false;
        }

        if self.export_clause_is_type_only(clause_node) {
            return false;
        }

        true
    }

    fn export_clause_is_type_only(&self, clause_node: &Node) -> bool {
        match clause_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => true,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let Some(enum_decl) = self.arena.get_enum(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&enum_decl.modifiers)
                    || self.has_const_modifier(&enum_decl.modifiers)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let Some(class_decl) = self.arena.get_class(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&class_decl.modifiers)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let Some(func_decl) = self.arena.get_function(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&func_decl.modifiers)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_decl) = self.arena.get_variable(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&var_decl.modifiers)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let Some(module_decl) = self.arena.get_module(clause_node) else {
                    return false;
                };
                self.has_declare_modifier(&module_decl.modifiers)
            }
            _ => false,
        }
    }

    fn export_has_runtime_dependency(
        &self,
        export_decl: &crate::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.module_specifier.is_none() {
            return false;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return true;
        };

        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return true;
        };

        if !named.name.is_none() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    fn get_module_specifier_text(&self, specifier: NodeIndex) -> Option<String> {
        if specifier.is_none() {
            return None;
        }

        let Some(node) = self.arena.get(specifier) else {
            return None;
        };
        let Some(literal) = self.arena.get_literal(node) else {
            return None;
        };

        Some(literal.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::parser::node::NodeArena;

    fn parse(source: &str) -> (NodeArena, NodeIndex) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        (parser.arena, root)
    }

    #[test]
    fn test_lowering_pass_es6_no_transforms() {
        let (arena, root) = parse("class Foo {}");
        let ctx = EmitContext::default();
        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        // ES6 target should not add transforms for classes
        assert!(transforms.is_empty());
    }

    #[test]
    fn test_lowering_pass_es5_class() {
        let (arena, root) = parse("class Foo { constructor(x) { this.x = x; } }");
        let mut ctx = EmitContext::default();
        ctx.target_es5 = true;

        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        // ES5 target should add ES5Class transform
        // The actual class node index depends on parser implementation
        // This test validates the architecture, not specific indices
        assert!(!transforms.is_empty(), "Expected ES5 class transform");
    }

    #[test]
    fn test_lowering_pass_commonjs_export() {
        let (arena, root) = parse("export class Foo {}");
        let mut ctx = EmitContext::default();
        ctx.options.module = crate::emitter::ModuleKind::CommonJS;

        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        // CommonJS module should add export transform
        assert!(!transforms.is_empty(), "Expected CommonJS export transform");
    }

    #[test]
    fn test_lowering_pass_commonjs_export_vars() {
        let (arena, root) = parse("export const a = 1, b = 2;");
        let mut ctx = EmitContext::default();
        ctx.options.module = crate::emitter::ModuleKind::CommonJS;

        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        assert!(
            !transforms.is_empty(),
            "Expected CommonJS export transform for variables"
        );
    }

    #[test]
    fn test_lowering_pass_commonjs_export_name_indices() {
        let (arena, root) = parse("export const x = 1;");
        let mut ctx = EmitContext::default();
        ctx.options.module = crate::emitter::ModuleKind::CommonJS;

        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        let root_node = arena.get(root).expect("expected source file node");
        let source = arena
            .get_source_file(root_node)
            .expect("expected source file data");
        let stmt_idx = *source.statements.nodes.first().expect("expected statement");
        let stmt_node = arena.get(stmt_idx).expect("expected statement node");
        let var_stmt_idx = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export_decl = arena
                .get_export_decl(stmt_node)
                .expect("expected export declaration");
            export_decl.export_clause
        } else {
            stmt_idx
        };
        assert!(!var_stmt_idx.is_none(), "expected variable statement node");

        let directive = transforms
            .get(var_stmt_idx)
            .expect("expected CommonJS export directive");
        match directive {
            TransformDirective::CommonJSExport { names, .. } => {
                assert_eq!(names.len(), 1, "Expected single exported name");
                let ident = arena
                    .identifiers
                    .get(names[0] as usize)
                    .expect("expected exported identifier");
                assert_eq!(ident.escaped_text, "x");
            }
            _ => panic!("Expected CommonJSExport directive"),
        }
    }

    #[test]
    fn test_lowering_pass_commonjs_non_export_function_no_transforms() {
        let (arena, root) = parse("function foo() {}");
        let mut ctx = EmitContext::default();
        ctx.options.module = crate::emitter::ModuleKind::CommonJS;

        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        assert!(
            transforms.is_empty(),
            "Non-exported functions should not add CommonJS transforms"
        );
    }

    #[test]
    fn test_lowering_pass_nested_arrow_in_class() {
        let (arena, root) = parse("class C { m() { const f = () => this; } }");
        let mut ctx = EmitContext::default();
        ctx.target_es5 = true;

        let lowering = LoweringPass::new(&arena, &ctx);
        let transforms = lowering.run(root);

        assert!(
            transforms.len() >= 2,
            "Expected transforms for class and nested arrow function"
        );
    }
}
