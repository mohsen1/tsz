//! JSX children normalization: shape validation (TS2745/TS2746), spread child
//! normalization, children prop type resolution, and multi-child assignability.

use crate::context::TypingRequest;
use crate::query_boundaries::common::{
    PropertyAccessResult, array_element_type, tuple_elements, unwrap_readonly,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn normalize_jsx_spread_child_type(
        &mut self,
        spread_child_idx: NodeIndex,
        spread_type: TypeId,
    ) -> TypeId {
        let spread_type = self.evaluate_type_with_env(spread_type);
        let spread_type = unwrap_readonly(self.ctx.types, spread_type);

        if matches!(spread_type, TypeId::ANY | TypeId::ERROR) {
            return TypeId::ANY;
        }

        if let Some(element_type) = array_element_type(self.ctx.types, spread_type) {
            return self.evaluate_type_with_env(element_type);
        }

        if let Some(elements) = tuple_elements(self.ctx.types, spread_type) {
            let element_types: Vec<TypeId> = elements.iter().map(|elem| elem.type_id).collect();
            return match element_types.as_slice() {
                [] => TypeId::NEVER,
                [element_type] => self.evaluate_type_with_env(*element_type),
                _ => self.ctx.types.factory().union(element_types),
            };
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, spread_type)
        {
            let mut element_types = Vec::with_capacity(members.len());
            for &member in &members {
                let member = unwrap_readonly(self.ctx.types, self.evaluate_type_with_env(member));
                if matches!(member, TypeId::ANY | TypeId::ERROR) {
                    return TypeId::ANY;
                }
                if let Some(element_type) = array_element_type(self.ctx.types, member) {
                    element_types.push(self.evaluate_type_with_env(element_type));
                    continue;
                }
                if let Some(elements) = tuple_elements(self.ctx.types, member) {
                    element_types.extend(elements.iter().map(|elem| elem.type_id));
                    continue;
                }

                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    spread_child_idx,
                    diagnostic_messages::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE,
                    diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE,
                );
                return TypeId::ANY;
            }

            return match element_types.as_slice() {
                [] => TypeId::NEVER,
                [element_type] => self.evaluate_type_with_env(*element_type),
                _ => self.ctx.types.factory().union(element_types),
            };
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            spread_child_idx,
            diagnostic_messages::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE,
        );
        TypeId::ANY
    }

    /// Check TS2745/TS2746 from one normalized children-shape path.
    pub(super) fn check_jsx_children_shape(
        &mut self,
        props_type: TypeId,
        attributes_idx: NodeIndex,
        child_count: usize,
        has_text_child: bool,
        synthesized_children_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        let children_prop_name = self.get_jsx_children_prop_name();
        let Some(children_type) = self.get_jsx_children_prop_type(props_type) else {
            return;
        };

        match child_count {
            0 => {}
            1 => {
                if self.single_jsx_child_satisfies_children_type(
                    children_type,
                    synthesized_children_type,
                    has_text_child,
                ) {
                    return;
                }

                if !self.type_requires_multiple_children(children_type) {
                    self.check_jsx_single_child_assignable(
                        attributes_idx,
                        children_type,
                        synthesized_children_type,
                        has_text_child,
                    );
                    return;
                }

                let children_type_str = self.format_type(children_type);
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    tag_name_idx,
                    diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING,
                    &[&children_prop_name, &children_type_str],
                );
            }
            _ => {
                if self.type_allows_multiple_children(children_type) {
                    if has_text_child && !self.children_type_accepts_text(children_type) {
                        return;
                    }
                    self.check_jsx_multiple_children_assignable(
                        attributes_idx,
                        children_type,
                        tag_name_idx,
                    );
                    return;
                }

                let child_nodes = self
                    .get_jsx_body_child_nodes(attributes_idx)
                    .unwrap_or_default();
                if !self.type_is_standard_jsx_child_type(children_type)
                    && self.report_invalid_jsx_multi_children(
                        attributes_idx,
                        children_type,
                        &child_nodes,
                    )
                {
                    return;
                }

                let children_type_str = self.format_type(children_type);
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    tag_name_idx,
                    diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO,
                    &[&children_prop_name, &children_type_str],
                );
            }
        }
    }

    fn single_jsx_child_satisfies_children_type(
        &mut self,
        children_type: TypeId,
        actual_child_type: TypeId,
        has_text_child: bool,
    ) -> bool {
        if matches!(actual_child_type, TypeId::ANY | TypeId::ERROR) {
            return true;
        }

        if has_text_child && !self.children_type_accepts_text(children_type) {
            return true;
        }

        self.is_assignable_to(actual_child_type, children_type)
    }

    pub(super) fn get_jsx_children_prop_type(&mut self, props_type: TypeId) -> Option<TypeId> {
        if let Some(children_type) = self.get_specific_jsx_union_children_prop_type(props_type) {
            return Some(children_type);
        }

        if let Some(children_type) =
            self.get_specific_jsx_intersection_children_prop_type(props_type)
        {
            return Some(children_type);
        }

        let resolved = self.resolve_type_for_property_access(props_type);
        let children_prop_name = self.get_jsx_children_prop_name();
        let children_type =
            match self.resolve_property_access_with_env(resolved, &children_prop_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return None,
            };
        let children_type = self.evaluate_type_with_env(children_type);
        if matches!(children_type, TypeId::ANY | TypeId::ERROR) {
            return None;
        }
        Some(children_type)
    }

    pub(super) fn normalize_jsx_props_member_for_children_resolution(
        &mut self,
        props_type: TypeId,
    ) -> TypeId {
        let props_type = self.resolve_type_for_property_access(props_type);
        let props_type = self.evaluate_type_with_env(props_type);

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, props_type)
        {
            let mut best_member = None;
            let mut best_score = 0;
            for member in members {
                let normalized_member = self.strip_jsx_readonly_application_alias(member);
                let Some(children_type) = self.get_direct_jsx_children_prop_type(normalized_member)
                else {
                    continue;
                };
                let score = if self.type_has_jsx_children_callable_signature(children_type) {
                    3
                } else if children_type == TypeId::NEVER {
                    2
                } else {
                    1
                };
                if score > best_score {
                    best_score = score;
                    best_member = Some(normalized_member);
                }
            }
            if let Some(best_member) = best_member {
                return best_member;
            }
        }

        self.strip_jsx_readonly_application_alias(props_type)
    }

    fn get_specific_jsx_union_children_prop_type(&mut self, props_type: TypeId) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, props_type)?;
        let mut callable_candidates = Vec::new();
        let mut other_candidates = Vec::new();
        let mut callable_seen = rustc_hash::FxHashSet::default();
        let mut other_seen = rustc_hash::FxHashSet::default();

        for member in members {
            let member = self.normalize_jsx_props_member_for_children_resolution(member);
            let Some(children_type) = self
                .get_specific_jsx_intersection_children_prop_type(member)
                .or_else(|| self.get_direct_jsx_children_prop_type(member))
            else {
                continue;
            };

            let key = self.format_type(children_type);
            if self.type_has_jsx_children_callable_signature(children_type) {
                if callable_seen.insert(key) {
                    callable_candidates.push(children_type);
                }
            } else if other_seen.insert(key) {
                other_candidates.push(children_type);
            }
        }

        match callable_candidates.len() {
            0 => match other_candidates.len() {
                0 => None,
                1 => other_candidates.into_iter().next(),
                _ => Some(self.ctx.types.factory().union(other_candidates)),
            },
            1 if other_candidates.is_empty() => callable_candidates.into_iter().next(),
            _ if other_candidates.is_empty() => {
                Some(self.ctx.types.factory().union(callable_candidates))
            }
            _ => {
                callable_candidates.extend(other_candidates);
                Some(self.ctx.types.factory().union(callable_candidates))
            }
        }
    }

    fn get_specific_jsx_intersection_children_prop_type(
        &mut self,
        props_type: TypeId,
    ) -> Option<TypeId> {
        let members =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, props_type)?;
        let mut callable_candidates = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        for member in members {
            let Some(children_type) = self.get_direct_jsx_children_prop_type(member) else {
                continue;
            };
            if !self.type_has_jsx_children_callable_signature(children_type) {
                continue;
            }

            let key = self.format_type(children_type);
            if seen.insert(key) {
                callable_candidates.push(children_type);
            }
        }

        match callable_candidates.len() {
            0 => None,
            1 => callable_candidates.into_iter().next(),
            _ => Some(self.ctx.types.factory().union(callable_candidates)),
        }
    }

    fn get_direct_jsx_children_prop_type(&mut self, props_type: TypeId) -> Option<TypeId> {
        let resolved = self.resolve_type_for_property_access(props_type);
        let children_prop_name = self.get_jsx_children_prop_name();
        let children_type =
            match self.resolve_property_access_with_env(resolved, &children_prop_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return None,
            };
        let children_type = self.evaluate_type_with_env(children_type);
        if matches!(children_type, TypeId::ANY | TypeId::ERROR) {
            return None;
        }
        Some(children_type)
    }

    fn type_has_jsx_children_callable_signature(&self, type_id: TypeId) -> bool {
        tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id)
            .is_some_and(|shape| !shape.is_constructor)
            || tsz_solver::type_queries::get_call_signatures(self.ctx.types, type_id)
                .is_some_and(|sigs| !sigs.is_empty())
    }

    fn strip_jsx_readonly_application_alias(&mut self, type_id: TypeId) -> TypeId {
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.evaluate_type_with_env(type_id);
        if let Some((base, args)) =
            tsz_solver::type_queries::get_application_info(self.ctx.types, type_id)
            && args.len() == 1
            && self.format_type(base) == "Readonly"
        {
            return self.resolve_type_for_property_access(args[0]);
        }
        type_id
    }

    fn children_type_accepts_text(&mut self, children_type: TypeId) -> bool {
        self.is_assignable_to(TypeId::STRING, children_type)
    }

    fn check_jsx_multiple_children_assignable(
        &mut self,
        attributes_idx: NodeIndex,
        children_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        let Some(actual_children_type) =
            self.get_precise_jsx_children_body_type(attributes_idx, children_type)
        else {
            return;
        };

        if actual_children_type == TypeId::ANY || actual_children_type == TypeId::ERROR {
            return;
        }
        if self.is_assignable_to(actual_children_type, children_type) {
            return;
        }

        self.check_assignable_or_report_at(
            actual_children_type,
            children_type,
            tag_name_idx,
            tag_name_idx,
        );
    }

    fn check_jsx_single_child_assignable(
        &mut self,
        attributes_idx: NodeIndex,
        children_type: TypeId,
        actual_child_type: TypeId,
        has_text_child: bool,
    ) {
        if matches!(actual_child_type, TypeId::ANY | TypeId::ERROR) {
            return;
        }

        if has_text_child && !self.children_type_accepts_text(children_type) {
            return;
        }

        if self.is_assignable_to(actual_child_type, children_type) {
            return;
        }

        let Some(child_idx) = self
            .get_jsx_body_child_nodes(attributes_idx)
            .and_then(|children| children.into_iter().next())
        else {
            return;
        };

        let diag_node = if let Some(child_node) = self.ctx.arena.get(child_idx) {
            if child_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                self.ctx
                    .arena
                    .get_jsx_expression(child_node)
                    .map(|expr_data| expr_data.expression)
                    .filter(|&expr_idx| expr_idx != NodeIndex::NONE)
                    .unwrap_or(child_idx)
            } else {
                child_idx
            }
        } else {
            child_idx
        };

        if self.report_jsx_single_child_constructor_instance_mismatch(
            diag_node,
            actual_child_type,
            children_type,
        ) {
            return;
        }

        self.check_assignable_or_report_at(actual_child_type, children_type, diag_node, diag_node);
    }

    fn report_jsx_single_child_constructor_instance_mismatch(
        &mut self,
        diag_node: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        let instance_type_from_symbol = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, diag_node)
            .and_then(|sym_id| {
                let lib_binders = self.get_lib_binders();
                let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                ((symbol.flags & tsz_binder::symbol_flags::CLASS) != 0)
                    .then(|| self.class_instance_type_from_symbol(sym_id))
                    .flatten()
            });
        let Some(instance_type) = instance_type_from_symbol.or_else(|| {
            crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                self.ctx.types,
                source_type,
            )
        }) else {
            return false;
        };

        let resolved_target = self.resolve_type_for_property_access(target_type);
        let resolved_instance = self.resolve_type_for_property_access(instance_type);
        if !(self.is_assignable_to(resolved_instance, resolved_target)
            && self.is_assignable_to(resolved_target, resolved_instance))
        {
            return false;
        }

        let resolved_source = self.resolve_type_for_property_access(source_type);
        let Some(target_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_target)
        else {
            return false;
        };
        let source_props: rustc_hash::FxHashSet<_> =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_source)
                .map(|shape| shape.properties.iter().map(|prop| prop.name).collect())
                .unwrap_or_default();
        let missing_names: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|prop| !prop.optional && !source_props.contains(&prop.name))
            .map(|prop| prop.name)
            .collect();
        if missing_names.len() <= 1 {
            return false;
        }

        let source_str = self.format_type(source_type);
        let target_str = self.format_type(target_type);
        let props_joined = missing_names
            .iter()
            .take(4)
            .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
            .collect::<Vec<_>>()
            .join(", ");

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        if missing_names.len() > 4 {
            let more_count = (missing_names.len() - 4).to_string();
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                &[&source_str, &target_str, &props_joined, &more_count],
            );
            self.error_at_node(
                diag_node,
                &message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
            );
        } else {
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                &[&source_str, &target_str, &props_joined],
            );
            self.error_at_node(
                diag_node,
                &message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
            );
        }
        true
    }

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

    fn report_invalid_jsx_multi_children(
        &mut self,
        attributes_idx: NodeIndex,
        children_type: TypeId,
        child_nodes: &[NodeIndex],
    ) -> bool {
        let valid_child_type = self.get_valid_jsx_multi_child_type();
        let contextual_type = self
            .get_jsx_opening_element_from_attributes(attributes_idx)
            .and_then(|opening_idx| self.get_jsx_children_contextual_type(opening_idx));
        let mut emitted = false;

        for &child_idx in child_nodes {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };
            if child_node.kind == tsz_scanner::SyntaxKind::JsxText as u16 {
                continue;
            }

            let (diag_node, type_node) = if child_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                if let Some(expr_data) = self.ctx.arena.get_jsx_expression(child_node) {
                    let expr_idx = expr_data.expression;
                    (expr_idx, expr_idx)
                } else {
                    (child_idx, child_idx)
                }
            } else {
                (child_idx, child_idx)
            };
            let child_request =
                TypingRequest::with_contextual_type(contextual_type.unwrap_or(children_type));
            let child_type = self.compute_type_of_node_with_request(type_node, &child_request);
            if matches!(child_type, TypeId::ANY | TypeId::ERROR) {
                continue;
            }
            if self.is_assignable_to(child_type, valid_child_type) {
                continue;
            }

            let source_type = self.format_type(child_type);
            let message = format!(
                "Type '{source_type}' is not assignable to type 'boolean | any[] | ReactChild'."
            );
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                diag_node,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            emitted = true;
        }

        emitted
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

    fn get_valid_jsx_multi_child_type(&mut self) -> TypeId {
        let mut members = vec![
            TypeId::STRING,
            TypeId::NUMBER,
            TypeId::BOOLEAN,
            self.ctx.types.factory().array(TypeId::ANY),
        ];
        if let Some(element_type) = self.get_jsx_element_type_for_check() {
            members.push(element_type);
        }
        self.ctx.types.factory().union(members)
    }

    fn get_jsx_opening_element_from_attributes(
        &self,
        attributes_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let opening_idx = self.ctx.arena.get_extended(attributes_idx)?.parent;
        let opening_node = self.ctx.arena.get(opening_idx)?;
        self.ctx.arena.get_jsx_opening(opening_node)?;
        Some(opening_idx)
    }

    fn type_is_standard_jsx_child_type(&mut self, type_id: TypeId) -> bool {
        let valid_child_type = self.get_valid_jsx_multi_child_type();
        self.is_assignable_to(type_id, valid_child_type)
    }

    fn type_has_tuple_like_multiple_children(&mut self, type_id: TypeId) -> bool {
        let type_id = self.evaluate_type_with_env(type_id);

        if tsz_solver::is_tuple_type(self.ctx.types, type_id) {
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
        if self.is_assignable_to(array_of_children, type_id) {
            return true;
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
