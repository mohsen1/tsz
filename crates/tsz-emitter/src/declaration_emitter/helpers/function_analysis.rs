//! Function body analysis, typeof helpers, and literal formatting

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

use super::JsDefinedPropertyDecl;

#[derive(Default)]
struct BooleanReturnSummary {
    has_true: bool,
    has_false: bool,
    has_undefined: bool,
    has_other: bool,
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn conditional_boolean_undefined_default_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<&'static str> {
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(initializer);
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return None;
        }
        let conditional = self.arena.get_conditional_expr(init_node)?;
        let true_branch = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(conditional.when_true);
        let false_branch = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(conditional.when_false);
        let true_node = self.arena.get(true_branch)?;
        let false_node = self.arena.get(false_branch)?;

        let has_boolean_literal = true_node.kind == SyntaxKind::TrueKeyword as u16
            || true_node.kind == SyntaxKind::FalseKeyword as u16
            || false_node.kind == SyntaxKind::TrueKeyword as u16
            || false_node.kind == SyntaxKind::FalseKeyword as u16;
        let has_undefined = true_node.kind == SyntaxKind::UndefinedKeyword as u16
            || false_node.kind == SyntaxKind::UndefinedKeyword as u16
            || self.get_identifier_text(true_branch).as_deref() == Some("undefined")
            || self.get_identifier_text(false_branch).as_deref() == Some("undefined");

        (has_boolean_literal && has_undefined).then_some("boolean | undefined")
    }

    pub(in crate::declaration_emitter) fn boolean_default_param_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        if func.type_annotation.is_some() || !func.body.is_some() {
            return None;
        }

        let mut defaulted_params = FxHashSet::default();
        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if param.initializer.is_some()
                && let Some(name) = self.get_identifier_text(param.name)
            {
                defaulted_params.insert(name);
            }
        }
        if defaulted_params.is_empty() {
            return None;
        }

        let body_node = self.arena.get(func.body)?;
        let block = self.arena.get_block(body_node)?;
        let mut summary = BooleanReturnSummary::default();
        let mut false_narrowed = FxHashSet::default();
        let definitely_returns = self.collect_boolean_default_returns_from_block(
            &block.statements,
            &defaulted_params,
            &mut false_narrowed,
            &mut summary,
        );
        if !definitely_returns {
            summary.has_undefined = true;
        }

        if summary.has_other {
            return None;
        }
        match (summary.has_true, summary.has_false, summary.has_undefined) {
            (true, true, false) => Some("boolean".to_string()),
            (false, true, true) => Some("false | undefined".to_string()),
            _ => None,
        }
    }

    fn collect_boolean_default_returns_from_block(
        &self,
        statements: &NodeList,
        defaulted_params: &FxHashSet<String>,
        false_narrowed: &mut FxHashSet<String>,
        summary: &mut BooleanReturnSummary,
    ) -> bool {
        for stmt_idx in statements.nodes.iter().copied() {
            if self.collect_boolean_default_returns_from_statement(
                stmt_idx,
                defaulted_params,
                false_narrowed,
                summary,
            ) {
                return true;
            }
        }
        false
    }

    fn collect_boolean_default_returns_from_statement(
        &self,
        stmt_idx: NodeIndex,
        defaulted_params: &FxHashSet<String>,
        false_narrowed: &mut FxHashSet<String>,
        summary: &mut BooleanReturnSummary,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return true;
                };
                if !ret.expression.is_some() {
                    summary.has_undefined = true;
                    return true;
                }
                self.collect_boolean_return_expression(
                    ret.expression,
                    defaulted_params,
                    false_narrowed,
                    summary,
                );
                true
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_boolean_default_returns_from_block(
                        &block.statements,
                        defaulted_params,
                        false_narrowed,
                        summary,
                    )
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.arena.get_if_statement(stmt_node) else {
                    return false;
                };
                let narrowed_name =
                    self.false_equality_default_param_name(if_data.expression, defaulted_params);
                if let Some(name) = narrowed_name.as_ref() {
                    false_narrowed.insert(name.clone());
                }
                let then_returns = self.collect_boolean_default_returns_from_statement(
                    if_data.then_statement,
                    defaulted_params,
                    false_narrowed,
                    summary,
                );
                if let Some(name) = narrowed_name.as_ref() {
                    false_narrowed.remove(name);
                }

                if if_data.else_statement.is_some() {
                    let else_returns = self.collect_boolean_default_returns_from_statement(
                        if_data.else_statement,
                        defaulted_params,
                        false_narrowed,
                        summary,
                    );
                    then_returns && else_returns
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn collect_boolean_return_expression(
        &self,
        expr_idx: NodeIndex,
        defaulted_params: &FxHashSet<String>,
        false_narrowed: &FxHashSet<String>,
        summary: &mut BooleanReturnSummary,
    ) {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            summary.has_other = true;
            return;
        };
        if expr_node.kind == SyntaxKind::TrueKeyword as u16 {
            summary.has_true = true;
        } else if expr_node.kind == SyntaxKind::FalseKeyword as u16 {
            summary.has_false = true;
        } else if expr_node.kind == SyntaxKind::Identifier as u16
            && let Some(name) = self.get_identifier_text(expr_idx)
            && defaulted_params.contains(&name)
            && false_narrowed.contains(&name)
        {
            summary.has_false = true;
        } else {
            summary.has_other = true;
        }
    }

    fn false_equality_default_param_name(
        &self,
        expr_idx: NodeIndex,
        defaulted_params: &FxHashSet<String>,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsEqualsEqualsToken as u16 {
            return None;
        }
        self.false_equality_side(binary.left, binary.right, defaulted_params)
            .or_else(|| self.false_equality_side(binary.right, binary.left, defaulted_params))
    }

    fn false_equality_side(
        &self,
        name_idx: NodeIndex,
        false_idx: NodeIndex,
        defaulted_params: &FxHashSet<String>,
    ) -> Option<String> {
        let false_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(false_idx);
        let false_node = self.arena.get(false_idx)?;
        if false_node.kind != SyntaxKind::FalseKeyword as u16 {
            return None;
        }
        let name_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(name_idx);
        let name = self.get_identifier_text(name_idx)?;
        defaulted_params.contains(&name).then_some(name)
    }

    pub(in crate::declaration_emitter) fn js_returned_define_property_function_info(
        &self,
        body_idx: NodeIndex,
    ) -> Option<(NodeIndex, Vec<JsDefinedPropertyDecl>)> {
        if !self.source_is_js_file {
            return None;
        }

        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        let returned_name = self.get_identifier_text(returned_identifier)?;

        let mut initializer = None;
        let mut properties = Vec::new();

        for stmt_idx in block.statements.nodes.iter().copied() {
            if initializer.is_none() {
                initializer = self.js_function_initializer_for_statement(stmt_idx, &returned_name);
            }
            if let Some(property) =
                self.js_define_property_decl_for_statement(stmt_idx, &returned_name)
            {
                properties.push(property);
            }
        }

        initializer
            .filter(|_| !properties.is_empty())
            .map(|init| (init, properties))
    }

    pub(in crate::declaration_emitter) fn js_function_initializer_for_statement(
        &self,
        stmt_idx: NodeIndex,
        returned_name: &str,
    ) -> Option<NodeIndex> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let variable = self.arena.get_variable(stmt_node)?;
        let decl_list_node = self.arena.get(variable.declarations.nodes[0])?;
        let decl_list = self.arena.get_variable(decl_list_node)?;

        decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;
                if self.get_identifier_text(decl.name).as_deref() != Some(returned_name) {
                    return None;
                }
                let init_node = self.arena.get(decl.initializer)?;
                if init_node.is_function_expression_or_arrow() {
                    Some(decl.initializer)
                } else {
                    None
                }
            })
    }

    pub(in crate::declaration_emitter) fn js_define_property_decl_for_statement(
        &self,
        stmt_idx: NodeIndex,
        returned_name: &str,
    ) -> Option<JsDefinedPropertyDecl> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.arena.get(expr_stmt.expression)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        if !self.is_object_define_property_call(call.expression) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        if args.nodes.len() != 3 {
            return None;
        }
        if self.get_identifier_text(args.nodes[0]).as_deref() != Some(returned_name) {
            return None;
        }

        let name = self.js_define_property_name(args.nodes[1])?;
        let (mut type_text, readonly) = self.js_define_property_descriptor(args.nodes[2])?;
        if name == "name" && type_text == "any" {
            type_text = "string".to_string();
        }
        Some(JsDefinedPropertyDecl {
            name,
            type_text,
            readonly,
        })
    }

    pub(in crate::declaration_emitter) fn js_commonjs_define_property_export_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<JsDefinedPropertyDecl> {
        if !self.source_is_js_file {
            return None;
        }

        let stmt_node = self.arena.get(stmt_idx)?;
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.arena.get(expr_stmt.expression)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        if !self.is_object_define_property_call(call.expression) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        if args.nodes.len() != 3 {
            return None;
        }

        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(args.nodes[0]);
        if !self.is_exports_identifier_reference(receiver)
            && !self.is_module_exports_reference(receiver)
        {
            return None;
        }

        let name = self.js_define_property_name(args.nodes[1])?;
        if self.declaration_property_name_text(&name) != name {
            return None;
        }
        let (type_text, readonly) = self.js_define_property_descriptor(args.nodes[2])?;
        Some(JsDefinedPropertyDecl {
            name,
            type_text,
            readonly,
        })
    }

    pub(in crate::declaration_emitter) fn is_object_define_property_call(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let expr_node = match self.arena.get(expr_idx) {
            Some(node) => node,
            None => return false,
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let access = match self.arena.get_access_expr(expr_node) {
            Some(access) => access,
            None => return false,
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("defineProperty") {
            return false;
        }
        self.get_identifier_text(access.expression).as_deref() == Some("Object")
    }

    pub(crate) fn is_object_assign_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("assign") {
            return false;
        }
        self.get_identifier_text(access.expression).as_deref() == Some("Object")
    }

    pub(in crate::declaration_emitter) fn js_define_property_name(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if let Some(identifier) = self.arena.get_identifier(expr_node) {
            return Some(identifier.escaped_text.clone());
        }
        self.arena
            .get_literal(expr_node)
            .map(|literal| literal.text.clone())
    }

    pub(in crate::declaration_emitter) fn js_define_property_descriptor(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, bool)> {
        let expr_node = self.arena.get(expr_idx)?;
        let object = self.arena.get_literal_expr(expr_node)?;
        let mut value_expr = None;
        let mut writable = false;

        for member_idx in object.elements.nodes.iter().copied() {
            let member_node = self.arena.get(member_idx)?;
            let assignment = self.arena.get_property_assignment(member_node)?;
            let name = self.js_define_property_name(assignment.name)?;
            match name.as_str() {
                "value" => value_expr = Some(assignment.initializer),
                "writable" => {
                    writable = self
                        .arena
                        .get(assignment.initializer)
                        .is_some_and(|init_node| init_node.kind == SyntaxKind::TrueKeyword as u16);
                }
                _ => {}
            }
        }

        let value_expr = value_expr?;
        let type_text = self
            .resolve_declaration_type_text(&[value_expr], Some(value_expr))
            .filter(|resolved_type| resolved_type.type_id != tsz_solver::types::TypeId::ANY)
            .map(|resolved_type| resolved_type.emitted_type_text)
            .or_else(|| self.js_string_concatenation_type_text(value_expr))
            .or_else(|| self.allowlisted_initializer_type_text(value_expr))
            .unwrap_or_else(|| "any".to_string());
        Some((type_text, !writable))
    }

    pub(in crate::declaration_emitter) fn js_string_concatenation_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::PlusToken as u16 {
            return None;
        }
        if self.js_expression_is_string_like(binary.left)
            || self.js_expression_is_string_like(binary.right)
        {
            Some("string".to_string())
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn js_expression_is_string_like(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::StringLiteral as u16
            || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || expr_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
        {
            return true;
        }
        self.js_string_concatenation_type_text(expr_idx).is_some()
    }

    pub(in crate::declaration_emitter) fn emit_function_initializer_call_signature(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && init_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write("): ");

        if func.type_annotation.is_some() {
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(initializer) {
            self.write(&return_type_text);
        } else if let Some(interner) = self.type_interner
            && let Some(type_id) = self.get_node_type_or_names(&[initializer])
            && let Some(return_type_id) =
                tsz_solver::type_queries::get_return_type(interner, type_id)
        {
            self.write(&self.print_type_id(return_type_id));
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write("void");
        } else {
            self.write("any");
        }

        true
    }

    pub(in crate::declaration_emitter) fn declaration_property_name_text(
        &self,
        name: &str,
    ) -> String {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return "\"\"".to_string();
        };
        let needs_quotes = !(first == '_' || first == '$' || first.is_ascii_alphabetic())
            || chars.any(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()));
        if needs_quotes {
            format!("\"{}\"", super::escape_string_for_double_quote(name))
        } else {
            name.to_string()
        }
    }

    pub(in crate::declaration_emitter) fn statement_returns_identifier(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .and_then(|ret| self.return_expression_is_identifier(ret.expression, name))
                .unwrap_or(false),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|child| self.statement_returns_identifier(child, name))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.statement_returns_identifier(if_data.then_statement, name)
                        || (if_data.else_statement.is_some()
                            && self.statement_returns_identifier(if_data.else_statement, name))
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.statement_returns_identifier(try_data.try_block, name)
                        || (try_data.catch_clause.is_some()
                            && self.statement_returns_identifier(try_data.catch_clause, name))
                        || (try_data.finally_block.is_some()
                            && self.statement_returns_identifier(try_data.finally_block, name))
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.statement_returns_identifier(catch_data.block, name)
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => self
                .arena
                .get_case_clause(stmt_node)
                .is_some_and(|case_data| {
                    case_data
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|child| self.statement_returns_identifier(child, name))
                }),
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn return_expression_identifier(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return Some(expr_idx);
        }
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.return_expression_identifier(paren.expression));
        }
        None
    }

    pub(in crate::declaration_emitter) fn return_expression_is_identifier(
        &self,
        expr_idx: NodeIndex,
        name: &str,
    ) -> Option<bool> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return Some(
                self.get_identifier_text(expr_idx)
                    .is_some_and(|text| text == name),
            );
        }
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.return_expression_is_identifier(paren.expression, name));
        }
        Some(false)
    }

    pub(in crate::declaration_emitter) fn type_has_visible_declaration_members(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };

        if let Some(shape_id) = tsz_solver::visitor::object_shape_id(interner, type_id)
            .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id))
        {
            let shape = interner.object_shape(shape_id);
            return shape.string_index.is_some()
                || shape.number_index.is_some()
                || shape.properties.iter().any(|property| {
                    let name = interner.resolve_atom(property.name);
                    name != "prototype" && !name.starts_with("__private_brand_")
                });
        }

        if let Some(shape_id) = tsz_solver::visitor::callable_shape_id(interner, type_id) {
            let shape = interner.callable_shape(shape_id);
            return shape.string_index.is_some()
                || shape.number_index.is_some()
                || shape.properties.iter().any(|property| {
                    let name = interner.resolve_atom(property.name);
                    name != "prototype" && !name.starts_with("__private_brand_")
                });
        }

        if let Some(list_id) = tsz_solver::visitor::intersection_list_id(interner, type_id) {
            return interner
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| self.type_has_visible_declaration_members(member));
        }

        if let Some(inner) = tsz_solver::visitor::readonly_inner_type(interner, type_id)
            .or_else(|| tsz_solver::visitor::no_infer_inner_type(interner, type_id))
        {
            return self.type_has_visible_declaration_members(inner);
        }

        false
    }

    pub(in crate::declaration_emitter) fn parameter_has_leading_inline_block_comment(
        &self,
        param_pos: u32,
    ) -> bool {
        let Some(ref text) = self.source_file_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut actual_start = param_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;

        for comment in &self.all_comments {
            if comment.end > actual_start {
                break;
            }
            let c_pos = comment.pos as usize;
            let c_end = comment.end as usize;
            let ct = &text[c_pos..c_end];
            if !ct.starts_with("/*") {
                continue;
            }

            let mut p = c_pos;
            let mut leading = true;
            while p > 0 {
                p -= 1;
                match bytes[p] {
                    b' ' | b'\t' | b'\r' | b'\n' => continue,
                    b'(' | b',' | b'[' | b'<' => break,
                    b'/' if p > 0 && bytes[p - 1] == b'*' => break,
                    _ => {
                        leading = false;
                        break;
                    }
                }
            }

            if leading {
                return true;
            }
        }

        false
    }

    /// Check if a type should be printed with a `typeof` prefix because the
    /// initializer is a bare identifier referencing a value-space entity (enum,
    /// module, function). Returns `Some("typeof Name")` if so, `None` otherwise.
    ///
    /// In tsc, `var x = E` (where E is an enum) emits `declare var x: typeof E;`
    /// because the variable holds the enum's runtime VALUE, not its TYPE meaning.
    pub(crate) fn typeof_prefix_for_value_entity(
        &self,
        initializer: NodeIndex,
        has_initializer: bool,
        type_id: Option<tsz_solver::types::TypeId>,
    ) -> Option<String> {
        if !has_initializer {
            return None;
        }
        let init_node = self.arena.get(initializer)?;
        let interner = self.type_interner?;

        if let Some(typeof_text) = self.direct_value_reference_typeof_text(initializer) {
            return Some(typeof_text);
        }

        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let rhs = self.get_identifier_text(access.name_or_argument)?;
            let lhs = self.nameable_constructor_expression_text(access.expression)?;
            let reference_text = format!("{lhs}.{rhs}");
            let typeof_text = || {
                format!(
                    "typeof {}",
                    self.relative_value_reference_text(&reference_text)
                )
            };
            let base_is_namespace_import_alias = self
                .value_reference_symbol(access.expression)
                .is_some_and(|sym_id| self.is_namespace_import_alias_symbol(sym_id));
            if self
                .value_reference_symbol_needs_typeof(access.name_or_argument)
                .or_else(|| self.value_reference_symbol_needs_typeof(initializer))
                .unwrap_or(false)
            {
                return Some(typeof_text());
            }
            let tid = type_id?;
            let is_callable = tsz_solver::visitor::function_shape_id(interner, tid).is_some()
                || tsz_solver::visitor::callable_shape_id(interner, tid).is_some();
            if base_is_namespace_import_alias && is_callable {
                return Some(typeof_text());
            }
            if !is_callable {
                return None;
            }
            let base_type = self.get_node_type_or_names(&[access.expression]);
            let is_constructor_like = base_type.is_some_and(|base_type| {
                if let Some(shape_id) = tsz_solver::visitor::callable_shape_id(interner, base_type)
                {
                    return !interner
                        .callable_shape(shape_id)
                        .construct_signatures
                        .is_empty();
                }
                tsz_solver::visitor::function_shape_id(interner, base_type)
                    .is_some_and(|shape_id| interner.function_shape(shape_id).is_constructor)
            });
            if is_constructor_like {
                return Some(typeof_text());
            }
            let binder = self.binder?;
            let base_sym_id = binder.get_node_symbol(access.expression)?;
            let symbol = binder.symbols.get(base_sym_id)?;
            if symbol.flags
                & (tsz_binder::symbol_flags::ENUM | tsz_binder::symbol_flags::VALUE_MODULE)
                != 0
            {
                return Some(typeof_text());
            }
            return None;
        }

        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let identifier_name = self.get_identifier_text(initializer)?;

        if self
            .value_reference_symbol_needs_typeof(initializer)
            .unwrap_or(false)
        {
            return Some(format!("typeof {identifier_name}"));
        }

        // Check if the type is an Enum type — this means the initializer is
        // referencing the enum value directly (e.g., `var x = E`)
        if let Some(tid) = type_id
            && let Some((def_id, _members_id)) = tsz_solver::visitor::enum_components(interner, tid)
        {
            // Verify the enum name matches the identifier to avoid
            // false positives with enum member types
            if let Some(cache) = &self.type_cache
                && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
                && let Some(binder) = self.binder
                && let Some(symbol) = binder.symbols.get(sym_id)
                && symbol.escaped_name == identifier_name
                && symbol.flags & tsz_binder::symbol_flags::ENUM != 0
                && symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
            {
                return Some(format!("typeof {identifier_name}"));
            }
        }

        // For Lazy(DefId) types pointing to VALUE_MODULE/FUNCTION, the printer
        // already handles the typeof prefix in print_lazy_type.
        None
    }

    pub(in crate::declaration_emitter) fn shadowed_property_initializer_typeof_text(
        &self,
        property_name: NodeIndex,
        initializer: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(property_name)?;
        let init_node = self.arena.get(initializer)?;
        if name_node.kind != SyntaxKind::Identifier as u16
            || init_node.kind != SyntaxKind::Identifier as u16
        {
            return None;
        }

        let name = self.get_identifier_text(property_name)?;
        if self.get_identifier_text(initializer).as_deref() != Some(name.as_str()) {
            return None;
        }

        let binder = self.binder?;
        let sym_id = binder.file_locals.get(&name)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }

        if symbol.has_any_flags(
            symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::ENUM
                | symbol_flags::VALUE_MODULE
                | symbol_flags::METHOD,
        ) {
            return Some(format!(
                "typeof {}",
                self.relative_value_reference_text(&name)
            ));
        }

        None
    }

    pub(in crate::declaration_emitter) fn direct_value_reference_typeof_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let binder = self.binder?;
        let sym_id = binder
            .get_node_symbol(expr_idx)
            .or_else(|| self.value_reference_symbol(expr_idx))?;
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        let symbol = binder.symbols.get(resolved_sym_id)?;
        let needs_typeof =
            self.value_reference_symbol_can_use_typeof(expr_idx, sym_id, resolved_sym_id, symbol);
        if !needs_typeof {
            return None;
        }

        let reference_text = self
            .nameable_constructor_expression_text(expr_idx)
            .or_else(|| self.get_identifier_text(expr_idx))?;
        let reference_text = self.relative_value_reference_text(&reference_text);
        Some(format!("typeof {reference_text}"))
    }

    pub(in crate::declaration_emitter) fn relative_value_reference_text(
        &self,
        reference_text: &str,
    ) -> String {
        let Some(enclosing_sym_id) = self.enclosing_namespace_symbol else {
            return reference_text.to_string();
        };
        let Some(binder) = self.binder else {
            return reference_text.to_string();
        };

        let mut enclosing_parts = Vec::new();
        let mut current = enclosing_sym_id;
        while current != SymbolId::NONE {
            let Some(symbol) = binder.symbols.get(current) else {
                break;
            };
            if !symbol.escaped_name.starts_with('"')
                && !symbol.escaped_name.starts_with("__")
                && symbol
                    .escaped_name
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            {
                enclosing_parts.push(symbol.escaped_name.as_str());
            }
            current = symbol.parent;
        }
        enclosing_parts.reverse();
        if enclosing_parts.len() < 2 {
            return reference_text.to_string();
        }

        let reference_parts: Vec<&str> = reference_text.split('.').collect();
        let common_len = reference_parts
            .iter()
            .zip(enclosing_parts.iter())
            .take_while(|(left, right)| left == right)
            .count();
        if common_len < 2 || common_len >= reference_parts.len() {
            return reference_text.to_string();
        }

        reference_parts[common_len - 1..].join(".")
    }

    pub(in crate::declaration_emitter) fn value_reference_symbol_needs_typeof(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<bool> {
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        let symbol = binder.symbols.get(resolved_sym_id)?;
        Some(self.value_reference_symbol_can_use_typeof(expr_idx, sym_id, resolved_sym_id, symbol))
    }

    pub(in crate::declaration_emitter) fn value_reference_symbol_can_use_typeof(
        &self,
        _expr_idx: NodeIndex,
        sym_id: SymbolId,
        resolved_sym_id: SymbolId,
        resolved_symbol: &tsz_binder::Symbol,
    ) -> bool {
        if resolved_symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return false;
        }

        if resolved_symbol.has_any_flags(
            symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::ENUM
                | symbol_flags::VALUE_MODULE
                | symbol_flags::METHOD,
        ) || self.is_namespace_import_alias_symbol(sym_id)
            || self.is_namespace_import_alias_symbol(resolved_sym_id)
        {
            return true;
        }

        false
    }

    pub(in crate::declaration_emitter) fn value_reference_symbol(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.get_identifier_text(expr_idx)?;
            return self.resolve_identifier_symbol(expr_idx, &ident);
        }
        if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
            return self.resolve_enclosing_class_symbol(expr_idx);
        }
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(expr_node)?;
            let is_super_access = self
                .arena
                .get(access.expression)
                .is_some_and(|base| base.kind == SyntaxKind::SuperKeyword as u16);
            if !is_super_access
                && let Some(sym_id) = binder.get_node_symbol(access.name_or_argument)
            {
                return Some(sym_id);
            }
            if self
                .arena
                .get(access.expression)
                .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
                && let Some(member_name) = self.get_identifier_text(access.name_or_argument)
                && let Some(sym_id) =
                    self.enclosing_class_member_symbol(access.expression, &member_name)
            {
                return Some(sym_id);
            }
            let base_sym_id = self.value_reference_symbol(access.expression)?;
            let resolved_base_sym_id = self.resolve_portability_symbol(base_sym_id, binder);
            let base_symbol = binder.symbols.get(resolved_base_sym_id)?;
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            // Try exports first (for namespaces, static class members via class name)
            if let Some(sym_id) = base_symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(&member_name))
            {
                return Some(sym_id);
            }
            // Also try members (for class instance members via `this`)
            if let Some(sym_id) = base_symbol
                .members
                .as_ref()
                .and_then(|members| members.get(&member_name))
            {
                return Some(sym_id);
            }
            if let Some(sym_id) = binder.get_node_symbol(expr_idx) {
                return Some(sym_id);
            }
            return None;
        }
        binder.get_node_symbol(expr_idx)
    }

    fn enclosing_class_member_symbol(
        &self,
        this_idx: NodeIndex,
        member_name: &str,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let this_node = self.arena.get(this_idx)?;
        let this_pos = this_node.pos;
        let mut best_class: Option<(NodeIndex, u32)> = None;

        for sym in binder.symbols.iter() {
            if (sym.flags & tsz_binder::symbol_flags::CLASS) == 0 {
                continue;
            }
            for &decl_idx in &sym.declarations {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if this_pos >= decl_node.pos && this_pos < decl_node.end {
                    let span = decl_node.end - decl_node.pos;
                    if best_class.is_none_or(|(_, best_span)| span < best_span) {
                        best_class = Some((decl_idx, span));
                    }
                }
            }
        }

        let class_idx = best_class.map(|(idx, _)| idx)?;
        let class_node = self.arena.get(class_idx)?;
        let class_decl = self.arena.get_class(class_node)?;
        for &member_idx in &class_decl.members.nodes {
            let Some(member_name_idx) = self.get_member_name_idx(member_idx) else {
                continue;
            };
            if self.get_identifier_text(member_name_idx).as_deref() != Some(member_name) {
                continue;
            }
            if let Some(sym_id) = binder
                .get_node_symbol(member_name_idx)
                .or_else(|| binder.get_node_symbol(member_idx))
            {
                return Some(sym_id);
            }
        }

        None
    }

    /// Resolve `this` to the innermost enclosing class symbol by position.
    pub(in crate::declaration_emitter) fn resolve_enclosing_class_symbol(
        &self,
        this_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let this_node = self.arena.get(this_idx)?;
        let this_pos = this_node.pos;

        let mut best: Option<(SymbolId, u32)> = None; // (sym_id, span_size)
        for sym in binder.symbols.iter() {
            if (sym.flags & tsz_binder::symbol_flags::CLASS) == 0 {
                continue;
            }
            for &decl_idx in &sym.declarations {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if this_pos >= decl_node.pos && this_pos < decl_node.end {
                    let span = decl_node.end - decl_node.pos;
                    if best.is_none_or(|(_, best_span)| span < best_span) {
                        best = Some((sym.id, span));
                    }
                }
            }
        }
        best.map(|(id, _)| id)
    }

    /// Get the text of an identifier node.
    pub(crate) fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.arena
            .get_identifier(node)
            .map(|id| id.escaped_text.clone())
    }

    /// Format a literal value as an initializer string for `const` declarations in .d.ts.
    ///
    /// Produces the value form used in `declare const x = "abc"` style declarations.
    pub(crate) fn format_literal_initializer(
        lit: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match lit {
            tsz_solver::types::LiteralValue::String(atom) => {
                format!(
                    "\"{}\"",
                    super::escape_string_for_double_quote(&interner.resolve_atom(*atom))
                )
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    pub(in crate::declaration_emitter) fn js_special_initializer_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;

        if self.is_import_meta_url_expression(initializer) {
            return Some("string".to_string());
        }

        if init_node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            let await_expr = self.arena.get_unary_expr_ex(init_node)?;
            return self.js_literal_type_text(await_expr.expression);
        }

        None
    }

    pub(in crate::declaration_emitter) fn initializer_uses_inaccessible_class_constructor(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }

        let Some(new_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        let Some(sym_id) = self.new_expression_target_symbol(new_expr.expression) else {
            return false;
        };

        self.symbol_has_inaccessible_constructor(sym_id)
    }

    pub(in crate::declaration_emitter) fn new_expression_target_symbol(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        if let Some(sym_id) = binder.get_node_symbol(expr_idx) {
            return Some(sym_id);
        }

        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .get_identifier_text(expr_idx)
                .and_then(|name| binder.file_locals.get(&name)),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                let access = self.arena.get_access_expr(expr_node)?;
                binder.get_node_symbol(access.name_or_argument)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn symbol_has_inaccessible_constructor(
        &self,
        sym_id: SymbolId,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            let Some(class_decl) = self.arena.get_class(decl_node) else {
                return false;
            };

            class_decl.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = self.arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                    return false;
                }
                let Some(ctor) = self.arena.get_constructor(member_node) else {
                    return false;
                };
                self.arena
                    .has_modifier(&ctor.modifiers, SyntaxKind::PrivateKeyword)
                    || self
                        .arena
                        .has_modifier(&ctor.modifiers, SyntaxKind::ProtectedKeyword)
            })
        })
    }

    pub(in crate::declaration_emitter) fn js_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => self
                .arena
                .get_literal(expr_node)
                .map(|lit| format!("\"{}\"", super::escape_string_for_double_quote(&lit.text))),
            k if k == SyntaxKind::NumericLiteral as u16 => {
                self.arena.get_literal(expr_node).map(|lit| {
                    let text = &lit.text;
                    // Strip numeric separators (tsc strips them in .d.ts output)
                    if text.contains('_') {
                        if let Some(v) = lit.value {
                            if v.fract() == 0.0 && v.abs() < 1e20 {
                                return format!("{}", v as i64);
                            }
                            return v.to_string();
                        }
                        return text.replace('_', "");
                    }
                    // For large numbers (21+ digits), parse as f64 and format
                    // using JS Number.toString() semantics (scientific notation).
                    let digits = text.chars().filter(|c| c.is_ascii_digit()).count();
                    if digits >= 21
                        && let Ok(n) = text.parse::<f64>()
                    {
                        return Self::format_js_number(n);
                    }
                    text.clone()
                })
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                self.arena.get_literal(expr_node).map(|lit| {
                    // Strip numeric separators from bigint literals
                    if lit.text.contains('_') {
                        lit.text.replace('_', "")
                    } else {
                        lit.text.clone()
                    }
                })
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let raw = self.get_source_slice_no_semi(expr_node.pos, expr_node.end)?;
                // Strip numeric separators from negative literals (e.g., -1_000 → -1000)
                if raw.contains('_') {
                    Some(raw.replace('_', ""))
                } else {
                    Some(raw)
                }
            }
            _ => None,
        }
    }
}
