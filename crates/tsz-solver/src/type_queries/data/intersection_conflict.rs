//! Conflict detection over a *written* (pre-reduction) intersection member
//! list, used only for diagnostic elaboration — see
//! [`find_disjoint_literal_property_across_intersection`] and
//! [`find_private_brand_conflicting_property_across_intersection`].

use crate::construction::TypeDatabase;
use crate::types::TypeData;
use crate::types::TypeId;
use crate::types::Visibility;
use crate::utils::is_synthetic_private_brand_name;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
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

/// The property name whose required occurrences across `members` come from
/// two or more distinct declaring classes with at least one `private`
/// occurrence — the shape `tsc` reports as `TS18032` (`The intersection
/// '{0}' was reduced to 'never' because property '{1}' exists in multiple
/// constituents and is private in some.`), the private-brand sibling of
/// [`find_disjoint_literal_property_across_intersection`]'s literal-conflict
/// `TS18031`.
///
/// Declaring-class identity is `PropertyInfo::parent_id`: class-shape
/// construction only rewrites a member's `parent_id` to the owning class
/// when that member is *own* (newly declared or overriding), so a property
/// inherited unchanged through a shared base class keeps the base's
/// `parent_id` in every subclass and is correctly seen as one occurrence,
/// not a conflict (`class A extends Base {}` / `class B extends Base {}` /
/// `A & B` does not reduce to `never`, matching `tsc`).
///
/// Like its literal-conflict sibling, this is display-only and deliberately
/// narrow: it does not re-derive whether `members` actually reduces to
/// `never` (the caller already knows that from the receiver's `TypeId`), it
/// only names the first conflicting property, by source declaration order,
/// for the elaboration line. Returning `None` leaves today's plain `TS2339`
/// exactly as it is, never a wrong message.
pub fn find_private_brand_conflicting_property_across_intersection(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Atom> {
    struct Entry {
        parents: FxHashSet<Option<SymbolId>>,
        saw_private: bool,
        min_declaration_order: u32,
    }

    let mut by_name: FxHashMap<Atom, Entry> = FxHashMap::default();
    let mut ingest = |properties: &[crate::types::PropertyInfo]| {
        for prop in properties {
            if is_synthetic_private_brand_name(&db.resolve_atom(prop.name)) {
                continue;
            }
            let entry = by_name.entry(prop.name).or_insert_with(|| Entry {
                parents: FxHashSet::default(),
                saw_private: false,
                min_declaration_order: prop.declaration_order,
            });
            entry.parents.insert(prop.parent_id);
            entry.saw_private |= prop.visibility == Visibility::Private;
            entry.min_declaration_order = entry.min_declaration_order.min(prop.declaration_order);
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
        .filter(|(_, entry)| entry.saw_private && entry.parents.len() >= 2)
        .min_by_key(|(_, entry)| entry.min_declaration_order)
        .map(|(name, _)| name)
}
