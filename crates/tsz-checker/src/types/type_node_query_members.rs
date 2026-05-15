//! Type query member helpers.

use super::type_node::TypeNodeChecker;
use crate::context::CheckerContext;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(crate) fn value_property_type_query(&self, expr_name: NodeIndex) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        if let Some(imported_type) = self.import_call_typeof_value(expr_name) {
            return Some(imported_type);
        }

        let node = self.ctx.arena.get(expr_name)?;
        let (base, property_name_node) = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let access = self.ctx.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            (access.expression, access.name_or_argument)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(node)?;
            (qualified.left, qualified.right)
        } else {
            return None;
        };

        let property_name = self.property_name_text(property_name_node)?;
        let base_type = self.value_type_for_type_query_member_base(base)?;
        let evaluated_base =
            crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
                self.ctx.types,
                &*self.ctx,
                base_type,
                std::iter::empty(),
                false,
                self.ctx.is_declaration_file() || self.ctx.emit_declarations(),
            )
            .result;
        let base_type = if evaluated_base != TypeId::ERROR {
            evaluated_base
        } else {
            base_type
        };
        match crate::query_boundaries::property_access::resolve_property_access(
            self.ctx.types,
            base_type,
            &property_name,
        ) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. }
            | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } if type_id != TypeId::ANY && type_id != TypeId::ERROR => Some(type_id),
            _ => None,
        }
    }

    pub(crate) fn value_type_for_type_query_member_base(
        &self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            let sym_id = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, expr_name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & tsz_binder::symbol_flags::VALUE == 0 {
                return None;
            }
            let mut type_id = {
                let env = self.ctx.type_environment.borrow();
                tsz_solver::TypeResolver::resolve_type_query(
                    &*env,
                    tsz_solver::SymbolRef(sym_id.0),
                    self.ctx.types,
                )
            }
            .or_else(|| self.ctx.symbol_types.get(&sym_id).copied())?;
            if self
                .symbol_is_bare_const_object_literal(sym_id)
                .unwrap_or(false)
            {
                type_id = self.widen_mutable_object_literal_property_types(type_id);
            }
            return (type_id != TypeId::ANY && type_id != TypeId::ERROR).then_some(type_id);
        }

        self.value_property_type_query(expr_name)
    }

    pub(crate) fn import_call_type_reference(&mut self, type_name: NodeIndex) -> Option<TypeId> {
        let (sym_id, target_file_idx, remaining_segments) =
            self.resolve_import_call_member_symbol(type_name)?;
        let mut resolved = self.with_import_target_checker(target_file_idx, |checker| {
            checker.type_reference_symbol_type(sym_id)
        })?;
        resolved = self.materialize_imported_lazy_body(resolved);
        resolved = self.resolve_remaining_import_member_segments(resolved, &remaining_segments)?;
        (resolved != TypeId::ANY && resolved != TypeId::ERROR).then_some(resolved)
    }

    /// True when `expr_name` is the body of a `typeof import("...")[.member]*`
    /// type-query, i.e. an `import("...")` call optionally followed by qualified
    /// names or property accesses.
    ///
    /// This is the inline-typeof-import detection used by [`get_type_from_type_query`]
    /// to route to the namespace-aware resolver instead of falling through to
    /// lowering (which returns `Error` for non-identifier expression names).
    pub(crate) fn is_import_call_typeof_query(&self, expr_name: NodeIndex) -> bool {
        let mut current = expr_name;
        for _ in 0..64 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            match node.kind {
                syntax_kind_ext::CALL_EXPRESSION => {
                    let Some(call) = self.ctx.arena.get_call_expr(node) else {
                        return false;
                    };
                    let Some(callee) = self.ctx.arena.get(call.expression) else {
                        return false;
                    };
                    return callee.kind == SyntaxKind::ImportKeyword as u16;
                }
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    let Some(access) = self.ctx.arena.get_access_expr(node) else {
                        return false;
                    };
                    if access.question_dot_token {
                        return false;
                    }
                    current = access.expression;
                }
                syntax_kind_ext::QUALIFIED_NAME => {
                    let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                        return false;
                    };
                    current = qn.left;
                }
                _ => return false,
            }
        }
        false
    }

    /// Resolve a `typeof import("...")` `TYPE_QUERY` node by reusing the
    /// `CheckerState` namespace builder. The `TypeNodeChecker` holds a
    /// `&mut CheckerContext`, but the namespace-building machinery lives on
    /// `CheckerState`. We wrap the current-file context in a sibling
    /// `CheckerState` (same arena / binder / file index), run the resolver,
    /// and propagate the namespace bookkeeping back to the parent so display
    /// (`typeof import("...")`) and TS2339 receiver formatting stay accurate.
    pub(crate) fn resolve_import_typeof_query_via_state(
        &mut self,
        type_query_idx: NodeIndex,
    ) -> Option<TypeId> {
        let type_query = self
            .ctx
            .arena
            .get(type_query_idx)
            .and_then(|node| self.ctx.arena.get_type_query(node))?;
        let expr_name = type_query.expr_name;
        let current_file_idx = self.ctx.current_file_idx;
        let arena = self.ctx.arena;
        let binder = self.ctx.binder;
        let file_name = arena.source_files.first()?.file_name.clone();
        let mut child = CheckerState {
            ctx: CheckerContext::with_parent_cache(
                arena,
                binder,
                self.ctx.types,
                file_name,
                self.ctx.compiler_options.clone(),
                self.ctx,
            ),
        };
        child.ctx.copy_cross_file_state_from(self.ctx);
        self.ctx.copy_symbol_file_targets_to_attributed(
            &mut child.ctx,
            tsz_common::perf_counters::CheckerCreationReason::ImportType,
        );
        child.ctx.current_file_idx = current_file_idx;
        let resolved = child.resolve_typeof_import_query(expr_name);
        // Merge namespace bookkeeping back so display/TS2339 receiver and any
        // subsequent property access see the same namespace TypeId.
        for (type_id, module_name) in child.ctx.namespace_module_names.drain() {
            self.ctx
                .namespace_module_names
                .entry(type_id)
                .or_insert(module_name);
        }
        self.ctx.merge_symbol_file_targets_from(&child.ctx);
        resolved.filter(|&type_id| type_id != TypeId::ANY && type_id != TypeId::ERROR)
    }

    fn import_call_typeof_value(&self, expr_name: NodeIndex) -> Option<TypeId> {
        let (sym_id, target_file_idx, remaining_segments) =
            self.resolve_import_call_member_symbol(expr_name)?;
        let mut resolved = self.with_import_target_checker(target_file_idx, |checker| {
            checker.get_type_of_symbol(sym_id)
        })?;
        resolved = self.materialize_imported_lazy_body(resolved);
        resolved = self.resolve_remaining_import_member_segments(resolved, &remaining_segments)?;
        (resolved != TypeId::ANY && resolved != TypeId::ERROR).then_some(resolved)
    }

    fn materialize_imported_lazy_body(&self, type_id: TypeId) -> TypeId {
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let Some(body) = self.ctx.definition_store.get_body(def_id) else {
            return type_id;
        };
        let params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        if params.is_empty() {
            self.ctx.register_def_in_envs(def_id, body);
        } else {
            self.ctx
                .register_def_with_params_in_envs(def_id, body, params);
        }
        body
    }

    fn resolve_remaining_import_member_segments(
        &self,
        mut current_type: TypeId,
        segments: &[String],
    ) -> Option<TypeId> {
        for segment in segments {
            current_type = match crate::query_boundaries::property_access::resolve_property_access(
                self.ctx.types,
                current_type,
                segment,
            ) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id,
                    ..
                }
                | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } if type_id != TypeId::ANY && type_id != TypeId::ERROR => type_id,
                _ => return None,
            };
        }
        Some(current_type)
    }

    fn resolve_import_call_member_symbol(
        &self,
        type_name: NodeIndex,
    ) -> Option<(SymbolId, usize, Vec<String>)> {
        let call_idx = self.find_leftmost_import_call(type_name)?;
        let (module_name, _) = self.import_call_module_specifier(call_idx)?;
        let mut segments = self.import_call_member_segments(type_name)?;
        let first_segment = segments.first()?.clone();
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, &module_name)
            .or_else(|| self.ctx.resolve_import_target(&module_name))?;
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.as_str();

        let sym_id = self
            .ctx
            .module_exports_for_module(target_binder, target_file_name)
            .and_then(|exports| exports.get(&first_segment))
            .or_else(|| {
                self.ctx
                    .module_exports_for_module(target_binder, &module_name)
                    .and_then(|exports| exports.get(&first_segment))
            })
            .or_else(|| target_binder.file_locals.get(&first_segment))?;
        target_binder.get_symbol(sym_id)?;
        self.ctx
            .register_symbol_file_target(sym_id, target_file_idx);
        segments.remove(0);
        Some((sym_id, target_file_idx, segments))
    }

    fn with_import_target_checker<R>(
        &self,
        target_file_idx: usize,
        f: impl for<'child> FnOnce(&mut CheckerState<'child>) -> R,
    ) -> Option<R> {
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();
        let mut checker = CheckerState {
            ctx: CheckerContext::with_parent_cache(
                target_arena,
                target_binder,
                self.ctx.types,
                target_file_name,
                self.ctx.compiler_options.clone(),
                self.ctx,
            ),
        };
        checker.ctx.copy_cross_file_state_from(self.ctx);
        self.ctx.copy_symbol_file_targets_to_attributed(
            &mut checker.ctx,
            tsz_common::perf_counters::CheckerCreationReason::ImportType,
        );
        checker.ctx.current_file_idx = target_file_idx;
        let resolved = f(&mut checker);
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        Some(resolved)
    }

    fn find_leftmost_import_call(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        const MAX_DEPTH: usize = 64;
        for _ in 0..MAX_DEPTH {
            let node = self.ctx.arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qualified = self.ctx.arena.get_qualified_name(node)?;
                idx = qualified.left;
            } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.ctx.arena.get_access_expr(node)?;
                if access.question_dot_token {
                    return None;
                }
                idx = access.expression;
            } else if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                let call = self.ctx.arena.get_call_expr(node)?;
                let expr_node = self.ctx.arena.get(call.expression)?;
                return (expr_node.kind == SyntaxKind::ImportKeyword as u16).then_some(idx);
            } else {
                return None;
            }
        }
        None
    }

    fn import_call_module_specifier(&self, call_idx: NodeIndex) -> Option<(String, NodeIndex)> {
        let node = self.ctx.arena.get(call_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        let args = call.arguments.as_ref()?;
        let &first_arg = args.nodes.first()?;
        let literal = self
            .ctx
            .arena
            .get(first_arg)
            .and_then(|arg| self.ctx.arena.get_literal(arg))?;
        Some((literal.text.clone(), first_arg))
    }

    fn import_call_member_segments(&self, mut idx: NodeIndex) -> Option<Vec<String>> {
        let mut reversed = Vec::new();
        loop {
            let node = self.ctx.arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qualified = self.ctx.arena.get_qualified_name(node)?;
                reversed.push(self.property_name_text(qualified.right)?);
                idx = qualified.left;
            } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.ctx.arena.get_access_expr(node)?;
                if access.question_dot_token {
                    return None;
                }
                reversed.push(self.property_name_text(access.name_or_argument)?);
                idx = access.expression;
            } else if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                break;
            } else {
                return None;
            }
        }
        reversed.reverse();
        Some(reversed)
    }

    fn symbol_is_bare_const_object_literal(&self, sym_id: tsz_binder::SymbolId) -> Option<bool> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return Some(false);
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let assertion_expr = self.ctx.arena.skip_parenthesized(decl.initializer);
        if self
            .ctx
            .arena
            .get(assertion_expr)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
            .and_then(|assertion| self.ctx.arena.get(assertion.type_node))
            .is_some_and(|type_node| type_node.kind == SyntaxKind::ConstKeyword as u16)
        {
            return Some(false);
        }
        if self.ctx.arena.get(assertion_expr).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::SATISFIES_EXPRESSION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::TYPE_ASSERTION
            )
        }) {
            return Some(false);
        }

        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        Some(
            self.ctx
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION),
        )
    }

    fn widen_mutable_object_literal_property_types(&self, type_id: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let mut widened_shape = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut widened_shape.properties {
            let widened_read =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop.type_id);
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                prop.write_type,
            );
            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }
            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }

        if changed {
            self.ctx.types.factory().object_with_index(widened_shape)
        } else {
            type_id
        }
    }
}
