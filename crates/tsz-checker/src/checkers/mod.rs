//! Domain-specific checker modules.
//!
//! Each module implements type-checking logic for a particular language feature,
//! delegating type-semantic queries to the solver via `query_boundaries`.

pub mod accessor_checker;
pub mod call_checker;
pub mod enum_checker;
pub mod generic_checker;
pub mod iterable_checker;
pub mod jsx_checker;
pub mod parameter_checker;
pub mod promise_checker;
pub mod property_checker;
pub mod signature_builder;
