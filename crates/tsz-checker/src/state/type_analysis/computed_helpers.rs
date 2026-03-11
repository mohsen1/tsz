//! Contextual literal types, circular reference detection, and private property access.

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

    pub(crate) fn contextual_literal_type(&mut self, literal_type: TypeId) -> Option<TypeId> {
        let ctx_type = self.ctx.contextual_type?;
        self.contextual_type_allows_literal(ctx_type, literal_type)
            .then_some(literal_type)
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
        // tsc rule: "If the contextual type is a literal type, we consider this
        // a literal context for ALL literals of the same base type."
        // e.g., contextual type "A" allows literal "f" because both are string literals.
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
                let expanded = self.evaluate_mapped_type_with_resolution(ctx_type);
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
        // Depth guard: this function recurses through union/intersection members
        // and evaluated type expansions. For recursive type aliases like
        // `type N<T, K> = T | { [P in K]: N<T, K> }[K]`, evaluation can produce
        // a union containing N again, and the two TypeIds alternate indefinitely.
        // A depth limit of 30 prevents stack overflow while allowing legitimate
        // shallow circularity detection.
        thread_local! {
            static CIRC_REF_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        }
        const MAX_CIRC_REF_DEPTH: u32 = 30;

        let depth = CIRC_REF_DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v
        });
        if depth >= MAX_CIRC_REF_DEPTH {
            CIRC_REF_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return false; // Conservatively: not a direct circular reference
        }

        let result = self.is_direct_circular_reference_inner(
            sym_id,
            resolved_type,
            type_node,
            in_union_or_intersection,
        );
        CIRC_REF_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
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
                    // Always mark the target itself as circular.  For same-file
                    // cycles, the target is on the stack and already marked above.
                    // For cross-file cycles, the target comes from the parent's
                    // resolution set but is NOT on this checker's stack — mark it
                    // explicitly so the parent can detect circularity after the
                    // delegation returns.
                    self.ctx.circular_type_aliases.insert(target_sym_id);
                }

                return is_direct;
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

    /// Detect cross-file circular type alias cycles via `DefinitionStore` Lazy chain.
    pub(crate) fn is_cross_file_circular_alias(
        &self,
        sym_id: SymbolId,
        alias_type: TypeId,
    ) -> bool {
        let own_def_id = match self.ctx.symbol_to_def.borrow().get(&sym_id).copied() {
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

    pub(crate) fn report_private_identifier_outside_class(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let class_name = self
            .get_declaring_class_name_for_private_member(object_type, property_name)
            .unwrap_or_else(|| "the class".to_string());
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
            &[property_name, &class_name],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
        );
    }

    pub(crate) fn report_private_identifier_shadowed(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_string = self
            .get_class_name_from_type(object_type)
            .unwrap_or_else(|| "the type".to_string());
        let message = format_message(
            diagnostic_messages::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
            &[property_name, &type_string],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
        );
    }

    // Resolve a typeof type reference to its structural type.
    //
    // This function resolves `typeof X` type queries to the actual type of `X`.
    // This is useful for type operations where we need the structural type rather
    // than the type query itself.
    // **TypeQuery Resolution:**
    // - **TypeQuery**: `typeof X` → get the type of symbol X
    // - **Other types**: Return unchanged (not a typeof query)
    //
    // **Use Cases:**
    // - Assignability checking (need actual type, not typeof reference)
    // - Type comparison (typeof X should be compared to X's type)
    // - Generic constraint evaluation
    pub(crate) fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &tsz_parser::parser::node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        use tsz_solver::operations::property::PropertyAccessResult;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        // Mark the private identifier symbol as referenced for unused-variable tracking.
        // Private identifier accesses (`this.#foo`) go through this path (not
        // `check_property_accessibility`), so reference tracking must happen here.
        // Without this, ES private members accessed via `this.#foo` would be falsely
        // reported as unused (TS6133).
        for &sym_id in &symbols {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }

        // NOTE: Do NOT emit TS18016 here for property access expressions.
        // `obj.#prop` is always valid syntax — the private identifier in a property
        // access position is grammatically correct. TSC only emits TS18016 for truly
        // invalid positions (object literals, standalone expressions). For property
        // access, the error is always semantic (TS18013: can't access private member),
        // which is handled below based on the object's type.

        // Evaluate for type checking but preserve original for error messages
        // This preserves nominal identity (e.g., D<string>) in error messages
        let is_original_unknown = object_type == TypeId::UNKNOWN;
        let original_object_type = object_type;
        let object_type = self.evaluate_application_type(object_type);
        let emit_unknown_on_expression = if is_original_unknown && saw_class_scope {
            self.error_is_of_type_unknown(access.expression)
        } else {
            false
        };

        // NOTE: Do NOT resolve Lazy class types to constructor type here.
        // Static private member access (e.g., `C.#method()`) is handled later at the
        // member_is_static check below, which correctly only converts to constructor
        // type when the accessed member is actually static.

        if emit_unknown_on_expression && !symbols.is_empty() {
            return TypeId::ERROR;
        }

        // Property access on `never` returns `never` (bottom type propagation).
        // TSC does not emit TS18050 for property access on `never` — the result is
        // simply `never`, which allows exhaustive narrowing patterns to work correctly.
        if object_type == TypeId::NEVER {
            self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            return TypeId::NEVER;
        }

        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
            }
            return TypeId::ERROR;
        };

        // If `symbols.is_empty()`, the private identifier was not declared in any enclosing lexical class scope.
        // Therefore, this access is invalid, regardless of whether the object type actually has the property.
        if symbols.is_empty() {
            let resolved_type = self.resolve_type_for_property_access(object_type_for_check);
            let is_any_like = resolved_type == TypeId::ANY
                || resolved_type == TypeId::UNKNOWN
                || resolved_type == TypeId::ERROR;

            if is_any_like {
                if emit_unknown_on_expression {
                    // TSC can still emit TS2339 for undeclared private names even when
                    // `unknown` diagnostics are emitted (e.g., `x.#bar` where x: unknown).
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!("Property '{property_name}' does not exist on type 'any'."),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                    if resolved_type == TypeId::ERROR {
                        return TypeId::ANY;
                    }
                    return TypeId::ERROR;
                }
                // TSC special case: for any-like types, private names can't be looked up
                // dynamically. If we're outside any class body, emit TS18016. If inside a class
                // body (but the private name isn't declared there), emit TS2339.
                if !saw_class_scope {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        "Private identifiers are not allowed outside class bodies.",
                        diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                    );
                } else {
                    // For private identifiers on any-like types inside a class, tsc emits
                    // TS2339 directly (unlike regular properties which are suppressed on `any`).
                    // Private names are nominally scoped, so `any` doesn't satisfy them.
                    let type_str = if resolved_type == TypeId::ANY {
                        "any"
                    } else if resolved_type == TypeId::UNKNOWN {
                        "unknown"
                    } else {
                        "error"
                    };
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        name_idx,
                        &format!(
                            "Property '{property_name}' does not exist on type '{type_str}'.",
                        ),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
            } else {
                // For concrete types, check if the property actually exists on the type.
                // If found: TS18013 (property exists but not accessible from outside its class).
                // If not found: TS2339 (property does not exist on type).
                let mut found = false;

                use tsz_solver::operations::property::PropertyAccessResult;
                match self
                    .ctx
                    .types
                    .property_access_type(resolved_type, &property_name)
                {
                    PropertyAccessResult::Success { .. } => {
                        found = true;
                    }
                    _ => {
                        if let Some(shape) =
                            crate::query_boundaries::state::type_analysis::callable_shape_for_type(
                                self.ctx.types,
                                resolved_type,
                            )
                        {
                            let prop_atom = self.ctx.types.intern_string(&property_name);
                            for prop in &shape.properties {
                                if prop.name == prop_atom {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if found {
                    // Property exists, but we are not in the declaring scope (TS18013)
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                    );
                } else {
                    // TS2339: Property does not exist
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                }
            }
            return TypeId::ERROR;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                } else {
                    self.report_private_identifier_outside_class(
                        name_idx,
                        &property_name,
                        original_object_type,
                    );
                }
                return TypeId::ERROR;
            }
        };

        if object_type_for_check == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_check == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        if object_type_for_check == TypeId::UNKNOWN {
            if self.error_is_of_type_unknown(access.expression) {
                return TypeId::ERROR;
            }
            return TypeId::ANY;
        }

        // Resolve Lazy class references to their constructor types for STATIC private members.
        //
        // When a class type is referenced during its own type construction (e.g., in a static
        // field initializer `static s = C.#method()`), the identifier resolves to
        // `Lazy(class_def_id)` — a placeholder inserted to break circular resolution. This
        // Lazy type would otherwise resolve to the *instance* type (via
        // `resolve_and_insert_def_type`), causing the compatibility check to fail when the
        // private member is static (whose declaring type is the constructor type).
        //
        // Only apply this resolution for static members; for instance members the Lazy
        // resolves to the instance type which is correct.
        let member_is_static = self.ctx.binder.get_symbol(symbols[0]).is_some_and(|sym| {
            sym.declarations
                .iter()
                .any(|&decl_idx| self.class_member_is_static(decl_idx))
        });
        let object_type_for_check = if member_is_static
            && let Some(def_id) =
                tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, object_type_for_check)
            && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & tsz_binder::symbol_flags::CLASS != 0
        {
            // Resolve to the constructor type. Use the class_constructor_type_cache to
            // avoid triggering further recursion if the constructor is already built.
            let class_decl = self.get_class_declaration_from_symbol(sym_id);
            if let Some(class_idx) = class_decl
                && let Some(&ctor_type) = self.ctx.class_constructor_type_cache.get(&class_idx)
            {
                ctor_type
            } else {
                self.get_type_of_symbol(sym_id)
            }
        } else {
            object_type_for_check
        };

        // For private member access, use nominal typing based on private brand.
        // If both types have the same private brand, they're from the same class
        // declaration and the access should be allowed.
        let types_compatible =
            if self.types_have_same_private_brand(object_type_for_check, declaring_type) {
                true
            } else {
                self.is_assignable_to(object_type_for_check, declaring_type)
            };

        if !types_compatible {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .is_some_and(|ty| {
                        if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
            });
            if shadowed {
                self.report_private_identifier_shadowed(
                    name_idx,
                    &property_name,
                    original_object_type,
                );
                return TypeId::ERROR;
            }

            // Use original_object_type to preserve nominal identity (e.g., D<string>)
            self.error_property_not_exist_at(&property_name, original_object_type, name_idx);
            return TypeId::ERROR;
        }

        let declaring_type = self.resolve_type_for_property_access(declaring_type);
        let mut result_type = match self
            .ctx
            .types
            .property_access_type(declaring_type, &property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                write_type,
                from_index_signature,
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    // Use original_object_type to preserve nominal identity (e.g., D<string>)
                    self.error_property_not_exist_at(
                        &property_name,
                        original_object_type,
                        name_idx,
                    );
                    return TypeId::ERROR;
                }
                // In write context (assignment target), use the setter parameter type
                // instead of the read type. For setter-only accessors (no getter),
                // the read type is `undefined` but assignments should check against
                // the setter's parameter type.
                if self.ctx.skip_flow_narrowing {
                    write_type.unwrap_or(type_id)
                } else if type_id == TypeId::UNDEFINED && write_type.is_some() {
                    // TS2806: Reading from a private setter-only accessor.
                    // The property has a setter but no getter, so reading is invalid.
                    // Report at the full property access expression (idx), not just the name,
                    // to match tsc's error location.
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        idx,
                        diagnostic_messages::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER,
                        diagnostic_codes::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER,
                    );
                    return TypeId::ERROR;
                } else {
                    type_id
                }
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If we got here, we already resolved the symbol, so the private field exists.
                // The solver might not find it due to type encoding issues.
                // FALLBACK: Try to manually find the property in the callable type
                if let Some(shape) =
                    crate::query_boundaries::state::type_analysis::callable_shape_for_type(
                        self.ctx.types,
                        declaring_type,
                    )
                {
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                factory.union(vec![prop.type_id, TypeId::UNDEFINED])
                            } else {
                                prop.type_id
                            };
                        }
                    }
                }
                // Property not found even in fallback, return ANY for type recovery
                TypeId::ANY
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS18046: 'x' is of type 'unknown'.
                // Report on the expression, not the property name.
                // Without strictNullChecks, unknown is treated like any.
                if self.error_is_of_type_unknown(name_idx) {
                    TypeId::ERROR
                } else {
                    TypeId::ANY
                }
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = factory.union(vec![result_type, TypeId::UNDEFINED]);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }

    /// Check if a symbol is a type-only export (excludable from namespace value type).
    pub(crate) fn is_type_only_export_symbol(&self, sym_id: SymbolId) -> bool {
        // Use cross-file lookup first to avoid SymbolId collisions across binders.
        let symbol = self.get_cross_file_symbol(sym_id);

        let Some(symbol) = symbol else {
            return false;
        };

        if !symbol.is_type_only {
            return false;
        }

        // If the symbol has ALIAS + VALUE flags, is_type_only came from an
        // `import type` alias that merged with a value declaration. The value
        // export is not type-only.
        use tsz_binder::symbol_flags;
        if symbol.flags & symbol_flags::ALIAS != 0 && symbol.flags & symbol_flags::VALUE != 0 {
            return false;
        }

        true
    }

    /// Check if an export symbol has no value component (type-only).
    pub(crate) fn export_symbol_has_no_value(&self, sym_id: SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        let lib_binders = self.get_lib_binders();

        // Use get_cross_file_symbol first (same as is_type_only_export_symbol)
        // to correctly resolve symbols from other files' binders.
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return false;
        };

        let flags = symbol.flags;

        // If the symbol has VALUE flag, check for the special case of modules.
        // Our binder always sets VALUE_MODULE | NAMESPACE_MODULE for all
        // modules. We need to check if the module is actually instantiated.
        if (flags & symbol_flags::VALUE) != 0 {
            // For module/namespace symbols, check if all exports are type-only.
            // An uninstantiated module has no runtime value even with VALUE_MODULE.
            if (flags & symbol_flags::VALUE_MODULE) != 0
                && (flags & symbol_flags::NAMESPACE_MODULE) != 0
                && self.is_module_uninstantiated(sym_id)
            {
                return true;
            }
            return false;
        }

        // NAMESPACE_MODULE without VALUE_MODULE
        if (flags & symbol_flags::NAMESPACE_MODULE) != 0 {
            return true;
        }

        // If the symbol has TYPE flag but no VALUE flag, it's type-only
        // (type alias, interface, etc.)
        if (flags & symbol_flags::TYPE) != 0 {
            return true;
        }

        // For alias-only symbols (ALIAS without TYPE or VALUE), resolve the
        // target to check its flags
        if flags & symbol_flags::ALIAS != 0 {
            let mut visited = Vec::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited) {
                let target_sym = self
                    .get_cross_file_symbol(target)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(target, &lib_binders));
                if let Some(target_sym) = target_sym {
                    let tf = target_sym.flags;
                    if (tf & symbol_flags::VALUE) != 0 {
                        // Check uninstantiated module in alias target too
                        if (tf & symbol_flags::VALUE_MODULE) != 0
                            && (tf & symbol_flags::NAMESPACE_MODULE) != 0
                            && self.is_module_uninstantiated(target)
                        {
                            return true;
                        }
                        return false;
                    }
                    if (tf & symbol_flags::NAMESPACE_MODULE) != 0 {
                        return true;
                    }
                    return (tf & symbol_flags::TYPE) != 0;
                }
            }
        }

        false
    }

    /// Check if a module/namespace has only type-only exports (uninstantiated).
    fn is_module_uninstantiated(&self, sym_id: SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return false;
        };
        let Some(exports) = &symbol.exports else {
            // No exports → uninstantiated
            return true;
        };
        for (_, &export_sym_id) in exports.iter() {
            let export_sym = self.get_cross_file_symbol(export_sym_id).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(export_sym_id, &lib_binders)
            });
            let Some(export_sym) = export_sym else {
                continue;
            };
            let ef = export_sym.flags;
            // Has a non-module value → instantiated
            if (ef & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE)) != 0 {
                return false;
            }
            // Nested module: recursively check
            if (ef & symbol_flags::VALUE_MODULE) != 0
                && !self.is_module_uninstantiated(export_sym_id)
            {
                return false;
            }
        }
        true
    }

    /// Check if a named export was reached through a `export type *` wildcard chain.
    pub(crate) fn is_export_from_type_only_wildcard(
        &self,
        module_name: &str,
        export_name: &str,
    ) -> bool {
        // Resolve the target file for this module specifier
        let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };
        // Get the canonical file name used as key in the target binder's data structures
        let target_file_name = self
            .ctx
            .get_arena_for_file(target_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str());
        let Some(file_name) = target_file_name else {
            return false;
        };

        // Use the binder's re-export resolution which tracks type-only status
        // through wildcard chains
        if let Some((sym_id, true)) =
            target_binder.resolve_import_with_reexports_type_only(file_name, export_name)
        {
            // The binder's type-only flag conflates chain-level type-only
            // (from `export type *`) with symbol-level type-only (from
            // `import type { A }` merged with a value declaration).
            // When the resolved symbol has both ALIAS and VALUE flags, the
            // is_type_only came from a merged import-type + value declaration,
            // NOT from the wildcard chain. Don't treat it as type-only here;
            // the caller's is_type_only_export_symbol already handles that case.
            use tsz_binder::symbol_flags;
            if let Some(sym) = target_binder.symbols.get(sym_id)
                && sym.flags & symbol_flags::ALIAS != 0
                && sym.flags & symbol_flags::VALUE != 0
            {
                return false;
            }
            true
        } else {
            false
        }
    }

    /// Returns true if `sym_id` is a merged interface+value symbol.
    pub(crate) fn is_merged_interface_value_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let flags = symbol.flags;
        use tsz_binder::symbol_flags;
        (flags & symbol_flags::INTERFACE) != 0 && (flags & symbol_flags::VALUE) != 0
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

impl<'a> CheckerState<'a> {
    /// Resolve the display module name for namespace `typeof import("...")`.
    pub(crate) fn resolve_namespace_display_module_name(
        &self,
        exports_table: &tsz_binder::SymbolTable,
        fallback: &str,
    ) -> String {
        exports_table
            .get("export=")
            .and_then(|export_eq_sym| {
                let lib_binders = self.get_lib_binders();
                let sym = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(export_eq_sym, &lib_binders)
                    .or_else(|| self.get_cross_file_symbol(export_eq_sym))?;
                let is_ns_import =
                    sym.import_name.is_none() || sym.import_name.as_deref() == Some("*");
                if is_ns_import {
                    sym.import_module.clone()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| fallback.to_string())
    }

    /// Resolve binding element type from annotated destructured function parameter.
    pub(crate) fn resolve_binding_element_from_annotated_param(
        &mut self,
        value_decl: NodeIndex,
        name: &str,
    ) -> Option<TypeId> {
        use tsz_scanner::SyntaxKind;

        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // Identifier -> BindingElement
        let ext = self.ctx.arena.get_extended(value_decl)?;
        let be_idx = ext.parent;
        if !be_idx.is_some() {
            return None;
        }
        let be_node = self.ctx.arena.get(be_idx)?;
        if be_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let be_data = self.ctx.arena.get_binding_element(be_node)?;

        // BindingElement -> BindingPattern (direct parent, no intermediate nodes)
        let ext2 = self.ctx.arena.get_extended(be_idx)?;
        let pat_idx = ext2.parent;
        if !pat_idx.is_some() {
            return None;
        }
        let pat_node = self.ctx.arena.get(pat_idx)?;
        let is_obj = pat_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;
        let is_arr = pat_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
        if !is_obj && !is_arr {
            return None;
        }

        // BindingPattern -> Parameter
        let ext3 = self.ctx.arena.get_extended(pat_idx)?;
        let param_idx = ext3.parent;
        if !param_idx.is_some() {
            return None;
        }
        let param_node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        if !param.type_annotation.is_some() {
            return None;
        }
        let ann_type = self.get_type_from_type_node(param.type_annotation);
        if ann_type == TypeId::ANY || ann_type == TypeId::UNKNOWN || ann_type == TypeId::ERROR {
            return None;
        }
        // Evaluate to resolve Lazy/Application types
        let ann_type = self.evaluate_type_for_assignability(ann_type);
        if is_obj {
            let prop_name_str = if be_data.property_name.is_some() {
                self.get_identifier_text_from_idx(be_data.property_name)
            } else {
                Some(name.to_string())
            };
            let prop_name_str = prop_name_str?;
            let prop_atom = self.ctx.types.intern_string(&prop_name_str);

            // Look up property in object shape
            if let Some(shape) = object_shape_for_type(self.ctx.types, ann_type)
                && let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom)
            {
                let mut t = prop.type_id;
                // Optional property adds undefined under strict null checks
                if prop.optional && self.ctx.strict_null_checks() {
                    t = self.ctx.types.factory().union(vec![t, TypeId::UNDEFINED]);
                }
                // Default value strips undefined
                if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                    t = tsz_solver::remove_undefined(self.ctx.types, t);
                }
                return Some(t);
            }
        }
        // Array binding patterns are rare for function params; skip for now
        None
    }
}
