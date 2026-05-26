//! Circular alias detection helpers.

use crate::query_boundaries::common::{
    self as common, TypeTraversalKind, classify_for_traversal, index_access_types,
    intersection_members, keyof_inner_type, lazy_def_id, mapped_type_info, number_literal_value,
    object_shape_for_type, string_literal_value, union_members,
};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

    /// Look through enclosing parentheses to the first non-parenthesized type
    /// node. Used by circular-alias detection so `(A<T>)` is treated like
    /// `A<T>`.
    pub(crate) fn unwrap_parenthesized_type(&self, type_node: NodeIndex) -> Option<NodeIndex> {
        let mut current = type_node;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_TYPE {
                return Some(current);
            }
            current = self.ctx.arena.get_wrapped_type(node)?.type_node;
        }
    }

    /// The type-name node of an unwrapped simple type reference. Returns `None`
    /// for any structurally wrapping form (object/array/tuple/union/
    /// intersection/function/conditional/...), which breaks a circular chain.
    fn unwrapped_type_reference_name(&self, type_node: NodeIndex) -> Option<NodeIndex> {
        let inner = self.unwrap_parenthesized_type(type_node)?;
        let node = self.ctx.arena.get(inner)?;
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            self.ctx.arena.get_type_ref(node).map(|tr| tr.type_name)
        } else if node.kind == SyntaxKind::Identifier as u16 {
            Some(inner)
        } else {
            None
        }
    }

    /// `(has_type_parameters, body_node)` for `sym_id`'s type-alias
    /// declaration, if it is a type alias with a declaration node.
    fn type_alias_decl_parts(&self, sym_id: SymbolId) -> Option<(bool, NodeIndex)> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0 {
            return None;
        }
        symbol.declarations.iter().find_map(|&decl_idx| {
            let node = self.ctx.arena.get(decl_idx)?;
            if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return None;
            }
            let ta = self.ctx.arena.get_type_alias(node)?;
            Some((ta.type_parameters.is_some(), ta.type_node))
        })
    }

    /// True when `sym_id` is a generic type alias that collapses to a
    /// non-generic error type because its unwrapped body is a self-application
    /// cycle (see [`Self::generic_alias_body_is_self_circular`]). A structural
    /// re-check so the answer does not depend on resolution order at
    /// type-reference sites.
    pub(crate) fn type_alias_is_generic_self_circular(&self, sym_id: SymbolId) -> bool {
        match self.type_alias_decl_parts(sym_id) {
            Some((true, body)) => self.generic_alias_body_is_self_circular(sym_id, body),
            _ => false,
        }
    }

    /// True when a type reference to `sym_id` should resolve to the collapsed
    /// non-generic error type: either it was already recorded as circular, or
    /// it is a generic self-application cycle.
    pub(crate) fn type_reference_alias_collapsed_to_error(&self, sym_id: SymbolId) -> bool {
        self.ctx.circular_type_aliases.contains(&sym_id)
            || self.type_alias_is_generic_self_circular(sym_id)
    }

    /// Detect a generic self-application cycle for `sym_id` and, if found,
    /// record it as circular *before* its body is lowered so the body
    /// self-reference and downstream use sites observe the collapsed,
    /// non-generic form. Returns true when `sym_id` is generic-self-circular.
    pub(crate) fn detect_and_mark_generic_self_circular(&mut self, sym_id: SymbolId) -> bool {
        let circular = self.type_alias_is_generic_self_circular(sym_id);
        if circular {
            self.ctx.circular_type_aliases.insert(sym_id);
            if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                self.ctx.definition_store.mark_circular_def(def_id);
            }
        }
        circular
    }

    /// Collapse a generic self-circular alias to a non-generic error type:
    /// record an error body so use sites that apply type arguments report
    /// TS2315 instead of cascading from a stale generic shape.
    pub(crate) fn register_generic_circular_alias_error(&mut self, sym_id: SymbolId) {
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        self.ctx
            .definition_store
            .register_type_to_def(TypeId::ERROR, def_id);
        self.ctx.definition_store.set_body(def_id, TypeId::ERROR);
    }

    /// True when a *generic* type alias's unwrapped body is a self-application
    /// that cycles back to `root_sym` through simple-reference alias hops
    /// (`type A<T> = A<T>`, or `type Foo<T> = Bar<T>; type Bar<T> = Foo<T>`).
    ///
    /// tsc resolves the alias body eagerly via `pushTypeResolution`, so an
    /// unwrapped self-application re-enters the alias mid-resolution and
    /// collapses it to a non-generic error type (TS2456 + TS2315). Structural
    /// wrappers defer resolution and are NOT followed here, so legitimate
    /// recursion such as `type List<T> = T | List<T>[]` is never flagged.
    pub(crate) fn generic_alias_body_is_self_circular(
        &self,
        root_sym: SymbolId,
        body_node: NodeIndex,
    ) -> bool {
        let mut current = body_node;
        let mut visited: FxHashSet<SymbolId> = FxHashSet::default();
        visited.insert(root_sym);
        loop {
            let Some(name_idx) = self.unwrapped_type_reference_name(current) else {
                return false;
            };
            let Some(sym_raw) = self.resolve_type_symbol_for_lowering(name_idx) else {
                return false;
            };
            let sym_id = SymbolId(sym_raw);
            if sym_id == root_sym {
                return true;
            }
            // Only simple-reference alias hops can extend the cycle. A type
            // parameter, interface, class, enum, or builtin breaks it. A repeat
            // visit is a cycle that does not pass through `root_sym`; that alias
            // reports its own diagnostic when it is computed.
            if !visited.insert(sym_id) {
                return false;
            }
            match self.type_alias_decl_parts(sym_id) {
                Some((_, body)) => current = body,
                None => return false,
            }
        }
    }

    /// True for direct circular aliases (`type A = B; type B = A`), false for
    /// structurally wrapped recursion. Marks all aliases on the resolution stack.
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
        if let Some(def_id) = lazy_def_id(self.ctx.types, resolved_type)
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
                if target_sym_id == sym_id
                    && self.type_node_contains_value_type_query_for_alias(type_node, sym_id)
                {
                    return false;
                }
                let is_direct = if in_union_or_intersection {
                    if target_sym_id == sym_id {
                        !self
                            .ctx
                            .symbol_resolution_stack
                            .iter()
                            .skip_while(|&&stack_sym| stack_sym != target_sym_id)
                            .skip(1)
                            .any(|&stack_sym| {
                                self.ctx.binder.get_symbol(stack_sym).is_some_and(|symbol| {
                                    symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                                }) && self.alias_ast_is_deferred(stack_sym)
                            })
                    } else {
                        let body = self
                            .ctx
                            .definition_store
                            .get_body(def_id)
                            .filter(|&b| lazy_def_id(self.ctx.types, b).is_none());
                        if let Some(b) = body {
                            !crate::query_boundaries::common::is_structurally_deferred_type(
                                self.ctx.types,
                                b,
                            )
                        } else {
                            !self.alias_ast_is_deferred(target_sym_id)
                        }
                    }
                } else {
                    self.is_simple_type_reference(type_node)
                };

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
                                if let Some(did) = self.ctx.get_existing_def_id(stack_sym) {
                                    self.ctx.definition_store.mark_circular_def(did);
                                }
                            }
                        }
                    }
                    // Always mark the target itself as circular (handles cross-file cycles).
                    self.ctx.circular_type_aliases.insert(target_sym_id);
                    if let Some(did) = self.ctx.get_existing_def_id(target_sym_id) {
                        self.ctx.definition_store.mark_circular_def(did);
                    }
                }

                return is_direct;
            }
        }

        // For mapped types, check if the constraint references the alias being
        // defined (via keyof or directly).  This catches non-generic self-referencing
        // mapped type aliases like `type Recurse = { [K in keyof Recurse]: Recurse[K] }`.
        if let Some(mapped_info) = mapped_type_info(self.ctx.types, resolved_type) {
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
                if let Some(def_id) = lazy_def_id(self.ctx.types, ref_type)
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
                                if let Some(did) = self.ctx.get_existing_def_id(stack_sym) {
                                    self.ctx.definition_store.mark_circular_def(did);
                                }
                            }
                        }
                    }
                    self.ctx.circular_type_aliases.insert(target_sym_id);
                    if let Some(did) = self.ctx.get_existing_def_id(target_sym_id) {
                        self.ctx.definition_store.mark_circular_def(did);
                    }
                    return true;
                }
            }
        }

        // Type query (typeof X): per the TS spec, "a type query directly depends
        // on the type of the referenced entity". When the alias body is a
        // TYPE_QUERY at the top level (`type T = typeof X`) pointing at a
        // non-self variable, look at that variable's annotation AST and check
        // whether it references any alias on the resolution chain. We inspect
        // the AST rather than evaluate x's type to avoid re-entering type alias
        // resolution for x → typeof x → alias.
        if let Some(node) = self.ctx.arena.get(type_node)
            && node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(type_query) = self.ctx.arena.get_type_query(node)
        {
            let entity_idx = type_query.expr_name;
            // The entity in `typeof X` is a value identifier — use the
            // value-position resolver, not the type-position one.
            if entity_idx != NodeIndex::NONE
                && let Some(query_raw) = self.resolve_value_symbol_for_lowering(entity_idx)
            {
                let query_sym_id = SymbolId(query_raw);
                if query_sym_id != sym_id
                    && self.typeof_target_annotation_refs_resolution_chain(query_sym_id)
                {
                    return true;
                }
                if self.typeof_parameter_flow_predicate_refs_resolution_chain(entity_idx, type_node)
                {
                    return true;
                }
            }
        }

        // Skip evaluation when the type contains a TypeQuery (typeof) referencing
        // the symbol being checked. TypeQuery accesses the VALUE namespace, not the
        // TYPE namespace, so `type X = Static<typeof X>` (where `const X` also exists)
        // is NOT circular. Evaluating such types would re-enter the type alias
        // resolution for the merged symbol and produce a false TS2456.
        let has_typeof_self =
            crate::query_boundaries::common::collect_type_queries(self.ctx.types, resolved_type)
                .iter()
                .any(|sym_ref| sym_ref.0 == sym_id.0);
        let has_deferred_resolution_chain_ref =
            common::is_structurally_deferred_type(self.ctx.types, resolved_type)
                && common::collect_lazy_def_ids(self.ctx.types, resolved_type)
                    .into_iter()
                    .any(|def_id| {
                        self.ctx
                            .def_to_symbol
                            .borrow()
                            .get(&def_id)
                            .copied()
                            .is_some_and(|target_sym_id| {
                                self.ctx.symbol_resolution_set.contains(&target_sym_id)
                                    && self.ctx.binder.get_symbol(target_sym_id).is_some_and(
                                        |symbol| {
                                            symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                                        },
                                    )
                            })
                    });
        let evaluated = if has_typeof_self || has_deferred_resolution_chain_ref {
            resolved_type
        } else {
            match classify_for_traversal(self.ctx.types, resolved_type) {
                TypeTraversalKind::Application { .. }
                | TypeTraversalKind::Conditional(_)
                | TypeTraversalKind::Mapped(_)
                | TypeTraversalKind::IndexAccess { .. }
                | TypeTraversalKind::KeyOf(_) => self.evaluate_type_with_resolution(resolved_type),
                _ => resolved_type,
            }
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

        if let Some((object_type, index_type)) = index_access_types(self.ctx.types, resolved_type) {
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
                } else if let Some(key) = string_literal_value(self.ctx.types, index_type) {
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
                } else if number_literal_value(self.ctx.types, index_type).is_some()
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
        if let Some(members) = union_members(self.ctx.types, resolved_type) {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node, true) {
                    return true;
                }
            }
        }
        if let Some(members) = intersection_members(self.ctx.types, resolved_type) {
            for &member in &members {
                if self.is_direct_circular_reference(sym_id, member, type_node, true) {
                    return true;
                }
            }
        }

        false
    }

    /// Walk the AST node tree under `root_idx` and return the SymbolId of any
    /// type-reference/identifier that resolves to a member of
    /// `ctx.symbol_resolution_set` and represents a type alias. Descends through
    /// arrays, tuples, unions, intersections, and parenthesized types so that
    /// `T5[]` is detected as referencing `T5`. Stops at structural-deferral
    /// wrappers (`TYPE_LITERAL`, `MAPPED_TYPE`, `FUNCTION_TYPE`, `CONSTRUCTOR_TYPE`),
    /// because tsc creates those types lazily — property types and signature
    /// types are not eagerly resolved during typeof-target type construction,
    /// so a reference to a resolution-chain alias inside such a wrapper does
    /// NOT make the chain directly circular.
    fn ast_finds_resolution_chain_alias(&self, root_idx: NodeIndex) -> Option<SymbolId> {
        let mut stack = vec![root_idx];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };
            // Look at TYPE_REFERENCE or bare Identifier — both can name a type alias.
            let lookup_target_idx = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx.arena.get_type_ref(node).map(|tr| tr.type_name)
            } else if node.kind == SyntaxKind::Identifier as u16 {
                Some(node_idx)
            } else {
                None
            };
            if let Some(target_idx) = lookup_target_idx
                && let Some(sym_raw) = self.resolve_type_symbol_for_lowering(target_idx)
            {
                let sym_id = SymbolId(sym_raw);
                if self.ctx.symbol_resolution_set.contains(&sym_id) {
                    let is_type_alias = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0);
                    if is_type_alias {
                        return Some(sym_id);
                    }
                }
            }
            // Skip descent into structural-deferral wrappers. tsc resolves
            // property types and signature types lazily, so a chain-member
            // reference inside `{ x: T }`, `() => T`, `new (...) => T`, or a
            // mapped type is NOT eagerly resolved at typeof-target type
            // construction time. Treating it as circular is a false positive.
            // ARRAY_TYPE / TUPLE_TYPE intentionally still descend — tsc
            // eagerly computes element types via getArrayType / getTupleType.
            let k = node.kind;
            if k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::MAPPED_TYPE
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE
            {
                continue;
            }
            // A TYPE_REFERENCE with type arguments creates a generic
            // instantiation boundary — descend into its children only if the
            // ref itself isn't the chain target.
            for child_idx in self.ctx.arena.get_children(node_idx) {
                stack.push(child_idx);
            }
        }
        None
    }

    pub(crate) fn alias_ast_refs_symbol_or_resolution_chain_alias(
        &self,
        root_idx: NodeIndex,
        primary_sym_id: SymbolId,
    ) -> bool {
        let mut stack = vec![root_idx];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            let lookup_target_idx = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx.arena.get_type_ref(node).map(|tr| tr.type_name)
            } else if node.kind == SyntaxKind::Identifier as u16 {
                Some(node_idx)
            } else {
                None
            };
            if let Some(target_idx) = lookup_target_idx
                && let Some(sym_raw) = self.resolve_type_symbol_for_lowering(target_idx)
            {
                let sym_id = SymbolId(sym_raw);
                if (sym_id == primary_sym_id || self.ctx.symbol_resolution_set.contains(&sym_id))
                    && self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
                {
                    return true;
                }
            }

            for child_idx in self.ctx.arena.get_children(node_idx) {
                stack.push(child_idx);
            }
        }
        false
    }

    fn type_node_contains_value_type_query_for_alias(
        &self,
        type_node: NodeIndex,
        alias_sym_id: SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(type_query) = self.ctx.arena.get_type_query(node)
            && let Some(raw_query_sym) =
                self.resolve_value_symbol_for_lowering(type_query.expr_name)
        {
            let query_sym_id = SymbolId(raw_query_sym);
            if self.value_symbol_matches_alias_name(query_sym_id, alias_sym_id) {
                return true;
            }
        }

        self.ctx
            .arena
            .get_children(type_node)
            .into_iter()
            .any(|child| self.type_node_contains_value_type_query_for_alias(child, alias_sym_id))
    }

    fn value_symbol_matches_alias_name(
        &self,
        query_sym_id: SymbolId,
        alias_sym_id: SymbolId,
    ) -> bool {
        let Some(alias_symbol) = self.ctx.binder.get_symbol(alias_sym_id) else {
            return false;
        };
        if alias_symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }
        let Some(query_symbol) = self.ctx.binder.get_symbol(query_sym_id) else {
            return false;
        };
        let query_has_value = query_symbol.value_declaration.is_some()
            || query_symbol.flags & tsz_binder::symbol_flags::VALUE != 0;
        query_has_value && query_symbol.escaped_name == alias_symbol.escaped_name
    }

    /// True when the `typeof X` target's variable annotation references any type
    /// alias in the current resolution chain. Marks the chain as circular when
    /// found. AST-only — never resolves x's type, to avoid re-entering alias
    /// resolution.
    fn typeof_target_annotation_refs_resolution_chain(&mut self, var_sym_id: SymbolId) -> bool {
        let decls: Vec<NodeIndex> = match self.ctx.binder.get_symbol(var_sym_id) {
            Some(symbol) => symbol.declarations.clone(),
            None => return false,
        };
        let mut hit: Option<SymbolId> = None;
        for decl_idx in decls {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                continue;
            }
            let annotation_idx = self
                .ctx
                .arena
                .get_variable_declaration(decl_node)
                .map(|vd| vd.type_annotation);
            let Some(annotation_idx) = annotation_idx else {
                continue;
            };
            if annotation_idx == NodeIndex::NONE {
                continue;
            }
            if let Some(found) = self.ast_finds_resolution_chain_alias(annotation_idx) {
                hit = Some(found);
                break;
            }
        }
        let Some(found_sym_id) = hit else {
            return false;
        };
        // Mark all aliases on the resolution stack between target and current as circular.
        let stack_snapshot: Vec<SymbolId> = self.ctx.symbol_resolution_stack.to_vec();
        let mut found_target = false;
        for stack_sym in stack_snapshot {
            if stack_sym == found_sym_id {
                found_target = true;
            }
            if found_target {
                let is_alias = self
                    .ctx
                    .binder
                    .get_symbol(stack_sym)
                    .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0);
                if is_alias {
                    self.ctx.circular_type_aliases.insert(stack_sym);
                    if let Some(did) = self.ctx.get_existing_def_id(stack_sym) {
                        self.ctx.definition_store.mark_circular_def(did);
                    }
                }
            }
        }
        self.ctx.circular_type_aliases.insert(found_sym_id);
        if let Some(did) = self.ctx.get_existing_def_id(found_sym_id) {
            self.ctx.definition_store.mark_circular_def(did);
        }
        true
    }

    fn typeof_parameter_flow_predicate_refs_resolution_chain(
        &mut self,
        entity_idx: NodeIndex,
        type_node: NodeIndex,
    ) -> bool {
        let Some(entity_node) = self.ctx.arena.get(entity_idx) else {
            return false;
        };
        if entity_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(param_sym) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, entity_idx)
        else {
            return false;
        };
        let Some(param_symbol) = self.ctx.binder.get_symbol(param_sym) else {
            return false;
        };
        let is_parameter = param_symbol.declarations.iter().any(|&decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind == syntax_kind_ext::PARAMETER {
                return true;
            }
            decl_node.kind == SyntaxKind::Identifier as u16
                && self
                    .ctx
                    .arena
                    .get_extended(decl_idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| parent.kind == syntax_kind_ext::PARAMETER)
        });
        if !is_parameter {
            return false;
        }

        let Some(alias_decl_idx) = self.ctx.arena.get_extended(type_node).map(|ext| ext.parent)
        else {
            return false;
        };
        let Some(alias_decl_node) = self.ctx.arena.get(alias_decl_idx) else {
            return false;
        };
        let alias_pos = alias_decl_node.pos;
        let mut current = alias_decl_idx;
        let mut block_idx = NodeIndex::NONE;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::BLOCK
            {
                block_idx = parent;
                break;
            }
            current = parent;
        }
        if block_idx.is_none() {
            return false;
        }
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(block_node) else {
            return false;
        };

        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.pos >= alias_pos {
                break;
            }
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                continue;
            }
            let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
                continue;
            };
            let Some(callee_node) = self.ctx.arena.get(call.expression) else {
                continue;
            };
            if callee_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(callee_raw) = self.resolve_value_symbol_for_lowering(call.expression) else {
                continue;
            };
            if self.value_symbol_type_predicate_refs_resolution_chain(SymbolId(callee_raw)) {
                return true;
            }
        }

        false
    }

    fn value_symbol_type_predicate_refs_resolution_chain(&self, value_sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(value_sym_id) else {
            return false;
        };
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                continue;
            }
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if var_decl.type_annotation.is_some()
                && self.annotation_type_predicate_refs_resolution_chain(var_decl.type_annotation)
            {
                return true;
            }
        }
        false
    }

    fn annotation_type_predicate_refs_resolution_chain(&self, annotation_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::TYPE_PREDICATE
            && let Some(pred) = self.ctx.arena.get_type_predicate(node)
        {
            return pred.type_node.is_some()
                && self
                    .ast_finds_resolution_chain_alias(pred.type_node)
                    .is_some();
        }
        if node.kind == syntax_kind_ext::FUNCTION_TYPE
            && let Some(func) = self.ctx.arena.get_function_type(node)
            && func.type_annotation.is_some()
        {
            return self.annotation_type_predicate_refs_resolution_chain(func.type_annotation);
        }
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && let Some(sym_raw) = self.resolve_type_symbol_for_lowering(type_ref.type_name)
            && let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_raw))
            && symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
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
                if self.annotation_type_predicate_refs_resolution_chain(alias.type_node) {
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
    pub(crate) fn alias_ast_is_deferred(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                continue;
            }
            let Some(ta) = self.ctx.arena.get_type_alias(decl_node) else {
                continue;
            };
            let Some(body_node) = self.ctx.arena.get(ta.type_node) else {
                continue;
            };
            let k = body_node.kind;
            if k == syntax_kind_ext::ARRAY_TYPE
                || k == syntax_kind_ext::TUPLE_TYPE
                || k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::MAPPED_TYPE
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                || k == syntax_kind_ext::TYPE_OPERATOR
            {
                return true;
            }
            // A generic type reference (e.g. ReadonlyArray<T>, Promise<T>) provides
            // structural deferral via generic instantiation — the recursive reference
            // is behind a layer of indirection.
            if k == syntax_kind_ext::TYPE_REFERENCE
                && let Some(tr) = self.ctx.arena.get_type_ref(body_node)
                && tr.type_arguments.is_some()
            {
                return true;
            }
            if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE {
                let children = self.ctx.arena.get_children(ta.type_node);
                if !children.is_empty()
                    && children.iter().all(|&c| {
                        self.union_alias_child_is_deferred_or_non_recursive(
                            c,
                            sym_id,
                            &mut FxHashSet::default(),
                        )
                    })
                {
                    return true;
                }
            }
        }
        false
    }

    fn union_alias_child_is_deferred_or_non_recursive(
        &self,
        node_idx: NodeIndex,
        target_sym: SymbolId,
        visited_aliases: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return true;
        };
        let k = node.kind;
        if k == syntax_kind_ext::ARRAY_TYPE
            || k == syntax_kind_ext::TUPLE_TYPE
            || k == syntax_kind_ext::TYPE_LITERAL
            || k == syntax_kind_ext::MAPPED_TYPE
            || k == syntax_kind_ext::FUNCTION_TYPE
            || k == syntax_kind_ext::CONSTRUCTOR_TYPE
            || k == syntax_kind_ext::TYPE_OPERATOR
        {
            return true;
        }
        if k == syntax_kind_ext::TYPE_REFERENCE {
            let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                return true;
            };
            if type_ref.type_arguments.is_some() {
                return true;
            }
            let Some(sym_raw) = self.resolve_type_symbol_for_lowering(type_ref.type_name) else {
                return true;
            };
            let ref_sym = SymbolId(sym_raw);
            if ref_sym == target_sym {
                return false;
            }
            let Some(symbol) = self.ctx.binder.get_symbol(ref_sym) else {
                return true;
            };
            if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0 {
                return true;
            }
            if !self.alias_body_references_symbol(ref_sym, target_sym, visited_aliases) {
                return true;
            }
            return self.alias_ast_is_deferred(ref_sym);
        }
        !self.ast_node_references_symbol(node_idx, target_sym, visited_aliases)
    }

    fn alias_body_references_symbol(
        &self,
        alias_sym: SymbolId,
        target_sym: SymbolId,
        visited_aliases: &mut FxHashSet<SymbolId>,
    ) -> bool {
        if !visited_aliases.insert(alias_sym) {
            return false;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sym) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                && self
                    .ctx
                    .arena
                    .get_type_alias(decl_node)
                    .is_some_and(|alias| {
                        self.ast_node_references_symbol(
                            alias.type_node,
                            target_sym,
                            visited_aliases,
                        )
                    })
        })
    }

    fn ast_node_references_symbol(
        &self,
        root_idx: NodeIndex,
        target_sym: SymbolId,
        visited_aliases: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let mut stack = vec![root_idx];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };
            let lookup_target_idx = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx.arena.get_type_ref(node).map(|tr| tr.type_name)
            } else if node.kind == SyntaxKind::Identifier as u16 {
                Some(node_idx)
            } else {
                None
            };
            if let Some(target_idx) = lookup_target_idx
                && let Some(sym_raw) = self.resolve_type_symbol_for_lowering(target_idx)
            {
                let sym_id = SymbolId(sym_raw);
                if sym_id == target_sym {
                    return true;
                }
                if self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
                    && self.alias_body_references_symbol(sym_id, target_sym, visited_aliases)
                {
                    return true;
                }
            }
            for child_idx in self.ctx.arena.get_children(node_idx) {
                stack.push(child_idx);
            }
        }
        false
    }

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
        let Some(mapped) = self.ctx.arena.get_mapped_type(node) else {
            return false;
        };
        let mut visited = FxHashSet::default();
        if let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter)
            && let Some(tp) = self.ctx.arena.get_type_parameter(tp_node)
            && tp.constraint != NodeIndex::NONE
            && self.ast_contains_circular_type_ref(tp.constraint, sym_id, &mut visited)
        {
            return true;
        }
        if mapped.name_type != NodeIndex::NONE
            && self.ast_contains_circular_type_ref(mapped.name_type, sym_id, &mut visited)
        {
            return true;
        }
        false
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
        // Track whether we have traversed at least one intermediate alias.
        // get_type_of_symbol_with_params pre-caches a Lazy(own_def_id) placeholder
        // for type aliases before computing their real type. If the cached type
        // is still this placeholder (e.g., because the parent checker's cache was
        // not overwritten by a child checker's merge), the first hop would
        // immediately match own_def_id and produce a false positive TS2456.
        // A real cross-file circular alias must traverse through at least one
        // other alias body before returning to own_def_id.
        let mut hops = 0;

        loop {
            let Some(def_id) = lazy_def_id(self.ctx.types, current) else {
                return false;
            };

            if def_id == own_def_id {
                // Only flag as circular if we traversed at least one intermediate
                // alias. A direct Lazy(own_def_id) on the first hop is the
                // pre-cached placeholder, not a real cycle.
                return hops > 0;
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
            hops += 1;
        }
    }

    /// Post-processing: detect cross-file circular type aliases (TS2456).
    pub(crate) fn check_cross_file_circular_type_aliases(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;

        // Collect type alias symbols from the current file's node_symbols.
        let mut type_alias_syms: Vec<SymbolId> = self
            .ctx
            .binder
            .node_symbols
            .values()
            .copied()
            .filter(|&sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS))
            })
            .collect::<FxHashSet<_>>()
            .into_iter()
            .collect();
        type_alias_syms.sort_unstable();

        let mut circular_ids: Vec<SymbolId> = Vec::new();

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

            // Check circularity via two paths:
            // 1. Lazy chain: resolved type is still Lazy -> follow through DefinitionStore.
            // 2. Shared circular set: a sibling file's checker already marked
            //    this symbol's DefId as circular during inline detection
            //    (`is_direct_circular_reference` walks the symbol_resolution_stack
            //    and marks every type-alias on it). This holds even when the
            //    cached resolved type is still a Lazy placeholder for the
            //    sibling file's alias — circular2.ts hits this path for `B`
            //    after `/a.ts`'s recursion marks both A and B.
            let is_lazy = lazy_def_id(self.ctx.types, resolved).is_some();
            let shared_circular = self
                .ctx
                .get_existing_def_id(sym_id)
                .is_some_and(|def_id| self.ctx.definition_store.is_circular_def(def_id));

            if !is_lazy && !shared_circular {
                continue;
            }

            if shared_circular || self.is_cross_file_circular_alias(sym_id, resolved) {
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
                            // Suppress TS2456 when the declaration has parse
                            // errors (empty type parameter names from reserved
                            // word recovery, e.g., `type T1<in in> = T1`).
                            let has_parse_error_tp =
                                ta.type_parameters.as_ref().is_some_and(|tp_list| {
                                    tp_list.nodes.iter().any(|&tp_idx| {
                                        self.ctx
                                            .arena
                                            .get(tp_idx)
                                            .and_then(|n| self.ctx.arena.get_type_parameter(n))
                                            .and_then(|tp| {
                                                self.ctx
                                                    .arena
                                                    .get(tp.name)
                                                    .and_then(|n| self.ctx.arena.get_identifier(n))
                                            })
                                            .is_some_and(|ident| {
                                                self.ctx
                                                    .arena
                                                    .resolve_identifier_text(ident)
                                                    .is_empty()
                                            })
                                    })
                                });
                            // Suppress TS2456 when the type alias has an
                            // import alias partner — the apparent circularity
                            // is from the name conflict (TS2440), not a real cycle.
                            let has_import_partner = self
                                .ctx
                                .alias_partner_for(self.ctx.binder, sym_id)
                                .and_then(|pid| self.ctx.binder.get_symbol(pid))
                                .is_some_and(|p| p.flags & tsz_binder::symbol_flags::ALIAS != 0);
                            // Suppress TS2456 when the type alias body provides
                            // structural wrapping (cross-file deferred cycles),
                            // but keep TS2456 for non-generic mapped-type key
                            // cycles like `type T = { [K in keyof T]: ... }`.
                            let body_is_deferred = self.alias_ast_is_deferred(sym_id)
                                && !self.is_non_generic_mapped_type_circular(sym_id, ta.type_node);
                            let is_jsx_runtime_bridge_alias = self
                                .is_jsx_import_source_runtime_bridge_alias(
                                    self.ctx.arena,
                                    ta.type_node,
                                );
                            // Restore the pre-existing suppression for generic
                            // simple-reference aliases unless the precise
                            // structural self-cycle detection confirmed it.
                            let generic_self_ref = ta.type_parameters.is_some()
                                && self.is_simple_type_reference(ta.type_node)
                                && !self.type_alias_is_generic_self_circular(sym_id);
                            if name_matches
                                && !has_parse_error_tp
                                && !has_import_partner
                                && !body_is_deferred
                                && !generic_self_ref
                                && !is_jsx_runtime_bridge_alias
                                && !self.ctx.import_conflict_names.contains(&name)
                            {
                                self.error_at_node(
                                    ta.name,
                                    &message,
                                    diagnostic_codes::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF,
                                );
                                circular_ids.push(sym_id);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Apply side effects after scanning all aliases so diagnostics are not
        // order-dependent.
        for sym_id in &circular_ids {
            self.ctx.circular_type_aliases.insert(*sym_id);
            if let Some(def_id) = self.ctx.get_existing_def_id(*sym_id) {
                self.ctx.definition_store.mark_circular_def(def_id);
            }
        }
    }
}
