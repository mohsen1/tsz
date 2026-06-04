impl<'a> FlowAnalyzer<'a> {
    /// Deduplicate flow merge members using identity only.
    ///
    /// Flow merges must NOT use structural assignability to eliminate types.
    /// Structural subtype reduction collapses distinct class types that share
    /// the same interface (e.g. `Derived1 | Derived2` → `Derived1` when
    /// Derived2 has all of Derived1's members), which loses narrowing
    /// information needed by subsequent control flow analysis.
    ///
    /// The solver's `union()` handles any appropriate subtype reduction
    /// when constructing the actual union type.
    fn simplify_flow_merge_types(&self, types: Vec<TypeId>) -> Vec<TypeId> {
        let mut seen = FxHashSet::with_capacity_and_hasher(types.len(), Default::default());
        let mut simplified = Vec::with_capacity(types.len());
        for ty in types {
            if seen.insert(ty) {
                simplified.push(ty);
            }
        }
        if simplified.contains(&TypeId::UNKNOWN) {
            return vec![TypeId::UNKNOWN];
        }
        simplified
    }

    fn reference_is_evolving_array_symbol(&self, reference: NodeIndex) -> bool {
        let Some(sym_id) = self.reference_symbol(reference) else {
            return false;
        };
        if self.is_control_flow_typed_any_symbol(sym_id) {
            return true;
        }

        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        let value_decl = symbol.value_declaration;
        let Some(mut decl_node) = self.arena.get(value_decl) else {
            return false;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_node = parent_node;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if decl.type_annotation.is_some() || decl.initializer.is_none() {
            return false;
        }
        self.arena.get(decl.initializer).is_some_and(|node| {
            node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && self
                    .arena
                    .get_literal_expr(node)
                    .is_some_and(|lit| lit.elements.nodes.is_empty())
        })
    }

    fn array_mutation_evolved_type(
        &self,
        current_type: TypeId,
        call: &CallExprData,
        reference: NodeIndex,
    ) -> (TypeId, bool) {
        if !self.reference_is_evolving_array_symbol(reference) {
            return (current_type, true);
        }

        let Some(callee_node) = self.arena.get(call.expression) else {
            return (current_type, true);
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return (current_type, true);
        };
        let Some(name_node) = self.arena.get(access.name_or_argument) else {
            return (current_type, true);
        };
        let method_name = if let Some(ident) = self.arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = self.arena.get_literal(name_node) {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return (current_type, true);
            }
        } else {
            return (current_type, true);
        };
        if method_name != "push" && method_name != "unshift" {
            return (current_type, true);
        }

        let Some(args) = &call.arguments else {
            return (current_type, true);
        };
        let Some(current_element) = query::get_array_element_type(self.interner, current_type)
        else {
            return (current_type, true);
        };

        let mut element_types = Vec::new();
        if current_element != TypeId::ANY && current_element != TypeId::NEVER {
            element_types.push(current_element);
        }
        for &arg in &args.nodes {
            if !arg.is_some() {
                continue;
            }
            let Some(arg_type) = self
                .node_types
                .and_then(|node_types| node_types.get(&arg.0).copied())
                .or_else(|| self.literal_type_from_node(arg))
            else {
                return (current_type, false);
            };
            if arg_type == TypeId::ERROR {
                return (current_type, false);
            }
            element_types.push(query::widen_literal_to_primitive(self.interner, arg_type));
        }
        if element_types.is_empty() {
            return (current_type, true);
        }

        let element_type = self.simplify_flow_merge_types(element_types);
        let element_type = if element_type.len() == 1 {
            element_type[0]
        } else {
            query::union_types(self.interner, element_type)
        };
        (query::array_type(self.interner, element_type), true)
    }

    /// Returns true when two types represent the same union member set.
    ///
    /// Used by switch-clause fallthrough merging to preserve the original
    /// pre-switch type identity (including alias/display metadata) when the
    /// merged type expands back to that same semantic union.
    fn same_union_member_set(&self, left: TypeId, right: TypeId) -> bool {
        fn normalized_union_members(db: &dyn QueryDatabase, ty: TypeId) -> Vec<TypeId> {
            if let Some(members) = union_members_for_type(db, ty) {
                let mut normalized: Vec<TypeId> = members.to_vec();
                normalized.sort_unstable_by_key(|member| member.0);
                normalized.dedup();
                normalized
            } else {
                vec![ty]
            }
        }

        normalized_union_members(self.interner, left)
            == normalized_union_members(self.interner, right)
    }

    /// Create a new `FlowAnalyzer`.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        interner: &'a dyn QueryDatabase,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            checker_context: None,
            node_types: None,
            flow_graph,
            flow_cache: None,
            type_environment: None,
            switch_reference_cache: RefCell::new(FxHashMap::default()),
            shared_switch_reference_cache: None,
            reference_match_cache: RefCell::new(FxHashMap::default()),
            reference_symbol_cache: RefCell::new(FxHashMap::default()),
            shared_reference_match_cache: None,
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            shared_numeric_atom_cache: None,
            narrowing_cache: None,
            call_type_predicates: None,
            flow_worklist: None,
            flow_in_worklist: None,
            flow_visited: None,
            flow_results: None,
            shared_symbol_last_assignment_pos: None,
            destructured_bindings: None,
            concrete_this_type: None,
        }
    }

    pub fn with_node_types(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        interner: &'a dyn QueryDatabase,
        node_types: &'a crate::context::NodeTypeCache,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            checker_context: None,
            node_types: Some(node_types),
            flow_graph,
            flow_cache: None,
            type_environment: None,
            switch_reference_cache: RefCell::new(FxHashMap::default()),
            shared_switch_reference_cache: None,
            reference_match_cache: RefCell::new(FxHashMap::default()),
            reference_symbol_cache: RefCell::new(FxHashMap::default()),
            shared_reference_match_cache: None,
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            shared_numeric_atom_cache: None,
            narrowing_cache: None,
            call_type_predicates: None,
            flow_worklist: None,
            flow_in_worklist: None,
            flow_visited: None,
            flow_results: None,
            shared_symbol_last_assignment_pos: None,
            destructured_bindings: None,
            concrete_this_type: None,
        }
    }

    /// Set the flow analysis cache to avoid redundant graph traversals.
    pub const fn with_flow_cache(
        mut self,
        cache: &'a RefCell<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>>,
    ) -> Self {
        self.flow_cache = Some(cache);
        self
    }

    /// Set a shared reference-match cache used by `is_matching_reference`.
    pub const fn with_reference_match_cache(mut self, cache: &'a ReferenceMatchCache) -> Self {
        self.shared_reference_match_cache = Some(cache);
        self
    }

    /// Set a shared switch-reference cache.
    pub const fn with_switch_reference_cache(mut self, cache: &'a ReferenceMatchCache) -> Self {
        self.shared_switch_reference_cache = Some(cache);
        self
    }

    /// Set a shared narrowing cache.
    pub const fn with_narrowing_cache(mut self, cache: &'a NarrowingCache) -> Self {
        self.narrowing_cache = Some(cache);
        self
    }

    /// Set instantiated call type predicates from generic call resolutions.
    pub const fn with_call_type_predicates(mut self, predicates: &'a CallPredicateMap) -> Self {
        self.call_type_predicates = Some(predicates);
        self
    }

    /// Set a shared numeric atom cache.
    pub const fn with_numeric_atom_cache(
        mut self,
        cache: &'a RefCell<FxHashMap<u64, Atom>>,
    ) -> Self {
        self.shared_numeric_atom_cache = Some(cache);
        self
    }

    /// Set reusable flow buffers.
    pub const fn with_flow_buffers(
        mut self,
        worklist: &'a RefCell<VecDeque<(FlowNodeId, TypeId)>>,
        in_worklist: &'a RefCell<FxHashSet<FlowNodeId>>,
        visited: &'a RefCell<FxHashSet<FlowNodeId>>,
        results: &'a RefCell<FxHashMap<FlowNodeId, TypeId>>,
    ) -> Self {
        self.flow_worklist = Some(worklist);
        self.flow_in_worklist = Some(in_worklist);
        self.flow_visited = Some(visited);
        self.flow_results = Some(results);
        self
    }

    /// Set a shared last-assignment-position cache for "effectively const" detection.
    pub const fn with_symbol_last_assignment_pos(
        mut self,
        cache: &'a RefCell<FxHashMap<tsz_binder::SymbolId, u32>>,
    ) -> Self {
        self.shared_symbol_last_assignment_pos = Some(cache);
        self
    }

    pub const fn with_destructured_bindings(
        mut self,
        bindings: &'a FxHashMap<SymbolId, crate::context::DestructuredBindingInfo>,
    ) -> Self {
        self.destructured_bindings = Some(bindings);
        self
    }

    pub const fn with_concrete_this_type(mut self, concrete_this_type: TypeId) -> Self {
        self.concrete_this_type = Some(concrete_this_type);
        self
    }

    /// Check if a type contains type parameters, using the shared narrowing cache
    /// when available to avoid per-call `FxHashMap` allocation.
    fn contains_type_parameters_cached(&self, type_id: TypeId) -> bool {
        if let Some(cache) = self.narrowing_cache {
            let cached = cache
                .contains_type_parameters_cache
                .borrow()
                .get(&type_id)
                .copied();
            if let Some(result) = cached {
                return result;
            }
            let result = query::contains_type_parameters(self.interner, type_id);
            cache
                .contains_type_parameters_cache
                .borrow_mut()
                .insert(type_id, result);
            result
        } else {
            query::contains_type_parameters(self.interner, type_id)
        }
    }

    /// Create a `NarrowingContext`, sharing the pre-allocated cache when available.
    /// This avoids 7 `FxHashMap` allocations per narrowing operation on the hot path.
    pub(super) fn make_narrowing_context(&self) -> NarrowingContext<'_> {
        if let Some(cache) = self.narrowing_cache {
            NarrowingContext::with_cache(self.interner, cache)
        } else {
            NarrowingContext::new(self.interner)
        }
    }

    fn flow_assignability_related(&self, source: TypeId, target: TypeId) -> bool {
        let env = self.type_environment.map(std::cell::RefCell::borrow);
        query::flow_assignability_outcome(
            self.interner,
            env.as_deref(),
            self.concrete_this_type,
            source,
            target,
            false,
        )
        .related
    }

    /// Set the `TypeEnvironment` for resolving Lazy types during narrowing.
    pub const fn with_type_environment(mut self, type_env: &'a RefCell<TypeEnvironment>) -> Self {
        self.type_environment = Some(type_env);
        self
    }

    /// Set the owning checker context for stable `DefId` fallback resolution.
    pub const fn with_checker_context(
        mut self,
        ctx: &'a crate::context::CheckerContext<'a>,
    ) -> Self {
        self.checker_context = Some(ctx);
        self
    }

    /// Check if the switch expression is the literal `true` keyword.
    /// `switch(true)` is a pattern where each case clause acts as an independent
    /// type guard condition, not a comparison against the switch expression.
    pub(crate) fn is_switch_true(&self, switch_expr: NodeIndex) -> bool {
        self.arena
            .get(switch_expr)
            .is_some_and(|node| node.kind == SyntaxKind::TrueKeyword as u16)
    }

    fn flow_chain_contains_switch_clause(&self, flow_id: FlowNodeId) -> bool {
        let mut worklist = VecDeque::from([flow_id]);
        let mut visited = FxHashSet::default();
        let mut steps = 0usize;

        while let Some(current) = worklist.pop_front() {
            if current.is_none() || !visited.insert(current) {
                continue;
            }
            steps += 1;
            if steps > 32 {
                return false;
            }
            let Some(flow) = self.binder.flow_nodes.get(current) else {
                continue;
            };
            if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                return true;
            }
            for &ant in &flow.antecedent {
                worklist.push_back(ant);
            }
        }

        false
    }

    #[inline]
    fn switch_can_affect_reference(&self, switch_expr: NodeIndex, reference: NodeIndex) -> bool {
        // switch(true) can narrow any reference — each case expression is an
        // independent condition (like an if-else chain).
        if self.is_switch_true(switch_expr) {
            return true;
        }

        let key = (switch_expr.0, reference.0);
        if let Some(shared) = self.shared_switch_reference_cache
            && let Some(&cached) = shared.borrow().get(&key)
        {
            return cached;
        }
        if let Some(&cached) = self.switch_reference_cache.borrow().get(&key) {
            return cached;
        }

        let affects = self.is_matching_reference(switch_expr, reference)
            || self
                .relative_discriminant_path(switch_expr, reference)
                .is_some_and(|(path, _)| !path.is_empty())
            // switch (typeof x) narrows x through typeof comparison
            || self.is_typeof_target(switch_expr, reference)
            || self.is_optional_chain_containing_target(switch_expr, reference)
            // switch (alias) where alias is a const alias for reference.prop
            // (e.g. `const kind = obj.kind; switch(kind)`) or a destructuring alias
            // (e.g. `const { kind } = obj; switch(kind)`) — the aliased discriminant
            // path is resolved by narrow_by_switch_case_clause → narrow_by_binary_expr
            // → discriminant_comparison → aliased_discriminant once we allow entry.
            || self.is_aliased_discriminant_switch_expr(switch_expr, reference);

        if let Some(shared) = self.shared_switch_reference_cache {
            shared.borrow_mut().insert(key, affects);
        }
        self.switch_reference_cache
            .borrow_mut()
            .insert(key, affects);
        affects
    }

    /// Get a reference to the flow graph.
    pub const fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }

    /// Get the narrowed type of a symbol at a specific flow node.
    ///
    /// This walks backwards through the flow graph, applying narrowing operations
    /// when it encounters condition nodes.
    pub fn get_flow_type(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        // Short-circuit for error types: flow narrowing must not transform ERROR
        // into a concrete type. When the declared/initial type is ERROR (e.g.,
        // property access on an unresolved type), condition narrowing handlers
        // like `== null` can produce `null | undefined` regardless of the input
        // type, turning a suppressed error into a false positive diagnostic.
        if initial_type == TypeId::ERROR {
            return initial_type;
        }
        let narrowed = self.get_flow_type_uncorrelated(reference, initial_type, flow_node);
        self.apply_correlated_destructured_narrowing(reference, initial_type, narrowed, flow_node)
    }

    fn get_flow_type_uncorrelated(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        if flow_node.is_none() {
            return initial_type;
        }

        // Resolve symbol for caching purposes.
        // Fallback to reference_symbol for non-identifier references (e.g. some
        // qualified/member references) so repeated flow queries can share cache
        // entries instead of using per-node synthetic symbols.
        let symbol_id = self
            .binder
            .resolve_identifier(self.arena, reference)
            .or_else(|| self.reference_symbol(reference));

        self.check_flow(
            reference,
            initial_type,
            flow_node,
            &mut Vec::new(),
            symbol_id,
        )
    }

    fn apply_correlated_destructured_narrowing(
        &self,
        reference: NodeIndex,
        _initial_type: TypeId,
        narrowed_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        let Some(bindings) = self.destructured_bindings else {
            return narrowed_type;
        };
        let Some(sym_id) = self
            .binder
            .resolve_identifier(self.arena, reference)
            .or_else(|| self.reference_symbol(reference))
        else {
            return narrowed_type;
        };
        let Some(info) = bindings.get(&sym_id) else {
            return narrowed_type;
        };
        if !info.is_const {
            return narrowed_type;
        }

        let Some(source_members) = union_members_for_type(self.interner, info.source_type) else {
            return narrowed_type;
        };

        let siblings: Vec<_> = bindings
            .iter()
            .filter(|(other_sym, other_info)| {
                **other_sym != sym_id && other_info.group_id == info.group_id && other_info.is_const
            })
            .map(|(other_sym, other_info)| (*other_sym, other_info))
            .collect();
        if siblings.is_empty() {
            return narrowed_type;
        }

        let mut remaining_members = source_members.to_vec();
        let original_member_count = remaining_members.len();

        for (sib_sym, sib_info) in siblings {
            let Some(sib_ref) = self.symbol_identifier_ref(sib_sym) else {
                continue;
            };
            let Some(sib_initial) =
                self.derive_binding_type_from_members(&source_members, sib_info)
            else {
                continue;
            };

            let sib_narrowed = self.get_flow_type_uncorrelated(sib_ref, sib_initial, flow_node);
            if sib_narrowed == sib_initial {
                continue;
            }

            remaining_members.retain(|&member| {
                self.binding_type_from_member(member, sib_info)
                    .is_none_or(|member_ty| self.types_overlap(member_ty, sib_narrowed))
            });
        }

        if remaining_members.len() == original_member_count {
            return narrowed_type;
        }
        if remaining_members.is_empty() {
            return TypeId::NEVER;
        }

        let Some(correlated) = self.derive_binding_type_from_members(&remaining_members, info)
        else {
            return narrowed_type;
        };

        if correlated == narrowed_type {
            return correlated;
        }

        self.intersect_types(correlated, narrowed_type)
            .unwrap_or(correlated)
    }

    fn symbol_identifier_ref(&self, sym: SymbolId) -> Option<NodeIndex> {
        let mut declaration_ident = None;
        for (&node_id, &node_sym) in self.binder.node_symbols.iter() {
            if node_sym != sym {
                continue;
            }
            let idx = NodeIndex(node_id);
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }

            let is_declaration_ident = self
                .arena
                .get_extended(idx)
                .and_then(|ext| self.arena.get(ext.parent))
                .is_some_and(|parent| {
                    parent.kind == syntax_kind_ext::BINDING_ELEMENT
                        || parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                        || parent.kind == syntax_kind_ext::PARAMETER
                });

            if !is_declaration_ident {
                return Some(idx);
            }
            declaration_ident = Some(idx);
        }
        declaration_ident
    }

    fn binding_type_from_member(
        &self,
        member: TypeId,
        info: &crate::context::DestructuredBindingInfo,
    ) -> Option<TypeId> {
        if !info.property_name.is_empty() {
            let mut current = member;
            for segment in info.property_name.split('.') {
                let prop = find_property_in_object_by_str(self.interner, current, segment)?;
                current = prop.type_id;
            }
            Some(current)
        } else if let Some(elements) = tuple_elements_for_type(self.interner, member) {
            resolve_tuple_binding_type(
                self.interner,
                &elements,
                info.element_index as usize,
                info.is_rest,
            )
        } else {
            None
        }
    }

    fn derive_binding_type_from_members(
        &self,
        members: &[TypeId],
        info: &crate::context::DestructuredBindingInfo,
    ) -> Option<TypeId> {
        let mut result_types = Vec::new();
        for &member in members {
            if let Some(member_ty) = self.binding_type_from_member(member, info) {
                result_types.push(member_ty);
            }
        }
        if result_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.interner,
                result_types,
            ))
        }
    }

    fn types_overlap(&self, left: TypeId, right: TypeId) -> bool {
        left == right
            || self.flow_assignability_related(left, right)
            || self.flow_assignability_related(right, left)
    }

    fn intersect_types(&self, left: TypeId, right: TypeId) -> Option<TypeId> {
        let left_members = union_members_for_type(self.interner, left);
        let right_members = union_members_for_type(self.interner, right);

        match (left_members, right_members) {
            (Some(left_members), Some(right_members)) => {
                let filtered: Vec<_> = left_members
                    .iter()
                    .filter(|member| right_members.contains(member))
                    .copied()
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(tsz_solver::utils::union_or_single(self.interner, filtered))
                }
            }
            (Some(left_members), None) => left_members.contains(&right).then_some(right),
            (None, Some(right_members)) => right_members.contains(&left).then_some(left),
            (None, None) => (left == right).then_some(left),
        }
    }

    /// Check if a reference is definitely assigned at a specific flow node.
    pub fn is_definitely_assigned(&self, reference: NodeIndex, flow_node: FlowNodeId) -> bool {
        if flow_node.is_none() {
            return true;
        }

        let mut visited = Vec::new();
        let mut cache = FxHashMap::default();
        self.check_definite_assignment(reference, flow_node, &mut visited, &mut cache)
    }

    /// Analyze a loop using fixed-point iteration to determine the stable type of a variable.
    ///
    /// This implements TypeScript's loop flow analysis where the type of a variable
    /// at the start of a loop depends on its type at the end (back-edge). We iterate
    /// until the type stabilizes (reaches a fixed point).
    ///
    /// # Arguments
    /// * `loop_flow_id` - The `FlowNodeId` of the `LOOP_LABEL` (for cache key)
    /// * `loop_flow` - The `LOOP_LABEL` flow node
    /// * `reference` - The variable reference we're analyzing
    /// * `entry_type` - The type entering the loop (from antecedent[0])
    /// * `initial_type` - The declared type of the variable (for widening)
    /// * `symbol_id` - The symbol ID (for cache key)
    ///
    /// # Returns
    /// The stabilized type after fixed-point iteration
    fn analyze_loop_fixed_point(
        &self,
        loop_flow_id: FlowNodeId,
        loop_flow: &FlowNode,
        reference: NodeIndex,
        entry_type: TypeId,
        initial_type: TypeId,
        symbol_id: Option<SymbolId>,
    ) -> TypeId {
        const MAX_ITERATIONS: usize = 5;

        // For const symbols, no fixed-point needed - they can't be reassigned
        if let Some(sym_id) = symbol_id
            && self.is_const_symbol(sym_id)
        {
            return entry_type;
        }

        // Without a symbol_id we cannot inject cache entries to break the
        // get_flow_type → check_flow → LOOP_LABEL → analyze_loop_fixed_point
        // recursion cycle.  This happens for property-access references
        // (e.g. `fns.length`) whose base symbol is tracked separately.
        // Returning the entry type is safe because property access expressions
        // are never reassigned inside loops.
        if symbol_id.is_none() {
            return entry_type;
        }

        // If there's only one antecedent (just the entry, no back-edges), no iteration needed
        if loop_flow.antecedent.len() <= 1 {
            return entry_type;
        }

        let mut current_type = entry_type;

        // Fixed-point iteration: union entry type with all back-edge types
        for _iteration in 0..MAX_ITERATIONS {
            let prev_type = current_type;

            // CRITICAL FIX: Inject current assumption into cache to break infinite recursion
            // Without this, get_flow_type -> check_flow -> LOOP_LABEL -> analyze_loop_fixed_point
            // would cause stack overflow
            //
            // This tells the recursive traversal: "If you hit this loop header again,
            // assume its type is current_type and stop"
            //
            // We inject under TWO keys: one with initial_type (for the outer check_flow's
            // cache lookup) and one with current_type (for the inner back-edge traversal
            // which uses current_type as its initial_type).
            if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache) {
                let key = (loop_flow_id, sym_id, initial_type);
                cache.borrow_mut().insert(key, current_type);
                if current_type != initial_type {
                    let inner_key = (loop_flow_id, sym_id, current_type);
                    cache.borrow_mut().insert(inner_key, current_type);
                }
            }

            // Union entry type with all back-edge types (antecedents[1+])
            for &back_edge in loop_flow.antecedent.iter().skip(1) {
                // Use current_type (the current loop assumption) as the initial type
                // for back-edge traversal instead of the declared type. This ensures
                // narrowing inside the loop body uses the loop's computed type, not
                // the full declared type. E.g., if declared type is string|number|boolean
                // but the loop only assigns string and number, narrowing typeof !== "number"
                // should give string (not string|boolean).
                let back_edge_type = self.get_flow_type(reference, current_type, back_edge);

                // Union current type with back-edge type
                current_type =
                    query::union_types(self.interner, vec![current_type, back_edge_type]);
            }

            // Check if we've reached a fixed point (type stopped changing)
            if current_type == prev_type {
                // Update cache with the final converged type for all intermediate keys.
                // During iteration, we inject `(loop, sym, entry_type) -> entry_type` which
                // is a pessimistic guess. Once the fixed point is reached, we must update
                // the cache so subsequent queries with initial_type=entry_type get the
                // correct converged result, not the stale intermediate.
                if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache)
                    && entry_type != current_type
                {
                    let entry_key = (loop_flow_id, sym_id, entry_type);
                    cache.borrow_mut().insert(entry_key, current_type);
                }
                return current_type;
            }
        }

        // Fixed point not reached within iteration limit
        // Conservative widening: return union of entry type and initial declared type
        // This matches TypeScript's behavior for complex loops
        let widened = query::union_types(self.interner, vec![entry_type, initial_type]);

        // Update cache with final widened result
        if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache) {
            let key = (loop_flow_id, sym_id, initial_type);
            cache.borrow_mut().insert(key, widened);
        }

        widened
    }

    /// Internal sentinel for "unreachable never" — returned by `handle_call_iterative`
    /// when a call returns `never`. This is distinct from `TypeId::NEVER` which represents
    /// legitimate narrowing to the empty type (e.g., exhaustive checks). This sentinel is
    /// used only within `check_flow` and never escapes to the rest of the system.
    ///
    /// Matches tsc's `unreachableNeverType` vs `neverType` distinction:
    /// - At `BRANCH_LABEL` merge points, `UNREACHABLE_NEVER` branches are filtered out
    /// - At the final return, `UNREACHABLE_NEVER` is mapped back to `initial_type`
    ///   (declared type), matching tsc's `getFlowTypeOfReference` behavior
    const UNREACHABLE_NEVER: TypeId = TypeId(98);
}
