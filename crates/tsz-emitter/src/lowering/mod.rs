//! Lowering Pass - Phase 1 of the Transform/Print Architecture
//!
//! This module implements the first phase of emission: analyzing the AST and
//! producing transform directives. The lowering pass walks the Node AST
//! and determines which nodes need transformation based on compiler options
//! (ES5 target, module format, etc.).
//!
//! # Architecture
//!
//! The lowering pass is a **read-only** traversal of the AST that produces
//! a `TransformContext` containing `TransformDirective`s for nodes that need
//! special handling during emission.
//!
//! ## Examples
//!
//! ### ES5 Class Transform
//!
//! When `target: ES5`, a `ClassDeclaration` needs transformation:
//!
//! ```typescript
//! class Point {
//!     constructor(x, y) { this.x = x; this.y = y; }
//! }
//! ```
//!
//! The lowering pass creates a `TransformDirective::ES5Class` for this node,
//! which the printer will use to emit an IIFE pattern instead of `class`.
//!
//! ### `CommonJS` Export
//!
//! When `module: CommonJS`, exported declarations need wrapping:
//!
//! ```typescript
//! export class Foo {}
//! ```
//!
//! The lowering pass creates a `TransformDirective::CommonJSExport` that
//! chains with any other transforms (like `ES5Class`).

mod core;
mod helpers;
mod visit_children;

pub use self::core::LoweringPass;

// Re-exports consumed by submodules via `use super::*`
use self::core::{MAX_BINDING_PATTERN_DEPTH, MAX_QUALIFIED_NAME_DEPTH};
pub(super) use crate::context::transform::{
    IdentifierId, ModuleFormat, TransformDirective,
};
pub(super) use crate::transforms::emit_utils;
pub(super) use std::sync::Arc;
pub(super) use tsz_common::common::ModuleKind;
pub(super) use tsz_parser::parser::node::Node;
pub(super) use tsz_parser::parser::syntax_kind_ext;
pub(super) use tsz_parser::parser::{NodeIndex, NodeList};
pub(super) use tsz_parser::syntax::transform_utils::is_private_identifier;
pub(super) use tsz_scanner::SyntaxKind;
