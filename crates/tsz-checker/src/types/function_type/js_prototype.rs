//! JS prototype owner expression helpers and constructor body instance type resolution.
use crate::state::CheckerState;
use crate::types_domain::computation::complex_constructors::PrototypeMembers;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
impl<'a> CheckerState<'a> {
    pub(crate) fn js_prototype_owner_expression_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..6 {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                | syntax_kind_ext::PROPERTY_ASSIGNMENT
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    current = parent;
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.ctx.arena.get_binary_expr(parent_node)?;
                    if binary.right != current
                        || !self.is_assignment_operator(binary.operator_token)
                    {
                        return None;
                    }
                    return self.js_prototype_owner_expression_from_assignment_left(binary.left);
                }
                _ => break,
            }
        }
        None
    }

    fn js_prototype_owner_expression_for_function(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        self.js_prototype_owner_expression_for_node(func_idx)
    }

    /// Check if a function expression is the right-hand side of an assignment expression.
    /// This is used to determine if return type errors should be anchored at the assignment
    /// level (matching tsc behavior for `A.prototype.foo = function() {}`).
    pub(crate) fn is_rhs_of_assignment(&self, func_idx: NodeIndex) -> bool {
        let mut current = func_idx;
        for _ in 0..6 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                | syntax_kind_ext::PROPERTY_ASSIGNMENT
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    current = parent;
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
                        return false;
                    };
                    return binary.right == current
                        && self.is_assignment_operator(binary.operator_token);
                }
                _ => return false,
            }
        }
        false
    }

    /// Find the left-hand side of an assignment when given the RHS node.
    /// This is used to anchor errors at the assignment target position.
    pub(crate) fn find_assignment_lhs_for_rhs(&self, rhs_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = rhs_idx;
        for _ in 0..6 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                | syntax_kind_ext::PROPERTY_ASSIGNMENT
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    current = parent;
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.ctx.arena.get_binary_expr(parent_node)?;
                    if binary.right == current && self.is_assignment_operator(binary.operator_token)
                    {
                        return Some(binary.left);
                    }
                    return None;
                }
                _ => return None,
            }
        }
        None
    }

    fn js_prototype_owner_expression_from_assignment_left(
        &self,
        left_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let left_node = self.ctx.arena.get(left_idx)?;
        let left_access = self.ctx.arena.get_access_expr(left_node)?;

        if self.access_name_matches(left_access.name_or_argument, "prototype") {
            return Some(left_access.expression);
        }

        let proto_node = self.ctx.arena.get(left_access.expression)?;
        let proto_access = self.ctx.arena.get_access_expr(proto_node)?;
        if self.access_name_matches(proto_access.name_or_argument, "prototype") {
            return Some(proto_access.expression);
        }

        None
    }

    pub(crate) fn js_prototype_owner_function_target(
        &self,
        owner_expr: NodeIndex,
    ) -> Option<NodeIndex> {
        let owner_text = self.expression_text(owner_expr)?;

        if !owner_text.contains('.')
            && let Some(sym_id) = self.ctx.binder.file_locals.get(owner_text.as_str())
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let value_decl = symbol.value_declaration;
            let value_node = self.ctx.arena.get(value_decl)?;
            if value_node.is_function_like() {
                return Some(value_decl);
            }
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(value_node) {
                let init_node = self.ctx.arena.get(var_decl.initializer)?;
                if init_node.is_function_like() {
                    return Some(var_decl.initializer);
                }
            }
        }

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if self.expression_text(binary.left).as_deref() != Some(owner_text.as_str()) {
                continue;
            }
            let Some(right_node) = self.ctx.arena.get(binary.right) else {
                continue;
            };
            if right_node.is_function_like() {
                return Some(binary.right);
            }
        }

        None
    }

    fn access_name_matches(&self, name_idx: NodeIndex, expected: &str) -> bool {
        self.ctx.arena.get(name_idx).is_some_and(|name_node| {
            self.ctx
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == expected)
                || (name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    && self
                        .ctx
                        .arena
                        .get_literal(name_node)
                        .is_some_and(|lit| lit.text == expected))
        })
    }

    pub(crate) fn js_constructor_body_instance_type_for_function(
        &mut self,
        func_idx: NodeIndex,
    ) -> Option<TypeId> {
        let body_idx = self
            .ctx
            .arena
            .get(func_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
            .and_then(|func| {
                if func.body.is_none() {
                    None
                } else {
                    Some(func.body)
                }
            })?;
        let mut properties = rustc_hash::FxHashMap::default();
        self.collect_js_constructor_this_properties(body_idx, &mut properties, None, false);
        if let Some(func_node) = self.ctx.arena.get(func_idx)
            && let Some(func) = self.ctx.arena.get_function(func_node)
            && let Some(func_name) = func.name.into_option().and_then(|name_idx| {
                self.ctx
                    .arena
                    .get(name_idx)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.clone())
            })
            && let Some(sym_id) = self.ctx.binder.get_node_symbol(func_idx)
        {
            let PrototypeMembers {
                method_bindings,
                this_props,
                ..
            } = self.collect_prototype_members_and_this_properties(func_idx, &func_name, sym_id);
            for (name, prop) in method_bindings {
                properties.entry(name).or_insert(prop);
            }
            for (name, mut prop) in this_props {
                let factory = self.ctx.types.factory();
                let widened_prop_type = factory.union2(prop.type_id, TypeId::UNDEFINED);
                if let Some(existing) = properties.get_mut(&name) {
                    if existing.write_type == TypeId::ANY {
                        existing.type_id = factory.union2(existing.type_id, widened_prop_type);
                    }
                } else {
                    prop.type_id = widened_prop_type;
                    prop.write_type = prop.type_id;
                    properties.insert(name, prop);
                }
            }
        }

        if properties.is_empty() {
            None
        } else {
            Some(
                self.ctx
                    .types
                    .factory()
                    .object(properties.into_values().collect()),
            )
        }
    }
}
