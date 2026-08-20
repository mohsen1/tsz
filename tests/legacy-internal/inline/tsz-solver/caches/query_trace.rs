//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/caches/query_trace.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 83fb5fde4d36c93136997821792319778b12f7ddb03dec279ab4b09566adaab9 135 enabled_returns_false_without_subscriber
    #[test]
    fn enabled_returns_false_without_subscriber() {
        // Without a `tracing::subscriber::set_default(...)` call, the
        // `enabled!` macro returns false because no subscriber is
        // listening for `tsz::query_json`. This pins the cheap-fast-path
        // contract: in production, when no JSON-trace subscriber is
        // installed, `enabled()` short-circuits before any trace emission.
        assert!(!enabled());
    }
// TSZ_INLINE_TEST_END 83fb5fde4d36c93136997821792319778b12f7ddb03dec279ab4b09566adaab9

// TSZ_INLINE_TEST_BEGIN ae3d9228ff13d345114e2e5584d7f0fc7e7ca5f3387378f16ae54ff07ac2ee79 145 next_query_id_increments_monotonically
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
// TSZ_INLINE_TEST_END ae3d9228ff13d345114e2e5584d7f0fc7e7ca5f3387378f16ae54ff07ac2ee79

// TSZ_INLINE_TEST_BEGIN a892d4ee37dcf25bb860c1621d9b5fa138cf582a7ecf22640d6a246040b88349 158 next_query_id_is_thread_safe
    #[test]
    fn next_query_id_is_thread_safe() {
        // Concurrent increments from N threads must produce N distinct
        // values (Relaxed ordering on a single counter is sufficient for
        // uniqueness, which is the contract callers rely on).
        use crate::utils::MutexExt;
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
                collected
                    .lock_unpoisoned("query_trace.collected")
                    .extend(local);
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        let all = collected.lock_unpoisoned("query_trace.collected");
        let unique: HashSet<u64> = all.iter().copied().collect();
        assert_eq!(
            unique.len(),
            all.len(),
            "next_query_id produced duplicates across threads"
        );
    }
// TSZ_INLINE_TEST_END a892d4ee37dcf25bb860c1621d9b5fa138cf582a7ecf22640d6a246040b88349

// TSZ_INLINE_TEST_BEGIN 89b51a98fa6f245945532371b866abe741f1f215f9b50a4efbd7d7142a1d98dc 198 next_query_id_starts_at_one_or_higher
    #[test]
    fn next_query_id_starts_at_one_or_higher() {
        // `NEXT_QUERY_ID` is initialized to 1 and never decrements.
        let id = next_query_id();
        assert!(
            id >= 1,
            "first non-fetched query id should be >= 1, got {id}"
        );
    }
// TSZ_INLINE_TEST_END 89b51a98fa6f245945532371b866abe741f1f215f9b50a4efbd7d7142a1d98dc

// TSZ_INLINE_TEST_BEGIN 5255f11396ef9b5fd19b2c196012fbb2265fb79e685298d6a72bee8f2f25e20b 208 relation_cache_config_trace_fields_include_any_mode
    #[test]
    fn relation_cache_config_trace_fields_include_any_mode() {
        let config = crate::relations::relation_queries::RelationPolicy::from_relation_flags(
            crate::types::RelationFlags::STRICT_NULL_CHECKS,
        )
        .with_any_propagation_mode(crate::relations::subtype::AnyPropagationMode::TopLevelOnly)
        .cache_config();

        let fields = relation_cache_config_trace_fields(config);

        assert!(
            crate::types::RelationFlags::from_bits_retain(fields.flags)
                .contains(crate::types::RelationFlags::STRICT_NULL_CHECKS)
        );
        assert_eq!(fields.any_mode, "top_level_only_at_top");
    }
// TSZ_INLINE_TEST_END 5255f11396ef9b5fd19b2c196012fbb2265fb79e685298d6a72bee8f2f25e20b

// TSZ_INLINE_TEST_BEGIN d629adcf6e3f988bbc12de64c985e94561cdc8f2ab5b7e4d73afe5970abbad27 225 run_id_defaults_when_env_unset_or_default
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
// TSZ_INLINE_TEST_END d629adcf6e3f988bbc12de64c985e94561cdc8f2ab5b7e4d73afe5970abbad27
