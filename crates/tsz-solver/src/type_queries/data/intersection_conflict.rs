//! Conflict detection over a *written* (pre-reduction) intersection member
//! list, used only for diagnostic elaboration — see
//! [`find_disjoint_literal_property_across_intersection`].

use crate::construction::TypeDatabase;
use crate::types::TypeData;
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

/// Invoke `visit` once per property of the intersection's object/callable
/// members, in *combined declaration order*: members left to right, and within
/// each member its own properties sorted by `declaration_order` (since
/// `ObjectShape::properties` is `Atom`-id-sorted for canonical hashing and so
/// does not preserve source order). Intrinsic and non-object members carry no
/// properties and are skipped.
///
/// This owns the ordered member-walk that both the `TS18031` and `TS18032`
/// elaboration queries fold into their own first-seen accumulators — the
/// combined declaration order is the property `tsc`'s `elaborateNeverIntersection`
/// depends on for both diagnostics, so it lives in one place rather than being
/// re-established identically in each query.
fn for_each_member_property_in_declaration_order(
    db: &dyn TypeDatabase,
    members: &[TypeId],
    mut visit: impl FnMut(&crate::types::PropertyInfo),
) {
    let mut visit_member = |properties: &[crate::types::PropertyInfo]| {
        let mut ordered: Vec<&crate::types::PropertyInfo> = properties.iter().collect();
        ordered.sort_by_key(|prop| prop.declaration_order);
        for prop in ordered {
            visit(prop);
        }
    };
    for &member in members {
        if member.is_intrinsic() {
            continue;
        }
        match db.lookup(member) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                visit_member(&db.object_shape(shape_id).properties);
            }
            Some(TypeData::Callable(callable_id)) => {
                visit_member(&db.callable_shape(callable_id).properties);
            }
            _ => {}
        }
    }
}

/// The property name whose required occurrences across `members` are literal
/// values from mutually exclusive value-sets — the shape `tsc` reports as
/// `TS18031` (`The intersection '{0}' was reduced to 'never' because
/// property '{1}' has conflicting types in some constituents.`).
///
/// `members` is the *pre-reduction* member list of a written intersection
/// (e.g. recovered from source syntax via
/// `declared_intersection_annotation_display_for_expression`'s sibling walk),
/// since `TypeInterner::intern` already collapses a conflicting intersection
/// to the single canonical `TypeId::NEVER` at construction time — by the time
/// a checker query sees `NEVER` there is no member list left to inspect.
///
/// This intentionally covers only the common single-literal-per-member
/// discriminant shape (`{ kind: "a" } & { kind: "b" }`), mirroring the
/// `TypeInterner`-internal `intersection_has_disjoint_object_literals`'s
/// single-literal fast path rather than its full cross-domain/optional
/// analysis. It is a display-only helper consumed for diagnostic elaboration:
/// returning `None` for a conflict shape it doesn't recognize leaves the
/// primary `TS2339` diagnostic exactly as it is today (no elaboration line),
/// never a wrong one, so under-covering here is safe.
///
/// When more than one property name conflicts, tsc always names the first
/// one by a combined declaration order: walk `members` left to right, and
/// within each member walk its own properties in declaration order, adding
/// each name to the combined order only the first time it is seen (a
/// property introduced by a later member is ordered after every name the
/// earlier members already declared, even ones that didn't conflict).
/// Oracle-verified against `typescript@7.0.2`: the picked property tracks
/// this combined order regardless of which property was actually accessed,
/// and is independent of alphabetical order or which operand later members
/// re-declare the name in. `ObjectShape::properties` itself is sorted by
/// interned `Atom` id for canonical hashing identity, so this reads each
/// property's own `declaration_order` field (excluded from hashing, backfilled
/// by the interner from source insertion order) rather than `Vec` position.
pub fn find_disjoint_literal_property_across_intersection(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Atom> {
    let mut by_name: FxHashMap<Atom, FxHashSet<crate::types::LiteralValue>> = FxHashMap::default();
    // Names disqualified by a non-literal required occurrence, tracked
    // separately from `by_name` so a disqualification is permanent
    // regardless of whether it is observed before or after a literal
    // occurrence of the same name.
    let mut excluded: FxHashSet<Atom> = FxHashSet::default();
    let mut order: Vec<Atom> = Vec::new();
    let mut seen: FxHashSet<Atom> = FxHashSet::default();
    let ingest = |prop: &crate::types::PropertyInfo| {
        if seen.insert(prop.name) {
            order.push(prop.name);
        }
        if prop.optional || excluded.contains(&prop.name) {
            return;
        }
        let Some(TypeData::Literal(value)) = db.lookup(prop.type_id) else {
            // A non-literal (or absent) required occurrence could still
            // conflict via the fuller `TypeInterner`-internal analysis,
            // but this helper only recognizes the single-literal shape —
            // drop the whole name rather than guess at a partial match.
            excluded.insert(prop.name);
            by_name.remove(&prop.name);
            return;
        };
        by_name.entry(prop.name).or_default().insert(value);
    };
    for_each_member_property_in_declaration_order(db, members, ingest);
    order
        .into_iter()
        .find(|name| by_name.get(name).is_some_and(|values| values.len() >= 2))
}

/// The property name responsible for `tsc`'s `TS18032` elaboration (`The
/// intersection '{0}' was reduced to 'never' because property '{1}' exists
/// in multiple constituents and is private in some.`): a name declared by
/// two or more `members`, where at least one declaration is modifier-`private`.
///
/// Oracle-verified (`typescript@7.0.2`) that this fires even when only ONE
/// side is `private` and the other is `public` with an identical type — the
/// conflict is about the private member's nominal brand, not about the
/// property's structural type, so no type-compatibility check is needed
/// here (unlike [`find_disjoint_literal_property_across_intersection`]'s
/// literal-value comparison). Mirrors that function's scope discipline:
/// under-covering (returning `None`) just leaves the diagnostic as it is
/// today, so a name this helper doesn't recognize is safe, never wrong.
///
/// Deliberately excludes ES `#`-private names
/// ([`crate::utils::is_es_private_identifier_name`]): unlike modifier-
/// `private`, two classes' `#x` fields are lexically scoped to their own
/// class body and are never the same name to `tsc` even when they share
/// identical source text, so `#x` on `A` and `#x` on `B` is not a naming
/// collision at all — tsc reports no elaboration (and, correctly, no
/// `never` reduction) for that shape. Matching on the interned `Atom`'s
/// text alone (as the modifier-`private` case does) would wrongly treat
/// same-spelled `#`-private fields from different classes as one
/// conflicting name.
///
/// When more than one property name qualifies (each declared in two or more
/// members with at least one modifier-`private` occurrence), tsc names the
/// first one by the same combined declaration order as
/// [`find_disjoint_literal_property_across_intersection`]: both diagnostics
/// are produced by `elaborateNeverIntersection`, which walks
/// `getPropertiesOfUnionOrIntersectionType` — members left to right, each
/// member's own properties in declaration order, positioning each name at its
/// *first* occurrence across all members — and returns the first match. So
/// this reads each property's `declaration_order` field (excluded from
/// hashing, backfilled by the interner from source insertion order) rather
/// than `ObjectShape::properties`'s `Atom`-id-sorted `Vec` position, which
/// does not recover written order.
pub fn find_private_brand_conflict_property(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Atom> {
    let mut occurrences: FxHashMap<Atom, (u32, bool)> = FxHashMap::default();
    let mut order: Vec<Atom> = Vec::new();
    let mut seen: FxHashSet<Atom> = FxHashSet::default();
    let ingest = |prop: &crate::types::PropertyInfo| {
        let name = db.resolve_atom(prop.name);
        if name.starts_with("__private_brand_")
            || crate::utils::is_es_private_identifier_name(&name)
        {
            return;
        }
        if seen.insert(prop.name) {
            order.push(prop.name);
        }
        let entry = occurrences.entry(prop.name).or_insert((0, false));
        entry.0 += 1;
        if prop.visibility == crate::types::Visibility::Private {
            entry.1 = true;
        }
    };
    for_each_member_property_in_declaration_order(db, members, ingest);
    order.into_iter().find(|name| {
        occurrences
            .get(name)
            .is_some_and(|(count, saw_private)| *count >= 2 && *saw_private)
    })
}
