/// #14344 identity-collision observability. Record that the
/// `raw_symbol_fallback_def` `#13862` guard suppressed a *genuine* content
/// collision: a store-registered `DefId(N)` whose raw value `N`, reread as a
/// `SymbolId`, would resolve to a DIFFERENT def whose canonical decl name
/// differs from `DefId(N)`'s own (the `HTMLDivElement` -> `FileSystemEntry`
/// class). Callers MUST establish content-difference before calling — mere
/// raw-`u32` overlap is ~100% by construction and must never be counted here.
/// Measurement only: never feeds back into resolution (the guard already
/// returns `None`).
#[inline]
pub fn record_identity_collision_wrong_decl_suppressed() {
    if !enabled_fast() {
        return;
    }
    counters()
        .identity_collision_wrong_decl_suppressed
        .fetch_add(1, Ordering::Relaxed);
}

/// #14344 denominator context. Record a `symbol_def_index` composite-key
/// `(symbol, file)` resolution attempt, partitioned by whether it hit.
#[inline]
pub fn record_symbol_def_index_lookup(hit: bool) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    if hit {
        c.symbol_def_index_lookup_hits
            .fetch_add(1, Ordering::Relaxed);
    } else {
        c.symbol_def_index_lookup_misses
            .fetch_add(1, Ordering::Relaxed);
    }
}

/// #14351 relation-hot-path gating measurement. Record one `check_subtype`
/// `Application`<->`Application` pair that reached the pre-evaluation variance
/// fast path, partitioned by whether the variance fast path could NOT decide
/// (`fell_through` => the relation eagerly `evaluate_type`-expands both members,
/// `cache.rs` ~1161) and, among those, whether the two `Application` BASES
/// differ (`cross_base` => the cross-base HKT pattern `Kind<F,A>` vs
/// `HKT<F,B>`). Measurement only: the relation verdict is unchanged whether or
/// not this fires. `cross_base` is only meaningful when `fell_through`.
#[inline]
pub fn record_relation_app_pair(fell_through: bool, cross_base: bool) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.relation_app_pair_total.fetch_add(1, Ordering::Relaxed);
    if fell_through {
        c.relation_app_pair_variance_fallthrough
            .fetch_add(1, Ordering::Relaxed);
        if cross_base {
            c.relation_app_pair_variance_fallthrough_cross_base
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Record a budget-conditional `LimitTrue` relation cache hit (a limit-hit
/// relation verdict was reused instead of re-burning the relation chain).
#[inline]
pub fn record_relation_limit_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .relation_limit_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one maybe-stack key promoted into the relation cache at outermost
/// relation success (tsc `maybeKeys` promotion parity).
#[inline]
pub fn record_relation_maybe_promotion() {
    if !enabled_fast() {
        return;
    }
    counters()
        .relation_maybe_promotions
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_application_eval_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_application_eval_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_application_eval_cache_miss() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_application_eval_cache_misses
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_application_eval_cache_insert() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_application_eval_cache_inserts
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_application_eval_cache_bypass() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_application_eval_cache_bypasses
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_instantiation_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_instantiation_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_instantiation_cache_miss() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_instantiation_cache_misses
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_instantiation_cache_insert() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_instantiation_cache_inserts
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_shared_instantiation_cache_bypass() {
    if !enabled_fast() {
        return;
    }
    counters()
        .shared_instantiation_cache_bypasses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one failure-reason walk over a failing reason-collecting
/// assignability relation (issue #13243).
#[inline]
pub fn record_relation_failure_reason_walk() {
    if !enabled_fast() {
        return;
    }
    counters()
        .relation_failure_reason_walks
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one failing relation analysis served from the checker's
/// stamp-guarded failure-analysis memo (issue #13243).
#[inline]
pub fn record_relation_failure_memo_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .relation_failure_memo_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one weak-type/weak-union probe executed while collecting a failure
/// reason (issue #13243). `count` is the number of probes run by the call site
/// (1 for the single-pass `analyze_weak_and_explain` path; the legacy
/// double-probe paths recorded 2).
#[inline]
pub fn record_relation_weak_violation_probes(count: u64) {
    if !enabled_fast() {
        return;
    }
    counters()
        .relation_weak_violation_probes
        .fetch_add(count, Ordering::Relaxed);
}

/// Record one concrete union-subtype reduction attempt.
#[inline]
pub fn record_union_subtype_reduction(member_count: u64, pairwise_budget: u64) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.union_subtype_reduction_calls
        .fetch_add(1, Ordering::Relaxed);
    c.union_subtype_reduction_members_total
        .fetch_add(member_count, Ordering::Relaxed);
    c.union_subtype_reduction_pairwise_budget_total
        .fetch_add(pairwise_budget, Ordering::Relaxed);
    record_max(&c.union_subtype_reduction_members_max, member_count);
}

/// Record a concrete object/callable property-instantiation walk.
#[inline]
pub fn record_property_instantiation_walk(property_count: u64, changed: bool) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.property_instantiation_walks
        .fetch_add(1, Ordering::Relaxed);
    c.property_instantiation_properties_total
        .fetch_add(property_count, Ordering::Relaxed);
    record_max(&c.property_instantiation_properties_max, property_count);
    if changed {
        c.property_instantiation_changed
            .fetch_add(1, Ordering::Relaxed);
    }
}

/// Record a `TypeEvaluator` construction (issue #13097 memo-lifecycle audit).
#[inline]
pub fn record_eval_evaluator_construction() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_evaluator_constructions
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a hit on an evaluator's own per-run memo.
#[inline]
pub fn record_eval_local_memo_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_local_memo_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a node compute that passed every memo/cache layer.
#[inline]
pub fn record_eval_compute_node() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_compute_nodes
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a clean compute that an earlier same-file evaluator already
/// produced (same key, same result) but discarded with its memo.
#[inline]
pub fn record_eval_lost_memo_recompute() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_lost_memo_recomputes
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a nested `lookup_eval_memo` hit inside an evaluator.
#[inline]
pub fn record_eval_memo_nested_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_memo_nested_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a lost-memo recompute attributed to an evaluator context class:
/// 0 = plain memo-reading, 1 = authoritative checker pass, 2 = other.
#[inline]
pub fn record_eval_lost_memo_recompute_ctx(ctx: u8) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    let field = match ctx {
        0 => &c.eval_lost_memo_recomputes_plain,
        1 => &c.eval_lost_memo_recomputes_authoritative,
        _ => &c.eval_lost_memo_recomputes_other,
    };
    field.fetch_add(1, Ordering::Relaxed);
}

/// Record a lost-memo recompute whose result was the input itself.
#[inline]
pub fn record_eval_lost_memo_recompute_identity() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_lost_memo_recomputes_identity
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a same-key clean compute whose result differed across evaluators.
#[inline]
pub fn record_eval_lost_memo_mismatch() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_lost_memo_mismatches
        .fetch_add(1, Ordering::Relaxed);
}

/// Record memo entries discarded (not drained) when an evaluator dropped.
#[inline]
pub fn record_eval_dropped_memo_entries(count: u64) {
    if count == 0 || !enabled_fast() {
        return;
    }
    counters()
        .eval_dropped_memo_entries
        .fetch_add(count, Ordering::Relaxed);
}

/// Record auxiliary memo entries (conditional-subtype / contains-infer)
/// discarded when an evaluator dropped.
#[inline]
pub fn record_eval_dropped_aux_entries(count: u64) {
    if count == 0 || !enabled_fast() {
        return;
    }
    counters()
        .eval_dropped_aux_entries
        .fetch_add(count, Ordering::Relaxed);
}

/// Record a cross-evaluator conditional-branch verdict cache hit (issues
/// #8356 / #13097): a `check <: extends` branch probe served from the
/// project-wide cache instead of a fresh structural walk.
#[inline]
pub fn record_eval_conditional_verdict_persist_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_conditional_verdict_persist_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a definitive conditional-branch verdict published to the
/// project-wide cache (it passed every limit/registration-window gate).
#[inline]
pub fn record_eval_conditional_verdict_persist_insert() {
    if !enabled_fast() {
        return;
    }
    counters()
        .eval_conditional_verdict_persist_inserts
        .fetch_add(1, Ordering::Relaxed);
}

/// Record which guard cut a `TypeEvaluator::evaluate` walk short (#14346).
///
/// The firing-order signal the issue flags: which bound a runaway recursive
/// walk hits first. Measurement only — the evaluator's bail outcome (the
/// returned `TypeId`) is unchanged whether or not this fires. Zero atomic
/// traffic and a single predictable branch when `TSZ_PERF_COUNTERS` is off.
#[inline]
pub fn record_eval_termination_guard(guard: EvaluationTerminationGuard) {
    if !enabled_fast() {
        return;
    }
    counters().eval_termination_guard_fires[guard.as_index()].fetch_add(1, Ordering::Relaxed);
}
