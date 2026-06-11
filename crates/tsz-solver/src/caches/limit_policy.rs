//! Kill switch for the limit-hit result caching policy (issue #13241).
//!
//! When a relation or evaluation chain hits a recursion/depth/fuel limit,
//! `tsc` records `Ternary.Maybe` outcomes (its `maybeKeys` stack) and promotes
//! them to cached successes once the outermost relation completes
//! successfully. `tsz` mirrors that policy with:
//!
//! - the maybe-stack promotion in `relations::subtype::cache` (cycle-derived
//!   `Maybe` keys promoted to definitive `true`, fuel-derived `Maybe` keys
//!   promoted to band-conditional [`crate::types::RelationCacheValue::LimitTrue`]
//!   entries), and
//! - the per-intermediate taint discrimination in the evaluator
//!   (`TypeEvaluator` tainted set), which lets clean intermediate evaluation
//!   results persist even when an unrelated subtree hit a limit.
//!
//! `TSZ_DISABLE_LIMIT_RESULT_CACHE=1` restores the previous
//! drop-everything-on-limit-hit behavior for cache-on/off A/B verification.

use std::sync::OnceLock;

/// Whether limit-hit relation/eval outcomes may be recorded and reused.
///
/// Enabled by default; set `TSZ_DISABLE_LIMIT_RESULT_CACHE=1` to disable.
pub(crate) fn limit_result_cache_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| !std::env::var("TSZ_DISABLE_LIMIT_RESULT_CACHE").is_ok_and(|v| v == "1"))
}
