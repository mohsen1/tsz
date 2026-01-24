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
//! # Migration Status
//!
//! The following transforms have been migrated to use IR nodes:
//!
//! | Transform | Status | Notes |
//! |-----------|--------|-------|
//! | `enum_es5` | ✅ Migrated | Uses `EnumES5Transformer` + `IRPrinter` |
//! | `destructuring_es5` | ✅ Migrated | Uses `ES5DestructuringTransformer` |
//! | `spread_es5` | ✅ Migrated | Uses `SpreadES5Transformer` |
//! | `optional_chain` | ✅ Migrated | Uses IR nodes |
//! | `generators` | ✅ Migrated | Uses `GeneratorTransformer` |
//! | `decorators` | ✅ Migrated | Uses IR nodes |
//! | `namespace_es5` | ✅ Migrated | Uses `NamespaceES5Transformer` + `IRPrinter` |
//! | `class_es5` | ✅ Migrated | Uses `ES5ClassTransformer` + `IRPrinter` |
//! | `async_es5` | ✅ Migrated | Uses `AsyncES5Transformer` + `IRPrinter` |
//! | `arrow_es5` | ℹ️ Helper | Analysis only, no emit |
//! | `block_scoping_es5` | ℹ️ Helper | Analysis only, no emit |
//! | `module_commonjs` | ℹ️ N/A | Module transform (different pattern) |
//!
//! # Public Emitter API
//!
//! The following emitter types are re-exported from this module for use by
//! the emitter. This provides a clean API boundary and breaks direct dependencies
//! on internal submodules.
//!
//! - `ClassES5Emitter` - ES5 class transformation
//! - `EnumES5Emitter` - ES5 enum transformation
//! - `NamespaceES5Emitter` - ES5 namespace transformation

pub mod arrow_es5;
pub mod async_es5;
pub mod emitter;
pub mod async_es5_ir;
pub mod block_scoping_es5;
pub mod class_es5;
pub mod class_es5_ir;
pub mod destructuring_es5;
mod emit_utils;
pub mod enum_es5;
pub mod enum_es5_ir;
pub mod es5;
pub mod helpers;
pub mod ir;
pub mod ir_printer;
pub mod module_commonjs;
pub mod module_commonjs_ir;
pub mod namespace_es5;
pub mod namespace_es5_ir;
pub mod private_fields_es5;
pub mod spread_es5;

// Re-export concrete emitter types for use by the emitter module
// This breaks the dependency on internal submodules (transforms::class_es5)
pub use class_es5::ClassES5Emitter;
pub use enum_es5::EnumES5Emitter;
pub use namespace_es5::NamespaceES5Emitter;

#[cfg(test)]
mod module_commonjs_tests;

#[cfg(test)]
mod ir_tests;

#[cfg(test)]
mod ir_transforms_tests;
