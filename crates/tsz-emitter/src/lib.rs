//! TypeScript-to-JavaScript emitter and transforms for the tsz compiler.
//!
//! This crate provides:
//! - JavaScript code emission from AST
//! - AST transforms (TypeScript to JavaScript downleveling)
//! - Declaration file (.d.ts) emission
//! - Source map generation

pub mod declaration_emitter;
pub mod emit_context;
pub mod emitter;
pub mod enums;
pub mod lowering_pass;
pub mod printer;
pub mod source_writer;
pub mod transform_context;
pub mod transforms;
