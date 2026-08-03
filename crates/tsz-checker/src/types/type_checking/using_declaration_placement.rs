//! Placement grammar for `using` / `await using` declaration lists —
//! TS1545, TS1546, TS1547, TS1548.
//!
//! Extracted from `core.rs` to keep module size manageable; the single caller is
//! `check_variable_declaration_list_with_request` there.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    /// TS1545/TS1546/TS1547/TS1548: where a `using` / `await using` declaration
    /// list is allowed to stand.
    ///
    /// Both rules are keyed on the *list*, not on the declaration or the keyword:
    /// an ambient context rejects the form outright, and a `case`/`default` clause
    /// rejects it unless a block intervenes. tsc anchors either diagnostic on the
    /// whole list (`using a = null`), reports at most one, and returns — which is
    /// why the caller gates the `await using` placement family on the result.
    ///
    /// Returns `true` when a diagnostic was reported.
    pub(super) fn check_grammar_using_declaration_placement(
        &mut self,
        list_idx: NodeIndex,
        is_await_using: bool,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // `checkGrammarModifiers` runs ahead of the list grammar and returns as
        // soon as it reports, so a rejected modifier suppresses the placement
        // rules entirely. Every modifier is rejected on a `using` / `await using`
        // declaration — TS1491/TS1495 interpolate whichever one was written — so
        // the predicate is simply "the statement carries a modifier", not a list
        // of specific keywords. Without this, `declare using y: null;` draws
        // TS1491 *and* TS1545 where tsc draws TS1491 alone.
        if self.using_declaration_statement_has_modifiers(list_idx) {
            return false;
        }

        if self.ctx.is_ambient_declaration(list_idx) {
            let (message, code) = if is_await_using {
                (
                    diagnostic_messages::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                    diagnostic_codes::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                )
            } else {
                (
                    diagnostic_messages::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                    diagnostic_codes::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                )
            };
            self.error_at_node(list_idx, message, code);
            return true;
        }

        if self.is_declaration_list_directly_in_case_or_default_clause(list_idx) {
            let (message, code) = if is_await_using {
                (
                    diagnostic_messages::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTA,
                    diagnostic_codes::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTA,
                )
            } else {
                (
                    diagnostic_messages::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_W,
                    diagnostic_codes::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_W,
                )
            };
            self.error_at_node(list_idx, message, code);
            return true;
        }

        false
    }

    /// Whether the variable statement wrapping this declaration list carries any
    /// modifier. No modifier is legal on a `using` / `await using` declaration, so
    /// the presence of one means `checkGrammarModifiers` already reported
    /// TS1491/TS1495 and tsc never reached the list-level placement grammar.
    fn using_declaration_statement_has_modifiers(&self, list_idx: NodeIndex) -> bool {
        let Some(list_ext) = self.ctx.arena.get_extended(list_idx) else {
            return false;
        };
        let Some(statement_node) = self.ctx.arena.get(list_ext.parent) else {
            return false;
        };
        if statement_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        self.ctx
            .arena
            .get_variable(statement_node)
            .and_then(|statement| statement.modifiers.as_ref())
            .is_some_and(|modifiers| !modifiers.nodes.is_empty())
    }

    /// Whether a variable declaration list is the direct child of a `case` or
    /// `default` clause's statement list — i.e. `case 1: using a = null;` with no
    /// intervening block. A list nested inside a block (`case 1: { using a = null; }`)
    /// is legal and must not answer `true`; so must a `for (using a of ...)` head,
    /// whose parent is the loop rather than a variable statement.
    fn is_declaration_list_directly_in_case_or_default_clause(&self, list_idx: NodeIndex) -> bool {
        let Some(list_ext) = self.ctx.arena.get_extended(list_idx) else {
            return false;
        };
        let statement_idx = list_ext.parent;
        if self
            .ctx
            .arena
            .get(statement_idx)
            .is_none_or(|node| node.kind != syntax_kind_ext::VARIABLE_STATEMENT)
        {
            return false;
        }
        let Some(statement_ext) = self.ctx.arena.get_extended(statement_idx) else {
            return false;
        };
        self.ctx
            .arena
            .get(statement_ext.parent)
            .is_some_and(|node| {
                node.kind == syntax_kind_ext::CASE_CLAUSE
                    || node.kind == syntax_kind_ext::DEFAULT_CLAUSE
            })
    }
}
