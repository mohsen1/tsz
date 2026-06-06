use std::path::Path;

// Counting filesystem probes (`PERFORMANCE_PLAN.md` §4.T0.3 follow-up).
// Keep the counter bump and underlying syscall bundled so resolver call sites
// stay one token away from a future `CountingFs` trait.
#[inline]
pub(super) fn count_is_file(path: &Path) -> bool {
    tsz_common::perf_counters::record_resolver_is_file();
    path.is_file()
}

#[inline]
pub(super) fn count_is_dir(path: &Path) -> bool {
    tsz_common::perf_counters::record_resolver_is_dir();
    path.is_dir()
}

#[inline]
pub(super) fn count_read_dir(path: &Path) -> std::io::Result<std::fs::ReadDir> {
    tsz_common::perf_counters::record_resolver_read_dir();
    std::fs::read_dir(path)
}

#[inline]
pub(super) fn count_candidate_path() {
    tsz_common::perf_counters::record_resolver_candidate_path();
}

#[inline]
pub(super) fn count_read_package_json() {
    tsz_common::perf_counters::record_resolver_read_package_json();
}
