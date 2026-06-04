impl<'a> CheckerState<'a> {
    /// Narrow a union contextual type by inspecting discriminant properties in the
    /// object literal.  When the object literal has properties with literal values
    /// (e.g. `kind: "a"`) that match only a subset of the union members, we narrow
    /// the contextual type so that other properties receive precise contextual types
    /// from the matching member(s) rather than a union of all members' property types.
    ///
    /// This is how tsc provides precise contextual typing for discriminated union
    /// object literals:
    /// ```ts
    /// type A = { kind: "a"; onClick: (e: string) => void };
    /// type B = { kind: "b"; onClick: (e: number) => void };
    /// const x: A | B = { kind: "a", onClick: (e) => e.length }; // e: string
    /// ```
    pub(crate) fn narrow_contextual_union_via_object_literal_discriminants(
        &mut self,
        ctx_type: TypeId,
        elements: &[NodeIndex],
    ) -> TypeId {
        // Get union members; bail if not a union.
        let resolved = self.resolve_type_for_property_access(ctx_type);
        let Some(members) = common::union_members(self.ctx.types, resolved) else {
            return ctx_type;
        };
        let raw_members = common::union_members(self.ctx.types, ctx_type);

        if members.len() < 2 {
            return ctx_type;
        }

        // Pre-scan: collect discriminant info from the object literal.
        // - `unit_discriminants`: properties with unit-type literal values (e.g. `kind: "a"`)
        // - `present_property_names`: all explicitly named properties (for never-elimination)
        // - `non_unit_named_properties`: present properties whose initializer is NOT a
        //   unit literal (e.g. `type: foo1` where `foo1: string`). When such a property
        //   names a discriminator slot in the union, narrowing must bail entirely so
        //   the diagnostic reports the full union (`"foo" | "bar"`) rather than a
        //   single arm — matches tsc's `indirectDiscriminantAndExcessProperty` shape.
        let mut unit_discriminants: Vec<(String, TypeId)> = Vec::new();
        let mut present_property_names: Vec<String> = Vec::new();
        let mut non_unit_named_properties: Vec<String> = Vec::new();
        for &elem_idx in elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    continue;
                };
                present_property_names.push(name.clone());
                // Get the literal type of the initializer without full type computation.
                let unit_lit = self
                    .literal_type_from_initializer(prop.initializer)
                    .or_else(|| {
                        let initializer_type = self.get_type_of_node(prop.initializer);
                        common::is_unit_type(self.ctx.types, initializer_type)
                            .then_some(initializer_type)
                    })
                    .filter(|&lit_type| common::is_unit_type(self.ctx.types, lit_type));
                if let Some(lit_type) = unit_lit {
                    unit_discriminants.push((name, lit_type));
                } else {
                    non_unit_named_properties.push(name);
                }
            } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name) = self.get_property_name_resolved(shorthand.name)
            {
                present_property_names.push(name.clone());
                // For shorthand properties like `{ kind }` where `const kind = "a"`,
                // resolve the identifier to its const declaration and extract the literal
                // type from the initializer. This enables discriminant narrowing for
                // shorthand properties, matching tsc behavior.
                let unit_lit = self
                    .shorthand_const_literal_type(shorthand.name)
                    .or_else(|| self.literal_type_from_initializer(shorthand.name))
                    .filter(|&lit_type| common::is_unit_type(self.ctx.types, lit_type));
                if let Some(lit_type) = unit_lit {
                    unit_discriminants.push((name, lit_type));
                } else {
                    non_unit_named_properties.push(name);
                }
            }
        }

        if unit_discriminants.is_empty() && present_property_names.is_empty() {
            return ctx_type;
        }

        let mut is_discriminator_slot = |prop_name: &str| -> bool {
            let mut unit_member_count = 0;
            for &member in &members {
                let lazy_member = self.resolve_lazy_type(member);
                let resolved_member = self.resolve_type_for_property_access(lazy_member);
                let evaluated_member = self.evaluate_contextual_type(resolved_member);
                let member_candidates = [evaluated_member, resolved_member, lazy_member, member];
                let member_prop_type = member_candidates.iter().find_map(|&candidate| {
                    self.ctx
                        .types
                        .contextual_property_type(candidate, prop_name)
                });
                let Some(member_prop_type) = member_prop_type else {
                    continue;
                };
                if !common::is_unit_type(self.ctx.types, member_prop_type) {
                    return false;
                }
                unit_member_count += 1;
            }
            unit_member_count >= 2
        };

        unit_discriminants.retain(|(prop_name, _)| is_discriminator_slot(prop_name));

        // If the literal supplies a discriminator slot with a non-unit value
        // (e.g. `type: foo1` where `foo1: string`), the user is attempting a
        // dynamic discriminator. tsc reports the assignability error against
        // the FULL union (`"foo" | "bar"`); narrowing here would collapse the
        // diagnostic to a single arm. Bail entirely in that case.
        let literal_has_dynamic_discriminator = non_unit_named_properties
            .iter()
            .any(|name| is_discriminator_slot(name));
        if literal_has_dynamic_discriminator {
            return ctx_type;
        }

        // For each union member, check if all discriminant values are compatible
        // AND no present property maps to `never` in that member.
        let mut matching_members: Vec<TypeId> = Vec::new();
        for (member_index, &member) in members.iter().enumerate() {
            let lazy_member = self.resolve_lazy_type(member);
            let resolved_member = self.resolve_type_for_property_access(lazy_member);
            let evaluated_member = self.evaluate_contextual_type(resolved_member);
            let member_candidates = [evaluated_member, resolved_member, lazy_member, member];

            // Check unit-type discriminants: literal must be subtype of member's prop type.
            let unit_match = unit_discriminants.iter().all(|(prop_name, lit_type)| {
                let member_prop_type = member_candidates.iter().find_map(|&candidate| {
                    self.ctx
                        .types
                        .contextual_property_type(candidate, prop_name)
                });
                match member_prop_type {
                    Some(target_type) => {
                        if *lit_type == target_type
                            || self
                                .diagnostic_subtype_outcome(*lit_type, target_type)
                                .related
                        {
                            return true;
                        }
                        // For optional properties (e.g. `disc?: false`), the effective type
                        // includes `undefined`. contextual_property_type returns the raw
                        // declared type without `undefined`, so we must check optionality
                        // explicitly. If the property is optional and the literal is
                        // `undefined`, it matches (undefined is always valid for optional
                        // properties).
                        if *lit_type == TypeId::UNDEFINED {
                            let prop_name_atom = self.ctx.types.intern_string(prop_name);
                            let is_optional = member_candidates.iter().any(|&candidate| {
                                common::find_property_in_object(
                                    self.ctx.types,
                                    candidate,
                                    prop_name_atom,
                                )
                                .is_some_and(|p| p.optional)
                            });
                            if is_optional {
                                return true;
                            }
                        }
                        false
                    }
                    // If the member doesn't have this property, it could still match
                    // (the property might be optional or absent).
                    None => true,
                }
            });

            // Check present properties: eliminate members where a present property
            // has type `never` (the member requires the property to be absent).
            // Note: `prop?: never` resolves to `undefined` via contextual typing,
            // so we check the raw property type from the object shape instead.
            let never_match = present_property_names.iter().all(|prop_name| {
                let prop_name_atom = self.ctx.types.intern_string(prop_name);
                // Look up the raw property type from the member's object shape.
                let raw_prop_type = member_candidates.iter().find_map(|&candidate| {
                    common::raw_property_type(self.ctx.types, candidate, prop_name_atom)
                });
                match raw_prop_type {
                    Some(type_id) => type_id != TypeId::NEVER,
                    // Property not in object shape; don't eliminate.
                    None => true,
                }
            });

            // Check absent required discriminants: if the member has a required
            // (non-optional) property that is NOT present in the object literal,
            // AND at least one other member either doesn't have that property or
            // has it as optional, then this member can be eliminated.
            // This handles cases like:
            //   type A = { disc: true; cb: (x: string) => void }
            //   type B = { disc?: false; cb: (x: number) => void }
            //   f({ cb: n => ... })  // disc is required in A but optional in B
            //
            // Run this check even when there are no unit-typed discriminant
            // properties present in the literal — the inference is purely
            // structural (required-vs-optional) and a missing discriminator
            // is itself a signal in tsc's discriminantPropertyInference.
            let absent_required_match = {
                let mut ok = true;
                if let Some(shape) = member_candidates
                    .iter()
                    .find_map(|&candidate| common::object_shape_for_type(self.ctx.types, candidate))
                {
                    for prop in &shape.properties {
                        if prop.optional {
                            continue;
                        }
                        let prop_name_str = self.ctx.types.resolve_atom_ref(prop.name).to_string();
                        // Skip properties that ARE present in the object literal.
                        if present_property_names.contains(&prop_name_str) {
                            continue;
                        }
                        let member_is_array_like = member_candidates.iter().any(|&candidate| {
                            common::array_element_type(self.ctx.types, candidate).is_some()
                                || common::tuple_elements(self.ctx.types, candidate).is_some()
                                || common::object_shape_for_type(self.ctx.types, candidate)
                                    .is_some_and(|shape| {
                                        shape.number_index.is_some() || {
                                            let has_length = shape.properties.iter().any(|prop| {
                                                self.ctx.types.resolve_atom_ref(prop.name).as_ref()
                                                    == "length"
                                            });
                                            let has_array_method =
                                                shape.properties.iter().any(|prop| {
                                                    matches!(
                                                        self.ctx
                                                            .types
                                                            .resolve_atom_ref(prop.name)
                                                            .as_ref(),
                                                        "push" | "pop" | "concat" | "slice"
                                                    )
                                                });
                                            has_length && has_array_method
                                        }
                                    })
                        });
                        if !member_is_array_like
                            && !common::is_unit_type(self.ctx.types, prop.type_id)
                        {
                            continue;
                        }
                        // This member requires a property that the literal doesn't have.
                        // Check if at least one other member doesn't require it (optional or absent).
                        let some_other_doesnt_require = members.iter().any(|&other| {
                            if other == member {
                                return false;
                            }
                            let lazy_other = self.resolve_lazy_type(other);
                            let resolved_other = self.resolve_type_for_property_access(lazy_other);
                            let evaluated_other = self.evaluate_contextual_type(resolved_other);
                            let other_candidates =
                                [evaluated_other, resolved_other, lazy_other, other];
                            let other_prop = other_candidates.iter().find_map(|&candidate| {
                                common::find_property_in_object(
                                    self.ctx.types,
                                    candidate,
                                    prop.name,
                                )
                            });
                            match other_prop {
                                None => true,          // other member doesn't have it at all
                                Some(p) => p.optional, // other member has it as optional
                            }
                        });
                        if some_other_doesnt_require {
                            ok = false;
                            break;
                        }
                    }
                }
                ok
            };

            if unit_match && never_match && absent_required_match {
                let raw_member = raw_members
                    .as_ref()
                    .and_then(|members| members.get(member_index))
                    .copied()
                    .unwrap_or(member);
                matching_members.push(raw_member);
            }
        }

        // Only narrow if we eliminated at least one member.
        if matching_members.is_empty() || matching_members.len() == members.len() {
            return ctx_type;
        }

        if matching_members.len() == 1 {
            matching_members[0]
        } else {
            self.ctx
                .types
                .factory()
                .union_preserve_members(matching_members)
        }
    }

    /// For a shorthand property identifier (e.g., `kind` in `{ kind }`),
    /// resolve it to its declaration. If the declaration is a `const` variable
    /// with a literal initializer, return the literal type.
    fn shorthand_const_literal_type(
        &self,
        name_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        let sym_id = self.resolve_identifier_symbol_without_tracking(name_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl_idx)?;
        // Only handle VariableDeclaration nodes
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        // Check if it's a const declaration
        if !self.ctx.arena.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if var_decl.initializer.is_none() {
            return None;
        }
        self.literal_type_from_initializer(var_decl.initializer)
    }

    fn sanitize_contextual_property_type(&self, property_type: TypeId) -> TypeId {
        if property_type == TypeId::ERROR
            || common::contains_error_type(self.ctx.types, property_type)
        {
            return TypeId::UNKNOWN;
        }
        if let Some(default) = common::type_parameter_default(self.ctx.types, property_type) {
            return default;
        }
        property_type
    }
}
