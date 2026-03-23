//! Contextual literal types and circular reference detection.

use crate::call_checker::CallableContext;
use crate::class_inheritance::ClassInheritanceChecker;
use crate::query_boundaries::common::{
    TypeTraversalKind, classify_for_traversal, object_shape_for_type,
};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::keyof_inner_type;
use tsz_solver::type_queries::{ContextualLiteralAllowKind, classify_for_contextual_literal};

impl<'a> CheckerState<'a> {
    fn raw_contextual_signature_available(&self, type_id: TypeId) -> bool {
        let helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            type_id,
            self.ctx.compiler_options.no_implicit_any,
        );
        helper.get_this_type_from_marker().is_some()
            || helper.get_this_type().is_some()
            || helper.get_return_type().is_some()
            || helper.get_parameter_type(0).is_some()
            || helper.get_rest_parameter_type(0).is_some()
    }

    pub(crate) fn contextual_type_for_expression(&mut self, type_id: TypeId) -> TypeId {
        // Evaluate union members individually without subtype reduction to
        // preserve literal types (e.g., avoid `string | 'done'` → `string`).
        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
        {
            let mut evaluated_members = Vec::with_capacity(members.len());
            let mut any_changed = false;
            for &member in &members {
                let evaluated = self.contextual_type_for_expression(member);
                if evaluated != member {
                    any_changed = true;
                }
                evaluated_members.push(evaluated);
            }
            if any_changed {
                return self
                    .ctx
                    .types
                    .factory()
                    .union_preserve_members(evaluated_members);
            }
            return type_id;
        }

        // Resolve TypeQuery (typeof) types to their concrete types before
        // contextual typing. Without this, `var x: typeof F = (a) => ...`
        // leaves the contextual type as an unresolved TypeQuery, which the
        // solver's ContextualTypeContext cannot extract parameter types from,
        // causing false TS7006 ("Parameter implicitly has 'any' type").
        {
            use crate::query_boundaries::type_checking_utilities::{
                TypeQueryKind, classify_type_query,
            };
            if matches!(
                classify_type_query(self.ctx.types, type_id),
                TypeQueryKind::TypeQuery(_) | TypeQueryKind::ApplicationWithTypeQuery { .. }
            ) {
                let resolved = self.resolve_type_query_type(type_id);
                if resolved != type_id && resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    return self.contextual_type_for_expression(resolved);
                }
            }
        }

        if let Some(constraint) =
            tsz_solver::type_queries::get_type_parameter_constraint(self.ctx.types, type_id)
            && constraint != type_id
            && constraint != TypeId::UNKNOWN
            && constraint != TypeId::ERROR
        {
            return self.contextual_type_for_expression(constraint);
        }

        // Preserve direct callable shapes as contextual types. Re-evaluating them
        // can simplify contravariant parameter unions inside callback types, e.g.
        // `(value: A | B | C) => U` collapsing to `(value: A) => any` during
        // generic call argument collection for `Array.prototype.map`.
        {
            let db = self.ctx.types.as_type_database();
            if tsz_solver::is_function_type(db, type_id)
                || tsz_solver::visitor::callable_shape_id(db, type_id).is_some()
            {
                return type_id;
            }
        }

        if crate::query_boundaries::state::should_evaluate_contextual_declared_type(
            self.ctx.types,
            type_id,
        ) {
            self.evaluate_type_with_env(type_id)
        } else {
            type_id
        }
    }

    pub(crate) fn contextual_type_option_for_expression(
        &mut self,
        type_id: Option<TypeId>,
    ) -> Option<TypeId> {
        type_id.map(|type_id| self.contextual_type_for_expression(type_id))
    }

    pub(crate) fn contextual_type_option_for_call_argument(
        &mut self,
        type_id: Option<TypeId>,
        arg_idx: NodeIndex,
        callable_ctx: CallableContext,
    ) -> Option<TypeId> {
        self.contextual_type_option_for_call_argument_at(type_id, arg_idx, None, None, callable_ctx)
    }

    pub(crate) fn contextual_type_option_for_call_argument_at(
        &mut self,
        type_id: Option<TypeId>,
        arg_idx: NodeIndex,
        arg_index: Option<usize>,
        arg_count: Option<usize>,
        callable_ctx: CallableContext,
    ) -> Option<TypeId> {
        let type_id = type_id?;
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return Some(self.contextual_type_for_expression(type_id));
        };

        let preserve_raw = matches!(
            arg_node.kind,
            k if k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
        );
        let preserve_raw_object_context = matches!(
            arg_node.kind,
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        ) && (tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, type_id)
            || crate::computation::call_inference::should_preserve_contextual_application_shape(
                self.ctx.types,
                type_id,
            ));
        let needs_resolved_callable_context =
            tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, type_id).is_some()
                || tsz_solver::type_queries::get_index_access_types(self.ctx.types, type_id)
                    .is_some()
                || tsz_solver::type_queries::is_conditional_type(self.ctx.types, type_id)
                || tsz_solver::type_queries::get_type_application(self.ctx.types, type_id)
                    .is_some();

        if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && arg_index.is_some()
            && arg_count.is_some()
            && self.ctx.generic_excess_skip.as_ref().is_some_and(|skip| {
                arg_index.is_some_and(|index| index < skip.len() && skip[index])
            })
            && let Some(arg_index) = arg_index
            && let Some(callable_type) = callable_ctx.callable_type
            && let Some(shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                callable_type,
            )
            && let Some(raw_contextual_type) = shape
                .params
                .get(arg_index)
                .or_else(|| shape.params.last().filter(|param| param.rest))
                .map(|param| param.type_id)
        {
            return Some(raw_contextual_type);
        }

        if preserve_raw_object_context
            || (preserve_raw
                && !needs_resolved_callable_context
                && self.raw_contextual_signature_available(type_id))
        {
            Some(type_id)
        } else {
            Some(self.contextual_type_for_expression(type_id))
        }
    }

    pub(crate) fn contextual_type_allows_literal(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.contextual_type_allows_literal_inner(ctx_type, literal_type, &mut visited)
    }

    pub(crate) fn contextual_type_allows_literal_inner(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if ctx_type == literal_type {
            return true;
        }
        // tsc: literal contextual type allows ALL literals of the same base type.
        if tsz_solver::type_queries::are_same_base_literal_kind(
            self.ctx.types,
            ctx_type,
            literal_type,
        ) {
            return true;
        }
        // tsz's BOOLEAN is intrinsic (not true|false union), so explicit check needed.
        if is_boolean_context_for_boolean_literal(ctx_type, literal_type) {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        // Resolve Lazy(DefId) types before classification.
        if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, ctx_type) {
            // Try type_env first
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            // If not resolved, use centralized relation precondition setup to populate type_env.
            self.ensure_relation_input_ready(ctx_type);
            let resolved = {
                let env = self.ctx.type_env.borrow();
                env.get_def(def_id)
            };
            if let Some(resolved) = resolved
                && resolved != ctx_type
            {
                return self.contextual_type_allows_literal_inner(resolved, literal_type, visited);
            }
            return false;
        }

        // Evaluate KeyOf/IndexAccess to concrete form before classification.
        if tsz_solver::type_queries::is_keyof_type(self.ctx.types, ctx_type)
            || tsz_solver::type_queries::is_index_access_type(self.ctx.types, ctx_type)
            || tsz_solver::type_queries::is_conditional_type(self.ctx.types, ctx_type)
        {
            let evaluated = self.evaluate_type_with_env(ctx_type);
            if evaluated != ctx_type && evaluated != TypeId::ERROR {
                return self.contextual_type_allows_literal_inner(evaluated, literal_type, visited);
            }
        }
        // Generic `keyof` contexts preserve literal arguments.
        if let Some(keyof_inner) = keyof_inner_type(self.ctx.types, ctx_type)
            && (tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, keyof_inner)
                .is_some()
                || tsz_solver::type_queries::is_this_type(self.ctx.types, keyof_inner))
        {
            return true;
        }

        match classify_for_contextual_literal(self.ctx.types, ctx_type) {
            ContextualLiteralAllowKind::Members(members) => members.iter().any(|&member| {
                self.contextual_type_allows_literal_inner(member, literal_type, visited)
            }),
            // Type parameters always allow literal types.
            ContextualLiteralAllowKind::TypeParameter { .. }
            | ContextualLiteralAllowKind::TemplateLiteral => true,
            ContextualLiteralAllowKind::Application => {
                let expanded = self.evaluate_application_type(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::Mapped => {
                let expanded = self.evaluate_type_with_env(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            ContextualLiteralAllowKind::NotAllowed => false,
        }
    }

    /// True for bare type refs (`type A = B`), false for wrapped (`type A = { x: B }`).
    pub(crate) fn is_simple_type_reference(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };

        // Type reference or identifier without structural wrapping
        matches!(
            node.kind,
            k if k == syntax_kind_ext::TYPE_REFERENCE || k == SyntaxKind::Identifier as u16
        )
    }

    /// True for direct circular aliases (`type A = B; type B = A`), false for
    /// structurally wrapped recursion. Marks all aliases on the resolution stack.
    #[allow(clippy::only_used_in_recursion)]
    pub(crate) fn is_direct_circular_reference(
        &mut self,
        sym_id: SymbolId,
        resolved_type: TypeId,
        type_node: NodeIndex,
        in_union_or_intersection: bool,
    ) -> bool {
        // Depth guard: recursive type aliases can cause infinite expansion.
        // Limit via `ctx.circ_ref_depth` (limit 30) to prevent stack overflow.
        if !self.ctx.circ_ref_depth.borrow_mut().enter() {
            return false; // Conservatively: not a direct circular reference
        }

        let result = self.is_direct_circular_reference_inner(
            sym_id,
            resolved_type,
            type_node,
            in_union_or_intersection,
        );
        self.ctx.circ_ref_depth.borrow_mut().leave();
        result
    }

    /// Inner implementation of circular reference detection (after depth guard).
    fn is_direct_circular_reference_inner(
        &mut self,
        sym_id: SymbolId,
        resolved_type: TypeId,
        type_node: NodeIndex,
        in_union_or_intersection: bool,
    ) -> bool {
        // Check if resolved_type is Lazy(DefId) pointing to a type alias in the
        // current resolution chain.
        if let Some(def_id) =
            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved_type)
            && let Some(&target_sym_id) = self.ctx.def_to_symbol.borrow().get(&def_id)
        {
            // Check if the target is in the resolution set (detecting cycles).
            let is_in_resolution_chain = self.ctx.symbol_resolution_set.contains(&target_sym_id);

            // Only flag type alias symbols to avoid false positives for
            // interfaces/classes which can have valid structural recursion.
            let is_type_alias = self
                .ctx
                .binder
                .get_symbol(target_sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0);

            if is_in_resolution_chain && is_type_alias {
                let is_direct =
                    in_union_or_intersection || self.is_simple_type_reference(type_node);

                if is_direct {
                    // Mark all aliases on stack between target and current as circular.
                    let mut found_target = false;
                    for &stack_sym in &self.ctx.symbol_resolution_stack {
                        if stack_sym == target_sym_id {
                            found_target = true;
                        }
                        if found_target {
                            let is_alias = self.ctx.binder.get_symbol(stack_sym).is_some_and(|s| {
                                s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                            });
                            if is_alias {
                                self.ctx.circular_type_aliases.insert(stack_sym);
                            }
                        }
                    }
                    // Always mark the target itself as circular (handles cross-file cycles).
                    self.ctx.circular_type_aliases.insert(target_sym_id);
                }

                return is_direct;
            }
        }

        // For mapped types, check if the constraint references the alias being
        // defined (via keyof or directly).  This catches non-generic self-referencing
        // mapped type aliases like `type Recurse = { [K in keyof Recurse]: Recurse[K] }`.
        if let Some(mapped_info) =
            tsz_solver::type_queries::get_mapped_type(self.ctx.types, resolved_type)
        {
            let constraint = mapped_info.constraint;
            // Check constraint directly and also its keyof inner type
            let refs_to_check: Vec<TypeId> = {
                let mut v = vec![constraint];
                if let Some(inner) = keyof_inner_type(self.ctx.types, constraint) {
                    v.push(inner);
                }
                v
            };
            for ref_type in refs_to_check {
                if let Some(def_id) =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, ref_type)
                    && let Some(&target_sym_id) = self.ctx.def_to_symbol.borrow().get(&def_id)
                    && self.ctx.symbol_resolution_set.contains(&target_sym_id)
                    && self
                        .ctx
                        .binder
                        .get_symbol(target_sym_id)
                        .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
                {
                    // Mark all aliases on stack between target and current as circular.
                    let mut found_target = false;
                    for &stack_sym in &self.ctx.symbol_resolution_stack {
                        if stack_sym == target_sym_id {
                            found_target = true;
                        }
                        if found_target {
                            let is_alias = self.ctx.binder.get_symbol(stack_sym).is_some_and(|s| {
                                s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                            });
                            if is_alias {
                                self.ctx.circular_type_aliases.insert(stack_sym);
                            }
                        }
                    }
                    self.ctx.circular_type_aliases.insert(target_sym_id);
                    return true;
                }
            }
        }

        let evaluated = match classify_for_traversal(self.ctx.types, resolved_type) {
            TypeTraversalKind::Application { .. }
            | TypeTraversalKind::Conditional(_)
            | TypeTraversalKind::Mapped(_)
            | TypeTraversalKind::IndexAccess { .. }
            | TypeTraversalKind::KeyOf(_) => self.evaluate_type_with_resolution(resolved_type),
            _ => resolved_type,
        };
        if evaluated != resolved_type
            && self.is_direct_circular_reference(
                sym_id,
                evaluated,
                type_node,
                in_union_or_intersection,
            )
        {
            return true;
        }

        if let Some((object_type, index_type)) =
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, resolved_type)
        {
            if self.is_direct_circular_reference(sym_id, object_type, type_node, true)
                || self.is_direct_circular_reference(sym_id, index_type, type_node, true)
            {
                return true;
            }

            let resolved_object = self.resolve_lazy_type(object_type);
            if let Some(shape) = object_shape_for_type(self.ctx.types, resolved_object) {
                let index_targets_all_properties = keyof_inner_type(self.ctx.types, index_type)
                    .is_some_and(|inner| {
                        let resolved_inner = self.resolve_lazy_type(inner);
                        resolved_inner == object_type || resolved_inner == resolved_object
                    });

                if index_targets_all_properties {
                    for prop in &shape.properties {
                        if self.is_direct_circular_reference(sym_id, prop.type_id, type_node, true)
                            || self.is_direct_circular_reference(
                                sym_id,
                                prop.write_type,
                                type_node,
                                true,
                            )
                        {
                            return true;
                        }
                    }
                    if let Some(index_sig) = &shape.string_index
                        && self.is_direct_circular_reference(
                            sym_id,
                            index_sig.value_type,
                            type_node,
                            true,
                        )
                    {
                        return true;
                    }
                    if let Some(index_sig) = &shape.number_index
                        && self.is_direct_circular_reference(
                            sym_id,
                            index_sig.value_type,
                            type_node,
                            true,
                        )
                    {
                        return true;
                    }
                } else if let Some(key) =
                    tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
                {
                    let key_text = self.ctx.types.resolve_atom(key);
                    if let Some(prop) = shape
                        .properties
                        .iter()
                        .find(|prop| self.ctx.types.resolve_atom(prop.name) == key_text)
                        && (self.is_direct_circular_reference(
                            sym_id,
                            prop.type_id,
                            type_node,
                            true,
                        ) || self.is_direct_circular_reference(
                            sym_id,
                            prop.write_type,
                            type_node,
                            true,
                        ))
                    {
                        return true;
                    }
                    if let Some(index_sig) = &shape.string_index
                        && self.is_direct_circular_reference(
                            sym_id,
                            index_sig.value_type,
                            type_node,
                            true,
                        )
                    {
                        return true;
                    }
                } else if tsz_solver::type_queries::get_number_literal_value(
                    self.ctx.types,
                    index_type,
                )
                .is_some()
                    && let Some(index_sig) = &shape.number_index
                    && self.is_direct_circular_reference(
                        sym_id,
                        index_sig.value_type,
                        type_node,
                        true,
                    )
                {
                    return true;
                }
            }
        }

        // Also check union/intersection members for circular references.
        // Per TS spec: "A union type directly depends on each of the constituent types."
        if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node, true) {
                    return true;
                }
            }
        }
        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, resolved_type)
        {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node, true) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a non-generic type alias with a mapped type body is circular.
    /// Walks the type alias body AST to find type references that resolve back
    /// to the alias being defined or to another non-generic type alias that
    /// references this one (mutual recursion).
    ///
    /// This covers patterns like:
    /// - `type Recurse = { [K in keyof Recurse]: Recurse[K] }` (self)
    /// - `type A = { [K in keyof B]: B[K] }; type B = { [K in keyof A]: A[K] }` (mutual)
    pub(crate) fn is_non_generic_mapped_type_circular(
        &mut self,
        sym_id: SymbolId,
        type_node: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };
        // Only applies when the body IS a mapped type
        if node.kind != syntax_kind_ext::MAPPED_TYPE {
            return false;
        }
        // Walk the type body AST and check if any type reference resolves to
        // a non-generic type alias that participates in a cycle with sym_id.
        self.ast_contains_circular_type_ref(type_node, sym_id, &mut FxHashSet::default())
    }

    /// Recursive AST walk to find type references that close a cycle back to `target_sym`.
    fn ast_contains_circular_type_ref(
        &mut self,
        node_idx: NodeIndex,
        target_sym: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // If this is an identifier or type reference, check if it resolves to the target
        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::TYPE_REFERENCE
        {
            let ident_idx = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx
                    .arena
                    .get_type_ref(node)
                    .map(|tr| tr.type_name)
                    .unwrap_or(NodeIndex::NONE)
            } else {
                node_idx
            };
            if let Some(ident_node) = self.ctx.arena.get(ident_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(ident_node)
            {
                let name = self.ctx.arena.resolve_identifier_text(ident);
                if let Some(ref_sym_id) = self.ctx.binder.file_locals.get(name) {
                    if ref_sym_id == target_sym {
                        return true;
                    }
                    // For mutual recursion: check if this alias's body references
                    // the target (one hop). Only for non-generic type aliases.
                    if visited.insert(ref_sym_id)
                        && let Some(symbol) = self.ctx.binder.get_symbol(ref_sym_id)
                        && symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                    {
                        // Check if this alias has type parameters (non-generic only)
                        for &decl_idx in &symbol.declarations {
                            if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                && decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                && let Some(type_alias) = self.ctx.arena.get_type_alias(decl_node)
                                && type_alias.type_parameters.is_none()
                                && self.ast_contains_circular_type_ref(
                                    type_alias.type_node,
                                    target_sym,
                                    visited,
                                )
                            {
                                // Mark the intermediate alias as circular too
                                self.ctx.circular_type_aliases.insert(ref_sym_id);
                                return true;
                            }
                        }
                    }
                }
            }
            // For TypeReference nodes, do NOT recurse into type arguments.
            // Generic type application provides structural wrapping that breaks
            // direct circularity (e.g., `type HTML = { [K in 'div']: Block<HTML> }`
            // where `Block<P> = <T>(func: HTML) => {}` is not circular).
            // Identifiers also have no meaningful children to recurse into.
            return false;
        }

        // Recurse into children
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.ast_contains_circular_type_ref(child_idx, target_sym, visited) {
                return true;
            }
        }
        false
    }

    /// Detect cross-file circular type alias cycles via `DefinitionStore` Lazy chain.
    pub(crate) fn is_cross_file_circular_alias(
        &self,
        sym_id: SymbolId,
        alias_type: TypeId,
    ) -> bool {
        let own_def_id = match self.ctx.get_existing_def_id(sym_id) {
            Some(def_id) => def_id,
            None => return false,
        };

        let mut current = alias_type;
        let mut visited = FxHashSet::default();
        visited.insert(own_def_id);

        loop {
            let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, current)
            else {
                return false;
            };

            if def_id == own_def_id {
                return true;
            }
            if !visited.insert(def_id) {
                return false;
            }

            // Verify the target is a type alias (not interface/class which can be recursive)
            if let Some(&target_sym) = self.ctx.def_to_symbol.borrow().get(&def_id) {
                let is_type_alias = self
                    .get_symbol_globally(target_sym)
                    .or_else(|| self.ctx.binder.get_symbol(target_sym))
                    .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0);
                if !is_type_alias {
                    return false;
                }
            }

            let Some(body) = self.ctx.definition_store.get_body(def_id) else {
                return false;
            };
            current = body;
        }
    }

    /// Post-processing: detect cross-file circular type aliases (TS2456).
    pub(crate) fn check_cross_file_circular_type_aliases(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;

        // Collect type alias symbols from the current file's node_symbols.
        let type_alias_syms: Vec<SymbolId> = self
            .ctx
            .binder
            .node_symbols
            .values()
            .copied()
            .filter(|&sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0)
            })
            .collect::<FxHashSet<_>>()
            .into_iter()
            .collect();

        for sym_id in type_alias_syms {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            if symbol.flags & (symbol_flags::ALIAS | symbol_flags::NAMESPACE) != 0 {
                continue;
            }

            // Skip if already detected as circular during compute_type_of_symbol.
            if self.ctx.circular_type_aliases.contains(&sym_id) {
                continue;
            }

            // Get the cached resolved type for this symbol.
            let Some(&resolved) = self.ctx.symbol_types.get(&sym_id) else {
                continue;
            };

            // Only check if the resolved type is a Lazy reference (unresolved
            // cross-file placeholder).
            if tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved).is_none() {
                continue;
            }

            if self.is_cross_file_circular_alias(sym_id, resolved) {
                // Get the symbol name for the diagnostic message.
                let name = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .map(|s| s.escaped_name.to_string())
                    .unwrap_or_default();

                let message = format_message(
                    diagnostic_messages::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                    &[&name],
                );

                // Find the type alias declaration node for error positioning.
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    for &decl_idx in &symbol.declarations {
                        if let Some(node) = self.ctx.arena.get(decl_idx)
                            && node.kind
                                == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                            && let Some(ta) = self.ctx.arena.get_type_alias(node)
                        {
                            // Verify the name matches (prevent NodeIndex collision).
                            let name_matches = self
                                .ctx
                                .arena
                                .get(ta.name)
                                .and_then(|n| self.ctx.arena.get_identifier(n))
                                .map(|ident| {
                                    self.ctx.arena.resolve_identifier_text(ident)
                                        == symbol.escaped_name.as_str()
                                })
                                .unwrap_or(false);
                            if name_matches {
                                self.error_at_node(
                                    ta.name,
                                    &message,
                                    diagnostic_codes::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                                );
                                // Mark as circular so we don't re-emit.
                                self.ctx.circular_type_aliases.insert(sym_id);

                                // Update the symbol's type to `any` (same as inline
                                // circular detection).
                                self.ctx
                                    .symbol_types
                                    .insert(sym_id, tsz_solver::TypeId::ANY);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Report TS2310 for a class/interface whose instantiated base type needs
    /// the current symbol's structure to resolve.
    pub(crate) fn report_recursive_base_type_for_symbol(&mut self, sym_id: SymbolId) {
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        checker.error_recursive_base_type_for_symbol(sym_id);
    }

    /// True when resolving `type_id` requires `target_sym`'s structure (meta-type ops).
    pub(crate) fn type_requires_structure_of_symbol(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
    ) -> bool {
        use tsz_solver::recursion::RecursionProfile;
        let mut guard =
            tsz_solver::recursion::RecursionGuard::with_profile(RecursionProfile::ShallowTraversal);
        self.type_requires_structure_of_symbol_inner(type_id, target_sym, false, false, &mut guard)
    }

    /// Like above but skips member types — for TS2310 base-type cycle detection.
    pub(crate) fn type_requires_structure_of_symbol_for_base_type(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
    ) -> bool {
        use tsz_solver::recursion::RecursionProfile;
        let mut guard =
            tsz_solver::recursion::RecursionGuard::with_profile(RecursionProfile::ShallowTraversal);
        self.type_requires_structure_of_symbol_inner(type_id, target_sym, false, true, &mut guard)
    }

    fn type_requires_structure_of_symbol_inner(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
        requires_structure: bool,
        skip_members: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<(TypeId, bool)>,
    ) -> bool {
        let key = (type_id, requires_structure);
        if !guard.enter(key).is_entered() {
            return false;
        }
        let result = self.type_requires_structure_of_symbol_body(
            type_id,
            target_sym,
            requires_structure,
            skip_members,
            guard,
        );
        guard.leave(key);
        result
    }

    fn type_requires_structure_of_symbol_body(
        &mut self,
        type_id: TypeId,
        target_sym: SymbolId,
        requires_structure: bool,
        skip_members: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<(TypeId, bool)>,
    ) -> bool {
        if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, type_id)
            && self.ctx.def_to_symbol_id_with_fallback(def_id) == Some(target_sym)
        {
            return requires_structure;
        }
        if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
        {
            let mut resolved_alias = self
                .resolve_and_insert_def_type(def_id)
                .or_else(|| Some(self.get_type_of_symbol(sym_id)))
                .unwrap_or(type_id);
            if resolved_alias == type_id
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                for &decl_idx in &symbol.declarations {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                        continue;
                    }
                    let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                        continue;
                    };
                    let (_params, updates) = self.push_type_parameters(&alias.type_parameters);
                    resolved_alias = self.get_type_from_type_node(alias.type_node);
                    self.pop_type_parameters(updates);
                    break;
                }
            }
            if resolved_alias != type_id
                && self.type_requires_structure_of_symbol_inner(
                    resolved_alias,
                    target_sym,
                    requires_structure,
                    skip_members,
                    guard,
                )
            {
                return true;
            }
        }
        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
            && let Some(body) = self.ctx.definition_store.get_body(def_id)
            && body != type_id
            && self.type_requires_structure_of_symbol_inner(
                body,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            )
        {
            return true;
        }
        if requires_structure
            && self
                .ctx
                .definition_store
                .find_def_for_type(type_id)
                .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                == Some(target_sym)
        {
            return true;
        }

        let resolved_lazy = self.resolve_lazy_type(type_id);
        if resolved_lazy != type_id
            && self.type_requires_structure_of_symbol_inner(
                resolved_lazy,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            )
        {
            return true;
        }

        // Performance guard: skip expensive evaluation after 200 unique type visits.
        // Deep generic/mapped-type chains (styled-components) cause hangs otherwise.
        let evaluated = if guard.iterations() <= 200 {
            match classify_for_traversal(self.ctx.types, type_id) {
                TypeTraversalKind::Application { .. }
                | TypeTraversalKind::Conditional(_)
                | TypeTraversalKind::Mapped(_)
                | TypeTraversalKind::IndexAccess { .. }
                | TypeTraversalKind::KeyOf(_) => self.evaluate_type_with_resolution(type_id),
                _ => type_id,
            }
        } else {
            type_id
        };
        if evaluated != type_id
            && self.type_requires_structure_of_symbol_inner(
                evaluated,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            )
        {
            return true;
        }

        match classify_for_traversal(self.ctx.types, type_id) {
            TypeTraversalKind::Terminal
            | TypeTraversalKind::Lazy(_)
            | TypeTraversalKind::TypeQuery(_)
            | TypeTraversalKind::SymbolRef(_) => false,
            TypeTraversalKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if requires_structure && shape.symbol == Some(target_sym) {
                    return true;
                }
                // skip_members: don't walk into properties (TS2310 base-type check)
                if skip_members {
                    return false;
                }
                shape.properties.iter().any(|prop| {
                    self.type_requires_structure_of_symbol_inner(
                        prop.type_id,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    ) || self.type_requires_structure_of_symbol_inner(
                        prop.write_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.string_index.as_ref().is_some_and(|sig| {
                    self.type_requires_structure_of_symbol_inner(
                        sig.key_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    ) || self.type_requires_structure_of_symbol_inner(
                        sig.value_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.number_index.as_ref().is_some_and(|sig| {
                    self.type_requires_structure_of_symbol_inner(
                        sig.key_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    ) || self.type_requires_structure_of_symbol_inner(
                        sig.value_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Members(members) => members.into_iter().any(|member| {
                self.type_requires_structure_of_symbol_inner(
                    member,
                    target_sym,
                    requires_structure,
                    skip_members,
                    guard,
                )
            }),
            TypeTraversalKind::Array(elem) => self.type_requires_structure_of_symbol_inner(
                elem,
                target_sym,
                requires_structure,
                skip_members,
                guard,
            ),
            TypeTraversalKind::Tuple(list_id) => {
                self.ctx.types.tuple_list(list_id).iter().any(|elem| {
                    self.type_requires_structure_of_symbol_inner(
                        elem.type_id,
                        target_sym,
                        requires_structure,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Function(shape_id) => {
                if skip_members {
                    return false;
                }
                let shape = self.ctx.types.function_shape(shape_id);
                shape.params.iter().any(|param| {
                    self.type_requires_structure_of_symbol_inner(
                        param.type_id,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || self.type_requires_structure_of_symbol_inner(
                    shape.return_type,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || shape.this_type.is_some_and(|this_type| {
                    self.type_requires_structure_of_symbol_inner(
                        this_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Callable(shape_id) => {
                if skip_members {
                    return false;
                }
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.call_signatures.iter().any(|sig| {
                    sig.params.iter().any(|param| {
                        self.type_requires_structure_of_symbol_inner(
                            param.type_id,
                            target_sym,
                            false,
                            skip_members,
                            guard,
                        )
                    }) || self.type_requires_structure_of_symbol_inner(
                        sig.return_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.construct_signatures.iter().any(|sig| {
                    sig.params.iter().any(|param| {
                        self.type_requires_structure_of_symbol_inner(
                            param.type_id,
                            target_sym,
                            false,
                            skip_members,
                            guard,
                        )
                    }) || self.type_requires_structure_of_symbol_inner(
                        sig.return_type,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                }) || shape.properties.iter().any(|prop| {
                    self.type_requires_structure_of_symbol_inner(
                        prop.type_id,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::TypeParameter {
                constraint,
                default,
            } => {
                constraint.is_some_and(|constraint| {
                    self.type_requires_structure_of_symbol_inner(
                        constraint,
                        target_sym,
                        requires_structure,
                        skip_members,
                        guard,
                    )
                }) || default.is_some_and(|default| {
                    self.type_requires_structure_of_symbol_inner(
                        default,
                        target_sym,
                        requires_structure,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Application { base, args, .. } => {
                self.type_requires_structure_of_symbol_inner(
                    base,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || args.iter().any(|&arg| {
                    self.type_requires_structure_of_symbol_inner(
                        arg,
                        target_sym,
                        false,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.type_requires_structure_of_symbol_inner(
                    cond.check_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    cond.extends_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    cond.true_type,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    cond.false_type,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                )
            }
            TypeTraversalKind::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_requires_structure_of_symbol_inner(
                        constraint,
                        target_sym,
                        true,
                        skip_members,
                        guard,
                    )
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_requires_structure_of_symbol_inner(
                        default,
                        target_sym,
                        true,
                        skip_members,
                        guard,
                    )
                }) || self.type_requires_structure_of_symbol_inner(
                    mapped.constraint,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    mapped.template,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ) || mapped.name_type.is_some_and(|name_type| {
                    self.type_requires_structure_of_symbol_inner(
                        name_type,
                        target_sym,
                        true,
                        skip_members,
                        guard,
                    )
                })
            }
            TypeTraversalKind::IndexAccess {
                object: object_type,
                index: index_type,
            } => {
                self.type_requires_structure_of_symbol_inner(
                    object_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ) || self.type_requires_structure_of_symbol_inner(
                    index_type,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                )
            }
            TypeTraversalKind::TemplateLiteral(types) => types.into_iter().any(|type_id| {
                self.type_requires_structure_of_symbol_inner(
                    type_id,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                )
            }),
            TypeTraversalKind::KeyOf(inner) | TypeTraversalKind::Readonly(inner) => self
                .type_requires_structure_of_symbol_inner(
                    inner,
                    target_sym,
                    true,
                    skip_members,
                    guard,
                ),
            TypeTraversalKind::StringIntrinsic(type_arg) => self
                .type_requires_structure_of_symbol_inner(
                    type_arg,
                    target_sym,
                    false,
                    skip_members,
                    guard,
                ),
        }
    }

    /// Report TS2313 for mapped constraints in instantiated type alias cycles.
    pub(crate) fn report_instantiated_type_alias_mapped_constraint_cycles(
        &mut self,
        alias_sym_id: SymbolId,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args: &[TypeId],
        current_sym: SymbolId,
    ) {
        let Some(symbol) = self.get_symbol_globally(alias_sym_id) else {
            return;
        };
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return;
        }

        let declarations = symbol.declarations.clone();
        let bindings: FxHashMap<String, TypeId> = type_params
            .iter()
            .zip(type_args.iter().copied())
            .map(|(param, arg)| (self.ctx.types.resolve_atom(param.name), arg))
            .collect();
        let updates: Vec<(String, Option<TypeId>)> = bindings
            .iter()
            .map(|(name, &arg)| {
                let previous = self.ctx.type_parameter_scope.insert(name.clone(), arg);
                (name.clone(), previous)
            })
            .collect();

        for &decl_idx in &declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                continue;
            }
            let Some(alias) = self.ctx.arena.get_type_alias(node) else {
                continue;
            };
            self.report_instantiated_mapped_constraint_cycles_in_node(
                alias.type_node,
                current_sym,
                &bindings,
            );
        }

        for (name, previous) in updates.into_iter().rev() {
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(previous) = previous {
                self.ctx.type_parameter_scope.insert(name, previous);
            }
        }
    }

    fn report_instantiated_mapped_constraint_cycles_in_node(
        &mut self,
        node_idx: NodeIndex,
        current_sym: SymbolId,
        bindings: &FxHashMap<String, TypeId>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::MAPPED_TYPE {
            let Some(mapped) = self.ctx.arena.get_mapped_type(node) else {
                return;
            };
            let Some(type_param_node) = self.ctx.arena.get(mapped.type_parameter) else {
                return;
            };
            let Some(type_param) = self.ctx.arena.get_type_parameter(type_param_node) else {
                return;
            };

            let mut local_update = None;
            if let Some(name_node) = self.ctx.arena.get(type_param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let atom = self.ctx.types.intern_string(&name);
                let provisional = self
                    .ctx
                    .types
                    .factory()
                    .type_param(tsz_solver::TypeParamInfo {
                        name: atom,
                        constraint: None,
                        default: None,
                        is_const: false,
                    });
                let previous = self
                    .ctx
                    .type_parameter_scope
                    .insert(name.clone(), provisional);
                local_update = Some((name, previous));
            }

            if type_param.constraint != NodeIndex::NONE {
                let constraint_type = self.get_type_from_type_node(type_param.constraint);
                let syntactic_cycle = self.constraint_mentions_instantiated_cycle_target(
                    type_param.constraint,
                    current_sym,
                    bindings,
                    &mut FxHashSet::default(),
                );
                if (self.type_requires_structure_of_symbol(constraint_type, current_sym)
                    || syntactic_cycle)
                    && let Some(name_node) = self.ctx.arena.get(type_param.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    self.error_at_node_msg(
                        type_param.constraint,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                        &[ident.escaped_text.as_str()],
                    );
                }
            }

            for child_idx in self.ctx.arena.get_children(node_idx) {
                self.report_instantiated_mapped_constraint_cycles_in_node(
                    child_idx,
                    current_sym,
                    bindings,
                );
            }

            if let Some((name, previous)) = local_update {
                self.ctx.type_parameter_scope.remove(&name);
                if let Some(previous) = previous {
                    self.ctx.type_parameter_scope.insert(name, previous);
                }
            }
            return;
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.report_instantiated_mapped_constraint_cycles_in_node(
                child_idx,
                current_sym,
                bindings,
            );
        }
    }

    fn constraint_mentions_instantiated_cycle_target(
        &mut self,
        node_idx: NodeIndex,
        current_sym: SymbolId,
        bindings: &FxHashMap<String, TypeId>,
        shadowed_params: &mut FxHashSet<String>,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(node)
            && !shadowed_params.contains(ident.escaped_text.as_str())
            && let Some(&arg) = bindings.get(ident.escaped_text.as_str())
        {
            use tsz_solver::recursion::RecursionProfile;
            let mut guard = tsz_solver::recursion::RecursionGuard::with_profile(
                RecursionProfile::ShallowTraversal,
            );
            return self.type_requires_structure_of_symbol_inner(
                arg,
                current_sym,
                true,
                false,
                &mut guard,
            );
        }

        let mut inserted_shadow = None;
        if node.kind == syntax_kind_ext::MAPPED_TYPE
            && let Some(mapped) = self.ctx.arena.get_mapped_type(node)
            && let Some(type_param_node) = self.ctx.arena.get(mapped.type_parameter)
            && let Some(type_param) = self.ctx.arena.get_type_parameter(type_param_node)
            && let Some(name_node) = self.ctx.arena.get(type_param.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.clone();
            if shadowed_params.insert(name.clone()) {
                inserted_shadow = Some(name);
            }
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.constraint_mentions_instantiated_cycle_target(
                child_idx,
                current_sym,
                bindings,
                shadowed_params,
            ) {
                if let Some(name) = inserted_shadow {
                    shadowed_params.remove(&name);
                }
                return true;
            }
        }

        if let Some(name) = inserted_shadow {
            shadowed_params.remove(&name);
        }

        false
    }
}

/// Check if contextual type is `boolean` and the literal is a boolean literal.
fn is_boolean_context_for_boolean_literal(ctx_type: TypeId, literal_type: TypeId) -> bool {
    if ctx_type != TypeId::BOOLEAN
        && ctx_type != TypeId::BOOLEAN_TRUE
        && ctx_type != TypeId::BOOLEAN_FALSE
    {
        return false;
    }
    literal_type == TypeId::BOOLEAN_TRUE || literal_type == TypeId::BOOLEAN_FALSE
}
