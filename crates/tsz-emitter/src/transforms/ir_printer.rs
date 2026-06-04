use std::borrow::Cow;

use std::fmt::Write;

use std::sync::Arc;

#[path = "ir_printer_class_emit.rs"]
mod ir_printer_class_emit;

#[path = "ir_printer_generator_state.rs"]
mod ir_printer_generator_state;

#[path = "ir_printer_helpers.rs"]
mod ir_printer_helpers;

#[path = "ir_printer_namespace.rs"]
mod ir_printer_namespace;

#[path = "ir_printer_recovery.rs"]
mod ir_printer_recovery;

use ir_printer_namespace::NamespaceIifeContext;

use crate::context::transform::TransformContext;

use crate::emitter::{Printer as AstPrinter, PrinterOptions};

use crate::transforms::ClassES5Emitter;

use crate::transforms::ir::{
    EnumMember, EnumMemberValue, IRMethodName, IRNode, IRParam, IRProperty, IRPropertyKey,
    IRPropertyKind, IRSwitchCase,
};

use tsz_parser::parser::base::NodeIndex;

use tsz_parser::parser::node::NodeArena;

/// IR Printer - converts IR nodes to JavaScript strings
pub struct IRPrinter<'a> {
    output: String,
    indent_level: u32,
    indent_str: &'static str,
    /// Optional arena for handling `ASTRef` nodes
    arena: Option<&'a NodeArena>,
    /// Source text for emitting `ASTRef` nodes
    source_text: Option<&'a str>,
    /// Optional transform directives for `ASTRef` nodes
    transforms: Option<TransformContext>,
    /// Avoid duplicate trailing comments when a sequence explicitly carries one.
    suppress_function_trailing_extraction: bool,
    /// Tracks when the last emitted IR node wrote a trailing line comment.
    last_emit_ended_with_line_comment: bool,
    /// Source range end for nested AST arrow comments that should be left for
    /// an IR-owned semicolon/trailing-comment site.
    ast_arrow_comment_defer_end: Option<u32>,
    /// Name of the current ES5 class IIFE constructor, used to force constructor
    /// empty-body formatting without affecting nested function declarations.
    current_class_iife_name: Option<String>,
    /// When true, the next `FunctionExpr` emit will force multiline for empty bodies.
    /// Set by `CallExpr` when emitting an IIFE callee.
    force_iife_multiline_empty: bool,
    /// When true, we are inside a namespace IIFE body.
    /// Nested namespace variable declarations use `let` instead of `var` in ES2015+ targets.
    in_namespace_iife_body: bool,
    /// When true, the target is ES5 and `let`/`const` should not be emitted.
    target_es5: bool,
    /// When true, comments like `/** @class */` are suppressed in output.
    remove_comments: bool,
    /// CommonJS `tslib` binding used to prefix runtime helper calls for importHelpers.
    tslib_prefix: bool,
    tslib_import_binding: String,
    commonjs_import_substitutions: rustc_hash::FxHashMap<String, String>,
    system_import_meta: bool,
    pub(crate) base_printer_options: Option<PrinterOptions>,
    generator_state_name: &'static str,
    generator_this_arg: String,
    /// Outer names (e.g. a class-expression alias) excluded from generator state
    /// variable selection.  Treated as already-allocated hoisted vars so the
    /// state-name picker skips past them.
    outer_reserved_for_generator_state: Vec<String>,
    namespace_ast_name: Option<String>,
    namespace_ast_exported_names: rustc_hash::FxHashSet<String>,
    block_scope_shadowed_names: Vec<String>,
    block_scope_reserved_names: Vec<String>,
    pending_commonjs_class_export_name: Option<(String, Vec<String>)>,
}

include!("ir_printer_parts/part1.rs");
include!("ir_printer_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/ir_printer.rs"]
mod tests;
