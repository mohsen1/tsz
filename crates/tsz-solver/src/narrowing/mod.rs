//! Type narrowing for discriminated unions and type guards.
//!
//! Discriminated unions are unions where each member has a common "discriminant"
//! property with a literal type that uniquely identifies that member.
//!
//! Example:
//! ```typescript
//! type Action =
//!   | { type: "add", value: number }
//!   | { type: "remove", id: string }
//!   | { type: "clear" };
//!
//! function handle(action: Action) {
//!   if (action.type === "add") {
//!     // action is narrowed to { type: "add", value: number }
//!   }
//! }
//! ```
//!
//! ## `TypeGuard` Abstraction
//!
//! The `TypeGuard` enum provides an AST-agnostic representation of narrowing
//! conditions. This allows the Solver to perform pure type algebra without
//! depending on AST nodes.
//!
//! Architecture:
//! - **Checker**: Extracts `TypeGuard` from AST nodes (WHERE)
//! - **Solver**: Applies `TypeGuard` to types (WHAT)

mod compound;
mod core;
mod discriminants;
mod instanceof;
mod property;
pub(crate) mod utils;

// Re-export utility functions from the utils submodule
pub use utils::{
    find_discriminants, is_definitely_nullish, is_nullish_type, narrow_by_discriminant,
    narrow_by_typeof, remove_nullish, remove_nullish_query, remove_undefined, split_nullish_type,
    type_contains_undefined,
};

// Re-export public items from compound narrowing
pub use self::compound::NullishFilter;

// Re-export all public items from core implementation
pub(crate) use self::core::union_or_single_preserve;
pub use self::core::{
    DiscriminantInfo, GuardSense, NarrowingCache, NarrowingContext, NarrowingResult, TypeGuard,
    TypeofKind,
};

#[cfg(test)]
use crate::types::*;

#[cfg(test)]
#[path = "../../tests/narrowing_tests.rs"]
mod tests;
