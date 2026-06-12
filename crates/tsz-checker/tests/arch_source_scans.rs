//! Architecture source-scan ratchets, declared as an explicit `[[test]]`
//! integration target so the checker-integration CI lane (which enumerates
//! `cargo metadata` targets with `kind == "test"`) builds and runs them.
//!
//! These ratchets previously lived as `#[cfg(test)]` lib-test mounts in
//! `src/lib.rs`, but CI never builds the checker lib-test binary (it exceeds
//! what the `32 GiB` runners can link; see `run_checker_integration_tests`
//! in `scripts/ci/gcp-full-ci.sh`), so the invariants were silently
//! unenforced.
//!
//! Both scans are pure source-text checks over `$CARGO_MANIFEST_DIR/src` with
//! no dependency on crate internals:
//! - `relation_routing_residual_arch_tests`: diagnostic-bearing relation
//!   probes in production checker code must route through named
//!   `*_relation_outcome` helpers at the assignability boundary
//!   (issues #8227 / #12949).
//! - `common_boundary_export_ratchets`: the `query_boundaries/common.rs`
//!   `pub(crate) fn` surface only changes with an explicit allowlist update
//!   (issue #12948).

#[path = "arch_source_scans/common_boundary_export_ratchets.rs"]
mod common_boundary_export_ratchets;
#[path = "arch_source_scans/relation_routing_residual_arch_tests.rs"]
mod relation_routing_residual_arch_tests;
