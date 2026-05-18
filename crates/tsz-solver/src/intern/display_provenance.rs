//! Diagnostic display provenance store.
//!
//! This module owns the priority rules and records for how types should be
//! displayed in diagnostics. Separating display policy from semantic interning
//! keeps `TypeInterner` focused on canonical identity and deduplication.
//!
//! # Record kinds
//!
//! - Alias application: evaluated-type → `Application` `TypeId` mapping so the
//!   formatter can show `Dictionary<string>` instead of `{ [index: string]: string }`.
//! - Fresh object literal properties: pre-widened property types so diagnostics
//!   display `{ x: "hello" }` rather than the widened `{ x: string }`.
//! - Union origin: as-written member list before flattening so `T | null` prints
//!   correctly after structural union normalization.
//! - Conditional alias base markers: tags an application base whose body is a
//!   conditional type, used to select alias-preferring display in branch unions.
//!
//! # Priority rules
//!
//! Each `record_*` method embeds the first-writer/last-writer heuristic that was
//! previously scattered through `TypeInterner`. Tests for these rules live in
//! the [`tests`] submodule and run independently of semantic interning.

use crate::types::{
    LiteralValue, ObjectShape, ObjectShapeId, PropertyInfo, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId,
};
use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Minimal type-query interface needed by display provenance policy methods.
///
/// Implemented by [`super::TypeInterner`] in its module. Defined here to keep
/// `DisplayProvenanceStore` decoupled from the concrete interner type while
/// avoiding a dependency on the heavier [`crate::caches::db::TypeDatabase`].
pub trait ProvenanceLookup {
    fn lookup(&self, id: TypeId) -> Option<TypeData>;
    fn lookup_alloc_order(&self, id: TypeId) -> Option<u32>;
    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication>;
    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]>;
    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape>;
    fn contains_generic_type_parameters(&self, id: TypeId) -> bool;
}

/// Stores display provenance records and owns the priority rules for recording them.
///
/// Physical storage uses interior-mutable `DashMap`s so provenance can be recorded
/// concurrently with type interning without requiring a mutable borrow.
pub struct DisplayProvenanceStore {
    /// Pre-widened property types for fresh object literal diagnostics.
    ///
    /// Key: `TypeId` of the widened (interned) object type.
    /// Value: `PropertyInfo` vec with original (non-widened) `type_ids`.
    display_properties: DashMap<TypeId, Arc<Vec<PropertyInfo>>, FxBuildHasher>,

    /// Reverse mapping: evaluated-type → alias-application `TypeId`.
    display_alias: DashMap<TypeId, TypeId, FxBuildHasher>,

    /// Application bases whose type-alias body is a conditional type.
    conditional_alias_bases: DashMap<TypeId, (), FxBuildHasher>,

    /// As-written origin members for a flattened union `TypeId`.
    ///
    /// Key: the canonical (flattened/sorted) union returned to the checker.
    /// Value: the unflattened member list in source order.
    display_union_origin: DashMap<TypeId, Arc<Vec<TypeId>>, FxBuildHasher>,

    /// Display-order `Array` base type used for keyof/mapped diagnostics.
    array_display_base_type: AtomicU32,

    /// Set when union normalization detects a union too complex to represent.
    /// Mirrors tsc's `removeSubtypes` complexity heuristic (TS2590).
    union_too_complex: AtomicBool,
}

impl Default for DisplayProvenanceStore {
    fn default() -> Self {
        Self {
            display_properties: DashMap::with_hasher(FxBuildHasher),
            display_alias: DashMap::with_hasher(FxBuildHasher),
            conditional_alias_bases: DashMap::with_hasher(FxBuildHasher),
            display_union_origin: DashMap::with_hasher(FxBuildHasher),
            array_display_base_type: AtomicU32::new(u32::MAX),
            union_too_complex: AtomicBool::new(false),
        }
    }
}

impl DisplayProvenanceStore {
    /// Estimate memory occupied by provenance maps.
    ///
    /// `dashmap_entry_overhead` is the per-entry bookkeeping cost (typically 64 bytes).
    pub fn memory_size_estimate(&self, dashmap_entry_overhead: usize) -> usize {
        let mut size = 0;
        size += self.display_properties.len()
            * (dashmap_entry_overhead
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<Arc<Vec<PropertyInfo>>>());
        size +=
            self.display_alias.len() * (dashmap_entry_overhead + std::mem::size_of::<TypeId>() * 2);
        size
    }

    pub fn record_fresh_object_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        self.display_properties.insert(type_id, Arc::new(props));
    }

    pub fn get_fresh_object_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        self.display_properties.get(&type_id).map(|r| r.clone())
    }

    /// Record that `evaluated` was produced by evaluating `application`.
    ///
    /// Applies priority rules: generic-arg aliases that structurally pre-date
    /// the application are skipped to prevent structural types from being
    /// spuriously rebranded. Concrete (non-generic-arg) applications always
    /// win so named library types display their nominal form.
    pub fn record_alias_application(
        &self,
        db: &dyn ProvenanceLookup,
        evaluated: TypeId,
        application: TypeId,
    ) {
        if evaluated == application {
            return;
        }
        // Type parameters are scoped identities. Aliasing them repaints
        // unrelated uses of the same parameter in later diagnostics.
        let evaluated_data = db.lookup(evaluated);
        if matches!(evaluated_data, Some(TypeData::TypeParameter(_))) {
            return;
        }
        let application_data = db.lookup(application);
        let application_is_alias = matches!(application_data, Some(TypeData::Application(_)));
        let application_has_generic_args =
            if let Some(TypeData::Application(app_id)) = application_data {
                db.type_application(app_id)
                    .args
                    .iter()
                    .any(|&arg| db.contains_generic_type_parameters(arg))
            } else {
                false
            };
        let evaluated_is_mapped = matches!(evaluated_data, Some(TypeData::Mapped(_)));
        let evaluated_precedes_application = match (
            db.lookup_alloc_order(evaluated),
            db.lookup_alloc_order(application),
        ) {
            (Some(ev_order), Some(app_order)) => ev_order <= app_order,
            _ => evaluated.0 <= application.0,
        };
        if application_is_alias
            && application_has_generic_args
            && evaluated_precedes_application
            && !evaluated_is_mapped
        {
            let existing_is_application =
                self.display_alias.get(&evaluated).is_some_and(|existing| {
                    matches!(db.lookup(*existing), Some(TypeData::Application(_)))
                });
            if !existing_is_application {
                return;
            }
        }
        // Never alias intrinsics — they are shared sentinels.
        if evaluated.is_intrinsic() {
            return;
        }
        // Guard against self-referential cycles.
        if let Some(TypeData::Application(app_id)) = application_data {
            let app = db.type_application(app_id);
            if app.args.contains(&evaluated) {
                return;
            }
        }
        if application_is_alias
            && let Some(existing) = self.display_alias.get(&evaluated).map(|alias| *alias)
            && !matches!(db.lookup(existing), Some(TypeData::Application(_)))
        {
            return;
        }
        self.display_alias.insert(evaluated, application);
    }

    /// Prefer a concrete `Application` display alias over structural provenance
    /// recorded while evaluating the alias body.
    pub fn record_alias_application_preferring_application(
        &self,
        db: &dyn ProvenanceLookup,
        evaluated: TypeId,
        application: TypeId,
    ) {
        self.record_alias_application(db, evaluated, application);
        if self.get_alias(evaluated) == Some(application) {
            return;
        }
        if evaluated == application || evaluated.is_intrinsic() {
            return;
        }
        let evaluated_data = db.lookup(evaluated);
        if matches!(evaluated_data, Some(TypeData::TypeParameter(_))) {
            return;
        }
        let Some(TypeData::Application(app_id)) = db.lookup(application) else {
            return;
        };
        let app = db.type_application(app_id);
        if app.args.contains(&evaluated) {
            return;
        }
        let preserves_conditional_branch_alias = self.is_conditional_alias_base(app.base)
            && self.get_alias(evaluated).is_some_and(|existing| {
                matches!(db.lookup(existing), Some(TypeData::Intersection(_)))
            });
        if preserves_conditional_branch_alias {
            return;
        }
        let application_has_generic_args = app
            .args
            .iter()
            .any(|&arg| db.contains_generic_type_parameters(arg));
        let evaluated_precedes_application = match (
            db.lookup_alloc_order(evaluated),
            db.lookup_alloc_order(application),
        ) {
            (Some(ev_order), Some(app_order)) => ev_order <= app_order,
            _ => evaluated.0 <= application.0,
        };
        let evaluated_is_mapped = matches!(evaluated_data, Some(TypeData::Mapped(_)));
        if application_has_generic_args && evaluated_precedes_application && !evaluated_is_mapped {
            return;
        }
        self.display_alias.insert(evaluated, application);
    }

    /// Look up the alias application recorded for `type_id`.
    pub fn get_alias(&self, type_id: TypeId) -> Option<TypeId> {
        self.display_alias.get(&type_id).map(|r| *r)
    }

    /// Mark an application base whose type-alias body is a conditional type.
    pub fn mark_conditional_alias_base(&self, base: TypeId) {
        self.conditional_alias_bases.insert(base, ());
    }

    pub fn is_conditional_alias_base(&self, base: TypeId) -> bool {
        self.conditional_alias_bases.contains_key(&base)
    }

    /// Record the as-written origin members for a flattened union.
    ///
    /// Stores only when the canonical sort differs from the source order in a way
    /// that tsc's printer preserves (anonymous objects, number literals, `keyof`
    /// members, same-base applications, array-element pairs, generic mixed unions).
    /// First writer wins for deterministic display order.
    pub fn record_union_origin(
        &self,
        db: &dyn ProvenanceLookup,
        union_type_id: TypeId,
        origin_members: Vec<TypeId>,
    ) {
        if origin_members.len() < 2 {
            return;
        }
        let Some(TypeData::Union(list_id)) = db.lookup(union_type_id) else {
            return;
        };
        let current = db.type_list(list_id);
        let flattened = current.len() > origin_members.len();
        if !flattened {
            let needs_origin = self.union_origin_overrides_canonical_anon_object_sort(
                db,
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_number_literal_sort(
                db,
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_keyof_sort(
                db,
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_application_sort(
                db,
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_array_pair_sort(
                db,
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_keyof_literal_sort(
                db,
                current.as_ref(),
                &origin_members,
            ) || self.union_origin_overrides_canonical_generic_display_sort(
                db,
                current.as_ref(),
                &origin_members,
            );
            if !needs_origin {
                return;
            }
        }
        self.display_union_origin
            .entry(union_type_id)
            .or_insert_with(|| Arc::new(origin_members));
    }

    /// Replace the display origin with a more specific tsc-compatible member order.
    pub fn replace_union_origin(
        &self,
        db: &dyn ProvenanceLookup,
        union_type_id: TypeId,
        origin_members: Vec<TypeId>,
    ) {
        if origin_members.len() < 2 {
            return;
        }
        let Some(TypeData::Union(_)) = db.lookup(union_type_id) else {
            return;
        };
        self.display_union_origin
            .insert(union_type_id, Arc::new(origin_members));
    }

    /// Look up the as-written origin members for a flattened union.
    pub fn get_union_origin(&self, type_id: TypeId) -> Option<Arc<Vec<TypeId>>> {
        self.display_union_origin.get(&type_id).map(|r| r.clone())
    }

    pub fn set_array_display_base_type(&self, type_id: TypeId) {
        self.array_display_base_type
            .store(type_id.0, Ordering::Relaxed);
    }

    pub fn get_array_display_base_type(&self) -> Option<TypeId> {
        let raw = self.array_display_base_type.load(Ordering::Relaxed);
        if raw == u32::MAX {
            None
        } else {
            Some(TypeId(raw))
        }
    }

    /// Atomically read and clear the "union too complex" flag.
    pub fn take_union_too_complex(&self) -> bool {
        self.union_too_complex.swap(false, Ordering::Relaxed)
    }

    /// Mark that union normalization was aborted due to complexity.
    pub fn set_union_too_complex(&self) {
        self.union_too_complex.store(true, Ordering::Relaxed);
    }

    /// Returns `true` when `a` and `b` have the same length and differ in order.
    /// Used as a fast pre-check in the `union_origin_overrides_*` helpers.
    fn same_set_different_order(a: &[TypeId], b: &[TypeId]) -> bool {
        a.len() == b.len() && a != b
    }

    fn union_origin_overrides_canonical_anon_object_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != origin.len() {
            return false;
        }
        let has_anon_object = current.iter().any(|&id| {
            if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                db.lookup(id)
            {
                db.object_shape(shape_id).symbol.is_none()
            } else {
                false
            }
        });
        has_anon_object && current != origin
    }

    fn union_origin_overrides_canonical_number_literal_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if !Self::same_set_different_order(current, origin) {
            return false;
        }
        current.iter().all(|&id| {
            matches!(
                db.lookup(id),
                Some(TypeData::Literal(LiteralValue::Number(_)))
            )
        })
    }

    fn union_origin_overrides_canonical_keyof_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if !Self::same_set_different_order(current, origin) {
            return false;
        }
        current
            .iter()
            .chain(origin.iter())
            .any(|&id| matches!(db.lookup(id), Some(TypeData::KeyOf(_))))
    }

    fn union_origin_overrides_canonical_application_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if !Self::same_set_different_order(current, origin) {
            return false;
        }
        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }
        let mut expected_base: Option<TypeId> = None;
        for ids in [origin, current] {
            for &id in ids {
                let Some(TypeData::Application(app_id)) = db.lookup(id) else {
                    return false;
                };
                let app = db.type_application(app_id);
                match expected_base {
                    Some(base) if base != app.base => return false,
                    Some(_) => {}
                    None => expected_base = Some(app.base),
                }
            }
        }
        expected_base.is_some()
    }

    fn union_origin_overrides_canonical_array_pair_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != 2 || !Self::same_set_different_order(current, origin) {
            return false;
        }
        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }
        let is_array_of = |array: TypeId, element: TypeId| -> bool {
            matches!(db.lookup(array), Some(TypeData::Array(inner)) if inner == element)
        };
        is_array_of(origin[0], origin[1]) || is_array_of(origin[1], origin[0])
    }

    fn union_origin_overrides_canonical_keyof_literal_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if current.len() != 2 || !Self::same_set_different_order(current, origin) {
            return false;
        }
        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }
        let is_keyof = |id| matches!(db.lookup(id), Some(TypeData::KeyOf(_)));
        let is_literal = |id| matches!(db.lookup(id), Some(TypeData::Literal(_)));
        (is_keyof(origin[0]) && is_literal(origin[1]))
            || (is_literal(origin[0]) && is_keyof(origin[1]))
    }

    fn union_origin_overrides_canonical_generic_display_sort(
        &self,
        db: &dyn ProvenanceLookup,
        current: &[TypeId],
        origin: &[TypeId],
    ) -> bool {
        if !Self::same_set_different_order(current, origin) {
            return false;
        }
        let mut current_sorted = current.to_vec();
        let mut origin_sorted = origin.to_vec();
        current_sorted.sort_unstable_by_key(|id| id.0);
        origin_sorted.sort_unstable_by_key(|id| id.0);
        if current_sorted != origin_sorted {
            return false;
        }
        let has_complex_generic = origin.iter().any(|&id| {
            matches!(
                db.lookup(id),
                Some(TypeData::Application(_) | TypeData::KeyOf(_) | TypeData::IndexAccess(_, _))
            )
        });
        if has_complex_generic {
            return true;
        }
        let all_literals = origin
            .iter()
            .all(|&id| matches!(db.lookup(id), Some(TypeData::Literal(_))));
        if all_literals {
            return false;
        }
        origin
            .iter()
            .any(|&id| matches!(db.lookup(id), Some(TypeData::Literal(_))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::PropertyInfo;

    fn make_interner() -> TypeInterner {
        TypeInterner::new()
    }

    #[test]
    fn record_alias_application_basic() {
        let interner = make_interner();
        let lit_a = interner.literal_string("a");
        let lit_b = interner.literal_string("b");
        interner
            .display_provenance
            .record_alias_application(&interner, lit_a, lit_b);
        assert_eq!(interner.display_provenance.get_alias(lit_a), Some(lit_b));
    }

    #[test]
    fn record_alias_application_skips_self() {
        let interner = make_interner();
        let lit = interner.literal_number(1.0);
        interner
            .display_provenance
            .record_alias_application(&interner, lit, lit);
        assert_eq!(interner.display_provenance.get_alias(lit), None);
    }

    #[test]
    fn record_alias_application_skips_intrinsic_evaluated() {
        let interner = make_interner();
        let lit = interner.literal_number(42.0);
        interner
            .display_provenance
            .record_alias_application(&interner, TypeId::STRING, lit);
        assert_eq!(interner.display_provenance.get_alias(TypeId::STRING), None);
    }

    #[test]
    fn record_alias_application_last_writer_wins_for_non_application() {
        let interner = make_interner();
        let lit_a = interner.literal_number(1.0);
        let lit_b = interner.literal_number(2.0);
        let lit_c = interner.literal_number(3.0);
        interner
            .display_provenance
            .record_alias_application(&interner, lit_a, lit_b);
        interner
            .display_provenance
            .record_alias_application(&interner, lit_a, lit_c);
        // When neither is a concrete Application, the last call wins (no first-writer guard).
        assert_eq!(interner.display_provenance.get_alias(lit_a), Some(lit_c));
    }

    #[test]
    fn union_origin_skipped_when_order_matches() {
        let interner = make_interner();
        let a = interner.literal_number(1.0);
        let b = interner.literal_number(2.0);
        let union_id = interner.union_from_sorted_vec(vec![a, b]);
        interner
            .display_provenance
            .record_union_origin(&interner, union_id, vec![a, b]);
        assert!(
            interner
                .display_provenance
                .get_union_origin(union_id)
                .is_none(),
            "should not store when canonical order matches origin"
        );
    }

    #[test]
    fn union_origin_stored_on_flatten() {
        let interner = make_interner();
        let a = interner.literal_number(1.0);
        let b = interner.literal_number(2.0);
        let c = interner.literal_number(3.0);
        let union_id = interner.union_from_sorted_vec(vec![a, b, c]);
        // origin is shorter than current → flattening occurred
        interner
            .display_provenance
            .record_union_origin(&interner, union_id, vec![a, b]);
        assert!(
            interner
                .display_provenance
                .get_union_origin(union_id)
                .is_some(),
            "should store when flattening occurred"
        );
    }

    #[test]
    fn union_origin_stored_for_anon_object_reorder() {
        let interner = make_interner();
        // Two distinct anonymous shapes so they get distinct TypeIds.
        let a_obj = interner.object(vec![PropertyInfo {
            name: interner.intern_string("p"),
            type_id: TypeId::NUMBER,
            ..Default::default()
        }]);
        let b_obj = interner.object(vec![]);
        assert_ne!(
            a_obj, b_obj,
            "test requires two distinct anonymous object types"
        );
        // Canonical order is by TypeId allocation order; origin is reversed.
        let (canon, origin) = if a_obj.0 < b_obj.0 {
            ([a_obj, b_obj], vec![b_obj, a_obj])
        } else {
            ([b_obj, a_obj], vec![a_obj, b_obj])
        };
        let union_id = interner.union_from_sorted_vec(canon.to_vec());
        interner
            .display_provenance
            .record_union_origin(&interner, union_id, origin.clone());
        let stored = interner.display_provenance.get_union_origin(union_id);
        assert_eq!(
            stored.as_deref().map(|v| v.as_slice()),
            Some(origin.as_slice()),
            "should store origin when anon object sort was reordered"
        );
    }

    #[test]
    fn conditional_alias_base_roundtrip() {
        let interner = make_interner();
        let ty = interner.literal_number(0.0);
        assert!(!interner.display_provenance.is_conditional_alias_base(ty));
        interner.display_provenance.mark_conditional_alias_base(ty);
        assert!(interner.display_provenance.is_conditional_alias_base(ty));
    }

    #[test]
    fn fresh_properties_roundtrip() {
        let interner = make_interner();
        let ty = interner.literal_number(99.0);
        let prop = PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            ..Default::default()
        };
        interner
            .display_provenance
            .record_fresh_object_properties(ty, vec![prop.clone()]);
        let got = interner
            .display_provenance
            .get_fresh_object_properties(ty)
            .expect("should have stored props");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, prop.name);
    }

    #[test]
    fn union_too_complex_flag_take_clears() {
        let interner = make_interner();
        assert!(!interner.display_provenance.take_union_too_complex());
        interner.display_provenance.set_union_too_complex();
        assert!(interner.display_provenance.take_union_too_complex());
        // second take should return false — flag was cleared
        assert!(!interner.display_provenance.take_union_too_complex());
    }

    #[test]
    fn array_display_base_type_roundtrip() {
        let interner = make_interner();
        assert_eq!(
            interner.display_provenance.get_array_display_base_type(),
            None
        );
        let lit = interner.literal_number(7.0);
        interner.display_provenance.set_array_display_base_type(lit);
        assert_eq!(
            interner.display_provenance.get_array_display_base_type(),
            Some(lit)
        );
    }
}
