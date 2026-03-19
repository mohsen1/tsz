//! JSX helper methods extracted from `jsx_checker.rs` to keep that file under 2000 LOC.
//!
//! Contains: children shape validation (TS2745/TS2746), grammar checks (TS17000),
//! missing-required-props (TS2741), intrinsic-attribute-only fallback, and
//! generic SFC spread checking.

use crate::query_boundaries::common::PropertyAccessResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check that all required properties in the props type are provided. Emits TS2741.
    pub(super) fn check_missing_required_jsx_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
        attributes_idx: NodeIndex,
    ) {
        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        else {
            return;
        };

        for prop in &shape.properties {
            if prop.optional {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(prop.name);

            // 'children' is now handled via jsx_children_info synthesis in
            if provided_attrs.iter().any(|(a, _)| a == &prop_name) {
                continue;
            }

            // Build synthetic source type for the error message.
            let source_type = if provided_attrs.is_empty() {
                "{}".to_string()
            } else {
                let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
                    .iter()
                    .map(|(name, type_id)| {
                        let name_atom = self.ctx.types.intern_string(name);
                        tsz_solver::PropertyInfo {
                            name: name_atom,
                            type_id: *type_id,
                            write_type: *type_id,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: tsz_solver::Visibility::Public,
                            parent_id: None,
                            declaration_order: 0,
                        }
                    })
                    .collect();
                let obj_type = self.ctx.types.factory().object(properties);
                self.format_type(obj_type)
            };
            let target_type = self.format_type(props_type);
            let message = format!(
                "Property '{prop_name}' is missing in type '{source_type}' but required in type '{target_type}'."
            );
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                attributes_idx,
                &message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }
    }

    /// Check TS2745/TS2746 from one normalized children-shape path.
    pub(super) fn check_jsx_children_shape(
        &mut self,
        props_type: TypeId,
        child_count: usize,
        tag_name_idx: NodeIndex,
    ) {
        let Some(children_type) = self.get_jsx_children_prop_type(props_type) else {
            return;
        };

        match child_count {
            0 => {}
            1 => {
                if !self.type_requires_multiple_children(children_type) {
                    return;
                }

                let children_type_str = self.format_type(children_type);
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    tag_name_idx,
                    diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING,
                    &["children", &children_type_str],
                );
            }
            _ => {
                if self.type_allows_multiple_children(children_type) {
                    return;
                }

                // Fallback: any[] assignable to children type (handles complex aliases like ReactNode).
                let any_array = self.ctx.types.factory().array(TypeId::ANY);
                if self.is_assignable_to(any_array, children_type) {
                    return;
                }

                let children_type_str = self.format_type(children_type);
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    tag_name_idx,
                    diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO,
                    &["children", &children_type_str],
                );
            }
        }
    }

    fn get_jsx_children_prop_type(&mut self, props_type: TypeId) -> Option<TypeId> {
        let resolved = self.resolve_type_for_property_access(props_type);
        let children_type = match self.resolve_property_access_with_env(resolved, "children") {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            _ => return None,
        };
        let children_type = self.evaluate_type_with_env(children_type);
        if matches!(children_type, TypeId::ANY | TypeId::ERROR) {
            return None;
        }
        Some(children_type)
    }

    /// Check if a type can accept multiple JSX body children (tuple/array-like or a union with one).
    fn type_allows_multiple_children(&mut self, type_id: TypeId) -> bool {
        // Evaluate to resolve type aliases and lazy references
        let type_id = self.evaluate_type_with_env(type_id);

        if type_id == TypeId::ANY || type_id == TypeId::ERROR {
            return true;
        }

        // Direct array/tuple check
        if tsz_solver::is_array_type(self.ctx.types, type_id)
            || tsz_solver::is_tuple_type(self.ctx.types, type_id)
        {
            return true;
        }

        // Object with numeric index signature
        if tsz_solver::type_queries::get_object_shape(self.ctx.types, type_id)
            .is_some_and(|shape| shape.number_index.is_some())
        {
            return true;
        }

        // Union: multiple JSX children are allowed if any branch accepts them.
        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
        {
            let members_vec: Vec<TypeId> = members.to_vec();
            return members_vec
                .iter()
                .any(|&member| self.type_allows_multiple_children(member));
        }

        false
    }

    /// Check if a type requires multiple JSX body children instead of a single child value.
    fn type_requires_multiple_children(&mut self, type_id: TypeId) -> bool {
        let type_id = self.evaluate_type_with_env(type_id);

        if type_id == TypeId::ANY || type_id == TypeId::ERROR {
            return false;
        }

        if tsz_solver::is_array_type(self.ctx.types, type_id)
            || tsz_solver::is_tuple_type(self.ctx.types, type_id)
        {
            return true;
        }

        // Object with numeric index signature
        if tsz_solver::type_queries::get_object_shape(self.ctx.types, type_id)
            .is_some_and(|shape| shape.number_index.is_some())
        {
            return true;
        }

        // Union: a single JSX child is only invalid when every branch requires
        // the body-children form (for example `A[] | [A, B]`).
        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
        {
            let members_vec: Vec<TypeId> = members.to_vec();
            return members_vec
                .iter()
                .all(|&member| self.type_requires_multiple_children(member));
        }

        false
    }

    /// Fallback: check `IntrinsicAttributes` when component props couldn't be extracted.
    pub(super) fn check_jsx_intrinsic_attributes_only(
        &mut self,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let Some(intrinsic_attrs_type) = self.get_intrinsic_attributes_type() else {
            return;
        };

        // Collect provided attribute names with types
        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                if let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                    && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                    && let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                {
                    provided_attrs.push((attr_name, TypeId::ANY));
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                // Spread of `any` covers all properties
                if let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) {
                    let spread_type = self.compute_type_of_node(spread_data.expression);
                    if spread_type == TypeId::ANY {
                        return; // any covers everything
                    }
                }
            }
        }

        self.check_missing_required_jsx_props(intrinsic_attrs_type, &provided_attrs, tag_name_idx);
    }

    /// TS2322: Check spread attributes against `IntrinsicAttributes` for generic SFCs.
    pub(super) fn check_generic_sfc_spread_intrinsic_attrs(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        // Only applies to generic SFCs (functions with type parameters)
        let is_generic_sfc =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, component_type)
                .is_some_and(|shape| !shape.type_params.is_empty() && !shape.is_constructor)
                || tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
                    .is_some_and(|sigs| sigs.iter().any(|s| !s.type_params.is_empty()));

        if !is_generic_sfc {
            return;
        }

        let Some(ia_type) = self.get_intrinsic_attributes_type() else {
            return;
        };

        // Get spread attributes
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                continue;
            };
            let spread_type = self.compute_type_of_node(spread_data.expression);

            if spread_type == TypeId::ANY || spread_type == TypeId::ERROR {
                continue;
            }

            // Build target: IntrinsicAttributes & spread_type
            let target = self.ctx.types.factory().intersection2(ia_type, spread_type);

            if !self.is_assignable_to(spread_type, target) {
                let spread_name = self.format_type(spread_type);
                let target_name = format!("IntrinsicAttributes & {spread_name}");
                let message =
                    format!("Type '{spread_name}' is not assignable to type '{target_name}'.");
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    tag_name_idx,
                    &message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }
    }

    /// Grammar check: TS17000 for empty expressions in JSX attributes.
    /// Matches tsc's `checkGrammarJsxElement`: reports only the first empty
    /// expression per JSX opening element, then returns.
    pub(super) fn check_grammar_jsx_element(&mut self, attributes_idx: NodeIndex) {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            if attr_data.initializer.is_none() {
                continue;
            }
            let Some(init_node) = self.ctx.arena.get(attr_data.initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::JSX_EXPRESSION {
                continue;
            }
            let Some(expr_data) = self.ctx.arena.get_jsx_expression(init_node) else {
                continue;
            };
            // Empty expression {} without spread
            if expr_data.expression.is_none() && !expr_data.dot_dot_dot_token {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    attr_data.initializer,
                    "JSX attributes must only be assigned a non-empty 'expression'.",
                    diagnostic_codes::JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION,
                );
                // tsc returns after the first grammar error per element
                return;
            }
        }
    }
}
