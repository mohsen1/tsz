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

// Re-exports for submodule access (used by sibling modules via `use super::*`)
#[allow(unused_imports)]
use self::core::{MAX_AST_DEPTH, MAX_BINDING_PATTERN_DEPTH, MAX_QUALIFIED_NAME_DEPTH};
#[allow(unused_imports)]
use crate::context::emit::EmitContext;
#[allow(unused_imports)]
use crate::context::transform::{IdentifierId, ModuleFormat, TransformContext, TransformDirective};
#[allow(unused_imports)]
use crate::transforms::emit_utils;
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tsz_common::common::ModuleKind;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_parser::syntax::transform_utils::{
    contains_arguments_reference, contains_this_reference, is_private_identifier,
};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;
