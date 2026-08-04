//! Placement grammar for `using` / `await using` declaration lists —
//! TS1545, TS1546, TS1547, TS1548, TS18054.
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

    /// TS18054/TS2852: where a non-top-level `await using` statement is
    /// allowed to stand, once the top-level-eligibility branch in the caller
    /// has already been ruled out.
    ///
    /// Checks the static-block placement first — see
    /// `check_grammar_await_using_static_block_placement` for why that must
    /// run ahead of, and independently of, the enclosing-function check.
    pub(super) fn check_grammar_await_using_nested_placement(&mut self, list_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if self.check_grammar_await_using_static_block_placement(list_idx) {
            return;
        }
        if !self.enclosing_function_allows_await_using(list_idx) {
            // TS2852: Nested 'await using' is only valid inside async functions.
            self.error_at_node(
                list_idx,
                diagnostic_messages::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE,
                diagnostic_codes::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE,
            );
        }
    }

    fn enclosing_function_allows_await_using(&self, idx: NodeIndex) -> bool {
        let Some(function_idx) = self.find_enclosing_function(idx) else {
            return false;
        };
        let Some(node) = self.ctx.arena.get(function_idx) else {
            return false;
        };

        self.ctx
            .arena
            .get_function(node)
            .is_some_and(|function| function.is_async)
            || self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|method| self.has_async_modifier(&method.modifiers))
            || self
                .ctx
                .arena
                .get_accessor(node)
                .is_some_and(|accessor| self.has_async_modifier(&accessor.modifiers))
    }

    /// TS18054: `await using` statements cannot be used inside a class static
    /// block. Returns `true` when the diagnostic was reported.
    ///
    /// `find_enclosing_static_block` stops at the first function boundary, so
    /// this fires only when the `await using` statement sits directly inside
    /// the static block (or under further blocks/control flow within it) with
    /// no intervening function — a nested async function inside the static
    /// block leaves this returning `false`, letting the caller's ordinary
    /// `enclosing_function_allows_await_using` check take over, exactly as
    /// `check_for_await_statement`'s TS18038 already does for `for await`.
    ///
    /// An outer async function does NOT suppress this: a static block is
    /// never itself async, so `enclosing_function_allows_await_using` must
    /// not be consulted first. Oracle-confirmed (`typescript@7.0.2`):
    /// `async function outer() { class C { static { await using x = y; } } }`
    /// still reports TS18054.
    pub(super) fn check_grammar_await_using_static_block_placement(
        &mut self,
        list_idx: NodeIndex,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if self.find_enclosing_static_block(list_idx).is_none() {
            return false;
        }
        self.error_at_node(
            list_idx,
            diagnostic_messages::AWAIT_USING_STATEMENTS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
            diagnostic_codes::AWAIT_USING_STATEMENTS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
        );
        true
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
