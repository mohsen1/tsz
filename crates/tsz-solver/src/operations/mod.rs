//! Type operations and expression evaluation.
//!
//! This module contains the "brain" of the type system - all the logic for
//! evaluating expressions, resolving calls, accessing properties, etc.
//!
//! ## Architecture Principle
//!
//! The Solver handles **WHAT** (type operations and relations), while the
//! Checker handles **WHERE** (AST traversal, scoping, control flow).
//!
//! All functions here:
//! - Take `TypeId` as input (not AST nodes)
//! - Return structured results (not formatted error strings)
//! - Are pure logic (no side effects, no diagnostic formatting)
//!
//! This allows the Solver to be:
//! - Unit tested without AST nodes
//! - Reused across different checkers
//! - Optimized independently
//!
//! ## Module Organization
//!
//! Components are organized into separate modules:
//! - `core`: Call evaluation, assignability traits, and free-function entry points
//! - `binary_ops`: Binary operation evaluation (+, -, *, /, etc.)
//! - `call_args`: Argument checking, parameter analysis, tuple rest handling, placeholder detection
//! - `constraints`: Type constraint collection for generic inference
//! - `constructors`: Constructor (new) expression resolution
//! - `generic_call`: Generic function call inference
//! - `generics`: Generic type instantiation validation
//! - `iterators`: Iterator/async iterator type extraction
//! - `property`: Property access resolution (includes helpers for mapped, primitive, array, etc.)
//! - `property_readonly`: Readonly property checks
//! - `property_visitor`: `TypeVisitor` impl for `PropertyAccessEvaluator`
//! - `compound_assignment`: Compound assignment operator classification and fallback types
//! - `expression_ops`: Expression type computation (conditional, template, best common type)
//! - `widening`: Type widening (literal → primitive) and `as const` assertion

pub mod binary_ops;
mod call_args;
pub mod compound_assignment;
mod constraints;
mod constructors;
mod core;
pub mod expression_ops;
mod generic_call;
pub mod generics;
pub mod iterators;
pub mod property;
mod property_readonly;
mod property_visitor;
pub mod widening;

// Re-exports from core implementation
pub use self::core::{
    AssignabilityChecker, CallEvaluator, CallResult, CallWithCheckerResult,
    MAX_CONSTRAINT_RECURSION_DEPTH, compute_contextual_types_with_compat_checker,
    get_contextual_signature_cached_with_compat_checker,
    get_contextual_signature_for_arity_cached_with_compat_checker,
    get_contextual_signature_for_arity_with_compat_checker,
    get_contextual_signature_with_compat_checker, infer_call_signature, infer_generic_function,
    resolve_call_with_checker, resolve_new_with_checker,
};

// Re-exports from submodules
pub use binary_ops::{BinaryOpEvaluator, BinaryOpResult};
pub use compound_assignment::*;
pub use expression_ops::*;
#[cfg(test)]
pub(crate) use generics::{GenericInstantiationResult, solve_generic_instantiation};
pub use iterators::{
    IteratorInfo, extract_iterator_result_value_types, get_async_iterable_element_type,
    get_iterator_info,
};
pub use widening::*;

#[cfg(test)]
use crate::types::*;

#[cfg(test)]
#[path = "../../tests/operations_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/binary_ops_comprehensive_tests.rs"]
mod binary_ops_comprehensive_tests;
