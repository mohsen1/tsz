impl<'a> CheckerState<'a> {
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
            let mut with_props = sigs.iter().filter(|sig| !sig.params.is_empty());
            let sig = with_props.next()?;
            if with_props.next().is_some() {
                return None;
            }
            sig
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
