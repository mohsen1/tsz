use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Facts extracted from a class component's construct signature in one walk:
/// the instance `.props` field type and the first constructor parameter type.
/// Used by the JSX target-display takeover to decide whether to render the
/// `.props` wrapper or the LMA-projected constructor parameter.
#[derive(Default, Clone, Copy)]
pub(in crate::checkers_domain::jsx) struct JsxClassComponentConstructFacts {
    pub(in crate::checkers_domain::jsx) props_field: Option<TypeId>,
    pub(in crate::checkers_domain::jsx) first_param: Option<TypeId>,
}

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn get_class_component_props_from_construct_return(
        &mut self,
        component_type: TypeId,
    ) -> Option<TypeId> {
        self.jsx_class_component_construct_return_facts(component_type)
            .props_field
    }

    /// Walk the construct signatures of `component_type` and return both the
    /// instance `.props` field type and the first constructor parameter type
    /// from the same chosen signature. Folding them into one walk avoids
    /// walking construct signatures twice per JSX element on the hot path.
    pub(in crate::checkers_domain::jsx) fn jsx_class_component_construct_return_facts(
        &mut self,
        component_type: TypeId,
    ) -> JsxClassComponentConstructFacts {
        use crate::query_boundaries::common::PropertyAccessResult;

        let mut facts = JsxClassComponentConstructFacts::default();

        let evaluated_component_type = self.evaluate_type_with_env(component_type);
        let Some(sigs) =
            crate::query_boundaries::checkers::jsx::construct_signatures_with_env_fallback(
                self.ctx.types,
                component_type,
                evaluated_component_type,
            )
        else {
            return facts;
        };

        for sig in sigs.iter().filter(|sig| !sig.params.is_empty()) {
            let instance_type = sig.return_type;
            let evaluated_instance = self.evaluate_type_with_env(instance_type);
            let props_access = match self.resolve_property_access_with_env(instance_type, "props") {
                success @ PropertyAccessResult::Success { .. } => success,
                _ => self.resolve_property_access_with_env(evaluated_instance, "props"),
            };
            if let PropertyAccessResult::Success { type_id, .. } = props_access
                && !matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
            {
                facts.props_field = Some(type_id);
                facts.first_param = sig.params.first().map(|p| p.type_id);
                return facts;
            }
        }

        facts
    }

    /// Thin orchestrator over the JSX-boundary helper. The structural
    /// `Readonly`-wrapper detection lives in `query_boundaries::checkers::jsx`
    /// (`class_props_is_readonly_wrapper_intersection`) so the
    /// checker-side call site keeps a single direct dependency on the
    /// boundary surface rather than three on `query_boundaries::common`
    /// (#8225 quarantine-budget rule).
    pub(in crate::checkers_domain::jsx) fn jsx_class_props_is_readonly_wrapper(
        &self,
        class_props: TypeId,
    ) -> bool {
        crate::query_boundaries::checkers::jsx::class_props_is_readonly_wrapper_intersection(
            self.ctx.types,
            &self.ctx.definition_store,
            class_props,
        )
    }

    pub(in crate::checkers_domain::jsx) fn jsx_class_props_has_readonly_mapped_surface(
        &self,
        class_props: TypeId,
    ) -> bool {
        self.jsx_class_props_is_readonly_wrapper(class_props)
            || crate::query_boundaries::checkers::jsx::contains_mapped_type_with_readonly_modifier(
                self.ctx.types,
                class_props,
            )
    }
}

impl<'a> CheckerState<'a> {
    pub(super) fn strip_implicit_jsx_children_from_props_fallback(
        &mut self,
        props_type: TypeId,
    ) -> TypeId {
        let props_type = self.normalize_jsx_required_props_target(props_type);
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type)
        {
            let filtered_props: Vec<_> = shape
                .properties
                .iter()
                .filter(|prop| self.ctx.types.resolve_atom(prop.name) != "children")
                .cloned()
                .collect();
            if filtered_props.len() != shape.properties.len() {
                return self.ctx.types.factory().object(filtered_props);
            }
        }

        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, props_type)
        else {
            return props_type;
        };

        let filtered: Vec<_> = members
            .into_iter()
            .filter(|member| {
                let Some(shape) =
                    crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *member)
                else {
                    return true;
                };
                if shape.properties.len() != 1 {
                    return true;
                }
                let prop = &shape.properties[0];
                self.ctx.types.resolve_atom(prop.name) != "children"
            })
            .collect();

        match filtered.len() {
            0 => props_type,
            1 => filtered[0],
            _ => self.ctx.types.factory().intersection(filtered),
        }
    }

    pub(super) fn jsx_managed_attributes_preserve_original_props(
        &mut self,
        original_props: TypeId,
        managed_props: TypeId,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let original_props = self.normalize_jsx_required_props_target(original_props);
        let managed_props = self.normalize_jsx_required_props_target(managed_props);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, original_props)
        else {
            return true;
        };

        shape.properties.iter().all(|prop| {
            let prop_name = self.ctx.types.resolve_atom(prop.name);
            matches!(
                self.resolve_property_access_with_env(managed_props, &prop_name),
                PropertyAccessResult::Success { .. }
            )
        })
    }

    pub(super) fn try_apply_jsx_default_props_fallback(
        &mut self,
        props_type: TypeId,
        default_props_type: TypeId,
    ) -> Option<TypeId> {
        let props_type = self.normalize_jsx_required_props_target(props_type);
        let props_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type)?;
        if props_shape.string_index.is_some() || props_shape.number_index.is_some() {
            return None;
        }

        let default_props_type = self.evaluate_type_with_env(default_props_type);
        let default_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            default_props_type,
        )?;
        if default_shape.properties.is_empty() {
            return Some(props_type);
        }

        let defaulted_names: rustc_hash::FxHashSet<_> = default_shape
            .properties
            .iter()
            .map(|prop| prop.name)
            .collect();
        let mut changed = false;
        let properties: Vec<_> = props_shape
            .properties
            .iter()
            .cloned()
            .map(|mut prop| {
                if defaulted_names.contains(&prop.name) && !prop.optional {
                    prop.optional = true;
                    changed = true;
                }
                prop
            })
            .collect();

        if !changed {
            return Some(props_type);
        }

        Some(self.ctx.types.factory().object(properties))
    }

    /// Get the property name from `JSX.ElementAttributesProperty`.
    /// Returns None/Some("")/Some("name"); emits TS2608 if >1 property.
    pub(super) fn get_element_attributes_property_name_with_check(
        &mut self,
        _element_idx: Option<NodeIndex>,
    ) -> Option<String> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let eap_sym_id = exports.get("ElementAttributesProperty")?;

        let eap_type = self.type_reference_symbol_type(eap_sym_id);
        let evaluated = self.evaluate_type_with_env(eap_type);

        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR {
            return Some("props".to_string());
        }

        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
        {
            if shape.properties.is_empty() {
                return Some(String::new());
            }
            if shape.properties.len() > 1 {
                if let Some(eap_symbol) = self.ctx.binder.get_symbol(eap_sym_id)
                    && let Some(&decl_idx) = eap_symbol.declarations.first()
                {
                    let anchor_idx = self
                        .ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .map(|iface| iface.name)
                        .unwrap_or(decl_idx);
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        anchor_idx,
                        diagnostic_codes::THE_GLOBAL_TYPE_JSX_MAY_NOT_HAVE_MORE_THAN_ONE_PROPERTY,
                        &["ElementAttributesProperty"],
                    );
                }
                return None;
            }
            if let Some(first_prop) = shape.properties.first() {
                return Some(self.ctx.types.resolve_atom(first_prop.name));
            }
        }

        Some(String::new())
    }

    pub(super) fn get_jsx_class_props_with_explicit_type_args(
        &mut self,
        component_type: TypeId,
        explicit_type_args: &[TypeId],
    ) -> Option<TypeId> {
        let sigs = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        )?;
        if sigs.is_empty() {
            return None;
        }

        let sig = if sigs.len() == 1 {
            sigs.into_iter().next()?
        } else {
            let with_props: Vec<_> = sigs.into_iter().filter(|s| !s.params.is_empty()).collect();
            if with_props.len() == 1 {
                with_props.into_iter().next()?
            } else {
                return None;
            }
        };

        if sig.type_params.is_empty() {
            return sig
                .params
                .first()
                .map(|p| self.evaluate_type_with_env(p.type_id));
        }

        if explicit_type_args.len() > sig.type_params.len() {
            return None;
        }

        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &sig.type_params,
            explicit_type_args,
        );

        let instantiated_return = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            sig.return_type,
            &substitution,
        );
        let instance_type = self.evaluate_type_with_env(instantiated_return);

        let prop_name = self.get_element_attributes_property_name_with_check(None);

        let props_type = match prop_name {
            None => {
                self.get_jsx_namespace_type()?;
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(instance_type, "props") {
                    PropertyAccessResult::Success { type_id, .. } => type_id,
                    _ => {
                        let instantiated_param = sig.params.first().map(|p| {
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                p.type_id,
                                &substitution,
                            )
                        })?;
                        self.evaluate_type_with_env(instantiated_param)
                    }
                }
            }
            Some(ref name) if name.is_empty() => {
                let instantiated_param = sig.params.first().map(|p| {
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        p.type_id,
                        &substitution,
                    )
                })?;
                self.evaluate_type_with_env(instantiated_param)
            }
            Some(ref name) => {
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(instance_type, name) {
                    PropertyAccessResult::Success { type_id, .. } => type_id,
                    _ => return None,
                }
            }
        };

        let evaluated = self.evaluate_type_with_env(props_type);
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR || evaluated == TypeId::UNKNOWN {
            return None;
        }
        Some(evaluated)
    }
}
