use super::assignability_diagnostic_common as common;
use crate::query_boundaries::assignability::is_type_parameter_like;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns true when a bivariant-assignability mismatch should produce a diagnostic.
    ///
    /// Uses the bivariant relation
    /// entrypoint for method-compatibility scenarios.
    pub(crate) fn should_report_assignability_mismatch_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        if self
            .checker_only_assignability_failure_reason(source, target)
            .is_some()
        {
            return true;
        }

        // Route both the relation decision and weak-union skip hint through
        // the bivariant `RelationOutcome` helper.
        let outcome = self.bivariant_callbacks_relation_outcome(source, target);
        if outcome.related {
            return false;
        }

        // `should_skip_weak_union_error_with_outcome` is the sole authority
        // over the weak-union skip decision — including for `weak_union_violation`
        // cases. Do not add an outer `!outcome.weak_union_violation` gate here;
        // that guard suppresses TS2416 for non-object-literal sources (e.g.
        // class property declarations) where the skip should NOT fire.
        !self.should_skip_weak_union_error_with_outcome(source, target, source_idx, Some(&outcome))
    }

    /// Check bidirectional assignability.
    ///
    /// Useful in checker locations that need type comparability/equivalence-like checks.
    pub(crate) fn are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.type_comparability_relation_outcome(left, right)
            .related
            && self
                .type_comparability_relation_outcome(right, left)
                .related
    }

    /// Check if two object types with call/construct signatures are comparable
    /// because at least one has generic type parameters.
    ///
    /// In tsc's Comparable relation, object types with generic call signatures
    /// are considered comparable to concrete call signature objects because the
    /// generic could potentially be instantiated to match. For example:
    /// `{ fn<T, U extends T>(x: T, y: U): T }` is comparable to
    /// `{ fn(x: Base, y: C): Base }` because T=Base, U=C is a valid instantiation.
    ///
    /// This checks both direct callable shapes (for Callable types) and
    /// property-level callable shapes (for Object types with method properties).
    fn objects_with_generic_signatures_are_comparable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source_resolved = self.evaluate_type_with_resolution(source);
        let target_resolved = self.evaluate_type_with_resolution(target);

        let src_has_generics = self.type_has_generic_signatures(source_resolved);
        let tgt_has_generics = self.type_has_generic_signatures(target_resolved);

        // At least one side must have generic type parameters
        if !src_has_generics && !tgt_has_generics {
            return false;
        }

        // Both must be object-like types (have callable shape or object shape)
        let src_is_object_like = self.is_object_or_callable_type(source_resolved);
        let tgt_is_object_like = self.is_object_or_callable_type(target_resolved);

        src_is_object_like && tgt_is_object_like
    }

    /// Check if a type has any generic call/construct signatures, either directly
    /// (Callable/Function type) or through object properties.
    fn type_has_generic_signatures(&self, type_id: TypeId) -> bool {
        // Check direct callable shape (CallableShape has call_signatures + construct_signatures)
        if let Some(shape) = common::callable_shape_for_type(self.ctx.types, type_id) {
            let has_generic_sigs = shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
                .any(|sig| !sig.type_params.is_empty());
            if has_generic_sigs {
                return true;
            }
        }

        // Check direct function shape (FunctionShape has type_params)
        if let Some(func_shape) = common::function_shape_for_type(self.ctx.types, type_id)
            && !func_shape.type_params.is_empty()
        {
            return true;
        }

        // Check object properties for callable/function types with generics
        if let Some(obj_shape) = common::object_shape_for_type(self.ctx.types, type_id) {
            for prop in &obj_shape.properties {
                // Check callable property types
                if let Some(callable) =
                    common::callable_shape_for_type(self.ctx.types, prop.type_id)
                {
                    let has_generic_sigs = callable
                        .call_signatures
                        .iter()
                        .chain(callable.construct_signatures.iter())
                        .any(|sig| !sig.type_params.is_empty());
                    if has_generic_sigs {
                        return true;
                    }
                }
                // Check function property types
                if let Some(func_shape) =
                    common::function_shape_for_type(self.ctx.types, prop.type_id)
                    && !func_shape.type_params.is_empty()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type is object-like (Callable, Object, or Function type).
    fn is_object_or_callable_type(&self, type_id: TypeId) -> bool {
        common::callable_shape_for_type(self.ctx.types, type_id).is_some()
            || common::object_shape_for_type(self.ctx.types, type_id).is_some()
            || common::has_function_shape(self.ctx.types, type_id)
    }

    /// Check if two object types are comparable because their function-typed
    /// properties have overlapping arity.
    ///
    /// In tsc's comparable relation, objects like `{ fn(a?: Base): void }` and
    /// `{ fn(a?: C): void }` are considered comparable because both functions
    /// can be called with 0 arguments (all optional). The comparable relation
    /// threads through object properties and checks function signatures for
    /// arity overlap, not strict assignability.
    fn objects_with_arity_overlapping_functions_are_comparable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use common::{function_shape_for_type, object_shape_for_type};

        let source_resolved = self.evaluate_type_with_resolution(source);
        let target_resolved = self.evaluate_type_with_resolution(target);

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source_resolved) else {
            return false;
        };
        let Some(target_shape) = object_shape_for_type(self.ctx.types, target_resolved) else {
            return false;
        };

        // Need at least one common property that is a function type
        let mut found_function_prop = false;

        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                // Check if both properties are function types
                let src_func = function_shape_for_type(self.ctx.types, source_prop.type_id);
                let tgt_func = function_shape_for_type(self.ctx.types, target_prop.type_id);

                match (src_func, tgt_func) {
                    (Some(src_fn), Some(tgt_fn)) => {
                        found_function_prop = true;
                        // Check arity overlap: min arity of one <= max arity of other
                        let src_min = src_fn.params.iter().filter(|p| p.is_required()).count();
                        let tgt_min = tgt_fn.params.iter().filter(|p| p.is_required()).count();
                        let src_has_rest = src_fn.params.iter().any(|p| p.rest);
                        let tgt_has_rest = tgt_fn.params.iter().any(|p| p.rest);
                        let src_max = if src_has_rest {
                            usize::MAX
                        } else {
                            src_fn.params.len()
                        };
                        let tgt_max = if tgt_has_rest {
                            usize::MAX
                        } else {
                            tgt_fn.params.len()
                        };

                        // Arity ranges must overlap: [src_min, src_max] ∩ [tgt_min, tgt_max] != empty
                        if src_min > tgt_max || tgt_min > src_max {
                            return false;
                        }

                        // Thread through signature: even with overlapping arity, tsc's
                        // comparable relation requires pairwise parameter comparability
                        // and return-type comparability. Two optional params of unrelated
                        // types are still comparable because both admit `undefined`
                        // (e.g., `a?: Base` vs `a?: C`); skip those positions. Rest
                        // params compare by their element type.
                        let min_pairs = src_fn.params.len().min(tgt_fn.params.len());
                        let mut sig_ok = true;
                        for i in 0..min_pairs {
                            let sp = &src_fn.params[i];
                            let tp = &tgt_fn.params[i];
                            if sp.optional && tp.optional && !sp.rest && !tp.rest {
                                continue;
                            }
                            let src_t = if sp.rest {
                                common::array_element_type(self.ctx.types, sp.type_id)
                                    .unwrap_or(sp.type_id)
                            } else {
                                sp.type_id
                            };
                            let tgt_t = if tp.rest {
                                common::array_element_type(self.ctx.types, tp.type_id)
                                    .unwrap_or(tp.type_id)
                            } else {
                                tp.type_id
                            };
                            if !self.is_type_comparable_to(src_t, tgt_t) {
                                sig_ok = false;
                                break;
                            }
                        }
                        if !sig_ok {
                            return false;
                        }
                        if !self.is_type_comparable_to(src_fn.return_type, tgt_fn.return_type) {
                            return false;
                        }
                    }
                    (None, None) => {
                        // Neither is a function type — check normal comparability
                        let prop_comparable = self
                            .type_comparability_relation_outcome(
                                source_prop.type_id,
                                target_prop.type_id,
                            )
                            .related
                            || self
                                .type_comparability_relation_outcome(
                                    target_prop.type_id,
                                    source_prop.type_id,
                                )
                                .related;
                        if !prop_comparable {
                            return false;
                        }
                    }
                    _ => {
                        // One is function, the other is not — not comparable
                        return false;
                    }
                }
            }
        }

        found_function_prop
    }

    /// Reduce an instantiable indexed-access operand (`Obj[Idx]`) to its base
    /// constraint for comparability, leaving everything else unchanged.
    ///
    /// Mirrors tsc's `getReducedApparentType` for the indexed-access case:
    /// `Parameters<F>["length"]` (where `F extends (...args: any[]) => any`)
    /// reduces through `F`'s constraint to `any[]["length"]` = `number`. The
    /// underlying reduction lives in the solver's
    /// `reduce_index_access_to_base_constraint`, a comparability-only reducer
    /// kept off the shared `get_base_constraint_of_type` hot path (so it does
    /// not perturb assignment narrowing / constraint validation). Non-indexed
    /// and type-parameter operands pass through unchanged.
    fn reduce_instantiable_indexed_access(&mut self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::comparability::reduce_index_access_to_base_constraint(
            self.ctx.types,
            type_id,
        )
    }

    /// Apparent type used by the comparable relation, mirroring tsc's
    /// `getApparentType` for instantiable operands.
    ///
    /// A bare type parameter resolves to its constraint (or `unknown`). A deferred
    /// conditional (`T extends U ? X : Y`) resolves to its default constraint
    /// (`getDefaultConstraintOfConditionalType`, the union of the inferred branch
    /// results); if that constraint is itself a bare type parameter — as in
    /// `Exclude<T, U>` whose constraint reduces to `T` — it is resolved one further
    /// step to the parameter's apparent type so the comparable relation sees a
    /// concrete value-space. Non-instantiable types are returned unchanged.
    fn comparable_apparent_type(&mut self, ty: TypeId, is_type_param: bool) -> TypeId {
        let apparent = if is_type_param {
            self.get_type_param_apparent_type(ty)
        } else {
            match crate::query_boundaries::conditional_constraints::conditional_default_constraint(
                self.ctx.types,
                ty,
            ) {
                Some(constraint) if is_type_parameter_like(self.ctx.types, constraint) => {
                    self.get_type_param_apparent_type(constraint)
                }
                Some(constraint) => constraint,
                None => ty,
            }
        };
        self.reduce_instantiable_indexed_access(apparent)
    }

    /// Check if two types are comparable (overlap).
    ///
    /// Corresponds to TypeScript's `areTypesComparable`: returns true if the types
    /// have any overlap. TSC's comparableRelation differs from assignability:
    /// - For union sources: uses `someTypeRelatedToType` (ANY member suffices)
    /// - For union targets: also checks per-member overlap
    /// - For `TypeParameter` sources: uses apparent type (constraint or `unknown`)
    /// - Special carve-out: two unrelated type params are NOT comparable
    ///
    /// Used for switch/case comparability (TS2678), equality narrowing,
    /// relational operator checks (TS2365), etc.
    pub(crate) fn is_type_comparable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        // Identity: any type is trivially comparable to itself
        if source == target {
            return true;
        }

        // Resolve type parameters to their apparent types for comparison.
        // In tsc, `isTypeComparableTo` uses `getReducedApparentType` for TypeParam sources,
        // and has a carve-out when BOTH source and target are type parameters (only comparable
        // if one constrains to the other). See tsc checker.ts:23671-23684.
        let source_is_tp = is_type_parameter_like(self.ctx.types, source);
        let target_is_tp = is_type_parameter_like(self.ctx.types, target);

        if source_is_tp && target_is_tp {
            // Both are type parameters: only comparable if one constrains to the other.
            // Unconstrained T is NOT comparable to unconstrained U.
            return self.type_params_are_comparable(source, target);
        }

        // Resolve type parameter to apparent type (constraint or `unknown`), and a
        // deferred conditional to its default constraint. tsc's `getApparentType`
        // resolves instantiable types (type parameters and conditionals alike)
        // before the comparable relation runs, so `Exclude<T, U>` overlaps with a
        // literal exactly when its branch constraint does. Instantiable indexed
        // accesses are then reduced through the comparability-only reducer so
        // `Parameters<F>["length"]` can overlap numeric literals without leaking
        // that reduction into assignment diagnostics or constraint validation.
        let source_apparent = self.comparable_apparent_type(source, source_is_tp);
        let target_apparent = self.comparable_apparent_type(target, target_is_tp);

        let skip_signature_only_fast_path =
            self.are_pure_signature_objects(source_apparent, target_apparent);

        // Fast path: direct bidirectional assignability (with apparent types).
        // Skip this for pure call/construct signature objects because TS overlap
        // checks are stricter than general object assignability there.
        if !skip_signature_only_fast_path
            && (self
                .type_comparability_relation_outcome(source_apparent, target_apparent)
                .related
                || self
                    .type_comparability_relation_outcome(target_apparent, source_apparent)
                    .related)
        {
            return true;
        }

        // An enum operand is comparable to a *non-enum* operand through its
        // member-value union (`switch(c: Color){ case "red": }` is comparable
        // because `"red"` is a member value), while enum-vs-enum stays nominal.
        // Without this, the nominal enum type reaches neither the assignability
        // fast path nor the union decomposition below, so a valid string-enum
        // case wrongly reports TS2678.
        if let Some((src, tgt)) = crate::query_boundaries::enum_analysis::enum_comparison_operands(
            self.ctx.types,
            source_apparent,
            target_apparent,
        ) {
            return self.is_type_comparable_to(src, tgt);
        }

        // TSC's comparable relation decomposes unions and checks if ANY member
        // is related to the other type. This handles cases like:
        // - `User.A | User.B` comparable to `User.A` (User.A member matches)
        // - `string & Brand` comparable to `"a"` (string member of intersection)

        // Decompose source union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, source_apparent) {
            for member in &members {
                if self
                    .type_comparability_relation_outcome(*member, target_apparent)
                    .related
                    || self
                        .type_comparability_relation_outcome(target_apparent, *member)
                        .related
                {
                    return true;
                }
            }
        }

        // Decompose target union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self
                    .type_comparability_relation_outcome(source_apparent, *member)
                    .related
                    || self
                        .type_comparability_relation_outcome(*member, source_apparent)
                        .related
                {
                    return true;
                }
            }
        }

        // Decompose intersection: `"a"` is comparable to `string & Brand` because
        // `"a"` is assignable to `string` (one constituent). tsc's comparable relation
        // treats intersections as comparable if the source overlaps with ANY member.
        if let Some(members) = query::intersection_members(self.ctx.types, source_apparent) {
            for member in &members {
                if self
                    .type_comparability_relation_outcome(*member, target_apparent)
                    .related
                    || self
                        .type_comparability_relation_outcome(target_apparent, *member)
                        .related
                {
                    return true;
                }
            }
        }
        if let Some(members) = query::intersection_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self
                    .type_comparability_relation_outcome(source_apparent, *member)
                    .related
                    || self
                        .type_comparability_relation_outcome(*member, source_apparent)
                        .related
                {
                    return true;
                }
            }
        }

        // Additional check: Two object types where ALL properties are optional always
        // overlap at `{}`, making them comparable even if property types differ.
        // Example: `{ b?: number }` vs `{ b?: string }` are comparable because both
        // include `{}` as a valid value.
        if self.objects_with_all_optional_common_props_overlap(source_apparent, target_apparent) {
            return true;
        }

        if self.constructor_signature_only_objects_overlap(source_apparent, target_apparent) {
            return true;
        }

        // Two object types where at least one has generic call/construct signatures
        // are considered comparable by tsc's Comparable relation. This is because
        // generic signatures can potentially be instantiated to match the concrete
        // type, so tsc treats them as having structural overlap.
        if self.objects_with_generic_signatures_are_comparable(source_apparent, target_apparent) {
            return true;
        }

        // Two object types where function-typed properties have overlapping arity
        // are comparable. For example, `{ fn(a?: Base): void }` and `{ fn(a?: C): void }`
        // are comparable because both functions can be called with 0 args (all optional).
        // tsc's Comparable relation threads through object properties and considers
        // function signatures comparable when their arity ranges overlap.
        if self.objects_with_arity_overlapping_functions_are_comparable(
            source_apparent,
            target_apparent,
        ) {
            return true;
        }

        false
    }

    /// Check if two object types have comparable properties.
    ///
    /// Resolves both types to their concrete shapes and checks if every common
    /// property's type is comparable (assignable in at least one direction).
    /// This implements the property-level threading of tsc's `comparableRelation`,
    /// handling cases where whole-object bidirectional assignability fails but
    /// individual property types overlap.
    pub(crate) fn object_properties_are_comparable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::query_boundaries::assignability::object_shape_for_type;
        use tsz_common::Visibility;

        // Skip when either type involves type parameters. Type parameter
        // constraints overlap structurally with many types, but tsc's
        // comparable relation for generics is stricter than per-property
        // bidirectional assignability.
        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
            || crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
        {
            return false;
        }

        let source_resolved = self.evaluate_type_with_resolution(source);
        let target_resolved = self.evaluate_type_with_resolution(target);

        // Tuples are already handled element-wise by the solver's
        // `types_are_comparable_for_assertion` (see flow.rs); the property-bag
        // view here would treat the implicit `length` literal as a shared
        // comparable property and falsely report overlap for casts like
        // `[C, D] as [A, I]` where the elements don't overlap. Defer to the
        // solver's tuple logic.
        let source_is_tuple = common::tuple_elements(self.ctx.types, source_resolved).is_some();
        let target_is_tuple = common::tuple_elements(self.ctx.types, target_resolved).is_some();
        if source_is_tuple && target_is_tuple {
            return false;
        }

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source_resolved) else {
            return false;
        };

        // When target is an intersection, source must overlap with every member.
        if let Some(members) = common::intersection_members(self.ctx.types, target_resolved) {
            return members
                .iter()
                .all(|&member| self.object_properties_are_comparable(source, member));
        }

        let Some(target_shape) = object_shape_for_type(self.ctx.types, target_resolved) else {
            return false;
        };

        // Skip for types with private/protected members. Classes with private
        // properties use nominal checking — the comparable relation requires
        // matching declarations, not just structural overlap.
        let has_non_public = source_shape
            .properties
            .iter()
            .chain(target_shape.properties.iter())
            .any(|p| p.visibility != Visibility::Public);
        if has_non_public {
            return false;
        }

        // Need at least one common property
        let mut found_common = false;

        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                found_common = true;
                // Property types must be comparable (assignable in at least one direction)
                let prop_comparable = self
                    .type_comparability_relation_outcome(source_prop.type_id, target_prop.type_id)
                    .related
                    || self
                        .type_comparability_relation_outcome(
                            target_prop.type_id,
                            source_prop.type_id,
                        )
                        .related;
                if !prop_comparable {
                    return false;
                }
            }
        }

        found_common
    }
}
