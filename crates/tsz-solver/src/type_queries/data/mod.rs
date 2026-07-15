//! Type Content Queries and Data Extraction Helpers
//!
//! This module provides functions for extracting type data and checking type content.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for querying type properties without matching on `TypeData` directly.

mod accessors;
mod conditional_distribution;
#[cfg(test)]
mod construct_return_union_tests;
mod content_predicate_guards;
mod content_predicates;
#[cfg(test)]
mod free_param_cache_tests;
mod signatures_and_advanced;
#[cfg(test)]
mod tests;
mod type_id_list;

pub use accessors::*;
pub use conditional_distribution::*;
pub use content_predicates::*;
pub use signatures_and_advanced::*;
pub use type_id_list::{TypeIdList, TypeIdListIter};
