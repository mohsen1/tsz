//! Contextual typing (reverse inference).
//!
//! Contextual typing allows type information to flow "backwards" from
//! an expected type to an expression. This is used for:
//! - Arrow function parameters: `const f: (x: string) => void = (x) => ...`
//! - Array literals: `const arr: number[] = [1, 2, 3]`
//! - Object literals: `const obj: {x: number} = {x: 1}`
//!
//! The key insight is that when we have an expected type, we can use it
//! to infer types for parts of the expression that would otherwise be unknown.
//!
//! The visitor-based type extractors used by [`ContextualTypeContext`] are in
//! the [`extractors`] submodule.

mod core;
pub(crate) mod extractors;

pub use self::core::{ContextualTypeContext, apply_contextual_type};
