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
pub mod tracer;
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

// Test modules: Some are loaded by their source files via #[path = "..."] declarations.
// Only include modules here that aren't loaded elsewhere to avoid duplicate_mod warnings.
#[cfg(test)]
mod bidirectional_tests;
// callable_tests: loaded from subtype.rs
// compat_tests: loaded from compat.rs
// contextual_tests: loaded from contextual.rs
// db_tests: loaded from db.rs
// diagnostics_tests: loaded from diagnostics.rs
// evaluate_tests: loaded from evaluate.rs
// index_signature_tests: loaded from subtype.rs
// infer_tests: loaded from infer.rs
// instantiate_tests: loaded from instantiate.rs
#[cfg(test)]
mod integration_tests;
// intern_tests: loaded from intern.rs
#[cfg(test)]
mod intersection_union_tests;
// lawyer_tests: loaded from lawyer.rs
// lower_tests: loaded from lower.rs
// mapped_key_remap_tests: TODO: Fix API mismatches (TypeId::TYPE_PARAM, keyof, etc.)
// narrowing_tests: loaded from narrowing.rs
// operations_tests: loaded from operations.rs
// subtype_tests: loaded from subtype.rs
#[cfg(test)]
mod template_expansion_tests;
// tracer_tests: tests are in tracer.rs module
#[cfg(test)]
mod type_law_tests;
// types_tests: loaded from types.rs
// union_tests: loaded from subtype.rs
#[cfg(test)]
mod visitor_tests;
