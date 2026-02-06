//! Type evaluation rules organized by type category.
//!
//! This module contains the implementation of TypeScript's meta-type evaluation,
//! split into focused modules for maintainability:
//!
//! - `conditional`: Conditional type evaluation (T extends U ? X : Y)
//! - `index_access`: Index access type evaluation (T[K])
//! - `mapped`: Mapped type evaluation ({ [K in keyof T]: T[K] })
//! - `keyof`: keyof operator evaluation
//! - `template_literal`: Template literal type evaluation
//! - `string_intrinsic`: String manipulation intrinsics (Uppercase, etc.)
//! - `infer_pattern`: Pattern matching for infer types
//! - `apparent`: Apparent type utilities for primitives

pub mod apparent;
pub mod conditional;
pub mod index_access;
pub mod infer_pattern;
pub mod keyof;
pub mod mapped;
pub mod string_intrinsic;
pub mod template_literal;
