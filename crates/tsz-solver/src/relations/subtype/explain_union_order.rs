//! Ordering helpers for union-source failure elaboration.
//!
//! `SubtypeChecker::explain_failure_guarded`'s union-source arm needs the
//! union's members in the order `tsc`'s relation walk would examine them, to
//! pick the same "first failing member" `tsc` elaborates beneath the
//! union-to-target line. The interner's own stored member order is a
//! *canonicalization* order (needed so `A | B` and `B | A` intern to one
//! `TypeId`), not the source-declaration order — these two reorders bridge
//! that gap without touching the interned identity itself.

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::SubtypeChecker;
use crate::types::TypeId;

/// Reorder a source union's members into tsc's relation-iteration order for
/// error elaboration.
///
/// `eachTypeRelatedToType` walks `source.types` in ascending type-id order, and
/// the intrinsic `undefined` and `null` types are allocated before any user
/// type — so tsc examines them first and elaborates a failing nullish member
/// ahead of the others. tsz stores union members with `undefined`/`null` last
/// (that is tsc's *printer* order, keyed by `builtin_sort_key`), so this
/// restores the relation order used to pick the first failing member:
/// `undefined`, then `null`, then the remaining members unchanged. Non-nullish
/// unions keep their stored order, which matches tsc's relation order except
/// for same-rank enum members — see [`SubtypeChecker::reorder_enum_members_by_declaration`].
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

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Reorder same-rank enum members of a union-source elaboration list into
    /// declaration order, in place.
    ///
    /// `sort_union_members` (the interner's canonicalization pass) orders a
    /// union's stored member list by allocation identity so that `E1 | E2`
    /// and `E2 | E1` intern to one canonical `TypeId` — a `DefId`/`TypeId` is
    /// allocated lazily, in whatever order the checker first requests an
    /// enum's type, which does not track source position. `tsc` always
    /// elaborates the union's first-*declared* failing member, and the
    /// printer's own tie-break for same-rank union members keeps enum
    /// members in declaration order (`order_union_members_by_source`'s
    /// `compare_union_member_names` exception for enums). Without this, the
    /// elaboration line can name the wrong enum entirely — not just the wrong
    /// order, but a sibling enum with an unrelated declaration (#16513).
    ///
    /// Only the slots already holding an enum member are touched: their
    /// values are reassigned by ascending `(file_id, span_start)`, leaving
    /// every other member's position — and the nullish-first order already
    /// applied — untouched. A union with 0 or 1 enum members is a no-op.
    pub(in crate::relations::subtype) fn reorder_enum_members_by_declaration(
        &self,
        members: &mut [TypeId],
    ) {
        let Some(def_store) = self
            .query_db
            .and_then(|db| db.definition_store_for_inference())
        else {
            return;
        };
        let mut enum_slots: Vec<usize> = Vec::new();
        let mut keyed: Vec<((u32, u32), TypeId)> = Vec::new();
        for (idx, &member) in members.iter().enumerate() {
            let Some(def_id) = crate::type_queries::get_enum_def_id(self.interner, member) else {
                continue;
            };
            let Some(def) = def_store.get(def_id) else {
                continue;
            };
            let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span) else {
                continue;
            };
            enum_slots.push(idx);
            keyed.push(((file_id, span_start), member));
        }
        if keyed.len() < 2 {
            return;
        }
        keyed.sort_by_key(|&(pos, _)| pos);
        for (&idx, &(_, member)) in enum_slots.iter().zip(keyed.iter()) {
            members[idx] = member;
        }
    }
}
