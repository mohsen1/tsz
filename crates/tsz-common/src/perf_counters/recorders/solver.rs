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
