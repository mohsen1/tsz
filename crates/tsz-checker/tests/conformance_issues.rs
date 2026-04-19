//! Unit tests documenting known conformance test failures.
//!
//! These tests are marked `#[ignore]` and document specific issues found during
//! conformance investigation.

#[path = "conformance_issues/core/mod.rs"]
mod core;
#[path = "conformance_issues/errors/mod.rs"]
mod errors;
#[path = "conformance_issues/features/mod.rs"]
mod features;
#[path = "conformance_issues/modules/mod.rs"]
mod modules;
#[path = "conformance_issues/types/mod.rs"]
mod types;
