//! Array Type Utilities Module
//!
//! Thin wrappers for array type queries, delegating to solver via `query_boundaries`.
//! Direct solver queries (e.g. `query::is_array_type`, `query::array_element_type`)
//! are preferred at call sites for clarity.
