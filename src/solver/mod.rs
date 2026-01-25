//! Phase 7.5: Query-Based Structural Solver
//!
//! This module implements a declarative, query-based type solver architecture
//! that replaces the legacy imperative checker. It uses:
//!
//! - **Salsa**: For incremental recomputation and query memoization
//! - **Ena**: For unification (Union-Find) in generic type inference
//! - **Custom TypeKey**: Structural type representation with interning
//! - **Cycle Detection**: Coinductive semantics for recursive types
//!
//! Key benefits:
//! - O(1) type equality via interning (TypeId comparison)
//! - Automatic cycle handling via coinductive semantics
//! - Lazy evaluation - only compute types that are queried
//! - Incremental recomputation via Salsa queries
mod apparent;
pub mod binary_ops;
mod compat;
mod contextual;
mod db;
mod diagnostics;
mod evaluate;
pub mod evaluate_rules;
mod format;
mod infer;
mod instantiate;
mod intern;
mod lawyer;
mod lower;
mod narrowing;
mod operations;
// salsa_db is feature-gated until salsa API is updated
#[cfg(feature = "experimental_salsa")]
pub mod salsa_db;
mod subtype;
mod subtype_rules;
// pub mod tracer; // TODO: Fix type mismatches
mod types;
pub mod unsoundness_audit;
mod utils;
pub mod visitor;
pub use visitor::*;

pub(crate) use apparent::*;
pub use binary_ops::*;
pub use compat::*;
pub use contextual::*;
pub use db::*;
pub use diagnostics::*;
pub use evaluate::*;
pub use format::*;
pub use infer::*;
pub use instantiate::*;
pub use intern::*;
pub use lawyer::*;
pub use lower::*;
pub use narrowing::*;
pub use operations::*;
#[cfg(feature = "experimental_salsa")]
pub use salsa_db::*;
pub use subtype::*;
pub use types::*;
pub use unsoundness_audit::*;

#[cfg(test)]
mod bidirectional_tests;
#[cfg(test)]
mod callable_tests;
#[cfg(test)]
mod compat_tests;
#[cfg(test)]
mod contextual_tests;
#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod diagnostics_tests;
#[cfg(test)]
mod evaluate_tests;
#[cfg(test)]
mod index_signature_tests;
#[cfg(test)]
mod infer_tests;
#[cfg(test)]
mod instantiate_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod intern_tests;
#[cfg(test)]
mod intersection_union_tests;
#[cfg(test)]
mod lawyer_tests;
#[cfg(test)]
mod lower_tests;
// #[cfg(test)]
// mod mapped_key_remap_tests; // TODO: Fix API mismatches (TypeId::TYPE_PARAM, keyof, etc.)
#[cfg(test)]
mod narrowing_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod subtype_tests;
#[cfg(test)]
mod template_expansion_tests;
// #[cfg(test)]
// mod tracer_tests; // TODO: Fix tracer module first
#[cfg(test)]
mod type_law_tests;
#[cfg(test)]
mod types_tests;
#[cfg(test)]
mod union_tests;
#[cfg(test)]
mod visitor_tests;
