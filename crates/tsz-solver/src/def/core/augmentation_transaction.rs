//! Isolated publication overlays for module-augmentation resolution.
//!
//! Cross-arena interface resolution can discover a recursion-depth bailout
//! only after it has resolved several members. Those resolutions normally
//! write through to the program-wide [`DefinitionStore`]. An overlay keeps
//! those writes private and reads through to its parent; commit replays the
//! ordered writes into the parent, while rollback simply drops the overlay.
//! Because a nested overlay's parent may itself be an overlay, inner commits
//! remain private until the outermost transaction commits.

use super::{DefId, DefinitionInfo, DefinitionStore};
use crate::types::{ObjectShape, TypeId, TypeParamInfo};
use std::sync::Arc;

pub(super) struct AugmentationDefinitionSemantics {
    body: Option<TypeId>,
    type_params: Vec<TypeParamInfo>,
    has_type_param_semantics: bool,
    instance_shape: Option<Arc<ObjectShape>>,
    static_shape: Option<Arc<ObjectShape>>,
}

impl AugmentationDefinitionSemantics {
    pub(super) fn split(mut info: DefinitionInfo) -> (DefinitionInfo, Self) {
        let type_params = info.type_params.clone();
        let has_type_param_semantics = type_params
            .iter()
            .any(|param| param.constraint.is_some() || param.default.is_some());
        for param in &mut info.type_params {
            param.constraint = None;
            param.default = None;
        }
        let semantics = Self {
            body: info.body.take(),
            type_params,
            has_type_param_semantics,
            instance_shape: info.instance_shape.take(),
            static_shape: info.static_shape.take(),
        };
        (info, semantics)
    }
}

#[derive(Clone, Debug)]
pub(super) enum AugmentationPublication {
    SetBody {
        id: DefId,
        body: TypeId,
        params: Option<Vec<TypeParamInfo>>,
        finalized: bool,
    },
    SetStaticShape {
        id: DefId,
        shape: Arc<ObjectShape>,
    },
    MarkPublishOnce(DefId),
    MarkDepthPoisoned(DefId),
    MarkCircular(DefId),
    MarkBodyComputed(TypeId),
    MarkBodyDirectlyNamed(TypeId),
    MarkTupleSpreadFlattenedAlias(DefId),
    SetTypeParams {
        id: DefId,
        params: Vec<TypeParamInfo>,
    },
    SetInstanceShape {
        id: DefId,
        shape: Arc<ObjectShape>,
    },
    RegisterTypeToDef {
        type_id: TypeId,
        def_id: DefId,
    },
    RegisterClassInstanceType {
        class_def: DefId,
        instance_type: TypeId,
    },
    RegisterTypeofValueLiteral {
        symbol_id: u32,
        literal: TypeId,
    },
    SetBodyDependencies {
        id: DefId,
        dependencies: Vec<DefId>,
    },
    AddHeritageInstantiation {
        derived: DefId,
        parent: DefId,
        base_type: TypeId,
    },
    CacheResolvedCrossFileQuery {
        kind: u8,
        file_idx: u32,
        primary: u32,
        secondary: u32,
        args_hash: u64,
        type_id: TypeId,
        type_params: Vec<TypeParamInfo>,
    },
    RegisterModuleAugmentedBody {
        def_id: DefId,
        body: TypeId,
        source_files: Vec<u32>,
    },
    RegisterAugmentedBaseBodyDef {
        symbol_id: u32,
        def_id: DefId,
    },
    RegisterModuleAugmentationSymbolDef {
        symbol_id: u32,
        def_id: DefId,
    },
}

impl AugmentationPublication {
    fn apply(self, store: &DefinitionStore) {
        match self {
            Self::SetBody {
                id,
                body,
                params,
                finalized,
            } => {
                if finalized {
                    store.set_body_finalized(id, body, params);
                } else {
                    store.set_body_with_params(id, body, params);
                }
            }
            Self::SetStaticShape { id, shape } => store.set_static_shape(id, shape),
            Self::MarkPublishOnce(id) => store.mark_publish_once(id),
            Self::MarkDepthPoisoned(id) => store.mark_depth_poisoned(id),
            Self::MarkCircular(id) => store.mark_circular_def(id),
            Self::MarkBodyComputed(body) => store.mark_body_as_computed(body),
            Self::MarkBodyDirectlyNamed(body) => store.mark_body_as_directly_named(body),
            Self::MarkTupleSpreadFlattenedAlias(id) => {
                store.mark_tuple_spread_flattened_alias(id);
            }
            Self::SetTypeParams { id, params } => store.set_type_params(id, params),
            Self::SetInstanceShape { id, shape } => store.set_instance_shape(id, shape),
            Self::RegisterTypeToDef { type_id, def_id } => {
                store.register_type_to_def(type_id, def_id);
            }
            Self::RegisterClassInstanceType {
                class_def,
                instance_type,
            } => store.register_class_instance_type(class_def, instance_type),
            Self::RegisterTypeofValueLiteral { symbol_id, literal } => {
                store.register_typeof_value_literal(symbol_id, literal);
            }
            Self::SetBodyDependencies { id, dependencies } => {
                store.set_body_dependency_defs(id, dependencies);
            }
            Self::AddHeritageInstantiation {
                derived,
                parent,
                base_type,
            } => store.add_heritage_instantiation(derived, parent, base_type),
            Self::CacheResolvedCrossFileQuery {
                kind,
                file_idx,
                primary,
                secondary,
                args_hash,
                type_id,
                type_params,
            } => store.cache_resolved_cross_file_query(
                kind,
                file_idx,
                primary,
                secondary,
                args_hash,
                type_id,
                type_params,
            ),
            Self::RegisterModuleAugmentedBody {
                def_id,
                body,
                source_files,
            } => {
                store.register_module_augmented_body(def_id, body, &source_files);
            }
            Self::RegisterAugmentedBaseBodyDef { symbol_id, def_id } => {
                store.register_augmented_base_body_def(symbol_id, def_id);
            }
            Self::RegisterModuleAugmentationSymbolDef { symbol_id, def_id } => {
                store.register_module_augmentation_symbol_def(symbol_id, def_id);
            }
        }
    }
}

impl DefinitionStore {
    /// Begin an isolated, nested-safe augmentation publication overlay.
    #[must_use]
    pub fn begin_augmentation_publication(self: &Arc<Self>) -> Arc<Self> {
        let mut overlay = Self::with_capacities(0, 0);
        overlay
            .cross_file_cache
            .set_scope(self.source_file_symbol_type_cache_scope());
        overlay.augmentation_parent = Some(Arc::clone(self));
        overlay.augmentation_publications = std::sync::Mutex::new(Some(Vec::new()));
        Arc::new(overlay)
    }

    /// Commit this overlay into its parent and return that parent.
    ///
    /// A nested commit only replays into the outer overlay. The shared base
    /// store is reached solely when the outermost overlay commits.
    #[must_use]
    pub fn commit_augmentation_publication(&self) -> Option<Arc<Self>> {
        let parent = self.augmentation_parent.as_ref()?.clone();
        let publications = self
            .augmentation_publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()?;
        for publication in publications {
            publication.apply(&parent);
        }
        Some(parent)
    }

    /// Discard this overlay and return its parent without publishing writes.
    #[must_use]
    pub fn rollback_augmentation_publication(&self) -> Option<Arc<Self>> {
        let parent = self.augmentation_parent.as_ref()?.clone();
        self.augmentation_publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()?;
        Some(parent)
    }

    pub(super) fn record_augmentation_publication(&self, publication: AugmentationPublication) {
        if self.augmentation_parent.is_none() {
            return;
        }
        let mut publications = self
            .augmentation_publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(publications) = publications.as_mut() {
            publications.push(publication);
        }
    }

    pub(super) fn record_augmentation_publication_with(
        &self,
        publication: impl FnOnce() -> AugmentationPublication,
    ) {
        if self.augmentation_parent.is_some() {
            self.record_augmentation_publication(publication());
        }
    }

    pub(super) const fn augmentation_parent(&self) -> Option<&Arc<Self>> {
        self.augmentation_parent.as_ref()
    }

    pub(super) fn augmentation_root(&self) -> &Self {
        self.augmentation_parent
            .as_ref()
            .map_or(self, |parent| parent.augmentation_root())
    }

    pub(super) fn stage_registered_definition_semantics(
        &self,
        id: DefId,
        semantics: AugmentationDefinitionSemantics,
    ) {
        if let Some(body) = semantics.body {
            self.set_body_with_params(id, body, Some(semantics.type_params));
        } else if semantics.has_type_param_semantics {
            self.set_type_params(id, semantics.type_params);
        }
        if let Some(shape) = semantics.instance_shape {
            self.set_instance_shape(id, shape);
        }
        if let Some(shape) = semantics.static_shape {
            self.set_static_shape(id, shape);
        }
    }

    /// Retain newly discovered declaration-parameter identity across rollback.
    ///
    /// Parameter names, arity, `const` markers, and origins come from syntax
    /// and are stable. Constraints and defaults contain checker-arena `TypeId`s
    /// and remain staged. Only fill a missing list: an already-published
    /// non-empty declaration identity is first-wins and must not be replaced by
    /// a speculative resolution of a colliding or stale definition.
    pub(super) fn retain_augmentation_type_param_identity(
        &self,
        id: DefId,
        params: &[TypeParamInfo],
    ) {
        if self.augmentation_parent.is_none() || params.is_empty() {
            return;
        }
        self.retain_type_param_identity_in_layer(id, params);
    }

    fn retain_type_param_identity_in_layer(&self, id: DefId, params: &[TypeParamInfo]) {
        if let Some(parent) = &self.augmentation_parent {
            parent.retain_type_param_identity_in_layer(id, params);
        }
        self.ensure_augmentation_definition(id);
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            if !entry.type_params.is_empty() {
                return;
            }
            let old_decl_site_key = Self::decl_site_key_for_info(&entry);
            if entry.kind == super::DefKind::TypeAlias
                && let Some(body) = entry.body
            {
                self.body_to_alias.remove(&body);
                if self.augmentation_parent.is_some() {
                    self.augmentation_removed_alias_bodies.insert(body);
                }
            }
            entry.type_params = stripped_type_param_identity(params);
            self.refresh_decl_site_identity(id, old_decl_site_key, &entry);
            self.bump_generation();
        } else {
            self.definitions.insert(
                id,
                DefinitionInfo {
                    kind: super::DefKind::Interface,
                    name: tsz_common::interner::Atom::default(),
                    type_params: stripped_type_param_identity(params),
                    body: None,
                    instance_shape: None,
                    static_shape: None,
                    extends: None,
                    implements: Vec::new(),
                    enum_members: Vec::new(),
                    exports: Vec::new(),
                    file_id: None,
                    span: None,
                    symbol_id: self.get_symbol_id(id),
                    heritage_names: Vec::new(),
                    is_abstract: false,
                    is_const: false,
                    is_exported: false,
                    is_global_augmentation: false,
                    is_declare: false,
                },
            );
            self.bump_generation();
        }
    }

    pub(super) fn alias_forward(&self, id: DefId) -> Option<DefId> {
        self.alias_forwards
            .get(&id)
            .map(|target| *target)
            .or_else(|| {
                self.augmentation_parent
                    .as_ref()
                    .and_then(|parent| parent.alias_forward(id))
            })
    }

    pub(super) fn augmentation_is_publish_once_frozen(&self, id: DefId) -> bool {
        self.state_flags.is_publish_once_frozen(id)
            || self
                .augmentation_parent
                .as_ref()
                .is_some_and(|parent| parent.augmentation_is_publish_once_frozen(id))
    }

    pub(super) fn augmentation_is_deferred_publish(&self, id: DefId) -> bool {
        self.state_flags.is_deferred_publish(id)
            || self
                .augmentation_parent
                .as_ref()
                .is_some_and(|parent| parent.augmentation_is_deferred_publish(id))
    }

    pub(super) fn augmentation_body_computed_marked(&self, body: TypeId) -> bool {
        self.state_flags.is_body_computed_marked(body)
            || self
                .augmentation_parent
                .as_ref()
                .is_some_and(|parent| parent.augmentation_body_computed_marked(body))
    }

    pub(super) fn augmentation_body_directly_named(&self, body: TypeId) -> bool {
        self.state_flags.is_body_directly_named(body)
            || self
                .augmentation_parent
                .as_ref()
                .is_some_and(|parent| parent.augmentation_body_directly_named(body))
    }

    pub(super) fn augmentation_alias_for_body(&self, body: TypeId) -> Option<DefId> {
        if self.augmentation_removed_alias_bodies.contains(&body) {
            return None;
        }
        self.body_to_alias
            .get(&body)
            .map(|entry| *entry)
            .or_else(|| {
                self.augmentation_parent
                    .as_ref()
                    .and_then(|parent| parent.augmentation_alias_for_body(body))
            })
    }

    /// Copy a parent definition into the sparse overlay before mutating it.
    pub(super) fn ensure_augmentation_definition(&self, id: DefId) {
        if self.definitions.contains_key(&id) {
            return;
        }
        if let Some(info) = self
            .augmentation_parent
            .as_ref()
            .and_then(|parent| parent.get(id))
        {
            self.definitions.insert(id, info);
        }
    }
}

fn stripped_type_param_identity(params: &[TypeParamInfo]) -> Vec<TypeParamInfo> {
    params
        .iter()
        .map(|param| TypeParamInfo {
            constraint: None,
            default: None,
            ..*param
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefinitionInfo;
    use tsz_common::interner::Atom;

    fn store_with_interface() -> (Arc<DefinitionStore>, DefId) {
        let store = Arc::new(DefinitionStore::new());
        let def_id = store.register(DefinitionInfo::interface(
            Atom::default(),
            Vec::new(),
            Vec::new(),
        ));
        (store, def_id)
    }

    #[test]
    fn rollback_discards_results_but_keeps_stable_symbol_identity() {
        let (store, def_id) = store_with_interface();
        let heritage_parent = store.register(DefinitionInfo::interface(
            Atom::default(),
            Vec::new(),
            Vec::new(),
        ));
        let overlay = store.begin_augmentation_publication();
        let body = TypeId(100);

        overlay.set_body(def_id, body);
        overlay.register_type_to_def(body, def_id);
        overlay.register_class_instance_type(def_id, TypeId(101));
        overlay.register_typeof_value_literal(17, TypeId(102));
        overlay.cache_resolved_cross_file_query(9, 3, 4, 5, 6, TypeId(103), Vec::new());
        overlay.add_heritage_instantiation(def_id, heritage_parent, TypeId(104));
        assert!(overlay.register_module_augmented_body(def_id, TypeId(105), &[8]));
        overlay.mark_publish_once(def_id);
        overlay.mark_depth_poisoned(def_id);
        overlay.mark_circular_def(def_id);
        overlay.register_augmented_base_body_def(78, def_id);
        overlay.register_module_augmentation_symbol_def(79, def_id);

        let (identity_def, minted) = overlay.register_for_symbol(
            77,
            8,
            DefinitionInfo::interface(Atom::default(), Vec::new(), Vec::new()),
        );
        assert!(minted);
        assert_eq!(store.lookup_by_symbol(77, 8), Some(identity_def));
        assert_eq!(overlay.get_body(def_id), Some(body));
        assert_eq!(store.get_body(def_id), None);

        let parent = overlay
            .rollback_augmentation_publication()
            .expect("overlay has a parent");
        assert!(Arc::ptr_eq(&parent, &store));
        assert_eq!(store.get_body(def_id), None);
        assert_eq!(store.find_def_for_type(body), None);
        assert_eq!(store.get_class_instance_type(def_id), None);
        assert_eq!(store.get_typeof_value_literal(17), None);
        assert_eq!(store.get_resolved_cross_file_query(9, 3, 4, 5, 6), None);
        assert_eq!(
            store.get_heritage_instantiation(def_id, heritage_parent),
            None
        );
        assert_eq!(store.module_augmented_body_entry(def_id), None);
        assert_eq!(store.lookup_by_symbol(77, 8), Some(identity_def));
        assert!(!store.augmentation_is_publish_once_frozen(def_id));
        assert!(!store.is_depth_poisoned(def_id));
        assert!(!store.is_circular_def(def_id));
        assert_eq!(store.augmented_base_body_def_for_symbol(78), None);
        assert_eq!(store.find_def_by_symbol(79), None);
    }

    #[test]
    fn commit_publishes_staged_results() {
        let (store, def_id) = store_with_interface();
        let overlay = store.begin_augmentation_publication();
        let body = TypeId(110);

        overlay.set_body(def_id, body);
        overlay.register_type_to_def(body, def_id);
        overlay.cache_resolved_cross_file_query(10, 4, 5, 6, 7, TypeId(111), Vec::new());
        assert!(overlay.register_module_augmented_body(def_id, TypeId(112), &[9]));
        let parent = overlay
            .commit_augmentation_publication()
            .expect("overlay has a parent");

        assert!(Arc::ptr_eq(&parent, &store));
        assert_eq!(store.get_body(def_id), Some(body));
        assert_eq!(store.find_def_for_type(body), Some(def_id));
        assert_eq!(
            store
                .get_resolved_cross_file_query(10, 4, 5, 6, 7)
                .map(|entry| entry.0),
            Some(TypeId(111))
        );
        assert_eq!(
            store.module_augmented_body_entry(def_id),
            Some((TypeId(112), vec![9]))
        );
    }

    #[test]
    fn registered_definition_semantics_remain_private_until_commit() {
        let store = Arc::new(DefinitionStore::new());
        let overlay = store.begin_augmentation_publication();
        let mut param = TypeParamInfo::simple(Atom::default());
        param.constraint = Some(TypeId(150));
        param.default = Some(TypeId(151));
        let def_id = overlay.register(DefinitionInfo::type_alias(
            Atom::default(),
            vec![param],
            TypeId(152),
        ));

        let base_info = store.get(def_id).expect("stable identity is published");
        assert_eq!(base_info.body, None);
        assert_eq!(base_info.type_params[0].constraint, None);
        assert_eq!(base_info.type_params[0].default, None);
        let overlay_info = overlay.get(def_id).expect("overlay has semantic state");
        assert_eq!(overlay_info.body, Some(TypeId(152)));
        assert_eq!(overlay_info.type_params, vec![param]);
        assert_eq!(store.find_type_alias_by_body(TypeId(152)), None);
        assert_eq!(
            overlay.find_type_alias_by_body(TypeId(152)),
            None,
            "generic aliases are never indexed by their uninstantiated body"
        );

        let non_generic_alias = overlay.register(DefinitionInfo::type_alias(
            Atom::default(),
            Vec::new(),
            TypeId(153),
        ));
        assert_eq!(store.find_type_alias_by_body(TypeId(153)), None);
        assert_eq!(
            overlay.find_type_alias_by_body(TypeId(153)),
            Some(non_generic_alias)
        );

        overlay
            .commit_augmentation_publication()
            .expect("overlay has a parent");
        let committed = store.get(def_id).expect("definition was committed");
        assert_eq!(committed.body, Some(TypeId(152)));
        assert_eq!(committed.type_params, vec![param]);
        assert_eq!(
            store.find_type_alias_by_body(TypeId(153)),
            Some(non_generic_alias)
        );
    }

    #[test]
    fn retry_after_rollback_reuses_identity_and_can_publish_semantics() {
        let store = Arc::new(DefinitionStore::new());
        let first = store.begin_augmentation_publication();
        let info = DefinitionInfo::type_alias(Atom::default(), Vec::new(), TypeId(160));
        let (def_id, minted) = first.register_for_symbol(80, 9, info.clone());
        assert!(minted);
        assert_eq!(store.get_body(def_id), None);
        assert_eq!(first.get_body(def_id), Some(TypeId(160)));
        first
            .rollback_augmentation_publication()
            .expect("overlay has a parent");
        assert_eq!(store.get_body(def_id), None);

        let retry = store.begin_augmentation_publication();
        let (retry_id, minted) = retry.register_for_symbol(80, 9, info);
        assert_eq!(retry_id, def_id);
        assert!(!minted);
        assert_eq!(retry.get_body(def_id), Some(TypeId(160)));
        retry
            .commit_augmentation_publication()
            .expect("overlay has a parent");
        assert_eq!(store.get_body(def_id), Some(TypeId(160)));
    }

    #[test]
    fn discovered_type_param_identity_survives_body_rollback() {
        let store = Arc::new(DefinitionStore::new());
        let mut initial = DefinitionInfo::interface(Atom::default(), Vec::new(), Vec::new());
        initial.file_id = Some(10);
        initial.span = Some((20, 21));
        let (def_id, minted) = store.register_for_symbol(81, 10, initial);
        assert!(minted);

        let mut param = TypeParamInfo::simple(Atom::default());
        param.constraint = Some(TypeId(170));
        param.default = Some(TypeId(171));
        let overlay = store.begin_augmentation_publication();
        overlay.set_body_with_params(def_id, TypeId(172), Some(vec![param]));

        let stable = store.get(def_id).expect("stable identity remains shared");
        assert_eq!(stable.body, None);
        assert_eq!(stable.type_params.len(), 1);
        assert_eq!(stable.type_params[0].constraint, None);
        assert_eq!(stable.type_params[0].default, None);
        assert_eq!(overlay.get_type_params(def_id), Some(vec![param]));
        overlay
            .rollback_augmentation_publication()
            .expect("overlay has a parent");

        let mut retry = DefinitionInfo::interface(Atom::default(), vec![param], Vec::new());
        retry.file_id = Some(10);
        retry.span = Some((20, 21));
        let (retry_id, minted) = store.register_for_symbol(81, 10, retry);
        assert_eq!(retry_id, def_id);
        assert!(!minted);
        assert_eq!(store.get_body(def_id), None);
    }

    #[test]
    fn set_type_params_retains_only_identity_on_rollback() {
        let (store, def_id) = store_with_interface();
        let mut param = TypeParamInfo::simple(Atom::default());
        param.constraint = Some(TypeId(180));
        let overlay = store.begin_augmentation_publication();

        overlay.set_type_params(def_id, vec![param]);
        assert_eq!(overlay.get_type_params(def_id), Some(vec![param]));
        let stable = store
            .get_type_params(def_id)
            .expect("identity was retained");
        assert_eq!(stable.len(), 1);
        assert_eq!(stable[0].constraint, None);
        overlay
            .rollback_augmentation_publication()
            .expect("overlay has a parent");
        assert_eq!(store.get_type_params(def_id), Some(stable));
    }

    #[test]
    fn nested_commit_remains_private_until_outer_commit() {
        let (store, def_id) = store_with_interface();
        let outer = store.begin_augmentation_publication();
        let inner = outer.begin_augmentation_publication();
        let body = TypeId(120);

        inner.set_body(def_id, body);
        let inner_parent = inner
            .commit_augmentation_publication()
            .expect("inner overlay has a parent");
        assert!(Arc::ptr_eq(&inner_parent, &outer));
        assert_eq!(outer.get_body(def_id), Some(body));
        assert_eq!(store.get_body(def_id), None);

        let outer_parent = outer
            .commit_augmentation_publication()
            .expect("outer overlay has a parent");
        assert!(Arc::ptr_eq(&outer_parent, &store));
        assert_eq!(store.get_body(def_id), Some(body));
    }

    #[test]
    fn outer_rollback_discards_nested_commit() {
        let (store, def_id) = store_with_interface();
        let outer = store.begin_augmentation_publication();
        let inner = outer.begin_augmentation_publication();

        inner.set_body(def_id, TypeId(130));
        inner
            .commit_augmentation_publication()
            .expect("inner overlay has a parent");
        outer
            .rollback_augmentation_publication()
            .expect("outer overlay has a parent");

        assert_eq!(store.get_body(def_id), None);
    }

    #[test]
    fn dependency_clear_shadows_parent_and_commits_as_a_clear() {
        let (store, def_id) = store_with_interface();
        let dependency = store.register(DefinitionInfo::interface(
            Atom::default(),
            Vec::new(),
            Vec::new(),
        ));
        store.set_body_dependency_defs(def_id, [dependency]);

        let overlay = store.begin_augmentation_publication();
        overlay.set_body_dependency_defs(def_id, []);
        assert_eq!(overlay.body_dependency_defs(def_id), None);
        assert_eq!(
            store.body_dependency_defs(def_id).as_deref(),
            Some([dependency].as_slice())
        );
        overlay
            .commit_augmentation_publication()
            .expect("overlay has a parent");
        assert_eq!(store.body_dependency_defs(def_id), None);
    }

    #[test]
    fn existing_parent_cross_file_result_remains_first_writer() {
        let store = Arc::new(DefinitionStore::new());
        store.cache_resolved_cross_file_query(11, 5, 6, 7, 8, TypeId(140), Vec::new());
        let overlay = store.begin_augmentation_publication();

        overlay.cache_resolved_cross_file_query(11, 5, 6, 7, 8, TypeId(141), Vec::new());
        assert_eq!(
            overlay
                .get_resolved_cross_file_query(11, 5, 6, 7, 8)
                .map(|entry| entry.0),
            Some(TypeId(140))
        );
        overlay
            .commit_augmentation_publication()
            .expect("overlay has a parent");
        assert_eq!(
            store
                .get_resolved_cross_file_query(11, 5, 6, 7, 8)
                .map(|entry| entry.0),
            Some(TypeId(140))
        );
    }
}
