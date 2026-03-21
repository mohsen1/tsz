//! JSX props/attribute adaptation: props extraction from components, attribute
//! type-checking (TS2322), overload resolution (TS2769), spread property
//! validation, union props checking, and missing required props (TS2741).

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// A JSX attribute with its source information for overload matching.
struct JsxAttrInfo {
    name: String,
    type_id: TypeId,
    /// Whether this attribute came from a spread (`{...obj}`) vs explicit (`name={val}`)
    from_spread: bool,
}

/// Collected JSX attribute information for overload matching.
struct JsxAttrsInfo {
    /// All attributes (explicit + spread-merged), last-wins for duplicates
    attrs: Vec<JsxAttrInfo>,
    /// Whether any spread attribute exists
    has_spread: bool,
    /// Whether any spread is `any`-typed (covers all props)
    has_any_spread: bool,
}

impl<'a> CheckerState<'a> {
    pub(super) fn compute_normalized_jsx_spread_type_with_request(
        &mut self,
        spread_expr_idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let spread_type = self.compute_type_of_node_with_request(spread_expr_idx, request);
        let spread_type = self.evaluate_type_with_env(spread_type);
        self.resolve_type_for_property_access(spread_type)
    }

    pub(super) fn build_jsx_provided_attrs_object_type(
        &mut self,
        provided_attrs: &[(String, TypeId)],
    ) -> TypeId {
        let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
            .iter()
            .map(|(name, type_id)| {
                let name_atom = self.ctx.types.intern_string(name);
                let display_type = if *type_id == TypeId::BOOLEAN_TRUE {
                    TypeId::BOOLEAN
                } else {
                    *type_id
                };
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: display_type,
                    write_type: display_type,
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
        self.ctx.types.factory().object(properties)
    }

    pub(super) fn should_report_custom_jsx_children_via_assignability(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
    ) -> bool {
        let children_prop_name = self.get_jsx_children_prop_name();
        if children_prop_name == "children" {
            return false;
        }
        if provided_attrs
            .iter()
            .any(|(name, _)| name == &children_prop_name)
        {
            return false;
        }

        let resolved = self.resolve_type_for_property_access(props_type);
        let resolved = self.evaluate_type_with_env(resolved);
        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved)
        else {
            return false;
        };

        shape.properties.iter().any(|prop| {
            !prop.optional && self.ctx.types.resolve_atom(prop.name) == children_prop_name
        })
    }

    pub(super) fn report_jsx_synthesized_props_assignability_error(
        &mut self,
        attrs_type: TypeId,
        display_target: &str,
        tag_name_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let source_str = self.format_type(attrs_type);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, display_target],
        );
        self.error_at_node(
            tag_name_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    pub(super) fn normalize_jsx_required_props_target(&mut self, props_type: TypeId) -> TypeId {
        let normalized = self.evaluate_application_type(props_type);
        let normalized = self.evaluate_type_with_env(normalized);
        let normalized = self.resolve_type_for_property_access(normalized);
        let normalized = self.resolve_lazy_type(normalized);
        let normalized = self.evaluate_application_type(normalized);
        self.evaluate_type_with_env(normalized)
    }

    /// Check that all required properties in the props type are provided. Emits TS2741.
    pub(super) fn check_missing_required_jsx_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
        attributes_idx: NodeIndex,
    ) {
        let Some(shape) = self.get_normalized_jsx_required_props_shape(props_type) else {
            return;
        };

        for prop in &shape.properties {
            if prop.optional {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(prop.name);

            // 'children' is now handled via JsxChildrenContext synthesis in dispatch
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

    pub(super) fn jsx_has_missing_required_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
    ) -> bool {
        let Some(shape) = self.get_normalized_jsx_required_props_shape(props_type) else {
            return false;
        };

        shape.properties.iter().any(|prop| {
            !prop.optional
                && !provided_attrs
                    .iter()
                    .any(|(name, _)| name == &self.ctx.types.resolve_atom(prop.name))
        })
    }

    fn get_normalized_jsx_required_props_shape(
        &mut self,
        props_type: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
        let resolved_props_type = self.normalize_jsx_required_props_target(props_type);
        tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_props_type)
    }
    /// Fallback: check `IntrinsicAttributes` when component props couldn't be extracted.
    pub(super) fn check_jsx_intrinsic_attributes_only(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let intrinsic_attrs_type = self.get_intrinsic_attributes_type();
        let intrinsic_class_attrs_type =
            self.get_intrinsic_class_attributes_type_for_component(component_type);
        if intrinsic_attrs_type.is_none() && intrinsic_class_attrs_type.is_none() {
            return;
        }

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

        if let Some(intrinsic_attrs_type) = intrinsic_attrs_type {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &provided_attrs,
                tag_name_idx,
            );
        }
        if let Some(intrinsic_class_attrs_type) = intrinsic_class_attrs_type {
            self.check_missing_required_jsx_props(
                intrinsic_class_attrs_type,
                &provided_attrs,
                tag_name_idx,
            );
        }
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
    // JSX Component Props Extraction

    /// Extract props type from a JSX component (SFC: first param of call sig;
    /// class: construct sig return → `JSX.ElementAttributesProperty` member).
    /// Returns `(props_type, raw_has_type_params)` where `raw_has_type_params`
    /// suppresses excess property checking when true.
    pub(super) fn get_jsx_props_type_for_component(
        &mut self,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<(TypeId, bool)> {
        let component_type = self.normalize_jsx_component_type_for_resolution(component_type);
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
        {
            return None;
        }

        // G1: Skip component attribute checking when the file has parse errors.
        // Parse errors can cause incorrect AST that leads to false-positive type
        // errors. TSC similarly suppresses some JSX checking in error-recovery
        // situations (e.g., tsxStatelessFunctionComponents1 with TS1005).
        if self.ctx.has_parse_errors {
            return None;
        }

        // Skip type parameters — we can't check attributes against unresolved generics
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return None;
        }

        if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, component_type)
        {
            let mut candidates = Vec::new();
            let mut seen = rustc_hash::FxHashSet::default();
            let mut any_raw_has_type_params = false;
            for member in members {
                let Some((props_type, member_raw_has_type_params)) =
                    self.get_jsx_props_type_for_component_member(member, None)
                else {
                    if self.is_generic_jsx_component(member)
                        || tsz_solver::contains_type_parameters(self.ctx.types, member)
                    {
                        continue;
                    }
                    return None;
                };
                let resolved_props_type = self.resolve_type_for_property_access(props_type);
                let key = self.format_type(resolved_props_type);
                if seen.insert(key) {
                    candidates.push((props_type, member_raw_has_type_params));
                }
                any_raw_has_type_params |= member_raw_has_type_params;
            }
            match candidates.len() {
                0 => return None,
                1 => return candidates.pop(),
                _ => {
                    let props_union = self.ctx.types.factory().union(
                        candidates
                            .into_iter()
                            .map(|(props_type, _)| props_type)
                            .collect(),
                    );
                    return Some((props_union, any_raw_has_type_params));
                }
            }
        }

        self.get_jsx_props_type_for_component_member(component_type, element_idx)
    }

    pub(super) fn get_jsx_props_type_for_component_member(
        &mut self,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<(TypeId, bool)> {
        let component_type = self.normalize_jsx_component_type_for_resolution(component_type);
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
        {
            return None;
        }

        // Try SFC first: get call signatures → first parameter is props type
        if let Some((props, raw_has_tp)) = self.get_sfc_props_type(component_type) {
            return Some((props, raw_has_tp));
        }

        // Try class component: get construct signatures → instance type → props
        if let Some(props) = self.get_class_component_props_type(component_type, element_idx) {
            return Some((props, false));
        }

        None
    }

    /// Emit TS2604 if the component type has no call or construct signatures.
    pub(super) fn check_jsx_element_has_signatures(
        &mut self,
        component_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        // Skip for types that are inherently allowed in JSX position
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
            || component_type == TypeId::NEVER
        {
            return;
        }
        // Skip type parameters — they may resolve to callable types
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return;
        }
        // Skip string-like tag values without going through full assignability.
        // Dynamic tag names like `<Tag>` where `Tag` is `string` or a union of
        // string literals are valid JSX and should be treated like intrinsic
        // element lookups. A structural relation check here is unnecessarily
        // heavy for `React.ReactType`-style unions.
        if self.is_jsx_string_tag_type(component_type)
            && !(self
                .get_jsx_specific_string_literal_component_tag_name(tag_name_idx, component_type)
                .is_some()
                && self.get_intrinsic_elements_type().is_some())
        {
            return;
        }
        // Skip if file has parse errors (avoid cascading diagnostics)
        if self.ctx.has_parse_errors {
            return;
        }

        // Check if the type (or any union member) has call/construct signatures
        let types_to_check = if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, component_type)
        {
            members
        } else {
            vec![component_type]
        };

        let has_signatures = types_to_check.iter().any(|&ty| {
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, ty)
                .is_some_and(|sigs| !sigs.is_empty())
                || tsz_solver::type_queries::get_construct_signatures(self.ctx.types, ty)
                    .is_some_and(|sigs| !sigs.is_empty())
                || tsz_solver::type_queries::get_function_shape(self.ctx.types, ty).is_some()
        });

        if !has_signatures {
            // TSC uses the JSX tag text, not the resolved type.
            let tag_text = self.get_jsx_tag_name_text(tag_name_idx);
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                tag_name_idx,
                diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES,
                &[&tag_text],
            );
        }
    }

    pub(super) fn is_jsx_string_tag_type(&self, type_id: TypeId) -> bool {
        if tsz_solver::type_queries::is_string_type(self.ctx.types, type_id)
            || tsz_solver::type_queries::is_string_literal(self.ctx.types, type_id)
        {
            return true;
        }

        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_jsx_string_tag_type(member));
        }

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&member| self.is_jsx_string_tag_type(member));
        }

        false
    }

    /// TS2786: Check that a JSX component's return type is assignable to
    /// `JSX.Element` (SFC) or `JSX.ElementClass` (class component).
    pub(super) fn check_jsx_component_return_type(
        &mut self,
        component_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
            || component_type == TypeId::NEVER
        {
            return;
        }
        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, component_type) {
            return;
        }
        if self.ctx.has_parse_errors {
            return;
        }

        let jsx_element_type_raw = self.get_jsx_element_type_for_check();
        let jsx_element_class_type_raw = self.get_jsx_element_class_type();
        if jsx_element_type_raw.is_none() && jsx_element_class_type_raw.is_none() {
            return;
        }

        let is_concrete =
            |t: TypeId| t != TypeId::ANY && t != TypeId::ERROR && t != TypeId::UNKNOWN;
        let jsx_element_type = jsx_element_type_raw
            .map(|t| self.evaluate_type_with_env(t))
            .filter(|&t| is_concrete(t));
        let jsx_element_class_type = jsx_element_class_type_raw
            .map(|t| self.evaluate_type_with_env(t))
            .filter(|&t| is_concrete(t));
        if jsx_element_type.is_none() && jsx_element_class_type.is_none() {
            return;
        }

        let types_to_check = if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, component_type)
        {
            members
        } else {
            vec![component_type]
        };

        let mut any_checked = false;
        let mut all_valid = true;

        for &member_type in &types_to_check {
            if !is_concrete(member_type) {
                continue;
            }
            // Skip unresolved Application/Lazy member types (e.g. ComponentClass<any>)
            if tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, member_type) {
                continue;
            }

            let is_unresolved = |t: TypeId| -> bool {
                !is_concrete(t)
                    || tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, t)
            };
            let is_valid_null_like_return = |t: TypeId| -> bool { t == TypeId::NULL };

            let mut is_sfc = false;
            if let Some(shape) =
                tsz_solver::type_queries::get_function_shape(self.ctx.types, member_type)
                && !shape.is_constructor
            {
                is_sfc = true;
                let return_type = self.evaluate_type_with_env(shape.return_type);
                if !is_unresolved(return_type) && !is_valid_null_like_return(return_type) {
                    any_checked = true;
                    if let Some(element_type) = jsx_element_type {
                        // TSC allows null/undefined in SFC return types
                        // (e.g., `() => Element | null` is valid).
                        // Strip null/undefined before checking against JSX.Element.
                        let non_null_return =
                            tsz_solver::remove_nullish(self.ctx.types, return_type);
                        if non_null_return == TypeId::NEVER
                            || !self.is_assignable_to(non_null_return, element_type)
                        {
                            all_valid = false;
                        }
                    }
                }
            }

            // Check call/construct signatures against JSX.Element/ElementClass.
            for (get_sigs_fn, target, is_call_sig) in [
                (
                    tsz_solver::type_queries::get_call_signatures as fn(_, _) -> _,
                    jsx_element_type,
                    true,
                ),
                (
                    tsz_solver::type_queries::get_construct_signatures,
                    jsx_element_class_type,
                    false,
                ),
            ] {
                if !is_sfc
                    && let Some(sigs) = get_sigs_fn(self.ctx.types, member_type)
                    && !sigs.is_empty()
                {
                    let mut any_concrete = false;
                    let any_valid = sigs.iter().any(|sig| {
                        // Skip generic construct signatures — return type has
                        // unresolved type params that can't be checked until
                        // instantiation. Call sigs (SFCs) are still checked.
                        if !is_call_sig && !sig.type_params.is_empty() {
                            return true;
                        }
                        let ret = self.evaluate_type_with_env(sig.return_type);
                        if is_unresolved(ret) || is_valid_null_like_return(ret) {
                            return true;
                        }
                        // For construct sigs, skip if the return type still
                        // contains type parameters (from outer scopes). The
                        // instance type is incomplete until instantiation.
                        if !is_call_sig
                            && tsz_solver::type_queries::data::contains_type_parameters_db(
                                self.ctx.types,
                                ret,
                            )
                        {
                            return true;
                        }
                        any_concrete = true;
                        target.is_none_or(|t| {
                            let check_ret = if is_call_sig {
                                let stripped = tsz_solver::remove_nullish(self.ctx.types, ret);
                                if stripped == TypeId::NEVER {
                                    return true;
                                }
                                stripped
                            } else {
                                ret
                            };
                            self.is_assignable_to(check_ret, t)
                        })
                    });
                    if any_concrete {
                        any_checked = true;
                    }
                    if any_concrete && !any_valid {
                        all_valid = false;
                    }
                }
            }
        }

        if any_checked && !all_valid {
            let tag_text = self.get_jsx_tag_name_text(tag_name_idx);
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                tag_name_idx,
                diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT,
                &[&tag_text],
            );
        }
    }

    /// Extract props type from a Stateless Function Component (first param of call sig).
    fn get_sfc_props_type(&mut self, component_type: TypeId) -> Option<(TypeId, bool)> {
        // Check Function type (single signature)
        if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, component_type)
            && !shape.is_constructor
        {
            // Skip generic SFCs — we can't infer type args without full inference
            if !shape.type_params.is_empty() {
                return None;
            }
            let props = shape
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            // Check for type parameters BEFORE evaluation, since evaluation may
            // collapse `T & {children?: ReactNode}` into a concrete object type
            // that loses the type parameter information.
            let raw_has_type_params = tsz_solver::contains_type_parameters(self.ctx.types, props);
            // When the raw props type is already a union (e.g., discriminated unions like
            // `{ variant: Avatar } | { variant: OneLine }`), skip full evaluation.
            // The type evaluator may incorrectly merge union members with the same
            // property names into a single object, losing the discriminated union
            // structure needed for correct assignability checking.
            let evaluated = if tsz_solver::is_union_type(self.ctx.types, props) {
                props
            } else {
                self.evaluate_type_with_env(props)
            };
            return Some((evaluated, raw_has_type_params));
        }

        // Check Callable type (overloaded signatures)
        if let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
            && !sigs.is_empty()
        {
            // G4: Skip overloaded SFCs — we don't do JSX overload resolution.
            // Picking the first non-generic overload would produce wrong errors
            // when later overloads match the provided attributes.
            let non_generic: Vec<_> = sigs.iter().filter(|s| s.type_params.is_empty()).collect();
            if non_generic.len() != 1 {
                return None;
            }
            let sig = non_generic[0];
            let props = sig
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            let raw_has_type_params = tsz_solver::contains_type_parameters(self.ctx.types, props);
            let evaluated = self.evaluate_type_with_env(props);
            return Some((evaluated, raw_has_type_params));
        }

        None
    }

    /// Check if a component type is an overloaded SFC (>= 2 non-generic call signatures).
    pub(super) fn is_overloaded_sfc(&self, component_type: TypeId) -> bool {
        let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
        else {
            return false;
        };
        let non_generic_count = sigs.iter().filter(|s| s.type_params.is_empty()).count();
        non_generic_count >= 2
    }

    /// Check if a component type has generic call or construct signatures.
    pub(super) fn is_generic_jsx_component(&self, component_type: TypeId) -> bool {
        if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, component_type)
            && !shape.is_constructor
            && !shape.type_params.is_empty()
        {
            return true;
        }
        if let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
            && sigs.iter().any(|s| !s.type_params.is_empty())
        {
            return true;
        }
        if let Some(sigs) =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)
            && sigs.iter().any(|s| !s.type_params.is_empty())
        {
            return true;
        }
        false
    }

    /// Extract props type from a class component via construct signatures.
    fn get_class_component_props_type(
        &mut self,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<TypeId> {
        let sigs =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)?;
        if sigs.is_empty() {
            return None;
        }

        // Get instance type from the first construct signature
        let sig = sigs.first()?;

        // G3: Skip generic class components — we can't infer type arguments
        // without full generic type inference for JSX elements
        if !sig.type_params.is_empty() {
            return None;
        }

        let instance_type = sig.return_type;
        if instance_type == TypeId::ANY || instance_type == TypeId::ERROR {
            return None;
        }

        // Evaluate Application/Lazy instance types to their structural form.
        // e.g. `Component<{reqd: any}, any>` is an Application that evaluates
        // to a concrete object. Only skip if evaluation still yields a type
        // with unresolved type parameters (outer generic context).
        let instance_type = if tsz_solver::type_queries::needs_evaluation_for_merge(
            self.ctx.types,
            instance_type,
        ) {
            let evaluated = self.evaluate_type_with_env(instance_type);
            // After evaluation, if the type still contains type parameters,
            // we can't resolve it further — bail out.
            if tsz_solver::contains_type_parameters(self.ctx.types, evaluated) {
                return None;
            }
            evaluated
        } else {
            instance_type
        };

        // Look up ElementAttributesProperty to know which instance property is props
        // Pass element_idx so TS2608 can be emitted if >1 property
        let prop_name = self.get_element_attributes_property_name_with_check(element_idx);

        match prop_name {
            None => {
                // G2: No ElementAttributesProperty → no JSX infrastructure.
                // TSC skips attribute checking when JSX types aren't configured.
                None
            }
            Some(ref name) if name.is_empty() => {
                // Empty ElementAttributesProperty → instance type IS the props
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                Some(evaluated_instance)
            }
            Some(ref name) => {
                // ElementAttributesProperty has a member → access that property on instance
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(evaluated_instance, name) {
                    PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                    // Instance type doesn't have the ElementAttributesProperty member.
                    // This can happen when class inheritance doesn't include inherited
                    // members in the construct signature return type.
                    // Fall back to the first construct parameter as props type (the
                    // common React pattern: `new(props: P)`). If no suitable fallback,
                    // emit TS2607.
                    _ => {
                        // Try first construct param as fallback (React-style: new(props: P))
                        if let Some(first_param) = sig.params.first() {
                            let param_type = self.evaluate_type_with_env(first_param.type_id);
                            if param_type != TypeId::ANY
                                && param_type != TypeId::ERROR
                                && param_type != TypeId::STRING
                                && param_type != TypeId::NUMBER
                            {
                                return Some(param_type);
                            }
                        }
                        if let Some(elem_idx) = element_idx {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node_msg(
                                elem_idx,
                                diagnostic_codes::JSX_ELEMENT_CLASS_DOES_NOT_SUPPORT_ATTRIBUTES_BECAUSE_IT_DOES_NOT_HAVE_A_PROPERT,
                                &[name],
                            );
                        }
                        None
                    }
                }
            }
        }
    }

    /// Get the property name from `JSX.ElementAttributesProperty`.
    /// Returns None/Some("")/Some("name"); emits TS2608 if >1 property.
    pub(super) fn get_element_attributes_property_name_with_check(
        &mut self,
        element_idx: Option<NodeIndex>,
    ) -> Option<String> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let eap_sym_id = exports.get("ElementAttributesProperty")?;

        // Get the type of ElementAttributesProperty
        let eap_type = self.type_reference_symbol_type(eap_sym_id);
        let evaluated = self.evaluate_type_with_env(eap_type);

        // If the type couldn't be resolved (unknown/error), the symbol's declarations
        // are likely in a different file's arena (cross-file project mode). Fall back to
        // "props" as the standard JSX convention — all React types and most JSX configs
        // use `{ props: {} }` as the ElementAttributesProperty interface.
        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR {
            return Some("props".to_string());
        }

        // Check if it has any properties
        if let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated) {
            if shape.properties.is_empty() {
                return Some(String::new()); // Empty interface
            }
            // TS2608: ElementAttributesProperty may not have more than one property
            if shape.properties.len() > 1 {
                if let Some(elem_idx) = element_idx {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        elem_idx,
                        diagnostic_codes::THE_GLOBAL_TYPE_JSX_MAY_NOT_HAVE_MORE_THAN_ONE_PROPERTY,
                        &["ElementAttributesProperty"],
                    );
                }
                return Some(String::new());
            }
            // Return the name of the first (and typically only) property
            if let Some(first_prop) = shape.properties.first() {
                return Some(self.ctx.types.resolve_atom(first_prop.name));
            }
        }

        Some(String::new()) // Default: empty (instance type is props)
    }
    // JSX Children Contextual Typing

    pub(super) fn collect_jsx_union_resolution_attrs(
        &mut self,
        attributes_idx: NodeIndex,
    ) -> Option<Vec<(String, Option<TypeId>)>> {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return Some(Vec::new());
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return Some(Vec::new());
        };

        let mut provided = Vec::new();
        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                return None;
            }
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                continue;
            };
            let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                continue;
            };
            if matches!(attr_name.as_str(), "key" | "ref") {
                continue;
            }

            let attr_type = if attr_data.initializer.is_none() {
                Some(TypeId::BOOLEAN_TRUE)
            } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                    self.ctx
                        .arena
                        .get_jsx_expression(init_node)
                        .map(|expr| expr.expression)
                        .unwrap_or(attr_data.initializer)
                } else {
                    attr_data.initializer
                };
                if let Some(value_node) = self.ctx.arena.get(value_idx)
                    && matches!(
                        value_node.kind,
                        syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                    )
                {
                    None
                } else {
                    let prev = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = true;
                    let ty = self.compute_type_of_node(value_idx);
                    self.ctx.preserve_literal_types = prev;
                    Some(ty)
                }
            } else {
                Some(TypeId::ANY)
            };

            provided.push((attr_name, attr_type));
        }

        let children_prop_name = self.get_jsx_children_prop_name();
        if self
            .get_jsx_body_child_nodes(attributes_idx)
            .is_some_and(|children| !children.is_empty())
            && !provided.iter().any(|(name, _)| name == &children_prop_name)
        {
            provided.push((children_prop_name, None));
        }

        Some(provided)
    }

    pub(super) fn narrow_jsx_props_union_from_attributes(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
    ) -> TypeId {
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, props_type)
        else {
            return props_type;
        };

        let Some(provided_attrs) = self.collect_jsx_union_resolution_attrs(attributes_idx) else {
            return props_type;
        };
        let children_prop_name = self.get_jsx_children_prop_name();
        let provided_names: rustc_hash::FxHashSet<&str> = provided_attrs
            .iter()
            .map(|(name, _type_id)| name.as_str())
            .collect();
        let prefer_children_specificity = provided_names.contains(children_prop_name.as_str());

        let compatible: Vec<TypeId> = members
            .into_iter()
            .filter(|&member| {
                let member = self.resolve_type_for_property_access(member);

                let attrs_match = provided_attrs.iter().all(|(name, attr_type)| {
                    use crate::query_boundaries::common::PropertyAccessResult;
                    match self.resolve_property_access_with_env(member, name) {
                        PropertyAccessResult::Success { type_id, .. } => {
                            let expected = tsz_solver::remove_undefined(self.ctx.types, type_id);
                            match attr_type {
                                Some(attr_type) => {
                                    *attr_type == TypeId::ANY
                                        || *attr_type == TypeId::ERROR
                                        || self.is_assignable_to(*attr_type, expected)
                                }
                                None => expected != TypeId::NEVER && expected != TypeId::ERROR,
                            }
                        }
                        _ => false,
                    }
                });
                if !attrs_match {
                    return false;
                }

                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, member)
                {
                    shape.properties.iter().all(|prop| {
                        if prop.optional {
                            return true;
                        }
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        prop_name.as_str() == children_prop_name.as_str()
                            || provided_names.contains(prop_name.as_str())
                    })
                } else {
                    true
                }
            })
            .collect();

        match compatible.len() {
            0 => props_type,
            1 => {
                if prefer_children_specificity {
                    self.normalize_jsx_props_member_for_children_resolution(compatible[0])
                } else {
                    compatible[0]
                }
            }
            _ if !prefer_children_specificity => props_type,
            _ => {
                let mut normalized_members = Vec::new();
                let mut seen = rustc_hash::FxHashSet::default();
                for member in compatible {
                    let member = self.normalize_jsx_props_member_for_children_resolution(member);
                    let key = self.format_type(member);
                    if seen.insert(key) {
                        normalized_members.push(member);
                    }
                }

                match normalized_members.len() {
                    0 => props_type,
                    1 => normalized_members[0],
                    _ => self.ctx.types.factory().union(normalized_members),
                }
            }
        }
    }
    /// Check JSX attributes against an already-evaluated props type.
    ///
    /// For each attribute, checks that the assigned value is assignable to the
    /// expected property type from the props interface. Emits:
    /// - TS2322 for type mismatches and excess properties
    /// - TS2741 for missing required properties
    ///
    /// `display_target` is the pre-formatted string shown in TS2322 error messages
    /// for excess properties. tsc uses `IntrinsicAttributes & PropsType` (or
    /// `IntrinsicAttributes & IntrinsicClassAttributes<T> & PropsType`) rather
    /// than just `PropsType`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_jsx_attributes_against_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        component_type: Option<TypeId>,
        raw_props_has_type_params: bool,
        display_target: String,
        request: &TypingRequest,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        // Grammar check: TS17000 for empty expressions in JSX attributes.
        // Matches tsc: only the first empty expression per element is reported.
        self.check_grammar_jsx_element(attributes_idx);

        // Resolve Lazy(DefId) props before any checks (TS2698 needs this too).
        let props_type = self.resolve_type_for_property_access(props_type);

        // Union props: delegate to whole-object assignability checking.
        if tsz_solver::is_union_type(self.ctx.types, props_type) {
            self.check_jsx_union_props(attributes_idx, props_type, tag_name_idx, children_ctx);
            return;
        }
        // Skip attribute-vs-props checking for any/error props.
        let skip_prop_checks = props_type == TypeId::ANY
            || props_type == TypeId::ERROR
            || tsz_solver::contains_error_type(self.ctx.types, props_type);

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // String index signature → any attribute name is valid.
        let has_string_index =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
                .is_some_and(|shape| shape.string_index.is_some());

        // Suppress excess-property errors when props has unresolved type params.
        // Check both raw and evaluated props (evaluation may collapse type params).
        let props_has_type_params = raw_props_has_type_params
            || tsz_solver::contains_type_parameters(self.ctx.types, props_type);

        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let mut spread_covers_all = false;
        let mut has_excess_property_error = false;
        let mut needs_special_attr_object_assignability = false;

        // TS2783: track explicit attr names for spread overwrite detection.
        let mut named_attr_nodes: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();

        // Deferred spread entries: (spread_type, expr_idx, attr_index) for TS2322.
        let mut spread_entries: Vec<(TypeId, NodeIndex, usize)> = Vec::new();

        // Check each attribute
        let attr_nodes = &attrs.properties.nodes;
        for (attr_i, &attr_idx) in attr_nodes.iter().enumerate() {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                // Regular JSX attribute: name={value}
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };

                // Get attribute name (handles both simple and namespaced names like `ns:attr`)
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };

                // Track all attributes for missing-prop checking (including key/ref).
                // Even though key/ref are not checked against component props for TYPE
                // compatibility (they come from IntrinsicAttributes/IntrinsicClassAttributes),
                // they still need to be tracked as "provided" so the IntrinsicAttributes
                // missing-required-property check knows they were given.
                // Type will be filled in later after compute_type_of_node is called.
                provided_attrs.push((attr_name.clone(), TypeId::ANY));

                // Skip type-checking 'key' and 'ref' against component props.
                // These are special JSX attributes managed by IntrinsicAttributes /
                // IntrinsicClassAttributes, not by component props directly.
                // Checking them against the props type produces false positives when the
                // props type is an unevaluated application (e.g. DetailedHTMLProps<...>).
                if attr_name == "key" || attr_name == "ref" {
                    let attr_value_type = if attr_data.initializer.is_none() {
                        TypeId::BOOLEAN_TRUE
                    } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                        let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                            self.ctx
                                .arena
                                .get_jsx_expression(init_node)
                                .map(|e| e.expression)
                                .unwrap_or(attr_data.initializer)
                        } else {
                            attr_data.initializer
                        };
                        tsz_solver::widening::widen_type(
                            self.ctx.types,
                            self.compute_type_of_node(value_idx),
                        )
                    } else {
                        TypeId::ANY
                    };
                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = attr_value_type;
                    }
                    continue;
                }

                // Track for TS2783 spread-overwrite detection
                named_attr_nodes.insert(attr_name.clone(), attr_data.name);

                // Skip prop-type checking when props type is any/error/contains-error
                if skip_prop_checks {
                    continue;
                }

                // Get expected type from props
                use crate::query_boundaries::common::PropertyAccessResult;
                let is_data_or_aria =
                    attr_name.starts_with("data-") || attr_name.starts_with("aria-");
                let is_special_named_attr = attr_name.contains('-') || attr_name.contains(':');
                let (expected_type, expected_type_is_boolean_literal) = match self
                    .resolve_property_access_with_env(props_type, &attr_name)
                {
                    PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => {
                        // data-*/aria-* via index signature: skip (HTML convention).
                        if is_data_or_aria && from_index_signature {
                            continue;
                        }
                        let write_check_type =
                            tsz_solver::remove_undefined(self.ctx.types, type_id);
                        // Strip undefined from optional props (write-position checking).
                        (
                            write_check_type,
                            matches!(type_id, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE),
                        )
                    }
                    PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                        let Some(type_id) = property_type else {
                            continue;
                        };
                        let write_check_type =
                            tsz_solver::remove_undefined(self.ctx.types, type_id);
                        (
                            write_check_type,
                            matches!(type_id, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE),
                        )
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        // Compute actual value type (replacing ANY placeholder) for error messages.
                        let attr_value_type = if attr_data.initializer.is_none() {
                            TypeId::BOOLEAN_TRUE // shorthand boolean literal
                        } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                            let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                self.ctx
                                    .arena
                                    .get_jsx_expression(init_node)
                                    .map(|e| e.expression)
                                    .unwrap_or(attr_data.initializer)
                            } else {
                                attr_data.initializer
                            };
                            self.compute_type_of_node(value_idx)
                        } else {
                            TypeId::ANY
                        };
                        if let Some(entry) = provided_attrs.last_mut() {
                            entry.1 = attr_value_type;
                        }

                        if !has_string_index // excess property check
                            && !props_has_type_params
                            && !attr_name.starts_with("data-")
                            && !attr_name.starts_with("aria-")
                        {
                            let attr_type_name = if attr_data.initializer.is_none() {
                                "true".to_string()
                            } else {
                                self.format_type(attr_value_type)
                            };
                            let message = format!(
                                "Type '{{ {attr_name}: {attr_type_name}; }}' is not assignable to type '{display_target}'.\n  \
                                     Object literal may only specify known properties, \
                                     and '{attr_name}' does not exist in type '{display_target}'."
                            );
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                attr_idx,
                                &message,
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            );
                            has_excess_property_error = true;
                        }
                        continue;
                    }
                    _ => continue,
                };

                // Check attribute value assignability
                if attr_data.initializer.is_none() {
                    // Shorthand boolean is represented as `true`, but TS reports the
                    // source as `true` only for boolean-literal targets and `boolean`
                    // for non-literal targets (for example `string`).
                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = TypeId::BOOLEAN_TRUE;
                    }
                    if !self.is_assignable_to(TypeId::BOOLEAN_TRUE, expected_type) {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let source_str = if expected_type_is_boolean_literal {
                            "true"
                        } else {
                            "boolean"
                        };
                        let target_str = self.format_type(expected_type);
                        let message = format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[source_str, &target_str],
                        );
                        self.error_at_node(
                            attr_data.name,
                            &message,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                    }
                    continue;
                }

                // The initializer might be a JSX expression wrapper or a string literal
                let value_node_idx =
                    if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                        if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                            // Unwrap JSX expression to get the actual expression
                            if let Some(jsx_expr) = self.ctx.arena.get_jsx_expression(init_node) {
                                jsx_expr.expression
                            } else {
                                continue;
                            }
                        } else {
                            // String literal or other expression
                            attr_data.initializer
                        }
                    } else {
                        continue;
                    };

                // TS2783: Check if a later spread overwrites this attr (skip type check if so).
                let overwritten = self.check_jsx_attr_overwritten_by_spread(
                    &attr_name,
                    attr_data.name,
                    attr_nodes,
                    attr_i,
                );

                if !overwritten {
                    // Set contextual type to preserve narrow literal types.
                    let actual_type = self.compute_type_of_node_with_request(
                        value_node_idx,
                        &request.read().normal_origin().contextual(expected_type),
                    );

                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = actual_type;
                    }
                    if is_special_named_attr {
                        if actual_type != TypeId::ANY
                            && actual_type != TypeId::ERROR
                            && !self.is_assignable_to(actual_type, expected_type)
                        {
                            needs_special_attr_object_assignability = true;
                        }
                        continue;
                    }
                    // Assignability check — tsc anchors at the attribute NAME.
                    if actual_type != TypeId::ANY && actual_type != TypeId::ERROR {
                        self.check_assignable_or_report_at(
                            actual_type,
                            expected_type,
                            value_node_idx,
                            attr_data.name,
                        );
                    }
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_expr_idx = spread_data.expression;
                // Set contextual type so spread literals preserve narrow types.
                let spread_request = if !skip_prop_checks {
                    request.read().normal_origin().contextual(props_type)
                } else {
                    request.read().normal_origin().contextual_opt(None)
                };
                let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                    spread_expr_idx,
                    &spread_request,
                );

                // any/error/unknown spread covers all properties.
                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    // Mark all required props as provided (any spread covers everything)
                    spread_covers_all = true;
                    continue;
                }

                // TS2698: Validate spread type is object-like.
                // tsc rejects spreading `null`, `undefined`, `never`, primitives in JSX.
                // This runs regardless of skip_prop_checks — it's independent of props type.
                let resolved = self.resolve_lazy_type(spread_type);
                if resolved == TypeId::NEVER
                    || !crate::query_boundaries::type_computation::access::is_valid_spread_type(
                        self.ctx.types,
                        resolved,
                    )
                {
                    self.report_spread_not_object_type(spread_expr_idx);
                    continue;
                }

                // TS2783: Check if any earlier explicit attributes will be
                // overwritten by required (non-optional) properties from this spread.
                if !named_attr_nodes.is_empty() {
                    let spread_props = self.collect_object_spread_properties(spread_type);
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            if let Some(&attr_name_idx) = named_attr_nodes.get(&sp_name) {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                    &[&sp_name],
                                );
                                self.error_at_node(
                                    attr_name_idx,
                                    &message,
                                    diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                );
                            }
                        }
                    }
                    // Clear required spread props from tracking.
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            named_attr_nodes.remove(&sp_name);
                        }
                    }
                }

                // Extract spread props for TS2741 tracking.
                if let Some(spread_shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    for prop in &spread_shape.properties {
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        provided_attrs.push((prop_name.to_string(), prop.type_id));
                    }
                }

                // Defer TS2322 spread checking until after attribute override tracking.
                if !skip_prop_checks {
                    spread_entries.push((spread_type, spread_expr_idx, attr_i));
                }
            }
        }

        // TS2322: Check spread props against expected types (deferred to account for overrides).
        if !spread_entries.is_empty() {
            let mut explicit_attr_names_with_pos: Vec<(usize, String)> = Vec::new();
            let mut suppress_missing_props_from_spread = false;
            for (i, &node_idx) in attr_nodes.iter().enumerate() {
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };
                if node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                    && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(node)
                    && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                    && let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                {
                    explicit_attr_names_with_pos.push((i, attr_name));
                }
            }

            for &(spread_type, _spread_expr_idx, spread_pos) in &spread_entries {
                // Only later explicit attributes override the current spread.
                let overridden: rustc_hash::FxHashSet<&str> = explicit_attr_names_with_pos
                    .iter()
                    .filter(|(attr_pos, _)| *attr_pos > spread_pos)
                    .map(|(_, name)| name.as_str())
                    .collect();

                suppress_missing_props_from_spread |= self.check_spread_property_types(
                    spread_type,
                    props_type,
                    tag_name_idx,
                    &overridden,
                    &display_target,
                );
            }

            if suppress_missing_props_from_spread {
                spread_covers_all = true;
            }
        }

        // JSX children synthesis: incorporate body children into provided props.
        if let Some(crate::checkers_domain::JsxChildrenContext {
            child_count,
            has_text_child,
            synthesized_type,
            text_child_indices,
        }) = children_ctx
        {
            // TS2710: explicit children attr + body children = double specification.
            // Error location: the first JSX attribute (matching tsc's span).
            let children_prop_name = self.get_jsx_children_prop_name();
            let has_explicit_children_attr =
                self.has_explicit_jsx_attribute(attributes_idx, &children_prop_name);
            if has_explicit_children_attr && !skip_prop_checks {
                // tsc reports TS2710 on the JsxAttributes node, which spans from
                // the first attribute to the closing >. Our parser sets JsxAttributes.pos
                // to the first token after the tag name, matching tsc's behavior.
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    attributes_idx,
                    diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN,
                    &[&children_prop_name],
                );
            }

            provided_attrs.push((children_prop_name.clone(), synthesized_type));
            if child_count > 0
                && !has_explicit_children_attr
                && !skip_prop_checks
                && !has_excess_property_error
                && !has_string_index
                && !props_has_type_params
                && !display_target.is_empty()
            {
                let has_intrinsic_key_or_ref = provided_attrs
                    .iter()
                    .any(|(name, _)| name == "key" || name == "ref");
                use crate::query_boundaries::common::PropertyAccessResult;
                let props_has_children = matches!(
                    self.resolve_property_access_with_env(props_type, &children_prop_name),
                    PropertyAccessResult::Success { .. }
                );
                let intrinsic_has_children =
                    self.get_intrinsic_attributes_type().is_some_and(|ia_type| {
                        let resolved_ia = self.resolve_type_for_property_access(ia_type);
                        matches!(
                            self.resolve_property_access_with_env(resolved_ia, &children_prop_name),
                            PropertyAccessResult::Success { .. }
                        )
                    });
                if has_intrinsic_key_or_ref && !props_has_children && !intrinsic_has_children {
                    self.report_jsx_body_children_excess_property(
                        tag_name_idx,
                        &display_target,
                        &provided_attrs,
                    );
                    has_excess_property_error = true;
                }
            }
            // TS2745/TS2746: route JSX body children through one normalized
            // classifier so union/tuple shapes don't drift by component path.
            if child_count > 0 && !skip_prop_checks {
                self.check_jsx_children_shape(
                    props_type,
                    attributes_idx,
                    child_count,
                    has_text_child,
                    synthesized_type,
                    tag_name_idx,
                );
            }
            // TS2747: text children not accepted by component.
            if has_text_child && !skip_prop_checks {
                self.check_jsx_text_children_accepted(
                    props_type,
                    tag_name_idx,
                    &text_child_indices,
                );
            }
        }

        // For nonstandard ElementChildrenAttribute names, tsc reports the
        // missing required children property through whole-object
        // assignability (TS2322) rather than the generic TS2741 JSX fallback.
        let reported_custom_children_assignability = if !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !needs_special_attr_object_assignability
            && self.should_report_custom_jsx_children_via_assignability(props_type, &provided_attrs)
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &display_target,
                tag_name_idx,
            );
            true
        } else {
            false
        };

        let reported_special_attr_assignability = if !reported_custom_children_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && needs_special_attr_object_assignability
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            if !self.is_assignable_to(attrs_type, props_type) {
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &display_target,
                    tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        let reported_class_missing_props_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !display_target.is_empty()
            && component_type.is_some_and(|comp| {
                tsz_solver::type_queries::get_construct_signatures(self.ctx.types, comp)
                    .is_some_and(|sigs| !sigs.is_empty())
            })
            && self.jsx_has_missing_required_props(props_type, &provided_attrs)
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &display_target,
                tag_name_idx,
            );
            true
        } else {
            false
        };

        // TS2741: missing required properties.
        if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_class_missing_props_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
        {
            self.check_missing_required_jsx_props(props_type, &provided_attrs, tag_name_idx);
        }

        // Also check required IntrinsicAttributes.
        if !has_excess_property_error
            && !spread_covers_all
            && let Some(intrinsic_attrs_type) = self.get_intrinsic_attributes_type()
        {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &provided_attrs,
                tag_name_idx,
            );
        }

        if !has_excess_property_error
            && !spread_covers_all
            && let Some(comp) = component_type
            && let Some(intrinsic_class_attrs_type) =
                self.get_intrinsic_class_attributes_type_for_component(comp)
        {
            self.check_missing_required_jsx_props(
                intrinsic_class_attrs_type,
                &provided_attrs,
                tag_name_idx,
            );
        }
    }

    fn report_jsx_body_children_excess_property(
        &mut self,
        tag_name_idx: NodeIndex,
        display_target: &str,
        provided_attrs: &[(String, TypeId)],
    ) {
        let mut ordered_attrs: Vec<(String, TypeId)> = Vec::with_capacity(provided_attrs.len());
        if let Some((_, children_type)) = provided_attrs.iter().find(|(name, _)| name == "children")
        {
            ordered_attrs.push(("children".to_string(), *children_type));
        }
        ordered_attrs.extend(
            provided_attrs
                .iter()
                .filter(|(name, _)| name != "children")
                .cloned(),
        );

        let properties: Vec<tsz_solver::PropertyInfo> = ordered_attrs
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
        let source_type = self.format_type(self.ctx.types.factory().object(properties));
        let message = format!(
            "Type '{source_type}' is not assignable to type '{display_target}'.\n  Property 'children' does not exist on type '{display_target}'."
        );
        use crate::diagnostics::diagnostic_codes;
        self.error_at_node(
            tag_name_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    // ── JSX Overload Resolution ───────────────────────────────────────────

    /// JSX overload resolution for overloaded Stateless Function Components.
    ///
    /// When a component has multiple non-generic call signatures, tries each
    /// overload against the provided JSX attributes. If no overload matches,
    /// emits TS2769 ("No overload matches this call.").
    ///
    /// JSX overloads differ from regular function overloads: instead of positional
    /// arguments, the "call" is a single attributes object checked with excess
    /// property checking (like a fresh object literal).
    pub(crate) fn check_jsx_overloaded_sfc(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
        else {
            return;
        };

        // Collect non-generic call signatures
        let non_generic: Vec<_> = sigs.iter().filter(|s| s.type_params.is_empty()).collect();
        if non_generic.len() < 2 {
            return;
        }

        // Speculative attribute collection: save diagnostic checkpoint so side-effect
        // diagnostics (e.g. TS7006 from callback params without contextual typing) are
        // rolled back. Only the final TS2769 (if no overload matches) is kept.
        let snap = self.ctx.snapshot_diagnostics();

        // Collect JSX attributes: explicit + spread-merged, with override tracking
        let mut attrs_info = self.collect_jsx_provided_attrs(attributes_idx);

        // Include synthesized children from JSX element body
        let children_prop_name = self.get_jsx_children_prop_name();
        if let Some(children) = children_ctx {
            attrs_info.attrs.push(JsxAttrInfo {
                name: children_prop_name,
                type_id: children.synthesized_type,
                from_spread: false,
            });
        }

        // Try each overload
        let has_any_attrs = !attrs_info.attrs.is_empty() || attrs_info.has_spread;

        // When an `any`-typed spread exists, any non-0-param overload matches.
        // The `any` spread dominates the merged type, making it `any`.
        // Skip detailed attribute checking (it would false-positive on explicit attrs).
        if attrs_info.has_any_spread {
            let has_non_zero_param = non_generic.iter().any(|s| !s.params.is_empty());
            if has_non_zero_param {
                self.ctx.rollback_diagnostics(&snap);
                return;
            }
        }

        for sig in &non_generic {
            // For 0-param overloads: only match when NO attributes are provided.
            // tsc treats JSX as a 1-arg call (the attributes object), so 0-param
            // overloads fail on arg count when any attributes exist.
            if sig.params.is_empty() {
                if !has_any_attrs {
                    self.ctx.rollback_diagnostics(&snap);
                    return;
                }
                continue;
            }

            let props_type = sig.params[0].type_id;
            let evaluated = self.evaluate_type_with_env(props_type);
            let props_resolved = self.resolve_type_for_property_access(evaluated);

            if self.jsx_attrs_match_overload(&attrs_info, props_resolved) {
                // Found a matching overload — done.
                // Roll back speculative diagnostics from attribute collection.
                self.ctx.rollback_diagnostics(&snap);
                return;
            }
        }

        // No overload matched — roll back speculative diagnostics and emit TS2769.
        // tsc anchors JSX TS2769 at the tag name.
        self.ctx.rollback_diagnostics(&snap);
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            tag_name_idx,
            diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL,
            diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
        );
    }

    /// Collect provided JSX attributes as `JsxAttrsInfo`.
    ///
    /// Merges explicit and spread attributes. Later attributes override earlier
    /// ones with the same name (matching tsc's JSX override semantics).
    fn collect_jsx_provided_attrs(&mut self, attributes_idx: NodeIndex) -> JsxAttrsInfo {
        let empty = JsxAttrsInfo {
            attrs: Vec::new(),
            has_spread: false,
            has_any_spread: false,
        };
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return empty;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return empty;
        };

        // Use a map to handle overrides: later attrs with the same name replace earlier ones
        let mut attr_map: Vec<JsxAttrInfo> = Vec::new();
        let mut has_spread = false;
        let mut has_any_spread = false;

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };

                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                let attr_type = if attr_data.initializer.is_none() {
                    TypeId::BOOLEAN_TRUE
                } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                    let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                        self.ctx
                            .arena
                            .get_jsx_expression(init_node)
                            .map(|e| e.expression)
                            .unwrap_or(attr_data.initializer)
                    } else {
                        attr_data.initializer
                    };
                    self.compute_type_of_node(value_idx)
                } else {
                    TypeId::ANY
                };

                // Override any earlier attr with the same name
                if let Some(existing) = attr_map.iter_mut().find(|a| a.name == attr_name) {
                    existing.type_id = attr_type;
                    existing.from_spread = false;
                } else {
                    attr_map.push(JsxAttrInfo {
                        name: attr_name,
                        type_id: attr_type,
                        from_spread: false,
                    });
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                has_spread = true;
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_type = self.compute_type_of_node(spread_data.expression);
                if spread_type == TypeId::ANY {
                    has_any_spread = true;
                }
                let spread_evaluated = self.evaluate_type_with_env(spread_type);
                let resolved = self.resolve_type_for_property_access(spread_evaluated);
                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved)
                {
                    for prop in &shape.properties {
                        let name = self.ctx.types.resolve_atom(prop.name).to_string();
                        if name == "key" || name == "ref" {
                            continue;
                        }
                        // Override earlier attrs with the same name from this spread
                        if let Some(existing) = attr_map.iter_mut().find(|a| a.name == name) {
                            existing.type_id = prop.type_id;
                            existing.from_spread = true;
                        } else {
                            attr_map.push(JsxAttrInfo {
                                name,
                                type_id: prop.type_id,
                                from_spread: true,
                            });
                        }
                    }
                }
            }
        }

        JsxAttrsInfo {
            attrs: attr_map,
            has_spread,
            has_any_spread,
        }
    }

    /// Check whether JSX attributes match a specific overload's props type.
    ///
    /// Performs three checks (like tsc's `checkApplicableSignatureForJsxOpeningLikeElement`):
    /// 1. All required props in the overload must be provided
    /// 2. No excess properties from EXPLICIT attributes (spread props are exempt)
    /// 3. Provided attribute types must be assignable to expected prop types
    fn jsx_attrs_match_overload(&mut self, info: &JsxAttrsInfo, props_type: TypeId) -> bool {
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return true;
        }

        // When an `any`-typed spread exists, the merged attributes type is effectively
        // `any & {...explicitAttrs}` which simplifies to `any`. Since `any` is assignable
        // to any type, this overload automatically matches.
        if info.has_any_spread {
            return true;
        }

        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        else {
            // Can't resolve shape — use assignability fallback
            if info.attrs.is_empty() && !info.has_spread {
                return true;
            }
            let attrs_type = self.build_attrs_object_type_from_info(&info.attrs);
            return self.is_assignable_to(attrs_type, props_type);
        };

        let has_string_index = shape.string_index.is_some();
        let provided_names: rustc_hash::FxHashSet<&str> =
            info.attrs.iter().map(|a| a.name.as_str()).collect();

        // Check 1: All required props must be provided.
        // Children are now included in provided_names via synthesis above.
        if !info.has_any_spread {
            for prop in &shape.properties {
                if prop.optional {
                    continue;
                }
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if !provided_names.contains(prop_name.as_str()) {
                    return false;
                }
            }
        }

        // Check 2: Excess property check — only for EXPLICIT (non-spread) attributes.
        // tsc does not do excess property checking for spread-sourced attributes;
        // when all attrs come from spreads, no excess check occurs.
        // Hyphenated attribute names (e.g., `extra-prop`) are also exempt — in JSX,
        // they are only checked against string index signatures, not named properties.
        if !has_string_index {
            for attr in &info.attrs {
                if attr.from_spread {
                    continue; // Spreads are exempt from excess checking
                }
                if attr.name.contains('-') {
                    continue; // Hyphenated attrs exempt from excess checking
                }
                let attr_atom = self.ctx.types.intern_string(&attr.name);
                let exists = shape.properties.iter().any(|p| p.name == attr_atom);
                if !exists {
                    return false;
                }
            }
        }

        // Check 3: Type compatibility for all provided attributes
        for attr in &info.attrs {
            if attr.type_id == TypeId::ANY || attr.type_id == TypeId::ERROR {
                continue;
            }
            use crate::query_boundaries::common::PropertyAccessResult;
            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(props_type, &attr.name)
            {
                let expected = tsz_solver::remove_undefined(self.ctx.types, type_id);
                if !self.is_assignable_to(attr.type_id, expected) {
                    return false;
                }
            }
        }

        true
    }

    /// Build an object type from collected JSX attribute info.
    fn build_attrs_object_type_from_info(&mut self, attrs: &[JsxAttrInfo]) -> TypeId {
        let properties: Vec<tsz_solver::PropertyInfo> = attrs
            .iter()
            .map(|attr| {
                let name_atom = self.ctx.types.intern_string(&attr.name);
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: attr.type_id,
                    write_type: attr.type_id,
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
        self.ctx.types.factory().object(properties)
    }
    /// Check JSX attributes against union-typed props via whole-object assignability.
    ///
    /// When the component's props type is a union (e.g., discriminated unions like
    /// `{ editable: false } | { editable: true, onEdit: ... }`), we can't do per-property
    /// checking because `get_object_shape` doesn't work on unions. Instead, we build
    /// an object type from the provided JSX attributes and check the whole object
    /// against the union props type, letting the solver handle discriminated union logic.
    ///
    /// This matches tsc's behavior of constructing an "attributes type" object literal
    /// and checking assignability against the full props type.
    pub(crate) fn check_jsx_union_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // Collect provided attribute name→type pairs (excluding key/ref).
        // Skip when any attribute value is a function/arrow expression — these need
        // contextual typing from discriminated union narrowing which we don't implement.
        let attr_nodes = &attrs.properties.nodes;
        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let mut has_spread = false;

        for &attr_idx in attr_nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };

                // Skip key/ref — they come from IntrinsicAttributes, not component props
                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                // Check for function/arrow expressions — bail out for contextual typing
                if attr_data.initializer.is_some() {
                    let value_idx =
                        if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                            if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                self.ctx
                                    .arena
                                    .get_jsx_expression(init_node)
                                    .map(|e| e.expression)
                                    .unwrap_or(attr_data.initializer)
                            } else {
                                attr_data.initializer
                            }
                        } else {
                            attr_data.initializer
                        };
                    if let Some(value_node) = self.ctx.arena.get(value_idx)
                        && (value_node.kind == syntax_kind_ext::ARROW_FUNCTION
                            || value_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    {
                        return;
                    }
                }

                // Compute the attribute value type with literal preservation.
                // For union props, literals like "a" and true must stay as literal types
                // (not widen to string/boolean) so they can match discriminant properties
                // in the union members. Shorthand booleans stay as BOOLEAN_TRUE for
                // assignability but get widened to BOOLEAN in error message display.
                let attr_type = if attr_data.initializer.is_none() {
                    TypeId::BOOLEAN_TRUE
                } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                    let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                        self.ctx
                            .arena
                            .get_jsx_expression(init_node)
                            .map(|e| e.expression)
                            .unwrap_or(attr_data.initializer)
                    } else {
                        attr_data.initializer
                    };
                    let prev = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = true;
                    let t = self.compute_type_of_node(value_idx);
                    self.ctx.preserve_literal_types = prev;
                    t
                } else {
                    TypeId::ANY
                };

                provided_attrs.push((attr_name, attr_type));
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                has_spread = true;
            }
        }

        // Include synthesized children prop if body children exist
        if let Some(children) = children_ctx {
            provided_attrs.push((self.get_jsx_children_prop_name(), children.synthesized_type));
        }

        // Skip union check when there are no concrete attributes to check,
        // or when spread attributes are involved (handled separately).
        if provided_attrs.is_empty() || has_spread {
            return;
        }

        // Get union members — bail if not a union
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, props_type)
        else {
            return;
        };

        // For each union member, check:
        // 1. All provided attributes are type-compatible with the member's properties
        // 2. All required properties in the member are provided
        // If at least one member passes both checks, the attributes are valid.
        // Only emit TS2322 when NO member is compatible.
        let provided_names: rustc_hash::FxHashSet<&str> = provided_attrs
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        let mut any_member_compatible = false;
        for &member in &members {
            let member_resolved = self.resolve_type_for_property_access(member);

            // Check 1: All provided attribute values are assignable to member properties
            let all_attrs_compatible = provided_attrs.iter().all(|(name, attr_type)| {
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(member_resolved, name) {
                    PropertyAccessResult::Success { type_id, .. } => {
                        // Strip undefined from optional properties (write-position)
                        let expected = tsz_solver::remove_undefined(self.ctx.types, type_id);
                        // any/error types are always compatible
                        if *attr_type == TypeId::ANY || *attr_type == TypeId::ERROR {
                            return true;
                        }
                        self.is_assignable_to(*attr_type, expected)
                    }
                    // PropertyNotFound or other results: still compatible
                    // (excess property checking is separate)
                    _ => true,
                }
            });

            if !all_attrs_compatible {
                continue;
            }

            // Check 2: All required properties in the member are provided.
            // Children are now included in provided_names via synthesis above.
            let all_required_present = if let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, member_resolved)
            {
                shape.properties.iter().all(|prop| {
                    if prop.optional {
                        return true;
                    }
                    let prop_name = self.ctx.types.resolve_atom(prop.name);
                    provided_names.contains(prop_name.as_str())
                })
            } else {
                // Can't determine shape — assume compatible
                true
            };

            if all_required_present {
                any_member_compatible = true;
                break;
            }
        }

        if !any_member_compatible {
            // Build the attributes object type for the error message.
            // tsc widens shorthand boolean `true` to `boolean` in the JSX attribute
            // object type displayed in error messages (fresh object literal widening).
            let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
                .iter()
                .map(|(name, type_id)| {
                    let name_atom = self.ctx.types.intern_string(name);
                    // Widen BOOLEAN_TRUE → BOOLEAN for error display
                    let display_type = if *type_id == TypeId::BOOLEAN_TRUE {
                        TypeId::BOOLEAN
                    } else {
                        *type_id
                    };
                    tsz_solver::PropertyInfo {
                        name: name_atom,
                        type_id: display_type,
                        write_type: display_type,
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
            let attrs_type = self.ctx.types.factory().object(properties);
            // tsc anchors JSX union props errors at the tag name (e.g., <TextComponent>),
            // not the attributes container.
            self.check_assignable_or_report_at(attrs_type, props_type, tag_name_idx, tag_name_idx);
        }
    }
    /// TS2322: Check that spread attribute property types are compatible with props.
    ///
    /// tsc checks if the spread type is assignable to the expected props type and
    /// emits TS2322 with "Type '{`spread_type`}' is not assignable to type '{`props_type`}'"
    /// when a property type mismatch is found. Missing properties are handled
    /// separately by TS2741, not TS2322.
    ///
    /// Properties overridden by explicit attributes (either before or after the spread)
    /// are excluded from the check.
    ///
    /// tsc anchors these errors at the JSX opening tag (not the spread expression).
    pub(crate) fn check_spread_property_types(
        &mut self,
        spread_type: TypeId,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        overridden_names: &rustc_hash::FxHashSet<&str>,
        _display_target: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Safety guard: skip when types already contain checker error states.
        if tsz_solver::contains_error_type(self.ctx.types, spread_type) {
            return false;
        }

        let spread_has_type_params =
            tsz_solver::contains_type_parameters(self.ctx.types, spread_type);

        // For concrete spread types, whole-type assignability is the fast path and
        // also prevents false positives from imprecise per-property extraction.
        // For generic spreads, the relation can be too optimistic; keep them on the
        // normalized JSX spread path below so we can classify TS2322 vs TS2741 from
        // the apparent/object shape first.
        if !spread_has_type_params && self.is_assignable_to(spread_type, props_type) {
            return false;
        }

        // tsc does NOT emit TS2559 (weak type / no common properties) for JSX spread
        // attributes. Extra properties in spreads are silently ignored — only per-property
        // type mismatches (TS2322) are checked below. Skipping weak type detection here
        // matches tsc behavior.

        // Resolve the spread type to extract its properties
        let resolved_spread = self.evaluate_type_with_env(spread_type);
        let resolved_spread = self.resolve_type_for_property_access(resolved_spread);

        // tsc uses the component's props type name (e.g., "PoisonedProp") for TS2322
        // in spread attribute checking, NOT the full intersection with IntrinsicAttributes.
        let props_display = self.format_type(props_type);

        let Some(spread_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_spread)
        else {
            // If spread type has no object shape (e.g., type parameter), emit
            // whole-type TS2322: "Type 'U' is not assignable to type 'Attribs1'".
            let spread_name = self.format_type(spread_type);
            let message =
                format!("Type '{spread_name}' is not assignable to type '{props_display}'.");
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                tag_name_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            return true;
        };

        // tsc suppresses TS2322 for per-property type mismatches in spreads when
        // the spread also has missing required properties from the target. In that case,
        // TS2741 (missing required property) is emitted instead, and tsc doesn't pile on
        // with TS2322 for the type mismatches. Check if any required props are missing
        // from the spread + explicit attributes.
        if let Some(props_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        {
            let spread_prop_names: rustc_hash::FxHashSet<String> = spread_shape
                .properties
                .iter()
                .map(|p| self.ctx.types.resolve_atom(p.name))
                .collect();
            for req_prop in &props_shape.properties {
                if req_prop.optional {
                    continue;
                }
                let req_name = self.ctx.types.resolve_atom(req_prop.name).to_string();
                if req_name == "key" || req_name == "ref" {
                    continue;
                }
                if !spread_prop_names.contains(&req_name)
                    && !overridden_names.contains(req_name.as_str())
                {
                    if spread_has_type_params {
                        let spread_name = self.format_type(spread_type);
                        let message = format!(
                            "Type '{spread_name}' is not assignable to type '{props_display}'."
                        );
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            tag_name_idx,
                            &message,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                        return true;
                    }
                    // Missing required property → TS2741 will fire, suppress TS2322
                    return false;
                }
            }
        }

        // Check if the mismatch is a TYPE mismatch (not just missing properties).
        // tsc only emits TS2322 for spread type mismatches, not for missing properties
        // (those are handled by TS2741).
        let mut has_type_mismatch = false;
        for prop in &spread_shape.properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name).to_string();

            // Skip properties overridden by explicit attributes
            if overridden_names.contains(prop_name.as_str()) {
                continue;
            }

            // Skip key/ref — same as other JSX attribute handling
            if prop_name == "key" || prop_name == "ref" {
                continue;
            }

            // Look up the expected type for this property in the props type
            let expected_type = match self.resolve_property_access_with_env(props_type, &prop_name)
            {
                PropertyAccessResult::Success { type_id, .. } => {
                    tsz_solver::remove_undefined(self.ctx.types, type_id)
                }
                _ => continue,
            };

            // Check if the spread property type is assignable to the expected type
            if !self.is_assignable_to(prop.type_id, expected_type) {
                has_type_mismatch = true;
                break;
            }
        }

        // Emit a single TS2322 with whole-type message matching tsc's format:
        // "Type '{ x: number; }' is not assignable to type 'PoisonedProp'."
        // tsc uses the props type name, not the full IntrinsicAttributes intersection.
        if has_type_mismatch {
            let spread_name = self.format_type(spread_type);
            let message =
                format!("Type '{spread_name}' is not assignable to type '{props_display}'.");
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                tag_name_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        false
    }
    /// TS2783: Check if a later spread attribute will overwrite the current attribute.
    ///
    /// In JSX, `<Foo a={1} {...props}>` — if `props` has a required property `a`,
    /// the spread overwrites the explicit `a={1}`. TSC warns with TS2783:
    /// "'a' is specified more than once, so this usage will be overwritten."
    ///
    /// Only emitted under `strictNullChecks` (matching tsc behavior) and only for
    /// non-optional spread properties (optional properties may not overwrite).
    /// Returns `true` if the attribute is overwritten by a later spread (and
    /// optionally emits TS2783 when `strictNullChecks` is enabled).
    pub(crate) fn check_jsx_attr_overwritten_by_spread(
        &mut self,
        attr_name: &str,
        attr_name_idx: NodeIndex,
        attr_nodes: &[NodeIndex],
        current_idx: usize,
    ) -> bool {
        // Look at later siblings for spreads that contain this property
        for &later_idx in &attr_nodes[current_idx + 1..] {
            let Some(later_node) = self.ctx.arena.get(later_idx) else {
                continue;
            };
            if later_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(later_node) else {
                    continue;
                };
                let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                    spread_data.expression,
                    &crate::context::TypingRequest::NONE,
                );

                // Skip any/error/unknown — they might cover everything but we
                // can't tell which specific properties they contain.
                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    continue;
                }

                // Check if the spread type has a non-optional property with this name
                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    let attr_atom = self.ctx.types.intern_string(attr_name);
                    let has_required_prop = shape
                        .properties
                        .iter()
                        .any(|p| p.name == attr_atom && !p.optional);
                    if has_required_prop {
                        use tsz_common::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                            &[attr_name],
                        );
                        self.error_at_node(
                            attr_name_idx,
                            &message,
                            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                        );
                        // Attribute is overwritten regardless of SNC
                        return true;
                    }
                }
            }
        }
        false
    }
}
