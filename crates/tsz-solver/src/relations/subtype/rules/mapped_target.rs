//! One-sided Mapped-type subtype relations: relating a generic Mapped source or
//! target against the other side (expansion, homomorphic shortcuts, and the
//! generic-mapped-to-index-signature fall-back). Split out of `generics.rs` to
//! keep that module under the size cap; the logic is unchanged.

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::generics::is_filtering_name_type;
use crate::types::{MappedModifier, MappedType, MappedTypeId, TypeData, TypeId, TypeParamInfo};
use crate::visitor::{
    contains_type_parameter_named, index_access_parts, intersection_list_id, is_empty_object_type,
    keyof_inner_type, object_shape_id, object_with_index_shape_id, type_param_info,
};

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Check Mapped expansion to target (one-sided Mapped case).
    ///
    /// When the target is a Mapped type that can be expanded (e.g., `{ [K in keyof T]: T[K] }`),
    /// we first expand it and then check subtyping.
    pub(crate) fn check_mapped_expansion_target(
        &mut self,
        _source: TypeId,
        target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => {
                if let Some(expanded) = self.try_expand_mapped_with_constraint(mapped_id) {
                    let result = self.check_subtype(expanded, target);
                    if result.is_true() {
                        return result;
                    }
                }

                // Any generic mapped type is always an object type and is therefore assignable to `{}`.
                // Checked before the expensive homomorphic/index checks below (O(1)).
                if is_empty_object_type(self.interner, target) {
                    return SubtypeResult::True;
                }

                // Reverse homomorphic mapped type check:
                // { [K in keyof T]+?: T[K] } (Partial<T>, Readonly<T>, etc.) is
                // assignable to T. In tsc 6.0, homomorphic mapped types are
                // bidirectionally assignable to their source type parameter.
                if self.check_homomorphic_mapped_to_target(mapped_id, target) {
                    return SubtypeResult::True;
                }

                // Generic mapped type to index signature target:
                // A generic mapped type like Partial<T> has an implicit string index
                // signature derived from its template type. If the target has a string
                // index signature and the template is assignable to the element type,
                // the mapped type is assignable.
                if self.check_generic_mapped_to_index_target(mapped_id, target) {
                    return SubtypeResult::True;
                }

                SubtypeResult::False
            }
        }
    }

    /// Check if a homomorphic mapped type is assignable to a type parameter target.
    ///
    /// `{ [K in keyof T]: T[K] }` (identity, Readonly, Required) is assignable to T
    /// because these preserve or narrow the shape of T.
    ///
    /// `Partial<T>` (+? modifier) is NOT assignable to T because it widens properties
    /// to optional — a `Partial<T>` value may have `undefined` where T requires a value.
    pub(crate) fn check_homomorphic_mapped_to_target(
        &mut self,
        mapped_id: MappedTypeId,
        target: TypeId,
    ) -> bool {
        let mapped = self.interner.get_mapped(mapped_id);

        // Must not have name remapping (as clause) — remapping can change keys
        if mapped.name_type.is_some() {
            return false;
        }

        // Mapped types that ADD optionality (Partial<T>) are wider than T,
        // so Partial<T> is NOT assignable to T.
        if mapped.optional_modifier == Some(MappedModifier::Add) {
            return false;
        }

        // Constraint must be keyof(S), or a conditional alias equivalent to
        // keyof(S), for some S.
        let Some(constraint_source) = self.homomorphic_mapped_constraint_source(&mapped) else {
            return false;
        };

        // Check template compatibility with the source's property type S[K].
        // Fast path: if template is exactly S[K] (identity form), no further check needed.
        // General case: check if the template is a subtype of S[K].
        // This handles cases like Denullified<T> where template is NonNullable<T[K]>,
        // which is always <: T[K], so Denullified<T> is assignable to T.
        let template_ok = if let Some((template_obj, template_idx)) =
            index_access_parts(self.interner, mapped.template)
            && let Some(idx_param) = type_param_info(self.interner, template_idx)
            && idx_param.name == mapped.type_param.name
            && template_obj == constraint_source
        {
            true
        } else {
            let k_type_id = self.interner.type_param(TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(mapped.constraint),
                default: None,
                is_const: false,
                origin: mapped.type_param.origin,
            });
            let source_value_type = self.interner.index_access(constraint_source, k_type_id);
            self.check_subtype(mapped.template, source_value_type)
                .is_true()
        };

        if !template_ok {
            return false;
        }

        // The target must be the same type parameter as the constraint source,
        // or assignable to it.
        if constraint_source == target {
            return true;
        }
        if let Some(source_param) = type_param_info(self.interner, constraint_source)
            && let Some(source_constraint) = source_param.constraint
            && self.check_subtype(source_constraint, target).is_true()
        {
            return true;
        }
        if let Some(target_param) = type_param_info(self.interner, target) {
            if let Some(source_param) = type_param_info(self.interner, constraint_source)
                && source_param.name == target_param.name
            {
                return true;
            }
            // Also check if the target's constraint makes it related
            if let Some(target_constraint) = target_param.constraint {
                return self
                    .check_subtype(target_constraint, constraint_source)
                    .is_true()
                    || self
                        .check_subtype(constraint_source, target_constraint)
                        .is_true();
            }
        }

        false
    }

    /// Check if a generic mapped type (source) is assignable to a target with
    /// a string index signature.
    ///
    /// A generic mapped type like `Partial<T>` or `Readonly<T>` has an implicit
    /// string index signature derived from its template. When the target is
    /// `{ [x: string]: E }`, we check if the template type is assignable to E.
    fn check_generic_mapped_to_index_target(
        &mut self,
        mapped_id: MappedTypeId,
        target: TypeId,
    ) -> bool {
        // Target must have a string index signature
        let t_shape_id = object_with_index_shape_id(self.interner, target)
            .or_else(|| object_shape_id(self.interner, target));
        let Some(t_shape_id) = t_shape_id else {
            return false;
        };
        let t_shape = self.interner.object_shape(t_shape_id);
        let Some(ref string_index) = t_shape.string_index else {
            return false;
        };
        let index_value_type = string_index.value_type;

        // Target must not have required named properties that the mapped type can't satisfy
        if t_shape.properties.iter().any(|p| !p.optional) {
            return false;
        }

        let mapped = self.interner.get_mapped(mapped_id);

        // The mapped type's template produces the value type for each property.
        // Check if the template is assignable to the index value type.
        // For Partial<T> with template T[P], T[P] <: any is always true.
        if self
            .check_subtype(mapped.template, index_value_type)
            .is_true()
        {
            return true;
        }

        // Fall-back for a homomorphic template `Obj[P]` whose deferred indexed
        // access cannot reduce — e.g. `{ [P in K]: S[P] }` where `S` is a type
        // parameter constrained to an index-signature object and `P`'s key
        // constraint stays a generic `keyof T`. tsc treats every property value
        // of such a mapped type as a value drawn from `Obj` (its declared
        // properties and/or index signatures). When all of those source values
        // are assignable to the target's index value type, the mapped type is
        // assignable regardless of whether the per-key access resolves.
        self.template_source_index_value_assignable(
            mapped.template,
            mapped.type_param.name,
            index_value_type,
        )
    }

    /// For a homomorphic mapped template of the form `Obj[P]` (where `P` is the
    /// mapped type's own iteration parameter), relate every value `Obj` can yield
    /// — its declared property values and any string/number index-signature
    /// values — to `target_value`.
    ///
    /// Used as a fall-back in [`Self::check_generic_mapped_to_index_target`] when
    /// the deferred indexed access `Obj[P]` cannot reduce to a concrete value
    /// (the key constraint stays generic). The relation succeeds only when `Obj`
    /// has at least one value source and every one of those values is assignable
    /// to `target_value`.
    fn template_source_index_value_assignable(
        &mut self,
        template: TypeId,
        iter_param: tsz_common::interner::Atom,
        target_value: TypeId,
    ) -> bool {
        let Some((obj, idx)) = index_access_parts(self.interner, template) else {
            return false;
        };

        // Only the homomorphic form `Obj[P]` (indexed by the mapped's own
        // iteration parameter) draws its values from `Obj`. A template that
        // indexes by some unrelated key is not covered by this fall-back.
        if type_param_info(self.interner, idx).is_none_or(|p| p.name != iter_param) {
            return false;
        }

        // Resolve the indexed object's apparent type: a bare type parameter
        // contributes its constraint, otherwise the object itself.
        let apparent = match type_param_info(self.interner, obj).and_then(|p| p.constraint) {
            Some(constraint) => self.evaluate_type(constraint),
            None => self.evaluate_type(obj),
        };

        let Some(shape_id) = object_with_index_shape_id(self.interner, apparent)
            .or_else(|| object_shape_id(self.interner, apparent))
        else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);

        // Collect every value `Obj` can yield through a key access: its declared
        // property values plus any string/number index-signature values.
        let mut values: Vec<TypeId> = shape
            .string_index
            .iter()
            .chain(shape.number_index.iter())
            .map(|idx| idx.value_type)
            .collect();
        values.extend(shape.properties.iter().map(|p| p.type_id));
        if values.is_empty() {
            return false;
        }

        values
            .into_iter()
            .all(|value| self.check_subtype(value, target_value).is_true())
    }

    /// Check source to Mapped expansion (one-sided Mapped case).
    ///
    /// When the target is a Mapped type, first try expansion. If expansion fails
    /// (e.g., keyof T where T is a type parameter), fall back to homomorphic
    /// mapped type assignability: source <: { [K in keyof S]: S[K] } holds when
    /// source <: S and the mapped type doesn't remove optionality.
    pub(crate) fn check_source_to_mapped_expansion(
        &mut self,
        source: TypeId,
        target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        // Try distributing homomorphic mapped types over intersection arguments
        // BEFORE expansion. Expansion of mapped types like Readonly<T & { name: string }>
        // is lossy when T is a type parameter: it only produces the concrete properties
        // (e.g., { readonly name: string }), losing the generic T constraint.
        // Distribution preserves the full type structure:
        //   Readonly<T & { name: string }> → Readonly<T> & Readonly<{ name: string }>
        if let Some(distributed) = self.try_distribute_mapped_over_intersection(mapped_id) {
            let result = self.check_subtype(source, distributed);
            if result.is_true() {
                return result;
            }
        }

        // An all-`any` index-signature source (e.g. `Record<keyof T, any>`
        // reduced to its `{ [x: string]: any }` index shape) supplies `any` for
        // every property a still-generic homomorphic mapped target demands.
        // tsc's `any`-propagation accepts it regardless of whether each deferred
        // per-key access `target[K]` resolves, so handle it before the
        // unbounded-key veto below (which would otherwise reject the source for
        // not mentioning `T`).
        if self.any_valued_index_source_satisfies_homomorphic_mapped(source, mapped_id) {
            return SubtypeResult::True;
        }

        // A homomorphic mapped target `{ [K in keyof T]: ... }` over a bare,
        // unresolved type parameter `T` has an *unbounded* required key-set: a
        // concrete instantiation of `T` may carry members its constraint does not
        // advertise (`T extends object` admits `{ a: 1 }`; `T extends { a: number }`
        // admits `{ a: number, b: string }`; …). Expanding such a target through
        // `T`'s constraint keys is therefore only a *lower bound* on the demanded
        // members — a concrete source that merely matches the constraint shape is
        // NOT assignable, and tsc rejects it (TS2322 in value positions, TS2416 /
        // TS2430 in override checks). Only a source genuinely correlated with `T`
        // (one that mentions `T`, e.g. a `T[K]`-derived value) can satisfy the
        // unbounded portion. The constraint-derived expansions below still run so
        // their *failures* keep producing precise per-property diagnostics, but a
        // spurious *accept* from them is vetoed when the source is concrete w.r.t.
        // `T`.
        let constraint_expansion_can_overaccept = self
            .generic_homomorphic_key_param(mapped_id)
            .is_some_and(|tp_id| !self.source_correlated_with_type_param(source, tp_id));

        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => {
                let result = self.check_subtype(source, expanded);
                if result.is_true() && constraint_expansion_can_overaccept {
                    return SubtypeResult::False;
                }
                result
            }
            None => {
                // tsc: an empty object {} is assignable to any mapped type that adds
                // the optional modifier (+?), like Partial<T>. All properties are optional,
                // so an empty object trivially satisfies all constraints.
                {
                    let mapped = self.interner.get_mapped(mapped_id);
                    if mapped.optional_modifier == Some(MappedModifier::Add)
                        && is_empty_object_type(self.interner, source)
                    {
                        return SubtypeResult::True;
                    }
                }

                if !constraint_expansion_can_overaccept
                    && let Some(expanded) = self.try_expand_mapped_with_constraint(mapped_id)
                {
                    let result = self.check_subtype(source, expanded);
                    if result.is_true() {
                        return result;
                    }
                }

                // Homomorphic mapped type shortcut:
                // source <: { [K in keyof S]+?: S[K] } when source <: S
                // and the mapped type doesn't remove optional.
                if self.check_source_to_homomorphic_mapped(source, mapped_id) {
                    return SubtypeResult::True;
                }

                let mapped = self.interner.get_mapped(mapped_id);
                if mapped.constraint == TypeId::ANY
                    && mapped.name_type.is_none()
                    && mapped.template == TypeId::NEVER
                {
                    let evaluated = self.evaluate_type(target);
                    if evaluated != target {
                        let result = self.check_subtype(source, evaluated);
                        if result.is_true() {
                            return result;
                        }
                    }
                }

                SubtypeResult::False
            }
        }
    }

    /// When `mapped_id` is a homomorphic mapped type `{ [K in keyof T]: ... }`
    /// whose key source `T` is still a bare, unresolved type parameter (or an
    /// `infer` position) and which does not ADD optionality or rename keys,
    /// returns the `TypeId` of `T`; otherwise `None`.
    ///
    /// These are exactly the targets whose required key-set is unbounded, so
    /// relating a concrete source to them cannot be decided by expanding the
    /// target through `T`'s constraint keys (see
    /// [`Self::check_source_to_mapped_expansion`]). `+?` (Partial) and
    /// name-remapped (`as`) targets keep their existing handling.
    fn generic_homomorphic_key_param(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        let mapped = self.interner.get_mapped(mapped_id);
        if mapped.optional_modifier == Some(MappedModifier::Add) || mapped.name_type.is_some() {
            return None;
        }
        let constraint_source = keyof_inner_type(self.interner, mapped.constraint)?;
        matches!(
            self.interner.lookup(constraint_source),
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        )
        .then_some(constraint_source)
    }

    /// Whether `source` is correlated with the type parameter `tp_id` — i.e. it
    /// structurally mentions `tp_id` (so it can be a `T`/`T[K]`-derived value that
    /// tracks `T`'s concrete shape). A source that does not mention `tp_id` is
    /// concrete with respect to it and cannot satisfy an unbounded homomorphic
    /// mapped target over `tp_id`.
    fn source_correlated_with_type_param(&self, source: TypeId, tp_id: TypeId) -> bool {
        // Short-circuiting containment walk (no full type-set allocation).
        crate::visitor::contains_type_by_id(self.interner, source, tp_id)
    }

    /// Whether `source` yields `any` for every key it carries — it has at least
    /// one index signature and every index-signature value type and declared
    /// property type is exactly `any`.
    ///
    /// Such a source (e.g. `Record<keyof T, any>` reduced to its
    /// `{ [x: string]: any }` index shape) supplies `any` for every property a
    /// still-generic homomorphic mapped target `{ [K in keyof T]: ... }` demands,
    /// so `tsc`'s `any`-propagation makes the relation hold regardless of how
    /// each deferred per-key access `target[K]` resolves: the source no longer
    /// mentions `T`, yet `any` is assignable to every target property value.
    ///
    /// Gated to exactly `any` — an `unknown`/concrete index value still fails the
    /// ordinary structural comparison — and disabled in Sound Mode alongside the
    /// other `any`-propagation index waivers (see
    /// [`Self::target_string_index_any_waives_missing_index`]).
    fn any_valued_index_source_satisfies_homomorphic_mapped(
        &mut self,
        source: TypeId,
        mapped_id: MappedTypeId,
    ) -> bool {
        if self.disable_method_bivariance {
            return false;
        }
        if self.generic_homomorphic_key_param(mapped_id).is_none() {
            return false;
        }
        let source = self.evaluate_type(source);
        let Some(shape_id) = object_with_index_shape_id(self.interner, source)
            .or_else(|| object_shape_id(self.interner, source))
        else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);
        // A property-only object cannot cover the unbounded key set a homomorphic
        // mapped target over a bare type parameter demands.
        if shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.symbol_index.is_none()
        {
            return false;
        }
        shape
            .string_index
            .iter()
            .chain(shape.number_index.iter())
            .chain(shape.symbol_index.iter())
            .map(|idx| idx.value_type)
            .chain(shape.properties.iter().map(|p| p.type_id))
            .all(TypeId::is_any)
    }

    /// Check if any source type is assignable to a homomorphic mapped type.
    ///
    /// S <: { [K in keyof S]: S[K] } when S is the same as the constraint source
    /// and the mapped type doesn't REMOVE optionality. Removing `-?` (Required)
    /// makes the target NARROWER than the source, so S → Required<S> fails
    /// because S may have optional properties that Required demands.
    fn check_source_to_homomorphic_mapped(
        &mut self,
        source: TypeId,
        mapped_id: MappedTypeId,
    ) -> bool {
        let mapped = self.interner.get_mapped(mapped_id);

        // If there's an as-clause (name_type), it must be a filtering conditional
        // (produces only P or never) for this optimization to apply.
        // Renaming as-clauses (e.g., `as \`bool${P}\``) change property keys,
        // so T is not necessarily assignable to the result type.
        if let Some(name_type) = mapped.name_type
            && !is_filtering_name_type(self.interner, name_type, &mapped)
        {
            return false;
        }

        // Mapped types that REMOVE optionality (-?) like Required<T> are NARROWER
        // than T. The source (which may have optional properties) cannot satisfy
        // the target which demands all properties be present.
        if mapped.optional_modifier == Some(MappedModifier::Remove) {
            return false;
        }

        // Constraint must be keyof(S), or a conditional alias equivalent to
        // keyof(S), for some S.
        let Some(constraint_source) = self.homomorphic_mapped_constraint_source(&mapped) else {
            return false;
        };

        // Fast path: Template is exactly S[K] where K is the iteration parameter
        if let Some((template_obj, template_idx)) =
            index_access_parts(self.interner, mapped.template)
            && let Some(idx_param) = type_param_info(self.interner, template_idx)
            && idx_param.name == mapped.type_param.name
            && template_obj == constraint_source
        {
            return self.check_subtype(source, constraint_source).is_true();
        }

        // General case: construct the source's property value type S[K] where K is
        // the iteration parameter with constraint `keyof S`, then check S[K] <: Template.
        //
        // This handles mapped types like {[P in keyof T]: T[keyof T]} where the template
        // uses a broader index than just the iteration parameter. The visit_index_access
        // rule in the subtype visitor handles S[I] <: T[J] by checking S <: T AND I <: J,
        // and check_type_parameter_subtype handles K <: keyof S via K's constraint.
        let k_type_id = self.interner.type_param(TypeParamInfo {
            name: mapped.type_param.name,
            constraint: Some(mapped.constraint),
            default: None,
            is_const: false,
            origin: mapped.type_param.origin,
        });
        let source_value_type = self.interner.index_access(constraint_source, k_type_id);
        if self
            .check_subtype(source_value_type, mapped.template)
            .is_true()
            && self.check_subtype(source, constraint_source).is_true()
        {
            return true;
        }

        false
    }

    fn homomorphic_mapped_constraint_source(&mut self, mapped: &MappedType) -> Option<TypeId> {
        if let Some(source) = keyof_inner_type(self.interner, mapped.constraint) {
            return Some(self.peel_homomorphic_identity_mapped_source(source));
        }

        let (template_obj, template_idx) = index_access_parts(self.interner, mapped.template)?;
        let idx_param = type_param_info(self.interner, template_idx)?;
        if idx_param.name != mapped.type_param.name {
            return None;
        }

        let full_key_set = self.interner.keyof(template_obj);
        if self.mapped_key_constraint_covers(mapped.constraint, full_key_set)
            && self.mapped_key_constraint_covers(full_key_set, mapped.constraint)
        {
            Some(self.peel_homomorphic_identity_mapped_source(template_obj))
        } else {
            None
        }
    }

    /// Collapse a homomorphic *identity* mapped type to the source object it
    /// preserves, recursively.
    ///
    /// A homomorphic identity mapped type `{ [P in keyof S]: S[P] }` (no name
    /// remap, no `readonly`/`?` modifier change) is interchangeable with `S` as
    /// the picked-from object of an enclosing homomorphic mapped type, because
    /// `tsc` reduces `keyof { [P in keyof S]: S[P] }` to `keyof S` and
    /// `{ [P in keyof S]: S[P] }[K]` to `S[K]`. Nested `Prettify<Prettify<T>>` /
    /// `Id<Id<T>>` wrappers therefore collapse to the single-level case, so a
    /// source type parameter `T` relates to `Id<Id<T>>` exactly as it does to
    /// `Id<T>`. The peel is bounded against pathological self-referential
    /// nesting and only follows pure identity mapped types, so it never widens
    /// the accepted key/value domain (it preserves `tsc` semantics rather than
    /// relaxing them).
    pub(crate) fn peel_homomorphic_identity_mapped_source(&mut self, source: TypeId) -> TypeId {
        let mut current = source;
        for _ in 0..8 {
            // Fast path: a bare type parameter (the common source, and the peel's
            // own terminal) can never be a mapped type, so skip the evaluation.
            if matches!(
                self.interner.lookup(current),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            ) {
                break;
            }
            // Normalize an alias application (`Id<X>`) to its mapped body so the
            // identity shape is observable regardless of the deferred-vs-evaluated
            // representation a nested instantiation happened to mint.
            let normalized = self.evaluate_type(current);
            let Some(TypeData::Mapped(mapped_id)) = self.interner.lookup(normalized) else {
                break;
            };
            let mapped = self.interner.get_mapped(mapped_id);
            if mapped.name_type.is_some()
                || mapped.optional_modifier.is_some()
                || mapped.readonly_modifier.is_some()
            {
                break;
            }
            let Some(inner_source) = keyof_inner_type(self.interner, mapped.constraint) else {
                break;
            };
            let Some((template_obj, template_idx)) =
                index_access_parts(self.interner, mapped.template)
            else {
                break;
            };
            let Some(idx_param) = type_param_info(self.interner, template_idx) else {
                break;
            };
            if idx_param.name != mapped.type_param.name {
                break;
            }
            // The template's indexed object must be the *same type* as the
            // constraint's `keyof` source for this to be a genuine identity mapped
            // `{ [P in keyof S]: S[P] }` (rather than `{ [P in keyof A]: B[P] }`,
            // which is not identity-preserving). Structural identity — not handle
            // equality — so a nested instantiation that minted `S` as two
            // distinct-but-equal representations still matches.
            if template_obj != inner_source
                && !(self.check_subtype(template_obj, inner_source).is_true()
                    && self.check_subtype(inner_source, template_obj).is_true())
            {
                break;
            }
            current = inner_source;
        }
        current
    }

    /// Distribute a homomorphic mapped type over an intersection argument.
    ///
    /// When the mapped type has the form `{ [K in keyof (A & B)]: (A & B)[K] }`
    /// (possibly with readonly/optional modifiers), this is equivalent to
    /// `{ [K in keyof A]: A[K] } & { [K in keyof B]: B[K] }` with the same
    /// modifiers. This implements the tsc equivalence:
    ///   `Readonly<A & B>` ≡ `Readonly<A> & Readonly<B>`
    ///
    /// Returns `Some(distributed_intersection)` if distribution applies, `None` otherwise.
    fn try_distribute_mapped_over_intersection(
        &mut self,
        mapped_id: MappedTypeId,
    ) -> Option<TypeId> {
        let mapped = self.interner.get_mapped(mapped_id);

        // Must not have name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        // Constraint must be keyof(S) for some S
        let constraint_source = keyof_inner_type(self.interner, mapped.constraint)?;

        // S must be an intersection
        let list_id = intersection_list_id(self.interner, constraint_source)?;
        let members = self.interner.type_list(list_id).to_vec();

        if members.len() < 2 {
            return None;
        }

        // Template must be S[K] (identity indexed access form)
        let (template_obj, template_idx) = index_access_parts(self.interner, mapped.template)?;
        let idx_param = type_param_info(self.interner, template_idx)?;
        if idx_param.name != mapped.type_param.name || template_obj != constraint_source {
            return None;
        }

        // Distribute: for each member M, create { [K in keyof M]: M[K] } with same modifiers
        let mut distributed_members = Vec::with_capacity(members.len());
        for &member in &members {
            let member_constraint = self.interner.keyof(member);
            let member_k = self.interner.type_param(TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(member_constraint),
                default: None,
                is_const: false,
                origin: mapped.type_param.origin,
            });
            let member_template = self.interner.index_access(member, member_k);
            let member_mapped = self.interner.mapped(MappedType {
                type_param: mapped.type_param,
                constraint: member_constraint,
                name_type: None,
                template: member_template,
                readonly_modifier: mapped.readonly_modifier,
                optional_modifier: mapped.optional_modifier,
            });
            distributed_members.push(member_mapped);
        }

        Some(self.interner.intersection(distributed_members))
    }

    fn try_expand_mapped_with_constraint(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
        let mapped = self.interner.get_mapped(mapped_id);
        if let Some(TypeData::KeyOf(source)) = self.interner.lookup(mapped.constraint)
            && let Some(TypeData::TypeParameter(param)) = self.interner.lookup(source)
            && let Some(constraint) = param.constraint
        {
            // A self-referential bound like `T extends Box<T>` is not a concrete
            // structural expansion source. Substituting it back into a mapped type
            // can make recursive constraints look satisfiable simply because the
            // relation checker re-enters the same bound coinductively.
            if contains_type_parameter_named(self.interner, constraint, param.name) {
                return None;
            }

            let subst = TypeSubstitution::single(param.name, constraint);
            // Use keyof(constraint) directly to prevent eager evaluation
            // which would break array/tuple preservation in evaluate_mapped.
            let inst_constraint = self.interner.keyof(constraint);
            let inst_template = instantiate_type(self.interner, mapped.template, &subst);
            let inst_name = mapped
                .name_type
                .map(|n| instantiate_type(self.interner, n, &subst));
            let new_mapped_id = self.interner.mapped(MappedType {
                type_param: mapped.type_param,
                constraint: inst_constraint,
                name_type: inst_name,
                template: inst_template,
                optional_modifier: mapped.optional_modifier,
                readonly_modifier: mapped.readonly_modifier,
            });
            if let Some(TypeData::Mapped(m_id)) = self.interner.lookup(new_mapped_id) {
                let new_mapped = self.interner.get_mapped(m_id);
                let res = crate::evaluation::evaluate::evaluate_mapped(self.interner, &new_mapped);
                if res != TypeId::ERROR && res != new_mapped_id {
                    return Some(res);
                }
            }
        }
        None
    }
}
