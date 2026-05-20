//! TypeScript-to-JavaScript emitter and transforms for the tsz compiler.
//!
//! This crate provides:
//! - JavaScript code emission from AST
//! - AST transforms (TypeScript to JavaScript downleveling)
//! - Declaration file (`.d.ts`) emission behind the `dts` feature
//! - Source map generation

#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::needless_borrow)]

pub mod context;
#[cfg(feature = "dts")]
pub mod declaration_emitter;
pub mod emitter;
pub mod enums;
pub mod import_usage;
pub(crate) mod jsx_pragmas;

/// tsc emits this exact string when recursive DTS expansion reaches its depth limit.
pub(crate) const ELIDED_ANY: &str = "/*elided*/ any";
/// tsc stops expanding recursive generic function return types at this depth.
pub(crate) const MAX_RECURSIVE_EXPANSION: u32 = 10;
pub mod lowering;
pub mod output;
pub mod safe_slice;
pub mod transforms;
pub mod type_cache_view;

#[cfg(test)]
#[path = "../tests/es5_transforms_e2e.rs"]
mod es5_transforms_e2e;
