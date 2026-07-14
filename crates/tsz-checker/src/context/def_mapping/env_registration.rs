//! Definition-body / kind / symbol-mapping registration into both
//! `TypeEnvironment` instances for `CheckerContext`, split out of
//! `def_mapping.rs`.
//!
//! Owns the `register_def_*_in_envs` family plus the published-body history
//! and application-eval invalidation helpers backing it.

use crate::context::CheckerContext;
use crate::context::deferred_flow_env_write::DeferredFlowEnvWrite;
use crate::query_boundaries::common::TypeResolver;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl CheckerContext<'_> {
    /// Record that `def_id` has been published with body `body`, returning
    /// whether that body was **already** in the def's published-body history.
    ///
    /// Used to detect the benign re-resolution oscillation where a generic
    /// alias re-lowers to one of a small set of structurally-equivalent interned
    /// bodies on every application. The history is tiny (bounded by the number
    /// of distinct equivalent re-lowerings, typically 1-2), so the linear scan
    /// over the `SmallVec` is effectively constant.
    fn record_published_body(&self, def_id: DefId, body: TypeId) -> bool {
        let mut map = self.def_published_bodies.borrow_mut();
        let history = map.entry(def_id).or_default();
        if history.contains(&body) {
            true
        } else {
            history.push(body);
            false
        }
    }

    /// Register a non-generic definition body in **both** type environments.
    #[track_caller]
    pub fn register_def_in_envs(&self, def_id: DefId, body: TypeId) {
        let prev_body = self.definition_store.get_body(def_id);
        let body_changed = prev_body != Some(body);
        // Skip the expensive env-eval cache sweep when a def re-publishes a body
        // it already held (a benign re-resolution oscillation). See
        // `def_published_bodies` and `register_def_with_params_in_envs`.
        let body_seen_before = body_changed && self.record_published_body(def_id, body);
        self.publish_definition_body(def_id, body);
        if body_changed {
            // First publication (`None -> Some`) needs no env-eval/narrowing
            // sweep, for the same reason `invalidate_application_evals_on_body_rewrite`
            // skips it: no cached entry can reference `def_id` before it had a
            // resolvable body (the solver refuses to persist results computed
            // against an unresolved def, `mark_unresolved_def_seen`). Sweeping
            // on every first registration is `O(env_eval_cache)` per def, which
            // is `O(N^2)` across a file of `N` aliases whose closed-but-deferred
            // bodies (e.g. `Uppercase<"..">`) each seed one new cache entry. Gate
            // the sweep on a genuine rewrite (`Some(old) -> Some(new)`).
            if !body_seen_before && prev_body.is_some() {
                self.clear_type_evaluation_caches_for_def(def_id);
            }
            self.invalidate_application_evals_on_body_rewrite(def_id, prev_body);
        }
        self.register_in_envs(DeferredFlowEnvWrite::InsertDef { def_id, body });
    }

    /// Drop stale per-file application-eval entries when an already-published
    /// definition body is *rewritten* to different content.
    ///
    /// First publication (`None -> Some`) needs no sweep: the solver's
    /// evaluator refuses to persist application/closed-eval results computed
    /// while the def had no resolvable body (`mark_unresolved_def_seen`), so
    /// no entry derived from the body-less window can exist. Rewrites
    /// (`Some(old) -> Some(new)`, e.g. a partial pre-merge interface body
    /// upgraded to its heritage-merged form) do leave stale entries — those
    /// were legitimately cached against `old`. The sweep is def-keyed and
    /// rewrite-gated so the common first-registration path never pays the
    /// cache scan.
    fn invalidate_application_evals_on_body_rewrite(
        &self,
        def_id: DefId,
        prev_body: Option<TypeId>,
    ) {
        if prev_body.is_some() {
            self.types.invalidate_application_eval_cache_for_def(def_id);
        }
    }

    /// Register a generic definition body (with type parameters) in **both**
    /// type environments.
    ///
    /// Body and params are published to the shared `DefinitionStore` in one
    /// atomic write so concurrent readers never observe a generic alias whose
    /// body is visible but whose parameter list is still missing (which would
    /// mis-instantiate every application of the alias).
    #[track_caller]
    pub fn register_def_with_params_in_envs(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        let prev_body = self.definition_store.get_body(def_id);
        let body_changed = prev_body != Some(body);
        let params_changed = self
            .definition_store
            .get_type_params(def_id)
            .is_none_or(|existing| existing != params);
        // A generic alias re-resolves on every application; the re-lowering can
        // emit one of a few structurally-equivalent interned bodies, so
        // `body_changed` oscillates. Re-publishing a body the def already held
        // introduces no new staleness beyond the first occurrence, so the
        // expensive `O(env_eval_cache)` `clear_type_evaluation_caches_for_def`
        // sweep can be skipped for that benign rewrite (see
        // `def_published_bodies`). A genuinely new body, or any params change,
        // still sweeps. The cheap, def-keyed application-eval invalidation still
        // runs on every real body change.
        let body_seen_before = body_changed && self.record_published_body(def_id, body);
        self.publish_definition_body_with_params(def_id, body, params.clone());
        // First publication (`prev_body == None`) needs no sweep: no cached
        // entry can reference `def_id` before it had a resolvable body (see
        // `register_def_in_envs` and `invalidate_application_evals_on_body_rewrite`).
        // A params-only change without a prior body cannot occur — params are
        // published atomically with the body — so gating on `prev_body.is_some()`
        // preserves every genuine-rewrite sweep while removing the per-first-
        // registration `O(env_eval_cache)` scan that is `O(N^2)` across a file.
        if prev_body.is_some() && ((body_changed && !body_seen_before) || params_changed) {
            self.clear_type_evaluation_caches_for_def(def_id);
        }
        if body_changed {
            self.invalidate_application_evals_on_body_rewrite(def_id, prev_body);
        }
        let declared_variances = TypeResolver::get_type_param_variance(self, def_id);
        self.register_in_envs(DeferredFlowEnvWrite::InsertDefWithParams {
            def_id,
            body,
            params,
            variances: declared_variances,
        });
    }

    /// Register a definition body in **both** type environments, choosing
    /// `insert_def` or `insert_def_with_params` based on whether `params` is
    /// empty.
    #[track_caller]
    pub fn register_def_auto_params_in_envs(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if params.is_empty() {
            self.register_def_in_envs(def_id, body);
        } else {
            self.register_def_with_params_in_envs(def_id, body, params);
        }
    }

    /// Register a class instance type in **both** type environments.
    pub fn register_class_instance_in_envs(&self, def_id: DefId, instance_type: TypeId) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertClassInstance {
            def_id,
            instance_type,
        });
    }

    /// Register a class `extends` relationship in **both** type environments.
    ///
    /// This is required so the `FlowAnalyzer`'s `NarrowingContext` (which uses
    /// `type_environment`) can resolve nominal instanceof relationships just as
    /// the evaluator (`type_env`) can.  Without this, `is_class_ancestor` always
    /// returns `false` for user-defined class hierarchies during narrowing, causing
    /// `D1 & C1` intersections instead of the correct `D1` narrowed type.
    pub fn register_class_extends_in_envs(&self, def_id: DefId, parent_def_id: DefId) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterClassExtends {
            def_id,
            parent_def_id,
        });
    }

    /// Register a `DefId` ↔ `SymbolId` bridge in **both** type environments.
    ///
    /// This keeps evaluator and flow-analyzer resolution paths aligned for
    /// `TypeQuery`, inheritance, and solver-side DefId identity lookups.
    pub fn register_def_symbol_mapping_in_envs(&self, def_id: DefId, sym_id: SymbolId) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterDefSymbolMapping { def_id, sym_id });
    }

    /// Register a `DefId` ↔ `SymbolId` bridge in the flow-analyzer environment.
    ///
    /// `register_resolved_type` historically populated this bridge only in
    /// `type_environment`. Keep that path scoped so resolving a symbol's body
    /// does not also change evaluator-side TypeQuery/Lazy resolution order. On a
    /// borrow conflict the write is deferred and replayed rather than dropped.
    pub fn register_def_symbol_mapping_in_type_environment(&self, def_id: DefId, sym_id: SymbolId) {
        self.mirror_to_flow_env(DeferredFlowEnvWrite::RegisterDefSymbolMapping { def_id, sym_id });
    }

    /// Register an augmented definition body in **both** type environments.
    ///
    /// If the definition is a class (or already has a class-instance entry),
    /// updates the class-instance type. Otherwise, preserves existing type
    /// parameters (if any) when re-inserting the definition body.
    pub fn register_augmented_def_in_envs(&self, def_id: DefId, augmented: TypeId, is_class: bool) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterAugmentedDef {
            def_id,
            augmented,
            is_class,
        });
    }

    /// Register a `DefKind` for a `DefId` in **both** type environments.
    ///
    /// This ensures the evaluator (`type_env`) and flow-analyzer (`type_environment`)
    /// both see the `DefKind`, which is needed for `Lazy(DefId)` resolution and
    /// semantic queries (e.g., distinguishing class vs interface callables).
    ///
    /// Prior to this helper, pre-population and fallback paths only propagated
    /// `DefKind` to `type_env`, leaving `type_environment` without the mapping
    /// until the full checker walk populated it incidentally.
    pub(crate) fn register_def_kind_in_envs(&self, def_id: DefId, kind: tsz_solver::def::DefKind) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertDefKind { def_id, kind });
    }
}
