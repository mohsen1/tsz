use super::{DefId, DefinitionStore, augmentation_transaction::AugmentationPublication};
use crate::types::{TypeId, TypeParamInfo};
use rustc_hash::FxHashSet;
use std::sync::Arc;

impl DefinitionStore {
    /// Get type parameters for a definition.
    pub fn get_type_params(&self, id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.definitions
            .get(&id)
            .map(|r| r.type_params.clone())
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_type_params(id))
            })
    }

    /// Get the body `TypeId` for a definition.
    pub fn get_body(&self, id: DefId) -> Option<TypeId> {
        self.definitions.get(&id).and_then(|r| r.body).or_else(|| {
            self.augmentation_parent()
                .and_then(|parent| parent.get_body(id))
        })
    }

    /// Record the `DefId`s directly referenced by a definition's published
    /// body.
    ///
    /// The caller must collect these dependencies with the interner that owns
    /// `body`. The store persists only `DefId` edges so later cache invalidators
    /// can chase the dependency graph without interpreting a foreign `TypeId`.
    pub fn set_body_dependency_defs(&self, id: DefId, deps: impl IntoIterator<Item = DefId>) {
        let mut seen = FxHashSet::default();
        let mut unique = Vec::new();
        for dep in deps {
            if seen.insert(dep) {
                unique.push(dep);
            }
        }
        self.record_augmentation_publication_with(|| {
            AugmentationPublication::SetBodyDependencies {
                id,
                dependencies: unique.clone(),
            }
        });
        if unique.is_empty() {
            self.body_dependency_defs.remove(&id);
            if self.augmentation_parent().is_some() {
                self.augmentation_removed_body_dependencies.insert(id);
            }
        } else {
            self.augmentation_removed_body_dependencies.remove(&id);
            self.body_dependency_defs.insert(id, Arc::from(unique));
        }
    }

    /// `DefId`s directly referenced by a published body, if a publisher
    /// recorded them with that body's owning interner.
    pub fn body_dependency_defs(&self, id: DefId) -> Option<Arc<[DefId]>> {
        if self.augmentation_removed_body_dependencies.contains(&id) {
            return None;
        }
        self.body_dependency_defs
            .get(&id)
            .map(|deps| Arc::clone(&deps))
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.body_dependency_defs(id))
            })
    }

    /// Whether `id` already publishes exactly `body` (and, when given,
    /// exactly `params`). Comparison runs under the entry guard without
    /// cloning, so no-op republication checks stay cheap on hot paths.
    pub fn body_and_params_published(
        &self,
        id: DefId,
        body: TypeId,
        params: Option<&[TypeParamInfo]>,
    ) -> bool {
        self.definitions
            .get(&id)
            .map(|entry| {
                entry.body == Some(body)
                    && params.is_none_or(|params| entry.type_params.as_slice() == params)
            })
            .unwrap_or_else(|| {
                self.augmentation_parent()
                    .is_some_and(|parent| parent.body_and_params_published(id, body, params))
            })
    }

    /// Get parent class `DefId` for a class.
    pub fn get_extends(&self, id: DefId) -> Option<DefId> {
        self.definitions
            .get(&id)
            .and_then(|r| r.extends)
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_extends(id))
            })
    }

    /// Set the heritage (extends + implements) for a definition after registration.
    ///
    /// Used for cross-batch heritage resolution: when a user class extends a lib
    /// type, the heritage is resolved by name after all pre-population batches
    /// have completed.
    pub fn set_heritage(&self, id: DefId, extends: Option<DefId>, implements: Vec<DefId>) {
        if let Some(parent) = self.augmentation_parent() {
            parent.set_heritage(id, extends, implements);
            return;
        }
        self.ensure_augmentation_definition(id);
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.extends = extends;
            entry.implements = implements;
            self.bump_generation();
        }
    }

    /// #14351: record one instantiated heritage edge for the lazy-reference
    /// relation. `derived` extends `parent`, and `base_type` is the parent
    /// reference as written in `derived`'s own scope (e.g. `Functor1<F>` from
    /// `interface Apply1<F> extends Functor1<F>`). First writer per
    /// `(derived, parent)` wins; later equal writes are idempotent. This is
    /// inert data — only the flag-gated lazy-reference relation branch reads it.
    pub fn add_heritage_instantiation(&self, derived: DefId, parent: DefId, base_type: TypeId) {
        if self.get_heritage_instantiation(derived, parent).is_some() {
            return;
        }
        self.record_augmentation_publication(AugmentationPublication::AddHeritageInstantiation {
            derived,
            parent,
            base_type,
        });
        let mut entry = self.heritage_instantiations.entry(derived).or_default();
        entry.push((parent, base_type));
    }

    /// #14351: the instantiated base `TypeId` for a DIRECT `extends` edge
    /// `derived extends target`, or `None` if `target` is not a direct parent
    /// of `derived` (the first slice handles single-hop heritage only).
    pub fn get_heritage_instantiation(&self, derived: DefId, target: DefId) -> Option<TypeId> {
        self.heritage_instantiations
            .get(&derived)
            .and_then(|edges| edges.iter().find(|&&(p, _)| p == target).map(|&(_, t)| t))
            .or_else(|| {
                self.augmentation_parent()
                    .and_then(|parent| parent.get_heritage_instantiation(derived, target))
            })
    }
}
