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

impl<'a> DeclarationEmitter<'a> {
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
                if init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                {
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
            .get_node_type_or_names(&[value_expr])
            .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
            .map(|type_id| self.print_type_id(type_id))
            .or_else(|| self.js_string_concatenation_type_text(value_expr))
            .or_else(|| self.infer_fallback_type_text(value_expr))
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
            let base_is_namespace_import_alias = self
                .value_reference_symbol(access.expression)
                .is_some_and(|sym_id| self.is_namespace_import_alias_symbol(sym_id));
            if self
                .value_reference_symbol_needs_typeof(access.name_or_argument)
                .or_else(|| self.value_reference_symbol_needs_typeof(initializer))
                .unwrap_or(false)
            {
                return Some(format!("typeof {lhs}.{rhs}"));
            }
            let tid = type_id?;
            let is_callable = tsz_solver::visitor::function_shape_id(interner, tid).is_some()
                || tsz_solver::visitor::callable_shape_id(interner, tid).is_some();
            if base_is_namespace_import_alias && is_callable {
                return Some(format!("typeof {lhs}.{rhs}"));
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
                return Some(format!("typeof {lhs}.{rhs}"));
            }
            let binder = self.binder?;
            let base_sym_id = binder.get_node_symbol(access.expression)?;
            let symbol = binder.symbols.get(base_sym_id)?;
            if symbol.flags
                & (tsz_binder::symbol_flags::ENUM | tsz_binder::symbol_flags::VALUE_MODULE)
                != 0
            {
                return Some(format!("typeof {lhs}.{rhs}"));
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
        Some(format!("typeof {reference_text}"))
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
        expr_idx: NodeIndex,
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

        self.is_value_import_alias_symbol(sym_id) && self.value_reference_type_is_callable(expr_idx)
    }

    pub(in crate::declaration_emitter) fn value_reference_type_is_callable(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        let Some(type_id) = self.get_node_type_or_names(&[expr_idx]) else {
            return false;
        };

        tsz_solver::visitor::function_shape_id(interner, type_id).is_some()
            || tsz_solver::visitor::callable_shape_id(interner, type_id).is_some()
    }

    pub(in crate::declaration_emitter) fn is_value_import_alias_symbol(
        &self,
        sym_id: SymbolId,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.import_module.is_some()
            && !symbol.is_type_only
            && symbol.import_name.as_deref() != Some("*")
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
            if let Some(sym_id) = binder.get_node_symbol(access.name_or_argument) {
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

    pub(in crate::declaration_emitter) fn const_literal_initializer_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => self
                .arena
                .get_unary_expr_ex(expr_node)
                .and_then(|await_expr| self.const_literal_initializer_text(await_expr.expression)),
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.const_literal_initializer_text(paren.expression)),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16 =>
            {
                self.js_literal_type_text(expr_idx)
            }
            // Template literal without substitutions: `hello` → "hello"
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    let escaped = super::escape_string_for_double_quote(&lit.text);
                    Some(format!("\"{escaped}\""))
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let raw = self.get_source_slice_no_semi(expr_node.pos, expr_node.end)?;
                if raw.contains('_') {
                    Some(raw.replace('_', ""))
                } else {
                    Some(raw)
                }
            }
            _ => self.simple_enum_access_member_text(expr_idx),
        }
    }

    /// Like `const_literal_initializer_text` but also unwraps `as` and
    /// `satisfies` expressions to find the underlying literal.
    pub(in crate::declaration_emitter) fn const_literal_initializer_text_deep(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        // Try the normal path first
        if let Some(text) = self.const_literal_initializer_text(expr_idx) {
            return Some(text);
        }
        if let Some(text) = self.const_literal_identity_call_text(expr_idx) {
            return Some(text);
        }
        // Unwrap as/satisfies expressions
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == syntax_kind_ext::AS_EXPRESSION
            || expr_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(expr_node)?;
            return self.const_literal_initializer_text_deep(assertion.expression);
        }
        None
    }

    pub(in crate::declaration_emitter) fn const_literal_identity_call_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let args = call.arguments.as_ref()?;
        if args.nodes.len() != 1 {
            return None;
        }

        let func = self.identity_returning_function(call.expression)?;
        let callee_body = func.body;
        let returned_identifier = self.function_body_unique_return_identifier(callee_body)?;
        let return_name = self.get_identifier_text(returned_identifier)?;
        let first_param_name = func
            .parameters
            .nodes
            .first()
            .copied()
            .and_then(|param_idx| self.arena.get(param_idx))
            .and_then(|param_node| self.arena.get_parameter(param_node))
            .and_then(|param| self.get_identifier_text(param.name))?;

        if first_param_name != return_name {
            return None;
        }

        let mut text = self.const_literal_initializer_text_deep(args.nodes[0])?;
        if text.starts_with('-') {
            while text.ends_with(')') {
                text.pop();
            }
        }
        Some(text)
    }

    pub(in crate::declaration_emitter) fn identity_returning_function(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let callee_name = self.get_identifier_text(callee_idx)?;
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
                let func = self.arena.get_function(decl_node)?;
                let same_name = self
                    .get_identifier_text(func.name)
                    .is_some_and(|name| name == callee_name);
                (same_name && func.body.is_some() && func.parameters.nodes.len() == 1)
                    .then_some(func)
            })
    }

    pub(in crate::declaration_emitter) fn is_import_meta_url_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("url") {
            return false;
        }

        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(base_access) = self.arena.get_access_expr(base_node) else {
            return false;
        };
        if self
            .get_identifier_text(base_access.name_or_argument)
            .as_deref()
            != Some("meta")
        {
            return false;
        }

        self.arena
            .get(base_access.expression)
            .is_some_and(|node| node.kind == SyntaxKind::ImportKeyword as u16)
    }

    /// Format a f64 value as JavaScript would display it.
    ///
    /// Matches JS `Number.prototype.toString()` behavior:
    /// - Infinity/NaN → "Infinity"/"NaN"
    /// - Uses scientific notation for numbers with >= 21 integer digits
    /// - Uses scientific notation for very small numbers
    pub(crate) fn format_js_number(n: f64) -> String {
        if n.is_infinite() {
            if n.is_sign_positive() {
                "Infinity".to_string()
            } else {
                "-Infinity".to_string()
            }
        } else if n.is_nan() {
            "NaN".to_string()
        } else {
            let s = n.to_string();
            // Rust's default formatter doesn't use scientific notation for large
            // integers. JS switches to scientific notation when the integer part
            // has 21+ digits. Detect and convert.
            let abs_s = s.strip_prefix('-').unwrap_or(&s);
            let needs_scientific = if let Some(dot_pos) = abs_s.find('.') {
                dot_pos >= 21
            } else {
                abs_s.len() >= 21
            };
            if needs_scientific {
                Self::format_js_scientific(n)
            } else {
                s
            }
        }
    }

    /// Format a number in JavaScript-style scientific notation (e.g., `1.2345678912345678e+53`).
    pub(in crate::declaration_emitter) fn format_js_scientific(n: f64) -> String {
        let neg = n < 0.0;
        let abs_n = n.abs();
        // Use Rust's {:e} format which gives e.g. "1.2345678912345678e53"
        let s = format!("{abs_n:e}");
        // JS uses e+N for positive exponents, e-N for negative
        let result = if let Some(pos) = s.find('e') {
            let (mantissa, exp_part) = s.split_at(pos);
            let exp_str = &exp_part[1..]; // skip 'e'
            if exp_str.starts_with('-') {
                format!("{mantissa}e{exp_str}")
            } else {
                format!("{mantissa}e+{exp_str}")
            }
        } else {
            s
        };
        if neg { format!("-{result}") } else { result }
    }

    /// Normalize a numeric literal string through f64, matching tsc's JS round-trip behavior.
    /// E.g., `123456789123456789123456789123456789123456789123456789` → `1.2345678912345678e+53`
    pub(crate) fn normalize_numeric_literal(text: &str) -> String {
        if let Ok(val) = text.parse::<f64>() {
            let normalized = Self::format_js_number(val);
            if normalized != text {
                return normalized;
            }
        }
        text.to_string()
    }
}
