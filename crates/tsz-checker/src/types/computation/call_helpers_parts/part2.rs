impl<'a> CheckerState<'a> {
    /// Enhance a partial Round 1 object type by including sensitive lambda properties
    /// whose contextual parameter types from the generic function shape are concrete
    /// (i.e., they don't depend on the type parameters being inferred).
    ///
    /// This enables "intra-expression inference" for patterns like:
    /// ```ts
    /// declare function callIt<T>(obj: { produce: (n: number) => T, consume: (x: T) => void }): void;
    /// callIt({ produce: _a => 0, consume: n => n.toFixed() });
    /// ```
    /// Here `produce`'s param type `(n: number)` doesn't depend on `T`, so we can
    /// safely type `_a` as `number` and use the return type `0` to infer `T = number`.
    pub(crate) fn extract_inference_contributing_object_type(
        &mut self,
        arg_idx: NodeIndex,
        target_param_type: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<TypeId> {
        use super::complex::is_contextually_sensitive;

        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        // Evaluate the target parameter type when possible, but keep the raw target
        // around so contextual property lookup can still pierce unresolved generic
        // intersections/applications like `{ as?: C } & Elements[C]`.
        let target_type = self.evaluate_type_with_env(target_param_type);
        let target_shape = common::object_shape_for_type(self.ctx.types, target_type);
        let target_props: rustc_hash::FxHashMap<tsz_common::Atom, TypeId> = target_shape
            .map(|shape| {
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect()
            })
            .unwrap_or_default();

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let Some(name) = self.get_property_name(prop.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                if !is_contextually_sensitive(self, prop.initializer) {
                    // Non-sensitive: compute type without context (already handled by
                    // extract_non_sensitive_object_type, but include here for completeness
                    // of the partial type).
                    let value_type =
                        self.get_type_of_node_with_request(prop.initializer, &TypingRequest::NONE);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                    continue;
                }

                // Sensitive property: check if the contextual function type's params are concrete
                let target_prop_type = target_props.get(&name_atom).copied().or_else(|| {
                    self.contextual_object_literal_property_type(target_param_type, &name)
                });
                let Some(target_prop_type) = target_prop_type else {
                    continue;
                };

                // If the target property type is a bare type parameter being inferred,
                // compute the property type without context. The property's type
                // directly constrains the type parameter.
                // Example: make({ mutations: { foo() {} }, action: (m) => m.foo() })
                // where mutations has target type M (a type param).
                if self.type_contains_any_type_param(target_prop_type, type_param_names)
                    && common::type_param_info(self.ctx.types, target_prop_type).is_some()
                {
                    let value_type =
                        self.speculative_type_of_node(prop.initializer, &TypingRequest::NONE);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                    continue;
                }

                // If the target property is an object type, try to recursively extract
                // inference-contributing properties from nested object literals.
                // Example: nested({ prop: { produce: (a) => [a], consume: (arg) => arg.join(",") } })
                // where prop has target type { produce: (arg1: number) => T, consume: (arg2: T) => void }
                if common::object_shape_for_type(self.ctx.types, target_prop_type).is_some()
                    && let Some(nested_partial) = self.extract_inference_contributing_object_type(
                        prop.initializer,
                        target_prop_type,
                        type_param_names,
                    )
                {
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, nested_partial));
                    continue;
                }

                // If the target property type is a mapped type like `{ [K in keyof T]: V[K] }`,
                // recursively extract inference from the property value's object literal by
                // instantiating the mapped type template for each nested key.
                // Example: VuexStoreOptions pattern where `modules` has type
                // `{ [k in keyof Modules]: VuexStoreOptions<Modules[k], never> }` and the
                // initializer is `{ foo: { state() {...}, mutations: {...} } }`.
                if let Some(mapped_id) = crate::query_boundaries::common::mapped_type_id(
                    self.ctx.types,
                    target_prop_type,
                ) && let Some(nested_partial) = self.extract_inference_from_mapped_type_target(
                    prop.initializer,
                    mapped_id,
                    type_param_names,
                ) {
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, nested_partial));
                    continue;
                }

                let Some((contextual_fn_type, _target_params, target_return_type)) = self
                    .inference_callable_context_for_property_target(
                        target_prop_type,
                        type_param_names,
                    )
                else {
                    continue;
                };

                // When the return type contains unresolved type parameters AND the
                // function body has context-sensitive return expressions (e.g., nested
                // arrow functions with unannotated params in block-body returns),
                // skip speculative evaluation. The speculative pass would assign the
                // unresolved type parameter to inner function params, and while
                // diagnostics are rolled back, the resulting cached type pollutes the
                // inference. The full contextual type (with substituted type params)
                // will be applied in Round 2.
                if self.type_contains_any_type_param(target_return_type, type_param_names)
                    && super::contextual::expression_needs_contextual_return_type(
                        self,
                        prop.initializer,
                    )
                {
                    // Conditional-return callbacks can still contribute their
                    // concrete branch return without unresolved contextual T.
                    let conditional_branch_can_seed = self
                        .callback_first_conditional_branch(prop.initializer)
                        .is_some_and(|branch_idx| !is_contextually_sensitive(self, branch_idx));
                    let zero_param_can_seed = self
                        .unannotated_zero_param_callback_return_expression(prop.initializer)
                        .is_some_and(|return_idx| !is_contextually_sensitive(self, return_idx));
                    if conditional_branch_can_seed || zero_param_can_seed {
                        let value_type =
                            self.speculative_type_of_node(prop.initializer, &TypingRequest::NONE);
                        if !self.type_contains_any_type_param(value_type, type_param_names) {
                            properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                        }
                    }
                    continue;
                }

                // The contextual param types are concrete, so we can safely type this
                // lambda with those contextual types and extract its return type.
                // Use the target function type as contextual type for the lambda.
                // Suppress diagnostics from this speculative evaluation
                // (the params WILL get contextual types in the final pass).
                let value_type = self.speculative_type_of_node(
                    prop.initializer,
                    &TypingRequest::with_contextual_type(contextual_fn_type),
                );

                // If the speculative result still contains any of the type parameters
                // being inferred, skip it. Including such types in the partial can
                // poison Round 1 inference by creating self-referential constraints
                // (e.g., T appearing in both source and target positions).
                // This happens when a callback's return type references T
                // through the un-instantiated contextual return type.
                if self.type_contains_any_type_param(value_type, type_param_names) {
                    continue;
                }

                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Shorthand properties are never contextually sensitive
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let value_type = self.get_type_of_node(shorthand.name);
                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Method declarations: check similarly to lambda properties
            else if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
            {
                let Some(name) = self.property_name_for_error(method.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                let target_prop_type = target_props.get(&name_atom).copied().or_else(|| {
                    self.contextual_object_literal_property_type(target_param_type, &name)
                });
                let Some(target_prop_type) = target_prop_type else {
                    continue;
                };

                let Some((contextual_fn_type, _target_params, _target_return_type)) = self
                    .inference_callable_context_for_property_target(
                        target_prop_type,
                        type_param_names,
                    )
                else {
                    continue;
                };

                let value_type = self.speculative_type_of_function(
                    elem_idx,
                    &TypingRequest::with_contextual_type(contextual_fn_type),
                );

                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object_fresh(properties))
    }

    fn inference_callable_context_for_property_target(
        &self,
        target_prop_type: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<(TypeId, Vec<tsz_solver::ParamInfo>, TypeId)> {
        let mut candidates = Vec::new();
        if let Some(members) = common::union_members(self.ctx.types, target_prop_type) {
            candidates.extend(members);
        } else if let Some(members) = common::intersection_members(self.ctx.types, target_prop_type)
        {
            candidates.extend(members);
        } else {
            candidates.push(target_prop_type);
        }

        for candidate in candidates {
            if let Some(target_fn) = common::function_shape_for_type(self.ctx.types, candidate)
                && target_fn.params.iter().all(|param| {
                    !self.type_contains_any_type_param(param.type_id, type_param_names)
                })
            {
                return Some((candidate, target_fn.params.clone(), target_fn.return_type));
            }

            if let Some(signatures) = common::call_signatures_for_type(self.ctx.types, candidate) {
                for sig in signatures {
                    if sig.params.iter().all(|param| {
                        !self.type_contains_any_type_param(param.type_id, type_param_names)
                    }) {
                        return Some((candidate, sig.params, sig.return_type));
                    }
                }
            }
        }

        None
    }

    /// Extract inference from an object literal whose target type is a mapped type.
    ///
    /// For patterns like `VuexStoreOptions` where `modules` has type
    /// `{ [k in keyof Modules]: VuexStoreOptions<Modules[k], never> }` and the initializer is
    /// `{ foo: { state() {...}, mutations: {...} } }`, we need to:
    /// 1. For each property key (e.g., `foo`), extract the partial type from the property value
    /// 2. Build a partial object type from the results
    ///
    /// This enables inference from nested "thisless" functions like `state()` even when the
    /// overall object contains context-sensitive parts.
    fn extract_inference_from_mapped_type_target(
        &mut self,
        arg_idx: NodeIndex,
        mapped_id: tsz_solver::MappedTypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        let mapped = self.ctx.types.get_mapped(mapped_id);
        let template = mapped.template;
        let type_param_name = mapped.type_param.name;

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Handle property assignments
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let Some(name) = self.get_property_name(prop.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                // Create a substitution mapping the mapped type's key param to this literal key
                let key_literal = self.ctx.types.literal_string(&name);
                let subst = common::TypeSubstitution::single(type_param_name, key_literal);

                // Instantiate the template with this key
                let instantiated_template =
                    common::instantiate_type(self.ctx.types, template, &subst);

                // Try to recursively extract inference from the property value
                if let Some(nested_partial) = self
                    .extract_inference_contributing_object_type(
                        prop.initializer,
                        instantiated_template,
                        type_param_names,
                    )
                    .or_else(|| {
                        // Fallback: if we couldn't extract against the template (likely because
                        // it contains unresolved type params), try to extract non-sensitive
                        // parts directly from the nested object literal. This handles patterns
                        // like VuexStoreOptions where nested modules have "thisless" state()
                        // methods whose return types should contribute to inference.
                        self.extract_non_sensitive_object_type(prop.initializer)
                    })
                {
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, nested_partial));
                }
            }
            // Handle method declarations - for mapped types, these typically aren't at this level
            // but handle them for completeness
            else if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
            {
                // Check if this method is "thisless" (no params, no this)
                let has_params = !method.parameters.nodes.is_empty();
                if has_params {
                    continue;
                }

                let Some(name) = self.property_name_for_error(method.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                // For thisless methods, compute the return type directly
                let value_type = self.speculative_type_of_function(elem_idx, &TypingRequest::NONE);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object_fresh(properties))
    }

    /// Like `extract_inference_contributing_object_type` but for array/tuple literals.
    ///
    /// Handles patterns like:
    /// ```ts
    /// declare function callItT<T>(obj: [(n: number) => T, (x: T) => void]): void;
    /// callItT([_a => 0, n => n.toFixed()]);
    /// ```
    /// The first element `_a => 0` has concrete contextual param type `(n: number)`,
    /// so we can type it in Round 1 and use its return type to infer T.
    pub(crate) fn extract_inference_contributing_array_type(
        &mut self,
        arg_idx: NodeIndex,
        target_param_type: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<TypeId> {
        use super::complex::is_contextually_sensitive;

        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let arr = self.ctx.arena.get_literal_expr(node)?;

        // Get the target tuple type
        let target_type = self.evaluate_type_with_env(target_param_type);
        let target_elements = common::tuple_elements(self.ctx.types, target_type)?;

        let mut elements = Vec::new();
        let mut any_contributed = false;

        for (idx, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            let target_elem_type = target_elements
                .get(idx)
                .map(|e| e.type_id)
                .unwrap_or(TypeId::ANY);

            if !is_contextually_sensitive(self, elem_idx) {
                // Non-sensitive: compute type without context
                let value_type = self.get_type_of_node_with_request(elem_idx, &TypingRequest::NONE);
                elements.push(tsz_solver::TupleElement {
                    type_id: value_type,
                    optional: false,
                    rest: false,
                    name: None,
                });
                any_contributed = true;
                continue;
            }

            // Sensitive element: check if contextual function params are concrete
            let target_fn = common::function_shape_for_type(self.ctx.types, target_elem_type)?;

            let params_are_concrete = target_fn
                .params
                .iter()
                .all(|param| !self.type_contains_any_type_param(param.type_id, type_param_names));

            if params_are_concrete {
                let value_type = self.speculative_type_of_node(
                    elem_idx,
                    &TypingRequest::with_contextual_type(target_elem_type),
                );
                elements.push(tsz_solver::TupleElement {
                    type_id: value_type,
                    optional: false,
                    rest: false,
                    name: None,
                });
                any_contributed = true;
            } else {
                // Can't contribute — use ANY as placeholder
                elements.push(tsz_solver::TupleElement {
                    type_id: TypeId::ANY,
                    optional: false,
                    rest: false,
                    name: None,
                });
            }
        }

        if !any_contributed {
            return None;
        }

        Some(self.ctx.types.factory().tuple(elements))
    }

    /// Check if a type contains any of the specified type parameter names.
    fn type_contains_any_type_param(
        &self,
        type_id: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> bool {
        type_param_names
            .iter()
            .any(|&name| common::contains_type_parameter_named(self.ctx.types, type_id, name))
    }

    /// Check if a type is an intersection containing an Application of a conditional
    /// type alias (like Extract, Exclude, `NonNullable`). These types arise from type
    /// predicate narrowing and should not be treated as constructor types.
    pub(crate) fn is_intersection_with_conditional_application(&self, type_id: TypeId) -> bool {
        let Some(members) = common::intersection_members(self.ctx.types, type_id) else {
            return false;
        };

        members.iter().any(|&member| {
            let Some(app_id) = common::application_id(self.ctx.types, member) else {
                return false;
            };
            let app = self.ctx.types.type_application(app_id);
            let Some(def_id) = common::lazy_def_id(self.ctx.types, app.base) else {
                return false;
            };

            self.ctx
                .def_to_symbol_id(def_id)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS))
        })
    }
}
