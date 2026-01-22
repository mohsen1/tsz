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
//! | `async_es5` | ⚠️ Legacy | Directly emits strings (needs migration) |
//! | `arrow_es5` | ℹ️ Helper | Analysis only, no emit |
//! | `block_scoping_es5` | ℹ️ Helper | Analysis only, no emit |
//! | `module_commonjs` | ℹ️ N/A | Module transform (different pattern) |
//!
//! # Migration Pattern
//!
//! To migrate a transform from string emission to IR:
//!
//! 1. Create a new `*Transformer` struct that produces `IRNode` trees
//! 2. Keep the old `*Emitter` as a wrapper for backward compatibility
//! 3. Update the emitter to use the transformer + `IRPrinter`
//! 4. Add tests comparing old and new output
//! 5. Update callers to use the new transformer directly
//!
//! Example from `enum_es5.rs`:
//! ```ignore
//! // NEW: Transformer produces IR
//! pub struct EnumES5Transformer<'a> {
//!     arena: &'a NodeArena,
//! }
//!
//! impl<'a> EnumES5Transformer<'a> {
//!     pub fn transform_enum(&mut self, idx: NodeIndex) -> Option<IRNode> {
//!         // Build IR tree...
//!     }
//! }
//!
//! // LEGACY: Wrapper for backward compatibility
//! pub struct EnumES5Emitter<'a> {
//!     transformer: EnumES5Transformer<'a>,
//! }
//!
//! impl<'a> EnumES5Emitter<'a> {
//!     pub fn emit_enum(&mut self, idx: NodeIndex) -> String {
//!         let ir = self.transformer.transform_enum(idx)?;
//!         IRPrinter::emit_to_string(&ir)
//!     }
//! }
//! ```
//!
//! The transforms are used by the emitter for ES5 downleveling.

pub mod arrow_es5;
pub mod async_es5;
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

#[cfg(test)]
mod module_commonjs_tests;

#[cfg(test)]
mod ir_tests;

#[cfg(test)]
mod ir_transforms_tests;
