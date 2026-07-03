//! Architecture source-scan ratchets, declared as an explicit `[[test]]`
//! integration target so the checker-integration CI lane (which enumerates
//! `cargo metadata` targets with `kind == "test"`) builds and runs them.
//!
//! These ratchets previously lived as `#[cfg(test)]` lib-test mounts in
//! `src/lib.rs`, but CI never builds the checker lib-test binary (it exceeds
//! what the `32 GiB` runners can link; see `run_checker_integration_tests`
//! in `scripts/ci/full-ci.sh`), so the invariants were silently
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
//! - `diagnostic_construction_boundary_scans`: diagnostic reporters route
//!   display-only solver shape construction through
//!   `query_boundaries::diagnostics`.
//! - `class_instance_walk_state_scans`: class instance base traversal uses a
//!   named checker-owned walk state instead of paired raw visited sets.
//! - `cross_arena_delegation_scope_scans`: cross-arena delegation depth uses a
//!   scoped guard instead of manual enter/leave pairs.
//! - `index_signature_boundary_scans`: production checker index-signature
//!   queries go through `query_boundaries::index_signature` rather than
//!   constructing the raw solver resolver at call sites.
//! - `jsdoc_construction_boundary_scans`: JSDoc type-resolution callers route
//!   solver shape construction through `query_boundaries::jsdoc_construction`.
//! - `jsx_construction_boundary_scans`: JSX checker callers route object and
//!   function shape construction through `query_boundaries::checkers::jsx`.

#[path = "arch_source_scans/class_instance_walk_state_scans.rs"]
mod class_instance_walk_state_scans;
#[path = "arch_source_scans/common_boundary_export_ratchets.rs"]
mod common_boundary_export_ratchets;
#[path = "arch_source_scans/construction_boundary_signature_scans.rs"]
mod construction_boundary_signature_scans;
#[path = "arch_source_scans/cross_arena_delegation_scope_scans.rs"]
mod cross_arena_delegation_scope_scans;
#[path = "arch_source_scans/diagnostic_construction_boundary_scans.rs"]
mod diagnostic_construction_boundary_scans;
#[path = "arch_source_scans/index_signature_boundary_scans.rs"]
mod index_signature_boundary_scans;
#[path = "arch_source_scans/jsdoc_construction_boundary_scans.rs"]
mod jsdoc_construction_boundary_scans;
#[path = "arch_source_scans/jsx_construction_boundary_scans.rs"]
mod jsx_construction_boundary_scans;
#[path = "arch_source_scans/lazy_resolution_session_scans.rs"]
mod lazy_resolution_session_scans;
#[path = "arch_source_scans/object_flags_boundary_scans.rs"]
mod object_flags_boundary_scans;
#[path = "arch_source_scans/object_literal_annotation_walker_scans.rs"]
mod object_literal_annotation_walker_scans;
#[path = "arch_source_scans/relation_boundary_session_scans.rs"]
mod relation_boundary_session_scans;
#[path = "arch_source_scans/relation_routing_residual_arch_tests.rs"]
mod relation_routing_residual_arch_tests;
#[path = "arch_source_scans/spelling_suggestion_gateway_scans.rs"]
mod spelling_suggestion_gateway_scans;
#[path = "arch_source_scans/type_guard_walk_state.rs"]
mod type_guard_walk_state;
#[path = "arch_source_scans/type_reference_depth_session_scans.rs"]
mod type_reference_depth_session_scans;
