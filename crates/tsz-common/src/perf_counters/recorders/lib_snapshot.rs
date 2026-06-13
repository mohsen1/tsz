pub fn record_lib_snapshot_set_load(file_count: u64, hit: bool, elapsed_ns: u64) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.lib_snapshot_set_load_attempts
        .fetch_add(1, Ordering::Relaxed);
    if hit {
        c.lib_snapshot_set_load_hits
            .fetch_add(1, Ordering::Relaxed);
    } else {
        c.lib_snapshot_set_load_misses
            .fetch_add(1, Ordering::Relaxed);
    }
    c.lib_snapshot_set_load_files_total
        .fetch_add(file_count, Ordering::Relaxed);
    c.lib_snapshot_set_load_elapsed_ns_total
        .fetch_add(elapsed_ns, Ordering::Relaxed);
    record_max(&c.lib_snapshot_set_load_elapsed_ns_max, elapsed_ns);
}
