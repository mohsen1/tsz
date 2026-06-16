//! Deferred flow-environment writes.
//!
//! Captures dual-environment registrations that must be applied to the
//! flow-analyzer environment (`type_environment`) but lost the `RefCell` borrow
//! race when first attempted, so they can be replayed later without holding any
//! borrow of the originating `CheckerContext`.

use std::sync::Arc;

use tsz_binder::SymbolId;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

use crate::query_boundaries::common::TypeEnvironment;

/// A dual-environment registration that must be applied to the flow-analyzer
/// environment (`type_environment`) but lost the `RefCell` borrow race when it
/// was first attempted.
///
/// Each variant carries fully-owned data so the operation can be replayed later
/// without holding any borrow of the originating `CheckerContext`. Replaying a
/// deferred write reproduces exactly the same `TypeEnvironment` mutation the
/// direct dual-write would have performed, which is why the previous full
/// per-file `clone()` repair is no longer required.
#[derive(Clone, Debug)]
pub enum DeferredFlowEnvWrite {
    /// `set_definition_store` — wire the shared `DefinitionStore` fallback.
    SetDefinitionStore(Arc<tsz_solver::def::DefinitionStore>),
    /// `insert_def` — register a non-generic definition body.
    InsertDef { def_id: DefId, body: TypeId },
    /// `insert_def_with_params` (+ optional declared variances) — register a
    /// generic definition body.
    InsertDefWithParams {
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
        variances: Option<Arc<[tsz_solver::type_handles::Variance]>>,
    },
    /// `insert_class_instance_type` — register a class instance type.
    InsertClassInstance {
        def_id: DefId,
        instance_type: TypeId,
    },
    /// `register_class_extends` — register a class `extends` parent.
    RegisterClassExtends { def_id: DefId, parent_def_id: DefId },
    /// `register_def_symbol_mapping` — register the `DefId` <-> `SymbolId` bridge.
    RegisterDefSymbolMapping { def_id: DefId, sym_id: SymbolId },
    /// `register_augmented_def` — re-apply an augmentation merge.
    RegisterAugmentedDef {
        def_id: DefId,
        augmented: TypeId,
        is_class: bool,
    },
    /// `insert_def_kind` — register a `DefKind`.
    InsertDefKind {
        def_id: DefId,
        kind: tsz_solver::def::DefKind,
    },
}

impl DeferredFlowEnvWrite {
    /// Apply this deferred registration to a flow-analyzer `TypeEnvironment`.
    pub(crate) fn apply(&self, env: &mut TypeEnvironment) {
        match self {
            Self::SetDefinitionStore(store) => env.set_definition_store(Arc::clone(store)),
            Self::InsertDef { def_id, body } => env.insert_def(*def_id, *body),
            Self::InsertDefWithParams {
                def_id,
                body,
                params,
                variances,
            } => {
                env.insert_def_with_params(*def_id, *body, params.clone());
                if let Some(variances) = variances {
                    env.insert_declared_variances(*def_id, Arc::clone(variances));
                }
            }
            Self::InsertClassInstance {
                def_id,
                instance_type,
            } => env.insert_class_instance_type(*def_id, *instance_type),
            Self::RegisterClassExtends {
                def_id,
                parent_def_id,
            } => env.register_class_extends(*def_id, *parent_def_id),
            Self::RegisterDefSymbolMapping { def_id, sym_id } => {
                env.register_def_symbol_mapping(*def_id, *sym_id);
            }
            Self::RegisterAugmentedDef {
                def_id,
                augmented,
                is_class,
            } => apply_augmented_def(env, *def_id, *augmented, *is_class),
            Self::InsertDefKind { def_id, kind } => env.insert_def_kind(*def_id, *kind),
        }
    }
}

/// Apply an augmentation merge to a single environment.
///
/// Shared by the live dual-write path and deferred replay so both produce
/// identical results: class-like defs update the instance-type slot, other defs
/// re-insert the body while preserving any existing type parameters.
fn apply_augmented_def(
    env: &mut TypeEnvironment,
    def_id: DefId,
    augmented: TypeId,
    is_class: bool,
) {
    if is_class || env.get_class_instance_type(def_id).is_some() {
        env.insert_class_instance_type(def_id, augmented);
    } else {
        // Read params through the shared-store fallback (`get_def_params_owned`)
        // rather than the local-only `get_def_params`: when a sibling fresh
        // checker derived the def, its param list lives only in the shared
        // `DefinitionStore`, and a local-only read would drop the arity here and
        // re-insert the augmented body as if non-generic (#13255 shared-def
        // observation family).
        if let Some(params) = env.get_def_params_owned(def_id) {
            env.insert_def_with_params(def_id, augmented, params);
        } else {
            env.insert_def(def_id, augmented);
        }
    }
}
