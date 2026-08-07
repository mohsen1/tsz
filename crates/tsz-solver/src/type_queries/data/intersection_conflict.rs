//! Conflict detection over a *written* (pre-reduction) intersection member
//! list, used only for diagnostic elaboration — see
//! [`find_disjoint_literal_property_across_intersection`].

use crate::construction::TypeDatabase;
use crate::types::TypeData;
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

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
    let mut ingest = |properties: &[crate::types::PropertyInfo]| {
        for prop in properties {
            if prop.optional || excluded.contains(&prop.name) {
                continue;
            }
            let Some(TypeData::Literal(value)) = db.lookup(prop.type_id) else {
                // A non-literal (or absent) required occurrence could still
                // conflict via the fuller `TypeInterner`-internal analysis,
                // but this helper only recognizes the single-literal shape —
                // drop the whole name rather than guess at a partial match.
                excluded.insert(prop.name);
                by_name.remove(&prop.name);
                continue;
            };
            by_name.entry(prop.name).or_default().insert(value);
        }
    };
    for &member in members {
        if member.is_intrinsic() {
            continue;
        }
        match db.lookup(member) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                ingest(&db.object_shape(shape_id).properties);
            }
            Some(TypeData::Callable(callable_id)) => {
                ingest(&db.callable_shape(callable_id).properties);
            }
            _ => {}
        }
    }
    by_name
        .into_iter()
        .find(|(_, values)| values.len() >= 2)
        .map(|(name, _)| name)
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
pub fn find_private_brand_conflict_property(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Atom> {
    let mut occurrences: FxHashMap<Atom, (u32, bool)> = FxHashMap::default();
    let mut ingest = |properties: &[crate::types::PropertyInfo]| {
        for prop in properties {
            let name = db.resolve_atom(prop.name);
            if name.starts_with("__private_brand_") {
                continue;
            }
            let entry = occurrences.entry(prop.name).or_insert((0, false));
            entry.0 += 1;
            if prop.visibility == crate::types::Visibility::Private {
                entry.1 = true;
            }
        }
    };
    for &member in members {
        if member.is_intrinsic() {
            continue;
        }
        match db.lookup(member) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                ingest(&db.object_shape(shape_id).properties);
            }
            Some(TypeData::Callable(callable_id)) => {
                ingest(&db.callable_shape(callable_id).properties);
            }
            _ => {}
        }
    }
    occurrences
        .into_iter()
        .find(|(_, (count, saw_private))| *count >= 2 && *saw_private)
        .map(|(name, _)| name)
}
