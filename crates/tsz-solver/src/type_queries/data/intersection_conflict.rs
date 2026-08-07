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
    // Where each name was *first written*, so a multi-conflict shape names the
    // property `tsc` names. `tsc` always reports the first-written conflicting
    // property (intersection-member order, then source declaration order within
    // a member), independent of which property was accessed — never whichever
    // name a hashmap happens to iterate first. `PropertyInfo::declaration_order`
    // still carries source order here even though the interned shape's
    // `properties` are sorted by atom id for canonical hashing, so the ordering
    // key survives the shape sort. Members are walked in written order, and a
    // name's position is recorded at its first occurrence only.
    let mut first_written: FxHashMap<Atom, (usize, u32)> = FxHashMap::default();
    let mut ingest = |member_idx: usize, properties: &[crate::types::PropertyInfo]| {
        for prop in properties {
            if prop.optional || excluded.contains(&prop.name) {
                continue;
            }
            first_written
                .entry(prop.name)
                .or_insert((member_idx, prop.declaration_order));
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
    for (member_idx, &member) in members.iter().enumerate() {
        if member.is_intrinsic() {
            continue;
        }
        match db.lookup(member) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                ingest(member_idx, &db.object_shape(shape_id).properties);
            }
            Some(TypeData::Callable(callable_id)) => {
                ingest(member_idx, &db.callable_shape(callable_id).properties);
            }
            _ => {}
        }
    }
    by_name
        .into_iter()
        .filter(|(_, values)| values.len() >= 2)
        .map(|(name, _)| name)
        .min_by_key(|name| {
            first_written
                .get(name)
                .copied()
                .unwrap_or((usize::MAX, u32::MAX))
        })
}
