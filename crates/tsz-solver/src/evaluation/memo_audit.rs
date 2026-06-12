//! Measurement-only audit of cross-evaluator memo loss (issue #13097).
//!
//! Each `TypeEvaluator` owns a per-run `TypeId -> TypeId` memo that is
//! dropped with the evaluator. This module maintains a thread-local shadow
//! record of every *clean* (untainted) compute in the current file scope so
//! we can count how often a fresh evaluator recomputes a key an earlier
//! evaluator in the same file already computed — and whether the result
//! matched (a true memo loss) or differed (evidence the result is
//! context-dependent and must stay per-run).
//!
//! Everything here is gated on `TSZ_PERF_COUNTERS` via
//! [`tsz_common::perf_counters::enabled_fast`]; in normal runs the hooks are
//! a single branch and the shadow map is never touched. The audit never
//! feeds back into evaluation — it only increments perf counters.

use crate::types::TypeId;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use tsz_common::perf_counters;

/// Shadow record key: `(root TypeId, no_unchecked_indexed_access)`.
/// `exact_optional_property_types` is constant within one file scope, so it
/// is not part of the key.
type AuditKey = (TypeId, bool);

struct AuditState {
    /// Clean computes seen this file scope: key -> (result, evaluator id).
    entries: FxHashMap<AuditKey, (TypeId, u64)>,
    /// Monotonic evaluator id source for this thread.
    next_evaluator_id: u64,
}

thread_local! {
    static AUDIT: RefCell<AuditState> = RefCell::new(AuditState {
        entries: FxHashMap::default(),
        next_evaluator_id: 1,
    });
}

/// Begin a new file scope: forget all shadow entries. Called when a fresh
/// per-file `QueryCache` is constructed, mirroring the per-file lifetime of
/// the caches whose loss we are measuring. No-op unless counters are on.
pub(crate) fn begin_file_scope() {
    if !perf_counters::enabled_fast() {
        return;
    }
    AUDIT.with(|audit| audit.borrow_mut().entries.clear());
}

/// Allocate an id for a newly constructed evaluator. Returns 0 (unused)
/// when counters are off.
pub(crate) fn next_evaluator_id() -> u64 {
    if !perf_counters::enabled_fast() {
        return 0;
    }
    AUDIT.with(|audit| {
        let mut audit = audit.borrow_mut();
        let id = audit.next_evaluator_id;
        audit.next_evaluator_id += 1;
        id
    })
}

/// Record a clean (untainted) compute of `type_id -> result` by evaluator
/// `evaluator_id`. Counts a lost-memo recompute when a *different* evaluator
/// in the same file scope already computed the same key with the same
/// result, and a mismatch when the results differ.
pub(crate) fn record_clean_compute(
    type_id: TypeId,
    no_unchecked_indexed_access: bool,
    result: TypeId,
    evaluator_id: u64,
    context_tag: u8,
) {
    if !perf_counters::enabled_fast() {
        return;
    }
    AUDIT.with(|audit| {
        let mut audit = audit.borrow_mut();
        match audit.entries.entry((type_id, no_unchecked_indexed_access)) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let (prev_result, prev_evaluator) = *entry.get();
                if prev_evaluator != evaluator_id {
                    if prev_result == result {
                        perf_counters::record_eval_lost_memo_recompute();
                        perf_counters::record_eval_lost_memo_recompute_ctx(context_tag);
                        if result == type_id {
                            perf_counters::record_eval_lost_memo_recompute_identity();
                        }
                    } else {
                        perf_counters::record_eval_lost_memo_mismatch();
                    }
                }
                entry.insert((result, evaluator_id));
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert((result, evaluator_id));
            }
        }
    });
}
