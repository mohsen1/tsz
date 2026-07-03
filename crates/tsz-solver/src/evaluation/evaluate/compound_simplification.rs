//! Union and intersection subtype simplification for `TypeEvaluator`.

use super::*;

/// Per-property `(optional, readonly)` modifier map keyed by property-name atom,
/// or `None` for members that contribute no object properties. Used by
/// intersection simplification to AND-merge modifiers when deciding whether a
/// structurally subsumed member can be dropped.
type MemberModifierMap = Option<FxHashMap<u32, (bool, bool)>>;

/// Controls which subtype direction makes a member redundant when simplifying
/// a union or intersection.
#[derive(Debug)]
enum SubtypeDirection {
    /// member[i] <: member[j] -> member[i] is redundant (union semantics).
    SourceSubsumedByOther,
    /// member[j] <: member[i] -> member[i] is redundant (intersection semantics).
    OtherSubsumedBySource,
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Simplify union members by removing redundant types using deep subtype checks.
    /// If A <: B, then A | B = B (A is redundant in the union).
    ///
    /// This uses `SubtypeChecker` with `bypass_evaluation=true` to prevent infinite
    /// recursion, since `TypeEvaluator` has already evaluated all members.
    ///
    /// Performance: O(N^2) where N is the number of members. We skip simplification
    /// if the union has more than 25 members to avoid excessive computation.
    ///
    /// ## Strategy
    ///
    /// 1. **Early exit for large unions** (>25 members) to avoid O(N^2) explosion
    /// 2. **Skip complex types** that require full resolution:
    ///    - `TypeParameter`, Infer, Conditional, Mapped, `IndexAccess`, `KeyOf`, `TypeQuery`
    ///    - `TemplateLiteral`, `ReadonlyType`, String manipulation types
    ///    - Note: Lazy and Application are NOW safe (Task #37: handled by Canonicalizer)
    /// 3. **Fast-path for any/unknown**: If any member is any, entire union becomes any
    /// 4. **Identity check**: O(1) structural identity via `SubtypeChecker` (Task #36 fast-path)
    /// 5. **Depth limit**: `MAX_SUBTYPE_DEPTH` enables deep recursive type simplification (Task #37)
    ///
    /// ## Example Reductions
    ///
    /// - `"a" | string` -> `string` (literal absorbed by primitive)
    /// - `number | 1 | 2` -> `number` (literals absorbed by primitive)
    /// - `{ a: string } | { a: string; b: number }` -> `{ a: string; b: number }`
    pub(super) fn simplify_union_members(&mut self, members: &mut Vec<TypeId>) {
        // Single-pass early-exit: check for unknown (skip entirely) and whether all
        // members are identity-comparable (disjoint, so O(n^2) loop finds nothing).
        let mut all_identity = true;
        for &id in members.iter() {
            if id.is_unknown() {
                return;
            }
            if all_identity && !self.interner.is_identity_comparable_type(id) {
                all_identity = false;
            }
        }
        if all_identity {
            return;
        }
        // In a union, A <: B means A is redundant (B subsumes it).
        // E.g. `"a" | string` => "a" is redundant, result: `string`
        self.remove_redundant_members(members, SubtypeDirection::SourceSubsumedByOther);
    }

    /// Simplify intersection members by removing redundant types using deep subtype checks.
    /// If A <: B, then A & B = A (B is redundant in the intersection).
    ///
    /// ## Example Reductions
    ///
    /// - `{ a: string } & { a: string; b: number }` -> `{ a: string; b: number }`
    /// - `{ readonly a: string } & { a: string }` -> `{ readonly a: string }`
    /// - `number & 1` -> `1` (literal is more specific)
    pub(super) fn simplify_intersection_members(&mut self, members: &mut Vec<TypeId>) {
        // In an intersection, A <: B means B is redundant (A is more specific).
        // We check if other members are subtypes of the candidate to remove the supertype.
        self.remove_redundant_members(members, SubtypeDirection::OtherSubsumedBySource);
    }

    /// Remove redundant members from a type list using subtype checks.
    ///
    /// This is the shared O(n^2) core for both union and intersection simplification.
    /// The `direction` parameter controls which subtype relationship makes a member
    /// redundant:
    /// - `SourceSubsumedByOther`: member[i] <: member[j] -> i is redundant (union semantics)
    /// - `OtherSubsumedBySource`: member[j] <: member[i] -> i is redundant (intersection semantics)
    ///
    /// Common early exits (size guards, `any` check, complex-type check) are applied here.
    fn remove_redundant_members(&mut self, members: &mut Vec<TypeId>, direction: SubtypeDirection) {
        // Performance guard: skip small or very large type lists
        const MAX_SIMPLIFICATION_SIZE: usize = 25;
        if members.len() < 2 || members.len() > MAX_SIMPLIFICATION_SIZE {
            return;
        }

        // Single-pass early-exit check instead of two separate O(N) scans.
        for &id in members.iter() {
            if id.is_any()
                || crate::contains_this_type(self.interner, id)
                || self.is_complex_type(id)
            {
                return;
            }
        }

        use crate::relations::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
        let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
        checker.bypass_evaluation = true;
        checker.max_depth = MAX_SUBTYPE_DEPTH;
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access;
        checker.exact_optional_property_types = self.exact_optional_property_types;

        // Pre-compute property name sets for all members once, avoiding O(N^2) FxHashSet
        // allocations in the inner loop. Each entry is None for non-object types.
        let prop_names: Vec<Option<FxHashSet<u32>>> = members
            .iter()
            .map(|&id| {
                let mut names = FxHashSet::default();
                Self::collect_property_names(self.interner, id, &mut names);
                if names.is_empty() { None } else { Some(names) }
            })
            .collect();

        // Pre-compute per-member property modifier maps once for the intersection
        // direction, mirroring the `prop_names` precompute above. The modifier
        // guard below then reduces to O(1) cached lookups instead of re-walking
        // shapes on every candidate pair. Union simplification never consults
        // these, so skip the work entirely in that direction.
        let prop_mods: Vec<MemberModifierMap> =
            if matches!(direction, SubtypeDirection::OtherSubsumedBySource) {
                members
                    .iter()
                    .map(|&id| {
                        let mut mods = FxHashMap::default();
                        Self::collect_property_modifiers(self.interner, id, &mut mods);
                        if mods.is_empty() { None } else { Some(mods) }
                    })
                    .collect()
            } else {
                Vec::new()
            };

        // Union subtype reduction only removes a member that is structured or
        // instantiable, or when the union contains an empty object type, any
        // member subsumed by that empty object. This mirrors tsc's
        // `removeSubtypes`, which gates removal on
        // `hasEmptyObject || source.flags & StructuredOrInstantiable`: a bare
        // primitive keyword (`boolean`, `number`, `string`, ...) vacuously
        // satisfies a weak (all-optional) object member structurally, but tsc
        // never drops it via that subsumption. Literal members are still removed
        // here because tsz folds tsc's separate literal-absorption pass
        // (`"a" | string` -> `string`) into this loop; literals are recognized
        // below and stay removable. `has_empty_object` is only consulted in the
        // union direction; intersection simplification keeps its own rules.
        let has_empty_object = matches!(direction, SubtypeDirection::SourceSubsumedByOther)
            && members.iter().any(|&id| {
                crate::visitors::visitor_predicates::is_empty_object_type(self.interner, id)
            });

        // Use mark-and-compact instead of Vec::remove() which is O(N) per removal.
        // Since max size is 25 (from guard above), a u32 bitset avoids heap allocation.
        let len = members.len();
        let mut keep: u32 = (1u32 << len) - 1; // all bits set
        for i in 0..len {
            if keep & (1u32 << i) == 0 {
                continue;
            }
            // Union subtype-removal eligibility depends only on `members[i]`
            // (structured/instantiable, or any member when the union holds an
            // empty object; a bare primitive keyword is kept even when it
            // structurally satisfies a weak object sibling). Compute it once per
            // `i` instead of per `(i, j)`. Intersection simplification does not
            // consult it, so the `matches!` short-circuits the helper away.
            let union_candidate_removable =
                matches!(direction, SubtypeDirection::SourceSubsumedByOther)
                    && Self::union_member_removable_as_subtype(
                        self.interner,
                        members[i],
                        has_empty_object,
                    );
            for j in 0..len {
                if i == j || keep & (1u32 << j) == 0 {
                    continue;
                }
                if members[i] == members[j] {
                    continue;
                }

                let is_subtype = match direction {
                    SubtypeDirection::SourceSubsumedByOther => {
                        union_candidate_removable
                            && self.compound_subtype_cached(&mut checker, members[i], members[j])
                            && !Self::has_unique_properties_cached(&prop_names[i], &prop_names[j])
                            && !Self::has_index_signature_not_in(
                                self.interner,
                                members[i],
                                members[j],
                            )
                            && !Self::is_literal_under_branded_primitive(
                                self.interner,
                                members[i],
                                members[j],
                            )
                    }
                    SubtypeDirection::OtherSubsumedBySource => {
                        // For intersections: member[j] <: member[i] means member[i] is
                        // a candidate for removal. But if member[i] contributes properties
                        // that member[j] doesn't have, it must be kept; removing it would
                        // lose those property declarations from the intersection type.
                        //
                        // Opaque Application/Lazy guard: when bypass_evaluation prevents
                        // SubtypeChecker from expanding an unreduced Application or Lazy
                        // member, that member appears empty to the checker. A concrete
                        // sibling like `{path?: _}` would then trivially "subsume" it and
                        // get the Application dropped, even though the Application would
                        // contribute additional union/object members once expanded.
                        !Self::is_opaque_under_bypass_eval(self.interner, members[i])
                            && self.compound_subtype_cached(&mut checker, members[j], members[i])
                            && !Self::has_unique_properties_cached(&prop_names[i], &prop_names[j])
                            && !Self::intersection_drop_changes_modifiers(
                                &prop_mods[i],
                                &prop_mods[j],
                            )
                            && !Self::has_index_signature_not_in(
                                self.interner,
                                members[i],
                                members[j],
                            )
                            && !Self::is_branded_primitive_pair(
                                self.interner,
                                members[i],
                                members[j],
                            )
                    }
                };
                if is_subtype {
                    tracing::trace!(
                        removed = ?members[i],
                        subsumed_by = ?members[j],
                        ?direction,
                        "remove_redundant_members: removing member"
                    );
                    keep &= !(1u32 << i);
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep & (1u32 << read) != 0 {
                if write != read {
                    members[write] = members[read];
                }
                write += 1;
            }
        }
        members.truncate(write);
    }

    /// Return the subtype answer used by compound simplification, reusing
    /// repeated pair checks during this evaluator request. Limit-derived
    /// relation answers are deliberately not inserted: a later pass should
    /// recompute rather than inherit a budget-conditional result.
    fn compound_subtype_cached(
        &mut self,
        checker: &mut crate::relations::subtype::SubtypeChecker<'_, R>,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let key = CompoundSubtypePairKey::from_checker(checker, source, target);
        let session = self.eval_session;
        if let Some(session) = session {
            if let Some(cached) = session.compound_subtype_probe_get(key) {
                return cached;
            }
        } else if let Some(&cached) = self.compound_subtype_cache.get(&key) {
            return cached;
        }

        let lazy_events_at_entry = checker.unresolved_lazy_relation_event_count();
        let weak_sensitivity_at_entry = crate::limits::weak_type_sensitivity_count();
        let result = checker.check_subtype(source, target);
        let related = result.is_true();
        let definitive = matches!(
            result,
            crate::relations::subtype::SubtypeResult::True
                | crate::relations::subtype::SubtypeResult::False
        );
        if definitive
            && !checker.depth_exceeded()
            && !checker.iteration_exceeded()
            && checker.unresolved_lazy_relation_event_count() == lazy_events_at_entry
            && crate::limits::weak_type_sensitivity_count() == weak_sensitivity_at_entry
        {
            if let Some(session) = session {
                session.compound_subtype_probe_put(key, related);
            } else {
                self.compound_subtype_cache.insert(key, related);
            }
        }
        related
    }

    /// Test hook: seed the simplification relation memo to prove its read path
    /// participates in the production reduction decision.
    #[cfg(test)]
    pub(crate) fn seed_compound_subtype_cache_for_test(
        &mut self,
        checker: &crate::relations::subtype::SubtypeChecker<'_, R>,
        source: TypeId,
        target: TypeId,
        result: bool,
    ) {
        let key = CompoundSubtypePairKey::from_checker(checker, source, target);
        self.compound_subtype_cache.insert(key, result);
    }

    /// Check if `candidate` has any property names that `subsuming` doesn't have,
    /// using pre-computed property name sets to avoid repeated allocation.
    fn has_unique_properties_cached(
        candidate_names: &Option<FxHashSet<u32>>,
        subsuming_names: &Option<FxHashSet<u32>>,
    ) -> bool {
        let Some(candidate) = candidate_names else {
            return false; // No properties -> can't contribute unique ones
        };
        let Some(subsuming) = subsuming_names else {
            return true; // Candidate has properties but subsuming doesn't
        };
        candidate.iter().any(|name| !subsuming.contains(name))
    }

    /// Decide whether a union member may be dropped by subtype reduction when a
    /// sibling member structurally subsumes it.
    ///
    /// Mirrors the eligibility gate in tsc's `removeSubtypes`
    /// (`hasEmptyObject || source.flags & StructuredOrInstantiable`):
    /// - Object / intersection / instantiable members (anything that is not a
    ///   bare intrinsic keyword) are eligible.
    /// - Literal members stay eligible because tsz folds tsc's separate literal
    ///   absorption pass into this same loop.
    /// - A bare primitive keyword (`boolean`, `number`, `string`, `symbol`,
    ///   `bigint`, `void`, `null`, `undefined`, `object`, ...) is NOT eligible.
    ///   The sole exception is `has_empty_object`: when the union literally
    ///   contains an empty object type, everything it subsumes is collapsed into
    ///   it (`boolean | {}` -> `{}`), matching tsc.
    fn union_member_removable_as_subtype(
        db: &dyn crate::caches::db::TypeDatabase,
        member: TypeId,
        has_empty_object: bool,
    ) -> bool {
        if has_empty_object {
            return true;
        }
        if crate::visitors::visitor_predicates::is_literal_type(db, member) {
            return true;
        }
        // Reserved intrinsic TypeIds (`boolean`, `number`, `string`, `object`,
        // ...) are bare keyword types and stay protected. They are checked
        // explicitly because they do not always resolve through `lookup`.
        if member.is_intrinsic() {
            return false;
        }
        // Bare intrinsic keyword types (non-literal) are protected from
        // object-subsumption removal; structured/instantiable members are not.
        !crate::visitors::visitor_predicates::is_intrinsic_or_literal_type(db, member)
    }

    /// Check whether a (candidate, subsuming) pair forms the branded-primitive
    /// idiom `string & {}` (or `number & {}`, `boolean & {}`, ...).
    fn is_branded_primitive_pair(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        crate::visitors::visitor_predicates::is_empty_object_type(db, candidate)
            && crate::visitors::visitor_predicates::is_widening_primitive_intrinsic(db, subsuming)
    }

    /// Returns true when `type_id` is an unreduced `Application` or `Lazy`
    /// whose structural shape cannot be inspected while `bypass_evaluation`
    /// is on.
    pub(super) fn is_opaque_under_bypass_eval(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        matches!(
            db.lookup(type_id),
            Some(TypeData::Application(_) | TypeData::Lazy(_))
        )
    }

    /// Check whether a union member is a literal that's only "subsumed" by a
    /// branded-primitive intersection (`string & {}` and friends).
    fn is_literal_under_branded_primitive(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        if !crate::visitors::visitor_predicates::is_literal_type(db, candidate) {
            return false;
        }
        let Some(TypeData::Intersection(list_id)) = db.lookup(subsuming) else {
            return false;
        };
        let members = db.type_list(list_id);
        let mut has_widening_primitive = false;
        let mut has_empty_object = false;
        for &m in members.iter() {
            if crate::visitors::visitor_predicates::is_widening_primitive_intrinsic(db, m) {
                has_widening_primitive = true;
            } else if crate::visitors::visitor_predicates::is_empty_object_type(db, m) {
                has_empty_object = true;
            } else {
                return false;
            }
        }
        has_widening_primitive && has_empty_object
    }

    /// Check if `candidate` has an index signature that `subsuming` lacks.
    fn has_index_signature_not_in(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        Self::carries_index_signature(db, candidate)
            && !Self::carries_index_signature(db, subsuming)
    }

    fn carries_index_signature(db: &dyn crate::caches::db::TypeDatabase, type_id: TypeId) -> bool {
        match db.lookup(type_id) {
            Some(TypeData::ObjectWithIndex(_) | TypeData::Mapped(_)) => true,
            Some(TypeData::Callable(shape_id)) => {
                let shape = db.callable_shape(shape_id);
                shape.string_index.is_some() || shape.number_index.is_some()
            }
            _ => false,
        }
    }

    /// Collect property name atoms from an object type into the provided set.
    fn collect_property_names(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
        names: &mut FxHashSet<u32>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                for prop in &shape.properties {
                    names.insert(prop.name.0);
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                let sub_members = db.type_list(list_id);
                for &sub in sub_members.iter() {
                    Self::collect_property_names(db, sub, names);
                }
            }
            Some(TypeData::Array(_) | TypeData::Tuple(_)) => {
                names.insert(u32::MAX);
            }
            _ => {}
        }
    }

    /// Collect per-property `(optional, readonly)` modifiers for an object-like
    /// member, merging nested intersection members with tsc's AND semantics.
    fn collect_property_modifiers(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
        mods: &mut FxHashMap<u32, (bool, bool)>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                for prop in &shape.properties {
                    let entry = mods
                        .entry(prop.name.0)
                        .or_insert((prop.optional, prop.readonly));
                    entry.0 = entry.0 && prop.optional;
                    entry.1 = entry.1 && prop.readonly;
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                for &sub in db.type_list(list_id).iter() {
                    Self::collect_property_modifiers(db, sub, mods);
                }
            }
            _ => {}
        }
    }

    /// Returns true when dropping the `dropped` member from an intersection
    /// while keeping `kept` would change a shared property's readonly/optional
    /// modifier relative to tsc's AND-merge semantics.
    fn intersection_drop_changes_modifiers(
        dropped: &MemberModifierMap,
        kept: &MemberModifierMap,
    ) -> bool {
        let (Some(dropped), Some(kept)) = (dropped, kept) else {
            return false;
        };
        kept.iter().any(
            |(name, &(kept_optional, kept_readonly))| match dropped.get(name) {
                Some(&(dropped_optional, dropped_readonly)) => {
                    (kept_readonly && !dropped_readonly) || (kept_optional && !dropped_optional)
                }
                None => false,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::evaluation::session::EvaluationSession;
    use crate::relations::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
    use crate::types::{PropertyInfo, TypeId};

    fn exact_optional_probe_pair(interner: &TypeInterner) -> (TypeId, TypeId) {
        let prop = interner.intern_string("value");
        let present_undefined = interner.object(vec![PropertyInfo::new(prop, TypeId::UNDEFINED)]);
        let optional_number = interner.object(vec![PropertyInfo::opt(prop, TypeId::NUMBER)]);
        (present_undefined, optional_number)
    }

    fn compound_probe_checker<'a>(
        interner: &'a TypeInterner,
        exact_optional_property_types: bool,
    ) -> SubtypeChecker<'a> {
        let mut checker = SubtypeChecker::new(interner);
        checker.bypass_evaluation = true;
        checker.max_depth = MAX_SUBTYPE_DEPTH;
        checker.exact_optional_property_types = exact_optional_property_types;
        checker
    }

    #[test]
    fn union_simplification_threads_exact_optional_mode_into_local_subtype_checker() {
        let interner = TypeInterner::new();
        let (present_undefined, optional_number) = exact_optional_probe_pair(&interner);

        let mut legacy_members = vec![present_undefined, optional_number];
        let mut legacy_evaluator = TypeEvaluator::new(&interner);
        legacy_evaluator.set_exact_optional_property_types(false);
        legacy_evaluator.simplify_union_members(&mut legacy_members);
        assert_eq!(
            legacy_members,
            vec![optional_number],
            "legacy optional mode treats an optional number property as accepting present undefined",
        );

        let mut exact_members = vec![present_undefined, optional_number];
        let mut exact_evaluator = TypeEvaluator::new(&interner);
        exact_evaluator.set_exact_optional_property_types(true);
        exact_evaluator.simplify_union_members(&mut exact_members);
        assert_eq!(
            exact_members,
            vec![present_undefined, optional_number],
            "exact optional mode must not reuse legacy optional-property subtyping",
        );
    }

    #[test]
    fn compound_subtype_cache_partitions_seeded_probe_by_exact_optional_mode() {
        let interner = TypeInterner::new();
        let (present_undefined, optional_number) = exact_optional_probe_pair(&interner);

        let mut evaluator = TypeEvaluator::new(&interner);
        evaluator.set_exact_optional_property_types(false);
        let legacy_checker = compound_probe_checker(&interner, false);
        evaluator.seed_compound_subtype_cache_for_test(
            &legacy_checker,
            present_undefined,
            optional_number,
            true,
        );

        // Flip the mode without clearing the memo so this test proves the key
        // partition itself, not only `set_exact_optional_property_types` reset.
        evaluator.exact_optional_property_types = true;
        let mut exact_members = vec![present_undefined, optional_number];
        evaluator.simplify_union_members(&mut exact_members);

        assert_eq!(
            exact_members,
            vec![present_undefined, optional_number],
            "a legacy-mode seeded verdict must not be read by an exact-mode compound probe",
        );
    }

    #[test]
    fn compound_simplification_reads_session_probe_cache() {
        let interner = TypeInterner::new();
        let lit_a = interner.literal_string("a");
        let narrow = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            lit_a,
        )]);
        let wide = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::STRING,
        )]);
        let checker = compound_probe_checker(&interner, false);
        let key = CompoundSubtypePairKey::from_checker(&checker, narrow, wide);
        let session = EvaluationSession::new();
        session.compound_subtype_probe_put(key, false);

        let mut members = vec![narrow, wide];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_union_members(&mut members);

        assert_eq!(
            members,
            vec![narrow, wide],
            "a fresh evaluator should read raw subtype probes from the owning session",
        );
        assert_eq!(
            session.compound_subtype_probe_cache_entries(),
            2,
            "the seeded decisive probe and the reverse miss should live in the session",
        );
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "session-backed probes should not duplicate entries in the evaluator-local fallback",
        );
    }

    #[test]
    fn compound_subtype_probe_key_tracks_relation_and_simplifier_modes() {
        let interner = TypeInterner::new();
        let (source, target) = exact_optional_probe_pair(&interner);

        let legacy_checker = compound_probe_checker(&interner, false);
        let mut exact_checker = compound_probe_checker(&interner, true);
        let mut unchecked_checker = compound_probe_checker(&interner, false);
        unchecked_checker.no_unchecked_indexed_access = true;
        let mut normal_eval_checker = compound_probe_checker(&interner, false);
        normal_eval_checker.bypass_evaluation = false;
        let mut shallow_checker = compound_probe_checker(&interner, false);
        shallow_checker.max_depth = MAX_SUBTYPE_DEPTH - 1;

        let legacy_key = CompoundSubtypePairKey::from_checker(&legacy_checker, source, target);
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&exact_checker, source, target),
            "exactOptionalPropertyTypes is part of compound subtype probe identity",
        );
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&unchecked_checker, source, target),
            "noUncheckedIndexedAccess is part of the underlying relation identity",
        );
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&normal_eval_checker, source, target),
            "bypass-evaluation mode is specific to compound simplification probes",
        );
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&shallow_checker, source, target),
            "compound subtype probe depth participates in the local memo key",
        );

        exact_checker.exact_optional_property_types = false;
        assert_eq!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&exact_checker, source, target),
            "matching relation and simplifier modes should address the same local memo slot",
        );
    }
}
