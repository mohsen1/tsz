//! Sparse nested rollback journal for checker-local [`TypeEnvironment`] state.

use super::TypeEnvironment;
use crate::def::{DefId, DefinitionStore};
use crate::types::{IntrinsicKind, TypeId, TypeParamInfo, Variance};
use rustc_hash::FxHashSet;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum UndoKey {
    Types(u32),
    TypeParams(u32),
    BoxedTypes(IntrinsicKind),
    ArrayBase,
    ReadonlyArrayBase,
    DefTypes(u32),
    DefTypeParams(u32),
    DeclaredVariances(u32),
    EnumNamespaceTypes(u32),
    ClassInstanceTypes(u32),
    InstanceTypeToClass(u32),
    DefinitionStore,
    ThisType,
    TypeofValueTypes(u32),
}

#[derive(Clone, Debug)]
pub(super) enum TypeEnvironmentUndo {
    Types(u32, Option<TypeId>),
    TypeParams(u32, Option<Vec<TypeParamInfo>>),
    BoxedTypes(IntrinsicKind, Option<TypeId>),
    ArrayBase(Option<TypeId>, Vec<TypeParamInfo>),
    ReadonlyArrayBase(Option<TypeId>),
    DefTypes(u32, Option<TypeId>),
    DefTypeParams(u32, Option<Vec<TypeParamInfo>>),
    DeclaredVariances(u32, Option<Arc<[Variance]>>),
    EnumNamespaceTypes(u32, Option<TypeId>),
    ClassInstanceTypes(u32, Option<TypeId>),
    InstanceTypeToClass(u32, Option<DefId>),
    DefinitionStore(Option<Arc<DefinitionStore>>),
    ThisType(Option<TypeId>),
    TypeofValueTypes(u32, Option<TypeId>),
}

impl TypeEnvironmentUndo {
    const fn key(&self) -> UndoKey {
        match self {
            Self::Types(key, _) => UndoKey::Types(*key),
            Self::TypeParams(key, _) => UndoKey::TypeParams(*key),
            Self::BoxedTypes(key, _) => UndoKey::BoxedTypes(*key),
            Self::ArrayBase(..) => UndoKey::ArrayBase,
            Self::ReadonlyArrayBase(_) => UndoKey::ReadonlyArrayBase,
            Self::DefTypes(key, _) => UndoKey::DefTypes(*key),
            Self::DefTypeParams(key, _) => UndoKey::DefTypeParams(*key),
            Self::DeclaredVariances(key, _) => UndoKey::DeclaredVariances(*key),
            Self::EnumNamespaceTypes(key, _) => UndoKey::EnumNamespaceTypes(*key),
            Self::ClassInstanceTypes(key, _) => UndoKey::ClassInstanceTypes(*key),
            Self::InstanceTypeToClass(key, _) => UndoKey::InstanceTypeToClass(*key),
            Self::DefinitionStore(_) => UndoKey::DefinitionStore,
            Self::ThisType(_) => UndoKey::ThisType,
            Self::TypeofValueTypes(key, _) => UndoKey::TypeofValueTypes(*key),
        }
    }

    fn apply(self, environment: &mut TypeEnvironment) {
        macro_rules! restore_map {
            ($map:expr, $key:expr, $old:expr) => {
                if let Some(old) = $old {
                    let _ = $map.insert($key, old);
                } else {
                    let _ = $map.remove(&$key);
                }
            };
        }
        match self {
            Self::Types(key, old) => restore_map!(environment.types, key, old),
            Self::TypeParams(key, old) => restore_map!(environment.type_params, key, old),
            Self::BoxedTypes(key, old) => restore_map!(environment.boxed_types, key, old),
            Self::ArrayBase(old_type, old_params) => {
                environment.array_base_type = old_type;
                environment.array_base_type_params = old_params;
            }
            Self::ReadonlyArrayBase(old) => environment.readonly_array_base_type = old,
            Self::DefTypes(key, old) => restore_map!(environment.def_types, key, old),
            Self::DefTypeParams(key, old) => {
                restore_map!(environment.def_type_params, key, old);
            }
            Self::DeclaredVariances(key, old) => {
                restore_map!(environment.declared_variances, key, old);
            }
            Self::EnumNamespaceTypes(key, old) => {
                restore_map!(environment.enum_namespace_types, key, old);
            }
            Self::ClassInstanceTypes(key, old) => {
                restore_map!(environment.class_instance_types, key, old);
            }
            Self::InstanceTypeToClass(key, old) => {
                restore_map!(environment.instance_type_to_class, key, old);
            }
            Self::DefinitionStore(old) => environment.definition_store = old,
            Self::ThisType(old) => environment.this_type = old,
            Self::TypeofValueTypes(key, old) => {
                restore_map!(environment.typeof_value_types, key, old);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct TypeEnvironmentAugmentationJournal {
    generation: u64,
    retained_generation_bumps: u64,
    seen: FxHashSet<UndoKey>,
    undos: Vec<TypeEnvironmentUndo>,
}

/// Transaction journals are checker-local control state, not semantic
/// environment data. Cloning a `TypeEnvironment` for a child checker starts
/// with no active transaction scopes.
#[derive(Debug, Default)]
pub(super) struct TypeEnvironmentAugmentationJournals(Vec<TypeEnvironmentAugmentationJournal>);

impl Clone for TypeEnvironmentAugmentationJournals {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl TypeEnvironment {
    /// Begin one nested sparse rollback scope.
    pub fn begin_augmentation_transaction(&mut self) {
        self.augmentation_journals
            .0
            .push(TypeEnvironmentAugmentationJournal {
                generation: self.generation,
                retained_generation_bumps: 0,
                seen: FxHashSet::default(),
                undos: Vec::new(),
            });
    }

    /// Keep mutations from the innermost scope.
    pub fn commit_augmentation_transaction(&mut self) {
        self.augmentation_journals
            .0
            .pop()
            .expect("type-environment augmentation transaction must be active");
    }

    /// Restore mutations from the innermost scope.
    pub fn rollback_augmentation_transaction(&mut self) {
        let journal = self
            .augmentation_journals
            .0
            .pop()
            .expect("type-environment augmentation transaction must be active");
        for undo in journal.undos.into_iter().rev() {
            undo.apply(self);
        }
        self.generation = journal
            .generation
            .saturating_add(journal.retained_generation_bumps);
    }

    /// Bump resolver generation for stable identity state that deliberately
    /// survives rollback, and retain that bump in every enclosing scope.
    pub(super) fn bump_retained_augmentation_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        for journal in &mut self.augmentation_journals.0 {
            journal.retained_generation_bumps = journal.retained_generation_bumps.saturating_add(1);
        }
    }

    pub(super) fn record_augmentation_undo_with(
        &mut self,
        undo: impl FnOnce(&Self) -> TypeEnvironmentUndo,
    ) {
        if self.augmentation_journals.0.is_empty() {
            return;
        }
        let undo = undo(self);
        let key = undo.key();
        for journal in &mut self.augmentation_journals.0 {
            if journal.seen.insert(key.clone()) {
                journal.undos.push(undo.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefKind;
    use crate::def::resolver::TypeResolver;
    use crate::types::SymbolRef;

    #[test]
    fn rollback_restores_results_but_retains_identity_and_generation() {
        let mut environment = TypeEnvironment::new();
        let generation_before = environment.generation();
        let definition = DefId(41);
        let symbol = tsz_binder::SymbolId(42);

        environment.begin_augmentation_transaction();
        environment.insert(SymbolRef(1), TypeId(100));
        environment.insert_def_kind(definition, DefKind::Interface);
        environment.register_def_symbol_mapping(definition, symbol);
        environment.rollback_augmentation_transaction();

        assert_eq!(environment.get(SymbolRef(1)), None);
        assert_eq!(
            environment.get_def_kind(definition),
            Some(DefKind::Interface)
        );
        assert_eq!(
            TypeResolver::symbol_to_def_id(&environment, SymbolRef(symbol.0)),
            Some(definition)
        );
        assert_eq!(environment.generation(), generation_before + 2);
    }

    #[test]
    fn nested_rollback_restores_outer_result() {
        let mut environment = TypeEnvironment::new();
        let symbol = SymbolRef(3);

        environment.begin_augmentation_transaction();
        environment.insert(symbol, TypeId(110));
        environment.begin_augmentation_transaction();
        environment.insert(symbol, TypeId(111));
        environment.rollback_augmentation_transaction();
        assert_eq!(environment.get(symbol), Some(TypeId(110)));

        environment.commit_augmentation_transaction();
        assert_eq!(environment.get(symbol), Some(TypeId(110)));
    }

    #[test]
    fn outer_rollback_discards_result_from_nested_commit() {
        let mut environment = TypeEnvironment::new();
        let symbol = SymbolRef(4);

        environment.begin_augmentation_transaction();
        environment.begin_augmentation_transaction();
        environment.insert(symbol, TypeId(120));
        environment.commit_augmentation_transaction();
        assert_eq!(environment.get(symbol), Some(TypeId(120)));

        environment.rollback_augmentation_transaction();
        assert_eq!(environment.get(symbol), None);
    }
}
