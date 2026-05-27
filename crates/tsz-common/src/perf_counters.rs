//! Process-wide performance counters used to drive the perf-architectural
//! plan in `docs/plan/PERFORMANCE_PLAN.md`.
//!
//! Counters are gated by the `TSZ_PERF_COUNTERS` environment variable. When
//! the variable is unset the increments still fire (`AtomicU64::fetch_add`
//! is a single relaxed atomic op, which is well under a nanosecond), so we
//! could in principle just always count, but the env var also gates the
//! more expensive counters (per-shard lock-wait histograms, top-N largest
//! types, recomputation tracking) so production builds stay clean.
//!
//! Output is printed on demand via [`PerfCounters::dump`]. Drivers wire that
//! into `--extendedDiagnostics` (or `--perfCounters`) so a single bench
//! invocation produces both the standard phase timings and the counter dump.
//!
//! Per the architectural plan, this is a plan-changing PR — the data we
//! collect here decides how PRs 2–7 are scoped. Don't ship later PRs without
//! looking at the dump on `large-ts-repo` first.

include!("perf_counters/definitions.rs");
include!("perf_counters/runtime.rs");
include!("perf_counters/dump.rs");
include!("perf_counters/snapshot.rs");

#[cfg(test)]
include!("perf_counters/tests.rs");
