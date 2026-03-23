//! Type query (typeof) resolution — `get_type_from_type_query` and helpers.

use crate::context::TypingRequest;
use crate::query_boundaries::common::lazy_def_id;
use crate::state::CheckerState;
use tracing::trace;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn get_enum_namespace_type_for_value(&mut self, type_id: TypeId) -> TypeId {
        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) else {
            return type_id;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return type_id;
        };
        if symbol.flags & tsz_binder::symbol_flags::ENUM == 0
            || (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
        {
            return type_id;
        }
        self.ctx
            .enum_namespace_types
            .get(&sym_id)
            .copied()
            .unwrap_or_else(|| self.merge_namespace_exports_into_object(sym_id, type_id))
    }

    pub(crate) fn get_type_from_type_query_flow_sensitive_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use tsz_solver::SymbolRef;
        trace!(idx = idx.0, "ENTER get_type_from_type_query_flow_sensitive");

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR; // Missing type query data - propagate error
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
            let expr_request = if use_flow {
                request.read().contextual_opt(None)
            } else {
                request.write().contextual_opt(None)
            };
            state.get_type_of_node_with_request(type_query.expr_name, &expr_request)
        };

        // `typeof default` is not valid — `default` is a keyword and is not visible
        // as a local binding even if the file has an `export default` declaration.
        // TypeScript reports TS2304 "Cannot find name 'default'" in this case.
        if is_identifier && name_text.as_deref() == Some("default") {
            // Route through boundary for TS2304/TS2552 with suggestion collection
            self.report_not_found_at_boundary(
                "default",
                type_query.expr_name,
                crate::query_boundaries::name_resolution::NameLookupKind::Value,
            );
            return TypeId::ERROR;
        }

        // Check typeof_param_scope — resolves `typeof paramName` in return type
        // annotations where the parameter isn't a file-level binding.
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
            // `typeof f` inside `f`'s own signature must stay as a type-query
            // marker. Expanding the symbol type here re-enters provisional
            // signature building and can recurse through self-referential
            // `typeof` annotations until the stack overflows.
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
            // Handle QualifiedName (e.g. `typeof x.p`) by resolving as value property access.
            // QualifiedName in typeof context means value.property, not namespace.member,
            // so we can't send it through get_type_of_node which dispatches to resolve_qualified_name.
            if expr_node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
                if let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) {
                    let left_idx = qn.left;
                    let right_idx = qn.right;
                    // Resolve the left side as a value expression.
                    // For nested qualified names (e.g. `typeof a.b.c`), recurse
                    // through the value property chain instead of dispatching to
                    // resolve_qualified_name which treats it as a namespace.
                    let left_type = self.resolve_typeof_qualified_value_chain_with_request(
                        left_idx,
                        request,
                        use_flow_sensitive_query,
                    );
                    trace!(left_type = ?left_type, "type_query qualified: left_type");
                    if left_type == TypeId::ANY {
                        // globalThis resolves to ANY since it's a synthetic global.
                        // `typeof globalThis.foo` should also be ANY (no TS2304).
                        if let Some(left_node) = self.ctx.arena.get(left_idx)
                            && let Some(ident) = self.ctx.arena.get_identifier(left_node)
                            && ident.escaped_text == "globalThis"
                        {
                            return TypeId::ANY;
                        }
                    }
                    if left_type != TypeId::ANY && left_type != TypeId::ERROR {
                        // Look up the right side as a property on the left type
                        if let Some(right_node) = self.ctx.arena.get(right_idx)
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
                                    // Resolve TypeQuery types (e.g., `typeof X`) in the
                                    // property result so that `typeof k.foo` where
                                    // `foo: typeof I` yields the resolved value type.
                                    let property_type = self.resolve_type_query_type(type_id);
                                    let resolved =
                                        self.get_enum_namespace_type_for_value(property_type);
                                    return if use_flow_sensitive_query {
                                        self.apply_flow_narrowing(type_query.expr_name, resolved)
                                    } else {
                                        resolved
                                    };
                                }
                                _ => {
                                    // Property access returned any/error or failed entirely.
                                    // Fall through to binder-based resolution below.
                                }
                            }
                        }
                    }
                    // Fall back: resolve via binder symbol exports for namespace members
                    if let Some(sym_id) = self.resolve_qualified_symbol(type_query.expr_name) {
                        let member_type = self.get_type_of_symbol(sym_id);
                        trace!(sym_id = ?sym_id, member_type = ?member_type, "type_query qualified: resolved via binder exports");
                        if member_type != TypeId::ERROR {
                            return self.get_enum_namespace_type_for_value(member_type);
                        }
                    }
                }
            } else if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || expr_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                || expr_node.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16
            {
                // Skip flow resolution for type-only imports — evaluating them as
                // expressions would emit a false TS1361.  They are handled below
                // via resolve_type_symbol_for_lowering which creates a TypeQuery.
                let is_type_only_import = expr_node.kind
                    == tsz_scanner::SyntaxKind::Identifier as u16
                    && self
                        .resolve_identifier_symbol(type_query.expr_name)
                        .is_some_and(|sym_id| self.alias_resolves_to_type_only(sym_id));

                if !is_type_only_import {
                    // Prefer the value-space type at the query site. Most `typeof`
                    // queries are flow-sensitive, but type-only function-like
                    // parameter positions use the declared type instead.
                    // BUT skip Lazy types - those indicate circular reference (e.g., `typeof A`
                    // inside class A's body). Lazy types resolve to the instance type via
                    // resolve_lazy, but typeof needs the constructor type. Fall through to
                    // create a TypeQuery(SymbolRef) which resolves correctly.
                    let expr_type = query_expr_type(self, use_flow_sensitive_query);
                    let is_lazy = lazy_def_id(self.ctx.types, expr_type).is_some();
                    if expr_type != TypeId::ANY && expr_type != TypeId::ERROR && !is_lazy {
                        return self.get_enum_namespace_type_for_value(expr_type);
                    }
                }
            }
        }

        let base = if let Some(sym_id) =
            self.resolve_value_symbol_for_lowering(type_query.expr_name)
        {
            trace!("=== get_type_from_type_query ===");
            trace!(name = ?name_text, sym_id, "get_type_from_type_query");

            // Always compute the symbol type to ensure it's in the type environment
            // This is important for Application resolution and TypeQuery resolution during subtype checking
            let resolved = self.get_type_of_symbol(tsz_binder::SymbolId(sym_id));
            trace!(resolved = ?resolved, "resolved type");

            if !has_type_args {
                // Prefer the type at the query site for `typeof expr`. Most queries
                // preserve control-flow narrowing, but type-only function-like
                // parameter positions resolve from the declared type.
                // Skip Lazy types - they indicate circular reference and would resolve to
                // the instance type instead of the constructor type needed for typeof.
                let flow_resolved = query_expr_type(self, use_flow_sensitive_query);
                let flow_is_lazy = lazy_def_id(self.ctx.types, flow_resolved).is_some();
                if flow_resolved != TypeId::ANY && flow_resolved != TypeId::ERROR && !flow_is_lazy {
                    let flow_resolved = self.get_enum_namespace_type_for_value(flow_resolved);
                    trace!(flow_resolved = ?flow_resolved, "=> returning flow-resolved type directly");
                    return flow_resolved;
                }
                let resolved_is_lazy = lazy_def_id(self.ctx.types, resolved).is_some();
                if resolved != TypeId::ANY && resolved != TypeId::ERROR && !resolved_is_lazy {
                    let resolved = self.get_enum_namespace_type_for_value(resolved);
                    // Fall back to symbol type when flow result is unavailable.
                    trace!("=> returning symbol-resolved type directly");
                    return resolved;
                }
            }

            // For type arguments or when resolved is ANY/ERROR, use TypeQuery
            let typequery_type = factory.type_query(SymbolRef(sym_id));
            trace!(typequery_type = ?typequery_type, "=> returning TypeQuery type");
            typequery_type
        } else if let Some(type_sym_id) = self
            .resolve_type_symbol_for_lowering(type_query.expr_name)
            .or_else(|| self.resolve_type_query_import_type_symbol(type_query.expr_name))
        {
            // Check if this is a type-only import (import type { A }).
            // tsc allows `typeof A` on type-only imports in type annotations
            // because typeof in a type position is a compile-time type query,
            // not a runtime value access. Resolve the type instead of erroring.
            let is_type_only_import = self
                .resolve_identifier_symbol(type_query.expr_name)
                .is_some_and(|sym_id| self.alias_resolves_to_type_only(sym_id));

            if is_type_only_import {
                factory.type_query(SymbolRef(type_sym_id))
            } else {
                let name = name_text.as_deref().unwrap_or("<unknown>");
                self.report_wrong_meaning_diagnostic(
                    name,
                    type_query.expr_name,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return TypeId::ERROR;
            }
        } else if let Some(name) = name_text {
            if is_identifier {
                // Handle global intrinsics that may not have symbols in the binder
                // (e.g., `typeof undefined`, `typeof NaN`, `typeof Infinity`, `typeof globalThis`)
                match name.as_str() {
                    "undefined" => return TypeId::UNDEFINED,
                    "NaN" | "Infinity" => return TypeId::NUMBER,
                    // `typeof globalThis` behaves as a top type in intersections:
                    // `Window & typeof globalThis` should preserve the concrete
                    // `Window` members instead of collapsing to `any`.
                    "globalThis" => return TypeId::UNKNOWN,
                    _ => {}
                }
                if self.is_known_global_value_name(&name) {
                    // Emit TS2318/TS2583 for missing global type in typeof context
                    // TS2583 for ES2015+ types, TS2304 for other globals
                    use tsz_binder::lib_loader;
                    if lib_loader::is_es2015_plus_type(&name) {
                        self.error_cannot_find_global_type(&name, type_query.expr_name);
                    } else {
                        // Route through boundary for TS2304/TS2552 with suggestion collection
                        self.report_not_found_at_boundary(
                            &name,
                            type_query.expr_name,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
                    }
                    return TypeId::ERROR;
                }
                // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                if self.is_unresolved_import_symbol(type_query.expr_name) {
                    return TypeId::ANY;
                }
                // Route through boundary for TS2304/TS2552 with suggestion collection
                let req = crate::query_boundaries::name_resolution::NameResolutionRequest::value(
                    &name,
                    type_query.expr_name,
                );
                let failure =
                    crate::query_boundaries::name_resolution::ResolutionFailure::not_found();
                self.report_name_resolution_failure(&req, &failure);
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
                // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                if self.is_unresolved_import_symbol(missing_idx) {
                    return TypeId::ANY;
                }
                // Route through boundary for TS2304/TS2552 with suggestion collection
                let req = crate::query_boundaries::name_resolution::NameResolutionRequest::value(
                    &missing_name,
                    missing_idx,
                );
                let failure =
                    crate::query_boundaries::name_resolution::ResolutionFailure::not_found();
                self.report_name_resolution_failure(&req, &failure);
                return TypeId::ERROR;
            }
            if self.report_type_query_missing_member(type_query.expr_name) {
                return TypeId::ERROR;
            }
            // Not found - fall back to hash (for forward compatibility)
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            name.hash(&mut hasher);
            let symbol_id = hasher.finish() as u32;
            factory.type_query(SymbolRef(symbol_id))
        } else {
            return TypeId::ERROR; // No name text - propagate error
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
