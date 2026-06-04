impl<'a> CheckerState<'a> {
    /// Check if an object type is a deferred indexed access that can't be resolved.
    /// Only suppresses TS2536 when the base of the indexed access is a type parameter
    /// (e.g., `Shape[k]` where Shape is a generic param), NOT when it's a concrete type
    /// (e.g., `DataFetchFns[T]` where `DataFetchFns` is a known type).
    fn is_deferred_indexed_access_object(&self, ty: TypeId) -> bool {
        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty) {
            return false;
        }
        // Decompose the indexed access and check if the base is a type parameter
        if let Some((base, _index)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, ty)
        {
            return crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, base);
        }
        false
    }

    /// Check if two AST nodes have the same text representation.
    fn nodes_have_same_text(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a_node = self.ctx.arena.get(a);
        let b_node = self.ctx.arena.get(b);
        match (a_node, b_node) {
            (Some(an), Some(bn)) if an.kind == bn.kind => {
                // Identifiers
                if let (Some(ai), Some(bi)) = (
                    self.ctx.arena.get_identifier(an),
                    self.ctx.arena.get_identifier(bn),
                ) {
                    return ai.escaped_text == bi.escaped_text;
                }
                // Literal types (e.g., LiteralType wrapping a string literal)
                if let (Some(alt), Some(blt)) = (
                    self.ctx.arena.get_literal_type(an),
                    self.ctx.arena.get_literal_type(bn),
                ) {
                    return self.nodes_have_same_text(alt.literal, blt.literal);
                }
                // String/number literals directly
                if let (Some(al), Some(bl)) = (
                    self.ctx.arena.get_literal(an),
                    self.ctx.arena.get_literal(bn),
                ) {
                    return al.text == bl.text;
                }
                false
            }
            _ => false,
        }
    }

    fn typeof_global_this_indexed_key_is_missing(&self, key: &str) -> bool {
        if key == "globalThis" {
            return false;
        }
        let Some(sym_id) = self.ctx.binder.file_locals.get(key) else {
            return true;
        };
        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
            symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE)
        })
    }

    pub(crate) fn is_keyof_for_current_object(
        &mut self,
        ty: TypeId,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        crate::query_boundaries::state::checking::keyof_target(self.ctx.types, ty).is_some_and(
            |operand| {
                let evaluated_operand = self.evaluate_type_with_env(operand);
                same_object_key_space(self.ctx.types, operand, object_type)
                    || same_object_key_space(self.ctx.types, operand, object_type_for_check)
                    || same_object_key_space(self.ctx.types, evaluated_operand, object_type)
                    || same_object_key_space(
                        self.ctx.types,
                        evaluated_operand,
                        object_type_for_check,
                    )
            },
        )
    }

    /// Resolve a type parameter's constraint from its AST declaration when the TypeId
    /// doesn't carry one. This handles cases where type parameters lose their constraints
    /// during type application argument resolution (e.g., `M[Event]` inside `Id<M[Event]>`).
    pub(crate) fn resolve_index_constraint_from_declaration(
        &mut self,
        index_node_idx: NodeIndex,
        _object_node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let index_name = self.simple_type_reference_name(index_node_idx)?;

        let mut current = self
            .ctx
            .arena
            .get_extended(index_node_idx)
            .map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let parent_node = self.ctx.arena.get(parent_idx)?;
            // Extract type_parameters NodeList from any generic declaration kind
            let type_params: Option<&tsz_parser::parser::base::NodeList> = match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent_node)
                        .and_then(|f| f.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                {
                    self.ctx
                        .arena
                        .get_signature(parent_node)
                        .and_then(|s| s.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent_node)
                    .and_then(|i| i.type_parameters.as_ref()),
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent_node)
                        .and_then(|c| c.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(parent_node)
                    .and_then(|ta| ta.type_parameters.as_ref()),
                k if k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                {
                    self.ctx
                        .arena
                        .get_function_type(parent_node)
                        .and_then(|ft| ft.type_parameters.as_ref())
                }
                _ => None,
            };

            if let Some(tp_list) = type_params {
                for &tp_idx in &tp_list.nodes {
                    let Some(tp_node) = self.ctx.arena.get(tp_idx) else {
                        continue;
                    };
                    let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(tp.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == index_name && tp.constraint != NodeIndex::NONE {
                        let constraint_type = self.get_type_from_type_node(tp.constraint);
                        if constraint_type != TypeId::ERROR {
                            return Some(constraint_type);
                        }
                    }
                }
            }
            // Mapped type key parameter: `[K in C]: ...` — extract constraint C
            if parent_node.kind == syntax_kind_ext::MAPPED_TYPE
                && let Some(mapped) = self.ctx.arena.get_mapped_type(parent_node)
                && let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter)
                && let Some(tp) = self.ctx.arena.get_type_parameter(tp_node)
                && let Some(name_node) = self.ctx.arena.get(tp.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text == index_name
                && tp.constraint != NodeIndex::NONE
            {
                let constraint_type = self.get_type_from_type_node(tp.constraint);
                if constraint_type != TypeId::ERROR {
                    return Some(constraint_type);
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        None
    }

    /// Check if the indexed access `T[K]` is inside the true branch of a conditional type
    /// `K extends keyof T ? ... : ...`. In the true branch, `K` is narrowed to `keyof T`,
    /// so the index is valid.
    fn is_in_conditional_keyof_narrowing_context(
        &mut self,
        node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
        _index_type: TypeId,
    ) -> bool {
        let index_name = self.simple_type_reference_name(
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_indexed_access_type(n))
                .map(|iat| iat.index_type)
                .unwrap_or(NodeIndex::NONE),
        );

        let mut current = self.ctx.arena.parent_of(node_idx);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                && let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
            {
                // Check if the indexed access is in the true branch
                // (the node_idx must be a descendant of cond.true_type)
                let in_true_branch = self.is_descendant_of(node_idx, cond.true_type);
                if in_true_branch {
                    // Check if the check type matches the index type
                    let check_name = self.simple_type_reference_name(cond.check_type);
                    if check_name.is_some() && check_name == index_name {
                        // Check if the extends type is `keyof T` for our object
                        let extends_type = self.get_type_from_type_node(cond.extends_type);
                        if self.is_keyof_for_current_object(
                            extends_type,
                            object_type,
                            object_type_for_check,
                        ) {
                            return true;
                        }
                        // Also check if extends type is keyof applied to the object
                        if let Some(extends_node) = self.ctx.arena.get(cond.extends_type)
                            && let Some(type_op) = self.ctx.arena.get_type_operator(extends_node)
                            && type_op.operator == SyntaxKind::KeyOfKeyword as u16
                        {
                            let keyof_target_type = self.get_type_from_type_node(type_op.type_node);
                            if same_object_key_space(self.ctx.types, keyof_target_type, object_type)
                                || same_object_key_space(
                                    self.ctx.types,
                                    keyof_target_type,
                                    object_type_for_check,
                                )
                            {
                                return true;
                            }
                        }
                    }
                    // Also check for `infer X extends C` patterns in the extends type.
                    // When the extends type contains `infer Head extends DistributedKeyOf<ObjT>`,
                    // the inferred type parameter `Head` is constrained to `keyof ObjT` in the
                    // true branch. If our index type matches such an infer parameter, suppress
                    // TS2536.
                    if let Some(ref idx_name) = index_name
                        && self.extends_type_has_infer_keyof_constraint(
                            cond.extends_type,
                            idx_name,
                            object_type,
                            object_type_for_check,
                        )
                    {
                        return true;
                    }
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        false
    }

    /// Check if the extends type of a conditional contains an `infer X extends C` pattern
    /// where `X` matches `target_name` and `C` resolves to `keyof ObjT`.
    fn extends_type_has_infer_keyof_constraint(
        &mut self,
        extends_node_idx: NodeIndex,
        target_name: &str,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        // Collect all infer type nodes from the extends type subtree.
        // We use a stack-based approach since there's no generic node_children method.
        let infer_nodes = self.collect_infer_nodes_in_subtree(extends_node_idx);
        for infer_node_idx in infer_nodes {
            let Some(node) = self.ctx.arena.get(infer_node_idx) else {
                continue;
            };
            let Some(infer_data) = self.ctx.arena.get_infer_type(node) else {
                continue;
            };
            let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter) else {
                continue;
            };
            let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(tp_data.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            if ident.escaped_text != target_name || tp_data.constraint == NodeIndex::NONE {
                continue;
            }
            // The constraint exists — check if it resolves to keyof ObjT
            let constraint_type = self.get_type_from_type_node(tp_data.constraint);
            let constraint_eval = self.evaluate_type_with_env(constraint_type);
            if self.is_keyof_for_current_object(constraint_type, object_type, object_type_for_check)
                || self.is_keyof_for_current_object(
                    constraint_eval,
                    object_type,
                    object_type_for_check,
                )
            {
                return true;
            }
            // Also check assignability: constraint might be
            // DistributedKeyOf<ObjT> which evaluates to keyof ObjT
            let keyof_object = self.ctx.types.evaluate_keyof(object_type_for_check);
            if self
                .indexed_access_key_space_relation_outcome(constraint_eval, keyof_object)
                .related
            {
                return true;
            }
        }
        false
    }

    /// Collect all `INFER_TYPE` node indices in a subtree, using parent-tracking.
    /// Walks all nodes whose parent chain leads back to `root_idx`.
    fn collect_infer_nodes_in_subtree(&self, root_idx: NodeIndex) -> Vec<NodeIndex> {
        let mut result = Vec::new();
        let mut stack = vec![root_idx];
        while let Some(idx) = stack.pop() {
            if idx == NodeIndex::NONE {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::INFER_TYPE {
                result.push(idx);
                // Nested `infer Y` inside the constraint is in the same scope.
                if let Some(infer_data) = self.ctx.arena.get_infer_type(node) {
                    stack.push(infer_data.type_parameter);
                }
                continue;
            }
            // Push children based on node type
            self.push_type_node_children(idx, node, &mut stack);
        }
        result
    }

    /// Push child indices of a type node onto the stack for traversal.
    fn push_type_node_children(
        &self,
        _idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
        stack: &mut Vec<NodeIndex>,
    ) {
        // Tuple type: push elements
        if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
            stack.extend(tuple.elements.nodes.iter().copied());
            return;
        }
        // Array type
        if let Some(arr) = self.ctx.arena.get_array_type(node) {
            stack.push(arr.element_type);
            return;
        }
        // Union/intersection type (both use CompositeTypeData)
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            stack.extend(composite.types.nodes.iter().copied());
            return;
        }
        // Type reference with type arguments
        if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
            if let Some(ref args) = type_ref.type_arguments {
                stack.extend(args.nodes.iter().copied());
            }
            return;
        }
        // Wrapped types: rest, optional, parenthesized (all share WrappedTypeData)
        if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
            stack.push(wrapped.type_node);
            return;
        }
        // Conditional type
        if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
            stack.push(cond.check_type);
            stack.push(cond.extends_type);
            stack.push(cond.true_type);
            stack.push(cond.false_type);
            return;
        }
        // Indexed access type
        if let Some(iat) = self.ctx.arena.get_indexed_access_type(node) {
            stack.push(iat.object_type);
            stack.push(iat.index_type);
            return;
        }
        // Type operator (keyof, readonly, unique)
        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            stack.push(type_op.type_node);
            return;
        }
        if let Some(tp) = self.ctx.arena.get_type_parameter(node) {
            stack.extend_from_slice(&[tp.constraint, tp.default]);
        }
    }

    /// Check if `node_a` is a descendant of `node_b` in the AST.
    fn is_descendant_of(&self, node_a: NodeIndex, node_b: NodeIndex) -> bool {
        let mut current = Some(node_a);
        while let Some(idx) = current {
            if idx == node_b {
                return true;
            }
            current = self.ctx.arena.parent_of(idx);
        }
        false
    }

    /// Check if the index type parameter has a `keyof` constraint targeting the object type,
    /// resolved from the AST declaration. Returns true if `K extends keyof T` for the current
    /// object T.
    fn index_has_keyof_constraint_from_declaration(
        &mut self,
        index_node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        if let Some(constraint_type) =
            self.resolve_index_constraint_from_declaration(index_node_idx, object_node_idx)
        {
            // Check if the constraint is `keyof T` for our object
            if self.is_keyof_for_current_object(constraint_type, object_type, object_type_for_check)
            {
                return true;
            }
            // Also check if the constraint is directly assignable to keyof of the object
            // (handles cases like `K extends string` indexing `Record<string, V>`)
        }
        false
    }
}
