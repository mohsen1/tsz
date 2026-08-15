//! TS2526 for a JSDoc `@return`/`@returns` `{this}` type expression whose
//! host function is not a non-static class or interface member.
//!
//! The TS-syntax `this` type node routes through `TypeNodeChecker`'s
//! `THIS_TYPE` branch, which asks `is_this_type_allowed` and reports TS2526.
//! JSDoc type expressions are resolved from comment text instead — the
//! `"this"` string maps straight to the solver's `this` type with no
//! positional gate — so the same invalid position went undiagnosed in
//! checked JS.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use crate::types_domain::type_node_helpers::{
    is_this_type_allowed_in, node_has_static_modifier_in,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    /// Report TS2526 when `func_idx`'s JSDoc carries a bare `@return {this}`
    /// (or `@returns {this}`) tag but the function is not a non-static
    /// class/interface member.
    ///
    /// A `@return` tag types the *function's own* return, so its `this`
    /// resolves against the function's `this` container: a real non-static
    /// method qualifies, while a function expression, arrow, or object
    /// literal method assigned onto a constructor function or its prototype
    /// does not. That is exactly `is_this_type_allowed`'s walk, started at
    /// the function node itself so a `static` member is still rejected.
    pub(crate) fn report_jsdoc_return_this_type_not_allowed(&mut self, func_idx: NodeIndex) {
        if !self.is_js_file() || func_idx.is_none() {
            return;
        }
        let Some(jsdoc) = self.find_jsdoc_for_function(func_idx) else {
            return;
        };
        let Some(type_expr) = Self::jsdoc_returns_type_expression(&jsdoc) else {
            return;
        };
        // Only the bare `this` spelling. Nested occurrences (`Array<this>`,
        // `this | null`) resolve through the generic expression parser and
        // would need their own inner-token anchors.
        if type_expr.trim() != "this" {
            return;
        }
        if self.jsdoc_this_type_allowed_for_host(func_idx) {
            return;
        }
        let Some(start) = self.jsdoc_return_this_token_position(func_idx) else {
            return;
        };
        let length = "this".len() as u32;
        let code = diagnostic_codes::A_THIS_TYPE_IS_AVAILABLE_ONLY_IN_A_NON_STATIC_MEMBER_OF_A_CLASS_OR_INTERFACE;
        // Signature building runs repeatedly per function across call sites
        // (declaration checking, overload resolution, contextual typing), so
        // guard against re-reporting the same span.
        let already_reported = self
            .ctx
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code && diagnostic.start == start);
        if already_reported {
            return;
        }
        self.error_at_position(
            start,
            length,
            diagnostic_messages::A_THIS_TYPE_IS_AVAILABLE_ONLY_IN_A_NON_STATIC_MEMBER_OF_A_CLASS_OR_INTERFACE,
            code,
        );
    }

    /// `is_this_type_allowed` starts its walk at the given node's *parent*,
    /// which is correct for a `this` type node nested inside a member but
    /// would skip the member's own `static` modifier when the host itself is
    /// the member. Inspect the host first, then defer to the shared walk.
    fn jsdoc_this_type_allowed_for_host(&self, host_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(host_idx) else {
            return false;
        };
        if matches!(
            node.kind,
            syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
        ) && node_has_static_modifier_in(&self.ctx, node)
        {
            return false;
        }
        is_this_type_allowed_in(&self.ctx, host_idx)
    }

    /// Absolute source position of the `this` token inside the function's
    /// `@return {this}` / `@returns {this}` tag.
    fn jsdoc_return_this_token_position(&self, func_idx: NodeIndex) -> Option<u32> {
        let comment_start = self.get_jsdoc_comment_pos_for_function(func_idx)? as usize;
        let source_file = self.source_file_data_for_node(func_idx)?;
        let source_text = source_file.text.as_ref();
        let tail = source_text.get(comment_start..)?;
        let (tag_offset, tag) = ["@returns {this}", "@return {this}"]
            .into_iter()
            .filter_map(|tag| tail.find(tag).map(|offset| (offset, tag)))
            .min_by_key(|(offset, _)| *offset)?;
        let brace_offset = tag.find('{')? + 1;
        Some((comment_start + tag_offset + brace_offset) as u32)
    }
}
