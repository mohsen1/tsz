//! Function call error reporting (TS2345, TS2554, TS2769).
//!
//! Split into focused submodules:
//! - `display_formatting`: Type display formatting helpers for call diagnostics
//! - `elaboration`: Call argument elaboration logic (object/array/function)
//! - `error_emission`: Call error emission functions

mod display_formatting;
mod elaboration;
mod error_emission;

#[path = "../call_errors_binding_patterns.rs"]
mod call_errors_binding_patterns;

#[cfg(test)]
#[path = "../call_errors_tests.rs"]
mod tests;
