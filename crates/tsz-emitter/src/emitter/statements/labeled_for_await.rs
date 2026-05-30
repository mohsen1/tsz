//! Label placement for downleveled labeled `for await...of` loops.
//!
//! When a `LabeledStatement` directly labels a `for await...of` loop that is
//! downleveled below ES2018, the loop is rewritten into a
//! `try { for (...) {...} } catch {...} finally {...}` shape (see
//! `emit_for_of_statement_es5_async_iterator`). tsc places the label on the
//! inner lowered `for` loop -- the actual iteration statement -- so that
//! `continue <label>` / `break <label>` target a real iteration statement.
//!
//! Attaching the label to the wrapping `try` instead produces non-runnable
//! JavaScript: `continue <label>` then refers to a label that does not denote
//! an iteration statement, which is a `SyntaxError` at parse time.
//!
//! The decision is keyed entirely on AST node kind (a `LabeledStatement` whose
//! body is an `await`-modified `ForOfStatement` that will be downleveled), never
//! on identifier spelling or rendered output, so it generalizes across label
//! names, binding names, and iterable expressions.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// True when a labeled statement's body is a non-declare enum that the
    /// emitter wraps in a block (`label: { ... }`). Used by
    /// `emit_labeled_statement`; lives here to keep `control_flow.rs` within the
    /// emitter file-size ratchet.
    pub(in crate::emitter) fn labeled_body_needs_block(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
            return false;
        }
        let Some(enum_decl) = self.arena.get_enum(stmt_node) else {
            return false;
        };
        if self.arena.is_declare(&enum_decl.modifiers) {
            return false;
        }
        !self
            .arena
            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
            || self.ctx.options.preserve_const_enums
    }

    /// True when `stmt_idx` is a `for await...of` loop that will be downleveled
    /// into the `try {...} finally {...}` async-iterator form, so that a label
    /// on it must move onto the inner lowered `for` loop rather than the
    /// wrapping `try`.
    ///
    /// This mirrors the lowering decision in `lowering::core` (a `for await`
    /// loop whose target does not natively support ES2018 for-await-of).
    pub(in crate::emitter) fn labeled_for_await_downlevels_to_try(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_in_of) = self.arena.get_for_in_of(stmt_node) else {
            return false;
        };
        for_in_of.await_modifier && !self.ctx.options.target.supports_es2018()
    }

    /// When `stmt_idx` (the body of a `LabeledStatement`) is a downleveled
    /// `for await...of`, emit the loop without writing the label here so the
    /// label moves onto the inner lowered `for`. Returns `true` when it handled
    /// the statement (the caller must then return early); `false` otherwise so
    /// the caller emits the label as usual.
    ///
    /// The async-iterator lowering reads the labeled-statement parent and emits
    /// `<label>: ` directly before the inner `for` via
    /// `emit_downlevel_for_await_loop_label`.
    pub(in crate::emitter) fn try_emit_downleveled_labeled_for_await(
        &mut self,
        stmt_idx: NodeIndex,
    ) -> bool {
        if !self.labeled_for_await_downlevels_to_try(stmt_idx) {
            return false;
        }
        self.emit(stmt_idx);
        true
    }

    /// If `for_of_idx` is the body of a `LabeledStatement`, emit that label
    /// (`<label>: `) here so it attaches to the inner lowered `for` loop that
    /// follows. Returns silently when the for-of is not directly labeled.
    ///
    /// Pairs with the label suppression in `emit_labeled_statement`, which skips
    /// writing the label before the wrapping `try` when the labeled body is a
    /// downleveled for-await-of.
    pub(in crate::emitter) fn emit_downlevel_for_await_loop_label(
        &mut self,
        for_of_idx: NodeIndex,
    ) {
        let Some(parent_idx) = self.arena.parent_of(for_of_idx) else {
            return;
        };
        let Some(parent_node) = self.arena.get(parent_idx) else {
            return;
        };
        if parent_node.kind != syntax_kind_ext::LABELED_STATEMENT {
            return;
        }
        let Some(labeled) = self.arena.get_labeled_statement(parent_node) else {
            return;
        };
        // Guard against an unexpected parent whose labeled body is some other
        // statement (defensive; the parent kind already implies this loop is
        // the labeled body for a direct `label: for await (...)`).
        if labeled.statement != for_of_idx {
            return;
        }
        self.emit(labeled.label);
        self.write(": ");
    }
}
