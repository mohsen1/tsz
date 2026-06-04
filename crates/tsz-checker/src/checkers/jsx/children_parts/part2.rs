impl<'a> CheckerState<'a> {
    fn get_precise_jsx_children_body_type(
        &mut self,
        attributes_idx: NodeIndex,
        children_type: TypeId,
    ) -> Option<TypeId> {
        let child_nodes = self.get_jsx_body_child_nodes(attributes_idx)?;
        if child_nodes.len() <= 1 {
            return None;
        }

        let child_types: Vec<TypeId> = child_nodes
            .iter()
            .map(|&child_idx| self.compute_type_of_node(child_idx))
            .collect();

        if self.type_has_tuple_like_multiple_children(children_type) {
            let elements = child_types
                .into_iter()
                .map(|type_id| tsz_solver::TupleElement {
                    type_id,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect();
            return Some(self.ctx.types.factory().tuple(elements));
        }

        let element_type = match child_types.len() {
            0 => TypeId::NEVER,
            1 => child_types[0],
            _ => self.ctx.types.factory().union(child_types),
        };
        Some(self.ctx.types.factory().array(element_type))
    }

    pub(super) fn get_jsx_body_child_nodes(
        &self,
        attributes_idx: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
        let opening_idx = self.ctx.arena.get_extended(attributes_idx)?.parent;
        let opening_node = self.ctx.arena.get(opening_idx)?;
        self.ctx.arena.get_jsx_opening(opening_node)?;

        let element_idx = self.ctx.arena.get_extended(opening_idx)?.parent;
        let element_node = self.ctx.arena.get(element_idx)?;
        let jsx_element = self.ctx.arena.get_jsx_element(element_node)?;

        let mut child_nodes = Vec::new();
        for &child_idx in &jsx_element.children.nodes {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };
            if child_node.kind == tsz_scanner::SyntaxKind::JsxText as u16
                && let Some(text) = self.ctx.arena.get_jsx_text(child_node)
            {
                let is_all_whitespace = text.text.chars().all(|c| c.is_ascii_whitespace());
                let has_newline = text.text.contains('\n');
                if is_all_whitespace && has_newline {
                    continue;
                }
            }
            if child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                && let Some(expr_data) = self.ctx.arena.get_jsx_expression(child_node)
                && expr_data.expression == NodeIndex::NONE
            {
                continue;
            }
            child_nodes.push(child_idx);
        }

        Some(child_nodes)
    }

    fn type_has_tuple_like_multiple_children(&mut self, type_id: TypeId) -> bool {
        let type_id = self.evaluate_type_with_env(type_id);

        if crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id) {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&member| self.type_has_tuple_like_multiple_children(member));
        }

        false
    }

    /// Check if a type can accept multiple JSX body children (tuple/array-like or a union with one).
    pub(super) fn type_allows_multiple_children(&mut self, type_id: TypeId) -> bool {
        // Evaluate to resolve type aliases and lazy references
        let type_id = self.evaluate_type_with_env(type_id);

        if type_id == TypeId::ANY || type_id == TypeId::ERROR {
            return true;
        }

        // Direct array/tuple check
        if crate::query_boundaries::common::is_array_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id)
        {
            return true;
        }

        // Object with numeric index signature
        if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
            .is_some_and(|shape| shape.number_index.is_some())
        {
            return true;
        }

        // Union: multiple JSX children are allowed if any branch accepts them.
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let members_vec: Vec<TypeId> = members.to_vec();
            if members_vec
                .iter()
                .any(|&member| self.type_allows_multiple_children(member))
            {
                return true;
            }
        }

        // Fallback: check if an array of the children type is assignable to the declared
        // children type. This handles cases like `ReactNode` where `ReactNodeArray extends
        // Array<ReactNode>` is a member of the union, but we can't detect it structurally
        // because it's an interface extending Array rather than a direct Array type.
        let array_of_children = self.ctx.types.factory().array(type_id);
        if self
            .jsx_children_relation_outcome(array_of_children, type_id)
            .related
        {
            return true;
        }

        false
    }

    /// Check if a type requires multiple JSX body children instead of a single child value.
    pub(super) fn type_requires_multiple_children(&mut self, type_id: TypeId) -> bool {
        let type_id = self.evaluate_type_with_env(type_id);

        if type_id == TypeId::ANY || type_id == TypeId::ERROR {
            return false;
        }

        if crate::query_boundaries::common::is_array_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id)
        {
            return true;
        }

        // Object with numeric index signature
        if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
            .is_some_and(|shape| shape.number_index.is_some())
        {
            return true;
        }

        // Union: a single JSX child is only invalid when every branch requires
        // the body-children form (for example `A[] | [A, B]`).
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let members_vec: Vec<TypeId> = members.to_vec();
            return members_vec
                .iter()
                .all(|&member| self.type_requires_multiple_children(member));
        }

        false
    }
}
