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
    /// `insert_def` when absent — merge a child env snapshot without
    /// overwriting an already-published parent body.
    InsertDefIfMissing { def_id: DefId, body: TypeId },
    /// `insert_def_with_params` (+ optional declared variances) — register a
    /// generic definition body.
    InsertDefWithParams {
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
        variances: Option<Arc<[tsz_solver::type_handles::Variance]>>,
    },
    /// `insert_class_instance_type` — register a class instance type.
    /// `provisional` is true when the class was still mid-resolution at
    /// registration time, so the body is a prescan/rough partial: the env
    /// marks the def provisional and `resolve_lazy` serves of it taint
    /// overlapping evaluations until the final registration clears the mark
    /// (issue #16055).
    InsertClassInstance {
        def_id: DefId,
        instance_type: TypeId,
        provisional: bool,
    },
    /// `insert_class_instance_type` when absent — merge a child env snapshot
    /// without overwriting parent metadata.
    InsertClassInstanceIfMissing {
        def_id: DefId,
        instance_type: TypeId,
    },
    /// `register_class_extends` — register a class `extends` parent.
    RegisterClassExtends { def_id: DefId, parent_def_id: DefId },
    /// `register_class_extends` when absent — merge a child env snapshot
    /// without overwriting a parent edge.
    RegisterClassExtendsIfMissing { def_id: DefId, parent_def_id: DefId },
    /// `register_interface_extends` — register a checker-verified (no TS2430)
    /// interface `extends` parent.
    RegisterInterfaceExtends { def_id: DefId, parent_def_id: DefId },
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
    /// `insert_unresolved_resolution` — cache a resolved bare type-name so the
    /// first-pass evaluator (which uses `TypeEnvironment` as its resolver) can
    /// reduce `Application(UnresolvedTypeName(name), args)` without bouncing
    /// back into the checker resolver.
    InsertUnresolvedResolution { name: String, def_id: DefId },
    /// `insert_typeof_value_type` — register a merged interface+value symbol's
    /// `typeof` value-space type.
    InsertTypeofValueType {
        symbol: tsz_solver::SymbolRef,
        value_type: TypeId,
    },
    /// `set_boxed_type` — register the boxed interface for a primitive.
    RegisterBoxedType {
        kind: tsz_solver::IntrinsicKind,
        type_id: TypeId,
    },
    /// `set_array_base_type` — register the canonical `Array<T>` base type.
    RegisterArrayBaseType {
        type_id: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    },
    /// `insert_def` + `register_boxed_def_id` — register a boxed interface's
    /// `Lazy(DefId)` body and intrinsic-kind marker.
    RegisterBoxedDef {
        kind: tsz_solver::IntrinsicKind,
        type_id: TypeId,
        def_id: DefId,
    },
    /// `register_well_known_symbol_name` — register a `[Symbol.*]` computed
    /// property name's backing `SymbolRef`.
    RegisterWellKnownSymbolName {
        name: String,
        symbol_ref: tsz_solver::SymbolRef,
    },
    /// `register_enum_namespace_type` — register an enum's namespace object type
    /// for `typeof Enum` / `keyof typeof Enum`.
    RegisterEnumNamespaceType { def_id: DefId, ns_type: TypeId },
    /// `register_numeric_enum` — register an enum as numeric for enum/number
    /// compatibility rules.
    RegisterNumericEnum { def_id: DefId },
    /// `register_enum_parent` — register an enum member's parent enum for member
    /// widening and discriminant narrowing.
    RegisterEnumParent {
        member_def_id: DefId,
        parent_def_id: DefId,
    },
    /// `insert` — register a symbol's resolved type (`SymbolRef -> TypeId`).
    InsertSymbolType {
        symbol: tsz_solver::SymbolRef,
        ty: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    },
}

impl DeferredFlowEnvWrite {
    /// Build the body-registration variant for a definition, selecting
    /// [`Self::InsertDef`] when `params` is empty and
    /// [`Self::InsertDefWithParams`] otherwise.
    ///
    /// The "empty params -> non-generic insert, else generic insert" choice is
    /// the single rule every body-registration site shares; expressing it once
    /// here keeps the resolved-body (`register_resolved_def_in_envs`) and
    /// flow-mirror (`mirror_def_in_type_environment`) construction paths from
    /// drifting apart. `variances` is threaded through unchanged so callers that
    /// already computed declared variances keep them and callers that do not
    /// pass `None`, exactly as before.
    pub(crate) fn insert_def_choosing_params(
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
        variances: Option<Arc<[tsz_solver::type_handles::Variance]>>,
    ) -> Self {
        if params.is_empty() {
            Self::InsertDef { def_id, body }
        } else {
            Self::InsertDefWithParams {
                def_id,
                body,
                params,
                variances,
            }
        }
    }

    /// Apply this deferred registration to a flow-analyzer `TypeEnvironment`.
    pub(crate) fn apply(&self, env: &mut TypeEnvironment) {
        match self {
            Self::SetDefinitionStore(store) => env.set_definition_store(Arc::clone(store)),
            Self::InsertDef { def_id, body } => env.insert_def(*def_id, *body),
            Self::InsertDefIfMissing { def_id, body } => {
                if env.get_def(*def_id).is_none() {
                    env.insert_def(*def_id, *body);
                }
            }
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
                provisional,
            } => {
                env.insert_class_instance_type(*def_id, *instance_type);
                if *provisional {
                    env.mark_def_provisional(*def_id);
                } else {
                    env.clear_def_provisional(*def_id);
                }
            }
            Self::InsertClassInstanceIfMissing {
                def_id,
                instance_type,
            } => {
                if env.get_class_instance_type(*def_id).is_none() {
                    env.insert_class_instance_type(*def_id, *instance_type);
                }
            }
            Self::RegisterClassExtends {
                def_id,
                parent_def_id,
            } => env.register_class_extends(*def_id, *parent_def_id),
            Self::RegisterClassExtendsIfMissing {
                def_id,
                parent_def_id,
            } => {
                if env.get_class_extends_def(*def_id).is_none() {
                    env.register_class_extends(*def_id, *parent_def_id);
                }
            }
            Self::RegisterInterfaceExtends {
                def_id,
                parent_def_id,
            } => env.register_interface_extends(*def_id, *parent_def_id),
            Self::RegisterDefSymbolMapping { def_id, sym_id } => {
                env.register_def_symbol_mapping(*def_id, *sym_id);
            }
            Self::RegisterAugmentedDef {
                def_id,
                augmented,
                is_class,
            } => apply_augmented_def(env, *def_id, *augmented, *is_class),
            Self::InsertDefKind { def_id, kind } => env.insert_def_kind(*def_id, *kind),
            Self::InsertUnresolvedResolution { name, def_id } => {
                env.insert_unresolved_resolution(name.clone(), *def_id);
            }
            Self::InsertTypeofValueType { symbol, value_type } => {
                env.insert_typeof_value_type(*symbol, *value_type);
            }
            Self::RegisterBoxedType { kind, type_id } => env.set_boxed_type(*kind, *type_id),
            Self::RegisterArrayBaseType { type_id, params } => {
                env.set_array_base_type(*type_id, params.clone());
            }
            Self::RegisterBoxedDef {
                kind,
                type_id,
                def_id,
            } => {
                env.insert_def(*def_id, *type_id);
                env.register_boxed_def_id(*kind, *def_id);
            }
            Self::RegisterWellKnownSymbolName { name, symbol_ref } => {
                env.register_well_known_symbol_name(name.clone(), *symbol_ref);
            }
            Self::RegisterEnumNamespaceType { def_id, ns_type } => {
                env.register_enum_namespace_type(*def_id, *ns_type);
            }
            Self::RegisterNumericEnum { def_id } => env.register_numeric_enum(*def_id),
            Self::RegisterEnumParent {
                member_def_id,
                parent_def_id,
            } => env.register_enum_parent(*member_def_id, *parent_def_id),
            Self::InsertSymbolType { symbol, ty, params } => {
                if params.is_empty() {
                    env.insert(*symbol, *ty);
                } else {
                    env.insert_with_params(*symbol, *ty, params.clone());
                }
            }
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
