//! Internal cache accessors and object-spread property collection for
//! [`QueryCache`].
//!
//! Split out of `query_cache.rs` to keep that shard under the 2000-line
//! file-size cap. This is a child module of `query_cache`, so it keeps
//! access to the cache's private fields.

use super::*;
use crate::types::Visibility;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObjectSpreadVisitState {
    Entered,
    AlreadyVisited,
}

fn object_spread_visit_state(
    visited: &mut FxHashSet<TypeId>,
    normalized: TypeId,
) -> ObjectSpreadVisitState {
    if visited.insert(normalized) {
        ObjectSpreadVisitState::Entered
    } else {
        ObjectSpreadVisitState::AlreadyVisited
    }
}

impl QueryCache<'_> {
    pub(super) fn check_property_cache(
        &self,
        key: PropertyAccessCacheKey,
    ) -> Option<PropertyAccessResult> {
        self.property_cache.borrow().get(&key).copied()
    }

    pub(super) fn insert_property_cache(
        &self,
        key: PropertyAccessCacheKey,
        result: PropertyAccessResult,
    ) {
        self.property_cache.borrow_mut().insert(key, result);
    }

    pub(super) fn check_element_access_cache(
        &self,
        key: ElementAccessTypeCacheKey,
    ) -> Option<TypeId> {
        self.element_access_cache.borrow().get(&key).copied()
    }

    pub(super) fn insert_element_access_cache(
        &self,
        key: ElementAccessTypeCacheKey,
        result: TypeId,
    ) {
        self.element_access_cache.borrow_mut().insert(key, result);
    }

    /// Layered eval-memo lookup: local per-file cache first, then the shared
    /// cross-file cache (promoting shared hits into the local map). Single
    /// source of truth for both the top-level `evaluate_type_with_options`
    /// boundary and nested `lookup_eval_memo` reads (issue #13097).
    pub(super) fn lookup_eval_cache_layers(&self, key: EvaluationCacheKey) -> Option<TypeId> {
        if let Some(result) = self.eval_cache.borrow().get(&key).copied() {
            return Some(result);
        }
        if let Some(shared) = self.shared
            && let Some(result) = shared.eval_cache.get(&key).map(|r| *r)
        {
            self.eval_cache.borrow_mut().insert(key, result);
            return Some(result);
        }
        None
    }

    pub(super) fn check_application_eval_cache(
        &self,
        key: ApplicationEvalCacheKey,
    ) -> Option<TypeId> {
        if let Some(result) = self.application_eval_cache.borrow().get(&key).copied() {
            self.application_eval_cache_stats.record_hit();
            return Some(result);
        }
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
        {
            if let Some(result) = shared.application_eval_cache.get(&key).map(|entry| *entry) {
                self.application_eval_cache
                    .borrow_mut()
                    .insert(key.clone(), result);
                application_eval_index::record_dependencies(
                    self.interner,
                    &self.application_eval_dependency_index,
                    &key,
                    None,
                    result,
                );
                self.application_eval_cache_stats.record_hit();
                self.application_eval_cache_stats.record_shared_hit();
                tsz_common::perf_counters::record_shared_application_eval_cache_hit();
                return Some(result);
            }
            self.application_eval_cache_stats.record_shared_miss();
            tsz_common::perf_counters::record_shared_application_eval_cache_miss();
        } else {
            tsz_common::perf_counters::record_shared_application_eval_cache_bypass();
        }
        self.application_eval_cache_stats.record_miss();
        None
    }

    pub(super) fn insert_application_eval_cache(
        &self,
        key: ApplicationEvalCacheKey,
        result: TypeId,
    ) {
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
        {
            shared.insert_application_eval_cache(self.interner, key.clone(), result);
            self.application_eval_cache_stats.record_shared_insert();
            tsz_common::perf_counters::record_shared_application_eval_cache_insert();
        }
        let old_result = self
            .application_eval_cache
            .borrow_mut()
            .insert(key.clone(), result);
        application_eval_index::record_dependencies(
            self.interner,
            &self.application_eval_dependency_index,
            &key,
            old_result,
            result,
        );
    }

    #[cfg(test)]
    pub(crate) fn application_eval_dependency_key_count(&self, def_id: DefId) -> usize {
        application_eval_index::key_count(&self.application_eval_dependency_index, def_id)
    }

    pub(super) fn check_object_spread_properties_cache(
        &self,
        key: TypeId,
    ) -> Option<Vec<PropertyInfo>> {
        self.object_spread_properties_cache
            .borrow()
            .get(&key)
            .cloned()
    }

    pub(super) fn insert_object_spread_properties_cache(
        &self,
        key: TypeId,
        value: Vec<PropertyInfo>,
    ) {
        self.object_spread_properties_cache
            .borrow_mut()
            .insert(key, value);
    }

    pub(super) fn collect_object_spread_properties_inner(
        &self,
        spread_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> Vec<PropertyInfo> {
        let normalized =
            self.evaluate_type_with_options(spread_type, self.no_unchecked_indexed_access());

        match object_spread_visit_state(visited, normalized) {
            ObjectSpreadVisitState::Entered => {}
            ObjectSpreadVisitState::AlreadyVisited => return Vec::new(),
        }

        if normalized != spread_type {
            return self.collect_object_spread_properties_inner(normalized, visited);
        }

        let Some(key) = self.interner.lookup(normalized) else {
            return Vec::new();
        };

        let props = match key {
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                self.interner.object_shape(shape_id).properties.to_vec()
            }
            TypeData::Callable(shape_id) => {
                self.interner.callable_shape(shape_id).properties.to_vec()
            }
            TypeData::Intersection(members_id) => {
                let members = self.interner.type_list(members_id);
                let mut merged: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

                for &member in members.iter() {
                    for prop in self.collect_object_spread_properties_inner(member, visited) {
                        merged.insert(prop.name, prop);
                    }
                }

                merged.into_values().collect()
            }
            TypeData::Union(members_id) => {
                let members = self.interner.type_list(members_id);
                // Collect properties from non-nullish union members.
                // Nullish members (null, undefined, void) spread to {} and
                // contribute no properties. Properties that don't appear in
                // every non-nullish member become optional.
                let has_nullish = members.iter().any(|m| m.is_nullable());
                let non_nullish_count = members.iter().filter(|m| !m.is_nullable()).count();

                if non_nullish_count == 0 {
                    return Vec::new();
                }

                // Collect properties per member
                let mut all_props: Vec<Vec<PropertyInfo>> = Vec::with_capacity(non_nullish_count);
                for &member in members.iter().filter(|m| !m.is_nullable()) {
                    all_props.push(self.collect_object_spread_properties_inner(member, visited));
                }

                // Merge: a property appears in the result if it exists in at
                // least one member. Its type is the union of types across
                // members where it appears. It is optional if it doesn't
                // appear in all non-nullish members or if any nullish member
                // exists (since the spread could be null/undefined → {}).
                let mut merged: FxHashMap<Atom, (TypeId, bool, usize)> = FxHashMap::default();
                for member_props in &all_props {
                    for prop in member_props {
                        let entry =
                            merged
                                .entry(prop.name)
                                .or_insert((prop.type_id, prop.optional, 0));
                        if entry.0 != prop.type_id {
                            entry.0 = self.interner.union2(entry.0, prop.type_id);
                        }
                        entry.1 = entry.1 && prop.optional;
                        entry.2 += 1;
                    }
                }

                merged
                    .into_iter()
                    .map(|(name, (type_id, was_optional, count))| {
                        let optional = was_optional || has_nullish || count < non_nullish_count;
                        PropertyInfo {
                            name,
                            type_id,
                            optional,
                            readonly: false,
                            write_type: type_id,
                            is_class_prototype: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: 0,
                            is_string_named: false,
                            is_symbol_named: false,
                            single_quoted_name: false,
                            non_widening: false,
                        }
                    })
                    .collect()
            }
            TypeData::TypeParameter(info) => {
                // For type parameters with constraints (e.g. `T extends { x: number }`),
                // collect properties from the constraint. Required properties in the
                // constraint are guaranteed to exist on any value of type T.
                if let Some(constraint) = info.constraint {
                    return self.collect_object_spread_properties_inner(constraint, visited);
                }
                Vec::new()
            }
            _ => Vec::new(),
        };

        // Spread removes readonly modifiers from properties (TypeScript spec).
        // `{ ...readonlyObj }` produces a mutable copy.
        // Also reset write_type to match type_id so the property is fully writable.
        // Class prototype members (methods/accessors) are excluded from spread results
        // because they live on the prototype, not as own enumerable properties.
        // This matches tsc's isSpreadPrototypeProperty() behavior.
        props
            .into_iter()
            .filter(|p| {
                !p.is_class_prototype
                    && p.visibility == Visibility::Public
                    && !self
                        .resolve_atom_ref(p.name)
                        .starts_with("__private_brand_")
            })
            .map(|mut p| {
                p.readonly = false;
                p.write_type = p.type_id;
                p
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_spread_visit_state_records_first_entry() {
        let mut visited = FxHashSet::default();

        let state = object_spread_visit_state(&mut visited, TypeId::STRING);

        assert_eq!(state, ObjectSpreadVisitState::Entered);
        assert!(visited.contains(&TypeId::STRING));
    }

    #[test]
    fn object_spread_visit_state_records_reentry() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            object_spread_visit_state(&mut visited, TypeId::STRING),
            ObjectSpreadVisitState::Entered
        );
        assert_eq!(
            object_spread_visit_state(&mut visited, TypeId::STRING),
            ObjectSpreadVisitState::AlreadyVisited
        );
        assert_eq!(visited.len(), 1);
    }
}
