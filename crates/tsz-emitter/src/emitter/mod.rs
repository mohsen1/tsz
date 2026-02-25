//! Emitter - Emitter using `NodeArena`
//!
//! This emitter uses the Node architecture for cache-optimized AST access.
//! It works directly with `NodeArena` instead of the old Node enum.
//!
//! # Architecture
//!
//! - Uses `NodeArena` for AST access (16-byte nodes, 13x cache improvement)
//! - Dispatches based on Node.kind (u16)
//! - Uses accessor methods to get typed node data
//!
//! # Module Organization
//!
//! The emitter is organized as a directory module:
//! - `core.rs` - Core Printer struct, dispatch logic, and emit methods
//! - `expressions.rs` - Expression emission helpers
//! - `statements.rs` - Statement emission helpers
//! - `declarations/` - Declaration emission (classes, class members, namespaces)
//! - `functions.rs` - Function emission helpers
//! - `types.rs` - Type emission helpers
//! - `jsx.rs` - JSX emission helpers
//! - `module_emission/` - Module emission (imports, exports, CommonJS/ES6)
//! - `es5/` - ES5 downlevel binding/destructuring helpers
//!
//! Note: pub(super) and pub(in `crate::emitter`) fields and methods allow
//! submodules to access Printer internals.

mod binding_patterns;
mod comment_helpers;
mod comments;
mod core;
mod declarations;
mod es5;
mod expressions;
mod expressions_access;
mod expressions_binary_downlevel;
mod expressions_call;
mod expressions_literals;
mod functions;
mod helpers;
mod jsx;
mod literals;
mod module_emission;
mod module_wrapper;
mod source_file;
mod special_expressions;
mod statements;
mod template_literals;
mod transform_dispatch;
pub mod type_printer;
mod types;

pub(crate) use self::core::get_operator_text;
pub(crate) use self::core::is_valid_identifier_name;
pub(crate) use self::core::{
    ParamTransform, ParamTransformPlan, RestParamTransform, TempScopeState, TemplateParts,
};
pub use self::core::{Printer, PrinterOptions};
pub use comments::{
    CommentKind, CommentRange, get_leading_comment_ranges, get_trailing_comment_ranges,
};

// Re-export common types for backward compatibility
pub use tsz_common::common::{ModuleKind, NewLineKind, ScriptTarget};

// Re-exports for submodule access (used by sibling modules via `use super::*`)
#[allow(unused_imports)]
pub(crate) use crate::context::emit::EmitContext;
#[allow(unused_imports)]
pub(crate) use crate::context::transform::{IdentifierId, TransformContext, TransformDirective};
#[allow(unused_imports)]
pub(crate) use crate::enums::evaluator::EnumValue;
#[allow(unused_imports)]
pub(crate) use crate::output::source_writer::{
    SourcePosition, SourceWriter, source_position_from_offset,
};
#[allow(unused_imports)]
pub(crate) use crate::transforms::{ClassES5Emitter, EnumES5Emitter, NamespaceES5Emitter};
pub(crate) use tsz_parser::parser::NodeIndex;
#[allow(unused_imports)]
pub(crate) use tsz_parser::parser::node::{Node, NodeArena};
pub(crate) use tsz_parser::parser::syntax_kind_ext;
pub(crate) use tsz_scanner::SyntaxKind;
