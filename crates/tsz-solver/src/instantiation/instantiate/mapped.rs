//! `Mapped`-type instantiation: the `TypeData::Mapped` arm of
//! `instantiate_key`.
//!
//! Carries the homomorphic special cases (union distribution, `any`-source
//! array results, tuple/array preservation, primitive pass-through) ahead of
//! the standard constraint/template substitution and the eager-evaluation
//! deferral gates.

use rustc_hash::FxHashMap;

use crate::types::{MappedType, MappedTypeId, TypeData, TypeId, TypeParamInfo};

use super::{
    TypeInstantiator, TypeSubstitution, conditional_condition_needs_resolver, instantiate_type,
    instantiate_type_with_shadowed, mapped_constraint_needs_resolver,
    template_has_lazy_application_in_composite, type_contains_lazy_application,
};

impl<'a> TypeInstantiator<'a> {
    /// Instantiate a mapped type: instantiate constraint and template.
    pub(super) fn instantiate_mapped(&mut self, mapped_id: &MappedTypeId) -> TypeId {
        let mut mapped = self.interner.get_mapped(*mapped_id);
        let tp_slice = std::slice::from_ref(&mapped.type_param);
        let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(tp_slice);

        // Restore homomorphic modifier inheritance for a self-indexed
        // mapped type `{ [Q in P]: T[P] }` whose constraint parameter is
        // substituted by a single key (ts-essentials `ReadonlyKeys` /
        // `WritableKeys` substrate). Done before the constraint/template
        // substitution below collapses `T[P]` to `T["k"]`.
        if let Some(rewritten) = self.rewrite_single_key_self_indexed_template(&mapped) {
            mapped = MappedType {
                template: rewritten,
                ..mapped
            };
        }

        // Homomorphic array/tuple handling must run before standard
        // instantiation collapses `keyof T` to a flat union.

        // HOMOMORPHIC UNION DISTRIBUTION (tsc: instantiateMappedType → mapTypeWithAlias)
        // Excluded: array/tuple-like unions are handled by the blocks below.
        if !self.preserve_meta_types
            && let Some(TypeData::KeyOf(keyof_source)) = self.interner.lookup(mapped.constraint)
            && let Some(TypeData::TypeParameter(tp_info)) = self.interner.lookup(keyof_source)
            && !self.is_shadowed(tp_info.name)
            && let Some(substituted) = self.substitution.get(tp_info.name)
        {
            let resolved = crate::evaluation::evaluate::evaluate_type(self.interner, substituted);
            if let Some(TypeData::Union(list_id)) = self.interner.lookup(resolved)
                && !Self::is_array_or_tuple_like(self.interner, resolved)
            {
                let members: Vec<TypeId> = self.interner.type_list(list_id).to_vec();
                let mut results = Vec::with_capacity(members.len());
                // The iteration variable is a fresh local of this
                // mapped type; without shadowing it during per-member
                // splicing, the constraint-resolution fallback in
                // `instantiate_key` rewrites `K` to its instantiated
                // constraint and produces `<member>[keyof <member>]`
                // where the source said `<member>[K]`.
                let iter_var_shadow = [mapped.type_param.name];
                // Reuse one substitution map across distributed
                // members. The only per-member semantic change is the
                // homomorphic source parameter (`tp_info.name`), so a
                // single mutable map preserves the same bindings as
                // clone+insert while avoiding O(members * subst_len)
                // entry copies for large union sources.
                let mut member_subst = self.substitution.clone();
                for &member in &members {
                    if crate::visitors::visitor_predicates::is_primitive_type(self.interner, member)
                    {
                        results.push(member);
                        continue;
                    }
                    member_subst.insert(tp_info.name, member);
                    let inst = |t| {
                        instantiate_type_with_shadowed(
                            self.interner,
                            t,
                            &member_subst,
                            &iter_var_shadow,
                        )
                    };
                    results.push(self.interner.mapped(MappedType {
                        constraint: inst(mapped.constraint),
                        template: inst(mapped.template),
                        name_type: mapped.name_type.map(&inst),
                        type_param: TypeParamInfo {
                            constraint: mapped.type_param.constraint.map(&inst),
                            default: mapped.type_param.default.map(&inst),
                            ..mapped.type_param
                        },
                        ..mapped
                    }));
                }
                self.exit_shadowing_scope(shadowed_len, saved_visiting);
                return self.interner.union(results);
            }
        }

        // tsc's `instantiateMappedType`: when the homomorphic source T
        // resolves to `any` and T is constrained to array/tuple types,
        // the result is an array shape — independent of whether the
        // template references T[K]. Templates that DO reference T[K]
        // are still handled by the main array-preservation block below
        // (which mirrors tsc's full instantiateMappedArrayType path).
        if !self.preserve_meta_types
            && crate::type_queries::mapped::is_identity_name_mapping(self.interner, &mapped)
            && let Some(TypeData::KeyOf(keyof_source)) = self.interner.lookup(mapped.constraint)
            && let Some(TypeData::TypeParameter(tp_info)) = self.interner.lookup(keyof_source)
            && !self.is_shadowed(tp_info.name)
            && let Some(substituted) = self.substitution.get(tp_info.name)
            && !Self::mapped_template_uses_source_index(
                self.interner,
                mapped.template,
                keyof_source,
                mapped.type_param.name,
            )
            && crate::evaluation::evaluate::evaluate_type(self.interner, substituted) == TypeId::ANY
            && tp_info.constraint.is_some_and(|c| {
                let ec = crate::evaluation::evaluate::evaluate_type(self.interner, c);
                Self::is_array_or_tuple_like(self.interner, ec)
            })
        {
            // Substitute T → any in the template, then K → number, then
            // wrap in Array (matching tsc's instantiateMappedArrayType).
            let new_template = self.instantiate(mapped.template);
            self.exit_shadowing_scope(shadowed_len, saved_visiting);

            let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
            let mapped_element = crate::evaluation::evaluate::evaluate_type(
                self.interner,
                instantiate_type(self.interner, new_template, &subst),
            );

            let final_element = if matches!(
                mapped.optional_modifier,
                Some(crate::types::MappedModifier::Add)
            ) {
                self.interner.union2(mapped_element, TypeId::UNDEFINED)
            } else {
                mapped_element
            };

            let array_type = self.interner.array(final_element);
            return if matches!(
                mapped.readonly_modifier,
                Some(crate::types::MappedModifier::Add)
            ) {
                self.interner.readonly_type(array_type)
            } else {
                array_type
            };
        }

        if !self.preserve_meta_types
            && crate::type_queries::mapped::is_identity_name_mapping(self.interner, &mapped)
            && let Some(TypeData::KeyOf(keyof_source)) = self.interner.lookup(mapped.constraint)
            && let Some(TypeData::TypeParameter(tp_info)) = self.interner.lookup(keyof_source)
            && !self.is_shadowed(tp_info.name)
            && let Some(substituted) = self.substitution.get(tp_info.name)
        {
            let template_uses_source_index = Self::mapped_template_uses_source_index(
                self.interner,
                mapped.template,
                keyof_source,
                mapped.type_param.name,
            );
            let resolved = crate::evaluation::evaluate::evaluate_type(self.interner, substituted);

            // tsc: When a homomorphic mapped type's source type parameter
            // is instantiated with `any`, the result depends on the type
            // parameter's constraint:
            //   - Array/tuple constraint → produce array result
            //   - Non-array constraint → fall through to standard mapped
            //     type instantiation (produces `{ [x: string]: ... }`)
            // We must NOT unconditionally return TypeId::ANY because that
            // makes `Objectish<any>` assignable to `any[]`, which is wrong.
            if resolved == TypeId::ANY {
                let constraint_is_array_like = tp_info.constraint.is_some_and(|c| {
                    let ec = crate::evaluation::evaluate::evaluate_type(self.interner, c);
                    Self::is_array_or_tuple_like(self.interner, ec)
                });

                if constraint_is_array_like {
                    // Array/tuple-constrained T with any: produce array.
                    // Substitute K → number in the template.
                    let new_template = self.instantiate(mapped.template);
                    self.exit_shadowing_scope(shadowed_len, saved_visiting);

                    let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
                    let mapped_element = crate::evaluation::evaluate::evaluate_type(
                        self.interner,
                        instantiate_type(self.interner, new_template, &subst),
                    );

                    let final_element = if matches!(
                        mapped.optional_modifier,
                        Some(crate::types::MappedModifier::Add)
                    ) {
                        self.interner.union2(mapped_element, TypeId::UNDEFINED)
                    } else {
                        mapped_element
                    };

                    let array_type = self.interner.array(final_element);
                    return if matches!(
                        mapped.readonly_modifier,
                        Some(crate::types::MappedModifier::Add)
                    ) {
                        self.interner.readonly_type(array_type)
                    } else {
                        array_type
                    };
                }
                // IDENTITY homomorphic mapped type with `any`: return any.
                // tsc returns `any` ONLY for identity templates (`T[K]`), not
                // for non-identity templates like `Box<T[K]>`.
                // For non-identity templates, we fall through to standard
                // instantiation which produces `{ [x: string]: Box<any> }`.
                let is_identity_template =
                    crate::index_access_parts(self.interner, mapped.template).is_some_and(
                        |(obj, key)| {
                            obj == keyof_source
                                && matches!(
                                    self.interner.lookup(key),
                                    Some(TypeData::TypeParameter(kp))
                                        if kp.name == mapped.type_param.name
                                )
                        },
                    );
                if is_identity_template {
                    self.exit_shadowing_scope(shadowed_len, saved_visiting);
                    return TypeId::ANY;
                }
                // Non-identity template: fall through to standard instantiation
            }

            // Check for Tuple first (tsc: instantiateMappedTupleType)
            // Must also handle ReadonlyType wrapping Tuple
            let tuple_source = if resolved.is_intrinsic() {
                None
            } else {
                match self.interner.lookup(resolved) {
                    Some(TypeData::Tuple(tid)) => Some((tid, false)),
                    Some(TypeData::ReadonlyType(inner)) => {
                        let ir = crate::evaluation::evaluate::evaluate_type(self.interner, inner);
                        if ir.is_intrinsic() {
                            None
                        } else {
                            match self.interner.lookup(ir) {
                                Some(TypeData::Tuple(tid)) => Some((tid, true)),
                                _ => None,
                            }
                        }
                    }
                    _ => None,
                }
            };
            if let Some((tuple_id, source_readonly)) = tuple_source {
                use crate::types::MappedModifier;
                let elements = self.interner.tuple_list(tuple_id);
                // Instantiate template first (substitutes T, keeps K shadowed).
                // After this `new_template` holds the *resolved* source tuple
                // wherever T appeared.
                let new_template = self.instantiate(mapped.template);
                self.exit_shadowing_scope(shadowed_len, saved_visiting);

                // Per-element rebinding mirrors tsc's
                // `instantiateMappedTupleType`. The choice of (template, key)
                // determines whether `T[K]` resolves to this element's own
                // type or the union of every element type. The naive
                // pre-fix loop bound K = "i" for every element, which:
                //   - dropped the Array<> wrapper from a rest element's
                //     type_id (producing structurally invalid tuples like
                //     `[string, ...number]`); and
                //   - silently widened any fixed element after a rest to
                //     the union of all element types, because `T["i"]` is
                //     ambiguous when the rest range could be 0 or more
                //     elements long.
                //
                // The four cases below mirror tsc's switch on per-element kind.
                enum ElemBinding {
                    /// Rest of `Array<E>` / `readonly E[]` — rewrite the
                    /// resolved source to `Array<E>` and bind K = number;
                    /// the result must re-wrap in `Array<>` so the rest's
                    /// `type_id` stays array-shaped.
                    RestArray(TypeId),
                    /// Rest of an opaque type (type parameter, lazy ref,
                    /// etc.) — bind K = number on the existing template
                    /// so the deferred `T[K]` shape is preserved.
                    OpaqueRest,
                    /// Fixed element after at least one rest — the
                    /// numeric index on the full source is ambiguous, so
                    /// rewrite the source to a single-element proxy and
                    /// bind K = "0".
                    SuffixFixed,
                    /// Fixed element before any rest — the literal index
                    /// resolves unambiguously, no rebinding needed.
                    PrefixFixed,
                }

                let rebind_source = |new_source: TypeId| {
                    let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
                    crate::evaluation::evaluate_rules::substitute::substitute_exact_type_db(
                        self.interner,
                        new_template,
                        resolved,
                        new_source,
                        &mut memo,
                    )
                };

                let mut seen_rest = false;
                let mut new_elements = Vec::with_capacity(elements.len());
                for (i, elem) in elements.iter().enumerate() {
                    let is_suffix = seen_rest && !elem.rest;
                    if elem.rest {
                        seen_rest = true;
                    }

                    let binding = match (elem.rest, self.interner.lookup(elem.type_id)) {
                        (true, Some(TypeData::Array(_))) => ElemBinding::RestArray(elem.type_id),
                        (true, Some(TypeData::ReadonlyType(roi)))
                            if matches!(self.interner.lookup(roi), Some(TypeData::Array(_))) =>
                        {
                            ElemBinding::RestArray(roi)
                        }
                        (true, _) => ElemBinding::OpaqueRest,
                        (false, _) if is_suffix => ElemBinding::SuffixFixed,
                        (false, _) => ElemBinding::PrefixFixed,
                    };

                    let (rebound_template, key_type) = match binding {
                        ElemBinding::RestArray(rest_arr) => {
                            (rebind_source(rest_arr), TypeId::NUMBER)
                        }
                        ElemBinding::OpaqueRest => (new_template, TypeId::NUMBER),
                        ElemBinding::SuffixFixed => {
                            let proxy = self
                                .interner
                                .tuple(vec![crate::types::TupleElement::fixed(elem.type_id)]);
                            (rebind_source(proxy), self.interner.literal_string("0"))
                        }
                        ElemBinding::PrefixFixed => {
                            (new_template, self.interner.literal_string(&i.to_string()))
                        }
                    };

                    let subst = TypeSubstitution::single(mapped.type_param.name, key_type);
                    let mapped_type = crate::evaluation::evaluate::evaluate_type(
                        self.interner,
                        instantiate_type(self.interner, rebound_template, &subst),
                    );

                    // Re-wrap a `...E[]` rest in `Array<>`; absorb
                    // `Add ?` on a rest as `T | undefined` (a rest can
                    // not syntactically combine with `?`); apply the
                    // optional modifier to fixed slots only.
                    let (type_id, optional) = match (binding, mapped.optional_modifier) {
                        (ElemBinding::RestArray(_), Some(MappedModifier::Add)) => (
                            self.interner
                                .union2(self.interner.array(mapped_type), TypeId::UNDEFINED),
                            elem.optional,
                        ),
                        (ElemBinding::RestArray(_), _) => {
                            (self.interner.array(mapped_type), elem.optional)
                        }
                        (ElemBinding::OpaqueRest, Some(MappedModifier::Add)) => (
                            self.interner.union2(mapped_type, TypeId::UNDEFINED),
                            elem.optional,
                        ),
                        (ElemBinding::OpaqueRest, _) | (_, None) => (mapped_type, elem.optional),
                        (_, Some(MappedModifier::Add)) => (mapped_type, true),
                        (_, Some(MappedModifier::Remove)) => (mapped_type, false),
                    };

                    new_elements.push(crate::types::TupleElement {
                        type_id,
                        name: elem.name,
                        optional,
                        rest: elem.rest,
                    });
                }

                let tuple_type = self.interner.tuple(new_elements);
                return if mapped.resolve_readonly(source_readonly) {
                    self.interner.readonly_type(tuple_type)
                } else {
                    tuple_type
                };
            }

            // Then check for Array (tsc: instantiateMappedArrayType)
            let array_element = Self::extract_array_element(self.interner, resolved);
            if let Some((_element_type, source_readonly)) = array_element {
                // Produce array result: substitute K → number in the template
                let new_template = self.instantiate(mapped.template);
                self.exit_shadowing_scope(shadowed_len, saved_visiting);

                let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
                let mapped_element = crate::evaluation::evaluate::evaluate_type(
                    self.interner,
                    crate::instantiation::instantiate::instantiate_type(
                        self.interner,
                        new_template,
                        &subst,
                    ),
                );

                // Apply mapped type modifiers
                let final_element = if matches!(
                    mapped.optional_modifier,
                    Some(crate::types::MappedModifier::Add)
                ) {
                    self.interner.union2(mapped_element, TypeId::UNDEFINED)
                } else {
                    mapped_element
                };

                let array_type = self.interner.array(final_element);
                return if mapped.resolve_readonly(source_readonly) {
                    self.interner.readonly_type(array_type)
                } else {
                    array_type
                };
            }

            // Primitive homomorphic sources pass through unchanged.
            if template_uses_source_index
                && Self::is_primitive_or_primitive_union(self.interner, resolved)
            {
                self.exit_shadowing_scope(shadowed_len, saved_visiting);
                return resolved;
            }

            if let Some(result) =
                self.try_expand_substituted_homomorphic_object_mapped(&mapped, resolved)
            {
                self.exit_shadowing_scope(shadowed_len, saved_visiting);
                return result;
            }
        }

        tracing::trace!(
            tp_name = ?self.interner.resolve_atom_ref(mapped.type_param.name),
            constraint = mapped.constraint.0,
            constraint_key = ?self.interner.lookup(mapped.constraint),
            shadowed = ?self.shadowed.iter().map(|a| self.interner.resolve_atom_ref(*a)).collect::<Vec<_>>(),
            subst = ?self.substitution.map.iter().map(|(k, v)| (self.interner.resolve_atom_ref(*k), v.0)).collect::<Vec<_>>(),
            "instantiate Mapped: about to instantiate constraint"
        );
        let saved_preserve_unsubstituted = self.preserve_unsubstituted_type_params;
        self.preserve_unsubstituted_type_params = true;

        let new_constraint = self.instantiate(mapped.constraint);
        let new_template = self.instantiate(mapped.template);
        let new_name_type = mapped.name_type.map(|t| self.instantiate(t));
        let new_param_constraint = mapped.type_param.constraint.map(|c| self.instantiate(c));
        let new_param_default = mapped.type_param.default.map(|d| self.instantiate(d));

        self.preserve_unsubstituted_type_params = saved_preserve_unsubstituted;

        self.exit_shadowing_scope(shadowed_len, saved_visiting);

        tracing::trace!(
            old_constraint = mapped.constraint.0,
            new_constraint = new_constraint.0,
            new_constraint_key = ?self.interner.lookup(new_constraint),
            old_template = mapped.template.0,
            new_template = new_template.0,
            "instantiate Mapped: result"
        );

        // If the mapped type is unchanged after substitution (e.g., because
        // the mapped type's own type parameter shadowed the outer substitution),
        // return the original to avoid eager evaluation that would collapse it.
        let unchanged = new_constraint == mapped.constraint
            && new_template == mapped.template
            && new_name_type == mapped.name_type
            && new_param_constraint == mapped.type_param.constraint
            && new_param_default == mapped.type_param.default;

        if unchanged {
            tracing::trace!("instantiate Mapped: UNCHANGED, returning original");
            return self.interner.mapped(mapped);
        }

        let instantiated = MappedType {
            type_param: TypeParamInfo {
                is_const: false,
                name: mapped.type_param.name,
                constraint: new_param_constraint,
                default: new_param_default,
                origin: mapped.type_param.origin,
            },
            constraint: new_constraint,
            name_type: new_name_type,
            template: new_template,
            readonly_modifier: mapped.readonly_modifier,
            optional_modifier: mapped.optional_modifier,
        };

        // Trigger evaluation immediately for changed mapped types.
        // This converts MappedType { constraint: "host"|"port", ... }
        // into Object { host?: string, port?: number }
        // Without this, the MappedType is returned unevaluated, causing subtype checks to fail.
        //
        // However, skip eager evaluation when the template Conditional's condition
        // references a body the `NoopResolver` cannot expand: a direct `Lazy(DefId)`
        // or a lazy *application* `Application(Lazy, args)` (cross-file
        // `T[K] extends V<any>`). The per-key subtype check then silently fails and
        // a `{ [K in keyof T]: … }[keyof T]` filter collapses to `never`; defer so
        // the resolver-backed outer evaluator decides each key. The lazy-app check is
        // contained-in (reached via the iteration var, e.g. `Conc[K]`); bare `Lazy`
        // stays top-only (keeps `undefined extends T[P]` eager).
        let mapped_type = self.interner.mapped(instantiated);
        let has_lazy_conditional_boundary =
            conditional_condition_needs_resolver(self.interner, new_template);
        // Also skip eager evaluation when the template contains Application
        // types whose base is a Lazy(DefId) reference (e.g. recursive type
        // aliases like `Spec<T[P]>`).  The instantiator's NoopResolver cannot
        // resolve these references, so the evaluator would silently drop
        // unresolvable union members.  Deferring lets the outer evaluator
        // (which has a proper TypeResolver) handle the full expansion.
        let has_lazy_application =
            template_has_lazy_application_in_composite(self.interner, new_template);
        // Same hazard for the `as` clause: a Lazy alias application in
        // `name_type` collapses to `never` under NoopResolver eager
        // evaluation, filtering out every key.
        let name_type_has_lazy_application =
            new_name_type.is_some_and(|nt| type_contains_lazy_application(self.interner, nt));
        let resolver_dependent_constraint =
            mapped_constraint_needs_resolver(self.interner, new_constraint);
        if self.preserve_meta_types
            || has_lazy_conditional_boundary
            || has_lazy_application
            || name_type_has_lazy_application
            || resolver_dependent_constraint
        {
            mapped_type
        } else if crate::visitor::contains_type_parameters(self.interner, new_constraint) {
            // Don't eagerly evaluate when the constraint still contains type
            // parameters (e.g., `keyof __infer_0` during generic call inference).
            // Premature evaluation would resolve `keyof T` through T's constraint
            // (e.g., `keyof Record<string, string>` → `string`), destroying the
            // homomorphic `keyof T` pattern needed for reverse-mapped inference.
            // The constraint collection and post-inference check will evaluate
            // the mapped type after inference resolves the type parameters.
            mapped_type
        } else {
            crate::evaluation::evaluate::evaluate_type(self.interner, mapped_type)
        }
    }
}
