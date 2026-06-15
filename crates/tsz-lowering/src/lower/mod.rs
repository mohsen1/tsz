//! Type lowering: AST nodes → `TypeId`
//!
//! This module implements the "bridge" that converts raw AST type nodes
//! into the structural type system (`TypeId`).

mod advanced;
mod core;
mod host;

pub use self::core::*;
pub use self::host::{ClosureLoweringHost, LoweringHost};
