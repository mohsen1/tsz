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
mod application;
pub mod binary_ops;
mod class_hierarchy;
pub mod compat;
mod contextual;
mod db;
pub mod def;
mod diagnostics;
pub mod element_access;
mod evaluate;
pub mod evaluate_rules;
pub mod expression_ops;
mod flow_analysis;
mod format;
pub mod freshness;
mod index_signatures;
mod infer;
pub mod inheritance;
mod instantiate;
mod intern;
pub mod judge;
mod lawyer;
mod lower;
mod narrowing;
mod object_literal;
pub mod objects;
pub mod operations;
pub mod operations_property;
// salsa_db is feature-gated until salsa API is updated
#[cfg(feature = "experimental_salsa")]
pub mod salsa_db;
pub mod sound;
mod subtype;
mod subtype_rules;
pub mod tracer;
pub mod type_queries;
pub mod type_queries_extended;
pub mod types;
pub mod unsoundness_audit;
mod utils;
pub mod visitor;
pub mod widening;
pub use visitor::*;

pub(crate) use apparent::*;
pub use application::*;
pub use binary_ops::*;
pub use class_hierarchy::*;
pub use compat::*;
pub use contextual::*;
pub use db::*;
pub use def::*;
pub use diagnostics::*;
pub use element_access::*;
pub use evaluate::*;
pub use flow_analysis::*;
pub use format::*;
pub use freshness::*;
pub use index_signatures::*;
pub use infer::*;
pub use inheritance::*;
pub use instantiate::*;
pub use intern::*;
pub use judge::*;
pub use lawyer::*;
pub use lower::*;
pub use narrowing::*;
pub use object_literal::*;
pub use objects::*;
pub use operations::*;
#[cfg(feature = "experimental_salsa")]
pub use salsa_db::*;
pub use sound::*;
pub use subtype::*;
pub use types::Visibility;
pub use types::*;
pub use unsoundness_audit::*;
pub use widening::*;

// Test modules: Most are loaded by their source files via #[path = "tests/..."] declarations.
// Only include modules here that aren't loaded elsewhere to avoid duplicate_mod warnings.
#[cfg(test)]
#[path = "tests/bidirectional_tests.rs"]
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
#[path = "tests/integration_tests.rs"]
mod integration_tests;
// intern_tests: loaded from intern.rs
#[cfg(test)]
#[path = "tests/enum_nominality.rs"]
mod enum_nominality;
#[cfg(test)]
#[path = "tests/intersection_union_tests.rs"]
mod intersection_union_tests;
// lawyer_tests: loaded from lawyer.rs
// lower_tests: loaded from lower.rs
// mapped_key_remap_tests: TODO: Fix API mismatches (TypeId::TYPE_PARAM, keyof, etc.)
// narrowing_tests: loaded from narrowing.rs
// operations_tests: loaded from operations.rs
// subtype_tests: loaded from subtype.rs
#[cfg(test)]
#[path = "tests/template_expansion_tests.rs"]
mod template_expansion_tests;
// tracer_tests: tests are in tracer.rs module
#[cfg(test)]
#[path = "tests/type_law_tests.rs"]
mod type_law_tests;
// types_tests: loaded from types.rs
// union_tests: loaded from subtype.rs
#[cfg(test)]
#[path = "tests/solver_refactoring_tests.rs"]
mod solver_refactoring_tests;
#[cfg(test)]
#[path = "tests/visitor_tests.rs"]
mod visitor_tests;
