use crate::context::emit::EmitContext;

use crate::context::plan::{EmitPlan, EmitPlanBuilder};

use crate::context::transform::{IdentifierId, TransformContext, TransformDirective};

use crate::jsx_pragmas::JsxPragmaFacts;

use std::sync::Arc;

use tsz_common::ScriptTarget;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{Node, NodeArena};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::syntax::transform_utils::{
    arrow_captures_lexical_this, contains_arguments_reference,
};

use tsz_scanner::SyntaxKind;

use crate::transforms::emit_utils;

/// Maximum recursion depth for AST traversal to prevent stack overflow
pub(super) const MAX_AST_DEPTH: u32 = 500;

/// Maximum depth for qualified name recursion (A.B.C.D...)
pub(super) const MAX_QUALIFIED_NAME_DEPTH: u32 = 100;

/// Maximum depth for binding pattern recursion ({a: {b: {c: ...}}})
pub(super) const MAX_BINDING_PATTERN_DEPTH: u32 = 100;

/// Lowering pass - Phase 1 of emission
///
/// Walks the AST and produces transform directives based on compiler options.
pub struct LoweringPass<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) ctx: &'a EmitContext,
    pub(super) transforms: TransformContext,
    pub(super) commonjs_mode: bool,
    pub(super) has_export_assignment: bool,
    /// Current recursion depth for stack overflow protection
    pub(super) visit_depth: u32,
    /// Track declared names for namespace/class/enum/function merging detection
    pub(super) declared_names: rustc_hash::FxHashSet<String>,
    /// Nesting depth of namespace/module declaration bodies.
    /// CommonJS export directives should only be forced at top-level (depth == 0).
    pub(super) namespace_depth: u32,
    /// Depth of arrow functions that capture 'this'
    /// When > 0, 'this' references should be substituted with '_this'
    pub(super) this_capture_level: u32,
    /// Depth of arrow functions that capture 'arguments'
    /// When > 0, 'arguments' references should be substituted with '_arguments'
    pub(super) arguments_capture_level: u32,
    /// Tracks if the current class declaration has an 'extends' clause
    pub(super) current_class_is_derived: bool,
    /// Tracks if we are currently inside a constructor body
    pub(super) in_constructor: bool,
    /// Tracks if we are inside a static class member
    pub(super) in_static_context: bool,
    /// Current class alias name (e.g., "_a") for static members
    pub(super) current_class_alias: Option<String>,
    /// True when visiting the left side of a destructuring assignment
    pub(super) in_assignment_target: bool,
    /// True when inside a class body in ES5 mode.
    /// Arrow functions inside class members should NOT propagate _this capture
    /// to the enclosing scope because the class IIFE creates its own scope
    /// and `class_es5_ir` handles _this capture independently.
    pub(super) in_es5_class: bool,
    /// Names that are re-exported via `export { Name }` (without a module specifier).
    /// Used to determine if a namespace/enum IIFE should fold exports into the
    /// closing argument (e.g., `(A || (exports.A = A = {}))`).
    pub(super) re_exported_names: rustc_hash::FxHashSet<String>,
    /// Export names from local `export { local as exported }` clauses, keyed by
    /// the local name. Namespace IIFE folding needs the exported name in the
    /// `exports.<name>` slot while keeping the local namespace binding unchanged.
    pub(super) re_exported_export_names: rustc_hash::FxHashMap<String, Vec<IdentifierId>>,
    /// Every export alias attached to a local binding (direct `export` modifier
    /// declarations AND local `export { x as y }` clauses), captured in source
    /// order. This is what enum/namespace lowering needs to fold every exported
    /// alias into the IIFE tail, e.g.
    /// ```text
    /// export enum E {}
    /// export { E as EE };
    /// ```
    /// produces `[E_id, EE_id]` for local `E`, which the emitter writes as
    /// `(E || (exports.EE = exports.E = E = {}))`. `re_exported_export_names`
    /// alone is not enough because it drops the direct export.
    pub(super) all_export_aliases_in_order: rustc_hash::FxHashMap<String, Vec<IdentifierId>>,
    /// Stack of enclosing non-arrow function body node indices.
    /// When an arrow function captures `this`, the top of this stack is the
    /// scope that needs `var _this = this;`.
    pub(super) enclosing_function_bodies: Vec<NodeIndex>,
    /// Stack of capture variable names matching `enclosing_function_bodies`.
    /// Each entry is the name to use for `_this` capture in that scope
    /// (e.g., "_this" or "_`this_1`" if there's a collision with a user-defined `_this`).
    pub(super) enclosing_capture_names: Vec<Arc<str>>,
    /// Source text for the source file currently being traversed.
    pub(super) current_source_text: Option<&'a str>,
    /// File-level JSX pragma facts for the source file currently being traversed.
    pub(super) current_jsx_pragmas: JsxPragmaFacts,
}

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/lowering_pass.rs"]
mod tests;
