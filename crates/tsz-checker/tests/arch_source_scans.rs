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
//! All scans are pure source-text checks over `$CARGO_MANIFEST_DIR/src` with
//! no dependency on crate internals:
//! - `relation_routing_residual_arch_tests`: diagnostic-bearing relation
//!   probes in production checker code must route through named
//!   `*_relation_outcome` helpers at the assignability boundary
//!   (issues #8227 / #12949).
//! - `common_boundary_export_ratchets`: the `query_boundaries/common.rs`
//!   `pub(crate) fn` surface only changes with an explicit allowlist update
//!   (issue #12948).
//! - `construction_boundary_signature_scans`: the issue #13022 module set
//!   constructs signature-bearing solver types only through
//!   `query_boundaries::construct_signatures`, never via inline shape
//!   literals or direct interning calls.

#[path = "arch_source_scans/common_boundary_export_ratchets.rs"]
mod common_boundary_export_ratchets;
#[path = "arch_source_scans/construction_boundary_signature_scans.rs"]
mod construction_boundary_signature_scans;
#[path = "arch_source_scans/relation_routing_residual_arch_tests.rs"]
mod relation_routing_residual_arch_tests;
#[path = "arch_source_scans/spelling_suggestion_gateway_scans.rs"]
mod spelling_suggestion_gateway_scans;
