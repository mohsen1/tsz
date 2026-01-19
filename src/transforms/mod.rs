//! JavaScript Transforms
//!
//! This module contains transforms that convert TypeScript/ES2015+ code to
//! earlier JavaScript versions (ES5, ES3) for compatibility.
//!
//! # Architecture
//!
//! Transforms follow a two-phase approach:
//!
//! 1. **Transform Phase**: Analyze AST nodes and produce IR (Intermediate Representation)
//!    nodes that represent the lowered JavaScript constructs.
//!
//! 2. **Print Phase**: The printer walks IR trees and emits JavaScript strings.
//!
//! This separation allows:
//! - Clean separation between transform logic and string emission
//! - IR is testable independently
//! - Printer can apply formatting consistently
//! - Future optimizations (minification, pretty-print) only need to change the printer
//!
//! The transforms are used by thin_emitter.rs for ES5 downleveling.

pub mod arrow_es5;
pub mod async_es5;
pub mod block_scoping_es5;
pub mod class_es5;
mod emit_utils;
pub mod enum_es5;
pub mod es5;
pub mod helpers;
pub mod ir;
pub mod ir_printer;
pub mod module_commonjs;
pub mod namespace_es5;
pub mod private_fields_es5;

#[cfg(test)]
mod module_commonjs_tests;

#[cfg(test)]
mod ir_tests;
