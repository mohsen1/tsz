//! Lifetime-class shell types for checker context partitioning.
//!
//! These structs are intentionally **empty** in this initial pass. They exist
//! as named types that future workstreams will populate with fields migrated out
//! of `CheckerContext`. The empty types serve three immediate purposes:
//!
//! 1. **Reviewers can grep** for `WorkerContext` / `FileSession` /
//!    `SpeculationScope` / `LspPersistentCache` and see exactly where the
//!    architecture is heading, even before fields move.
//! 2. **The field-lifetime manifest** at
//!    `crates/tsz-checker/src/context/checker_context_lifetimes.toml` can
//!    eventually mark each entry's "destination shell" alongside its
//!    `lifetime` class. The shells make those destinations real types,
//!    not just doc-comment strings.
//! 3. **Future PRs are smaller and more reviewable.** Each follow-up PR can
//!    migrate a single bucket of fields into one of these shells. The structural
//!    target already exists.
//!
//! These shells implement no methods today. **Do not add behavior to them
//! until the next focused field-migration PR, where the addition is reviewed
//! alongside its accompanying behavior changes.
//!
//! Mapping from the partitioning plan:
//!
//! ```text
//! ProgramContext      — already exists
//! WorkerContext       — this file
//! FileSession         — this file
//! SpeculationScope    — this file
//! LspPersistentCache  — this file
//! ```

/// Worker-scoped reusable scratch state.
///
/// Future home for `WorkerReusable` fields per the partitioning plan:
///
/// > Owned by one worker and reusable across file sessions.
///
/// Examples that will eventually live here (none yet — see manifest):
///
/// - allocation pools / scratch buffers that don't carry file-local data
/// - per-worker counters and histograms that survive between files
/// - thread-local mirrors of `ProgramStable` shared structures
///
/// **Currently empty.** Populate when scoped worker ownership is introduced.
#[derive(Debug, Default)]
pub struct WorkerContext {
    // Reserved. See the module-level comment for the population policy.
    _reserved: (),
}

impl WorkerContext {
    /// Create an empty `WorkerContext`. Intentionally trivial in this initial
    /// pass; meaningful constructors land alongside the first field population.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: () }
    }
}

/// Per-file checking session.
///
/// Future home for `FileLocalReset` and `DiagnosticsOnly` fields per the
/// partitioning plan:
///
/// > Initialized for one file check and reset or dropped before the next file.
///
/// Examples that will eventually live here (per the manifest's 119
/// `FileLocalReset` entries and 21 `DiagnosticsOnly` entries):
///
/// - per-file caches keyed by `NodeIndex` (e.g. `request_node_types`)
/// - flow-analysis state (e.g. `flow_results`, `flow_visited`)
/// - resolution stacks/sets (e.g. `node_resolution_stack`)
/// - diagnostic accumulators (e.g. `diagnostics`, `emitted_diagnostics`)
///
/// **Currently empty.** Populate when sequential session-reuse support is
/// introduced.
#[derive(Debug, Default)]
pub struct FileSession {
    // Reserved. See the module-level comment for the population policy.
    _reserved: (),
}

impl FileSession {
    /// Create an empty `FileSession`.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: () }
    }
}

/// Speculative-overload save/restore scope.
///
/// Future home for `SpeculationScoped` fields per the partitioning plan:
///
/// > Must roll back when overload/generic/speculative checking aborts.
///
/// Examples that will eventually live here (per the manifest's 41
/// `SpeculationScoped` entries):
///
/// - depth counters (`call_depth`, `recursion_depth`, `instantiation_depth`)
/// - contextual flags (`contextual_type`, `is_checking_statements`)
/// - return-type / yield-type / this-type stacks
///
/// **Currently empty.** Populate when speculative rollback state is introduced.
#[derive(Debug, Default)]
pub struct SpeculationScope {
    // Reserved. See the module-level comment for the population policy.
    _reserved: (),
}

impl SpeculationScope {
    /// Create an empty `SpeculationScope`.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: () }
    }
}

/// LSP-persistent cache that survives across requests.
///
/// Future home for `LspPersistent` fields per the partitioning plan:
///
/// > Survives requests and is invalidated by document/project version.
///
/// The `CheckerContext` manifest currently has zero `LspPersistent` entries.
/// This shell exists so a future LSP-driver PR can introduce them without
/// having to also introduce the type at the same time.
///
/// **Currently empty.** Populate via a future LSP-side PR.
#[derive(Debug, Default)]
pub struct LspPersistentCache {
    // Reserved. See the module-level comment for the population policy.
    _reserved: (),
}

impl LspPersistentCache {
    /// Create an empty `LspPersistentCache`.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: () }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shells should be `Default` so future field migrations can wire them up
    /// via `Default::default()` without bespoke constructor plumbing.
    #[test]
    fn shells_implement_default() {
        let _ = WorkerContext::default();
        let _ = FileSession::default();
        let _ = SpeculationScope::default();
        let _ = LspPersistentCache::default();
    }

    /// `const fn new()` returns the same logical shape as `Default::default()`
    /// — verifies that const-construction is wired up for compile-time
    /// initialization (future consumers may need this for static
    /// scratch).
    #[test]
    fn shells_can_be_constructed_const() {
        const _W: WorkerContext = WorkerContext::new();
        const _F: FileSession = FileSession::new();
        const _S: SpeculationScope = SpeculationScope::new();
        const _L: LspPersistentCache = LspPersistentCache::new();
    }
}
