//! JSX component props extraction and component validation.
//!
//! Extracts props types from JSX components (SFC first-param, class construct-sig
//! return type via `ElementAttributesProperty`), validates component signatures
//! (TS2604), return types (TS2786), and provides helpers for generic/overloaded
//! component detection.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // JSX Component Props Extraction

    /// Extract props type from a JSX component (SFC: first param of call sig;
    /// class: construct sig return -> `JSX.ElementAttributesProperty` member).
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
                        // String-like union members (e.g., `string` in `React.ReactType`)
                        // are valid intrinsic element references — skip them rather than
                        // aborting props extraction for the entire union.
                        || self.is_jsx_string_tag_type(member)
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

        // Try SFC first: get call signatures -> first parameter is props type
        if let Some((props, raw_has_tp)) = self.get_sfc_props_type(component_type) {
            return Some((props, raw_has_tp));
        }

        // Try class component: get construct signatures -> instance type -> props
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
        let (types_to_check, is_union) = if let Some(members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, component_type)
        {
            (members, true)
        } else {
            (vec![component_type], false)
        };

        let has_signatures = types_to_check.iter().any(|&ty| {
            // Types containing unresolved type parameters may resolve to callable
            // types once instantiated — treat them as potentially having signatures
            // to avoid false TS2604.  This mirrors the skip logic in
            // `get_jsx_props_type_for_component` for union members.
            if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, ty)
                || tsz_solver::contains_type_parameters(self.ctx.types, ty)
                || self.is_generic_jsx_component(ty)
            {
                return true;
            }
            // In unions like `React.ReactType` (`string | ComponentClass | SFC`),
            // string-like members are valid intrinsic element references and don't
            // need call/construct signatures.  Only apply this for union members,
            // not standalone string literal tags (which go through intrinsic lookup).
            if is_union && self.is_jsx_string_tag_type(ty) {
                return true;
            }
            // Application/Lazy types (e.g., `ComponentClass<any>`) may evaluate to
            // Callable types with call/construct signatures.  Treat them as potentially
            // having signatures to avoid false TS2604.  The actual signature checking
            // happens during props extraction where these types are fully evaluated.
            if tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, ty) {
                return true;
            }
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
                            && crate::query_boundaries::common::contains_type_parameters(
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
                // G2: No ElementAttributesProperty -> no JSX infrastructure.
                // TSC skips attribute checking when JSX types aren't configured.
                None
            }
            Some(ref name) if name.is_empty() => {
                // Empty ElementAttributesProperty -> instance type IS the props
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                Some(evaluated_instance)
            }
            Some(ref name) => {
                // ElementAttributesProperty has a member -> access that property on instance
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
}
