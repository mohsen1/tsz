// ─── interner locality instrumentation (issue #13246) ───────────────────
//
// The per-instance TLS direct-mapped caches in
// `crates/tsz-solver/src/intern/core/interner/cache.rs` are sized to hold a
// file's hot working set. When a file's live distinct-`TypeId` set exceeds the
// 1024-slot lookup cache (or 512-slot intern cache), inserts collide and evict
// live entries, and subsequent probes miss into the cold sharded
// `RwLock<Vec<TypeData>>` at ~15-25 ns/lookup. The helpers below quantify that
// thrash so the `O(files^1.7)` per-file slope can be attributed to locality
// decay (or ruled out). Every helper short-circuits on `enabled_fast()`, so
// default builds (env var unset) pay only the gate load.

/// TLS lookup-cache slot count mirror, used to classify a file's working set
/// as "over cache" without reaching into the solver crate. Must track
/// `LOOKUP_CACHE_SIZE` in
/// `crates/tsz-solver/src/intern/core/interner/cache.rs`; the per-file
/// sampler only uses it for a coarse threshold, so a stale value degrades the
/// classification but never affects correctness.
pub const INTERNER_TLS_LOOKUP_CACHE_SLOTS: u64 = 1024;

/// Record one `lookup()` entry past the intrinsic/error short-circuit, with
/// its TLS-cache outcome. `tls_hit == true` means the TLS direct-mapped probe
/// served the result; `false` means it fell through to the cold sharded
/// `RwLock<Vec<TypeData>>`.
#[inline]
pub fn record_interner_lookup(tls_hit: bool) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.interner_lookup_calls.fetch_add(1, Ordering::Relaxed);
    if tls_hit {
        c.interner_lookup_tls_hits.fetch_add(1, Ordering::Relaxed);
    } else {
        c.interner_lookup_cold_vec_fallbacks
            .fetch_add(1, Ordering::Relaxed);
    }
}

/// Record a TLS lookup-cache insert that overwrote a live entry belonging to a
/// different `TypeId` (a direct-mapped collision / eviction). The eviction
/// rate is the working-set-exceeds-cache thrash signal on the lookup side.
#[inline]
pub fn record_interner_lookup_tls_eviction() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_lookup_tls_evictions
        .fetch_add(1, Ordering::Relaxed);
}

/// Record an `intern()` TLS-cache outcome on the structural-key path (after
/// the intrinsic short-circuit). `tls_hit == true` means the TLS intern probe
/// served the id; `false` means it ran the `DashMap`/shard slow path.
#[inline]
pub fn record_interner_intern_tls_outcome(tls_hit: bool) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    if tls_hit {
        c.interner_intern_tls_hits.fetch_add(1, Ordering::Relaxed);
    } else {
        c.interner_intern_cold_fallbacks
            .fetch_add(1, Ordering::Relaxed);
    }
}

/// Record a TLS intern-cache insert that overwrote a live entry for a
/// different hash (a direct-mapped collision / eviction). The intern-side
/// thrash signal.
#[inline]
pub fn record_interner_intern_tls_eviction() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_intern_tls_evictions
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one promoted-tier probe consultation (`TSZ_PROMOTE_FIRST`). `hit`
/// distinguishes a stable-hot-set serve from a fall-through to the normal TLS
/// path. Measurement-only; never fires when the probe is off.
#[inline]
pub fn record_interner_promote_tier_probe(hit: bool) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    if hit {
        c.interner_promote_tier_hits.fetch_add(1, Ordering::Relaxed);
    } else {
        c.interner_promote_tier_misses
            .fetch_add(1, Ordering::Relaxed);
    }
}

// Per-file distinct-`TypeId` working-set sampler. A thread-local set tracks the
// distinct ids a file touched; the CLI snapshots and clears it at each
// `check_source_file` boundary via [`record_interner_working_set_for_file`].
// Only mutated when counters are enabled, so default builds never allocate.
thread_local! {
    static WORKING_SET_IDS: std::cell::RefCell<rustc_hash::FxHashSet<u32>> =
        std::cell::RefCell::new(rustc_hash::FxHashSet::default());
}

/// Note that a `TypeId` (raw u32) was touched by the current file's
/// `lookup`/`intern` activity. Cheap insert into a thread-local set; the
/// distinct count is read and reset at the file boundary. Gated, so disabled
/// builds skip the set entirely.
#[inline]
pub fn note_interner_working_set_id(raw_id: u32) {
    if !enabled_fast() {
        return;
    }
    WORKING_SET_IDS.with(|set| {
        set.borrow_mut().insert(raw_id);
    });
}

/// Snapshot and reset the current thread's per-file distinct working set,
/// folding the result into the run-wide max / total / over-cache buckets.
/// Called at each `check_source_file` boundary. Returns the distinct count
/// observed for the file (for optional tracing), or 0 when disabled.
pub fn record_interner_working_set_for_file() -> u64 {
    if !enabled_fast() {
        return 0;
    }
    let distinct = WORKING_SET_IDS.with(|set| {
        let mut set = set.borrow_mut();
        let n = set.len() as u64;
        set.clear();
        n
    });
    if distinct == 0 {
        return 0;
    }
    let c = counters();
    c.interner_working_set_files_sampled
        .fetch_add(1, Ordering::Relaxed);
    c.interner_working_set_distinct_total
        .fetch_add(distinct, Ordering::Relaxed);
    record_max_inner(&c.interner_working_set_distinct_max, distinct);
    if distinct > INTERNER_TLS_LOOKUP_CACHE_SLOTS {
        c.interner_working_set_files_over_cache
            .fetch_add(1, Ordering::Relaxed);
    }
    distinct
}

/// Whether the opt-in promote-first interner probe is enabled
/// (`TSZ_PROMOTE_FIRST` set). Default OFF: the interner consults its normal
/// per-instance TLS cache + sharded storage exactly as before. When ON, the
/// `lookup`/`intern` hot paths additionally probe a process-global promoted
/// tier of stable hot types (intrinsics + lib ids) first, and the
/// `interner_promote_tier_*` counters measure whether that raises the hit rate
/// and cuts the cold-Vec fallback. No semantic change: the promoted tier only
/// ever holds ids already resolvable through the normal path, so the answer is
/// identical; only the lookup *order* changes. Latched once via `OnceLock`,
/// mirroring [`enabled_fast`], so the hot-path read is one branch + one load.
#[inline(always)]
pub fn promote_first_enabled() -> bool {
    static PROMOTE_FIRST: OnceLock<bool> = OnceLock::new();
    *PROMOTE_FIRST.get_or_init(|| std::env::var_os("TSZ_PROMOTE_FIRST").is_some())
}

/// Record a `TypeInterner::intern_string` call. Mirrors the existing
/// `record_compute_type_of_symbol_*` shape: gate once, one `counters()`
/// lookup, increment exactly the named field.
///
/// `intern_string` is a fundamental hot path — every property name,
/// every string literal, every diagnostic message tag eventually flows
/// through it. The wrapper keeps that path cheap when counters are
/// disabled (one `OnceLock<bool>` read, no `OnceLock<PerfCounters>`
/// deref) without spreading the gate/deref pair across every interner
/// entry point.
#[inline]
pub fn record_interner_string_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_string_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_string` call that was served from the
/// thread-local string cache (no shard `RwLock`, no `ShardedInterner::intern`).
#[inline]
pub fn record_interner_string_intern_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_string_intern_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_type_list` call (covers both the
/// owning `Vec` entry point and the borrowed-slice entry point).
#[inline]
pub fn record_interner_type_list_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_type_list_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_object_shape` call.
#[inline]
pub fn record_interner_object_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_object_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_function_shape` call.
#[inline]
pub fn record_interner_function_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_function_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_conditional_type` call.
#[inline]
pub fn record_interner_conditional_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_conditional_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_mapped_type` call.
#[inline]
pub fn record_interner_mapped_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_mapped_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_callable_shape` call. Mirrors the
/// sibling `record_interner_function_shape_intern_call` shape — gate
/// once, one `counters()` lookup, increment the named field.
#[inline]
pub fn record_interner_callable_shape_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_callable_shape_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a `TypeInterner::intern_application` call.
#[inline]
pub fn record_interner_application_intern_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .interner_application_intern_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `ModuleResolver::lookup()` call from
/// `crates/tsz-cli/src/driver/sources.rs` — the entry point for per-import
/// module resolution. Sibling to the fs-probe `record_resolver_*`
/// helpers but lives in a different file (sources.rs vs resolution.rs)
/// because resolution caching happens at the lookup level, above the
/// individual fs-probe wrappers.
#[inline]
pub fn record_resolver_lookup_call() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_lookup_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `Path::is_file()` probe from the resolver fast path.
/// Used by the `count_is_file` wrapper in `crates/tsz-cli/src/driver/resolution.rs`,
/// which bundles the syscall and the counter in one place. Gate once,
/// deref `counters()` once, increment.
#[inline]
pub fn record_resolver_is_file() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_is_file_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `Path::is_dir()` probe from the resolver fast path.
/// Sibling to [`record_resolver_is_file`].
#[inline]
pub fn record_resolver_is_dir() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_is_dir_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one `std::fs::read_dir()` call from the resolver. Sibling to
/// [`record_resolver_is_file`]. The cost of the syscall itself dwarfs
/// the counter overhead — this helper is only structural cleanup.
#[inline]
pub fn record_resolver_read_dir() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_read_dir_calls
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one candidate path examined during module resolution
/// (path-mapping virtual roots and suffix-extension expansion).
/// Lifted into a helper so the two emit sites in
/// `crates/tsz-cli/src/driver/resolution.rs` don't re-pay the `counters()`
/// `OnceLock` deref.
#[inline]
pub fn record_resolver_candidate_path() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_candidate_paths_total
        .fetch_add(1, Ordering::Relaxed);
}

/// Record one uncached `package.json` read. Sits inside the resolver's
/// `read_package_json_uncached`, which `large-ts-repo` profiles flag
/// as the dominant resolver work — keeping the gate cheap matters even
/// though the surrounding `read_to_string` is several orders of
/// magnitude more expensive.
#[inline]
pub fn record_resolver_read_package_json() {
    if !enabled_fast() {
        return;
    }
    counters()
        .resolver_read_package_json_calls
        .fetch_add(1, Ordering::Relaxed);
}
