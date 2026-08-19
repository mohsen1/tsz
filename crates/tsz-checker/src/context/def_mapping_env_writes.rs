//! Race-safe dual-environment registration wrappers for `CheckerContext`.
//!
//! These thin `*_in_envs` helpers replace the raw dual `try_borrow_mut` blocks
//! that silently dropped a registration on a borrow conflict; each write is now
//! deferred and replayed instead of lost (#14348). They live in a sibling module
//! to keep `def_mapping.rs` under the 2000-line ceiling; behavior is identical to
//! defining them inline on `impl CheckerContext`.

use tsz_solver::TypeId;
use tsz_solver::def::DefId;

use crate::context::CheckerContext;
use crate::context::deferred_flow_env_write::DeferredFlowEnvWrite;
use crate::query_boundaries::common::TypeEnvironment;

use std::cell::RefCell;

pub(super) fn apply_or_defer_env_write(
    env: &RefCell<TypeEnvironment>,
    queue: &RefCell<Vec<DeferredFlowEnvWrite>>,
    op: DeferredFlowEnvWrite,
) {
    match env.try_borrow_mut() {
        Ok(mut env) => {
            drain_env_write_queue_into(queue, &mut env);
            op.apply(&mut env);
        }
        Err(_) => queue.borrow_mut().push(op),
    }
}

pub(super) fn drain_env_write_queue_into(
    queue: &RefCell<Vec<DeferredFlowEnvWrite>>,
    env: &mut TypeEnvironment,
) {
    let pending = std::mem::take(&mut *queue.borrow_mut());
    for op in pending {
        op.apply(env);
    }
}

pub(super) fn flush_env_write_queue(
    env: &RefCell<TypeEnvironment>,
    queue: &RefCell<Vec<DeferredFlowEnvWrite>>,
) {
    if let Ok(mut env) = env.try_borrow_mut() {
        drain_env_write_queue_into(queue, &mut env);
    }
}

impl CheckerContext<'_> {
    /// Register a merged interface+value symbol's `typeof` value-space type in
    /// **both** type environments through the race-safe deferral discipline.
    ///
    /// Replaces the raw dual `try_borrow_mut` blocks that silently dropped the
    /// registration on a borrow conflict; the write is now deferred and replayed
    /// instead of lost (#14348).
    pub(crate) fn register_typeof_value_type_in_envs(
        &self,
        symbol: tsz_solver::SymbolRef,
        value_type: TypeId,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertTypeofValueType { symbol, value_type });
    }

    /// Cache an unresolved type-name resolution in **both** type environments
    /// through the race-safe deferral discipline.
    pub(crate) fn register_unresolved_resolution_in_envs(&self, name: String, def_id: DefId) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertUnresolvedResolution { name, def_id });
    }

    /// Register the boxed interface type for a primitive in **both** type
    /// environments through the race-safe deferral discipline.
    pub(crate) fn register_boxed_type_in_envs(
        &self,
        kind: tsz_solver::IntrinsicKind,
        type_id: TypeId,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterBoxedType { kind, type_id });
    }

    /// Register the canonical `Array<T>` base type in **both** type environments
    /// through the race-safe deferral discipline.
    pub(crate) fn register_array_base_type_in_envs(
        &self,
        type_id: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterArrayBaseType { type_id, params });
    }

    /// Register a boxed interface's `Lazy(DefId)` body and boxed-kind marker in
    /// **both** type environments through the race-safe deferral discipline.
    pub(crate) fn register_boxed_def_in_envs(
        &self,
        kind: tsz_solver::IntrinsicKind,
        type_id: TypeId,
        def_id: DefId,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterBoxedDef {
            kind,
            type_id,
            def_id,
        });
    }

    /// Register a `[Symbol.*]` computed property name's backing `SymbolRef` in
    /// **both** type environments through the race-safe deferral discipline.
    pub(crate) fn register_well_known_symbol_name_in_envs(
        &self,
        name: String,
        symbol_ref: tsz_solver::SymbolRef,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterWellKnownSymbolName {
            name,
            symbol_ref,
        });
    }

    /// The canonical `[Symbol.xxx]` name registered for a well-known symbol
    /// `SymbolRef`, or `None` for an ordinary user `unique symbol`.
    ///
    /// The inverse of [`Self::register_well_known_symbol_name_in_envs`]; reads
    /// the evaluator environment, which the eager `seed_well_known_symbol_names`
    /// pre-pass populates from the lib `SymbolConstructor` members before any
    /// indexed-access / `keyof` type is evaluated. Returns an owned `String`
    /// rather than a borrow so callers need not hold the `type_env` borrow.
    pub(crate) fn well_known_symbol_name_for_ref(
        &self,
        symbol_ref: tsz_solver::SymbolRef,
    ) -> Option<String> {
        self.type_env.try_borrow().ok().and_then(|env| {
            env.lookup_well_known_symbol_name(symbol_ref)
                .map(str::to_string)
        })
    }

    /// The object-shape key a literal index `type_id` looks up, resolver-aware.
    ///
    /// A well-known `UniqueSymbol` index (`typeof Symbol.iterator`) resolves to
    /// its canonical `[Symbol.xxx]` shape key — the text under which shapes store
    /// well-known-symbol members — rather than the synthetic `__unique_N`
    /// placeholder the plain, resolver-less `literal_property_name` emits.
    /// Without this the key never matches the member and a valid indexed access
    /// wrongly reports TS2339/TS2538. Non-symbol indices fall through to the
    /// ordinary literal-key spelling.
    pub(crate) fn resolver_aware_index_key_name(&self, type_id: TypeId) -> Option<String> {
        if let Some(sym) =
            crate::query_boundaries::type_construction::unique_symbol_ref(self.types, type_id)
            && let Some(name) = self.well_known_symbol_name_for_ref(sym)
        {
            return Some(name);
        }
        crate::query_boundaries::type_computation::access::literal_property_name(
            self.types, type_id,
        )
        .map(|atom| self.types.resolve_atom(atom))
    }

    /// Register an enum's namespace object type in **both** type environments
    /// through the race-safe deferral discipline.
    pub(crate) fn register_enum_namespace_type_in_envs(&self, def_id: DefId, ns_type: TypeId) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterEnumNamespaceType { def_id, ns_type });
    }

    /// Register an enum as numeric in **both** type environments through the
    /// race-safe deferral discipline.
    pub(crate) fn register_numeric_enum_in_envs(&self, def_id: DefId) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterNumericEnum { def_id });
    }

    /// Register an enum member's parent enum in **both** type environments
    /// through the race-safe deferral discipline.
    pub(crate) fn register_enum_parent_in_envs(&self, member_def_id: DefId, parent_def_id: DefId) {
        // Publish the member -> parent edge directly to the shared
        // `DefinitionStore` as well. The env write-through only reaches the
        // store for envs that already have it wired at write time, but
        // solver-side generic-call inference reads the edge through the
        // `QueryCache`'s attached store (its resolver has no env access), so
        // the edge must be present there regardless of env wiring order.
        self.definition_store
            .register_enum_parent(member_def_id, parent_def_id);
        self.register_in_envs(DeferredFlowEnvWrite::RegisterEnumParent {
            member_def_id,
            parent_def_id,
        });
    }

    /// Register a symbol's resolved type (`SymbolRef -> TypeId`) in **both** type
    /// environments through the race-safe deferral discipline.
    pub(crate) fn register_symbol_type_in_envs(
        &self,
        symbol: tsz_solver::SymbolRef,
        ty: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertSymbolType { symbol, ty, params });
    }

    /// Merge a child environment's `DefId -> TypeId` body into both parent
    /// environments when the parent lacks the body, deferring on borrow races.
    pub(crate) fn merge_def_if_missing_in_envs(&self, def_id: DefId, body: TypeId) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertDefIfMissing { def_id, body });
    }

    /// Merge a child environment's class instance metadata into both parent
    /// environments when the parent lacks it, deferring on borrow races.
    pub(crate) fn merge_class_instance_if_missing_in_envs(
        &self,
        def_id: DefId,
        instance_type: TypeId,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertClassInstanceIfMissing {
            def_id,
            instance_type,
        });
    }

    /// Merge a child environment's class `extends` metadata into both parent
    /// environments when the parent lacks it, deferring on borrow races.
    pub(crate) fn merge_class_extends_if_missing_in_envs(
        &self,
        def_id: DefId,
        parent_def_id: DefId,
    ) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterClassExtendsIfMissing {
            def_id,
            parent_def_id,
        });
    }
}
