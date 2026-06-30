//! Symbol/body publication for module-augmented empty object registries.
//!
//! This is a narrow bridge for the higher-kinded registry pattern where an
//! empty interface body is interned before cross-file augmentation merges its
//! members. The checker publishes the base shape's symbol to the home `DefId`
//! and, behind a separate flag, the merged body for an empty registry. Consumers
//! use those channels only when they later see an empty symbolic object/body.

use std::sync::OnceLock;

use super::{DefId, DefinitionStore};
use crate::construction::TypeDatabase;
use crate::types::{TypeData, TypeId};
use dashmap::mapref::entry::Entry;

/// Whether module-augmentation symbol-edge publication and consumption is active.
pub(crate) fn module_augmentation_symbol_edge_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_MODULE_AUG_SYMBOL_EDGE").is_ok_and(|v| v == "1"))
}

/// Whether merged body publication for empty module-augmented registries is active.
pub(crate) fn module_augmentation_body_publish_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_MODULE_AUG_BODY_PUBLISH").is_ok_and(|v| v == "1"))
}

fn plain_object_property_count(db: &dyn TypeDatabase, ty: TypeId) -> Option<usize> {
    match db.lookup(ty)? {
        TypeData::Object(shape_id) => Some(db.object_shape(shape_id).properties.len()),
        _ => None,
    }
}

impl DefinitionStore {
    /// Register a first-wins module-augmentation edge from `symbol_id` to `def_id`.
    pub fn register_module_augmentation_symbol_def(&self, symbol_id: u32, def_id: DefId) {
        if self.find_def_by_symbol(symbol_id).is_some() {
            return;
        }
        self.insert_symbol_only_mapping(symbol_id, def_id);
        self.bump_generation();
    }

    /// Flag-gated wrapper for module-augmentation edge publication.
    pub fn register_module_augmentation_symbol_def_if_enabled(
        &self,
        symbol_id: u32,
        def_id: DefId,
    ) {
        if module_augmentation_symbol_edge_enabled() {
            self.register_module_augmentation_symbol_def(symbol_id, def_id);
        }
    }

    /// Register the merged body for an empty module-augmented registry.
    pub fn register_module_augmented_body(
        &self,
        def_id: DefId,
        body: TypeId,
        source_files: &[u32],
    ) -> bool {
        match self.module_augmented_bodies.entry(def_id) {
            Entry::Vacant(entry) => {
                entry.insert((body, source_files.to_vec()));
                self.bump_generation();
                true
            }
            Entry::Occupied(mut entry) => {
                if entry.get().0 == body {
                    merge_source_files(&mut entry.get_mut().1, source_files);
                }
                false
            }
        }
    }

    /// Flag-gated body publication for a plain empty object receiving members.
    pub fn register_module_augmented_body_if_enabled(
        &self,
        def_id: DefId,
        base_body: TypeId,
        augmented_body: TypeId,
        db: &dyn TypeDatabase,
        source_files: &[u32],
    ) -> bool {
        if !module_augmentation_body_publish_enabled() {
            return false;
        }
        if plain_object_property_count(db, base_body) != Some(0) {
            return false;
        }
        if plain_object_property_count(db, augmented_body).is_none_or(|count| count == 0) {
            return false;
        }
        self.register_module_augmented_body(def_id, augmented_body, source_files)
    }

    fn module_augmented_body_for_registered(
        &self,
        def_id: DefId,
        current_body: TypeId,
        db: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        if plain_object_property_count(db, current_body) != Some(0) {
            return None;
        }
        let body = self.module_augmented_bodies.get(&def_id)?.0;
        plain_object_property_count(db, body)
            .is_some_and(|count| count > 0)
            .then_some(body)
    }

    /// Resolve an empty registry body to its merged module-augmented body.
    pub fn module_augmented_body_for(
        &self,
        def_id: DefId,
        current_body: TypeId,
        db: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        module_augmentation_body_publish_enabled()
            .then(|| self.module_augmented_body_for_registered(def_id, current_body, db))
            .flatten()
    }

    /// Return the merged module-augmented body when `current_body` is the empty base.
    pub fn module_augmented_body_or_current(
        &self,
        def_id: DefId,
        current_body: TypeId,
        db: &dyn TypeDatabase,
    ) -> TypeId {
        self.module_augmented_body_for(def_id, current_body, db)
            .unwrap_or(current_body)
    }

    pub(crate) fn invalidate_module_augmented_bodies_for_file(&self, file_id: u32) {
        let affected: Vec<_> = self
            .module_augmented_bodies
            .iter()
            .filter_map(|entry| entry.value().1.contains(&file_id).then_some(*entry.key()))
            .collect();
        if affected.is_empty() {
            return;
        }
        for def_id in affected {
            self.module_augmented_bodies.remove(&def_id);
        }
        self.bump_generation();
    }
}

fn merge_source_files(existing: &mut Vec<u32>, source_files: &[u32]) {
    for &file_id in source_files {
        if !existing.contains(&file_id) {
            existing.push(file_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::types::PropertyInfo;

    #[test]
    fn module_augmentation_symbol_def_registers_missing_edge() {
        let store = DefinitionStore::new();
        let def_id = DefId(42);

        store.register_module_augmentation_symbol_def(100, def_id);

        assert_eq!(store.find_def_by_symbol(100), Some(def_id));
    }

    #[test]
    fn module_augmentation_symbol_def_keeps_first_edge() {
        let store = DefinitionStore::new();
        let first = DefId(42);
        let second = DefId(43);

        store.register_module_augmentation_symbol_def(100, first);
        store.register_module_augmentation_symbol_def(100, second);

        assert_eq!(store.find_def_by_symbol(100), Some(first));
    }

    #[test]
    fn module_augmented_body_redirects_empty_plain_object() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let empty = types.object(Vec::new());
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[]));

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, empty, &types),
            Some(augmented)
        );
    }

    #[test]
    fn module_augmented_body_keeps_non_empty_current_body() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let current = types.object(vec![PropertyInfo::new(
            types.intern_string("current"),
            TypeId::NUMBER,
        )]);
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[]));

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, current, &types),
            None
        );
    }

    #[test]
    fn module_augmented_body_keeps_first_publication() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let first = types.object(vec![PropertyInfo::new(
            types.intern_string("first"),
            TypeId::STRING,
        )]);
        let second = types.object(vec![PropertyInfo::new(
            types.intern_string("second"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, first, &[]));
        assert!(!store.register_module_augmented_body(def_id, second, &[]));

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, types.object(Vec::new()), &types),
            Some(first)
        );
    }

    #[test]
    fn module_augmented_body_invalidates_when_augmentation_file_changes() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let empty = types.object(Vec::new());
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[7]));
        assert_eq!(
            store.module_augmented_body_for_registered(def_id, empty, &types),
            Some(augmented)
        );

        assert_eq!(store.invalidate_file(7), 0);

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, empty, &types),
            None
        );
    }
}
