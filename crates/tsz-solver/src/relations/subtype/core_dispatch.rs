//! Structural subtype dispatch extracted from `core.rs`.
//!
//! This is the body of [`SubtypeChecker::check_subtype_inner_impl`], moved
//! verbatim into a child module so the `subtype/core.rs` engine shard stays
//! under the 2000-line file-size cap (§19). `use super::*` re-exposes the
//! parent module's imports and `SubtypeChecker` so the relocation is
//! behavior-preserving.

use super::*;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Actual structural comparison -- separated so `stacker::maybe_grow` can wrap it.
    pub(crate) fn check_subtype_inner_impl(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeResult {
        // Types are already evaluated in check_subtype, so no need to re-evaluate here

        // Substitution types (tsc `isRelatedTo` handling): a *source* substitution
        // relates through its substitution intersection `base & constraint`; a
        // *target* substitution relates through its base type. Unwrap and re-dispatch.
        if let Some((base, constraint)) =
            crate::type_queries::substitution_components(self.interner, source)
        {
            let intersection = self.interner.intersection2(base, constraint);
            return self.check_subtype(intersection, target);
        }
        if let Some((base, _)) = crate::type_queries::substitution_components(self.interner, target)
        {
            return self.check_subtype(source, base);
        }

        if let Some(inner) = self.readonly_application_or_display_alias_inner(source)
            && array_element_type(self.interner, target).is_none()
            && tuple_list_id(self.interner, target).is_none()
            && self.check_subtype(inner, target).is_true()
        {
            return SubtypeResult::True;
        }

        // Without strictNullChecks, null/undefined are assignable to all types
        // including type parameters. tsc's `isSimpleTypeRelatedTo` gates this
        // purely on `!strictNullChecks`; the `null`-not-assignable-to-`void`
        // asymmetry is a STRICT-mode rule (`t & (Undefined | Void)` for
        // undefined vs `t & Null` for null) and must not leak in here.
        if !self.strict_null_checks && source.is_nullish() {
            return SubtypeResult::True;
        }

        // Fast paths: tsc's `someTypeRelatedToType` (issue #17390) and the
        // `typeof globalThis` surface self-relation (issue #17436); see helpers.
        if self.intersection_or_merged_source_satisfies_target(source, target) {
            return SubtypeResult::True;
        }
        if let Some(result) = self.global_this_surface_relation(source, target) {
            return result;
        }
        // Canonicalization-based structural identity (Task #36) is intentionally
        // NOT a fast path here: it was slower than the QueryCache's O(1)
        // memoization. It stays for recursive-alias isomorphism detection
        // (`are_types_structurally_identical()` / isomorphism_tests.rs).

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        // Property access on inherited `this`-returning methods substitutes `this`
        // with the resolved class instance object. Keep that structural object
        // assignable to the nominal class reference that denotes the same DefId.
        if let Some(source_def_id) = self.non_generic_object_shape_def_id(source) {
            if lazy_def_id(self.interner, target) == Some(source_def_id) {
                return SubtypeResult::True;
            }
            if self.non_generic_object_shape_def_id(target) == Some(source_def_id) {
                return SubtypeResult::True;
            }
        }

        // Primitive-to-boxed-wrapper assignability: `string -> String`, `number -> Number`, etc.
        // Must run BEFORE apparent_primitive_shape_for_type which would do a structural
        // comparison that fails (the apparent shape of `string` doesn't structurally match `String`).
        if let Some(s_kind) = intrinsic_kind(self.interner, source)
            && let Some(kind) = boxable_intrinsic_kind(s_kind)
            && union_list_id(self.interner, target).is_none()
            && self.is_target_boxed_type(target, kind)
        {
            return SubtypeResult::True;
        }

        // Also handle string/number/boolean literals -> boxed wrapper
        if let Some(lit) = literal_value(self.interner, source) {
            let kind = match lit {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
            };
            if let Some(kind) = kind
                && union_list_id(self.interner, target).is_none()
                && self.is_target_boxed_type(target, kind)
            {
                return SubtypeResult::True;
            }
        }

        if let Some(shape) = self.apparent_primitive_shape_for_type(source) {
            // A deferred mapped target may be structurally a pure index-signature
            // object — e.g. `{ [P in any]: V }` (the shape of `Record<any, V>` /
            // `Record<PropertyKey, V>`) is equivalent to
            // `{ [k: string]: V; [k: number]: V }`. Such a target is never
            // expanded eagerly during evaluation (to keep error-message display
            // stable), so `object_with_index_shape_id` reports `None` for it and
            // the relation would otherwise fall through to the boxed-wrapper
            // fallback below, wrongly accepting a primitive against a pure index
            // signature (a primitive has no index signature; tsc rejects it).
            // Expand it here so the pure-index guard in the
            // `object_with_index_shape_id` arm owns the decision, exactly as it
            // does for the written-out `{ [k: string]: V }` form.
            let target = self.expand_mapped_target_for_shape(target);
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                // Reset `in_intersection_member_check` for apparent primitive structural
                // comparison. When called from within a target intersection member loop,
                // the flag suppresses weak type checks. But the apparent-primitive
                // comparison is a fresh structural query — the String wrapper shape
                // should NOT bypass weak type detection when checked against weak types.
                let saved_inter_check = self.in_intersection_member_check;
                self.in_intersection_member_check = false;
                let result =
                    self.check_object_subtype(&shape, None, Some(source), &t_shape, Some(target));
                self.in_intersection_member_check = saved_inter_check;
                if result.is_true() {
                    return result;
                }
                // Fallback: the hardcoded apparent shape may lack user-augmented members
                // (e.g., `interface Number extends ICloneable { }`), or missing iterable
                // interfaces (e.g., string <: Iterable<string>). Check the registered
                // boxed type which includes merged heritage from global augmentations.
                // Use apparent_primitive_kind to also handle literals (e.g., "test" <: Iterable<string>).
                if let Some(kind) = self.apparent_primitive_kind(source)
                    && union_list_id(self.interner, target).is_none()
                    && self.is_boxed_primitive_subtype(kind, target)
                {
                    return SubtypeResult::True;
                }
                return result;
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                let source_kind = self.apparent_primitive_kind(source);
                let has_string_index = t_shape.string_index.is_some();
                let has_number_index = t_shape.number_index.is_some();
                let allow_indexed_structural = !has_string_index
                    && (!has_number_index || source_kind == Some(IntrinsicKind::String));
                if !allow_indexed_structural {
                    // Primitives must NOT be assignable to pure index-signature
                    // types (e.g., `string` to `{ [index: string]: any }`), even
                    // though their boxed wrappers would be structurally compatible.
                    // Only allow the boxed fallback when the target has named
                    // properties (a mixed interface, not a pure index type).
                    if !t_shape.properties.is_empty()
                        && let Some(s_kind) = source_kind
                        && self.is_boxed_primitive_subtype(s_kind, target)
                    {
                        return SubtypeResult::True;
                    }
                    return SubtypeResult::False;
                }
                let result = self.check_object_with_index_subtype(
                    &shape,
                    None,
                    Some(source),
                    &t_shape,
                    Some(target),
                );
                if result.is_true() {
                    return result;
                }
                // Boxed fallback is safe here (no properties guard needed):
                // structural matching was already attempted above.
                if let Some(kind) = self.apparent_primitive_kind(source)
                    && self.is_boxed_primitive_subtype(kind, target)
                {
                    return SubtypeResult::True;
                }
                return result;
            }
            // Target is not a plain object/indexed-object (e.g., it's a generic
            // Application like `Iterable<string>`). The hardcoded apparent shape
            // can't match these. Fall back to the registered boxed type which
            // includes all heritage (e.g., String implements Iterable<string>).
            // Guard: skip for `object` type — primitives must NOT be subtypes of
            // `object` even though their boxed wrappers (Number, String, etc.) are.
            if target != TypeId::OBJECT
                && union_list_id(self.interner, target).is_none()
                && let Some(kind) = self.apparent_primitive_kind(source)
                && self.is_boxed_primitive_subtype(kind, target)
            {
                return SubtypeResult::True;
            }
        }

        if let Some(source_cond_id) = conditional_type_id(self.interner, source) {
            if let Some(target_cond_id) = conditional_type_id(self.interner, target) {
                let source_cond = self.interner.get_conditional(source_cond_id);
                let target_cond = self.interner.get_conditional(target_cond_id);
                if self
                    .check_conditional_subtype(&source_cond, &target_cond)
                    .is_true()
                {
                    return SubtypeResult::True;
                }
                // Conditional-to-conditional structural check failed (e.g., different extends types).
                // Fall through to conditional_branches_subtype which uses constraint decomposition
                // and branch-by-branch checking (e.g., A <: One when A's true branch IS One).
            }

            // Before decomposing the conditional into branches, check if the target
            // is a union containing the source by identity. This prevents false negatives
            // where `Cond<T> <: Cond<T> | undefined` fails because branch decomposition
            // cannot prove assignability even though the source IS a member of the target union.
            if let Some(members) = union_list_id(self.interner, target) {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    if source == member {
                        return SubtypeResult::True;
                    }
                    // Check via check_subtype for structural equivalence
                    // (handles cases where same conditional has different TypeIds)
                    if self.check_subtype(source, member).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }

            let source_cond = self.interner.get_conditional(source_cond_id);
            return self.conditional_branches_subtype(&source_cond, target);
        }

        if let Some(target_cond_id) = conditional_type_id(self.interner, target) {
            let target_cond = self.interner.get_conditional(target_cond_id);
            return self.subtype_of_conditional_target(source, &target_cond);
        }

        // Note: Source union/intersection handling is consolidated as follows:
        //
        // 1. Source union: Kept here (not moved to visitor) because it must run BEFORE
        //    the target union check. This order dependency is critical for correct
        //    union-to-union semantics: Union(A,B) <: Union(C,D) means ALL members of
        //    source must be subtypes of the target union (delegating to target union check).
        //
        // 2. Source intersection: Moved to visitor pattern (visit_intersection) which
        //    handles both the "at least one member" check AND the property merging logic
        //    for object targets. This removed ~50 lines of duplicate code.
        //
        // Source union check must run BEFORE target union check to handle union-to-union cases:
        // Union(A, B) <: Union(C, D) means (A <: Union(C, D)) AND (B <: Union(C, D))
        // This is different from the target union check which does: Source <: C OR Source <: D
        if let Some(members) = union_list_id(self.interner, source) {
            let member_list = self.interner.type_list(members);
            for &member in member_list.iter() {
                if !self.check_subtype(member, target).is_true() {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        if let Some(members) = union_list_id(self.interner, target) {
            if keyof_inner_type(self.interner, source).is_some()
                && self.is_keyof_subtype_of_string_number_symbol_union(members)
            {
                return SubtypeResult::True;
            }

            // Rule #7: Open Numeric Enums - number is assignable to unions containing numeric enums
            if source == TypeId::NUMBER {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    let def_id = lazy_def_id(self.interner, member)
                        .or_else(|| enum_components(self.interner, member).map(|(d, _)| d));
                    if let Some(def_id) = def_id
                        && self.resolver.is_numeric_enum(def_id)
                    {
                        return SubtypeResult::True;
                    }
                }
            }

            let member_list = self.interner.type_list(members);

            // Fast path: TypeId equality pre-scan before expensive structural checks.
            // If source has the same TypeId as any union member, it's trivially a subtype.
            // This avoids O(n × cost) structural comparisons when the match is by identity.
            for &member in member_list.iter() {
                if source == member {
                    return SubtypeResult::True;
                }
            }

            for &member in member_list.iter() {
                if let Some(source_elem) = array_element_type(self.interner, source) {
                    let evaluated_member = self.evaluate_type(member);
                    if let Some(target_elem) = array_element_type(self.interner, evaluated_member)
                        && self.check_subtype(source_elem, target_elem).is_true()
                    {
                        return SubtypeResult::True;
                    }
                }
                if self.check_subtype(source, member).is_true() {
                    return SubtypeResult::True;
                }
            }

            // Type parameter constraint check: if source is a type parameter with a constraint,
            // check if its constraint is assignable to the entire target union.
            // e.g., Bottom extends T | U should be assignable to T | U
            if let Some(s_info) = type_param_info(self.interner, source)
                && let Some(constraint) = s_info.constraint
                && self.check_subtype(constraint, target).is_true()
            {
                return SubtypeResult::True;
            }

            // String intrinsic constraint check: if source is a string mapping type
            // (e.g., Uppercase<T>) whose type arg is a type parameter with a constraint,
            // evaluate the intrinsic applied to the constraint and check that result
            // against the whole target union.
            // e.g., Uppercase<T> where T extends 'foo'|'bar' <: 'FOO'|'BAR'
            if let Some((s_kind, s_type_arg)) = string_intrinsic_components(self.interner, source)
                && let Some(param_info) = type_param_info(self.interner, s_type_arg)
                && let Some(constraint) = param_info.constraint
            {
                let intrinsic_of_constraint = self.interner.string_intrinsic(s_kind, constraint);
                let evaluated = self.evaluate_type(intrinsic_of_constraint);
                if evaluated != source && self.check_subtype(evaluated, target).is_true() {
                    return SubtypeResult::True;
                }
            }

            if let Some(projected) = self.constrained_projection_for_template_source(source)
                && projected != source
                && self.check_subtype(projected, target).is_true()
            {
                return SubtypeResult::True;
            }

            // Distributive intersection factoring:
            // S <: (A & S) | (B & S) is equivalent to S <: A | B
            let s_arc;
            let source_members: &[TypeId] =
                if let Some(s_list) = intersection_list_id(self.interner, source) {
                    s_arc = self.interner.type_list(s_list);
                    &s_arc
                } else {
                    std::slice::from_ref(&source)
                };

            let mut factored_members = Vec::with_capacity(member_list.len());
            let mut all_contain_source = true;
            for &member in member_list.iter() {
                let i_arc;
                let i_list: &[TypeId] =
                    if let Some(i_members) = intersection_list_id(self.interner, member) {
                        i_arc = self.interner.type_list(i_members);
                        &i_arc
                    } else {
                        std::slice::from_ref(&member)
                    };

                let mut contains_all = true;
                for &s_m in source_members.iter() {
                    if !i_list.contains(&s_m) {
                        contains_all = false;
                        break;
                    }
                }

                if contains_all {
                    let mut rem = Vec::with_capacity(i_list.len());
                    for &i_m in i_list.iter() {
                        if !source_members.contains(&i_m) {
                            rem.push(i_m);
                        }
                    }
                    factored_members.push(self.interner.intersection(rem));
                } else {
                    all_contain_source = false;
                    break;
                }
            }

            if all_contain_source && !factored_members.is_empty() {
                let factored_target = self.interner.union(factored_members);
                if self.check_subtype(source, factored_target).is_true() {
                    return SubtypeResult::True;
                }
            }

            // Discriminated union check: if the source has discriminant properties
            // that distinguish between target union members, check each discriminant
            // value against the matching target members with a narrowed source.
            // See TypeScript's typeRelatedToDiscriminatedType.
            if self
                .type_related_to_discriminated_tuple_type(source, &member_list)
                .is_true()
            {
                return SubtypeResult::True;
            }

            if self
                .type_related_to_discriminated_type(source, &member_list)
                .is_true()
            {
                return SubtypeResult::True;
            }

            // Intersection source check: if source is an intersection, check if any
            // member is assignable to the target union as a whole.
            // e.g., (A & B) <: C | D if A <: C | D
            if let Some(s_list) = intersection_list_id(self.interner, source) {
                let s_member_list = self.interner.type_list(s_list);
                for &s_member in s_member_list.iter() {
                    if self.check_subtype(s_member, target).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }

            // Enum source decomposition: if source is an enum type, decompose it to
            // its structural member union and check against the target union.
            // e.g., enum Choice { A, B, C } <: Choice.A | Choice.B | Choice.C
            // The per-member enum-to-enum check fails (nominal DefId mismatch between
            // parent enum and member enum), but the structural members (0|1|2) ARE
            // each assignable to one of the target member enums.
            if let Some((_s_def_id, s_members)) = enum_components(self.interner, source)
                && self.check_subtype(s_members, target).is_true()
            {
                return SubtypeResult::True;
            }

            // IndexAccess upper bound check: when source is T[K] (an index access
            // involving type parameters), compute the upper bound and check it
            // against the full target union. For unconstrained T, T[K]'s upper
            // bound is `unknown`, and `unknown <: {} | null | undefined` succeeds.
            if self.check_index_access_source_upper_bound_subtype(source, target) {
                return SubtypeResult::True;
            }

            // Source intersection base-constraint reduction:
            // When source is an intersection containing type parameters (e.g., `T & U`
            // where `T extends A` and `U extends B`), tsc computes the base constraint
            // as `A & B` (set-theoretic intersection of the constraints). If the
            // reduced constraint is assignable to the target union, the original
            // intersection is too.
            //
            // This mirrors tsc's getBaseConstraintOfType for intersections:
            //   getBaseConstraintOfType(T & U) = getIntersectionType([
            //       getBaseConstraintOfType(T),   // = constraint of T
            //       getBaseConstraintOfType(U),   // = constraint of U
            //   ])
            //
            // Example: `T extends string | number | undefined` & `U extends string | null | undefined`
            //   reduced constraint = `(string | number | undefined) & (string | null | undefined)`
            //                      = `string | undefined` (after distribution).
            //   `string | undefined <: string | undefined` → True.
            if let Some(s_list) = intersection_list_id(self.interner, source) {
                let s_member_list = self.interner.type_list(s_list);
                let has_type_params = s_member_list
                    .iter()
                    .any(|&m| type_param_info(self.interner, m).is_some());
                if has_type_params {
                    let constraint_members: Vec<TypeId> = s_member_list
                        .iter()
                        .map(|&m| {
                            if let Some(info) = type_param_info(self.interner, m) {
                                info.constraint.unwrap_or(TypeId::UNKNOWN)
                            } else {
                                m
                            }
                        })
                        .collect();
                    let reduced = self.interner.intersection(constraint_members);
                    if reduced != source && self.check_subtype(reduced, target).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }

            // Recursive array aliases can compare an array source whose element
            // is the alias application against the alias body's union. The
            // direct `source <: array-branch` check may fail before expanding
            // that element alias, so compare the evaluated source element to
            // each array branch's element type.
            if let Some(source_elem) = array_element_type(self.interner, source).or_else(|| {
                crate::type_queries::get_tuple_element_type_union(self.interner, source)
            }) {
                let source_elem_eval = self.evaluate_type(source_elem);
                for &member in member_list.iter() {
                    if let Some(target_elem) = array_element_type(self.interner, member)
                        && (source_elem == target_elem
                            || source_elem_eval == target_elem
                            || (source_elem_eval != source_elem
                                && self.check_subtype(source_elem_eval, target_elem).is_true())
                            || self.recursive_array_alias_element_matches_array_interface(
                                source_elem_eval,
                                target_elem,
                                target,
                            ))
                    {
                        return SubtypeResult::True;
                    }
                }
            }

            return SubtypeResult::False;
        }

        // Source intersection member check: when source is an intersection, check if
        // any individual member is a subtype of the target. This implements the
        // fundamental intersection rule: (A & B) <: T if A <: T or B <: T.
        //
        // This MUST run before type-specific target handlers (mapped types, applications,
        // lazy types) which may return False early, preventing the visitor-based
        // intersection decomposition from running.
        //
        // Example: Readonly<T> & { name: string } <: Readonly<T>
        //   → member Readonly<T> <: target Readonly<T> → True
        //
        // Note: property merging (e.g., { a: string } & { b: number } <: { a: string; b: number })
        // is still handled by the visitor's visit_intersection (reached when no individual
        // member matches and no type-specific handler intercepts).
        //
        // Exception: for direct object-like targets, skip this shortcut so that
        // property merging (visit_intersection) can catch conflicting concrete
        // members. Without this, `T & { a: boolean } <: { a?: string }` would
        // incorrectly pass because `T <: { a?: string }` succeeds under generic
        // weak-type generosity, even though the `{ a: boolean }` member forces a
        // concrete property conflict. tsc reports TS2322 here by property-merging
        // the intersection source before comparing against the object target.
        if let Some(members) = intersection_list_id(self.interner, source) {
            let member_list = self.interner.type_list(members);
            // When target is object-like, skip type-parameter members from the
            // shortcut: a type parameter may "generously" pass a weak-type target
            // via type-parameter relaxation, but concrete members of the same
            // intersection (e.g., `{ a: boolean }` in `T & { a: boolean }`) may
            // have properties that conflict with the target. In that case the
            // intersection as a whole must fail, not silently pass via the
            // generic member. Concrete members keep the shortcut so cases like
            // `{ a: string } & { b: number } <: { a: string }` still pass.
            // When target is not object-like (e.g., type parameter, primitive,
            // union, application), keep all members — those targets don't have
            // the weak-type generosity concern.
            let target_is_object_like = object_shape_id(self.interner, target).is_some()
                || object_with_index_shape_id(self.interner, target).is_some();
            let target_property_conflict = target_is_object_like
                && self.intersection_has_incompatible_target_property(source, target);
            // Branded-primitive targets carry their brand properties on a
            // sibling weak object member (e.g., `string & { kind?: K }`).  When
            // the source is also an intersection that mixes a primitive with a
            // brand object, the source primitive member must NOT shortcut the
            // overall check — letting it through would let `{ kind: 'a' } & string`
            // be assignable to `{ kind: 'b' } & string` because the bare `string`
            // member silently satisfies the weak `{ kind?: K }` member of the
            // target via the boxed-`String` heritage (which has no required
            // properties).  The brand mismatch must instead surface through the
            // property-merging path in `visit_intersection` — see
            // `commonTypeIntersection.ts`.  Detect this by asking whether the
            // target is either a bare weak object OR an intersection that
            // contains a weak object member.
            let object_shape_is_weak = |id: TypeId| -> bool {
                object_shape_id(self.interner, id)
                    .or_else(|| object_with_index_shape_id(self.interner, id))
                    .map(|sid| self.interner.object_shape(sid))
                    .is_some_and(|shape| {
                        !shape.properties.is_empty()
                            && shape.string_index.is_none()
                            && shape.number_index.is_none()
                            && shape.properties.iter().all(|p| p.optional)
                    })
            };
            let target_has_weak_object_member = if target_is_object_like {
                object_shape_is_weak(target)
            } else if let Some(t_members) = intersection_list_id(self.interner, target) {
                self.interner
                    .type_list(t_members)
                    .iter()
                    .any(|&m| object_shape_is_weak(m))
            } else {
                false
            };
            // Reset `in_intersection_member_check` for source member checks.
            // When we reach here from a target intersection loop, the flag is true
            // which suppresses weak type checks (TS2559). But the source member checks
            // are independent subtype queries that need full weak type enforcement.
            // Without this reset, `string <: { opt?: T }` would incorrectly succeed
            // (the apparent primitive shape bypasses the weak type check), allowing
            // intersections like `{ opt: X } & string` to be spuriously assignable to
            // `{ opt: Y } & string` when X and Y are incompatible.
            let saved_intersection_check = self.in_intersection_member_check;
            self.in_intersection_member_check = false;
            for &member in member_list.iter() {
                if target_property_conflict {
                    continue;
                }
                if target_is_object_like && type_param_info(self.interner, member).is_some() {
                    continue;
                }
                // Skip bare-primitive source members when the target carries a
                // weak object brand: sibling source members must carry the
                // brand check via the property-merging path.
                if target_has_weak_object_member
                    && intrinsic_kind(self.interner, member).is_some_and(|kind| {
                        matches!(
                            kind,
                            IntrinsicKind::String
                                | IntrinsicKind::Number
                                | IntrinsicKind::Boolean
                                | IntrinsicKind::Bigint
                                | IntrinsicKind::Symbol,
                        )
                    })
                {
                    continue;
                }
                if self.check_subtype(member, target).is_true() {
                    self.in_intersection_member_check = saved_intersection_check;
                    return SubtypeResult::True;
                }
            }
            self.in_intersection_member_check = saved_intersection_check;
            // No individual member matches; fall through to type-specific handlers
        }

        if let Some(members) = intersection_list_id(self.interner, target) {
            let member_list = self.interner.type_list(members);
            let resolver_generation = self.resolver.resolver_generation();

            // Fast path: check the shared intersection merge cache first to
            // skip the O(N) eligibility scan for repeated constraint checks.
            let cached = self
                .query_db
                .and_then(|db| db.lookup_intersection_merge(target, resolver_generation));
            let merged_target = if let Some(cached_result) = cached {
                cached_result.into_result()
            } else if self.can_use_object_intersection_fast_path(&member_list) {
                self.build_object_intersection_target(target)
            } else {
                // Not eligible; cache the negative result to avoid re-scanning.
                if let Some(db) = self.query_db {
                    db.insert_intersection_merge(target, resolver_generation, None);
                }
                None
            };
            if let Some(merged) = merged_target {
                return self.check_subtype(source, merged);
            }

            // When checking source <: each intersection member, temporarily disable
            // weak type checks (TS2559). Individual intersection members may be weak
            // types (all-optional properties) that the source has no properties in
            // common with. But `A <: A & WeakType` should still pass because the
            // source IS assignable to the combined intersection even though it has
            // no properties in common with the WeakType member alone.
            // The weak type check should only apply to the combined intersection
            // target, not to individual members.
            //
            // Use `in_intersection_member_check` instead of modifying `enforce_weak_types`
            // directly to avoid polluting the subtype cache with results computed under
            // different weak-type-enforcement policies.
            let saved = self.in_intersection_member_check;
            let saved_property_check = self.in_property_check;
            self.in_intersection_member_check = true;

            // tsc judges weak-ness on the WHOLE intersection target, not per
            // member: `isWeakType(A & B)` is true only when *every* member is a
            // weak object type. A member check inside an outer property comparison
            // sets `in_property_check`, which would otherwise force the per-member
            // weak check back on (objects.rs) even though the combined target is
            // not weak — e.g. `Meta & ((...args) => Fn)` is non-weak because the
            // call-signature member is not a weak object. When the whole
            // intersection is not weak, clear `in_property_check` for the member
            // checks so a source that satisfies every constituent — including a
            // function value satisfying an all-optional object constituent — is
            // accepted, matching tsc. When the whole intersection IS weak (every
            // member weak), keep `in_property_check` so
            // `{ c: string } <: { a?: string } & { b?: number }` still fails.
            // The weak-ness prepass only matters when `in_property_check` is set
            // (otherwise the clear is a no-op), so the common path pays nothing.
            if self.in_property_check {
                let intersection_target_is_weak = member_list.iter().all(|&member| {
                    let resolved = self.resolve_lazy_type(member);
                    object_shape_id(self.interner, resolved)
                        .or_else(|| object_with_index_shape_id(self.interner, resolved))
                        .map(|sid| self.interner.object_shape(sid))
                        .is_some_and(|shape| Self::is_weak_type_shape(&shape))
                });
                if !intersection_target_is_weak {
                    self.in_property_check = false;
                }
            }

            let mut all_members_match = true;
            for &member in member_list.iter() {
                if !self.check_subtype(source, member).is_true() {
                    all_members_match = false;
                    break;
                }
            }

            self.in_intersection_member_check = saved;
            self.in_property_check = saved_property_check;
            return if all_members_match {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        if let (Some(s_kind), Some(t_kind)) = (
            intrinsic_kind(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            return self.check_intrinsic_subtype(s_kind, t_kind);
        }

        // Type parameter checks BEFORE boxed primitive check
        // Unconstrained type parameters should be handled before other checks
        if let Some(s_info) = type_param_info(self.interner, source) {
            return self.check_type_parameter_subtype(&s_info, target);
        }

        if let Some(_t_info) = type_param_info(self.interner, target) {
            // Special case: T & SomeType <: T
            // If source is an intersection containing the target type parameter,
            // the intersection is a more specific version (excluding null/undefined)
            // and is assignable. This handles the common pattern: T & {} <: T.
            if let Some(members) = intersection_list_id(self.interner, source) {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    if member == target {
                        return SubtypeResult::True;
                    }
                }
            }

            // Reverse homomorphic mapped type check:
            // { [K in keyof T]: T[K] } (with any readonly/optional modifiers) is
            // assignable to T. This handles Readonly<T> → T, Partial<T> → T, etc.
            // In tsc 6.0, homomorphic mapped types are bidirectionally assignable
            // to their source type parameter.
            if self.check_homomorphic_mapped_source_to_type_param(source, target) {
                return SubtypeResult::True;
            }

            // Variadic tuple identity: [...T] is assignable to T.
            // tsc treats [...T] as structurally equivalent to T when T is a
            // type parameter constrained to an array/tuple type.
            if let Some(s_list) = tuple_list_id(self.interner, source) {
                let s_elems = self.interner.tuple_list(s_list);
                if s_elems.len() == 1 && s_elems[0].rest {
                    let spread_inner = s_elems[0].type_id;
                    // Check if the spread inner type is the same type parameter as target,
                    // or is assignable to target
                    if spread_inner == target || self.check_subtype(spread_inner, target).is_true()
                    {
                        return SubtypeResult::True;
                    }
                }
            }

            // A concrete type is never a subtype of an opaque type parameter.
            // The type parameter T could be instantiated as any type satisfying its constraint,
            // so we cannot guarantee that source <: T unless source is never/any (handled above).
            //
            // This is the correct TypeScript behavior:
            // - "hello" is NOT assignable to T extends string (T could be "world")
            // - { value: number } is NOT assignable to unconstrained T (T defaults to unknown)
            //
            // Note: When the type parameter is the SOURCE (e.g., T <: string), we check
            // against its constraint. But as TARGET, we return False.

            return SubtypeResult::False;
        }

        if let Some(s_kind) = intrinsic_kind(self.interner, source) {
            if self.is_boxed_primitive_subtype(s_kind, target) {
                return SubtypeResult::True;
            }
            // `string` is assignable to a template literal that spans the full
            // string domain (e.g. `` `${string}${string}` ``): any string can
            // be partitioned across the all-`${string}` placeholders. A lone
            // `${string}` already collapses to `string` at construction, so
            // this covers the multi-placeholder case. Mirrors tsc, which treats
            // such a template as mutually assignable with `string`.
            if s_kind == IntrinsicKind::String
                && template_literal_spans_full_string_domain(self.interner, target)
            {
                return SubtypeResult::True;
            }
            // `object` keyword is structurally equivalent to `{}` (empty object).
            // It's assignable to any object type where all properties are optional,
            // since no required properties need to be satisfied.
            //
            // However, `object` is NOT assignable to types with a *required* index
            // signature (e.g., `{ [s: string]: unknown }`). In tsc, `object` lacks an
            // implicit index signature, so assigning it to `Record<string, T>` fails
            // with "Index signature for type 'string' is missing in type '{}'".
            // An *optional* index signature (`Partial<Record<string, T>>`) imposes no
            // requirement, so `object` IS assignable to it — the same relaxation the
            // structural index-relation applies for a property-less source.
            // Note: `{}` IS assignable to required indexed types too (handled
            // elsewhere via the inferable-index rule), but the `object` keyword gets
            // stricter treatment in tsc.
            if s_kind == IntrinsicKind::Object {
                // `Record<any, V>` reaches relations as a deferred `Mapped` node
                // (`{ [P in any]: V }`), so the shape lookups below report `None`
                // for it and the keyword would wrongly fall through to `False`.
                // Expand it to its pure-index shape so the any-index waiver fires,
                // mirroring the apparent-primitive path above. See issue #14751.
                let target = self.expand_mapped_target_for_shape(target);
                let target_shape = object_shape_id(self.interner, target)
                    .or_else(|| object_with_index_shape_id(self.interner, target));
                if let Some(t_shape_id) = target_shape {
                    // Extract the index facts before calling `self`-borrowing helpers.
                    let (no_required_props, string_index, number_index) = {
                        let t_shape = self.interner.object_shape(t_shape_id);
                        (
                            t_shape.properties.iter().all(|p| p.optional),
                            t_shape
                                .string_index
                                .as_ref()
                                .map(|si| (t_shape.string_index_is_optional(), si.value_type)),
                            t_shape
                                .number_index
                                .as_ref()
                                .map(|ni| (t_shape.number_index_is_optional(), ni.value_type)),
                        )
                    };
                    // A *required* index signature normally rejects the `object`
                    // keyword (it has no implicit index), but tsc's any-index waiver
                    // (`indexSignaturesRelatedTo`) accepts it when the index value
                    // type is `any` (`{ [k: string]: any }`, `Record<any, any>`). An
                    // optional or absent index imposes no requirement; a concrete
                    // `unknown` value still rejects in both compilers.
                    let string_index_ok = match string_index {
                        None => true,
                        Some((optional, value_type)) => {
                            optional
                                || self.target_string_index_any_waives_missing_index(value_type)
                        }
                    };
                    let number_index_ok = match number_index {
                        None => true,
                        Some((optional, value_type)) => {
                            optional
                                || self.target_string_index_any_waives_missing_index(value_type)
                        }
                    };
                    if no_required_props && string_index_ok && number_index_ok {
                        return SubtypeResult::True;
                    }
                }
            }
            // An intrinsic source against an `Enum` target is owned by the
            // enum-target rule (`rules/enums.rs`), not this arm's early False.
            if enum_components(self.interner, target).is_some() {
                return self.check_non_enum_source_to_enum_target(source, target);
            }
            // When target is an unevaluated IndexAccess (e.g., Obj[K] where K is a
            // type parameter), don't return False early. The IndexAccess fallback
            // (check_generic_index_access_subtype) after the visitor dispatch can
            // resolve the access by distributing over K's constraint literals.
            if index_access_parts(self.interner, target).is_none() {
                return SubtypeResult::False;
            }
        }

        if let (Some(lit), Some(t_kind)) = (
            literal_value(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            return self.check_literal_to_intrinsic(&lit, t_kind);
        }

        if let (Some(s_lit), Some(t_lit)) = (
            literal_value(self.interner, source),
            literal_value(self.interner, target),
        ) {
            if s_lit == t_lit {
                return SubtypeResult::True;
            }
            return SubtypeResult::False;
        }

        if let (Some(LiteralValue::String(s_lit)), Some(t_spans)) = (
            literal_value(self.interner, source),
            template_literal_id(self.interner, target),
        ) {
            return self.check_literal_matches_template_literal(s_lit, t_spans);
        }

        if intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Object) {
            if self.is_object_keyword_type(source) {
                return SubtypeResult::True;
            }
            return SubtypeResult::False;
        }

        // Check if target is the Function intrinsic (TypeId::FUNCTION) or the
        // Function interface from lib.d.ts. We check three ways:
        // 1. Target is the Function intrinsic (TypeId::FUNCTION)
        // 2. Target matches the registered boxed Function TypeId
        // 3. Target was resolved from a Lazy(DefId) whose DefId is a known Function DefId
        //    (handles the case where get_type_of_symbol and resolve_lib_type_by_name
        //    produce different TypeIds for the same Function interface)
        let is_function_structural = self.is_function_interface_structural(target);
        let is_function_target = intrinsic_kind(self.interner, target)
            == Some(IntrinsicKind::Function)
            || crate::type_queries::is_global_interface_by_identity_with_resolver(
                self.interner,
                self.resolver,
                target,
                IntrinsicKind::Function,
            )
            || is_function_structural;
        if is_function_target {
            // A function's apparent type carries no index signature, so a target
            // that matches the `Function` surface but ALSO declares one (a user
            // `interface Function { [n: number]: T }` / `{ [x: string]: Object }`
            // augmentation) is not satisfied by a bare function. `function_target_
            // has_unwaived_index` resolves the augmentation even when `target` is
            // the intrinsic/boxed/`Lazy` global `Function` reference (the spelling
            // `CallableFunction`/`NewableFunction extends Function` hands it),
            // which #16473's structural-only check missed (#16525).
            let indexed_target = self.function_target_has_unwaived_index(target);
            if self.is_callable_type(source) && !indexed_target {
                return SubtypeResult::True;
            }
            // Only a non-callable *object* source (e.g. `class Foo extends
            // Function {}`, or a source declaring its own index) may still relate,
            // via the structural object-to-object comparison below. A bare
            // function/callable against an `indexed_target` is rejected here rather
            // than falling through: the structural arms key off `target`'s
            // unresolved shape and would miss an intrinsic/boxed augmentation.
            let source_is_object = object_shape_id(self.interner, source).is_some()
                || object_with_index_shape_id(self.interner, source).is_some();
            if !(source_is_object && (is_function_structural || indexed_target)) {
                return SubtypeResult::False;
            }
            // else: fall through to structural object-to-object comparison below.
        }

        // Check if target is the global `Object` interface from lib.d.ts.
        // This is a separate path from intrinsic `object`:
        // - `object` (lowercase) includes callable values.
        // - `Object` (capitalized interface) should follow TS structural rules and
        //   exclude bare callable types from primitive-style object assignability.
        let is_global_object_target =
            crate::type_queries::is_global_interface_by_identity_with_resolver(
                self.interner,
                self.resolver,
                target,
                IntrinsicKind::Object,
            );
        if is_global_object_target {
            let source_eval = self.evaluate_type(source);
            if self.is_global_object_interface_type(source_eval) {
                return SubtypeResult::True;
            }

            return SubtypeResult::False;
        }

        if let (Some(s_elem), Some(t_elem)) = (
            array_element_type(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            return self.check_subtype(s_elem, t_elem);
        }

        if let (Some(s_elems), Some(t_elems)) = (
            tuple_list_id(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            // OPTIMIZATION: Unit-tuple disjointness fast-path (O(1) cached lookup)
            // Two different identity-comparable tuples are guaranteed disjoint.
            // Since we already checked source == target at the top and returned True,
            // reaching here means source != target. If both are identity-comparable, they're disjoint.
            // This avoids O(N) structural recursion for each comparison in BCT's O(N²) loop.
            if self.interner.is_identity_comparable_type(source)
                && self.interner.is_identity_comparable_type(target)
            {
                return SubtypeResult::False;
            }
            let s_elems = self.interner.tuple_list(s_elems);
            let t_elems = self.interner.tuple_list(t_elems);
            return self.check_tuple_subtype(&s_elems, &t_elems);
        }

        if let (Some(s_elems), Some(t_elem)) = (
            tuple_list_id(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            return self.check_tuple_to_array_subtype(s_elems, t_elem);
        }

        if let (Some(s_elem), Some(t_elems)) = (
            array_element_type(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            let t_elems = self.interner.tuple_list(t_elems);
            return self.check_array_to_tuple_subtype(s_elem, &t_elems);
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);

            // Symbol-level cycle detection for recursive interface/class types.
            // When both objects have symbols, check if we're already comparing objects
            // with the same symbol pair. This catches cycles where type evaluation loses
            // DefId identity (e.g., Promise<never> evaluates to Object without DefId, but
            // its `then` method returns Promise<TResult> which produces another Object with
            // the same Promise symbol after instantiation/evaluation).
            //
            // Handles both same-symbol (Opt<X> vs Opt<Y>) and different-symbol
            // (Promise<X> vs PromiseLike<Y>) comparisons. Same-symbol cycles arise from
            // recursive generic types where structural expansion produces fresh TypeIds
            // that evade TypeId-based cycle detection.
            if let (Some(s_sym), Some(t_sym)) = (s_shape.symbol, t_shape.symbol) {
                let sym_pair = (s_sym, t_sym);
                if self.sym_visiting.contains(&(t_sym, s_sym)) {
                    return self.cycle_result();
                }
                if !self.sym_visiting.insert(sym_pair) {
                    // Already visiting this symbol pair — coinductive cycle
                    return self.cycle_result();
                }
                let result = self.check_object_subtype(
                    &s_shape,
                    Some(s_shape_id),
                    Some(source),
                    &t_shape,
                    Some(target),
                );
                self.sym_visiting.remove(&sym_pair);
                return result;
            }

            return self.check_object_subtype(
                &s_shape,
                Some(s_shape_id),
                Some(source),
                &t_shape,
                Some(target),
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);

            // Symbol-level cycle detection for ObjectWithIndex types (class instances).
            // Class instance types are interned as ObjectWithIndex with a symbol. Without
            // this check, recursive generic classes (e.g., `Opt<Vector<T>>` vs `Opt<Seq<T>>`)
            // cause infinite structural expansion: the subtype checker keeps expanding members
            // that produce new TypeIds, so TypeId-based cycle detection never fires.
            //
            // This handles BOTH same-symbol (Opt vs Opt with different type args) and
            // different-symbol (Vector vs Seq) comparisons. For same-symbol cases like
            // `Opt<X>` vs `Opt<Y>`, structural expansion of members can lead right back
            // to comparing `Opt<X'>` vs `Opt<Y'>`, creating infinite expansion.
            if let (Some(s_sym), Some(t_sym)) = (s_shape.symbol, t_shape.symbol) {
                let sym_pair = (s_sym, t_sym);
                if self.sym_visiting.contains(&(t_sym, s_sym)) {
                    return self.cycle_result();
                }
                if !self.sym_visiting.insert(sym_pair) {
                    return self.cycle_result();
                }
                let result = self.check_object_with_index_subtype(
                    &s_shape,
                    Some(s_shape_id),
                    Some(source),
                    &t_shape,
                    Some(target),
                );
                self.sym_visiting.remove(&sym_pair);
                return result;
            }

            return self.check_object_with_index_subtype(
                &s_shape,
                Some(s_shape_id),
                Some(source),
                &t_shape,
                Some(target),
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_to_object(
                &s_shape,
                s_shape_id,
                Some(source),
                &t_shape.properties,
                Some(target),
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_to_indexed(
                &s_shape.properties,
                Some(s_shape_id),
                Some(source),
                &t_shape,
                Some(target),
            );
        }

        // Object-like source vs array / readonly-array target. An interface or
        // class that declares `extends Array<T>` (e.g. `interface NonEmptyArray<A>
        // extends Array<A>`) or `extends ReadonlyArray<T>` is a heritage-flattened
        // object shape, not a `TypeData::Array`, so none of the structural object
        // branches above fire and the pair would otherwise fall through to failure.
        //
        // `tsc` treats array assignability covariantly, so `S <: U[]` /
        // `S <: readonly U[]` reduces to the element relation on the numeric
        // element type. We therefore:
        //   1. recognize both the mutable (`U[]`) and the readonly
        //      (`readonly U[]` / `ReadonlyArray<U>`, in either the
        //      `ReadonlyType(Array)` syntax form or the generic-application form)
        //      target shapes and extract the target element type;
        //   2. gate readonly direction. A `readonly` source numeric index (a
        //      `ReadonlyArray`-derived shape) is assignable to a readonly target
        //      but NOT to a mutable one, while a mutable source is assignable to
        //      either; this matches `tsc`;
        //   3. confirm the source genuinely provides the array member surface by
        //      name. The scan walks the registered mutable `Array<T>` base (always
        //      available) and the source's own named properties directly rather
        //      than going through `lookup_property`, distinguishing a
        //      heritage-flattened shape from a bare `Record<number, T>` and
        //      preventing a `[key: string]` index from spoofing every member name.
        //      For a readonly target the mutable-only members (`push`/`pop`/...)
        //      are not required: `ReadonlyArray<T>`'s surface is exactly `Array<T>`
        //      minus those mutating methods, so a `ReadonlyArray`-derived source
        //      legitimately omits them; and
        //   4. decide the relation by the covariant element check.
        //
        // This deliberately avoids materializing the instantiated array interface
        // and running a full structural comparison: doing that in this hot
        // dispatch path (a) re-enters instantiation/evaluation with observable
        // cache side effects and (b) can exhaust the relation depth limit on deep
        // generic sources, which is treated as `true` and silently over-accepts
        // unrelated types. The readonly-target arm runs *before* the readonly-peel
        // below (which strips the target to a mutable array and would otherwise
        // reject a legitimately readonly source). The cheap source discriminator
        // (`object_with_index_shape_id`) is checked first so the readonly-array
        // target extraction only runs for object-with-index sources.
        if let Some(s_shape_id) = object_with_index_shape_id(self.interner, source)
            && let Some(array_base) = self.resolver.get_array_base_type()
            && let Some(array_shape_id) = object_shape_id(self.interner, array_base)
                .or_else(|| object_with_index_shape_id(self.interner, array_base))
            && let Some((t_elem, target_readonly)) = array_element_type(self.interner, target)
                .map(|elem| (elem, false))
                .or_else(|| {
                    self.readonly_array_syntax_element(target)
                        .or_else(|| self.readonly_array_application_element(target))
                        .map(|elem| (elem, true))
                })
        {
            let s_shape = self.interner.object_shape(s_shape_id);
            // A `readonly` source numeric index (a `ReadonlyArray`-derived shape)
            // is assignable to a readonly target but NOT to a mutable one; a
            // mutable source is assignable to either.
            if let Some(num_idx) = s_shape.number_index
                && (target_readonly || !num_idx.readonly)
            {
                let array_shape = self.interner.object_shape(array_shape_id);
                let source_has_full_array_surface = array_shape.properties.iter().all(|member| {
                    member.optional
                        || s_shape.properties.iter().any(|p| p.name == member.name)
                        || (target_readonly
                            && crate::operations::property::property_helpers::is_array_mutating_method(
                                self.interner.resolve_atom_ref(member.name).as_ref(),
                            ))
                });
                if source_has_full_array_surface {
                    return self.check_subtype(num_idx.value_type, t_elem);
                }
            }
        }

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.check_function_subtype(&s_fn, &t_fn);
        }

        // Compatibility bridge: function-like values are assignable to interfaces
        // that only require Function members like `call`/`apply`.
        // This aligns with tsc behavior for:
        //   interface Callable { call(blah: any): any }
        //   const x: Callable = () => {}
        let source_function_like = function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some_and(|sid| {
                let shape = self.interner.callable_shape(sid);
                !shape.call_signatures.is_empty()
            })
            || source == TypeId::FUNCTION;
        if source_function_like {
            if let Some(t_callable_id) = callable_shape_id(self.interner, target) {
                let t_shape = self.interner.callable_shape(t_callable_id);
                // tsc: a function value provides no numeric index signature, so a
                // numeric index on the target is unsatisfiable no matter which
                // members the target also requires. This precedes the
                // `call`/`apply` bridge below, which models the apparent-type
                // members a function DOES provide and must not be read as a
                // blanket "function fits this shape" answer.
                if t_shape.number_index.is_some() {
                    return SubtypeResult::False;
                }
                if t_shape.call_signatures.is_empty() && t_shape.construct_signatures.is_empty() {
                    let required_props: Vec<_> =
                        t_shape.properties.iter().filter(|p| !p.optional).collect();
                    if required_props.len() == 1 {
                        let name = self.interner.resolve_atom(required_props[0].name);
                        if name == "call" || name == "apply" {
                            return SubtypeResult::True;
                        }
                    }
                }
            }
            if let Some(t_shape_id) = object_shape_id(self.interner, target)
                .or_else(|| object_with_index_shape_id(self.interner, target))
            {
                let t_shape = self.interner.object_shape(t_shape_id);
                // Same rule as the callable branch above: the numeric-index
                // verdict does not depend on the target's property list, so it
                // is decided before the `call`/`apply` bridge rather than only
                // when the target requires nothing else. Gating it on
                // `required_props.is_empty()` let `{ apply(..): any; [n: number]: T }`
                // — the shape a user augmentation gives the global `Function`
                // interface — take the bridge and answer assignable.
                //
                // The one exception is the exemption `check_number_index_compatibility`
                // already encodes for object sources: an `any`-valued number index is
                // waived when the target carries a co-present `any`-valued string
                // index, because `tsc`'s `indexSignaturesRelatedTo` short-circuits
                // *every* index info of such a target. So
                // `{ [k: string]: any; [n: number]: any }` accepts a function source
                // even though `{ [n: number]: any }` alone rejects it. Delegating to
                // the shared helper keeps the two paths from drifting.
                if t_shape.number_index.is_some()
                    && !self.target_dual_any_index_waives_missing_number_index(&t_shape)
                {
                    return SubtypeResult::False;
                }
                let required_props: Vec<_> =
                    t_shape.properties.iter().filter(|p| !p.optional).collect();
                if required_props.len() == 1 {
                    let name = self.interner.resolve_atom(required_props[0].name);
                    if name == "call" || name == "apply" {
                        return SubtypeResult::True;
                    }
                }
            }
        }

        if let (Some(s_callable_id), Some(t_callable_id)) = (
            callable_shape_id(self.interner, source),
            callable_shape_id(self.interner, target),
        ) {
            let s_callable = self.interner.callable_shape(s_callable_id);
            let t_callable = self.interner.callable_shape(t_callable_id);
            return self.check_callable_subtype(&s_callable, &t_callable);
        }

        if let (Some(s_fn_id), Some(t_callable_id)) = (
            function_shape_id(self.interner, source),
            callable_shape_id(self.interner, target),
        ) {
            return self.check_function_to_callable_subtype(s_fn_id, t_callable_id);
        }

        if let (Some(s_callable_id), Some(t_fn_id)) = (
            callable_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            return self.check_callable_to_function_subtype(s_callable_id, t_fn_id);
        }

        if function_shape_id(self.interner, source).is_some()
            && matches!(
                self.interner.lookup(target),
                Some(TypeData::Application(_) | TypeData::Lazy(_))
            )
        {
            let evaluated_target = self.evaluate_type_or_raw_fallback(target);
            if evaluated_target != target {
                if let (Some(s_fn_id), Some(t_fn_id)) = (
                    function_shape_id(self.interner, source),
                    function_shape_id(self.interner, evaluated_target),
                ) {
                    let s_fn = self.interner.function_shape(s_fn_id);
                    let t_fn = self.interner.function_shape(t_fn_id);
                    return self.check_function_subtype(&s_fn, &t_fn);
                }
                if let (Some(s_fn_id), Some(t_callable_id)) = (
                    function_shape_id(self.interner, source),
                    callable_shape_id(self.interner, evaluated_target),
                ) {
                    return self.check_function_to_callable_subtype(s_fn_id, t_callable_id);
                }
            }
        }

        if matches!(
            self.interner.lookup(source),
            Some(TypeData::Application(_) | TypeData::Lazy(_))
        ) && function_shape_id(self.interner, target).is_some()
        {
            let evaluated_source = self.evaluate_type_or_raw_fallback(source);
            if evaluated_source != source {
                if let (Some(s_fn_id), Some(t_fn_id)) = (
                    function_shape_id(self.interner, evaluated_source),
                    function_shape_id(self.interner, target),
                ) {
                    let s_fn = self.interner.function_shape(s_fn_id);
                    let t_fn = self.interner.function_shape(t_fn_id);
                    return self.check_function_subtype(&s_fn, &t_fn);
                }
                if let (Some(s_callable_id), Some(t_fn_id)) = (
                    callable_shape_id(self.interner, evaluated_source),
                    function_shape_id(self.interner, target),
                ) {
                    return self.check_callable_to_function_subtype(s_callable_id, t_fn_id);
                }
            }
        }

        if let (Some(s_app_id), Some(t_app_id)) = (
            application_id(self.interner, source),
            application_id(self.interner, target),
        ) {
            return self
                .check_application_to_application_subtype(source, target, s_app_id, t_app_id);
        }

        // When both source and target are applications, try mapped-to-mapped
        // comparison before falling through to one-sided expansion. This handles
        // cases like Readonly<T> <: Partial<T> where both resolve to mapped types
        // over a generic type parameter that can't be concretely expanded.
        if let (Some(s_app_id), Some(t_app_id)) = (
            application_id(self.interner, source),
            application_id(self.interner, target),
        ) {
            let result = self.check_application_to_application(source, target, s_app_id, t_app_id);
            if result != SubtypeResult::False {
                return result;
            }
            // Fall through to one-sided expansion
        }

        // Application(base=DefId(X), args) <: Lazy(DefId(X)):
        // When source is an instantiation of a generic type and target is a bare
        // reference to the same type (unresolved Lazy), this is an instantiation
        // being compared to its base. In TypeScript, a bare generic reference like
        // `Uint8Array` is implicitly instantiated with default type args (e.g.,
        // `Uint8Array<ArrayBuffer>`). When the resolver can't yet resolve the
        // target definition (lazy initialization), both resolve_lazy and
        // get_lazy_type_params return None. Since the Application shares the same
        // base DefId as the target Lazy, it's an instantiation of the same type,
        // and is assignable to its unresolved base.
        if let Some(s_app_id) = application_id(self.interner, source)
            && let Some(target_def_id) = lazy_def_id(self.interner, target)
        {
            let s_app = self.interner.type_application(s_app_id);
            if let Some(base_def_id) = lazy_def_id(self.interner, s_app.base)
                && base_def_id == target_def_id
            {
                // Try arity normalization: create a zero-arg Application for the
                // target and let check_application_to_application_subtype fill in
                // default type parameters for a precise comparison.
                let t_type_id = self.interner.application(s_app.base, vec![]);
                if let Some(t_app_id) = application_id(self.interner, t_type_id) {
                    let result = self.check_application_to_application_subtype(
                        source, t_type_id, s_app_id, t_app_id,
                    );
                    if result.is_true() {
                        return result;
                    }
                }

                // When the resolver can't resolve the definition yet (lazy init),
                // the Application is an instantiation of the exact same type as the
                // unresolved Lazy target. Return True to avoid false positives.
                if self
                    .resolver
                    .resolve_lazy(target_def_id, self.interner)
                    .is_none()
                {
                    return SubtypeResult::True;
                }
            }
        }

        if let Some(app_id) = application_id(self.interner, source) {
            return self.check_application_expansion_target(source, target, app_id);
        }

        if let Some(app_id) = application_id(self.interner, target) {
            return self.check_source_to_application_expansion(source, target, app_id);
        }

        // Check mapped-to-mapped structural comparison (for raw mapped types).
        if let (Some(source_mapped_id), Some(target_mapped_id)) = (
            mapped_type_id(self.interner, source),
            mapped_type_id(self.interner, target),
        ) {
            let result =
                self.check_mapped_to_mapped(source, target, source_mapped_id, target_mapped_id);
            if result != SubtypeResult::False {
                return result;
            }
        }

        if let Some(mapped_id) = mapped_type_id(self.interner, source) {
            return self.check_mapped_expansion_target(source, target, mapped_id);
        }

        if let Some(mapped_id) = mapped_type_id(self.interner, target) {
            return self.check_source_to_mapped_expansion(source, target, mapped_id);
        }

        // Enum relations (nominal identity + structural member values) live in
        // `rules/enums.rs`; `None` means neither side is an enum.
        if let Some(result) = self.check_enum_relations(source, target) {
            return result;
        }

        // =======================================================================
        // PHASE 3.2: PRIORITIZE DefId (Lazy) OVER SymbolRef (Ref)
        // =======================================================================
        // We now check Lazy(DefId) types before Ref(SymbolRef) types to establish
        // DefId as the primary type identity system. The InheritanceGraph bridge
        // enables Lazy types to use O(1) nominal subtype checking.
        // =======================================================================

        if let (Some(s_def), Some(t_def)) = (
            lazy_def_id(self.interner, source),
            lazy_def_id(self.interner, target),
        ) {
            // Use DefId-level cycle detection (checked before Ref types)
            return self.check_lazy_lazy_subtype(source, target, s_def, t_def);
        }

        // =======================================================================
        // Rule #7: Open Numeric Enums - Number <-> Numeric Enum Assignability
        // =======================================================================
        // In TypeScript, numeric enums are "open" - they allow bidirectional
        // assignability with the number type. This is unsound but matches tsc behavior.
        // See docs/specs/TS_UNSOUNDNESS_CATALOG.md Item #7.

        // Helper to extract DefId from Enum or Lazy types
        let get_enum_def_id = |type_id: TypeId| -> Option<DefId> {
            match self.interner.lookup(type_id) {
                Some(TypeData::Enum(def_id, _)) | Some(TypeData::Lazy(def_id)) => Some(def_id),
                _ => None,
            }
        };

        // Check: source is numeric enum, target is Number
        if let Some(s_def) = get_enum_def_id(source)
            && target == TypeId::NUMBER
            && self.resolver.is_numeric_enum(s_def)
        {
            return SubtypeResult::True;
        }

        // Check: source is Number (or numeric literal), target is numeric enum
        if let Some(t_def) = get_enum_def_id(target) {
            if source == TypeId::NUMBER && self.resolver.is_numeric_enum(t_def) {
                return SubtypeResult::True;
            }
            // Also check for numeric literals (subtypes of number)
            if matches!(
                self.interner.lookup(source),
                Some(TypeData::Literal(LiteralValue::Number(_)))
            ) && self.resolver.is_numeric_enum(t_def)
            {
                // For numeric literals, we need to check if they're assignable to the enum
                // Fall through to structural check (e.g., 0 -> E.A might succeed if E.A = 0)
                return self.check_subtype(source, self.resolve_lazy_type(target));
            }
        }

        if lazy_def_id(self.interner, source).is_some() {
            let resolved = self.resolve_lazy_type(source);
            return if resolved != source {
                self.check_subtype(resolved, target)
            } else {
                SubtypeResult::False
            };
        }

        if lazy_def_id(self.interner, target).is_some() {
            let resolved = self.resolve_lazy_type(target);
            return if resolved != target {
                self.check_subtype(source, resolved)
            } else {
                SubtypeResult::False
            };
        }

        if let (Some(s_sym), Some(t_sym)) = (
            type_query_symbol(self.interner, source),
            type_query_symbol(self.interner, target),
        ) {
            return self.check_typequery_typequery_subtype(source, target, s_sym, t_sym);
        }

        if let Some(s_sym) = type_query_symbol(self.interner, source) {
            return self.check_typequery_subtype(source, target, s_sym);
        }

        if let Some(t_sym) = type_query_symbol(self.interner, target) {
            return self.check_to_typequery_subtype(source, target, t_sym);
        }

        if let (Some(s_inner), Some(t_inner)) = (
            keyof_inner_type(self.interner, source),
            keyof_inner_type(self.interner, target),
        ) {
            return self.check_subtype(t_inner, s_inner);
        }

        if let (Some(s_inner), Some(t_inner)) = (
            readonly_inner_type(self.interner, source),
            readonly_inner_type(self.interner, target),
        ) {
            return self.check_subtype(s_inner, t_inner);
        }

        // Lib lowering can preserve `ReadonlyArray<T>` as a generic application,
        // while syntax lowering represents `readonly T[]` as ReadonlyType(Array<T>).
        // They are the same readonly-array surface and should compare by element.
        if let (Some(s_elem), Some(t_elem)) = (
            self.readonly_array_application_element(source),
            self.readonly_array_syntax_element(target),
        ) {
            return self.check_subtype(s_elem, t_elem);
        }

        if let (Some(s_elem), Some(t_elem)) = (
            self.readonly_array_syntax_element(source),
            self.readonly_array_application_element(target),
        ) {
            return self.check_subtype(s_elem, t_elem);
        }

        // Readonly target peeling: T <: Readonly<U> if T <: U
        // A mutable type can always be treated as readonly (readonly is a supertype)
        // CRITICAL: Only peel if source is NOT Readonly. If source IS Readonly, we must
        // fall through to the visitor to compare Readonly<S> vs Readonly<T>.
        if let Some(t_inner) = readonly_inner_type(self.interner, target)
            && readonly_inner_type(self.interner, source).is_none()
        {
            return self.check_subtype(source, t_inner);
        }

        // Readonly source to mutable target case is handled by SubtypeVisitor::visit_readonly_type
        // which returns False (correctly, because Readonly is not assignable to Mutable)

        if let (Some(s_sym), Some(t_sym)) = (
            unique_symbol_ref(self.interner, source),
            unique_symbol_ref(self.interner, target),
        ) {
            return if s_sym == t_sym {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        if unique_symbol_ref(self.interner, source).is_some()
            && intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Symbol)
        {
            return SubtypeResult::True;
        }

        if is_this_type(self.interner, source) && is_this_type(self.interner, target) {
            return SubtypeResult::True;
        }

        if is_this_type(self.interner, target) {
            if let Some(concrete_this) = self.resolver.resolve_this_type(self.interner)
                && concrete_this != target
            {
                return self.check_subtype(source, concrete_this);
            }
            return SubtypeResult::False;
        }

        if let (Some(s_spans), Some(t_spans)) = (
            template_literal_id(self.interner, source),
            template_literal_id(self.interner, target),
        ) {
            return self.check_template_assignable_to_template(s_spans, t_spans);
        }

        if template_literal_id(self.interner, source).is_some()
            && intrinsic_kind(self.interner, target) == Some(IntrinsicKind::String)
        {
            return SubtypeResult::True;
        }

        let source_is_callable = function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some();
        if source_is_callable {
            // Build a source ObjectShape from callable properties for structural comparison.
            // IMPORTANT: Sort properties by name (Atom) to match the merge scan's expectation.
            let source_props = if let Some(callable_id) = callable_shape_id(self.interner, source) {
                let callable = self.interner.callable_shape(callable_id);
                let mut props = callable.properties.clone();
                props.sort_by_key(|a| a.name);
                Some(ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties: props,
                    string_index: callable.string_index,
                    number_index: callable.number_index,
                    symbol_index: None,
                    symbol: callable.symbol,
                })
            } else {
                None
            };

            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                }
                // If source is a CallableShape with properties, check structural compatibility
                if let Some(ref s_shape) = source_props {
                    let result = self.check_object_subtype(
                        s_shape,
                        None,
                        Some(source),
                        &t_shape,
                        Some(target),
                    );
                    return self.or_global_function_interface_surface(target, &t_shape, result);
                }
                // A bare `FunctionShape` has no user-declared properties, so model
                // it as an object whose only members are the function's stable
                // apparent properties (`call`/`apply` for a callable, `prototype`
                // for a constructor), mirroring `CompatChecker`'s
                // `function_like_weak_type_properties`. Running the normal
                // `check_object_subtype` then lets the function satisfy an object
                // target whose required properties it covers — in particular an
                // all-optional ("weak") object that shares one of those apparent
                // names, or any optional-only target reached as a non-weak
                // *intersection member* (where `in_intersection_member_check`
                // suppresses the weak rule, e.g. the `{ brand?: number }` member of
                // `(() => void) & { brand?: number }`, which `tsc` accepts).
                //
                // Crucially, because these apparent properties are *required* (not
                // optional), the source is not itself a weak shape, so the weak-type
                // rejection in `check_object_subtype` still fires for a standalone or
                // union-member all-optional target with no common property name —
                // matching `tsc`'s per-member weak rule (e.g. a function is NOT a
                // member of `Fn | Ctor | { pre?; post? }`). A target with a missing
                // *required* property likewise still fails inside
                // `check_object_subtype`.
                let apparent_source = self.function_apparent_object_shape(source);
                let result = self.check_object_subtype(
                    &apparent_source,
                    None,
                    Some(source),
                    &t_shape,
                    Some(target),
                );
                return self.or_global_function_interface_surface(target, &t_shape, result);
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() && t_shape.string_index.is_none() {
                    return SubtypeResult::True;
                }
                // If source is a CallableShape with properties, check structural compatibility
                if let Some(ref s_shape) = source_props {
                    return self.check_object_subtype(
                        s_shape,
                        None,
                        Some(source),
                        &t_shape,
                        Some(target),
                    );
                }
                // A bare `FunctionShape` declares no index signature of its own, so
                // a target that requires one rejects it — with the single exception
                // `tsc` encodes in `indexSignaturesRelatedTo` (checker.ts ~24828):
                // when the target carries a string index whose value type is `any`,
                // every index obligation of that target is waived for a
                // non-primitive source, so `() => void` IS assignable to
                // `{ [k: string]: any }` / `Record<string, any>`. A concrete value
                // type (`unknown`, `string`, …) is never waived, and a target whose
                // only index is numeric is handled by the function-like branch above.
                //
                // The waiver covers only the *index* obligation, so the remaining
                // property and call/construct-signature obligations still have to be
                // adjudicated. Route them through the same apparent-shape structural
                // comparison the property-less object arm above uses: a function's
                // apparent members satisfy `length`/`name`/`bind`-shaped targets, and
                // a required property it does not carry (`{ zzz: string; … }`) still
                // fails inside `check_object_subtype` exactly as `tsc` reports it.
                if let Some(ref t_string_idx) = t_shape.string_index
                    && self.target_string_index_any_waives_missing_index(t_string_idx.value_type)
                {
                    let apparent_source = self.function_apparent_object_shape(source);
                    return self.check_object_subtype(
                        &apparent_source,
                        None,
                        Some(source),
                        &t_shape,
                        Some(target),
                    );
                }
                // FunctionShape has no properties - not assignable to non-empty indexed object
                return SubtypeResult::False;
            }
        }

        let source_is_array_or_tuple = array_element_type(self.interner, source).is_some()
            || tuple_list_id(self.interner, source).is_some();
        if source_is_array_or_tuple {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                }
                // Check if all target properties are satisfiable by the array.
                // First try a quick check for length-only targets.
                let only_length = t_shape
                    .properties
                    .iter()
                    .all(|p| self.interner.resolve_atom(p.name) == "length");
                if only_length {
                    let all_ok = t_shape
                        .properties
                        .iter()
                        .all(|p| self.check_subtype(TypeId::NUMBER, p.type_id).is_true());
                    if all_ok {
                        return SubtypeResult::True;
                    }
                }
                // Check tuple elements against numeric target properties.
                // In tsc, tuples have numeric properties ("0", "1", ...) that are
                // structurally compatible with object types having those properties.
                // e.g., [number] <: { "0": number } is valid.
                if let Some(tuple_id) = tuple_list_id(self.interner, source) {
                    let elements = self.interner.tuple_list(tuple_id);
                    let all_satisfied = t_shape.properties.iter().all(|t_prop| {
                        let name = self.interner.resolve_atom(t_prop.name);
                        if name == "length" {
                            let length_type = self.interner.literal_number(elements.len() as f64);
                            return self.check_subtype(length_type, t_prop.type_id).is_true();
                        }
                        // Check if the property name is a numeric index matching a tuple element
                        if let Ok(idx) = name.parse::<usize>()
                            && let Some(elem) = elements.get(idx)
                        {
                            return self.check_subtype(elem.type_id, t_prop.type_id).is_true();
                        }
                        // Non-numeric property: try the Array interface
                        t_prop.optional
                    });
                    if all_satisfied {
                        return SubtypeResult::True;
                    }
                }
                // Try the Array<T> interface for full structural comparison.
                // This handles cases like: number[] <: { toString(): string }
                // and tuple rest inference against evaluated Array<T> constraints.
                if let Some(elem) = array_element_type(self.interner, source).or_else(|| {
                    crate::type_queries::get_tuple_element_type_union(self.interner, source)
                }) && let Some(result) = self.check_array_interface_subtype(elem, target)
                {
                    return result;
                }
                return SubtypeResult::False;
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    // Arrays/tuples are named types (interfaces) and do not have
                    // implicit string index signatures. They cannot be assigned to
                    // types with a string index signature requirement, e.g.
                    // `number[] <: { [x: string]: unknown }` is false.
                    //
                    // The one exception is an `any`-valued string index
                    // (`{ [x: string]: any }`, or its `{ [P in any]: any }`
                    // mapped form): `tsc` waives the missing-string-index
                    // requirement when the index value type is `any`. This is the
                    // same `any`-propagation (Lawyer) quirk applied to object
                    // sources in `check_string_index_compatibility`, not a
                    // structural Judge invariant — a concrete value type
                    // (`unknown`, `boolean | number`, …) still rejects.
                    if let Some(ref str_idx) = t_shape.string_index
                        && !self.target_string_index_any_waives_missing_index(str_idx.value_type)
                    {
                        return SubtypeResult::False;
                    }
                    if let Some(ref num_idx) = t_shape.number_index {
                        let elem_type =
                            array_element_type(self.interner, source).unwrap_or(TypeId::ANY);
                        if !self.check_subtype(elem_type, num_idx.value_type).is_true() {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::True;
                }
                // Target has non-empty properties + index signature.
                if let Some(result) =
                    self.check_tuple_numeric_props_array_interface_subtype(source, target)
                {
                    return result;
                }
                // Try the Array<T> interface for full structural comparison.
                if let Some(elem) = array_element_type(self.interner, source).or_else(|| {
                    crate::type_queries::get_tuple_element_type_union(self.interner, source)
                }) && let Some(result) = self.check_array_interface_subtype(elem, target)
                {
                    return result;
                }
                if let Some(ref num_idx) = t_shape.number_index
                    && let Some(elem) = array_element_type(self.interner, source).or_else(|| {
                        crate::type_queries::get_tuple_element_type_union(self.interner, source)
                    })
                    && self.array_source_satisfies_minimal_indexed_array_target(
                        elem,
                        num_idx.value_type,
                        &t_shape.properties,
                    )
                {
                    return SubtypeResult::True;
                }
                return SubtypeResult::False;
            }
        }

        if let Some(projected) = self.constrained_projection_for_template_source(source)
            && projected != source
            && self.check_subtype(projected, target).is_true()
        {
            return SubtypeResult::True;
        }

        // =======================================================================
        // VISITOR PATTERN DISPATCH (Task #48.4)
        // =======================================================================
        // After all special-case checks above, dispatch to the visitor for
        // general structural type checking. The visitor implements double-
        // dispatch pattern to handle source type variants and their interaction
        // with the target type.
        // =======================================================================

        // Extract the interner reference FIRST (Copy trait)
        // This must happen before creating the visitor which mutably borrows self
        let interner = self.interner;

        // Create the visitor with a mutable reborrow of self
        let mut visitor = SubtypeVisitor {
            checker: self,
            source,
            target,
        };

        // Dispatch to the visitor using the extracted interner
        let result = visitor.visit_type(interner, source);

        if result == SubtypeResult::False && self.check_generic_index_access_subtype(source, target)
        {
            return SubtypeResult::True;
        }

        // When source is an IndexAccess like T["x"] where T is a constrained type
        // parameter, resolve through T's constraint. For example, T["x"] where
        // T extends { x: number } should resolve to number via the constraint.
        if result == SubtypeResult::False
            && let Some((s_obj, s_idx)) = index_access_parts(self.interner, source)
        {
            // Get the constraint: either from TypeParameter directly or
            // by evaluating the object type and extracting its constraint
            let constraint = if let Some(tp) = type_param_info(self.interner, s_obj) {
                tp.constraint
            } else {
                // Try evaluating in case it's wrapped (e.g., Lazy)
                let evaluated_obj = self.evaluate_type(s_obj);
                type_param_info(self.interner, evaluated_obj).and_then(|tp| tp.constraint)
            };
            if let Some(constraint) = constraint {
                let constraint = self.evaluate_type(constraint);
                let resolved = self.interner.index_access(constraint, s_idx);
                let resolved = self.evaluate_type(resolved);
                if resolved != source
                    && resolved != TypeId::ERROR
                    && resolved != TypeId::NONE
                    && self.check_subtype(resolved, target).is_true()
                {
                    return SubtypeResult::True;
                }
            }
        }

        result
    }
}
