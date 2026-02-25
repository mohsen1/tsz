//! Subtype checking rules organized by type category.
//!
//! This module contains the implementation of TypeScript's structural subtyping rules,
//! split into focused modules for maintainability:
//!
//! - `intrinsics`: Primitive/intrinsic type compatibility
//! - `literals`: Literal types and template literal matching
//! - `unions`: Union and intersection type logic
//! - `tuples`: Array and tuple compatibility
//! - `objects`: Object property matching and index signatures
//! - `functions`: Function/callable signature compatibility
//! - `generics`: Type parameters, references, and applications
//! - `conditionals`: Conditional type checking

pub mod conditionals;
pub mod functions;
pub mod generics;
pub mod intrinsics;
pub mod literals;
pub mod objects;
pub mod tuples;
pub mod unions;
