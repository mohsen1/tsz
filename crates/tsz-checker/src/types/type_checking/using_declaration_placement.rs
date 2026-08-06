//! Grammar for `using` / `await using` declaration lists: the TS1545-TS1548
//! placement rules and the `await using` module/target/async-context family
//! (TS2852, TS2853, TS2854, TS1309).
//!
//! Extracted from `core.rs` to keep module size manageable; the entry points
//! (`check_grammar_using_declaration_placement`, `check_await_using_context`)
//! are both driven from `check_variable_declaration_list_with_request` there.

use super::core_statement_checks::TopLevelAwaitVerdict;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    /// TS2852/TS2853/TS2854/TS1309: the module/target/async-context grammar for
    /// an `await using` declaration list, once its TS1545-family placement has
    /// been cleared. The caller gates this on the list actually being an
    /// `await using` (the `USING`-with-`CONST` flag pair); `placement_error`
    /// carries whether `check_grammar_using_declaration_placement` already
    /// reported, which suppresses this family the way tsc's early return does.
    ///
    /// tsc's `checkGrammarAwaitOrAwaitUsing` opens with the same
    /// containing-function-or-class-static-block test that `checkAwaitExpression`
    /// does, and this whole family lives in its `else` — so a class static block
    /// short-circuits the top-level-eligibility question outright, answering only
    /// TS18054 (`'await using'` cannot be used inside a class static block,
    /// emitted by the parser grammar). Without the
    /// `await_container_is_class_static_block` gate the top-level walk
    /// (`is_directly_at_source_file_top_level`) climbs past the static block to
    /// the source file and spuriously fires TS2853; adding the block to that
    /// walk's disqualifying list instead would only swap TS2853 for the nested
    /// TS2852 arm, so the container short-circuit is the tsc-faithful fix,
    /// mirroring `check_await_expression`'s gate for the bare-`await` family.
    /// Before #16597 this was reached only when TS18054 still set
    /// `has_syntax_parse_errors`; TS18054 is now non-suppressing, so the decline
    /// is made here on its own.
    pub(super) fn check_await_using_context(&mut self, list_idx: NodeIndex, placement_error: bool) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if placement_error
            || self.ctx.has_syntax_parse_errors
            || self.await_container_is_class_static_block(list_idx)
        {
            return;
        }

        // Same top-level-await-eligibility predicate as `check_await_expression`
        // (#16072): a namespace body disqualifies `await using` from being top
        // level without being function-like, so this is not `function_depth == 0`.
        if self.is_directly_at_source_file_top_level(list_idx) {
            // TS2853: Top-level 'await using' is only valid in modules.
            if self.top_level_await_requires_module_diagnostic() {
                self.error_at_node(
                    list_idx,
                    diagnostic_messages::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FIL,
                    diagnostic_codes::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FIL,
                );
            }

            // TS1309 when a Node module kind pairs with a CommonJS-format file;
            // otherwise TS2854, which requires specific module + target options.
            // Both answers come from the shared `checkGrammarAwaitOrAwaitUsing`
            // switch, which routes the module/target half through the environment
            // capability boundary.
            match self.top_level_await_verdict() {
                TopLevelAwaitVerdict::Allowed => {}
                TopLevelAwaitVerdict::CommonJsFile => {
                    self.error_at_node(
                        list_idx,
                        diagnostic_messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_AND_CANNOT_USE_AWAIT_AT_THE_TOP_LEVEL,
                        diagnostic_codes::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_AND_CANNOT_USE_AWAIT_AT_THE_TOP_LEVEL,
                    );
                }
                TopLevelAwaitVerdict::UnsupportedModuleOrTarget => {
                    self.error_at_node(
                        list_idx,
                        diagnostic_messages::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                        diagnostic_codes::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                    );
                }
            }
        } else if !self.enclosing_function_allows_await_using(list_idx) {
            // TS2852: Nested 'await using' is only valid inside async functions.
            self.error_at_node(
                list_idx,
                diagnostic_messages::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE,
                diagnostic_codes::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE,
            );
        }
    }

    /// Whether the nearest enclosing function of `idx` is `async`, which is what
    /// licenses a non-top-level `await using` (the TS2852 negative arm).
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
