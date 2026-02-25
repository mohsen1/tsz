//! Definition identifiers and resolution for the solver.
//!
//! This module groups the DefId system:
//! - `core`: DefId allocation, `DefinitionStore`, `DefKind`, `DefInfo`
//! - `resolver`: `TypeResolver` trait and `TypeEnvironment`

mod core;
pub mod resolver;

pub use self::core::*;
