//! Shared module-resolution primitives used across tsz crates.
//!
//! These are pure, dependency-light helpers (string + `serde_json` only) so
//! both the CLI driver resolver and the checker's resolution boundary can
//! share one implementation instead of maintaining divergent copies.

pub mod path_identity;
pub mod types_versions;
