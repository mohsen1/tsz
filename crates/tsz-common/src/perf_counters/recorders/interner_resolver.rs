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
