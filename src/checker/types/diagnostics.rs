//! Re-export diagnostics from tsz-common.
//!
//! The canonical definition lives in `tsz_common::diagnostics`.
//! This module re-exports everything so existing `crate::checker::types::diagnostics` paths
//! continue to work.

pub use tsz_common::diagnostics::*;
