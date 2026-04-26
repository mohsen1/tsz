//! Structured query tracing for solver query entry points.
//!
//! Events use target `tsz::query_json` and are intended to be consumed with:
//! `TSZ_LOG=tsz::query_json=trace TSZ_LOG_FORMAT=json`.
//!
//! Environment:
//! - `TSZ_QUERY_RUN_ID`: optional run identifier attached to every event.

use crate::TypeId;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{Level, trace};

static NEXT_QUERY_ID: AtomicU64 = AtomicU64::new(1);
static QUERY_RUN_ID: OnceLock<String> = OnceLock::new();

#[inline]
pub(crate) fn enabled() -> bool {
    tracing::enabled!(target: "tsz::query_json", Level::TRACE)
}

#[inline]
pub(crate) fn next_query_id() -> u64 {
    NEXT_QUERY_ID.fetch_add(1, Ordering::Relaxed)
}

#[inline]
fn run_id() -> &'static str {
    QUERY_RUN_ID
        .get_or_init(|| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::env::var("TSZ_QUERY_RUN_ID").unwrap_or_else(|_| "default".to_string())
            }
            #[cfg(target_arch = "wasm32")]
            {
                "default".to_string()
            }
        })
        .as_str()
}

#[inline]
pub(crate) fn unary_start(query_id: u64, op: &'static str, input: TypeId, no_unchecked: bool) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "start",
        run_id = run_id(),
        query_id,
        op,
        input_type_id = input.0,
        no_unchecked_indexed_access = no_unchecked
    );
}

#[inline]
pub(crate) fn unary_end(query_id: u64, op: &'static str, result_type: TypeId, cache_hit: bool) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "end",
        run_id = run_id(),
        query_id,
        op,
        result_type_id = result_type.0,
        cache_hit
    );
}

#[inline]
pub(crate) fn relation_start(
    query_id: u64,
    op: &'static str,
    source: TypeId,
    target: TypeId,
    flags: u16,
) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "start",
        run_id = run_id(),
        query_id,
        op,
        source_type_id = source.0,
        target_type_id = target.0,
        flags
    );
}

#[inline]
pub(crate) fn relation_end(query_id: u64, op: &'static str, result: bool, cache_hit: bool) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "end",
        run_id = run_id(),
        query_id,
        op,
        result,
        cache_hit
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_returns_false_without_subscriber() {
        // Without a `tracing::subscriber::set_default(...)` call, the
        // `enabled!` macro returns false because no subscriber is
        // listening for `tsz::query_json`. This pins the cheap-fast-path
        // contract: in production, when no JSON-trace subscriber is
        // installed, `enabled()` short-circuits before any trace emission.
        assert!(!enabled());
    }

    #[test]
    fn next_query_id_increments_monotonically() {
        // NEXT_QUERY_ID is a process-level static AtomicU64 shared across the
        // whole test binary. Other tests running in parallel can interleave
        // increments between our calls, so we can only assert strict monotonic
        // ordering here, not consecutive values (`b == a + 1` would be flaky).
        let a = next_query_id();
        let b = next_query_id();
        let c = next_query_id();
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn next_query_id_is_thread_safe() {
        // Concurrent increments from N threads must produce N distinct
        // values (Relaxed ordering on a single counter is sufficient for
        // uniqueness, which is the contract callers rely on).
        use std::collections::HashSet;
        use std::sync::Arc;
        use std::sync::Mutex;
        use std::thread;

        const THREADS: usize = 8;
        const PER_THREAD: usize = 32;

        let collected: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..THREADS {
            let collected = Arc::clone(&collected);
            handles.push(thread::spawn(move || {
                let mut local = Vec::with_capacity(PER_THREAD);
                for _ in 0..PER_THREAD {
                    local.push(next_query_id());
                }
                collected.lock().expect("lock poisoned").extend(local);
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        let all = collected.lock().expect("lock poisoned");
        let unique: HashSet<u64> = all.iter().copied().collect();
        assert_eq!(
            unique.len(),
            all.len(),
            "next_query_id produced duplicates across threads"
        );
    }

    #[test]
    fn next_query_id_starts_at_one_or_higher() {
        // `NEXT_QUERY_ID` is initialized to 1 and never decrements.
        let id = next_query_id();
        assert!(
            id >= 1,
            "first non-fetched query id should be >= 1, got {id}"
        );
    }

    #[test]
    fn run_id_defaults_when_env_unset_or_default() {
        // `run_id()` reads `TSZ_QUERY_RUN_ID` once via `OnceLock` (or
        // hard-codes "default" on wasm). Whatever the test environment
        // sets, the returned slice must be non-empty and stable across
        // calls (cached in the OnceLock).
        let r1 = run_id();
        let r2 = run_id();
        assert!(!r1.is_empty(), "run_id should never be empty");
        assert_eq!(
            r1, r2,
            "run_id must be stable across calls (OnceLock cached)"
        );
    }
}
