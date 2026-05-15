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
    pub(super) fn apply_jsx_library_managed_attributes(
        &mut self,
        component_type: TypeId,
        props_type: TypeId,
    ) -> TypeId {
        let Some(lma_sym_id) = self.get_jsx_namespace_export_symbol_id("LibraryManagedAttributes")
        else {
            return props_type;
        };

        let (body_type, type_params) = self.type_reference_symbol_type_with_params(lma_sym_id);
        if body_type == TypeId::ERROR || body_type == TypeId::ANY {
            return props_type;
        }

        let instantiated = if type_params.is_empty() {
            body_type
        } else {
            let args = [component_type, props_type];
            let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &type_params,
                &args,
            );
            crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                body_type,
                &substitution,
            )
        };
        let default_props_type =
            match self.resolve_property_access_with_env(component_type, "defaultProps") {
                crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id, ..
                } => Some(type_id),
                _ => None,
            };
        let has_managed_props_metadata = default_props_type.is_some()
            || matches!(
                self.resolve_property_access_with_env(component_type, "propTypes"),
                crate::query_boundaries::common::PropertyAccessResult::Success { .. }
            );
        if !has_managed_props_metadata
            && crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                component_type,
            )
        {
            let lma_ref = self.resolve_symbol_as_lazy_type(lma_sym_id);
            return self
                .ctx
                .types
                .factory()
                .application(lma_ref, vec![component_type, props_type]);
        }
        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, props_type) {
            return props_type;
        }
        if !has_managed_props_metadata
            && crate::computation::call_inference::should_preserve_contextual_application_shape(
                self.ctx.types,
                props_type,
            )
        {
            return props_type;
        }
        if !has_managed_props_metadata
            && !self.jsx_managed_attributes_preserve_original_props(props_type, instantiated)
        {
            return props_type;
        }
        if !has_managed_props_metadata
            && crate::computation::call_inference::should_preserve_contextual_application_shape(
                self.ctx.types,
                instantiated,
            )
        {
            instantiated
        } else {
            let evaluated = self.evaluate_type_with_env(instantiated);
            if evaluated == TypeId::ANY
                && let Some(default_props_type) = default_props_type
                && let Some(fallback) =
                    self.try_apply_jsx_default_props_fallback(props_type, default_props_type)
            {
                return fallback;
            }
            // LMA evaluation can produce an intersection whose members are still
            // unreduced applications when the user-defined helper alias inside
            // the conditional (e.g. React's distributive `Defaultize<P, D>` built
            // out of `Pick<P, Exclude<keyof P, keyof D>>` etc.) cannot collapse
            // its `Pick`/`Exclude`/`Extract`/`Partial` arms to concrete object
            // shapes. The intersection then fails `object_shape_for_type` even
            // though the conditional itself succeeded. In that structural case,
            // when the component still carries `defaultProps` metadata, apply
            // the same default-props transform used for the `evaluated == ANY`
            // branch — making the defaulted props optional matches what tsc
            // emits for these helper-alias shapes.
            if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
                .is_none()
                && let Some(default_props_type) = default_props_type
                && let Some(fallback) =
                    self.try_apply_jsx_default_props_fallback(props_type, default_props_type)
            {
                return fallback;
            }
            // If LMA evaluation produced error types (e.g. due to unresolved qualified
            // type references in complex conditional types like React's
            // LibraryManagedAttributes), fall back to the raw props type rather than
            // using a broken evaluation result that would cause false TS2322 diagnostics.
            // Use contains_error_type_in_args which checks Application.base as well,
            // matching the formatter's own error-detection logic.
            if crate::query_boundaries::common::contains_error_type_in_args(
                self.ctx.types,
                evaluated,
            ) {
                return props_type;
            }
            let evaluated_is_callable = self.jsx_type_contains_callable_surface(evaluated);
            let props_is_callable = self.jsx_type_contains_callable_surface(props_type);
            if evaluated_is_callable && !props_is_callable {
                return props_type;
            }
            if crate::computation::call_inference::should_preserve_contextual_application_shape(
                self.ctx.types,
                evaluated,
            )
                && !crate::computation::call_inference::should_preserve_contextual_application_shape(
                    self.ctx.types,
                    props_type,
                )
            {
                return props_type;
            }
            if !self.jsx_managed_attributes_preserve_original_props(props_type, evaluated) {
                return props_type;
            }
            evaluated
        }
    }

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
        let raw_component_type = component_type;
        if let Some(props_type) =
            self.react_component_alias_application_props_arg(raw_component_type)
        {
            let raw_has_type_params = crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                props_type,
            );
            let props_type =
                self.apply_jsx_library_managed_attributes(raw_component_type, props_type);
            return Some((props_type, raw_has_type_params));
        }

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

        // Bare type parameters use their callable/construct constraint for props
        // extraction, while the raw type parameter remains the component argument
        // to JSX.LibraryManagedAttributes<T, P>.
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, component_type) {
            let constraint = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                component_type,
            )?;
            return self.get_jsx_props_type_for_component_member_with_raw(
                raw_component_type,
                constraint,
                element_idx,
            );
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, component_type)
        {
            let mut candidates = Vec::new();
            let mut seen = rustc_hash::FxHashSet::default();
            let mut any_raw_has_type_params = false;
            for member in members {
                let Some((props_type, member_raw_has_type_params)) =
                    self.get_jsx_props_type_for_component_member(member, None)
                else {
                    if self.is_generic_jsx_component(member)
                        || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, member)
                        || crate::query_boundaries::common::needs_evaluation_for_merge(
                            self.ctx.types,
                            member,
                        )
                        || crate::query_boundaries::common::is_constructor_like_type(
                            self.ctx.types,
                            member,
                        )
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

        self.get_jsx_props_type_for_component_member(raw_component_type, element_idx)
    }

    pub(super) fn get_jsx_props_type_for_component_member(
        &mut self,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<(TypeId, bool)> {
        let raw_component_type = component_type;
        self.get_jsx_props_type_for_component_member_with_raw(
            raw_component_type,
            component_type,
            element_idx,
        )
    }

    fn get_jsx_props_type_for_component_member_with_raw(
        &mut self,
        raw_component_type: TypeId,
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

        if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            raw_component_type,
        ) && let Some(props) =
            self.get_jsx_type_parameter_callable_constraint_props_type(raw_component_type)
        {
            let props = self.apply_jsx_library_managed_attributes(raw_component_type, props);
            return Some((props, true));
        }

        // Try SFC first: get call signatures -> first parameter is props type
        if let Some((props, raw_has_tp)) = self.get_sfc_props_type(component_type) {
            let props = self.apply_jsx_library_managed_attributes(raw_component_type, props);
            let raw_has_tp = raw_has_tp
                || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, props);
            return Some((props, raw_has_tp));
        }

        // Try class component: get construct signatures -> instance type -> props
        if let Some(props) = self.get_class_component_props_type(component_type, element_idx) {
            let props = self.apply_jsx_library_managed_attributes(raw_component_type, props);
            // Flag raw_has_tp when the props still contain type parameters, or when
            // the component is generic AND its type params carry defaults.  Defaults
            // signal that the caller should run generic inference / default
            // instantiation (e.g. `MyComp<P = Prop>` → `<MyComp />` uses the
            // default).  When type params only have constraints (no defaults), the
            // props were already resolved via constraint substitution in
            // `get_class_component_props_type`, so re-deriving them is unnecessary
            // and can produce the wrong type for optional-parameter constructors.
            let raw_has_tp =
                crate::query_boundaries::common::contains_type_parameters(self.ctx.types, props)
                    || (self.is_generic_jsx_component(component_type)
                        && self.generic_jsx_component_has_defaults(component_type));
            return Some((props, raw_has_tp));
        }

        None
    }

    fn get_jsx_type_parameter_callable_constraint_props_type(
        &mut self,
        type_param: TypeId,
    ) -> Option<TypeId> {
        let constraint =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_param)?;
        let constraint = self.normalize_jsx_component_type_for_resolution(constraint);

        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, constraint)
            && !shape.is_constructor
        {
            return Some(
                shape
                    .params
                    .first()
                    .map(|p| p.type_id)
                    .unwrap_or_else(|| self.ctx.types.factory().object(vec![])),
            );
        }

        let sigs =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, constraint)?;
        let non_generic: Vec<_> = sigs
            .iter()
            .filter(|sig| sig.type_params.is_empty())
            .collect();
        if non_generic.len() != 1 {
            return None;
        }
        Some(
            non_generic[0]
                .params
                .first()
                .map(|p| p.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![])),
        )
    }

    /// Emit TS2604 if the component type has no call or construct signatures.
    pub(super) fn check_jsx_element_has_signatures(
        &mut self,
        component_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        let tag_text = self.get_jsx_tag_name_text(tag_name_idx);
        let is_this_tag = tag_text == "this";
        if is_this_tag {
            use crate::diagnostics::diagnostic_codes;

            if let Some((start, _)) = self.get_node_span(tag_name_idx)
                && self.ctx.diagnostics.iter().any(|diag| {
                    diag.code == diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT
                        && diag.start == start
                })
            {
                return;
            }
            self.error_at_node_msg(
                tag_name_idx,
                diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES,
                &[&tag_text],
            );
            return;
        }
        if tag_text
            .as_bytes()
            .first()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            return;
        }
        if component_type == TypeId::ANY
            || component_type == TypeId::ERROR
            || component_type == TypeId::UNKNOWN
            || component_type == TypeId::NEVER
        {
            return;
        }
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, component_type) {
            return;
        }
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
        // If props extraction succeeds, the type is already recognized as a
        // valid JSX component shape (callable/constructable in JSX context).
        // Keep `this` tags strict: `<this/>` should still report TS2604.
        // Pass `None` for element_idx so this probe doesn't emit its own
        // TS2607 — the call is purely a "did extraction succeed?" check.
        if !is_this_tag
            && self
                .get_jsx_props_type_for_component_member(component_type, None)
                .is_some()
        {
            return;
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, component_type)
        {
            let mut saw_component_union = false;
            let mut all_component_unions = true;
            for member_type in members {
                let member_type = if crate::query_boundaries::common::needs_evaluation_for_merge(
                    self.ctx.types,
                    member_type,
                ) {
                    self.evaluate_type_with_env(member_type)
                } else {
                    member_type
                };
                if self.is_jsx_string_tag_type(member_type) {
                    continue;
                }
                saw_component_union = true;
                if self
                    .get_jsx_props_type_for_component_member(member_type, None)
                    .is_none()
                {
                    all_component_unions = false;
                    break;
                }
            }
            if saw_component_union && all_component_unions {
                return;
            }
        }
        // Check if the type (or any union member) has call/construct signatures
        let (types_to_check, is_union) = if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, component_type)
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
            if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ty)
                || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
                || self.is_generic_jsx_component(ty)
            {
                if is_this_tag || crate::query_boundaries::common::is_this_type(self.ctx.types, ty)
                {
                    return false;
                }
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
            if !is_this_tag
                && crate::query_boundaries::common::needs_evaluation_for_merge(self.ctx.types, ty)
            {
                return true;
            }
            let direct_has_signatures =
                crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, ty)
                    .is_some_and(|sigs| !sigs.is_empty())
                    || crate::query_boundaries::common::construct_signatures_for_type(
                        self.ctx.types,
                        ty,
                    )
                    .is_some_and(|sigs| !sigs.is_empty())
                    || crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
                        .is_some()
                    || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)
                        .is_some();
            if direct_has_signatures {
                return true;
            }

            let evaluated = self.evaluate_type_with_env(ty);
            if evaluated == ty {
                return false;
            }
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, evaluated)
                .is_some_and(|sigs| !sigs.is_empty())
                || crate::query_boundaries::common::construct_signatures_for_type(
                    self.ctx.types,
                    evaluated,
                )
                .is_some_and(|sigs| !sigs.is_empty())
                || crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    evaluated,
                )
                .is_some()
                || crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    evaluated,
                )
                .is_some()
        });

        if !has_signatures {
            // TSC uses the JSX tag text, not the resolved type.
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                tag_name_idx,
                diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES,
                &[&tag_text],
            );
        }
    }

    pub(super) fn is_jsx_string_tag_type(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, type_id)
            && let Some(constraint) =
                crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id)
            && constraint != type_id
        {
            return self.is_jsx_string_tag_type(constraint);
        }

        if crate::query_boundaries::common::is_string_type(self.ctx.types, type_id)
            || crate::query_boundaries::checkers::iterable::is_string_literal_type(
                self.ctx.types,
                type_id,
            )
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .all(|&member| self.is_jsx_string_tag_type(member));
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&member| self.is_jsx_string_tag_type(member));
        }

        false
    }

    fn jsx_component_return_check_types(
        &mut self,
        component_type: TypeId,
    ) -> Vec<(TypeId, TypeId)> {
        let mut stack = vec![(component_type, component_type)];
        let mut seen = rustc_hash::FxHashSet::default();
        let mut types = Vec::new();

        while let Some((raw_type_id, type_id)) = stack.pop() {
            let resolved = if crate::query_boundaries::common::needs_evaluation_for_merge(
                self.ctx.types,
                type_id,
            ) {
                self.evaluate_type_with_env(type_id)
            } else {
                type_id
            };
            if !seen.insert((raw_type_id, resolved)) {
                continue;
            }
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, resolved)
            {
                stack.extend(members.into_iter().map(|member| (member, member)));
            } else {
                types.push((raw_type_id, resolved));
            }
        }

        types
    }

    fn jsx_instantiated_application_body_for_return_check(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, type_id)
            .or_else(|| {
                self.ctx.types.get_display_alias(type_id).and_then(|alias| {
                    crate::query_boundaries::common::type_application(self.ctx.types, alias)
                })
            })?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
        let (body_type, type_params) = {
            let env = self.ctx.type_env.borrow();
            (
                tsz_solver::TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types),
                tsz_solver::TypeResolver::get_lazy_type_params(&*env, def_id).unwrap_or_default(),
            )
        };
        let body_type = body_type?;
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &type_params,
            &app.args,
        );
        Some(crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            body_type,
            &substitution,
        ))
    }

    fn jsx_property_type_for_return_check(
        &mut self,
        type_id: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        let mut stack = vec![type_id];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(candidate) = stack.pop() {
            if !seen.insert(candidate) {
                continue;
            }
            match self.resolve_property_access_with_env(candidate, property_name) {
                PropertyAccessResult::Success { type_id, .. } => return Some(type_id),
                _ => {
                    if let Some(alias) = self.ctx.types.get_display_alias(candidate) {
                        stack.push(alias);
                    }
                    if let Some(instantiated) =
                        self.jsx_instantiated_application_body_for_return_check(candidate)
                    {
                        stack.push(instantiated);
                    }
                    let evaluated = self.evaluate_type_with_env(candidate);
                    if evaluated != candidate {
                        stack.push(evaluated);
                    }
                    let lazy_resolved = self.resolve_lazy_type(candidate);
                    if lazy_resolved != candidate {
                        stack.push(lazy_resolved);
                    }
                    let property_resolved = self.resolve_type_for_property_access(candidate);
                    if property_resolved != candidate {
                        stack.push(property_resolved);
                    }
                }
            }
        }

        None
    }

    fn jsx_callable_return_types_for_return_check(&mut self, type_id: TypeId) -> Vec<TypeId> {
        let type_id =
            if crate::query_boundaries::common::needs_evaluation_for_merge(self.ctx.types, type_id)
            {
                self.evaluate_type_with_env(type_id)
            } else {
                type_id
            };
        let mut returns = Vec::new();
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && !shape.is_constructor
        {
            returns.push(self.evaluate_type_with_env(shape.return_type));
        }
        if let Some(sigs) =
            crate::query_boundaries::common::get_call_signatures(self.ctx.types, type_id)
        {
            returns.extend(
                sigs.iter()
                    .map(|sig| self.evaluate_type_with_env(sig.return_type)),
            );
        }
        returns
    }

    pub(super) fn jsx_construct_return_satisfies_element_class_render(
        &mut self,
        instance_type: TypeId,
        element_class_type: TypeId,
    ) -> bool {
        let Some(source_render) = self.jsx_property_type_for_return_check(instance_type, "render")
        else {
            return false;
        };
        let Some(target_render) =
            self.jsx_property_type_for_return_check(element_class_type, "render")
        else {
            return false;
        };
        if self.is_assignable_to(source_render, target_render) {
            return true;
        }

        let source_returns = self.jsx_callable_return_types_for_return_check(source_render);
        let target_returns = self.jsx_callable_return_types_for_return_check(target_render);
        if source_returns.is_empty() || target_returns.is_empty() {
            return false;
        }

        source_returns.iter().any(|&source_return| {
            target_returns
                .iter()
                .any(|&target_return| self.is_assignable_to(source_return, target_return))
        })
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
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, component_type) {
            return;
        }
        if self.ctx.has_parse_errors {
            return;
        }

        // When the user defines `JSX.ElementType`, that type — not `JSX.Element`
        // — is the authoritative constraint for what can appear as a JSX
        // component. tsc validates `component_type` directly against
        // `JSX.ElementType` and skips the older return-type-vs-`JSX.Element`
        // check entirely. This unblocks React 18 / Server Component patterns
        // where a component may return string / number / array / Promise as
        // long as `JSX.ElementType` admits them.
        if let Some(element_type_sym_id) = self.get_jsx_namespace_export_symbol_id("ElementType") {
            let element_type = self.type_reference_symbol_type(element_type_sym_id);
            if !matches!(
                element_type,
                TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
            ) {
                if self.is_assignable_to(component_type, element_type) {
                    return;
                }
                self.report_invalid_jsx_component_return_type(tag_name_idx);
                return;
            }
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

        let is_union =
            crate::query_boundaries::common::union_members(self.ctx.types, component_type)
                .is_some();
        let types_to_check = self.jsx_component_return_check_types(component_type);

        let mut any_checked = false;
        let mut all_valid = true;
        let is_react_component_alias_union =
            self.is_react_jsx_component_alias_union(component_type);

        for (raw_member_type, member_type) in types_to_check {
            if self.is_jsx_string_tag_type(member_type) {
                continue;
            }
            if !is_concrete(member_type) {
                continue;
            }
            // Skip unresolved Application/Lazy member types (e.g. ComponentClass<any>)
            if crate::query_boundaries::common::needs_evaluation_for_merge(
                self.ctx.types,
                member_type,
            ) {
                continue;
            }
            // In a union, React component alias Applications (ComponentType<P>,
            // ReactType<P>, ComponentClass<P>, StatelessComponent<P>, etc.) are
            // valid JSX component shapes. Skip the return-type check: their
            // recursive return types (ReactElement<P> ↔ ComponentClass<P>/SFC<P>)
            // trigger cycle-detection false positives in the assignability checker.
            // The alias-application skip does not require props to be extractable
            // because the skip reason is cycle avoidance, not props availability.
            // The second clause (branch display) still requires props as an
            // extra guard that the member is a concrete component shape.
            let is_alias_app = self.is_react_jsx_component_alias_application(raw_member_type);
            let is_branch_disp =
                !is_alias_app && self.is_react_jsx_component_branch_display(raw_member_type);
            let branch_has_props = is_branch_disp
                && self
                    .get_jsx_props_type_for_component_member(member_type, None)
                    .is_some();
            if is_union && (is_alias_app || (is_react_component_alias_union && branch_has_props)) {
                continue;
            }
            let is_unresolved = |t: TypeId| -> bool {
                !is_concrete(t)
                    || crate::query_boundaries::common::needs_evaluation_for_merge(
                        self.ctx.types,
                        t,
                    )
            };
            let is_valid_null_like_return = |t: TypeId| -> bool { t == TypeId::NULL };

            let mut is_sfc = false;
            if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                member_type,
            ) && !shape.is_constructor
            {
                is_sfc = true;
                let return_type = self.evaluate_type_with_env(shape.return_type);
                if !is_unresolved(return_type) && !is_valid_null_like_return(return_type) {
                    any_checked = true;
                    if let Some(element_type) = jsx_element_type {
                        // Strip null/undefined before checking against JSX.Element.
                        // `() => Element | null` is valid; `() => undefined` is not.
                        let non_null_return = crate::query_boundaries::common::remove_nullish(
                            self.ctx.types,
                            return_type,
                        );
                        if non_null_return == TypeId::NEVER {
                            // Stripping nullish left NEVER. Two cases:
                            // 1. return_type IS never (unreachable bottom type) → valid.
                            // 2. return_type was nullish-only (e.g. `undefined`,
                            //    `null | undefined`) → the pure-null case was already
                            //    handled by `is_valid_null_like_return` above, so
                            //    `undefined` is present. With strictNullChecks,
                            //    `undefined` is not valid JSX (not in JSX.Element | null).
                            //    Without strictNullChecks, undefined is a subtype of
                            //    everything, so it is valid JSX → no error.
                            if return_type != TypeId::NEVER && self.ctx.strict_null_checks() {
                                all_valid = false;
                            }
                        } else if !self.is_assignable_to(non_null_return, element_type) {
                            all_valid = false;
                        }
                    }
                }
            }

            // Check call/construct signatures against JSX.Element/ElementClass.
            for (get_sigs_fn, target, is_call_sig) in [
                (
                    crate::query_boundaries::common::get_call_signatures as fn(_, _) -> _,
                    jsx_element_type,
                    true,
                ),
                (
                    crate::query_boundaries::common::get_construct_signatures,
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
                        if !is_call_sig
                            && crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                member_type,
                            )
                        {
                            return true;
                        }
                        let ret = self.evaluate_type_with_env(sig.return_type);
                        if is_unresolved(ret) || is_valid_null_like_return(ret) {
                            return true;
                        }
                        // For construct sigs, skip unresolved outer-scope type parameters.
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
                                let stripped = crate::query_boundaries::common::remove_nullish(
                                    self.ctx.types,
                                    ret,
                                );
                                if stripped == TypeId::NEVER {
                                    // Same logic as the function-shape path above:
                                    // never itself is valid; nullish-only (undefined
                                    // or null|undefined) is invalid with strictNullChecks,
                                    // but valid without (undefined is a universal subtype).
                                    if ret == TypeId::NEVER {
                                        return true;
                                    }
                                    return !self.ctx.strict_null_checks();
                                }
                                stripped
                            } else {
                                ret
                            };
                            self.is_assignable_to(check_ret, t)
                                || (!is_call_sig
                                    && self
                                        .jsx_construct_return_can_use_render_fallback(check_ret, t))
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
            self.report_invalid_jsx_component_return_type(tag_name_idx);
        }
    }

    pub(super) fn check_jsx_sfc_return_type(
        &mut self,
        return_type: TypeId,
        tag_name_idx: NodeIndex,
    ) {
        if return_type == TypeId::ANY
            || return_type == TypeId::ERROR
            || return_type == TypeId::UNKNOWN
            || return_type == TypeId::NEVER
        {
            return;
        }
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, return_type) {
            return;
        }
        if self.ctx.has_parse_errors {
            return;
        }

        let Some(jsx_element_type_raw) = self.get_jsx_element_type_for_check() else {
            return;
        };
        let jsx_element_type = self.evaluate_type_with_env(jsx_element_type_raw);
        if matches!(
            jsx_element_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return;
        }

        let evaluated_return = self.evaluate_type_with_env(return_type);
        if matches!(
            evaluated_return,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) || crate::query_boundaries::common::needs_evaluation_for_merge(
            self.ctx.types,
            evaluated_return,
        ) || crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            evaluated_return,
        ) {
            return;
        }

        let non_null_return =
            crate::query_boundaries::common::remove_nullish(self.ctx.types, evaluated_return);
        if non_null_return == TypeId::NEVER {
            return;
        }

        if !self.is_assignable_to(non_null_return, jsx_element_type) {
            self.report_invalid_jsx_component_return_type(tag_name_idx);
        }
    }

    fn report_invalid_jsx_component_return_type(&mut self, tag_name_idx: NodeIndex) {
        let tag_text = self.get_jsx_tag_name_text(tag_name_idx);
        use crate::diagnostics::diagnostic_codes;
        self.error_at_node_msg(
            tag_name_idx,
            diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT,
            &[&tag_text],
        );
    }

    /// Extract props type from a Stateless Function Component (first param of call sig).
    fn get_sfc_props_type(&mut self, component_type: TypeId) -> Option<(TypeId, bool)> {
        use crate::computation::call_inference::should_preserve_contextual_application_shape;

        // Check Function type (single signature)
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, component_type)
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
            let raw_has_type_params =
                crate::query_boundaries::common::contains_type_parameters(self.ctx.types, props);
            // When the raw props type is already a union (e.g., discriminated unions like
            // `{ variant: Avatar } | { variant: OneLine }`), skip full evaluation.
            // The type evaluator may incorrectly merge union members with the same
            // property names into a single object, losing the discriminated union
            // structure needed for correct assignability checking.
            let evaluated = if crate::query_boundaries::common::is_union_type(self.ctx.types, props)
                || should_preserve_contextual_application_shape(self.ctx.types, props)
            {
                props
            } else {
                self.evaluate_type_with_env(props)
            };
            let managed = self.apply_jsx_library_managed_attributes(component_type, evaluated);
            let managed_raw_has_type_params = raw_has_type_params
                || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    managed,
                );
            return Some((managed, managed_raw_has_type_params));
        }

        // Check Callable type (overloaded signatures)
        if let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) && !sigs.is_empty()
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
            let raw_has_type_params =
                crate::query_boundaries::common::contains_type_parameters(self.ctx.types, props);
            let evaluated = if should_preserve_contextual_application_shape(self.ctx.types, props) {
                props
            } else {
                self.evaluate_type_with_env(props)
            };
            let managed = self.apply_jsx_library_managed_attributes(component_type, evaluated);
            let managed_raw_has_type_params = raw_has_type_params
                || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    managed,
                );
            return Some((managed, managed_raw_has_type_params));
        }

        None
    }

    fn instantiate_jsx_generic_sfc_props_with_defaults(
        &mut self,
        component_type: TypeId,
        props_type: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> Option<TypeId> {
        use crate::computation::call_inference::should_preserve_contextual_application_shape;

        if type_params.is_empty() {
            return None;
        }

        let type_args: Vec<_> = type_params
            .iter()
            .map(|param| {
                param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN)
            })
            .collect();
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            type_params,
            &type_args,
        );
        let instantiated = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            props_type,
            &substitution,
        );
        let evaluated =
            if crate::query_boundaries::common::is_union_type(self.ctx.types, instantiated)
                || should_preserve_contextual_application_shape(self.ctx.types, instantiated)
            {
                instantiated
            } else {
                self.evaluate_type_with_env(instantiated)
            };
        let managed = self.apply_jsx_library_managed_attributes(component_type, evaluated);
        if managed == TypeId::ANY
            || managed == TypeId::UNKNOWN
            || managed == TypeId::ERROR
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, managed)
        {
            return None;
        }

        Some(managed)
    }

    pub(super) fn get_default_instantiated_generic_sfc_props_type(
        &mut self,
        component_type: TypeId,
    ) -> Option<TypeId> {
        let component_type = self.normalize_jsx_component_type_for_resolution(component_type);

        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, component_type)
            && !shape.is_constructor
            && !shape.type_params.is_empty()
        {
            let props = shape
                .params
                .first()
                .map(|param| param.type_id)
                .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
            return self.instantiate_jsx_generic_sfc_props_with_defaults(
                component_type,
                props,
                &shape.type_params,
            );
        }

        if let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) {
            let generic: Vec<_> = sigs
                .iter()
                .filter(|sig| !sig.type_params.is_empty())
                .collect();
            if generic.len() == 1 {
                let sig = generic[0];
                let props = sig
                    .params
                    .first()
                    .map(|param| param.type_id)
                    .unwrap_or_else(|| self.ctx.types.factory().object(vec![]));
                return self.instantiate_jsx_generic_sfc_props_with_defaults(
                    component_type,
                    props,
                    &sig.type_params,
                );
            }
        }

        None
    }

    /// Check if a component type is an overloaded SFC (>= 2 non-generic call signatures).
    pub(super) fn is_overloaded_sfc(&self, component_type: TypeId) -> bool {
        let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) else {
            return false;
        };
        let non_generic_count = sigs.iter().filter(|s| s.type_params.is_empty()).count();
        non_generic_count >= 2
    }

    /// Check if a component type has multiple call signatures (including generic ones)
    /// that should go through overload resolution. This is used as a fallback when
    /// `recover_jsx_component_props_type` returns `None` -- the component has
    /// overloaded generic signatures that couldn't be resolved to a single props type.
    pub(super) fn has_multi_signature_overloads(&self, component_type: TypeId) -> bool {
        let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) else {
            return false;
        };
        sigs.len() >= 2
    }

    /// Check if a component type has multiple construct signatures (including generic ones)
    /// that should go through overload resolution. This handles class components like
    /// `React.Component` which typically have 2 construct overloads.
    pub(super) fn has_multi_construct_overloads(&self, component_type: TypeId) -> bool {
        let Some(sigs) = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        ) else {
            return false;
        };
        sigs.len() >= 2
    }

    /// Check if a component type has generic call or construct signatures.
    pub(super) fn is_generic_jsx_component(&self, component_type: TypeId) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, component_type)
            && !shape.is_constructor
            && !shape.type_params.is_empty()
        {
            return true;
        }
        if let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) && sigs.iter().any(|s| !s.type_params.is_empty())
        {
            return true;
        }
        if let Some(sigs) = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        ) && sigs.iter().any(|s| !s.type_params.is_empty())
        {
            return true;
        }
        false
    }

    /// Check if a generic JSX component's type params carry default types.
    fn generic_jsx_component_has_defaults(&self, component_type: TypeId) -> bool {
        if let Some(sigs) = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        ) && sigs
            .iter()
            .any(|s| s.type_params.iter().any(|tp| tp.default.is_some()))
        {
            return true;
        }
        if let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) && sigs
            .iter()
            .any(|s| s.type_params.iter().any(|tp| tp.default.is_some()))
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
        let sigs = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        )?;
        if sigs.is_empty() {
            return None;
        }

        // Prefer the single constructor signature that carries props for JSX checks.
        // React-like class surfaces may expose a synthetic no-arg constructor
        // alongside a real props-taking constructor; we still want the latter
        // so `<MyComp a="x" />` produces type errors instead of falling into
        // overload mismatch fallback.
        let first_sig = if sigs.len() == 1 {
            sigs.first()?
        } else {
            let with_props: Vec<_> = sigs.iter().filter(|sig| !sig.params.is_empty()).collect();
            match with_props.len() {
                1 => with_props[0],
                _ => return None,
            }
        };

        let inferred_sig = Some(first_sig.clone())
            .and_then(|sig| {
                if sig.type_params.is_empty() {
                    None
                } else {
                    element_idx.and_then(|idx| {
                        self.infer_jsx_generic_class_component_signature(idx, component_type)
                    })
                }
            })
            // When inference didn't resolve all type params (e.g. `<MyComp />`
            // with no attributes to infer from), treat as inference failure and
            // fall through to the default constraint-based substitution path.
            .filter(|sig| sig.type_params.is_empty());

        let raw_instance_type = if let Some(sig) = inferred_sig.as_ref() {
            sig.return_type
        } else if first_sig.type_params.is_empty() {
            first_sig.return_type
        } else {
            let type_args: Vec<_> = first_sig
                .type_params
                .iter()
                .map(|param| {
                    param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN)
                })
                .collect();
            let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &first_sig.type_params,
                &type_args,
            );
            crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                first_sig.return_type,
                &substitution,
            )
        };

        let first_param_type = inferred_sig
            .as_ref()
            .and_then(|sig| sig.params.first().map(|param| param.type_id))
            .or_else(|| {
                if first_sig.type_params.is_empty() {
                    first_sig.params.first().map(|param| param.type_id)
                } else {
                    let type_args: Vec<_> = first_sig
                        .type_params
                        .iter()
                        .map(|param| {
                            param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN)
                        })
                        .collect();
                    let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                        self.ctx.types,
                        &first_sig.type_params,
                        &type_args,
                    );
                    first_sig.params.first().map(|param| {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            param.type_id,
                            &substitution,
                        )
                    })
                }
            });
        if raw_instance_type == TypeId::ANY || raw_instance_type == TypeId::ERROR {
            return None;
        }

        // Evaluate Application/Lazy instance types to their structural form.
        // e.g. `Component<{reqd: any}, any>` is an Application that evaluates
        // to a concrete object. Keep partially generic instances: JSX attribute
        // checking can still read `props` or fall back to the constructor
        // parameter, and later checks already guard the places where unresolved
        // type parameters would create false diagnostics.
        let instance_type = if crate::query_boundaries::common::needs_evaluation_for_merge(
            self.ctx.types,
            raw_instance_type,
        ) {
            let evaluated = self.evaluate_type_with_env(raw_instance_type);
            // If evaluation still contains type parameters from an outer generic
            // context, keep the raw application so member lookup can preserve the
            // generic props surface (for example React.Component<P>["props"]).
            if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, evaluated)
            {
                raw_instance_type
            } else {
                evaluated
            }
        } else {
            raw_instance_type
        };
        let props_alias_hint = self
            .jsx_class_component_props_alias_hint(raw_instance_type)
            .or_else(|| self.jsx_class_component_props_alias_hint(instance_type))
            .or_else(|| {
                first_param_type.filter(|&param_type| {
                    crate::query_boundaries::common::type_has_displayable_name(
                        self.ctx.types,
                        param_type,
                    )
                })
            });

        // Look up ElementAttributesProperty to know which instance property is props
        // Pass element_idx so TS2608 can be emitted if >1 property
        let prop_name = self.get_element_attributes_property_name_with_check(element_idx);

        match prop_name {
            None => {
                // When there is no JSX namespace at all (e.g., `@jsx: preserve`
                // without any JSX factory or React import), tsc does not perform
                // attribute type checking for class-based JSX elements. Only fall
                // back to the `props` property when a JSX namespace exists but
                // doesn't define `ElementAttributesProperty`.
                self.get_jsx_namespace_type()?;

                // In React-style JSX setups, class components frequently expose
                // their props through an inherited instance `props` member even
                // when ElementAttributesProperty is absent. Fall back to that
                // surface before giving up on attribute checking.
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                use crate::query_boundaries::common::PropertyAccessResult;
                let props_result =
                    match self.resolve_property_access_with_env(raw_instance_type, "props") {
                        success @ PropertyAccessResult::Success { .. } => success,
                        _ => self.resolve_property_access_with_env(evaluated_instance, "props"),
                    };
                match props_result {
                    PropertyAccessResult::Success { type_id, .. } => {
                        let props_type =
                            self.strip_implicit_jsx_children_from_props_fallback(type_id);
                        if let Some(alias) = props_alias_hint {
                            self.store_jsx_props_display_alias_if_matching(props_type, alias);
                        }
                        Some(props_type)
                    }
                    _ => first_param_type
                        .and_then(|param_type| {
                            let raw_param_type = param_type;
                            let param_type = self.evaluate_type_with_env(raw_param_type);
                            if param_type != raw_param_type
                                && param_type != TypeId::ERROR
                                && self.ctx.types.get_display_alias(param_type).is_none()
                            {
                                self.ctx
                                    .types
                                    .store_display_alias(param_type, raw_param_type);
                            }
                            // When no ElementAttributesProperty is defined, tsc uses the
                            // first constructor parameter as the props type even when it is
                            // a primitive (e.g. `new(n: string): …`). The synthesized attrs
                            // object is then checked against that primitive → TS2322.
                            (param_type != TypeId::ANY && param_type != TypeId::ERROR)
                                .then_some(param_type)
                        })
                        .or_else(|| {
                            let has_managed_props_metadata = matches!(
                                self.resolve_property_access_with_env(
                                    component_type,
                                    "defaultProps"
                                ),
                                PropertyAccessResult::Success { .. }
                            ) || matches!(
                                self.resolve_property_access_with_env(component_type, "propTypes"),
                                PropertyAccessResult::Success { .. }
                            );
                            has_managed_props_metadata
                                .then(|| self.ctx.types.factory().object(vec![]))
                        }),
                }
            }
            Some(ref name) if name.is_empty() => {
                // Empty ElementAttributesProperty -> use the construct signature's
                // return (instance) type as the attributes type. This matches tsc:
                // `forcedLookupLocation === ""` returns `getReturnTypeOfSignature(sig)`.
                Some(self.evaluate_type_with_env(instance_type))
            }
            Some(ref name) => {
                // ElementAttributesProperty has a member -> access that property on instance
                let evaluated_instance = self.evaluate_type_with_env(instance_type);
                use crate::query_boundaries::common::PropertyAccessResult;
                let props_result =
                    match self.resolve_property_access_with_env(raw_instance_type, name) {
                        success @ PropertyAccessResult::Success { .. } => success,
                        _ => self.resolve_property_access_with_env(evaluated_instance, name),
                    };
                match props_result {
                    PropertyAccessResult::Success { type_id, .. } => {
                        if let Some(alias) = props_alias_hint {
                            self.store_jsx_props_display_alias_if_matching(type_id, alias);
                        }
                        Some(type_id)
                    }
                    // Instance type doesn't have the ElementAttributesProperty member.
                    // This can happen when class inheritance doesn't include inherited
                    // members in the construct signature return type.
                    // Fall back to the first construct parameter as props type (the
                    // common React pattern: `new(props: P)`). If no suitable fallback,
                    // emit TS2607.
                    _ => {
                        // Try first construct param as fallback (React-style: new(props: P))
                        if let Some(first_param_type) = first_param_type {
                            let raw_param_type = first_param_type;
                            let param_type = self.evaluate_type_with_env(raw_param_type);
                            if param_type != raw_param_type
                                && param_type != TypeId::ERROR
                                && self.ctx.types.get_display_alias(param_type).is_none()
                            {
                                self.ctx
                                    .types
                                    .store_display_alias(param_type, raw_param_type);
                            }
                            if param_type != TypeId::ANY
                                && param_type != TypeId::ERROR
                                && param_type != TypeId::STRING
                                && param_type != TypeId::NUMBER
                            {
                                return Some(param_type);
                            }
                        }
                        // The class doesn't expose the configured ElementAttributesProperty
                        // member (e.g., `props`) on its instance and there's no usable
                        // first-construct-parameter fallback. tsc emits TS2607 in this
                        // case regardless of whether the class lacks construct params
                        // entirely (inherited from `any`) or has unusable ones.
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
}
