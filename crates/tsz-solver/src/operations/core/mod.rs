//! Core call evaluation types and logic.
//!
//! Split into submodules:
//! - `call_evaluator`: `AssignabilityChecker` trait, `CallResult`, `CallEvaluator` struct,
//!   signature inference, contextual signature extraction, and union signature analysis.
//! - `call_resolution`: `resolve_call`, union/intersection call resolution, overload
//!   resolution, and free-function entry points.

pub(crate) mod call_evaluator;
mod call_resolution;

pub(crate) use call_evaluator::MAX_CONSTRAINT_STEPS;
pub use call_evaluator::*;
pub use call_resolution::*;
