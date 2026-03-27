//! TypeScript-to-JavaScript emitter and transforms for the tsz compiler.
//!
//! This crate provides:
//! - JavaScript code emission from AST
//! - AST transforms (TypeScript to JavaScript downleveling)
//! - Declaration file (.d.ts) emission
//! - Source map generation

#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_borrow)]

pub mod context;
pub mod declaration_emitter;
pub mod emitter;
pub mod enums;
pub mod import_usage;
pub mod lowering;
pub mod output;
pub mod safe_slice;
pub mod transforms;
pub mod type_cache_view;

#[cfg(test)]
#[path = "../tests/es5_transforms_e2e.rs"]
mod es5_transforms_e2e;
