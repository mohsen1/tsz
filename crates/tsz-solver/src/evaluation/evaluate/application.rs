//! Generic type application evaluation (`Base<Args>`) for [`TypeEvaluator`].
//!
//! The orchestrator [`TypeEvaluator::evaluate_application`] resolves an
//! application's callee to a [`DefId`], guards per-`DefId` recursion depth and
//! divergent growth, builds an [`ApplicationEvalContext`], then dispatches to
//! the known-params or extracted-params body path. Body-aware shortcuts
//! (homomorphic mapped passthrough, mapped-union distribution,
//! recursive-call-return placeholders, `typeof f<Args>` specialization) and the
//! application-eval cache live here, along with the display-alias bookkeeping
//! that repaints evaluated structural forms back to their alias names.
//!
//! Split out of `evaluate.rs` so the core evaluator state and the `evaluate`
//! dispatch loop stay separable from the application-instantiation machinery.

use super::*;

/// Whether a generic type-alias application whose body is a *genuinely
/// registered* `unknown` (e.g. `type C<T> = unknown`, or a utility alias that
/// reduces to `unknown`) is allowed to reduce to the canonical `unknown` instead
/// of staying an opaque `Application`.
///
/// Default-on; `TSZ_DISABLE_GENUINE_UNKNOWN_ALIAS_REDUCTION=1` is the kill switch
/// that restores the prior behavior, where every `unknown`-bodied application was
/// kept opaque. That blanket bail conflates a registration-window placeholder
/// (a cross-file alias whose declaring file has not published its body yet, where
/// staying opaque is correct) with a genuine, finalized `unknown` body, so the
/// genuine case minted an identity-distinct `Application` that the relation layer
/// could not see was `unknown` — a false `unknown` ≠ `unknown` (TS2719) or
/// `unknown` ≰ `C<...>` (TS2322) in member position (issue #13212).
fn genuine_unknown_alias_reduction_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !std::env::var("TSZ_DISABLE_GENUINE_UNKNOWN_ALIAS_REDUCTION").is_ok_and(|v| v == "1")
    })
}

/// Debug kill-switch for the cross-evaluator in-flight application sentinel
/// (issue #13508 root cause B). Set
/// `TSZ_DISABLE_CROSS_EVAL_APPLICATION_SENTINEL=1` to restore the prior
/// behavior, where a fresh evaluator re-expanded an in-flight `Application`
/// node from scratch. Used only to bisect regressions; defaults to enabled.
fn cross_eval_application_sentinel_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !std::env::var("TSZ_DISABLE_CROSS_EVAL_APPLICATION_SENTINEL").is_ok_and(|v| v == "1")
    })
}

struct ApplicationFinalizeContext<'a> {
    original_args: &'a [TypeId],
    expanded_args: &'a [TypeId],
    body: TypeId,
    type_params: &'a [TypeParamInfo],
    prefer_application_display_alias: bool,
    record_structural_back_reference: bool,
    no_unchecked_indexed_access: bool,
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a generic type application: Base<Args>
    ///
    /// Algorithm:
    /// 1. Look up the base type - if it's a Ref, resolve it
    /// 2. Get the type parameters for the base symbol
    /// 3. If we have type params, instantiate the resolved type with args
    /// 4. Recursively evaluate the result
    pub(super) fn evaluate_application(
        &mut self,
        app_id: TypeApplicationId,
        original_type_id: TypeId,
    ) -> TypeId {
        let app = self.interner.type_application(app_id);

        // Phase 1 — callee normalization. `Lazy(DefId)` is the dominant
        // shape from the binder, but `TypeQuery`, `UnresolvedTypeName`, and
        // symbol-backed objects can also reach this entrypoint after
        // cross-file lowering or value-position queries. Bases without a
        // defining `DefId` stay opaque so later passes with a richer
        // resolver can expand them.
        let Some(def_id) = self.resolve_application_def_id(app.base) else {
            crate::evaluation::eval_materialization_probe::record_application_entry(false, false);
            return self.evaluate_application_no_def_id(app_id, original_type_id);
        };
        crate::evaluation::eval_materialization_probe::record_application_entry(
            true,
            self.query_db.is_some(),
        );

        tracing::trace!(
            base = app.base.0,
            ?def_id,
            num_args = app.args.len(),
            "evaluate_application"
        );

        // Phase 2 — per-DefId recursion guard. Up to MAX_DEF_DEPTH bounded
        // recursive expansions are allowed before bailing to `TypeId::ERROR`,
        // matching tsc's TS2589 behavior.
        if !self.increment_def_depth(def_id) {
            self.mark_depth_exceeded_for_request();
            return TypeId::ERROR;
        }

        // Divergence guard. MAX_DEF_DEPTH bounds the *number* of re-expansions
        // but not the *size* of each, so a growing recursive alias can build
        // enormous types within that budget. Gating on depth >= 2 keeps flat,
        // non-recursive instantiation from feeding the detector.
        if self.def_depth.get(&def_id).is_some_and(|&d| d >= 2)
            && self.detect_recursive_growth(def_id, &app.args)
        {
            self.decrement_def_depth(def_id);
            self.mark_depth_exceeded_for_request();
            return TypeId::ERROR;
        }

        // Phase 3 — build the evaluation context.
        let ctx = self.application_evaluation_context(def_id, app.base);

        // See `ApplicationEvalOutcome` for why ShortCircuit branches do not
        // restore `saved_apparent` — outer caller observes `None`.
        let saved_apparent = self.apparent_conditional_branch.take();

        // Phase 4 — raw-args cache shortcut. Only evaluators with an explicit
        // `query_db` consume this cache: limited/noop resolvers can otherwise
        // observe a result computed under stronger resolution assumptions and
        // skip the fallback behavior that preserves recursive/inference parity.
        if let Some(db) = self.query_db {
            let no_unchecked = self.no_unchecked_indexed_access;
            let cached = db.lookup_application_eval_cache(def_id, &app.args, no_unchecked);
            crate::evaluation::eval_materialization_probe::record_application_cache_lookup(
                crate::evaluation::eval_materialization_probe::ApplicationLookupSite::RawArgs,
                cached.is_some(),
            );
            if let Some(cached) = cached {
                tracing::trace!(
                    def_id = ?def_id,
                    ?cached,
                    "evaluate_application raw-args cache hit"
                );
                self.decrement_def_depth(def_id);
                return cached;
            }
        }

        // Phase 4.5 — cross-evaluator in-flight sentinel (issue #13508 root
        // cause B; rationale on `crate::limits::MAX_CROSS_EVAL_APPLICATION_EXPANSION`).
        // A re-entry past the in-flight allowance defers: the application
        // stays opaque and the in-flight owner produces the real result. The
        // deferral is a registration-window-class artifact — a later pass (or
        // the owner's completed expansion, via the application-eval cache)
        // resolves it — so it taints the run through
        // `mark_unresolved_def_seen`, keeping every enclosing partial result
        // out of the persistent caches. The TS2589 depth-detection pass is
        // exempt: it must re-walk the expansion the sentinel would skip.
        let sentinel_active =
            !self.flag_depth_on_app_cycle && cross_eval_application_sentinel_enabled();
        if sentinel_active
            && !self.with_evaluation_session_scope(|session| {
                session.enter_application_expansion(original_type_id)
            })
        {
            tracing::trace!(
                ?def_id,
                node = original_type_id.0,
                "evaluate_application: deferring cross-evaluator re-entry \
                 of an in-flight application"
            );
            self.mark_unresolved_def_seen();
            self.decrement_def_depth(def_id);
            return original_type_id;
        }

        // Phase 5 — evaluate the body under fresh application-body epoch
        // snapshots. Any `application_eval_cache` write made while finalizing
        // THIS application (or a nested one, which saves/restores these fields
        // in turn) compares the live epochs against the snapshots to learn
        // whether its own body subtree bailed or observed an unresolved def,
        // independent of earlier unrelated siblings (#10834).
        let saved_app_body_epoch = self.app_body_limit_epoch;
        let saved_app_body_unresolved_def_epoch = self.app_body_unresolved_def_epoch;
        self.app_body_limit_epoch = self.limit_epoch;
        self.app_body_unresolved_def_epoch = self.unresolved_def_epoch;
        let outcome = self.evaluate_application_body(def_id, original_type_id, &app.args, &ctx);
        self.app_body_limit_epoch = saved_app_body_epoch;
        self.app_body_unresolved_def_epoch = saved_app_body_unresolved_def_epoch;
        if sentinel_active {
            self.with_evaluation_session_scope(|session| {
                session.leave_application_expansion(original_type_id);
            });
        }

        // Phase 6 — outcome-dependent cleanup. ShortCircuit matches the
        // historical decrement-and-return shape; Computed restores the
        // outer apparent branch and runs display-alias bookkeeping.
        match outcome {
            ApplicationEvalOutcome::ShortCircuit(value) => {
                self.decrement_def_depth(def_id);
                value
            }
            ApplicationEvalOutcome::Computed(result) => {
                // Read the apparent conditional branch set during THIS
                // application, then restore whatever was saved for the
                // outer caller.
                let my_apparent_branch = self.apparent_conditional_branch.take();
                self.apparent_conditional_branch = saved_apparent;
                self.decrement_def_depth(def_id);

                // A class application whose declared constructor body
                // carries a nominal symbol, but whose evaluated result is a
                // structural object that dropped it, is a degraded,
                // partially built instance: the class's instance type
                // carried only its annotated fields (no methods, no nominal
                // identity) when this application forced its resolution — a
                // circular import can make a sibling's build reach
                // `Class<Args>` before the class publishes its final type.
                // Surfacing that partial object (e.g. as a union member
                // beside the complete representation) yields spurious
                // `TS2339`s for every missing method (issue #16055).
                // Gated on `ctx.class_declared_nominal_symbol` (the
                // constructor's OWN body already carrying a symbol before
                // this evaluation) so a class shape that is legitimately
                // symbol-less throughout — e.g. a synthetic construct
                // signature with no nominal identity to begin with — never
                // trips this guard (see
                // `evaluate_application_class_uses_construct_signature_return_type`,
                // the regression #16911 caused by skipping this check).
                // Discard the degraded result: purge whatever the body
                // evaluation cached under `(def, args)` so a later
                // evaluation recomputes against the finished body, taint the
                // run so nothing persists this partial, and keep the
                // application opaque so a property access re-resolves it.
                if result != original_type_id
                    && ctx.class_declared_nominal_symbol
                    && matches!(
                        self.interner.lookup(result),
                        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
                    )
                    && self.application_result_dropped_nominal_symbol(result)
                {
                    if let Some(db) = self.query_db {
                        db.invalidate_application_eval_cache_for_def(def_id);
                    }
                    self.mark_unresolved_def_seen();
                    return original_type_id;
                }

                // Phase 7 — display-alias bookkeeping. Skip entirely when
                // the result is the original `Application` itself (the
                // historical `if result != original_type_id` gate).
                if result != original_type_id {
                    // Semantic (non-display) provenance: a nominal
                    // class/interface instantiation that evaluation lowered to
                    // a structural object keeps a reverse link to its
                    // application so the relation layer can recover the
                    // generic identity for the accept-only variance fast
                    // path. Recorded without display heuristics; never read
                    // by the printer.
                    if !ctx.is_type_alias_def
                        && matches!(
                            self.interner.lookup(result),
                            Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
                        )
                    {
                        self.interner
                            .record_application_eval_origin(result, original_type_id);
                    }
                    self.record_application_evaluation_display_aliases(
                        result,
                        original_type_id,
                        &app.args,
                        ctx.is_type_alias_def,
                        ctx.prefer_application_display_alias,
                        my_apparent_branch,
                    );
                }
                result
            }
        }
    }

    /// True when `result` is a structural object (`Object`/`ObjectWithIndex`)
    /// carrying no nominal `symbol`. For a class application this is the
    /// signature of an instance body instantiated from a partial,
    /// mid-construction source: the complete instance keeps its class
    /// `SymbolId`, whereas the annotated-fields-only snapshot produced before
    /// the class finishes building loses it (issue #16055).
    fn application_result_dropped_nominal_symbol(&self, result: TypeId) -> bool {
        match self.interner.lookup(result) {
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                self.interner.object_shape(shape_id).symbol.is_none()
            }
            _ => false,
        }
    }

    /// Phase-1 helper: resolve an `Application` base to a [`DefId`].
    ///
    /// Returns `None` when the application's base does not normalize to a
    /// defining `DefId` (e.g. an interned base that no longer resolves, or
    /// a base whose `TypeData` shape simply has no associated `DefId`).
    /// Both cases must keep the application opaque, so the caller treats
    /// `None` the same way.
    pub(super) fn resolve_application_def_id(&self, base: TypeId) -> Option<DefId> {
        let base_key = self.interner.lookup(base)?;
        match base_key {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver.symbol_to_def_id(sym_ref),
            TypeData::UnresolvedTypeName(atom) => {
                // `Application(UnresolvedTypeName(name), args)` residue from
                // cross-file lowering can resolve through the merged binder
                // graph at evaluation time — e.g. `util.OmitKeys` whose
                // lowering pass missed the imported namespace's def_id.
                let name = self.interner.resolve_atom(atom);
                self.resolver.resolve_unresolved_type_name(&name)
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => self
                .interner
                .object_shape(shape_id)
                .symbol
                .and_then(|sym_id| {
                    self.resolver
                        .symbol_to_def_id(crate::types::SymbolRef(sym_id.0))
                })
                // A structural interface body can carry no shape symbol when a
                // checker's interface resolution returned (and published) the
                // body form directly instead of `Lazy(DefId)` — e.g. the lib
                // `Promise` body `{ then, catch, finally, ... }` registered
                // through `register_type_to_def`. Recover the declaration
                // identity through the shared store's reverse mapping so the
                // application still instantiates its type parameters;
                // otherwise the un-substituted body leaks raw type parameters
                // to whichever sibling checker observes this form first
                // (schedule-dependent false TS2339/TS7006/TS2314 storms).
                .or_else(|| self.resolver.def_for_type(base)),
            _ => None,
        }
    }

    /// Phase-3 helper: assemble the [`ApplicationEvalContext`] for a
    /// resolved `DefId`.
    ///
    /// Reads type parameters and the resolved body from the resolver,
    /// records whether the body is a conditional alias (which drives both
    /// the marker on the base type and the display-alias policy), and
    /// emits the matching trace event the historical inline code emitted.
    fn application_evaluation_context(
        &mut self,
        def_id: DefId,
        app_base: TypeId,
    ) -> ApplicationEvalContext {
        let type_params = self.resolver.get_lazy_type_params(def_id);
        let base_is_type_query =
            matches!(self.interner.lookup(app_base), Some(TypeData::TypeQuery(_)));
        // For `typeof ClassName<T>` (TypeQuery base), use `resolve_type_query` to get
        // the constructor type rather than the instance type that `resolve_lazy` returns
        // for classes. Type-position references (`ClassName<T>`) continue to use
        // `resolve_lazy` which correctly provides the instance type.
        let resolved = if base_is_type_query {
            if let Some(TypeData::TypeQuery(sym_ref)) = self.interner.lookup(app_base) {
                self.resolver
                    .resolve_type_query(sym_ref, self.interner)
                    .or_else(|| self.resolver.resolve_lazy(def_id, self.interner))
            } else {
                self.resolver.resolve_lazy(def_id, self.interner)
            }
        } else {
            self.resolver.resolve_lazy(def_id, self.interner)
        };
        let def_kind = self.resolver.get_def_kind(def_id);
        let is_type_alias_def = matches!(def_kind, Some(DefKind::TypeAlias));
        let resolved_has_conditional_body = resolved.is_some_and(|body| {
            matches!(self.interner.lookup(body), Some(TypeData::Conditional(_)))
        });
        if is_type_alias_def && resolved_has_conditional_body {
            self.interner.mark_conditional_alias_base(app_base);
        }
        let prefer_application_display_alias = is_type_alias_def && !resolved_has_conditional_body;

        let class_declared_nominal_symbol = matches!(def_kind, Some(DefKind::Class))
            && resolved.is_some_and(|body| {
                matches!(
                    self.interner.lookup(body),
                    Some(TypeData::Callable(cs_id))
                        if self.interner.callable_shape(cs_id).symbol.is_some()
                )
            });

        tracing::trace!(
            ?def_id,
            def_name = ?self
                .resolver
                .get_def_name(def_id)
                .map(|atom| self.interner.resolve_atom(atom)),
            canonical_def = ?self.resolver.canonical_def_id(def_id),
            resolver_gen = self.resolver.resolver_generation(),
            has_type_params = type_params.is_some(),
            type_params_count = type_params.as_ref().map(std::vec::Vec::len),
            has_resolved = resolved.is_some(),
            resolved_key = ?resolved.and_then(|r| self.interner.lookup(r)),
            "evaluate_application resolve"
        );

        ApplicationEvalContext {
            type_params,
            resolved,
            is_type_alias_def,
            prefer_application_display_alias,
            base_is_type_query,
            class_declared_nominal_symbol,
        }
    }

    /// Phase-5 dispatch between the canonical known-params path and the
    /// lite-resolver fallback that extracts parameters from the resolved
    /// type's shape.
    fn evaluate_application_body(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        args: &[TypeId],
        ctx: &ApplicationEvalContext,
    ) -> ApplicationEvalOutcome {
        // Recursive-call-return placeholder: an `Application(Lazy(value_def),
        // type_args)` whose `value_def` is a value-space symbol (a function or
        // `const`/`let`/`var` initialized to a function) with a callable type
        // denotes the RETURN type of a self-referential generic call
        // `f<type_args>(...)` whose return type was still being inferred when
        // the placeholder was built (see the checker's circular-call path).
        // It must evaluate to the matching call signature's return type
        // instantiated with `type_args`, not to the instantiated function
        // value, so property access, assignability, and display observe the
        // object the call returns. Instantiation expressions (`f<T>` /
        // `typeof f<T>`) never reach here as a `Lazy`-based application — they
        // are instantiated eagerly and `typeof f<T>` carries a `TypeQuery`
        // base — so this shape is unambiguous.
        if !ctx.base_is_type_query
            && let Some(resolved) = ctx.resolved
            && let Some(call_return) = self.value_call_return_application(def_id, resolved, args)
        {
            crate::evaluation::eval_materialization_probe::record_application_body_path(
                crate::evaluation::eval_materialization_probe::ApplicationBodyPath::ValueCallReturn,
            );
            return ApplicationEvalOutcome::Computed(call_return);
        }

        // `typeof f<Args>` instantiation expression: specialize per-signature
        // (consume, not shadow, the callable's type params) — see helper (#10933).
        if let Some(specialized) = self.try_specialize_typeof_instantiation_expression(ctx, args) {
            crate::evaluation::eval_materialization_probe::record_application_body_path(
                crate::evaluation::eval_materialization_probe::ApplicationBodyPath::TypeofSpecialized,
            );
            return ApplicationEvalOutcome::Computed(specialized);
        }

        if let Some(type_params) = ctx.type_params.as_ref() {
            let Some(resolved) = ctx.resolved else {
                // A genuinely-unresolved import alias (`TS2307`) collapses to
                // `any` rather than staying opaque: the classification is final,
                // not a registration-window artifact (issue #14747, mirrors the
                // bodyless `else` branch below).
                if self.resolver.is_unresolved_import_def(def_id) {
                    return ApplicationEvalOutcome::Computed(TypeId::ANY);
                }
                // Generic def with registered params but no body: the def is
                // mid-registration (or owned by a file whose checker has not
                // published it yet). The opaque result is a registration-window
                // artifact — keep it out of the persistent caches.
                self.mark_unresolved_def_seen();
                crate::evaluation::eval_materialization_probe::record_application_body_path(
                    crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueUnresolvedNoBody,
                );
                return ApplicationEvalOutcome::Computed(original_type_id);
            };
            // When the resolver returns `unknown` for the alias body, it is
            // EITHER a registration-window placeholder — a cross-file alias
            // whose declaring file is still being processed in parallel
            // checking, where the body has not been published yet — OR a
            // genuine, finalized `unknown` body (`type C<T> = unknown`, or a
            // utility alias that reduces to `unknown`).
            //
            // For the placeholder, substituting the `unknown` would collapse
            // `Foo<Args>` to bare `unknown` and erase its structural shape
            // downstream, so we keep the original `Application` opaque and let a
            // later pass (with a populated body) expand it.
            //
            // For the genuine case we must NOT stay opaque: the opaque
            // `Application` is identity-distinct from `TypeId::UNKNOWN`, so a
            // member typed `() => C<number>` reaches the relation layer as a
            // deferred application the relation cannot recognize as `unknown`,
            // producing a false `unknown` ≠ `unknown` / `unknown` ≰ `C<...>`
            // (TS2719 / TS2322, issue #13212). The two cases are distinguished
            // by `get_def_raw_body`: a genuine body is recorded in the
            // definition store at alias-registration time, whereas the
            // placeholder `unknown` comes from an unresolved symbol-type
            // fallback with no registered body. When the body is genuinely
            // `unknown`, return the canonical intrinsic directly: the body is
            // parameter-free, so the known-params instantiation/cache/display
            // path cannot refine it and only widens the observable surface of
            // this special case.
            if resolved == TypeId::UNKNOWN {
                // Reducing `Alias<Args>` to canonical `unknown` requires the
                // base def to be a *positively confirmed* type alias in THIS
                // evaluation's resolver context. A thin cross-arena
                // registration — where the resolver knows the `DefId` exists but
                // not its `DefKind` (`ctx.is_type_alias_def` is false) — is a
                // not-yet-materialized placeholder, never proof of a genuine
                // `type C = unknown`. Without this gate a lib
                // distributive-conditional alias (`Exclude`, `NonNullable`, …)
                // resolved consumer-first carries a placeholder `unknown` body
                // AND a lost lib file-origin in the consuming arena, so
                // `is_genuine_unknown_alias_body`'s file-origin check alone
                // misclassifies it as genuine and collapses
                // `Exclude<T | undefined, undefined>` to bare `unknown` (false
                // `TS2571`, issue #14740). Keeping the application opaque lets a
                // later pass expand the real conditional body. The narrower gate
                // lives here (the evaluator's reduction) rather than in the
                // shared `is_genuine_unknown_alias_body` predicate, which the
                // relation layer also consumes for deferred `unknown`-returning
                // members (#13212 / #14595).
                let genuine_unknown_body = genuine_unknown_alias_reduction_enabled()
                    && ctx.is_type_alias_def
                    && self
                        .resolver
                        .is_genuine_unknown_alias_body(def_id, self.interner);
                if genuine_unknown_body {
                    return ApplicationEvalOutcome::Computed(TypeId::UNKNOWN);
                }
                self.mark_unresolved_def_seen();
                crate::evaluation::eval_materialization_probe::record_application_body_path(
                    crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueResolvedUnknown,
                );
                return ApplicationEvalOutcome::Computed(original_type_id);
            }

            // The same situation arises when the body resolves to the alias's
            // own self-lazy wrapper `Lazy(def_id)`: the structural body (e.g. a
            // mapped type registered on demand in the type environment) is not
            // available on this query, so substituting `Args` into
            // `Lazy(def_id)` yields bare `Lazy(def_id)` — dropping the type
            // arguments. Caching that degenerate result poisons every later
            // use: a nested `Partial<X>` first evaluated while `Partial`'s body
            // is still self-lazy makes the enclosing `Omit<Partial<X>, K>`
            // collapse to `{}`, producing a false `TS2345` against a fresh
            // object literal with a valid optional subset (see #10682). Keep
            // the application opaque so a later pass (with the populated body)
            // expands it correctly.
            //
            // Gate on the outermost (non-recursive) expansion of this def:
            // `increment_def_depth` has already run for this entry, so depth 1
            // is the first expansion. For a genuinely recursive alias the
            // self-lazy wrapper is the legitimate cycle breaker at deeper
            // entries, where bailing must not interfere.
            if self.def_depth.get(&def_id).copied().unwrap_or(0) <= 1
                && matches!(
                    self.interner.lookup(resolved),
                    Some(TypeData::Lazy(body_def_id)) if body_def_id == def_id
                )
            {
                self.mark_unresolved_def_seen();
                crate::evaluation::eval_materialization_probe::record_application_body_path(
                    crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueSelfLazy,
                );
                return ApplicationEvalOutcome::Computed(original_type_id);
            }
            crate::evaluation::eval_materialization_probe::record_application_body_path(
                crate::evaluation::eval_materialization_probe::ApplicationBodyPath::KnownParams,
            );
            self.evaluate_application_with_known_params(
                def_id,
                original_type_id,
                args,
                resolved,
                type_params,
                ctx.prefer_application_display_alias,
                ctx.base_is_type_query,
            )
        } else if let Some(resolved) = ctx.resolved {
            // Lite-resolver fallback: extract type parameters from the
            // resolved type's properties. A `typeof X<Args>` base whose
            // signatures could consume `Args` was already specialized at the
            // top of this function; reaching here means the arity did not
            // match, so keep it opaque (invalid instantiation, TS2635/TS2344
            // parity) instead of feeding it to the extracted-params path.
            if ctx.base_is_type_query
                && matches!(self.interner.lookup(resolved), Some(TypeData::Callable(_)))
                && !args.is_empty()
            {
                crate::evaluation::eval_materialization_probe::record_application_body_path(
                    crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueTypeQueryCallable,
                );
                return ApplicationEvalOutcome::Computed(original_type_id);
            }
            // Class-instance extraction must apply on this path too: when a
            // `DefKind::Class` body resolves to the constructor (value side),
            // instantiating it directly would produce a `typeof C`-shaped
            // application for a type-position reference (#13185). Mirror the
            // known-params path's unwrap; `typeof C<Args>` bases were already
            // returned above.
            let resolved = self.extract_class_instance_body(def_id, resolved);
            let extracted_params = self.extract_type_params_from_type(resolved);
            if !extracted_params.is_empty() && extracted_params.len() == args.len() {
                crate::evaluation::eval_materialization_probe::record_application_body_path(
                    crate::evaluation::eval_materialization_probe::ApplicationBodyPath::ExtractedParams,
                );
                self.evaluate_application_with_extracted_params(
                    def_id,
                    original_type_id,
                    args,
                    resolved,
                    &extracted_params,
                    ctx.prefer_application_display_alias,
                )
            } else {
                crate::evaluation::eval_materialization_probe::record_application_body_path(
                    crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueExtractedMismatch,
                );
                ApplicationEvalOutcome::Computed(original_type_id)
            }
        } else if self.resolver.is_unresolved_import_def(def_id) {
            // The base def backs an `import` alias whose module failed to
            // resolve (`TS2307`). Unlike the registration-window cases below,
            // this classification is final — the module is genuinely missing —
            // so the application collapses to `any`, matching `tsc`'s error-type
            // substitution for a reference whose target failed to resolve. This
            // mirrors the no-type-argument path, where the checker already
            // poisons a bare unresolved-import reference to `any`; without it
            // `Gen<{...}>` from `import { Gen } from "missing"` survives as a
            // live structural application the relation layer rejects (false
            // `TS2322`/`TS2345`/`TS2353` cascade, issue #14747). The result is
            // stable, so it is NOT tainted with `mark_unresolved_def_seen`.
            crate::evaluation::eval_materialization_probe::record_application_body_path(
                crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueNoRegisteredBody,
            );
            ApplicationEvalOutcome::Computed(TypeId::ANY)
        } else {
            // Neither type parameters nor a body are registered for the base
            // def (e.g. an import-alias `DefId` that was never forwarded to
            // its target, or a def evaluated before its declaring file
            // published anything). Same registration-window taint as above.
            self.mark_unresolved_def_seen();
            crate::evaluation::eval_materialization_probe::record_application_body_path(
                crate::evaluation::eval_materialization_probe::ApplicationBodyPath::OpaqueNoRegisteredBody,
            );
            ApplicationEvalOutcome::Computed(original_type_id)
        }
    }

    /// Known-params application path: argument preparation, expanded-args
    /// cache lookup, homomorphic passthrough, class-instance extraction,
    /// mapped-union distribution, then the main `instantiate_generic` +
    /// evaluate sequence with display-alias storage.
    fn evaluate_application_with_known_params(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        args: &[TypeId],
        resolved: TypeId,
        type_params: &[TypeParamInfo],
        prefer_application_display_alias: bool,
        base_is_type_query: bool,
    ) -> ApplicationEvalOutcome {
        let expanded_args = self.prepare_expanded_args_for_body(resolved, args);
        let no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        if let Some(db) = self.query_db {
            let cached = db.lookup_application_eval_cache(
                def_id,
                &expanded_args,
                no_unchecked_indexed_access,
            );
            crate::evaluation::eval_materialization_probe::record_application_cache_lookup(
                crate::evaluation::eval_materialization_probe::ApplicationLookupSite::ExpandedArgs,
                cached.is_some(),
            );
            if let Some(cached) = cached {
                return ApplicationEvalOutcome::ShortCircuit(cached);
            }
        }

        // Homomorphic mapped-type passthrough for non-object arguments.
        // tsc's `instantiateMappedType` returns the argument directly when
        // the body is `{ [P in keyof T]: ... }` and T is not an object
        // type. Runs BEFORE instantiation because `instantiate_generic`
        // eagerly evaluates `keyof T` when T is concrete, destroying the
        // structural information needed for passthrough detection later.
        if let Some(passthrough) = self.try_homomorphic_mapped_passthrough(
            def_id,
            resolved,
            type_params,
            &expanded_args,
            no_unchecked_indexed_access,
        ) {
            return ApplicationEvalOutcome::ShortCircuit(passthrough);
        }

        // Class instance extraction: when a class is used in type position
        // via `Application` (e.g. `Component<P, S>`), the INSTANCE type
        // (the first construct signature's return type) is what we want,
        // not the class constructor type. Only applies for
        // `DefKind::Class`; interfaces with construct signatures keep
        // their Callable shape intact.
        //
        // Exception: when the base is a `TypeQuery` (`typeof ClassName<T>`),
        // the caller wants the constructor type — skipping extraction keeps
        // the specialized constructor so `InstanceType<typeof Cls<T>>` can
        // correctly reduce to the class instance type via conditional infer.
        let effective_body = if base_is_type_query {
            resolved
        } else {
            self.extract_class_instance_body(def_id, resolved)
        };

        // Homomorphic mapped-type union distribution: when the alias body
        // is `{ [K in keyof T]: ... }` and T's argument resolves to a
        // union, distribute over union members BEFORE calling
        // `instantiate_generic` so the mapped evaluator can distinguish
        // the post-instantiation constraint from the declared one.
        if let Some(distributed) = self.try_distribute_mapped_union_arg(
            def_id,
            effective_body,
            type_params,
            &expanded_args,
            no_unchecked_indexed_access,
        ) {
            return ApplicationEvalOutcome::ShortCircuit(distributed);
        }

        let evaluated = self.instantiate_and_finalize_application(
            def_id,
            original_type_id,
            ApplicationFinalizeContext {
                original_args: args,
                expanded_args: &expanded_args,
                body: effective_body,
                type_params,
                prefer_application_display_alias,
                record_structural_back_reference: true,
                no_unchecked_indexed_access,
            },
        );
        ApplicationEvalOutcome::Computed(evaluated)
    }

    /// Lite-resolver fallback application path. Used when the resolver
    /// does not surface formal type parameters (`get_lazy_type_params`
    /// returned `None`) but the resolved body itself embeds
    /// `TypeParameter` types that can be recovered structurally.
    fn evaluate_application_with_extracted_params(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        args: &[TypeId],
        resolved: TypeId,
        type_params: &[TypeParamInfo],
        prefer_application_display_alias: bool,
    ) -> ApplicationEvalOutcome {
        let expanded_args = self.expand_type_args(args);
        let no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        if let Some(db) = self.query_db {
            let cached = db.lookup_application_eval_cache(
                def_id,
                &expanded_args,
                no_unchecked_indexed_access,
            );
            crate::evaluation::eval_materialization_probe::record_application_cache_lookup(
                crate::evaluation::eval_materialization_probe::ApplicationLookupSite::ExpandedArgs,
                cached.is_some(),
            );
            if let Some(cached) = cached {
                return ApplicationEvalOutcome::ShortCircuit(cached);
            }
        }

        let evaluated = self.instantiate_and_finalize_application(
            def_id,
            original_type_id,
            ApplicationFinalizeContext {
                original_args: args,
                expanded_args: &expanded_args,
                body: resolved,
                type_params,
                prefer_application_display_alias,
                record_structural_back_reference: false,
                no_unchecked_indexed_access,
            },
        );
        ApplicationEvalOutcome::Computed(evaluated)
    }

    /// Expand `Application(base, args)` arguments based on the alias body
    /// shape.
    ///
    /// * Conditional bodies preserve `TypeParameter` args (the conditional
    ///   evaluator needs them in generic form to match at the `infer`
    ///   site) but eagerly expand concrete args.
    /// * Bodies whose extends-side is `Application(...infer...)` preserve
    ///   `Application` args so the matcher can compare at the application
    ///   level (e.g. `Promise<string>` vs `Promise<infer U>`).
    /// * Everything else uses the default `expand_type_args` which
    ///   evaluates `TypeQuery`, `Application`, and meta-types.
    fn prepare_expanded_args_for_body<'b>(
        &mut self,
        body: TypeId,
        args: &'b [TypeId],
    ) -> std::borrow::Cow<'b, [TypeId]> {
        let arg_preservation =
            crate::type_queries::classify_body_for_arg_preservation(self.interner, body);
        let body_is_conditional =
            matches!(self.interner.lookup(body), Some(TypeData::Conditional(_)));
        if matches!(
            arg_preservation,
            crate::type_queries::BodyArgPreservation::ConditionalApplicationInfer
        ) {
            std::borrow::Cow::Owned(
                args.iter()
                    .map(|&arg| self.prepare_conditional_application_infer_arg(arg))
                    .collect(),
            )
        } else if body_is_conditional {
            std::borrow::Cow::Owned(
                args.iter()
                    .map(|&arg| {
                        if crate::visitor::contains_type_parameters(self.interner, arg) {
                            arg
                        } else {
                            self.try_expand_type_arg(arg)
                        }
                    })
                    .collect(),
            )
        } else if matches!(
            arg_preservation,
            crate::type_queries::BodyArgPreservation::ConditionalInfer
                | crate::type_queries::BodyArgPreservation::ConditionalApplicationInfer
        ) {
            std::borrow::Cow::Owned(self.expand_type_args_preserve_applications(args))
        } else {
            self.expand_type_args(args)
        }
    }

    fn prepare_conditional_application_infer_arg(&mut self, arg: TypeId) -> TypeId {
        if crate::visitor::contains_type_parameters(self.interner, arg) {
            return arg;
        }
        if let Some(reduced) = self.reduce_alias_body_to_application_form(arg)
            && matches!(
                self.interner.lookup(reduced),
                Some(TypeData::Application(_))
            )
        {
            return reduced;
        }
        if matches!(self.interner.lookup(arg), Some(TypeData::Application(_))) {
            arg
        } else {
            self.try_expand_type_arg(arg)
        }
    }

    /// Homomorphic mapped-type passthrough.
    ///
    /// Returns `Some(value)` (with the cache populated) when the body is a
    /// `{ [P in keyof T]: ... }` mapped type and the argument for `T`
    /// matches one of two passthrough rules:
    /// * primitive (or array-constrained any/unknown/never) — return the
    ///   argument directly;
    /// * identity body `{ [P in keyof T]: T[P] }` over `any` — return
    ///   `{ [x: string]: any; [x: number]: any }` so the result is not
    ///   assignable to `any[]`.
    fn try_homomorphic_mapped_passthrough(
        &mut self,
        def_id: DefId,
        body: TypeId,
        type_params: &[TypeParamInfo],
        expanded_args: &[TypeId],
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        let preamble = self.homomorphic_mapped_arg(body, type_params, expanded_args)?;
        let HomomorphicMappedArg {
            mapped,
            source,
            tp,
            resolved_arg,
            ..
        } = preamble;

        // Passthrough for genuine primitives. For `any`/`unknown`/`never`/
        // `error`: only passthrough when the type parameter is constrained
        // to array/tuple types (e.g. `Arrayish<T extends unknown[]>`).
        // Otherwise these top/bottom types must flow through mapped type
        // expansion so `Objectish<any>` becomes
        // `{ [x: string]: any; [x: number]: any }` (matching tsc).
        let is_any_like = resolved_arg == TypeId::ANY
            || resolved_arg == TypeId::UNKNOWN
            || resolved_arg == TypeId::NEVER
            || resolved_arg == TypeId::ERROR;
        let should_passthrough = if is_any_like {
            tp.constraint.is_some_and(|c| {
                let eval_c = self.evaluate(c);
                matches!(
                    self.interner.lookup(eval_c),
                    Some(TypeData::Array(_) | TypeData::Tuple(_))
                )
            })
        } else {
            Self::is_primitive_or_primitive_union(self.interner, resolved_arg)
        };
        if should_passthrough {
            self.insert_application_eval_cache_if_some(
                def_id,
                expanded_args,
                no_unchecked_indexed_access,
                resolved_arg,
            );
            return Some(resolved_arg);
        }

        // Objectish<any>: identity homomorphic mapped type with `any`
        // argument and non-array constraint. tsc produces
        // `{ [x: string]: any; [x: number]: any }` (NOT `any`), keeping
        // the result not assignable to `any[]`. Previously handled in
        // checker-local object construction; centralized here for
        // architectural correctness.
        if resolved_arg == TypeId::ANY
            && let Some((obj, key)) = crate::index_access_parts(self.interner, mapped.template)
            && obj == source
            && matches!(
                self.interner.lookup(key),
                Some(TypeData::TypeParameter(kp)) if kp.name == mapped.type_param.name
            )
        {
            use crate::types::{IndexSignature, ObjectShape};
            let result = self.interner.object_with_index(ObjectShape {
                flags: crate::types::ObjectFlags::empty(),
                properties: vec![],
                string_index: Some(IndexSignature {
                    key_type: TypeId::STRING,
                    value_type: TypeId::ANY,
                    readonly: false,
                    param_name: None,
                }),
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type: TypeId::ANY,
                    readonly: false,
                    param_name: None,
                }),
                symbol_index: None,
                symbol: None,
            });
            self.insert_application_eval_cache_if_some(
                def_id,
                expanded_args,
                no_unchecked_indexed_access,
                result,
            );
            return Some(result);
        }

        None
    }

    /// Shared opening preamble for the two body-aware homomorphic-mapped
    /// shortcuts. Returns the structured `(mapped, source, tp, idx,
    /// resolved_arg)` tuple when `body` is `{ [P in keyof Tᵢ]: ... }` and
    /// the argument for `Tᵢ` resolves cleanly. Returns `None` if any guard
    /// in the chain fails.
    ///
    /// Extracted from the two call sites so a future change to the
    /// guard cannot drift between passthrough and union-distribute.
    fn homomorphic_mapped_arg(
        &mut self,
        body: TypeId,
        type_params: &[TypeParamInfo],
        expanded_args: &[TypeId],
    ) -> Option<HomomorphicMappedArg> {
        let TypeData::Mapped(mapped_id) = self.interner.lookup(body)? else {
            return None;
        };
        let mapped = self.interner.get_mapped(mapped_id);
        let TypeData::KeyOf(source) = self.interner.lookup(mapped.constraint)? else {
            return None;
        };
        let TypeData::TypeParameter(tp) = self.interner.lookup(source)? else {
            return None;
        };
        let idx = type_params.iter().position(|p| p.name == tp.name)?;
        if idx >= expanded_args.len() {
            return None;
        }
        let arg = expanded_args[idx];
        let resolved_arg = self.evaluate(arg);
        Some(HomomorphicMappedArg {
            mapped,
            source,
            tp,
            idx,
            resolved_arg,
        })
    }

    /// Whether an application-eval result produced by this run may be persisted
    /// to the cross-evaluator `application_eval_cache`.
    ///
    /// The cache key is `(DefId, expanded_args, no_unchecked)` — it is
    /// *resolver-* and *substitution-independent*, but it is NOT independent of
    /// the ambient stack depth at the use site. When a recursive alias bails
    /// because *its own* expansion was already deep, the result is a truncated
    /// stack-context artifact. Persisting it poisons every *other* use site of
    /// the same alias application — the "alias fan-out regression": one deep use
    /// contaminating all of its siblings, which would each converge on their own
    /// shallower stack.
    ///
    /// The discriminator is the per-application epoch, not the sticky
    /// `recursion_limit_hit` flag. `deep_recursion_seen` / `silent_depth_bailed`
    /// are set by the *first* bail anywhere in the run and never reset, so gating
    /// on them disabled every later write too — including results for unrelated
    /// applications whose own bodies expanded fully and terminated. Those results
    /// are complete, stack-independent functions of `(DefId, args)` and are safe,
    /// indeed necessary, to cache: without them the same finite application is
    /// re-instantiated combinatorially across each sibling branch, turning a
    /// terminating type into an effective hang (#10834, the `TypeBox` / zod
    /// `Static<TObject<…>>` schema shape).
    ///
    /// `limit_epoch == app_body_limit_epoch` is true exactly when no
    /// cycle/depth/iteration/divergence event fired anywhere within the body
    /// subtree of the application currently being finalized. That is strictly
    /// more permissive than `!recursion_limit_hit()` (it also admits clean bodies
    /// that ran *after* an earlier sibling bailed) while still never persisting a
    /// truncated result — a bail inside this body advances `limit_epoch` past the
    /// snapshot taken at body entry. Termination is owned by the recursion guards
    /// and fuel, not by this cache, so a (now rarer) skipped write cannot
    /// reintroduce a hang.
    ///
    /// This is the *epoch* permit. A second, complementary permit —
    /// [`is_concrete_application_fixpoint`](Self::is_concrete_application_fixpoint)
    /// — additionally admits fully-concrete, fully-resolved results that are
    /// stack-independent by construction even when this epoch test fails. Both
    /// permits are consulted at the single write site
    /// [`insert_application_eval_cache_if_some`](Self::insert_application_eval_cache_if_some).
    #[inline]
    const fn application_eval_result_cacheable(&self) -> bool {
        self.limit_epoch == self.app_body_limit_epoch
    }

    #[inline]
    const fn application_body_saw_unresolved_def(&self) -> bool {
        self.unresolved_def_epoch != self.app_body_unresolved_def_epoch
    }

    /// Insert into the application-eval cache iff `query_db` is connected and
    /// the result is safe to persist under the resolver-independent
    /// `(DefId, expanded_args, no_unchecked)` key.
    ///
    /// Folds the two-line `if let Some(db) = self.query_db { … }` idiom
    /// repeated in every body-aware shortcut and finalize helper.
    ///
    /// Writes stay gated on an authoritative (full-resolver) `query_db`
    /// context: a limited resolver could otherwise store an under-resolved
    /// result under the resolver-independent `(DefId, args)` key and poison
    /// sibling reads. Reads use the same explicit `query_db` gate for the same
    /// reason.
    ///
    /// A body that observed an unresolved def is never persisted: the concrete
    /// fixpoint permit cannot prove stability once an under-resolved conditional
    /// has collapsed to a concrete sentinel like `never`.
    ///
    /// Otherwise the result is persisted when *either* permit holds:
    /// - [`application_eval_result_cacheable`](Self::application_eval_result_cacheable):
    ///   no cycle/depth/iteration/divergence event fired in this application's
    ///   body subtree, so the result is not a stack-context artifact; or
    /// - [`is_concrete_application_fixpoint`](Self::is_concrete_application_fixpoint):
    ///   the arguments are fully concrete and the result is fully resolved, so
    ///   the value is an ambient-stack-independent fixpoint even if a *deeper*
    ///   sub-expansion brushed a limit (issue #13508, root cause B).
    fn insert_application_eval_cache_if_some(
        &self,
        def_id: DefId,
        expanded_args: &[TypeId],
        no_unchecked_indexed_access: bool,
        evaluated: TypeId,
    ) {
        let body_saw_unresolved_def = self.application_body_saw_unresolved_def();
        let cacheable = !body_saw_unresolved_def
            && (self.application_eval_result_cacheable()
                || self.is_concrete_application_fixpoint(expanded_args, evaluated));
        crate::evaluation::eval_materialization_probe::record_application_cache_insert(
            cacheable,
            self.query_db.is_some(),
        );
        if !cacheable {
            return;
        }
        // A limited-resolver (first-pass `TypeEnvironment`) evaluator must never
        // *write* the resolver-independent `(DefId, args)` application-eval
        // cache. Even a fully-materialized result can be context-dependent: a
        // conditional that binds `infer` against the inference/contextual state
        // at the use site (`propTypeValidatorInference`) produces a concrete
        // result that is NOT a pure function of `(DefId, args)`. Persisting it
        // would poison a later authoritative read. The limited pass still
        // *reads* the cache (authoritative entries are always correct) and
        // still shares the resolver-independent instantiation cache (pure
        // structural substitution), which is where its cross-block reuse comes
        // from.
        if self.limited_resolver {
            return;
        }
        if let Some(db) = self.query_db {
            db.insert_application_eval_cache(
                def_id,
                expanded_args,
                no_unchecked_indexed_access,
                evaluated,
            );
        }
    }

    /// Whether a `(args -> result)` application is a *concrete fixpoint*: its
    /// arguments carry no free type parameters and its result is fully resolved
    /// (no free type parameter and no `error` sentinel).
    ///
    /// Such a result is a complete, ambient-stack-independent function of
    /// `(def_id, args, no_unchecked)` — exactly the per-`(symbol, type-arg
    /// tuple)` instantiation fixpoint `tsc` shares via its `resolvingType` memo
    /// — so it is safe to persist even when
    /// [`application_eval_result_cacheable`](Self::application_eval_result_cacheable)
    /// would withhold the write because a *deeper* sub-expansion advanced
    /// `limit_epoch`. That epoch gate is correct but over-broad for the shape
    /// that dominates the recursive conditional-alias canaries (`typebox`
    /// `Static<…>`, `remeda` `FilteredArray<…>`): any depth/fuel truncation
    /// leaves an `error` sentinel (or an unexpanded type parameter) in the
    /// result, so the `error`-free + parameter-free predicate is precisely what
    /// distinguishes a genuine fixpoint from a truncated stack-context
    /// artifact. Without sharing these, every sibling conditional arm and every
    /// fresh assignability-check evaluator re-walks the same enormous concrete
    /// type — the "limit ↔ sharing" circularity (the limit fires *because* the
    /// DAG is re-walked, and sharing is blocked *because* the limit fired).
    /// Termination stays owned by the recursion guards and fuel, so this only
    /// collapses redundant re-evaluation.
    ///
    /// Parameter-freedom and `error`-freedom alone are NOT sufficient: a
    /// recursive conditional alias whose body expansion was *deferred* (a depth
    /// guard returned the unevaluated branch, or a relation/limited-resolver
    /// sub-evaluation declined to reduce) leaves a residual `Application` of the
    /// alias **in the result** — e.g. `D<{id:0}[]>` finalizing to the
    /// unevaluated `D<{id:0}>`. That residue carries no free type parameter and
    /// no `error`, so it passes both predicates above, yet it is a self-applied
    /// placeholder, not a converged value. Persisting `(D, args) -> D<…>` poisons
    /// the resolver-independent cache: a later authoritative read returns the
    /// deferred self-application, whose re-evaluation re-reads the same entry and
    /// re-applies `D` on the same input without ever recognizing the cycle —
    /// unbounded recursion to a SIGABRT (#14123, a regression introduced when
    /// this permit was added in #13508/#13894). A genuine concrete fixpoint of a
    /// terminating recursion contains no residual application of a recursive
    /// alias, so [`Self::result_has_residual_recursive_alias`] is the final
    /// discriminator.
    fn is_concrete_application_fixpoint(&self, args: &[TypeId], result: TypeId) -> bool {
        // Concrete instantiation: every argument must itself be parameter-free,
        // so the key identifies one global instantiation rather than a
        // context-dependent one. Checked first because it is cheap (few args)
        // and gates the deeper result walk below.
        if args
            .iter()
            .any(|&arg| crate::type_queries::contains_type_parameters_db(self.interner, arg))
        {
            return false;
        }
        // A fully-resolved result: no `error` sentinel (a depth/fuel truncation
        // artifact) and no free type parameter (which would make the value
        // context-dependent). Either disqualifies the result as a fixpoint.
        // The bare-`ERROR` test is a cheap fast-path for the common bail value
        // before the deep `contains_error_type_db` walk.
        result != TypeId::ERROR
            && !crate::type_queries::contains_error_type_db(self.interner, result)
            && !crate::type_queries::contains_type_parameters_db(self.interner, result)
            && !self.result_has_residual_recursive_alias(result)
    }

    /// Whether `result` still contains an unevaluated `Application` of a
    /// *recursive* generic type — one whose resolved body re-references its own
    /// `DefId`. Such a residue means the recursion was deferred (a depth guard
    /// returned the unevaluated branch, or a relation/limited sub-evaluation
    /// declined to reduce it) rather than converged, so `result` is a self- or
    /// mutually-applied placeholder, not a fixpoint. Caching it would let a
    /// later read re-enter the same recursive evaluation and re-apply the alias
    /// on the same input without recognizing the cycle (#14123).
    ///
    /// Only `Application` (generic-instantiation) nodes are inspected: a
    /// concrete, fully-reduced result of a terminating recursion never retains
    /// an application of a recursive alias. Non-recursive applications (whose
    /// body does not refer to itself) cannot re-enter their own evaluation on a
    /// cache read, so they remain cacheable residue.
    ///
    /// `DefKind` is intentionally *not* consulted: under a limited/relation
    /// resolver `get_def_kind` can return `None` for a def that nonetheless
    /// `resolve_lazy`-resolves to a self-referential body (the exact case in
    /// #14123). The body-self-reference signal is the resolver-robust
    /// discriminator.
    fn result_has_residual_recursive_alias(&self, result: TypeId) -> bool {
        // Memoize the self-reference verdict per `DefId`: a result can carry many
        // applications of the same alias, and `contains_lazy_def_id` walks the
        // alias body on every call. The cache collapses that to one body walk
        // per distinct def.
        let mut is_recursive: FxHashMap<DefId, bool> = FxHashMap::default();
        let mut found = false;
        crate::visitor::walk_referenced_types(self.interner, result, |current| {
            if found {
                return;
            }
            let Some(TypeData::Application(app_id)) = self.interner.lookup(current) else {
                return;
            };
            let base = self.interner.type_application(app_id).base;
            let Some(def_id) = self.resolve_application_def_id(base) else {
                return;
            };
            found = *is_recursive.entry(def_id).or_insert_with(|| {
                self.resolver
                    .resolve_lazy(def_id, self.interner)
                    .is_some_and(|body| {
                        crate::visitor::contains_lazy_def_id(self.interner, body, def_id)
                    })
            });
        });
        found
    }

    /// Extract the instance side of a class-shaped resolved body.
    ///
    /// Returns the body unchanged for interfaces and aliases. For
    /// `DefKind::Class`, returns the first construct signature's return
    /// type (the INSTANCE type) so `Component<P, S>` in type position
    /// refers to the instance rather than `typeof Component`. Interfaces
    /// with construct signatures (e.g. `ComponentClass<P>`) keep their
    /// Callable shape — only classes are unwrapped.
    fn extract_class_instance_body(&self, def_id: DefId, resolved: TypeId) -> TypeId {
        let is_class_def = matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Class)
        );
        if !is_class_def {
            return resolved;
        }
        let Some(TypeData::Callable(cs_id)) = self.interner.lookup(resolved) else {
            return resolved;
        };
        let shape = self.interner.callable_shape(cs_id);
        match shape.construct_signatures.first() {
            Some(construct_sig) => {
                tracing::trace!(
                    def_id = def_id.0,
                    instance = construct_sig.return_type.0,
                    "extract_class_instance_body: unwrapped construct-signature return"
                );
                construct_sig.return_type
            }
            None => resolved,
        }
    }

    /// Resolve a recursive-call-return placeholder `Application(Lazy(value_def),
    /// type_args)` to the call signature's return type, instantiated with
    /// `type_args`.
    ///
    /// Returns `Some` only when `def_id` is a value-space symbol
    /// (`DefKind::Variable`/`DefKind::Function`) and `resolved` is a callable
    /// with a generic call signature whose arity matches `type_args`. The
    /// returned type is instantiated one level and left otherwise un-evaluated:
    /// nested self-referential returns stay deferred (they carry the inner
    /// signature's free type parameter) and expand lazily on demand, matching
    /// `tsc`'s recursive object type — never eagerly expanding into an
    /// excessively deep instantiation.
    ///
    /// `None` keeps the normal generic-instantiation path, so type aliases,
    /// classes, interfaces, and non-generic value functions are unaffected.
    fn value_call_return_application(
        &self,
        def_id: DefId,
        resolved: TypeId,
        type_args: &[TypeId],
    ) -> Option<TypeId> {
        if type_args.is_empty() {
            return None;
        }
        if !matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Variable | crate::def::DefKind::Function)
        ) {
            return None;
        }
        // The resolved value type is either a single-signature `Function` or a
        // multi-signature `Callable`; in both cases pick the generic call
        // signature whose type-parameter arity matches the supplied
        // `type_args` and instantiate its return type.
        match self.interner.lookup(resolved)? {
            TypeData::Function(fs_id) => {
                let shape = self.interner.function_shape(fs_id);
                if shape.is_constructor
                    || shape.type_params.is_empty()
                    || shape.type_params.len() != type_args.len()
                {
                    return None;
                }
                Some(self.cached_generic_instantiation(
                    shape.return_type,
                    &shape.type_params,
                    type_args,
                ))
            }
            TypeData::Callable(cs_id) => {
                let shape = self.interner.callable_shape(cs_id);
                let signature = shape.call_signatures.iter().find(|sig| {
                    !sig.type_params.is_empty() && sig.type_params.len() == type_args.len()
                })?;
                Some(self.cached_generic_instantiation(
                    signature.return_type,
                    &signature.type_params,
                    type_args,
                ))
            }
            _ => None,
        }
    }

    /// Homomorphic mapped-type distribution over a union argument.
    ///
    /// Returns `Some(union)` (with cache populated) when the body is a
    /// homomorphic mapped type and the argument for `T` resolves to a
    /// non-array/non-tuple union. Distributes per member, calling
    /// `instantiate_generic` once per non-primitive member; primitive
    /// members pass through unchanged so `Partial<string | { x: number }>`
    /// becomes `string | { x?: number }` instead of `string | string`.
    fn try_distribute_mapped_union_arg(
        &mut self,
        def_id: DefId,
        effective_body: TypeId,
        type_params: &[TypeParamInfo],
        expanded_args: &[TypeId],
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        let HomomorphicMappedArg {
            idx, resolved_arg, ..
        } = self.homomorphic_mapped_arg(effective_body, type_params, expanded_args)?;
        let TypeData::Union(list_id) = self.interner.lookup(resolved_arg)? else {
            return None;
        };
        let members = self.interner.type_list(list_id).to_vec();
        let mut distributed = Vec::with_capacity(members.len());
        for member in members {
            if crate::visitors::visitor_predicates::is_primitive_type(self.interner, member) {
                distributed.push(member);
                continue;
            }
            let mut member_args = expanded_args.to_vec();
            member_args[idx] = member;
            let instantiated =
                self.cached_generic_instantiation(effective_body, type_params, &member_args);
            distributed.push(self.evaluate(instantiated));
        }
        let evaluated = self.interner.union(distributed);
        self.insert_application_eval_cache_if_some(
            def_id,
            expanded_args,
            no_unchecked_indexed_access,
            evaluated,
        );
        Some(evaluated)
    }

    /// Instantiate + evaluate the body for an application and record the
    /// appropriate display-alias provenance.
    ///
    /// Display-alias storage is gated on `prefer_application_display_alias`:
    /// type-alias applications whose evaluation produces an intermediate
    /// `Application` form store a forward display alias so diagnostics show
    /// the apparent name (e.g. `DeepReadonlyObject<Part>`).
    ///
    /// `record_structural_back_reference` is `true` only on the known-params
    /// path where the resolver surfaced a nominal interface/class signal
    /// strong enough to back-reference from the evaluated structural form to
    /// the original `Application`. The lite-resolver fallback path keeps
    /// this off because it cannot prove the nominal origin.
    fn instantiate_and_finalize_application(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        context: ApplicationFinalizeContext<'_>,
    ) -> TypeId {
        let ApplicationFinalizeContext {
            original_args,
            expanded_args,
            body,
            type_params,
            prefer_application_display_alias,
            record_structural_back_reference,
            no_unchecked_indexed_access,
        } = context;
        let mut instantiated = self.cached_generic_instantiation(body, type_params, expanded_args);
        // Rebind polymorphic `this` to the concrete application so
        // interface bodies like `constraint: Constraint<this>` preserve
        // their receiver-specific invariance.
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                instantiated,
                original_type_id,
            );
        }

        match self.classify_same_alias_expansion(
            def_id,
            original_type_id,
            expanded_args,
            instantiated,
        ) {
            SameAliasExpansion::DefDepthOwned => {
                return self.finish_instantiated_application(
                    original_type_id,
                    ApplicationFinalizeContext {
                        original_args,
                        expanded_args,
                        body: instantiated,
                        type_params,
                        prefer_application_display_alias,
                        record_structural_back_reference,
                        no_unchecked_indexed_access,
                    },
                );
            }
            SameAliasExpansion::DivergentConditional => {
                self.mark_depth_exceeded_for_request();
                return TypeId::ERROR;
            }
            SameAliasExpansion::None => {}
        }

        self.with_meta_rereduce_recursion_identity(
            original_type_id,
            original_type_id,
            |evaluator| {
                evaluator.finish_instantiated_application(
                    original_type_id,
                    ApplicationFinalizeContext {
                        original_args,
                        expanded_args,
                        body: instantiated,
                        type_params,
                        prefer_application_display_alias,
                        record_structural_back_reference,
                        no_unchecked_indexed_access,
                    },
                )
            },
        )
    }

    fn finish_instantiated_application(
        &mut self,
        original_type_id: TypeId,
        context: ApplicationFinalizeContext<'_>,
    ) -> TypeId {
        let ApplicationFinalizeContext {
            original_args,
            expanded_args,
            body: instantiated,
            type_params: _,
            prefer_application_display_alias,
            record_structural_back_reference,
            no_unchecked_indexed_access,
        } = context;

        // Preserve discriminated object intersections after instantiation.
        // Re-evaluating them here distributes impossible branches again,
        // which breaks both fresh EPC and `keyof` on generic applications.
        let evaluated = if crate::type_queries::is_discriminated_object_intersection(
            self.interner,
            instantiated,
        ) {
            instantiated
        } else {
            self.evaluate(instantiated)
        };
        if prefer_application_display_alias {
            self.store_intermediate_application_display_alias(
                instantiated,
                original_type_id,
                evaluated,
                original_args,
            );
        } else if record_structural_back_reference {
            self.store_parametric_structural_back_reference(evaluated, original_type_id);
        }
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(original_type_id) {
            let app = self.interner.type_application(app_id);
            if let Some(def_id) = self.resolve_application_def_id(app.base) {
                self.insert_application_eval_cache_if_some(
                    def_id,
                    expanded_args,
                    no_unchecked_indexed_access,
                    evaluated,
                );
            }
        }
        evaluated
    }

    /// Classify one instantiation step that left the same alias still to be
    /// expanded, so the meta-rereduce identity guard neither preempts the
    /// per-`DefId` `def_depth` accounting that owns `TS2589` nor lets a
    /// divergent conditional re-entry terminate silently — see
    /// [`SameAliasExpansion`] for the verdicts.
    fn classify_same_alias_expansion(
        &self,
        def_id: DefId,
        original_type_id: TypeId,
        expanded_args: &[TypeId],
        instantiated: TypeId,
    ) -> SameAliasExpansion {
        if self.instantiation_directly_grows_same_application(def_id, expanded_args, instantiated) {
            return SameAliasExpansion::DefDepthOwned;
        }
        // The wrapper-shape probes below only run on the rare about-to-defer
        // path, so the common convergent case pays nothing.
        if !self.meta_rereduce_recursion_identity_would_exceed_with_seen(original_type_id, &[])
            || !crate::visitor::contains_lazy_def_id(self.interner, instantiated, def_id)
        {
            return SameAliasExpansion::None;
        }
        // Only a fully *concrete* self-recursive expansion escapes to
        // `def_depth`: a still-generic application (an `Awaited`-shaped
        // distribution over a free-parameter union member) must stay behind
        // the identity wrapper and defer, resuming on instantiation.
        if expanded_args
            .iter()
            .any(|&arg| crate::type_queries::contains_type_parameters_db(self.interner, arg))
        {
            return SameAliasExpansion::None;
        }
        SameAliasExpansion::DefDepthOwned
    }

    fn instantiation_directly_grows_same_application(
        &self,
        def_id: DefId,
        expanded_args: &[TypeId],
        instantiated: TypeId,
    ) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(instantiated) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(next_def_id) = self.resolve_application_def_id(app.base) else {
            return false;
        };
        let same_def = def_id == next_def_id
            || self.resolver.canonical_def_id(def_id)
                == self.resolver.canonical_def_id(next_def_id)
            || self.resolver.defs_are_equivalent(def_id, next_def_id);
        same_def && self.application_args_extend_prior(expanded_args, &app.args)
    }

    fn application_args_extend_prior(&self, prior: &[TypeId], next: &[TypeId]) -> bool {
        next != prior
            && next.iter().any(|&next_arg| {
                prior.iter().any(|&prior_arg| {
                    next_arg == prior_arg || self.cached_contains_type_by_id(next_arg, prior_arg)
                })
            })
    }

    /// Record display-alias provenance after a successful application
    /// evaluation.
    ///
    /// Decides whether to repaint the alias name onto the evaluated
    /// structural form. Skipping the repaint protects unrelated diagnostics
    /// from being relabeled when:
    /// * the result is a non-empty structural shape that already existed
    ///   before this application,
    /// * the result is itself one of the application arguments,
    /// * a conditional branch alias is already pinned on `result`.
    ///
    /// When `my_apparent_branch` is set by the conditional evaluator and is
    /// distinct from the original application, also installs a one-step
    /// forward alias so the formatter shows the apparent intermediate name
    /// (e.g. `DeepReadonlyObject<Part>` instead of `DeepReadonly<Part>`).
    fn record_application_evaluation_display_aliases(
        &mut self,
        result: TypeId,
        original_type_id: TypeId,
        original_args: &[TypeId],
        is_type_alias_def: bool,
        prefer_application_display_alias: bool,
        my_apparent_branch: Option<TypeId>,
    ) {
        let display_origin = if self.expand_application_display_alias_args
            && let Some(TypeData::Application(original_app_id)) =
                self.interner.lookup(original_type_id)
        {
            let original_app = self.interner.type_application(original_app_id);
            let expanded_args = self.expand_type_args(&original_app.args);
            if expanded_args.as_ref() != original_app.args.as_slice() {
                let candidate = self
                    .interner
                    .application(original_app.base, expanded_args.into_owned());
                if self.cached_contains_type_by_id(candidate, result) {
                    original_type_id
                } else {
                    candidate
                }
            } else {
                original_type_id
            }
        } else {
            original_type_id
        };
        let has_param_args = original_args.iter().any(|&arg| {
            crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
        });
        // For concrete args the alias repaint is unconditional; for
        // generic args only Conditional/IndexAccess/Mapped results get
        // repainted (deferred mapped aliases retain the as-written
        // relationship needed for diagnostics like `Mapped<K>[Remapped<K>]`).
        if has_param_args
            && !matches!(
                self.interner.lookup(result),
                Some(
                    crate::types::TypeData::Conditional(_)
                        | crate::types::TypeData::IndexAccess(_, _)
                        | crate::types::TypeData::Mapped(_)
                )
            )
        {
            return;
        }

        let result_is_non_empty_structural = match self.interner.lookup(result) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            }
            Some(TypeData::Intersection(_)) => true,
            _ => false,
        };
        let result_is_application_arg = original_args.contains(&result);
        let skip_type_alias_repaint = matches!(
            self.interner.lookup(display_origin),
            Some(TypeData::Application(_))
        ) && result_is_non_empty_structural
            && (result_is_application_arg
                || (is_type_alias_def
                    && match (
                        self.interner.lookup_alloc_order(result),
                        self.interner.lookup_alloc_order(display_origin),
                    ) {
                        (Some(result_order), Some(display_order)) => result_order <= display_order,
                        _ => result.0 <= display_origin.0,
                    }));
        let keep_existing_conditional_branch_alias = is_type_alias_def
            && !prefer_application_display_alias
            && matches!(
                self.interner.lookup(display_origin),
                Some(TypeData::Application(_))
            )
            && display_provenance::display_alias(self.interner, result).is_some();
        if self.should_record_application_alias(
            result,
            display_origin,
            skip_type_alias_repaint,
            keep_existing_conditional_branch_alias,
        ) {
            let priority = if prefer_application_display_alias
                || (self.expand_application_display_alias_args
                    && matches!(
                        self.interner.lookup(display_origin),
                        Some(TypeData::Application(_))
                    )) {
                AliasApplicationPriority::PreferApplication
            } else {
                AliasApplicationPriority::PreserveExisting
            };
            display_provenance::record_alias_application(
                self.interner,
                AliasApplicationProvenance {
                    evaluated: result,
                    application: display_origin,
                },
                priority,
            );
        }

        // If the conditional branch resolved to an intermediate
        // Application (e.g. `DeepReadonly<Part>` -> conditional ->
        // `DeepReadonlyObject<Part>`), store a forward display alias so
        // the formatter shows the one-step apparent type name that tsc
        // displays.
        if let Some(branch_app) = my_apparent_branch
            && branch_app != original_type_id
            && branch_app != result
            && !has_param_args
            && matches!(
                self.interner.lookup(branch_app),
                Some(crate::types::TypeData::Application(_))
            )
        {
            display_provenance::record_alias_application(
                self.interner,
                AliasApplicationProvenance {
                    evaluated: original_type_id,
                    application: branch_app,
                },
                AliasApplicationPriority::PreserveExisting,
            );
        }
    }

    pub(super) fn store_intermediate_application_display_alias(
        &self,
        instantiated: TypeId,
        original_type_id: TypeId,
        evaluated: TypeId,
        original_args: &[TypeId],
    ) {
        if instantiated == original_type_id || evaluated == TypeId::ERROR {
            return;
        }
        // Only install this forward alias when the intermediate application
        // appears to have been introduced after the outer application.
        // If the instantiated application predates the outer one, it can be a
        // user-authored type occurrence and globally aliasing it risks repainting
        // unrelated diagnostics.
        let instantiated_is_new_intermediate = match (
            self.interner.lookup_alloc_order(instantiated),
            self.interner.lookup_alloc_order(original_type_id),
        ) {
            (Some(instantiated_order), Some(original_order)) => instantiated_order > original_order,
            _ => instantiated.0 > original_type_id.0,
        };
        if !instantiated_is_new_intermediate {
            return;
        }
        let instantiated_is_application = matches!(
            self.interner.lookup(instantiated),
            Some(TypeData::Application(_))
        );
        let original_is_application = matches!(
            self.interner.lookup(original_type_id),
            Some(TypeData::Application(_))
        );

        if !original_is_application {
            return;
        }

        if !instantiated_is_application {
            // Structural-body path: the type alias body resolved to a structural
            // type rather than another Application (e.g.
            // `type LinkedList<T> = T & { next: LinkedList<T> }` evaluates to an
            // Intersection). Map `evaluated → original_type_id` so diagnostics show
            // the alias name instead of the expanded structural form.
            //
            // `evaluated_is_mapped` is checked first: Mapped is a subset of structural,
            // so true short-circuits the more expensive `is_structural_display_alias_result`
            // call and avoids a duplicate `lookup(evaluated)`.
            let evaluated_is_mapped =
                matches!(self.interner.lookup(evaluated), Some(TypeData::Mapped(_)));
            if evaluated_is_mapped
                || Self::is_structural_display_alias_result(self.interner, evaluated)
            {
                // Only store the display alias when `evaluated` was freshly produced
                // by this evaluation (allocated after `original_type_id`). If it
                // pre-exists, it was already interned by a different alias and
                // overwriting its alias would corrupt diagnostics for that other alias.
                // For example, `NestedRecord<"x.y.z", string>` and `Id<...string...>`
                // can evaluate to the same structural object; the NestedRecord evaluation
                // must not replace the `Id<...>` alias that was recorded first.
                let evaluated_is_fresh = match (
                    self.interner.lookup_alloc_order(evaluated),
                    self.interner.lookup_alloc_order(original_type_id),
                ) {
                    (Some(eval_order), Some(orig_order)) => eval_order > orig_order,
                    _ => evaluated.0 > original_type_id.0,
                };
                // Safe to store in two cases:
                // 1. Recursive aliases: the recursive self-reference ensures the structural
                //    type is unique to this instantiation, so aliasing is unambiguous.
                // 2. Generic aliases whose body evaluates to a fresh Mapped type: each
                //    distinct set of type-argument TypeIds produces a distinct MappedType
                //    node (the constraint is baked into the interned key). Storing the
                //    alias lets diagnostics show e.g. `Mapped2<K>` instead of the
                //    expanded `{ [P in K as \`get${P}\`]: ... }` form, matching tsc.
                if evaluated_is_fresh
                    && self.should_store_structural_display_alias(
                        evaluated,
                        original_type_id,
                        evaluated_is_mapped,
                    )
                {
                    self.interner
                        .store_display_alias_preferring_application(evaluated, original_type_id);
                }
            }
            return;
        }

        // Application→Application chain: when the outer application's args contain
        // generic type parameters, skip storing the alias. Intermediate Applications
        // in a type-alias chain (e.g. `Outer<T>` instantiated to `Inner<T>`) must not
        // displace the outer Application as the canonical display alias.
        if original_args.iter().any(|&arg| {
            crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
        }) {
            return;
        }

        if !Self::is_structural_display_alias_result(self.interner, evaluated) {
            return;
        }

        display_provenance::record_alias_application(
            self.interner,
            AliasApplicationProvenance {
                evaluated: instantiated,
                application: original_type_id,
            },
            AliasApplicationPriority::PreferApplication,
        );
    }
}

/// How one instantiation step that left the same alias still to be expanded
/// should be bounded — see `classify_same_alias_expansion`.
///
/// `def_depth` (`MAX_DEF_DEPTH`, escalated through
/// `REAL_INSTANTIATION_BAILOUT_THRESHOLD`) is the tsz analogue of tsc's
/// `instantiationDepth`: divergent self-recursive aliases must reach it (or an
/// equivalent depth verdict) so the checker can surface `TS2589`, while
/// convergent ones terminate on their own. A silent iteration bail from the
/// meta-rereduce identity guard at depth 5 would return the deferred
/// application with no limit verdict the checker maps to `TS2589` — losing
/// parity for shapes like
/// `type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T]`, where
/// the growth hides behind an object property feeding an index access rather
/// than sitting in direct tail position.
enum SameAliasExpansion {
    /// Direct tail-position growth or a non-conditional self-recursive
    /// wrapper: skip the identity guard and let `def_depth` own the bound.
    DefDepthOwned,
    /// A fully concrete conditional-bodied self-recursive alias at the
    /// identity ceiling: non-convergent growth whose conditional re-entry
    /// would otherwise terminate silently — report the depth verdict now.
    // Designed verdict not yet wired: `classify_same_alias_expansion` does not
    // return this yet; the `resolve_application` match already handles the arm.
    #[expect(dead_code)]
    DivergentConditional,
    /// Convergent or still-generic: keep the meta-rereduce identity wrapper.
    None,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;
    use crate::intern::TypeInterner;

    #[test]
    fn unresolved_def_body_blocks_concrete_fixpoint_application_cache_write() {
        let types = TypeInterner::new();
        let query_cache = QueryCache::new(&types);
        let mut evaluator = TypeEvaluator::new(&types).with_query_db(&query_cache);
        let def_id = DefId(901_001);
        let args = [TypeId::STRING];

        evaluator.app_body_limit_epoch = evaluator.limit_epoch;
        evaluator.app_body_unresolved_def_epoch = evaluator.unresolved_def_epoch;
        evaluator.mark_unresolved_def_seen();
        evaluator.insert_application_eval_cache_if_some(def_id, &args, false, TypeId::NEVER);

        assert_eq!(
            query_cache.lookup_application_eval_cache(def_id, &args, false),
            None,
            "a concrete-looking result computed after a registration-window unresolved def \
             must not enter the application_eval_cache",
        );
    }

    #[test]
    fn prior_unresolved_def_does_not_block_later_clean_application_cache_write() {
        let types = TypeInterner::new();
        let query_cache = QueryCache::new(&types);
        let mut evaluator = TypeEvaluator::new(&types).with_query_db(&query_cache);
        let def_id = DefId(901_002);
        let args = [TypeId::NUMBER];

        evaluator.mark_unresolved_def_seen();
        evaluator.app_body_limit_epoch = evaluator.limit_epoch;
        evaluator.app_body_unresolved_def_epoch = evaluator.unresolved_def_epoch;
        evaluator.insert_application_eval_cache_if_some(def_id, &args, false, TypeId::STRING);

        assert_eq!(
            query_cache.lookup_application_eval_cache(def_id, &args, false),
            Some(TypeId::STRING),
            "the unresolved-def epoch is per application body, not a sticky global \
             application_eval_cache disable",
        );
    }

    #[test]
    fn expanded_application_display_alias_containment_uses_shared_memo() {
        let types = TypeInterner::new();
        let prop = types.intern_string("x");
        let object = types.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);
        let key = types.literal_string("x");
        let index_access = types.index_access(object, key);
        let base = types.lazy(DefId(901_010));
        let original = types.application(base, vec![index_access]);
        let expanded_candidate = types.application(base, vec![TypeId::NUMBER]);

        let mut evaluator =
            TypeEvaluator::new(&types).with_expanded_application_display_alias_args();

        assert!(crate::visitor::contains_type_by_id(
            &types,
            expanded_candidate,
            TypeId::NUMBER
        ));
        assert_eq!(
            types.contains_type_by_id_memo(expanded_candidate, TypeId::NUMBER),
            None
        );

        evaluator.record_application_evaluation_display_aliases(
            TypeId::NUMBER,
            original,
            &[index_access],
            true,
            false,
            None,
        );

        assert_eq!(
            types.contains_type_by_id_memo(expanded_candidate, TypeId::NUMBER),
            Some(true)
        );
        assert!(
            TypeEvaluator::new(&types)
                .cached_contains_type_by_id(expanded_candidate, TypeId::NUMBER)
        );
        assert_eq!(
            types
                .type_predicate_cache_statistics()
                .contains_type_by_id_cache_entries,
            1
        );
    }
}
