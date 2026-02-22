//! TypeScript-to-JavaScript emitter and transforms for the tsz compiler.
//!
//! This crate provides:
//! - JavaScript code emission from AST
//! - AST transforms (TypeScript to JavaScript downleveling)
//! - Declaration file (.d.ts) emission
//! - Source map generation

pub mod context;
pub mod declaration_emitter;
/// Re-export for backwards compatibility with external crates.
// TODO: update external consumers to use `context::emit` directly, then remove.
pub mod emit_context {
    pub use crate::context::emit::*;
}
pub mod emitter;
pub mod enums;
pub mod lowering;
/// Re-export for backwards compatibility with external crates.
// TODO: update external consumers to use `lowering` directly, then remove.
pub mod lowering_pass {
    pub use crate::lowering::*;
}
pub mod printer;
pub mod safe_slice;
pub mod source_writer;
/// Re-export for backwards compatibility with external crates.
// TODO: update external consumers to use `context::transform` directly, then remove.
pub mod transform_context {
    pub use crate::context::transform::*;
}
pub mod transforms;
pub mod type_cache_view;
