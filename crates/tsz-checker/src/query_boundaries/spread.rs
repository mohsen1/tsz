//! Solver-boundary helpers used by the object-spread collector.
//!
//! Thin wrappers that keep checker code from inspecting solver internals
//! directly; the architecture contract requires solver-internal types to
//! be reached only through `query_boundaries/`.

use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_solver::{DefId, PropertyInfo, TypeDatabase, TypeId};

pub(crate) fn unresolved_type_name_atom(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::visitor::unresolved_type_name_atom(db, type_id)
}

pub(crate) fn make_application(db: &dyn TypeDatabase, base: TypeId, args: Vec<TypeId>) -> TypeId {
    db.application(base, args)
}

pub(crate) fn make_lazy(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn make_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn contains_unresolved_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::contains_unresolved_application(types, type_id)
}

// ---------------------------------------------------------------------------
// Spread property merge semantics
// ---------------------------------------------------------------------------

/// Merge a single spread-contributed property into a running property map.
///
/// Structural rule: tsc's spread merge is asymmetric on the *later*
/// property's optionality:
/// - **Required later**: fully overrides the earlier contribution.
/// - **Optional later**: the earlier value still applies at runtime when the
///   spread source omits the key.  The merged read type is `union(earlier,
///   later)`; the merged write type is `union(earlier_write, later_write)`;
///   optionality is required iff *either* contributor was required; readonly
///   is the intersection (`earlier.readonly && prop.readonly`).
///
/// When `exact_optional_property_types` is `false` and the later property is
/// optional while the earlier is required, `undefined` is stripped from the
/// later property's type before the union (legacy tsc compatibility rule).
pub(crate) fn merge_spread_property_into_map(
    db: &dyn TypeDatabase,
    exact_optional_property_types: bool,
    properties: &mut FxHashMap<Atom, PropertyInfo>,
    prop: &PropertyInfo,
) {
    use std::collections::hash_map::Entry;
    match properties.entry(prop.name) {
        Entry::Vacant(slot) => {
            slot.insert(prop.clone());
        }
        Entry::Occupied(mut slot) => {
            if prop.optional {
                let e = slot.get();
                let (e_type, e_write, e_optional, e_readonly) =
                    (e.type_id, e.write_type, e.optional, e.readonly);
                let (spread_type, spread_write_type) =
                    if !exact_optional_property_types && !e_optional {
                        (
                            crate::query_boundaries::common::remove_undefined(db, prop.type_id),
                            crate::query_boundaries::common::remove_undefined(db, prop.write_type),
                        )
                    } else {
                        (prop.type_id, prop.write_type)
                    };
                let merged_type = db.union2(e_type, spread_type);
                let merged_write = db.union2(e_write, spread_write_type);
                slot.insert(PropertyInfo {
                    type_id: merged_type,
                    write_type: merged_write,
                    optional: e_optional && prop.optional,
                    readonly: e_readonly && prop.readonly,
                    is_class_prototype: false,
                    ..prop.clone()
                });
            } else {
                slot.insert(prop.clone());
            }
        }
    }
}

/// Assign display-stable ordering to a set of spread-contributed properties.
///
/// Sorts by original `declaration_order` and rebases onto `[base, base+N)` so
/// display order within a spread group is stable.
pub(crate) fn rebase_spread_display_property_order(
    mut props: Vec<PropertyInfo>,
    base: u32,
) -> Vec<PropertyInfo> {
    props.sort_by_key(|prop| prop.declaration_order);
    for (index, prop) in props.iter_mut().enumerate() {
        prop.declaration_order = base.saturating_add(index as u32);
    }
    props
}

/// Remove synthetic `p?: undefined` placeholder properties from union-spread
/// branches when another branch contributes the same property as required.
///
/// Conditional object literal unions use `p?: undefined` placeholders for
/// display balance; in an object spread context they must not materialize as
/// spread properties when another branch supplies a required `p`.
pub(crate) fn remove_synthetic_missing_union_spread_props(member_props: &mut [Vec<PropertyInfo>]) {
    let capacity = member_props.iter().map(|v| v.len()).sum();
    let mut required_names =
        rustc_hash::FxHashSet::with_capacity_and_hasher(capacity, Default::default());
    for props in member_props.iter() {
        for prop in props {
            if !prop.optional {
                required_names.insert(prop.name);
            }
        }
    }
    for props in member_props {
        props.retain(|prop| {
            !(prop.optional
                && prop.type_id == TypeId::UNDEFINED
                && required_names.contains(&prop.name))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::{TypeInterner, Visibility};

    fn make_prop(db: &TypeInterner, name: &str, ty: TypeId, optional: bool) -> PropertyInfo {
        PropertyInfo {
            name: db.intern_string(name),
            type_id: ty,
            write_type: ty,
            optional,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: true,
            is_symbol_named: false,
            single_quoted_name: false,
        }
    }

    #[test]
    fn merge_spread_vacant_entry_inserts_directly() {
        let db = TypeInterner::new();
        let mut map: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let prop = make_prop(&db, "x", TypeId::STRING, false);
        merge_spread_property_into_map(&db, false, &mut map, &prop);
        assert_eq!(map.len(), 1);
        let stored = map.get(&prop.name).unwrap();
        assert_eq!(stored.type_id, TypeId::STRING);
        assert!(!stored.optional);
    }

    #[test]
    fn merge_spread_required_later_overrides_earlier() {
        let db = TypeInterner::new();
        let mut map: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let earlier = make_prop(&db, "x", TypeId::STRING, false);
        let later = make_prop(&db, "x", TypeId::NUMBER, false);
        merge_spread_property_into_map(&db, false, &mut map, &earlier);
        merge_spread_property_into_map(&db, false, &mut map, &later);
        let stored = map.get(&earlier.name).unwrap();
        assert_eq!(stored.type_id, TypeId::NUMBER);
        assert!(!stored.optional);
    }

    #[test]
    fn merge_spread_optional_later_unions_with_required_earlier_strips_undefined() {
        let db = TypeInterner::new();
        let mut map: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let earlier = make_prop(&db, "x", TypeId::STRING, false);
        let num_or_undef = db.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
        let mut later = make_prop(&db, "x", num_or_undef, true);
        later.write_type = num_or_undef;
        merge_spread_property_into_map(&db, false, &mut map, &earlier);
        merge_spread_property_into_map(&db, false, &mut map, &later);
        let stored = map.get(&earlier.name).unwrap();
        assert!(!stored.optional);
        assert_ne!(stored.type_id, TypeId::UNDEFINED);
    }

    #[test]
    fn merge_spread_optional_later_unions_with_required_earlier_exact_optional_keeps_undefined() {
        let db = TypeInterner::new();
        let mut map: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let earlier = make_prop(&db, "x", TypeId::STRING, false);
        let num_or_undef = db.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
        let mut later = make_prop(&db, "x", num_or_undef, true);
        later.write_type = num_or_undef;
        merge_spread_property_into_map(&db, false, &mut map, &earlier);
        merge_spread_property_into_map(&db, true, &mut map, &later);
        let stored = map.get(&earlier.name).unwrap();
        assert!(!stored.optional);
        let members = tsz_solver::type_queries::get_union_members(&db, stored.type_id);
        assert!(members.is_some(), "result should be a union");
    }

    #[test]
    fn merge_spread_both_optional_stays_optional() {
        let db = TypeInterner::new();
        let mut map: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let earlier = make_prop(&db, "x", TypeId::STRING, true);
        let later = make_prop(&db, "x", TypeId::NUMBER, true);
        merge_spread_property_into_map(&db, false, &mut map, &earlier);
        merge_spread_property_into_map(&db, false, &mut map, &later);
        let stored = map.get(&earlier.name).unwrap();
        assert!(stored.optional);
    }

    #[test]
    fn rebase_spread_display_order_sorts_and_rebases() {
        let db = TypeInterner::new();
        let mut a = make_prop(&db, "a", TypeId::STRING, false);
        a.declaration_order = 5;
        let mut b = make_prop(&db, "b", TypeId::NUMBER, false);
        b.declaration_order = 2;
        let props = vec![a, b];
        let rebased = rebase_spread_display_property_order(props, 100);
        assert_eq!(rebased.len(), 2);
        assert_eq!(rebased[0].name, db.intern_string("b"));
        assert_eq!(rebased[0].declaration_order, 100);
        assert_eq!(rebased[1].name, db.intern_string("a"));
        assert_eq!(rebased[1].declaration_order, 101);
    }

    #[test]
    fn remove_synthetic_missing_props_drops_undefined_placeholders() {
        let db = TypeInterner::new();
        let branch1 = vec![make_prop(&db, "x", TypeId::STRING, false)];
        let branch2 = vec![make_prop(&db, "x", TypeId::UNDEFINED, true)];
        let mut all = vec![branch1, branch2];
        remove_synthetic_missing_union_spread_props(&mut all);
        assert_eq!(all[0].len(), 1);
        assert_eq!(all[1].len(), 0);
    }

    #[test]
    fn remove_synthetic_missing_props_keeps_real_optional_props() {
        let db = TypeInterner::new();
        let branch1 = vec![make_prop(&db, "y", TypeId::UNDEFINED, true)];
        let branch2 = vec![make_prop(&db, "y", TypeId::UNDEFINED, true)];
        let mut all = vec![branch1, branch2];
        remove_synthetic_missing_union_spread_props(&mut all);
        assert_eq!(all[0].len(), 1);
        assert_eq!(all[1].len(), 1);
    }
}
