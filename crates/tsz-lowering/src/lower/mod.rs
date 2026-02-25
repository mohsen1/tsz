//! Type lowering: AST nodes → `TypeId`
//!
//! This module implements the "bridge" that converts raw AST type nodes
//! into the structural type system (`TypeId`).

mod advanced;
mod core;

pub use self::core::*;
