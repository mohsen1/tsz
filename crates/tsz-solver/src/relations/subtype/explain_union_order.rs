//! Ordering helpers for union-source failure elaboration.
//!
//! `SubtypeChecker::explain_failure_guarded`'s union-source arm needs the
//! union's members in the order `tsc`'s relation walk would examine them, to
//! pick the same "first failing member" `tsc` elaborates beneath the
//! union-to-target line. The interner's own stored member order is a
//! *canonicalization* order (needed so `A | B` and `B | A` intern to one
//! `TypeId`), not the display/source-declaration order. The arm reconciles the
//! two by ranking the members through the display comparator
//! (`order_union_members_for_display`), seeded with the as-written source order
//! from [`SubtypeChecker::union_source_elaboration_origin_override`] when the
//! interner recorded one, and then applying [`reorder_union_members_nullish_first`]
//! on top — `tsc`'s relation walk visits the nullish intrinsics first even
//! though the display shows them last.

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::SubtypeChecker;
use crate::types::TypeId;

/// Hoist the nullish intrinsics to the front of an elaboration member list.
///
/// `eachTypeRelatedToType` walks `source.types` in ascending type-id order, and
/// the intrinsic `undefined` and `null` types are allocated before any user
/// type — so tsc examines them first and elaborates a failing nullish member
/// ahead of the others. The display order the arm ranks members into
/// (`order_union_members_for_display`) instead places `undefined`/`null` last
/// (tsc's *printer* order), so this restores the relation order used to pick the
/// first failing member: `undefined`, then `null`, then the remaining members in
/// the order they were given.
pub(super) fn reorder_union_members_nullish_first(members: &[TypeId]) -> Vec<TypeId> {
    let mut ordered = Vec::with_capacity(members.len());
    // `undefined` before `null` (their intrinsic allocation order), then the
    // remaining members in their stored order.
    for nullish in [TypeId::UNDEFINED, TypeId::NULL] {
        if members.contains(&nullish) {
            ordered.push(nullish);
        }
    }
    ordered.extend(
        members
            .iter()
            .copied()
            .filter(|&m| m != TypeId::UNDEFINED && m != TypeId::NULL),
    );
    ordered
}

/// Whether `a` and `b` hold the same multiset of `TypeId`s (same length, and
/// every value appears the same number of times). `TypeId` is only
/// `Hash + Eq`, so the check counts occurrences rather than sorting.
fn is_pure_permutation(a: &[TypeId], b: &[TypeId]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut counts: rustc_hash::FxHashMap<TypeId, isize> = rustc_hash::FxHashMap::default();
    for &member in a {
        *counts.entry(member).or_default() += 1;
    }
    for &member in b {
        *counts.entry(member).or_default() -= 1;
    }
    counts.values().all(|&count| count == 0)
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// As-written declaration order for a union failure elaboration, when the
    /// interner recorded one that should override the canonical member order.
    ///
    /// `tsc` walks a union's members in source (as-written) order on BOTH
    /// sides of a failed relation: a union *source* elaborates the first
    /// failing member beneath the union-to-target line in written order, and a
    /// union *target*'s best-member selection (`getBestMatchingType` →
    /// `findMostOverlappyType`, ties to the LAST member) scans `target.types`
    /// in written order too. tsz's interner stores members in a
    /// canonicalization order (by `ShapeId` / allocation identity) so that
    /// `A | B` and `B | A` share one `TypeId`; for anonymous object members
    /// that canonical order can diverge from source order — a written
    /// `{ a: string } | { b: number }` interned `{ b: number }`-first when its
    /// shape was content-interned first (#16965), or an instantiated generic
    /// union (`GU<1>`) whose substituted arm re-interns with a fresh `ShapeId`
    /// and sorts away from its declared position — so a raw interned walk
    /// names the wrong constituent even though the union *header* already
    /// prints source order.
    ///
    /// The interner records the as-written order in the `union_origin` side
    /// table (`get_union_origin`) precisely when canonical and source order
    /// disagree — the same table the printer's header uses. Returns `Some` of
    /// that order only when it is a *pure reordering* of the interned member set
    /// (same length and same multiset of `TypeId`s); the caller then walks it
    /// instead of the interned order. Returns `None` — walk the interned order —
    /// when no origin was recorded (canonical already matches source order) or
    /// when the recorded origin is a flatten (`T | null` where `T` is a union
    /// alias, which carries more members) or an anonymous-object duplicate
    /// collapse (`{ m } | { m }` deduped to one interned member, #14344), whose
    /// member set differs from the interned union. This is display-only: the
    /// order is the same multiset either way, so the relation outcome and the
    /// elaborated member's failure reason are unaffected — only which failing
    /// constituent is named changes.
    pub(in crate::relations::subtype) fn union_elaboration_origin_override(
        &self,
        union_type_id: TypeId,
        interned_members: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        union_declared_order_override(self.interner, union_type_id, interned_members)
    }
}

/// Free-function form of
/// [`SubtypeChecker::union_elaboration_origin_override`] for callers outside a
/// relation walk — the per-property elaboration boundary
/// ([`union_target_best_elaboration_member`]) restores its best-member scan
/// order through this.
///
/// [`union_target_best_elaboration_member`]: crate::relations::subtype::union_target_best_elaboration_member
pub fn union_declared_order_override(
    interner: &dyn crate::construction::TypeDatabase,
    union_type_id: TypeId,
    interned_members: &[TypeId],
) -> Option<Vec<TypeId>> {
    interner
        .get_union_origin(union_type_id)
        .filter(|origin| is_pure_permutation(origin.as_ref(), interned_members))
        .map(|origin| origin.as_ref().clone())
}
