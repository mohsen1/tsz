//! Union and intersection subtype simplification for `TypeEvaluator`.

use super::*;

/// Per-member property-name set keyed by property-name atom, or `None` for
/// members that contribute no object properties.
type MemberPropertyNames = Option<FxHashSet<u32>>;
/// Per-property `(optional, readonly)` modifier map keyed by property-name atom,
/// or `None` for members that contribute no object properties. Used by
/// intersection simplification to AND-merge modifiers when deciding whether a
/// structurally subsumed member can be dropped.
type MemberModifierMap = Option<FxHashMap<u32, (bool, bool)>>;

/// Structural facts about one compound simplification member.
///
/// These facts are computed without running relation or evaluation queries, so
/// they can be reused across the simplifier's O(n^2) pair loop. The final
/// decision still runs the relation probe and every veto in order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CompoundMemberFacts {
    property_names: MemberPropertyNames,
    property_modifiers: MemberModifierMap,
    carries_index_signature: bool,
    is_empty_object_type: bool,
    is_intrinsic: bool,
    is_intrinsic_or_literal_type: bool,
    is_literal_type: bool,
    is_widening_primitive_intrinsic: bool,
    is_opaque_under_bypass_eval: bool,
    is_branded_primitive_intersection: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CompoundMemberFactsKey {
    type_id: TypeId,
    include_property_modifiers: bool,
}

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
        // Union/intersection member removal must act only on a *definitive*
        // subtype verdict. Under `bypass_evaluation` the relation cannot expand
        // opaque `Application`/`Lazy` components, so a coinductive cycle or a
        // depth/iteration-limit hit yields a limit-derived optimistic "maybe"
        // (`CycleDetected`/`DepthExceeded`) that `is_true()` reads as related —
        // dropping distinct members whose difference lives inside the un-expanded
        // component (e.g. `Float32Array<ArrayBuffer>` vs `Float64Array<ArrayBuffer>`
        // via `Record<string, TypedArrayCtorUnion>[key]`, and the runtypes fuel
        // blowup documented in the storm soundness ledger). Turning off the
        // coinductive assumption makes every cycle/limit verdict `False` at every
        // nesting level, so reduction keeps a member unless it is *provably*
        // redundant. Termination is unaffected: the recursion guard still fires;
        // only the returned verdict changes from optimistic-true to not-related.
        checker.assume_related_on_cycle = false;
        checker.assume_related_on_depth = false;
        checker.max_depth = MAX_SUBTYPE_DEPTH;
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access;
        checker.exact_optional_property_types = self.exact_optional_property_types;
        // Union narrowing must apply the same weak-type ("no common properties",
        // TS2559) veto tsc's own `removeSubtypes` gets from `strictSubtypeRelation`:
        // that relation is not `ComparableRelation`, so `isPerformingCommonPropertyChecks`
        // still runs. Without this, a member that structurally satisfies an
        // all-optional ("weak") sibling only because it shares no properties with
        // it — e.g. `(() => T) | { get?(): T }` — reads as redundant and gets
        // dropped, collapsing the union to just the weak member. A later
        // assignability check against an argument that only matches the dropped
        // member then reports TS2559 against the sole remaining (weak) member
        // instead of succeeding through the one that was removed (#16707).
        // Intersection simplification (`OtherSubsumedBySource`) is unaffected:
        // tsc's intersection construction does not run this relation.
        if matches!(direction, SubtypeDirection::SourceSubsumedByOther) {
            checker.enforce_weak_types = true;
        }

        // Snapshot per-member veto facts before the pair loop. These facts are
        // pure, per-call data, so relation probes can mutably borrow `self`
        // without re-walking object/intersection shapes for every pair.
        let needs_property_modifiers = matches!(direction, SubtypeDirection::OtherSubsumedBySource);
        let mut member_facts_memo = FxHashMap::default();
        let member_facts: Vec<CompoundMemberFacts> = members
            .iter()
            .map(|&id| {
                self.compound_member_facts_memoized(
                    id,
                    needs_property_modifiers,
                    &mut member_facts_memo,
                )
            })
            .collect();

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
            && member_facts.iter().any(|facts| facts.is_empty_object_type);

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
                    && Self::union_member_removable_as_subtype(&member_facts[i], has_empty_object);
            // tsc keeps a redundant *concrete* array/tuple supertype member in
            // an intersection: `string[] & (string | number)[]` stays a
            // two-member `IntersectionType`, it does not collapse to `string[]`
            // the way object members merge. Dropping the wider element-typed
            // sibling makes `T extends (A & B)` and `T extends A` intern to one
            // `TypeId`, defeating the higher-order `Equal<A & B, A>` identity
            // probe (#16095).
            //
            // The `any`-containing case is the exception, and tsc drops it: in
            // `[any] & [1]` / `any[] & 1[]` the `any`-typed container is the
            // supertype, and tsc removes it so an array/tuple *literal* is
            // contextually typed by the concrete member (`[1]` / `1[]`) rather
            // than widening its elements through `any`
            // (`contextualTypeBasedOnIntersectionWithAnyInTheMix3`). So the veto
            // fires only for a container with no `any` anywhere in it.
            //
            // Objects still merge, and `never`/literal/duplicate reductions are
            // unaffected. Depends only on `members[i]`, so like
            // `union_candidate_removable` it is computed once per `i`.
            let intersection_candidate_is_container =
                matches!(direction, SubtypeDirection::OtherSubsumedBySource)
                    && crate::type_queries::is_array_or_tuple_type(self.interner, members[i])
                    && !Self::container_element_contains_any(self.interner, members[i]);
            for j in 0..len {
                if i == j || keep & (1u32 << j) == 0 {
                    continue;
                }
                if members[i] == members[j] {
                    continue;
                }

                let is_subtype = match direction {
                    SubtypeDirection::SourceSubsumedByOther => {
                        let candidate = &member_facts[i];
                        let subsuming = &member_facts[j];
                        let related =
                            self.compound_subtype_cached(&mut checker, members[i], members[j]);
                        related
                            && union_candidate_removable
                            && !Self::has_unique_properties_cached(
                                &candidate.property_names,
                                &subsuming.property_names,
                            )
                            && !Self::has_index_signature_not_in(candidate, subsuming)
                            && !Self::is_literal_under_branded_primitive(candidate, subsuming)
                    }
                    SubtypeDirection::OtherSubsumedBySource => {
                        let dropped = &member_facts[i];
                        let kept = &member_facts[j];
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
                        let related =
                            self.compound_subtype_cached(&mut checker, members[j], members[i]);
                        related
                            && !dropped.is_opaque_under_bypass_eval
                            && !intersection_candidate_is_container
                            && !Self::has_unique_properties_cached(
                                &dropped.property_names,
                                &kept.property_names,
                            )
                            && !Self::intersection_drop_changes_modifiers(
                                &dropped.property_modifiers,
                                &kept.property_modifiers,
                            )
                            && !Self::has_index_signature_not_in(dropped, kept)
                            && !Self::is_branded_primitive_pair(dropped, kept)
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
        let budget_events_at_entry = checker.incomplete_evaluation_relation_event_count();
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
            && checker.incomplete_evaluation_relation_event_count() == budget_events_at_entry
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

    /// Compute the structural facts used by compound simplification vetoes for
    /// one member. This is intentionally per-call state, not an evaluator cache:
    /// the member list is small and the facts are only valid inside one type
    /// database/request shape.
    #[cfg(test)]
    fn compound_member_facts(
        &self,
        type_id: TypeId,
        include_property_modifiers: bool,
    ) -> CompoundMemberFacts {
        let mut memo = FxHashMap::default();
        self.compound_member_facts_memoized(type_id, include_property_modifiers, &mut memo)
    }

    fn compound_member_facts_memoized(
        &self,
        type_id: TypeId,
        include_property_modifiers: bool,
        memo: &mut FxHashMap<CompoundMemberFactsKey, CompoundMemberFacts>,
    ) -> CompoundMemberFacts {
        let key = CompoundMemberFactsKey {
            type_id,
            include_property_modifiers,
        };
        if let Some(facts) = memo.get(&key) {
            return facts.clone();
        }

        let mut names = FxHashSet::default();
        self.collect_property_names_memoized(type_id, &mut names, memo);
        let property_names = if names.is_empty() { None } else { Some(names) };

        let property_modifiers = if include_property_modifiers {
            let mut mods = FxHashMap::default();
            self.collect_property_modifiers_memoized(type_id, &mut mods, memo);
            if mods.is_empty() { None } else { Some(mods) }
        } else {
            None
        };

        let facts = CompoundMemberFacts {
            property_names,
            property_modifiers,
            carries_index_signature: Self::carries_index_signature(self.interner, type_id),
            is_empty_object_type: crate::visitors::visitor_predicates::is_empty_object_type(
                self.interner,
                type_id,
            ),
            is_intrinsic: type_id.is_intrinsic(),
            is_intrinsic_or_literal_type:
                crate::visitors::visitor_predicates::is_intrinsic_or_literal_type(
                    self.interner,
                    type_id,
                ),
            is_literal_type: crate::visitors::visitor_predicates::is_literal_type(
                self.interner,
                type_id,
            ),
            is_widening_primitive_intrinsic:
                crate::visitors::visitor_predicates::is_widening_primitive_intrinsic(
                    self.interner,
                    type_id,
                ),
            is_opaque_under_bypass_eval: Self::is_opaque_under_bypass_eval(self.interner, type_id),
            is_branded_primitive_intersection: Self::is_branded_primitive_intersection(
                self.interner,
                type_id,
            ),
        };
        memo.insert(key, facts.clone());
        facts
    }

    /// Check if `candidate` has any property names that `subsuming` doesn't have,
    /// using pre-computed property name sets to avoid repeated allocation.
    fn has_unique_properties_cached(
        candidate_names: &MemberPropertyNames,
        subsuming_names: &MemberPropertyNames,
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
    const fn union_member_removable_as_subtype(
        member: &CompoundMemberFacts,
        has_empty_object: bool,
    ) -> bool {
        if has_empty_object {
            return true;
        }
        if member.is_literal_type {
            return true;
        }
        // Reserved intrinsic TypeIds (`boolean`, `number`, `string`, `object`,
        // ...) are bare keyword types and stay protected. They are checked
        // explicitly because they do not always resolve through `lookup`.
        if member.is_intrinsic {
            return false;
        }
        // Bare intrinsic keyword types (non-literal) are protected from
        // object-subsumption removal; structured/instantiable members are not.
        !member.is_intrinsic_or_literal_type
    }

    /// Check whether a (candidate, subsuming) pair forms the branded-primitive
    /// idiom `string & {}` (or `number & {}`, `boolean & {}`, ...).
    const fn is_branded_primitive_pair(
        candidate: &CompoundMemberFacts,
        subsuming: &CompoundMemberFacts,
    ) -> bool {
        candidate.is_empty_object_type && subsuming.is_widening_primitive_intrinsic
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

    /// Whether an array/tuple reaches `any` through its element type(s).
    ///
    /// `contains_any_type` treats an array/tuple as an opaque reference and
    /// never descends into it, so walk the element types explicitly. Used to
    /// exempt an `any`-elemented container supertype from the redundant-member
    /// veto: tsc drops `[any]`/`any[]` from an intersection so a literal is
    /// contextually typed by the concrete member, whereas a concrete container
    /// like `(string | number)[]` is kept (#16095 vs
    /// `contextualTypeBasedOnIntersectionWithAnyInTheMix3`).
    fn container_element_contains_any(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        let element_reaches_any = |elem: TypeId| {
            crate::visitors::visitor_predicates::contains_any_type(db, elem)
                || Self::container_element_contains_any(db, elem)
        };
        match db.lookup(type_id) {
            Some(TypeData::Array(elem)) => element_reaches_any(elem),
            Some(TypeData::Tuple(list_id)) => db
                .tuple_list(list_id)
                .iter()
                .any(|e| element_reaches_any(e.type_id)),
            _ => false,
        }
    }

    /// Check whether a union member is a literal that's only "subsumed" by a
    /// branded-primitive intersection (`string & {}` and friends).
    const fn is_literal_under_branded_primitive(
        candidate: &CompoundMemberFacts,
        subsuming: &CompoundMemberFacts,
    ) -> bool {
        candidate.is_literal_type && subsuming.is_branded_primitive_intersection
    }

    fn is_branded_primitive_intersection(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) else {
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
    const fn has_index_signature_not_in(
        candidate: &CompoundMemberFacts,
        subsuming: &CompoundMemberFacts,
    ) -> bool {
        candidate.carries_index_signature && !subsuming.carries_index_signature
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
    fn collect_property_names_memoized(
        &self,
        type_id: TypeId,
        names: &mut FxHashSet<u32>,
        memo: &mut FxHashMap<CompoundMemberFactsKey, CompoundMemberFacts>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    names.insert(prop.name.0);
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                let sub_members = self.interner.type_list(list_id);
                for &sub in sub_members.iter() {
                    if let Some(sub_names) = &self
                        .compound_member_facts_memoized(sub, false, memo)
                        .property_names
                    {
                        names.extend(sub_names.iter().copied());
                    }
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
    fn collect_property_modifiers_memoized(
        &self,
        type_id: TypeId,
        mods: &mut FxHashMap<u32, (bool, bool)>,
        memo: &mut FxHashMap<CompoundMemberFactsKey, CompoundMemberFacts>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    let entry = mods
                        .entry(prop.name.0)
                        .or_insert((prop.optional, prop.readonly));
                    entry.0 = entry.0 && prop.optional;
                    entry.1 = entry.1 && prop.readonly;
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                for &sub in self.interner.type_list(list_id).iter() {
                    if let Some(sub_mods) = &self
                        .compound_member_facts_memoized(sub, true, memo)
                        .property_modifiers
                    {
                        Self::merge_property_modifiers(mods, sub_mods);
                    }
                }
            }
            _ => {}
        }
    }

    fn merge_property_modifiers(
        target: &mut FxHashMap<u32, (bool, bool)>,
        source: &FxHashMap<u32, (bool, bool)>,
    ) {
        for (&name, &(optional, readonly)) in source {
            let entry = target.entry(name).or_insert((optional, readonly));
            entry.0 = entry.0 && optional;
            entry.1 = entry.1 && readonly;
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
    use crate::def::DefId;
    use crate::evaluation::session::EvaluationSession;
    use crate::recursion::{MAX_SOLVER_STACK_FRAMES, try_enter_solver_frame};
    use crate::relations::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
    use crate::types::{IndexSignature, ObjectShape, PropertyInfo, TupleElement, TypeId};

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
        // Mirror the production reduction checker (see `remove_redundant_members`):
        // member removal only acts on definitive verdicts, so the probe key must
        // carry the same not-coinductive relation mode.
        checker.assume_related_on_cycle = false;
        checker.assume_related_on_depth = false;
        checker.max_depth = MAX_SUBTYPE_DEPTH;
        checker.exact_optional_property_types = exact_optional_property_types;
        checker
    }

    fn seed_session_compound_probe(
        session: &EvaluationSession,
        checker: &SubtypeChecker<'_>,
        source: TypeId,
        target: TypeId,
        result: bool,
    ) {
        session.compound_subtype_probe_put(
            CompoundSubtypePairKey::from_checker(checker, source, target),
            result,
        );
    }

    fn object_with_string_index(
        interner: &TypeInterner,
        prop_name: tsz_common::interner::Atom,
    ) -> TypeId {
        interner.object_with_index(ObjectShape {
            properties: vec![PropertyInfo::new(prop_name, TypeId::STRING)],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        })
    }

    fn tuple_elem(type_id: TypeId) -> TupleElement {
        TupleElement {
            type_id,
            name: None,
            optional: false,
            rest: false,
        }
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
    fn compound_member_facts_extract_object_array_opaque_and_brand_facts() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let indexed = object_with_string_index(&interner, value);
        let tuple = interner.tuple(vec![tuple_elem(TypeId::STRING)]);
        let array = interner.array(TypeId::NUMBER);
        let lazy = interner.lazy(DefId(1001));
        let application = interner.application(lazy, vec![TypeId::STRING]);
        let branded = interner.intersect_types_raw2(TypeId::STRING, interner.object(Vec::new()));
        let evaluator = TypeEvaluator::new(&interner);

        let indexed_facts = evaluator.compound_member_facts(indexed, true);
        assert!(indexed_facts.carries_index_signature);
        assert!(
            indexed_facts
                .property_names
                .as_ref()
                .is_some_and(|names| names.contains(&value.0)),
            "object facts include declared property names",
        );

        for member in [tuple, array] {
            let facts = evaluator.compound_member_facts(member, false);
            assert!(
                facts
                    .property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&u32::MAX)),
                "array-like members carry the property-name sentinel used by uniqueness vetoes",
            );
        }

        let application_facts = evaluator.compound_member_facts(application, false);
        assert!(application_facts.is_opaque_under_bypass_eval);

        let branded_facts = evaluator.compound_member_facts(branded, false);
        assert!(branded_facts.is_branded_primitive_intersection);

        let literal_facts =
            evaluator.compound_member_facts(interner.literal_string("token"), false);
        assert!(TypeEvaluator::<crate::relations::subtype::NoopResolver>::union_member_removable_as_subtype(
            &literal_facts,
            false,
        ));

        let primitive_facts = evaluator.compound_member_facts(TypeId::STRING, false);
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::union_member_removable_as_subtype(
                &primitive_facts,
                false,
            ),
            "bare primitive keywords stay protected without an empty-object member",
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::union_member_removable_as_subtype(
                &primitive_facts,
                true,
            ),
            "the empty-object union case keeps tsc's primitive absorption exception",
        );
    }

    #[test]
    fn compound_member_facts_merge_modifiers_without_evaluator_cache_state() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let mut optional_readonly = PropertyInfo::opt(value, TypeId::STRING);
        optional_readonly.readonly = true;
        let required_readonly =
            interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);
        let optional_readonly = interner.object(vec![optional_readonly]);
        let intersection = interner.intersect_types_raw2(required_readonly, optional_readonly);
        let evaluator = TypeEvaluator::new(&interner);
        let before = evaluator.cache_statistics();

        let facts = evaluator.compound_member_facts(intersection, true);

        assert_eq!(
            evaluator.cache_statistics(),
            before,
            "per-call member facts must not mutate evaluator cache statistics",
        );
        assert_eq!(
            facts
                .property_modifiers
                .as_ref()
                .and_then(|mods| mods.get(&value.0).copied()),
            Some((false, true)),
            "intersection modifier facts use tsc's AND-merge semantics",
        );

        let union_facts = evaluator.compound_member_facts(intersection, false);
        assert!(union_facts.property_modifiers.is_none());
    }

    #[test]
    fn compound_member_facts_memo_partitions_property_modifier_mode() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let mut optional_readonly = PropertyInfo::opt(value, TypeId::STRING);
        optional_readonly.readonly = true;
        let required = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);
        let optional_readonly = interner.object(vec![optional_readonly]);
        let intersection = interner.intersect_types_raw2(required, optional_readonly);
        let evaluator = TypeEvaluator::new(&interner);
        let mut memo = FxHashMap::default();

        let union_facts = evaluator.compound_member_facts_memoized(intersection, false, &mut memo);

        assert!(union_facts.property_modifiers.is_none());
        assert!(memo.contains_key(&CompoundMemberFactsKey {
            type_id: intersection,
            include_property_modifiers: false,
        }));
        assert!(!memo.contains_key(&CompoundMemberFactsKey {
            type_id: intersection,
            include_property_modifiers: true,
        }));

        let intersection_facts =
            evaluator.compound_member_facts_memoized(intersection, true, &mut memo);

        assert_eq!(
            intersection_facts
                .property_modifiers
                .as_ref()
                .and_then(|mods| mods.get(&value.0).copied()),
            Some((false, true)),
            "modifier-sensitive facts must not reuse the union-mode memo entry",
        );
        assert!(memo.contains_key(&CompoundMemberFactsKey {
            type_id: intersection,
            include_property_modifiers: true,
        }));

        let entries_after_both_modes = memo.len();
        assert_eq!(
            evaluator.compound_member_facts_memoized(intersection, false, &mut memo),
            union_facts,
        );
        assert_eq!(
            memo.len(),
            entries_after_both_modes,
            "re-reading the same mode should hit the local memo",
        );
    }

    #[test]
    fn compound_member_facts_memo_reuses_shared_intersection_children() {
        let interner = TypeInterner::new();
        let shared_name = interner.intern_string("shared");
        let left_name = interner.intern_string("left");
        let shared = interner.object(vec![PropertyInfo::readonly(shared_name, TypeId::STRING)]);
        let left_only = interner.object(vec![PropertyInfo::new(left_name, TypeId::NUMBER)]);
        let tuple = interner.tuple(vec![tuple_elem(TypeId::BOOLEAN)]);
        let left = interner.intersect_types_raw2(shared, left_only);
        let right = interner.intersect_types_raw2(shared, tuple);
        let evaluator = TypeEvaluator::new(&interner);
        let mut memo = FxHashMap::default();

        let left_facts = evaluator.compound_member_facts_memoized(left, true, &mut memo);
        let entries_after_left = memo.len();
        let right_facts = evaluator.compound_member_facts_memoized(right, true, &mut memo);

        assert!(
            left_facts
                .property_names
                .as_ref()
                .is_some_and(|names| names.contains(&shared_name.0) && names.contains(&left_name.0)),
            "intersection facts merge object property names",
        );
        assert!(
            right_facts
                .property_names
                .as_ref()
                .is_some_and(|names| names.contains(&shared_name.0) && names.contains(&u32::MAX)),
            "array-like sentinel facts are preserved while sharing object children",
        );
        assert_eq!(
            right_facts
                .property_modifiers
                .as_ref()
                .and_then(|mods| mods.get(&shared_name.0).copied()),
            Some((false, true)),
            "shared child modifier facts stay available in intersection mode",
        );
        assert_eq!(
            memo.len(),
            entries_after_left + 3,
            "the second intersection adds only its unique child facts and top-level facts",
        );

        let entries_after_right = memo.len();
        assert_eq!(
            evaluator.compound_member_facts_memoized(right, true, &mut memo),
            right_facts,
        );
        assert_eq!(
            memo.len(),
            entries_after_right,
            "re-reading a top-level intersection should hit the local memo",
        );
    }

    #[test]
    fn compound_member_facts_keep_index_signature_veto_after_session_subtype_hit() {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("value");
        let with_index = object_with_string_index(&interner, prop);
        let without_index = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);
        let checker = compound_probe_checker(&interner, false);
        let session = EvaluationSession::new();
        seed_session_compound_probe(&session, &checker, with_index, without_index, true);
        seed_session_compound_probe(&session, &checker, without_index, with_index, true);

        let mut members = vec![with_index, without_index];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_union_members(&mut members);

        assert_eq!(
            members,
            vec![with_index],
            "a session raw-subtype hit must still rerun the index-signature removal veto",
        );
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "session-backed subtype probes should not duplicate evaluator-local relation entries",
        );
    }

    #[test]
    fn compound_member_facts_keep_branded_literal_veto_after_session_subtype_hit() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("token");
        let empty = interner.object(Vec::new());
        let branded_string = interner.intersect_types_raw2(TypeId::STRING, empty);
        let branded_number = interner.intersect_types_raw2(TypeId::NUMBER, empty);
        let checker = compound_probe_checker(&interner, false);
        let session = EvaluationSession::new();
        for &brand in &[branded_string, branded_number] {
            seed_session_compound_probe(&session, &checker, literal, brand, true);
            seed_session_compound_probe(&session, &checker, brand, literal, false);
        }
        seed_session_compound_probe(&session, &checker, branded_string, branded_number, false);
        seed_session_compound_probe(&session, &checker, branded_number, branded_string, false);

        let mut members = vec![literal, branded_string, branded_number];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_union_members(&mut members);

        assert_eq!(
            members,
            vec![literal, branded_string, branded_number],
            "cached raw subtype hits must not let branded-primitive vetoes drop literal members",
        );
    }

    #[test]
    fn compound_member_facts_keep_opaque_intersection_veto_after_session_subtype_hit() {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("path");
        let opaque = interner.application(interner.lazy(DefId(2002)), vec![TypeId::STRING]);
        let concrete = interner.object(vec![PropertyInfo::opt(prop, TypeId::STRING)]);
        let checker = compound_probe_checker(&interner, false);
        let session = EvaluationSession::new();
        seed_session_compound_probe(&session, &checker, concrete, opaque, true);
        seed_session_compound_probe(&session, &checker, opaque, concrete, false);

        let mut members = vec![opaque, concrete];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_intersection_members(&mut members);

        assert_eq!(
            members,
            vec![opaque, concrete],
            "a cached raw-subtype hit must still let opaque Application/Lazy members veto removal",
        );
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "session-backed subtype probes should not duplicate evaluator-local relation entries",
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
    fn compound_subtype_cache_skips_shared_budget_failure() {
        crate::limits::reset_subtype_thread_local_state();
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let extra = interner.intern_string("extra");
        let source = interner.object(vec![
            PropertyInfo::new(value, TypeId::STRING),
            PropertyInfo::new(extra, TypeId::NUMBER),
        ]);
        let target = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);
        let mut checker = compound_probe_checker(&interner, false);
        let mut evaluator = TypeEvaluator::new(&interner);

        let mut held_frames = Vec::new();
        for _ in 0..MAX_SOLVER_STACK_FRAMES {
            held_frames.push(try_enter_solver_frame().expect("solver frame budget has headroom"));
        }
        assert!(!evaluator.compound_subtype_cached(&mut checker, source, target));
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "a strict shared-budget failure must not enter the compound memo",
        );

        drop(held_frames);
        crate::limits::reset_subtype_thread_local_state();
        checker.reset();
        assert!(evaluator.compound_subtype_cached(&mut checker, source, target));
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            1,
            "a fresh-budget structural proof should be memoized",
        );
        crate::limits::reset_subtype_thread_local_state();
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
