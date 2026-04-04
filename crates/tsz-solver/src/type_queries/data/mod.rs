//! Type Content Queries and Data Extraction Helpers
//!
//! This module provides functions for extracting type data and checking type content.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for querying type properties without matching on `TypeData` directly.

mod accessors;
mod content_predicates;
mod signatures_and_advanced;
#[cfg(test)]
mod tests;

pub use accessors::*;
pub use content_predicates::*;
pub use signatures_and_advanced::*;
