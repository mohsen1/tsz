use super::*;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(super) fn check_subtype_inner_impl(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeResult {
        // Types are already evaluated in check_subtype, so no need to re-evaluate here

        // Without strictNullChecks, null/undefined are assignable to all types
        // including type parameters. Exception: `null` is not assignable to
        // `void` — only `undefined` is.
        if !self.strict_null_checks && source.is_nullish() {
            let null_to_void = source == TypeId::NULL && target == TypeId::VOID;
            if !null_to_void {
                return SubtypeResult::True;
            }
        }

        // Note: Canonicalization-based structural identity (Task #36) was previously
        // called here as a "fast path", but it was actually SLOWER than the normal path
        // because it allocated a fresh Canonicalizer per call (FxHashMap + Vecs) and
        // triggered O(n²) union reduction via interner.union(). The existing QueryCache
        // already provides O(1) memoization for repeated subtype checks.
        // The Canonicalizer remains available for its intended purpose: detecting
        // structural identity of recursive type aliases (graph isomorphism).
        // See: are_types_structurally_identical() and isomorphism_tests.rs

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        // Primitive-to-boxed-wrapper assignability: `string -> String`, `number -> Number`, etc.
        // Must run BEFORE apparent_primitive_shape_for_type which would do a structural
        // comparison that fails (the apparent shape of `string` doesn't structurally match `String`).
        if let Some(s_kind) = intrinsic_kind(self.interner, source)
            && let Some(kind) = boxable_intrinsic_kind(s_kind)
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
                && self.is_target_boxed_type(target, kind)
            {
                return SubtypeResult::True;
            }
        }

        if let Some(shape) = self.apparent_primitive_shape_for_type(source) {
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
                    if let Some(tracer) = &mut self.tracer
                        && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        })
                    {
                        return SubtypeResult::False;
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
                    // Trace: No union member matches target
                    if let Some(tracer) = &mut self.tracer
                        && !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                            source_type: source,
                            target_union_members: vec![target],
                        })
                    {
                        return SubtypeResult::False;
                    }
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

            // Trace: Source is not a subtype of any union member
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                    source_type: source,
                    target_union_members: member_list.iter().copied().collect(),
                })
            {
                return SubtypeResult::False;
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
        if let Some(members) = intersection_list_id(self.interner, source) {
            let member_list = self.interner.type_list(members);
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

            // Keep diagnostic precision when collecting mismatch reasons via tracer.
            if self.tracer.is_none() {
                // Fast path: check the shared intersection merge cache first to
                // skip the O(N) eligibility scan for repeated constraint checks.
                let cached = self
                    .query_db
                    .and_then(|db| db.lookup_intersection_merge(target));
                let merged_target = if let Some(cached_result) = cached {
                    cached_result
                } else if self.can_use_object_intersection_fast_path(&member_list) {
                    self.build_object_intersection_target(target)
                } else {
                    // Not eligible; cache the negative result to avoid re-scanning.
                    if let Some(db) = self.query_db {
                        db.insert_intersection_merge(target, None);
                    }
                    None
                };
                if let Some(merged) = merged_target {
                    return self.check_subtype(source, merged);
                }
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
            self.in_intersection_member_check = true;
            for &member in member_list.iter() {
                if !self.check_subtype(source, member).is_true() {
                    self.in_intersection_member_check = saved;
                    return SubtypeResult::False;
                }
            }
            self.in_intersection_member_check = saved;
            return SubtypeResult::True;
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

            // Trace: Concrete type not assignable to type parameter
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        if let Some(s_kind) = intrinsic_kind(self.interner, source) {
            if self.is_boxed_primitive_subtype(s_kind, target) {
                return SubtypeResult::True;
            }
            // `object` keyword is structurally equivalent to `{}` (empty object).
            // It's assignable to any object type where all properties are optional,
            // since no required properties need to be satisfied.
            //
            // However, `object` is NOT assignable to types with index signatures
            // (e.g., `{ [s: string]: unknown }`). In tsc, `object` lacks an implicit
            // index signature, so assigning it to `{ [s: string]: T }` fails with
            // "Index signature for type 'string' is missing in type '{}'".
            // Note: `{}` IS assignable to indexed types (handled elsewhere), but the
            // `object` keyword gets stricter treatment in tsc.
            if s_kind == IntrinsicKind::Object {
                let target_shape = object_shape_id(self.interner, target)
                    .or_else(|| object_with_index_shape_id(self.interner, target));
                if let Some(t_shape_id) = target_shape {
                    let t_shape = self.interner.object_shape(t_shape_id);
                    if t_shape.properties.iter().all(|p| p.optional)
                        && t_shape.string_index.is_none()
                        && t_shape.number_index.is_none()
                    {
                        return SubtypeResult::True;
                    }
                }
            }
            // When target is an unevaluated IndexAccess (e.g., Obj[K] where K is a
            // type parameter), don't return False early. The IndexAccess fallback
            // (check_generic_index_access_subtype) after the visitor dispatch can
            // resolve the access by distributing over K's constraint literals.
            if index_access_parts(self.interner, target).is_none() {
                // Trace: Intrinsic type mismatch (boxed primitive check failed)
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
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
            // Trace: Literal type mismatch
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::LiteralTypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
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
            // Trace: Source is not object-compatible
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
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
            || self
                .resolver
                .is_boxed_type_id(target, IntrinsicKind::Function)
            || self
                .resolver
                .get_boxed_type(IntrinsicKind::Function)
                .is_some_and(|boxed| boxed == target)
            || lazy_def_id(self.interner, target).is_some_and(|def_id| {
                self.resolver
                    .is_boxed_def_id(def_id, IntrinsicKind::Function)
            })
            || is_function_structural;
        if is_function_target {
            if self.is_callable_type(source) {
                return SubtypeResult::True;
            }
            // For structural Function interface targets (object types with apply/call/bind),
            // allow non-callable object types to fall through to structural checking.
            // This handles class instances that extend Function (e.g., `class Foo extends Function {}`),
            // where the instance type has apply/call/bind methods but no call signature.
            // TypeScript allows such types as valid instanceof RHS because they're structurally
            // assignable to the Function interface.
            let source_is_object = object_shape_id(self.interner, source).is_some()
                || object_with_index_shape_id(self.interner, source).is_some();
            if is_function_structural && source_is_object {
                // Fall through to structural object-to-object comparison below
            } else {
                // Trace: Source is not function-compatible
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
        }

        // Check if target is the global `Object` interface from lib.d.ts.
        // This is a separate path from intrinsic `object`:
        // - `object` (lowercase) includes callable values.
        // - `Object` (capitalized interface) should follow TS structural rules and
        //   exclude bare callable types from primitive-style object assignability.
        let is_global_object_target = self
            .resolver
            .is_boxed_type_id(target, IntrinsicKind::Object)
            || self
                .resolver
                .get_boxed_type(IntrinsicKind::Object)
                .is_some_and(|boxed| boxed == target)
            || lazy_def_id(self.interner, target)
                .is_some_and(|t_def| self.resolver.is_boxed_def_id(t_def, IntrinsicKind::Object));
        if is_global_object_target {
            let source_eval = self.evaluate_type(source);
            if self.is_global_object_interface_type(source_eval) {
                return SubtypeResult::True;
            }

            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
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

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.check_function_subtype(&s_fn, &t_fn);
        }

        // Function intrinsic as source against function/callable target:
        // In tsc, `Function` is structurally `(...args: any[]) => any`, so
        // `Function extends (...args: any) => any ? T : F` takes the true branch.
        // NOTE: This only handles `TypeId::FUNCTION` (the intrinsic). The Object
        // representation of the Function interface is handled in the conditional
        // type evaluator's infer pattern matching, not in general subtype checking,
        // because tsc distinguishes between conditional extends (true branch) and
        // generic constraint satisfaction (TS2344 for Parameters<Function>).
        if source == TypeId::FUNCTION {
            if let Some(t_fn_id) = function_shape_id(self.interner, target) {
                let t_fn = self.interner.function_shape(t_fn_id);
                let function_shape = crate::types::FunctionShape {
                    params: vec![crate::types::ParamInfo {
                        name: None,
                        type_id: TypeId::ANY,
                        optional: false,
                        rest: true,
                    }],
                    this_type: None,
                    return_type: TypeId::ANY,
                    type_params: Vec::new(),
                    type_predicate: None,
                    is_constructor: false,
                    is_method: false,
                };
                return self.check_function_subtype(&function_shape, &t_fn);
            }
            if let Some(t_callable_id) = callable_shape_id(self.interner, target) {
                let t_shape = self.interner.callable_shape(t_callable_id);
                if !t_shape.call_signatures.is_empty() {
                    // Function is callable, check against last call signature
                    return SubtypeResult::True;
                }
            }
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
            let mut evaluated_target = self.evaluate_type(target);
            if evaluated_target == target {
                let raw_evaluated =
                    crate::evaluation::evaluate::evaluate_type(self.interner, target);
                if raw_evaluated != target {
                    evaluated_target = raw_evaluated;
                }
            }
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
            let mut evaluated_source = self.evaluate_type(source);
            if evaluated_source == source {
                let raw_evaluated =
                    crate::evaluation::evaluate::evaluate_type(self.interner, source);
                if raw_evaluated != source {
                    evaluated_source = raw_evaluated;
                }
            }
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
            return self.check_application_to_application_subtype(s_app_id, t_app_id);
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
                    let result = self.check_application_to_application_subtype(s_app_id, t_app_id);
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

        // =======================================================================
        // ENUM TYPE CHECKING (Nominal Identity)
        // =======================================================================
        // Enums are nominal types - two different enums with the same member types
        // are NOT compatible. Enum(DefId, MemberType) preserves both:
        // - DefId: For nominal identity (E1 != E2)
        // - MemberType: For structural assignability to primitives (E1 <: number)
        // =======================================================================

        if let (Some((s_def_id, _s_members)), Some((t_def_id, _t_members))) = (
            enum_components(self.interner, source),
            enum_components(self.interner, target),
        ) {
            if s_def_id == t_def_id
                && source != target
                && crate::type_queries::is_literal_enum_member(self.interner, source)
                && crate::type_queries::is_literal_enum_member(self.interner, target)
            {
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }

            // Enum to Enum: Nominal check - DefIds must match
            if s_def_id == t_def_id {
                return SubtypeResult::True;
            }

            // Check for member-to-parent relationship (e.g., E.A -> E)
            // If source is a member of the target enum, it is a subtype
            if self.resolver.get_enum_parent_def_id(s_def_id) == Some(t_def_id) {
                // Source is a member of target enum
                // Only allow if target is the full enum type (not a different member)
                if self.resolver.is_enum_type(target, self.interner) {
                    return SubtypeResult::True;
                }
            }

            // Different enums are NOT compatible (nominal typing)
            // Trace: Enum nominal mismatch
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Source is Enum, Target is not - check structural member type
        if let Some((_s_def_id, s_members)) = enum_components(self.interner, source) {
            return self.check_subtype(s_members, target);
        }

        // Target is Enum, Source is not - check Rule #7 first, then structural member type
        if let Some((t_def_id, t_members)) = enum_components(self.interner, target) {
            // Rule #7: number is assignable to numeric enums
            if source == TypeId::NUMBER && self.resolver.is_numeric_enum(t_def_id) {
                return SubtypeResult::True;
            }
            // For number literals, fall through to structural check against t_members
            // so that only actual enum member values (e.g., 0|1|2) are accepted
            return self.check_subtype(source, t_members);
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
                    return self.check_object_subtype(
                        s_shape,
                        None,
                        Some(source),
                        &t_shape,
                        Some(target),
                    );
                }
                // FunctionShape has no properties - not assignable to non-empty object
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
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
                // FunctionShape has no properties - not assignable to non-empty indexed object
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
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
                // Try the Array<T> interface for full structural comparison.
                // This handles cases like: number[] <: { toString(): string }
                if let Some(elem) = array_element_type(self.interner, source)
                    && let Some(result) = self.check_array_interface_subtype(elem, target)
                {
                    return result;
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
                // Trace: Array/tuple not compatible with object
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
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
                    if t_shape.string_index.is_some() {
                        if let Some(tracer) = &mut self.tracer
                            && !tracer.on_mismatch_dyn(
                                SubtypeFailureReason::MissingIndexSignature {
                                    index_kind: "string",
                                },
                            )
                        {
                            return SubtypeResult::False;
                        }
                        return SubtypeResult::False;
                    }
                    if let Some(ref num_idx) = t_shape.number_index {
                        let elem_type =
                            array_element_type(self.interner, source).unwrap_or(TypeId::ANY);
                        if !self.check_subtype(elem_type, num_idx.value_type).is_true() {
                            // Trace: Array element type mismatch with index signature
                            if let Some(tracer) = &mut self.tracer
                                && !tracer.on_mismatch_dyn(
                                    SubtypeFailureReason::IndexSignatureMismatch {
                                        index_kind: "number",
                                        source_value_type: elem_type,
                                        target_value_type: num_idx.value_type,
                                    },
                                )
                            {
                                return SubtypeResult::False;
                            }
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::True;
                }
                // Target has non-empty properties + index signature.
                // Try the Array<T> interface for full structural comparison.
                if let Some(elem) = array_element_type(self.interner, source)
                    && let Some(result) = self.check_array_interface_subtype(elem, target)
                {
                    return result;
                }
                // Trace: Array/tuple not compatible with indexed object with non-empty properties
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
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

        // Trace: Generic fallback type mismatch (no specific reason matched above)
        if result == SubtypeResult::False
            && let Some(tracer) = &mut self.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            })
        {
            return SubtypeResult::False;
        }

        result
    }
}
