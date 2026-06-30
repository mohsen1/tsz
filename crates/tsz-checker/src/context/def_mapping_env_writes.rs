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
