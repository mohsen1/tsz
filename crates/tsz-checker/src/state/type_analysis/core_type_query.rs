//! Type query (`typeof`) resolution extracted from core.rs.

use crate::state::CheckerState;
use tracing::trace;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::SymbolRef;
        trace!(idx = idx.0, "ENTER get_type_from_type_query");

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR;
        };

        if self.is_import_type_query(type_query.expr_name) {
            trace!("get_type_from_type_query: is import type query");
            return TypeId::ANY;
        }

        let name_text = self.entity_name_text(type_query.expr_name);
        let is_identifier = self
            .ctx
            .arena
            .get(type_query.expr_name)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some();
        let has_type_args = type_query
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());
        let factory = self.ctx.types.factory();
        let use_flow_sensitive_query =
            !self.is_type_query_in_non_flow_sensitive_signature_parameter(idx);
        let query_expr_type = |state: &mut Self, use_flow: bool| {
            let prev_skip = state.ctx.skip_flow_narrowing;
            state.ctx.skip_flow_narrowing = !use_flow;
            let ty = state.get_type_of_node(type_query.expr_name);
            state.ctx.skip_flow_narrowing = prev_skip;
            ty
        };

        if is_identifier
            && let Some(ref name) = name_text
            && let Some(&param_type) = self.ctx.typeof_param_scope.get(name.as_str())
        {
            return param_type;
        }

        if let Some(sym_id) = self
            .resolve_value_symbol_for_lowering(type_query.expr_name)
            .filter(|sym_id| {
                self.ctx
                    .symbol_resolution_set
                    .contains(&tsz_binder::SymbolId(*sym_id))
            })
        {
            let base = factory.type_query(SymbolRef(sym_id));
            if let Some(args) = &type_query.type_arguments
                && !args.nodes.is_empty()
            {
                let type_args = args
                    .nodes
                    .iter()
                    .map(|&idx| self.get_type_from_type_node(idx))
                    .collect();
                return factory.application(base, type_args);
            }
            return base;
        }

        if !has_type_args && let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) {
            if expr_node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
                if let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) {
                    let left_idx = qn.left;
                    let right_idx = qn.right;
                    let left_type = self
                        .resolve_typeof_qualified_value_chain(left_idx, use_flow_sensitive_query);
                    trace!(left_type = ?left_type, "type_query qualified: left_type");
                    if left_type == TypeId::ANY
                        && let Some(left_node) = self.ctx.arena.get(left_idx)
                        && let Some(ident) = self.ctx.arena.get_identifier(left_node)
                        && ident.escaped_text == "globalThis"
                    {
                        return TypeId::ANY;
                    }
                    if left_type != TypeId::ANY
                        && left_type != TypeId::ERROR
                        && let Some(right_node) = self.ctx.arena.get(right_idx)
                        && let Some(ident) = self.ctx.arena.get_identifier(right_node)
                    {
                        let prop_name = ident.escaped_text.clone();
                        let object_type = self.resolve_type_for_property_access(left_type);
                        trace!(object_type = ?object_type, prop_name = %prop_name, "type_query qualified: property access");
                        use crate::query_boundaries::common::PropertyAccessResult;
                        match self.resolve_property_access_with_env(object_type, &prop_name) {
                            PropertyAccessResult::Success { type_id, .. }
                                if type_id != TypeId::ANY && type_id != TypeId::ERROR =>
                            {
                                let resolved = self.resolve_type_query_type(type_id);
                                return if use_flow_sensitive_query {
                                    self.apply_flow_narrowing(type_query.expr_name, resolved)
                                } else {
                                    resolved
                                };
                            }
                            _ => {}
                        }
                    }
                    if let Some(sym_id) = self.resolve_qualified_symbol(type_query.expr_name) {
                        let member_type = self.get_type_of_symbol(sym_id);
                        trace!(sym_id = ?sym_id, member_type = ?member_type, "type_query qualified: resolved via binder exports");
                        if member_type != TypeId::ERROR {
                            return member_type;
                        }
                    }
                }
            } else if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || expr_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                || expr_node.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16
            {
                let is_type_only_import = expr_node.kind
                    == tsz_scanner::SyntaxKind::Identifier as u16
                    && self
                        .resolve_identifier_symbol(type_query.expr_name)
                        .is_some_and(|sym_id| self.alias_resolves_to_type_only(sym_id));

                if !is_type_only_import {
                    let expr_type = query_expr_type(self, use_flow_sensitive_query);
                    let is_lazy =
                        tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, expr_type)
                            .is_some();
                    if expr_type != TypeId::ANY && expr_type != TypeId::ERROR && !is_lazy {
                        if tsz_solver::is_enum_type(self.ctx.types, expr_type)
                            && let Some(sym_id) =
                                self.resolve_value_symbol_for_lowering(type_query.expr_name)
                        {
                            if let Some(&ns_type) = self
                                .ctx
                                .enum_namespace_types
                                .get(&tsz_binder::SymbolId(sym_id))
                            {
                                return ns_type;
                            }
                            return self.merge_namespace_exports_into_object(
                                tsz_binder::SymbolId(sym_id),
                                expr_type,
                            );
                        }
                        return expr_type;
                    }
                }
            }
        }

        let base = if let Some(sym_id) =
            self.resolve_value_symbol_for_lowering(type_query.expr_name)
        {
            trace!("=== get_type_from_type_query ===");
            trace!(name = ?name_text, sym_id, "get_type_from_type_query");

            let resolved = self.get_type_of_symbol(tsz_binder::SymbolId(sym_id));
            trace!(resolved = ?resolved, "resolved type");

            if !has_type_args {
                let flow_resolved = query_expr_type(self, use_flow_sensitive_query);
                let flow_is_lazy =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, flow_resolved)
                        .is_some();
                if flow_resolved != TypeId::ANY && flow_resolved != TypeId::ERROR && !flow_is_lazy {
                    if tsz_solver::is_enum_type(self.ctx.types, flow_resolved) {
                        if let Some(&ns_type) = self
                            .ctx
                            .enum_namespace_types
                            .get(&tsz_binder::SymbolId(sym_id))
                        {
                            return ns_type;
                        }
                        return self.merge_namespace_exports_into_object(
                            tsz_binder::SymbolId(sym_id),
                            flow_resolved,
                        );
                    }
                    trace!(flow_resolved = ?flow_resolved, "=> returning flow-resolved type directly");
                    return flow_resolved;
                }
                let resolved_is_lazy =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved).is_some();
                if resolved != TypeId::ANY && resolved != TypeId::ERROR && !resolved_is_lazy {
                    if tsz_solver::is_enum_type(self.ctx.types, resolved) {
                        if let Some(&ns_type) = self
                            .ctx
                            .enum_namespace_types
                            .get(&tsz_binder::SymbolId(sym_id))
                        {
                            return ns_type;
                        }
                        return self.merge_namespace_exports_into_object(
                            tsz_binder::SymbolId(sym_id),
                            resolved,
                        );
                    }
                    trace!("=> returning symbol-resolved type directly");
                    return resolved;
                }
            }

            let typequery_type = factory.type_query(SymbolRef(sym_id));
            trace!(typequery_type = ?typequery_type, "=> returning TypeQuery type");
            typequery_type
        } else if let Some(type_sym_id) = self
            .resolve_type_symbol_for_lowering(type_query.expr_name)
            .or_else(|| self.resolve_type_query_import_type_symbol(type_query.expr_name))
        {
            let is_type_only_import = self
                .resolve_identifier_symbol(type_query.expr_name)
                .is_some_and(|sym_id| self.alias_resolves_to_type_only(sym_id));

            if is_type_only_import {
                factory.type_query(SymbolRef(type_sym_id))
            } else {
                let name = name_text.as_deref().unwrap_or("<unknown>");
                self.error_type_only_value_at(name, type_query.expr_name);
                return TypeId::ERROR;
            }
        } else if let Some(name) = name_text {
            if is_identifier {
                match name.as_str() {
                    "undefined" => return TypeId::UNDEFINED,
                    "NaN" | "Infinity" => return TypeId::NUMBER,
                    "globalThis" => return TypeId::UNKNOWN,
                    _ => {}
                }
                if self.is_known_global_value_name(&name) {
                    use tsz_binder::lib_loader;
                    if lib_loader::is_es2015_plus_type(&name) {
                        self.error_cannot_find_global_type(&name, type_query.expr_name);
                    } else {
                        self.error_cannot_find_name_at(&name, type_query.expr_name);
                    }
                    return TypeId::ERROR;
                }
                if self.is_unresolved_import_symbol(type_query.expr_name) {
                    return TypeId::ANY;
                }
                self.error_cannot_find_name_at(&name, type_query.expr_name);
                return TypeId::ERROR;
            }
            if let Some(missing_idx) = self.missing_type_query_left(type_query.expr_name)
                && let Some(missing_name) = self
                    .ctx
                    .arena
                    .get(missing_idx)
                    .and_then(|node| self.ctx.arena.get_identifier(node))
                    .map(|ident| ident.escaped_text.clone())
            {
                if self.is_unresolved_import_symbol(missing_idx) {
                    return TypeId::ANY;
                }
                self.error_cannot_find_name_at(&missing_name, missing_idx);
                return TypeId::ERROR;
            }
            if self.report_type_query_missing_member(type_query.expr_name) {
                return TypeId::ERROR;
            }
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            name.hash(&mut hasher);
            let symbol_id = hasher.finish() as u32;
            factory.type_query(SymbolRef(symbol_id))
        } else {
            return TypeId::ERROR;
        };

        let factory = self.ctx.types.factory();
        if let Some(args) = &type_query.type_arguments
            && !args.nodes.is_empty()
        {
            let type_args = args
                .nodes
                .iter()
                .map(|&idx| self.get_type_from_type_node(idx))
                .collect();
            return factory.application(base, type_args);
        }

        base
    }

    fn is_import_type_query(&self, expr_name: NodeIndex) -> bool {
        let mut current = expr_name;

        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION => {
                    let Some(call_expr) = self.ctx.arena.get_call_expr(node) else {
                        return false;
                    };
                    let Some(callee) = self.ctx.arena.get(call_expr.expression) else {
                        return false;
                    };
                    return callee.kind == tsz_scanner::SyntaxKind::ImportKeyword as u16;
                }
                tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    let Some(access) = self.ctx.arena.get_access_expr(node) else {
                        return false;
                    };
                    current = access.expression;
                }
                tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME => {
                    let Some(name) = self.ctx.arena.get_qualified_name(node) else {
                        return false;
                    };
                    current = name.left;
                }
                tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                        return false;
                    };
                    current = paren.expression;
                }
                _ => return false,
            }
        }
    }
}
