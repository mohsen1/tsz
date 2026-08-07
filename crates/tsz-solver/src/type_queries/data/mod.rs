//! Type Content Queries and Data Extraction Helpers
//!
//! This module provides functions for extracting type data and checking type content.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for querying type properties without matching on `TypeData` directly.

mod accessors;
mod conditional_constraint;
mod conditional_distribution;
#[cfg(test)]
mod construct_return_union_tests;
mod content_predicate_guards;
mod content_predicates;
mod exact_property_keys;
#[cfg(test)]
mod free_infer_cache_tests;
mod free_infer_predicate;
#[cfg(test)]
mod free_param_cache_tests;
mod intersection_conflict;
mod nominal_and_base;
mod rest_binder_queries;
mod signatures_and_advanced;
#[cfg(test)]
mod tests;
mod type_id_list;

pub use accessors::*;
pub use conditional_constraint::*;
pub use conditional_distribution::*;
pub use content_predicates::*;
pub use exact_property_keys::*;
pub use free_infer_predicate::*;
pub use intersection_conflict::*;
pub use nominal_and_base::*;
pub use rest_binder_queries::*;
pub use signatures_and_advanced::*;
pub use type_id_list::{TypeIdList, TypeIdListIter};
