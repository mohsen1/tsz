//! Precise-anchor TS2694 for a `@return`/`@returns` `import("./mod").Member`
//! type expression.
//!
//! Split out of `function_type_helpers.rs`/`function_type.rs` (both over the
//! per-file line budget) to keep this one rule — shared by every call site
//! that resolves a JSDoc return-type expression — in a single place instead
//! of duplicated inline at each site (#17193).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// The JSDoc return-type context for `func_idx`'s `@return`/`@returns`
    /// tag: `@type {() => T}` on the function itself takes precedence and is
    /// resolved by the caller; this handles the plain `@returns {T}` tag,
    /// giving a bare `import("./mod").Member` reference the precise
    /// member-token anchor (#17193).
    pub(crate) fn jsdoc_return_context_for_function(
        &mut self,
        func_idx: NodeIndex,
        func_jsdoc: Option<&str>,
    ) -> Option<TypeId> {
        let expr = Self::jsdoc_returns_type_expression(func_jsdoc?)?;
        let comment_start = self.get_jsdoc_comment_pos_for_function(func_idx);
        self.resolve_jsdoc_return_type_import_member(&expr, comment_start)
            .or_else(|| self.resolve_jsdoc_reference(&expr))
    }

    /// Resolve a `@return`/`@returns` type expression that may be a bare
    /// (non-`typeof`) `import("./mod").Member` reference, anchoring a
    /// failure's TS2694 at the member-name token via the literal `@returns
    /// {expr}` (or `@return {expr}`) tag text — matching tsc, instead of the
    /// coarse `jsdoc_typedef_anchor_pos` fallback `resolve_jsdoc_reference`
    /// uses for every other shape.
    ///
    /// Returns `None` when `type_expr` is not this shape at all (including
    /// the memberless `import("./mod")` form) — the caller falls through to
    /// its own generic resolver.
    pub(crate) fn resolve_jsdoc_return_type_import_member(
        &mut self,
        type_expr: &str,
        comment_start: Option<u32>,
    ) -> Option<TypeId> {
        let result = self.resolve_jsdoc_import_type_member_result(type_expr)?;
        Some(match result {
            Ok(ty) => ty,
            Err((namespace_display, member_name)) => {
                self.emit_jsdoc_return_type_import_member_error(
                    comment_start,
                    type_expr,
                    &namespace_display,
                    &member_name,
                );
                TypeId::ANY
            }
        })
    }

    fn emit_jsdoc_return_type_import_member_error(
        &mut self,
        comment_start: Option<u32>,
        type_expr: &str,
        namespace_display: &str,
        member_name: &str,
    ) {
        let message = format_message(
            diagnostic_messages::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
            &[namespace_display, member_name],
        );
        let member_offset = type_expr
            .find(&format!(".{member_name}"))
            .map_or(0, |offset| offset + 1);
        let source_start = self
            .ctx
            .arena
            .source_files
            .first()
            .and_then(|source_file| {
                let source_text = source_file.text.as_ref();
                ["@returns {", "@return {"].into_iter().find_map(|tag| {
                    source_text
                        .find(&format!("{tag}{type_expr}}}"))
                        .map(|offset| offset + tag.len())
                })
            })
            .map(|offset| offset + member_offset);
        let Some(start) = source_start.map(|offset| offset as u32).or(comment_start) else {
            return;
        };
        let length = member_name.len() as u32;
        let already_reported = self.ctx.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER
                && diagnostic.start == start
                && diagnostic.length == length
                && diagnostic.message_text == message
        });
        if !already_reported {
            self.error_at_position(
                start,
                length,
                &message,
                diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
            );
        }
    }
}
