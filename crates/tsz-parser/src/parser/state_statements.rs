//! Parser state - statement and declaration parsing methods
use super::state::{CONTEXT_FLAG_IN_BLOCK, IncrementalParseResult, ParserState};
use crate::parser::{
    NodeIndex, NodeList,
    node::{BlockData, QualifiedNameData, SourceFileData, VariableData, VariableDeclarationData},
    syntax_kind_ext,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::{SyntaxKind, token_is_keyword};

impl ParserState {
    fn recover_invalid_statement_list_comma(&mut self) -> bool {
        if !self.is_token(SyntaxKind::CommaToken) {
            return false;
        }

        self.parse_error_at_current_token(
            "Declaration or statement expected.",
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        );
        self.next_token();
        true
    }

    fn look_ahead_is_invalid_shebang(&mut self) -> bool {
        if !self.is_token(SyntaxKind::HashToken) || self.token_pos() == 0 {
            return false;
        }
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = self.is_token(SyntaxKind::ExclamationToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    fn recover_invalid_shebang_line(&mut self) {
        let start = self.u32_from_usize(self.token_pos() as usize);
        self.parse_error_at(
            start,
            2,
            "'#!' can only be used at the start of a file.",
            diagnostic_codes::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE,
        );
        self.next_token(); // consume '#', then let `!` start normal expression recovery
    }

    fn recover_invalid_shebang_token(&mut self) {
        let start = self.u32_from_usize(self.token_pos() as usize);
        self.parse_error_at(
            start,
            2,
            "'#!' can only be used at the start of a file.",
            diagnostic_codes::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE,
        );
        self.next_token(); // consume '#'
    }

    fn recover_after_unknown_token(
        &mut self,
        previous_statement_was_block: &mut bool,
        resync_after_unknown: bool,
    ) -> bool {
        if !self.is_token(SyntaxKind::Unknown) {
            return false;
        }

        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
            diagnostic_codes::INVALID_CHARACTER,
        );
        self.next_token();

        if self.is_token(SyntaxKind::EqualsToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
            *previous_statement_was_block = false;
            return true;
        }

        if self.is_identifier_or_keyword() && self.look_ahead_next_is_open_brace_on_same_line() {
            self.parse_error_at_current_token(
                "Unexpected keyword or identifier.",
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            );
            self.next_token();
            *previous_statement_was_block = false;
            return true;
        }

        if resync_after_unknown {
            self.resync_after_error_with_statement_starts(false);
        }
        *previous_statement_was_block = false;
        true
    }

    // =========================================================================
    // Parse Methods - Core Expressions
    // =========================================================================

    /// Parse a source file
    pub fn parse_source_file(&mut self) -> NodeIndex {
        let start_pos = 0u32;

        // Skip shebang (#!) if present at start of file
        self.scanner.scan_shebang_trivia();

        // Initialize scanner
        self.next_token();

        // Parse statements (using source file version that handles stray braces)
        let statements = self.parse_source_file_statements();

        // Cache comment ranges once during parsing (O(N) scan, done only once)
        // This avoids rescanning on every hover/documentation request
        // Use scanner's source text (no duplicate allocation)
        let comments = tsz_common::comments::get_comment_ranges(self.scanner.source_text());

        // Collect scanner-level diagnostics (e.g., conflict markers TS1185) into
        // parse diagnostics so they appear in the final diagnostic output.
        for diag in self.scanner.get_scanner_diagnostics() {
            let mut message = diag.message.to_string();
            for (idx, arg) in diag.args.iter().enumerate() {
                let placeholder = format!("{{{idx}}}");
                message = message.replace(&placeholder, arg);
            }
            self.parse_diagnostics.push(super::state::ParseDiagnostic {
                start: self.u32_from_usize(diag.pos),
                length: self.u32_from_usize(diag.length),
                message,
                code: diag.code,
            });
        }
        // Sort diagnostics by position to maintain correct order after merging
        self.parse_diagnostics.sort_by_key(|d| d.start);

        // Create source file node
        let end_pos = self.token_end();
        let eof_token = self
            .arena
            .add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);

        // Transfer the scanner's string interner to the arena so that atom-based
        // identifier text resolution works via get_arena() (not just into_arena()).
        // This is essential for LSP features that resolve identifier references.
        self.arena.set_interner(self.scanner.interner().clone());

        self.arena.add_source_file(
            start_pos,
            end_pos,
            SourceFileData {
                statements,
                end_of_file_token: eof_token,
                file_name: self.file_name.clone(),
                text: self.scanner.source_text_arc(),
                language_version: u32::from(self.language_version.ts_numeric_value()),
                language_variant: 0,
                script_kind: 3,
                is_declaration_file: self.is_declaration_file(),
                has_no_default_lib: false,
                comments, // Cached comment ranges
                parent: NodeIndex::NONE,
                id: 0,
                modifier_flags: 0,
                transform_flags: 0,
            },
        )
    }

    pub fn parse_source_file_statements_from_offset(
        &mut self,
        file_name: String,
        source_text: String,
        start: u32,
    ) -> IncrementalParseResult {
        let start = usize::min(start as usize, source_text.len());
        let reparse_start = self.u32_from_usize(start);

        self.file_name = file_name;
        self.scanner.set_text(source_text, Some(start), None);
        self.scanner.set_language_version(self.language_version);
        self.context_flags = 0;
        self.current_token = SyntaxKind::Unknown;
        self.parse_diagnostics.clear();
        self.recursion_depth = 0;

        self.next_token();
        let statements = self.parse_source_file_statements();
        let end_pos = self.token_end();
        let eof_token = self
            .arena
            .add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);

        // Refresh the arena's interner with the scanner's so any identifier
        // newly interned during this suffix parse is resolvable through the
        // arena. Without this, `NodeArena::resolve_identifier_text` silently
        // returns "" for atoms past the prior parse's tail, corrupting
        // binder, LSP, and diagnostic identifier text. Mirrors the symmetric
        // sync at the end of `parse_source_file`.
        self.arena.set_interner(self.scanner.interner().clone());

        IncrementalParseResult {
            statements,
            end_pos,
            end_of_file_token: eof_token,
            reparse_start,
        }
    }

    /// Parse list of statements for a source file (top-level).
    /// Reports error 1128 for unexpected closing braces.
    /// Uses resynchronization to recover from errors and continue parsing.
    pub(crate) fn parse_source_file_statements(&mut self) -> NodeList {
        let mut statements = Vec::new();
        let mut skip_after_binary_payload = false;
        let mut previous_statement_was_block = false;
        // True only when the previous statement was an ExpressionStatement whose
        // expression is an arrow/function-expression with a block body. tsc emits
        // an extra TS1005 (";' expected") at the start of the recovered token after
        // the `=` is consumed for this case (because the prior expression statement
        // still required a semicolon). Function/class declarations and other block
        // statements do NOT require a trailing `;`, so they skip the extra TS1005.
        let mut prev_block_needs_post_equals_semi = false;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            let pos_before = self.token_pos();
            if skip_after_binary_payload {
                break;
            }

            if self.look_ahead_is_invalid_shebang() {
                if self.scanner.has_preceding_line_break() {
                    self.recover_invalid_shebang_line();
                } else {
                    self.recover_invalid_shebang_token();
                }
                previous_statement_was_block = false;
                prev_block_needs_post_equals_semi = false;
                continue;
            }

            if previous_statement_was_block && self.is_token(SyntaxKind::EqualsToken) {
                self.parse_error_at_current_token(
                    "Declaration or statement expected. This '=' follows a block of statements, so if you intended to write a destructuring assignment, you might need to wrap the whole assignment in parentheses.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I,
                );
                self.next_token();
                if prev_block_needs_post_equals_semi && !self.is_token(SyntaxKind::EndOfFileToken) {
                    self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                }
                prev_block_needs_post_equals_semi = false;
                previous_statement_was_block = false;
                continue;
            }

            // Handle Unknown tokens (invalid characters) - must be checked FIRST.
            // In top-level lists we intentionally avoid resync here so each invalid
            // character still gets its own TS1127 instead of being skipped.
            if self.recover_after_unknown_token(&mut previous_statement_was_block, false) {
                continue;
            }

            // Handle bare `#` that can't become a PrivateIdentifier.
            // In tsc, the scanner emits TS1127 for a standalone `#` that is not
            // followed by a valid identifier character. We try to rescan as a
            // PrivateIdentifier; if that fails, emit TS1127 and skip.
            if self.is_token(SyntaxKind::HashToken) {
                let rescanned = self.scanner.re_scan_hash_token();
                if rescanned == SyntaxKind::PrivateIdentifier {
                    // Got a valid private identifier — let the normal statement
                    // parser handle it (it will likely fail with a meaningful error).
                    self.current_token = rescanned;
                } else {
                    // Bare `#` — emit TS1127 and skip, matching tsc.
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        diagnostic_codes::INVALID_CHARACTER,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }
            }

            // If we see a closing brace at the top level, report error 1128
            if self.is_token(SyntaxKind::CloseBraceToken) {
                // Only emit error if we haven't already emitted one at this position
                if self.token_pos() != self.last_error_pos {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                if self.deferred_module_close_braces > 0 {
                    self.deferred_module_close_braces -= 1;
                }
                self.next_token();
                previous_statement_was_block = false;
                // If the token after a stray top-level `}` already starts a
                // statement or expression, keep parsing there instead of
                // resyncing past it. This preserves follow-up recovery like
                // `from "./foo"` -> TS1434 in malformed import/export
                // specifiers, and avoids skipping valid declarations after a
                // brace recovered from a malformed arrow body.
                if !self.is_statement_start()
                    && !self.is_expression_start()
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                {
                    self.resync_after_error();
                }
                continue;
            }

            if self.is_token(SyntaxKind::AtToken) {
                let snapshot = self.scanner.save_state();
                let at_token = self.current_token;
                self.next_token();
                if self.is_token(SyntaxKind::Unknown) {
                    // Current token is the Unknown (e.g. \x04) right after @.
                    // Report TS1127 at this position, not after re-scanning.
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                    );

                    self.next_token();
                    if !self.is_token(SyntaxKind::EndOfFileToken) {
                        self.parse_error_at_current_token(
                            "Declaration or statement expected.",
                            tsz_common::diagnostics::diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                        );
                    }

                    skip_after_binary_payload = true;
                    previous_statement_was_block = false;
                    continue;
                }
                self.scanner.restore_state(snapshot);
                self.current_token = at_token;
            }

            let statement_start_token = self.token();
            let stmt = self.parse_statement();
            if stmt.is_none() {
                if self.is_token(SyntaxKind::GreaterThanToken) {
                    let snapshot = self.scanner.save_state();
                    let current_token = self.current_token;
                    self.next_token();
                    let followed_by_expression = self.is_expression_start();
                    self.scanner.restore_state(snapshot);
                    self.current_token = current_token;
                    if followed_by_expression {
                        self.next_token();
                        continue;
                    }
                }

                if self.is_token(SyntaxKind::CloseParenToken)
                    && !self.scanner.has_preceding_line_break()
                {
                    let source = self.scanner.source_text().as_bytes();
                    let mut i = self.token_pos() as usize;
                    while i > 0 && source[i - 1].is_ascii_whitespace() {
                        i -= 1;
                    }
                    if i > 0 && source[i - 1] == b')' {
                        self.parse_error_at_current_token(
                            "';' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token();
                        continue;
                    }
                }

                if matches!(
                    self.token(),
                    SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                ) {
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }

                // Statement parsing failed, resync to recover
                // Suppress cascading errors when:
                // 1. A recent error was within 3 chars, OR
                // 2. The token is a closing bracket/paren that is likely a
                //    stray artifact from earlier bracket-mismatch errors.
                let current = self.token_pos();
                let is_stray_close = self.last_error_pos != 0
                    && matches!(
                        self.token(),
                        SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                    );
                if (self.last_error_pos == 0 || current.abs_diff(self.last_error_pos) > 3)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !is_stray_close
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                // Resync to next statement boundary to continue parsing
                let allow_statement_starts = if statement_start_token == SyntaxKind::AtToken {
                    false
                } else {
                    !self.is_statement_start()
                };
                self.resync_after_error_with_statement_starts(allow_statement_starts);
                previous_statement_was_block = false;
                prev_block_needs_post_equals_semi = false;
            } else {
                let mut needs_semi_after_equals = false;
                previous_statement_was_block = self.arena.get(stmt).is_some_and(|node| {
                    let kind = node.kind;
                    if kind == syntax_kind_ext::BLOCK
                        || kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || kind == syntax_kind_ext::CLASS_DECLARATION
                        || kind == syntax_kind_ext::IF_STATEMENT
                        || kind == syntax_kind_ext::FOR_STATEMENT
                        || kind == syntax_kind_ext::FOR_IN_STATEMENT
                        || kind == syntax_kind_ext::FOR_OF_STATEMENT
                        || kind == syntax_kind_ext::WHILE_STATEMENT
                        || kind == syntax_kind_ext::DO_STATEMENT
                        || kind == syntax_kind_ext::SWITCH_STATEMENT
                        || kind == syntax_kind_ext::TRY_STATEMENT
                        || kind == syntax_kind_ext::WITH_STATEMENT
                    {
                        return true;
                    }
                    // ExpressionStatement whose expression is an arrow
                    // function or function expression with a block body —
                    // tsc treats `() => { } = value;` and
                    // `(function () { }) = value;` like a block-following-`=`
                    // and emits TS2809 instead of TS1005. Unlike function/class
                    // declarations these still need a semicolon, so tsc emits
                    // TS1005 at the recovered token after consuming the `=`.
                    if kind == syntax_kind_ext::EXPRESSION_STATEMENT
                        && let Some(expr_stmt) = self.arena.get_expression_statement(node)
                        && let Some(inner) = self.arena.get(expr_stmt.expression)
                    {
                        let is_arrow_or_func = inner.is_function_expression_or_arrow();
                        if is_arrow_or_func {
                            needs_semi_after_equals = true;
                        }
                        return is_arrow_or_func;
                    }
                    false
                });
                prev_block_needs_post_equals_semi = needs_semi_after_equals;
                statements.push(stmt);
                self.drain_pending_recovered_expression_statements(&mut statements);
                if self.recover_invalid_statement_list_comma() {
                    previous_statement_was_block = false;
                    prev_block_needs_post_equals_semi = false;
                    continue;
                }
            }

            // Safety: if position didn't advance, force-skip the current token
            // to prevent infinite loop when resync returns at a sync point
            // that parse_statement can't handle
            if self.token_pos() == pos_before && !self.is_token(SyntaxKind::EndOfFileToken) {
                self.next_token();
            }
        }

        self.make_node_list(statements)
    }

    /// Parse list of statements (for blocks, function bodies, etc.).
    /// Stops at closing brace without error (closing brace is expected).
    /// Uses resynchronization to recover from errors and continue parsing.
    pub(crate) fn parse_statements(&mut self) -> NodeList {
        self.statement_list_depth += 1;
        let statement_list_depth = self.statement_list_depth;
        let mut statements = Vec::new();
        let mut previous_statement_was_block = false;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::CloseBraceToken) {
                if self.non_block_close_brace_statement_errors_remaining > 0
                    && !self.in_block_context()
                {
                    self.non_block_close_brace_statement_errors_remaining -= 1;
                    if self.non_block_close_brace_statement_errors_remaining == 0 {
                        self.suppress_missing_close_brace_at_eof_statement_depth =
                            Some(statement_list_depth.saturating_sub(1));
                    }
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }
                break;
            }

            let pos_before = self.token_pos();

            if self.look_ahead_is_invalid_shebang() {
                if self.scanner.has_preceding_line_break() {
                    self.recover_invalid_shebang_line();
                } else {
                    self.recover_invalid_shebang_token();
                }
                previous_statement_was_block = false;
                continue;
            }

            if previous_statement_was_block && self.is_token(SyntaxKind::EqualsToken) {
                self.parse_error_at_current_token(
                    "Declaration or statement expected. This '=' follows a block of statements, so if you intended to write a destructuring assignment, you might need to wrap the whole assignment in parentheses.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I,
                );
                self.next_token();
                previous_statement_was_block = false;
                continue;
            }

            if self.recover_orphan_case_assignment_before_if() {
                previous_statement_was_block = false;
                continue;
            }

            // Error recovery: when inside a nested block within a class body (e.g.,
            // a method body with an unclosed `{`), terminate the block if we encounter
            // a class member modifier followed by an identifier on the same line. This
            // matches TSC's "abort parsing list" behavior: tokens that could start a
            // class member in an outer context cause the inner block list to terminate
            // rather than consuming tokens that belong to the class body.
            if self.in_block_context()
                && self.in_class_body()
                && matches!(
                    self.token(),
                    SyntaxKind::PublicKeyword
                        | SyntaxKind::PrivateKeyword
                        | SyntaxKind::ProtectedKeyword
                        | SyntaxKind::StaticKeyword
                        | SyntaxKind::AbstractKeyword
                        | SyntaxKind::ReadonlyKeyword
                        | SyntaxKind::OverrideKeyword
                        | SyntaxKind::AccessorKeyword
                )
                && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                break;
            }

            // Handle Unknown tokens (invalid characters). Nested lists keep the
            // existing behavior of resyncing after the immediate recovery.
            if self.recover_after_unknown_token(&mut previous_statement_was_block, true) {
                continue;
            }

            let statement_start_token = self.token();
            let stmt = self.parse_statement();
            if stmt.is_none() {
                if matches!(
                    self.token(),
                    SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                ) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }

                // Statement parsing failed, resync to recover
                // Emit error if we haven't already at the exact same position
                // Suppress cascading errors when a recent error was within 3 chars
                let current = self.token_pos();
                if (self.last_error_pos == 0 || current.abs_diff(self.last_error_pos) > 3)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                // Resync to next statement boundary to continue parsing
                let allow_statement_starts = if statement_start_token == SyntaxKind::AtToken {
                    false
                } else {
                    !self.is_statement_start()
                };
                self.resync_after_error_with_statement_starts(allow_statement_starts);
                previous_statement_was_block = false;
            } else {
                previous_statement_was_block = self.arena.get(stmt).is_some_and(|node| {
                    let kind = node.kind;
                    if kind == syntax_kind_ext::EXPRESSION_STATEMENT
                        && let Some(expr_stmt) = self.arena.get_expression_statement(node)
                        && let Some(inner) = self.arena.get(expr_stmt.expression)
                        && inner.is_function_expression_or_arrow()
                    {
                        return true;
                    }
                    kind == syntax_kind_ext::BLOCK
                        || kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || kind == syntax_kind_ext::CLASS_DECLARATION
                        || kind == syntax_kind_ext::IF_STATEMENT
                        || kind == syntax_kind_ext::FOR_STATEMENT
                        || kind == syntax_kind_ext::FOR_IN_STATEMENT
                        || kind == syntax_kind_ext::FOR_OF_STATEMENT
                        || kind == syntax_kind_ext::WHILE_STATEMENT
                        || kind == syntax_kind_ext::DO_STATEMENT
                        || kind == syntax_kind_ext::SWITCH_STATEMENT
                        || kind == syntax_kind_ext::TRY_STATEMENT
                        || kind == syntax_kind_ext::WITH_STATEMENT
                });
                statements.push(stmt);
                self.drain_pending_recovered_expression_statements(&mut statements);
                if self.recover_invalid_statement_list_comma() {
                    previous_statement_was_block = false;
                    continue;
                }
            }

            // Safety: if position didn't advance, force-skip the current token
            // to prevent infinite loop when resync returns at a sync point
            // that parse_statement can't handle
            if self.token_pos() == pos_before
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
            {
                self.next_token();
            }
        }

        self.statement_list_depth -= 1;
        self.make_node_list(statements)
    }

    fn recover_orphan_case_assignment_before_if(&mut self) -> bool {
        if !self.is_token(SyntaxKind::CaseKeyword) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let has_same_line_equals =
            !self.scanner.has_preceding_line_break() && self.is_token(SyntaxKind::EqualsToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        if !has_same_line_equals {
            return false;
        }

        self.parse_error_at_current_token(
            "Declaration or statement expected.",
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        );
        while !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            self.next_token();
            if self.scanner.has_preceding_line_break() {
                break;
            }
        }
        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        }
        if self.is_token(SyntaxKind::IfKeyword) {
            self.report_orphan_case_following_if_header_recovery();
        }
        true
    }

    fn report_orphan_case_following_if_header_recovery(&mut self) {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        if !self.is_token(SyntaxKind::OpenParenToken) {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return;
        }

        let mut previous_expr_token: Option<(u32, u32)> = None;
        let mut first_operator_token: Option<(u32, u32)> = None;
        self.next_token();
        while !matches!(
            self.token(),
            SyntaxKind::CloseParenToken | SyntaxKind::EndOfFileToken
        ) {
            if first_operator_token.is_none() && self.is_binary_operator() {
                first_operator_token = Some((self.token_pos(), self.token_end()));
            }
            previous_expr_token = Some((self.token_pos(), self.token_end()));
            self.next_token();
        }
        if let Some((start, end)) = first_operator_token.or(previous_expr_token) {
            self.parse_error_at(
                start,
                end.saturating_sub(start),
                "',' expected.",
                diagnostic_codes::EXPECTED,
            );
        }
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token();
        }

        self.scanner.restore_state(snapshot);
        self.current_token = current;
    }

    /// Parse a statement
    pub fn parse_statement(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::OpenBraceToken => self.parse_block(),
            SyntaxKind::VarKeyword => self.parse_variable_statement(),
            SyntaxKind::UsingKeyword => {
                if self.look_ahead_is_using_declaration() {
                    self.parse_variable_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::LetKeyword => {
                // In strict mode (modules, classes, etc.), `let` is a reserved word and
                // cannot be used as an identifier. But `let;` or `let` followed by a
                // non-declaration-start token should NOT be parsed as a variable declaration.
                // tsc checks `isLetDeclaration()`: next token must be identifier, `{`, or `[`.
                if self.look_ahead_is_let_declaration() {
                    self.try_parse_invalid_let_array_declaration_statement()
                        .unwrap_or_else(|| self.parse_variable_statement())
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::ConstKeyword => {
                // const enum or const variable
                if self.look_ahead_is_const_enum() {
                    let start_pos = self.token_pos();
                    self.parse_const_enum_declaration(start_pos, Vec::new())
                } else {
                    self.parse_variable_statement()
                }
            }
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::AsyncKeyword => self.parse_statement_async_declaration_or_expression(),
            SyntaxKind::AwaitKeyword => {
                // await using declaration (ES2022)
                // Look ahead to see if it's "await using" or "await:" (labeled statement)
                if self.look_ahead_is_await_using_declaration() {
                    self.parse_variable_statement()
                } else if self.is_identifier_or_keyword() && self.look_ahead_is_labeled_statement()
                {
                    // 'await' as a label (e.g., "await: statement")
                    // In static blocks, 'await' is reserved and cannot be used as a label.
                    // tsc treats `await` as a keyword, tries to parse an await expression,
                    // and emits TS1109 "Expression expected." at the colon position.
                    if self.in_static_block_context() {
                        // Look ahead to get the colon position
                        let colon_pos = self.look_ahead_get_labeled_colon_pos();
                        self.parse_error_at(
                            colon_pos,
                            1,
                            "Expression expected.",
                            diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                    }
                    self.parse_labeled_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::AtToken => {
                if self.look_ahead_has_missing_decorator_expression() {
                    self.next_token();
                    self.error_expression_expected();
                    self.parse_statement()
                } else {
                    // Decorator: @decorator class/function
                    self.parse_decorated_declaration()
                }
            }
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::AbstractKeyword => self.parse_statement_abstract_keyword(),
            SyntaxKind::AccessorKeyword => self.parse_statement_accessor_keyword(),
            // Modifier keywords used before declarations at top level
            // e.g., `public interface I {}`, `protected class C {}`, `static class C {}`
            // These should emit TS1044 and then parse the declaration
            SyntaxKind::StaticKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::OverrideKeyword
            | SyntaxKind::ReadonlyKeyword => self.parse_statement_top_level_modifier(),
            SyntaxKind::DefaultKeyword => {
                // 'default' is only valid after 'export': emit TS1005 "'export' expected"
                self.parse_error_at_current_token("'export' expected.", diagnostic_codes::EXPECTED);
                self.next_token();
                self.parse_statement()
            }
            SyntaxKind::InterfaceKeyword => {
                // ASI: `interface\nI {}` should be parsed as expression statement
                // 'interface' followed by identifier 'I', not InterfaceDeclaration.
                if self.look_ahead_next_is_identifier_or_keyword_on_same_line()
                    || self.look_ahead_next_is_numeric_literal_on_same_line()
                {
                    self.parse_interface_declaration()
                } else if self.look_ahead_next_is_open_brace_on_same_line() {
                    // `interface { }` — parse as interface with missing name (TS1438)
                    // rather than as expression statement. Matches tsc behavior.
                    self.parse_interface_declaration()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::TypeKeyword => self.parse_statement_type_keyword(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::DeclareKeyword => {
                // Note: TS1184/TS1234/TS1235 for `declare` in block context are
                // handled by the checker's grammar checks (check_module_declaration,
                // check_grammar_module_element_context, etc.), not the parser.
                // The parser must NOT emit TS1184 here because that would set
                // has_syntax_parse_errors and suppress the checker's more specific
                // diagnostics (TS1234 for ambient modules, TS1235 for namespaces).
                self.parse_statement_declare_or_expression()
            }
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => self.parse_statement_namespace_or_expression(),
            SyntaxKind::IfKeyword => self.parse_if_statement(),
            SyntaxKind::ReturnKeyword => self.parse_return_statement(),
            SyntaxKind::WhileKeyword => self.parse_while_statement(),
            SyntaxKind::ForKeyword => self.parse_for_statement(),
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::ExportKeyword => {
                // Note: TS1184/TS1231/TS1233/TS1258 for `export` in block context
                // are handled by the checker's grammar checks
                // (check_grammar_module_element_context), not the parser.
                // The parser must NOT emit TS1184 here because that would set
                // has_syntax_parse_errors and suppress the checker's more specific
                // diagnostics.
                self.parse_export_declaration()
            }
            SyntaxKind::ImportKeyword => self.parse_statement_import_keyword(),
            SyntaxKind::BreakKeyword => self.parse_break_statement(),
            SyntaxKind::ContinueKeyword => self.parse_continue_statement(),
            SyntaxKind::ThrowKeyword => self.parse_throw_statement(),
            SyntaxKind::DoKeyword => self.parse_do_statement(),
            SyntaxKind::SwitchKeyword => self.parse_switch_statement(),
            SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                self.parse_try_statement()
            }
            SyntaxKind::WithKeyword => self.parse_with_statement(),
            SyntaxKind::DebuggerKeyword => self.parse_debugger_statement(),
            SyntaxKind::Identifier => {
                // Check for labeled statement: label: statement
                if self.look_ahead_is_labeled_statement() {
                    self.parse_labeled_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            _ => {
                // Check for labeled statement with keyword as label (e.g., await: if (...))
                // TypeScript/JavaScript allow reserved keywords as labels
                // This enables: await: ..., arguments: ..., eval: ..., etc.
                if self.is_identifier_or_keyword() && self.look_ahead_is_labeled_statement() {
                    self.parse_labeled_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
        }
    }

    pub(crate) fn parse_entity_name_inner(&mut self, allow_reserved_words: bool) -> NodeIndex {
        // Handle 'this' keyword as a valid start for typeof expressions
        let mut left = if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
        } else if allow_reserved_words {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        while self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier_name(); // Use identifier_name to allow keywords as property names
            let start_pos = if let Some(node) = self.arena.get(left) {
                node.pos
            } else {
                0
            };
            let end_pos = self.token_full_start();

            left = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start_pos,
                end_pos,
                QualifiedNameData { left, right },
            );
        }

        left
    }

    /// Parse async function declaration
    pub(crate) fn parse_async_function_declaration(&mut self) -> NodeIndex {
        // TS1040: 'async' modifier cannot be used in an ambient context
        if (self.context_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) != 0 {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "'async' modifier cannot be used in an ambient context.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
            );
        }
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_function_declaration_with_async(true, None)
    }

    /// Parse a block statement
    pub(crate) fn parse_block(&mut self) -> NodeIndex {
        // Check recursion limit to prevent stack overflow on deeply nested code
        if !self.enter_recursion() {
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        let statements = if self.parse_expected(SyntaxKind::OpenBraceToken) {
            // Set IN_BLOCK flag so that modifiers like export/declare emit TS1184
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_IN_BLOCK;

            let stmts = self.parse_statements();

            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
            stmts
        } else {
            self.make_node_list(Vec::new())
        };
        let end_pos = self.token_end();

        self.exit_recursion();

        self.arena.add_block(
            syntax_kind_ext::BLOCK,
            start_pos,
            end_pos,
            BlockData {
                statements,
                multi_line: true,
            },
        )
    }

    /// Parse empty statement
    pub(crate) fn parse_empty_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        self.parse_expected(SyntaxKind::SemicolonToken);
        let end_pos = self.token_end();

        self.arena
            .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos)
    }

    /// Parse variable statement (var/let/const)
    pub(crate) fn parse_variable_statement(&mut self) -> NodeIndex {
        self.parse_variable_statement_with_modifiers(None, None)
    }

    /// Parse variable statement with optional start position and modifiers (for declare statements)
    pub(crate) fn parse_variable_statement_with_modifiers(
        &mut self,
        override_start_pos: Option<u32>,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = override_start_pos.unwrap_or_else(|| self.token_pos());
        let declaration_list = self.parse_variable_declaration_list();
        self.parse_semicolon();
        let end_pos = self.token_full_start();

        self.arena.add_variable(
            syntax_kind_ext::VARIABLE_STATEMENT,
            start_pos,
            end_pos,
            VariableData {
                modifiers,
                declarations: self.make_node_list(vec![declaration_list]),
            },
        )
    }

    /// Parse variable declaration list
    pub(crate) fn parse_variable_declaration_list(&mut self) -> NodeIndex {
        use crate::parser::node_flags;

        let start_pos = self.token_pos();

        // Consume var/let/const/using/await using and get flags
        // Use consume_keyword() for TS1260 check (keywords cannot contain escape characters)
        let flags: u16 = match self.token() {
            SyntaxKind::LetKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::LET)
            }
            SyntaxKind::ConstKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::CONST)
            }
            SyntaxKind::UsingKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::USING)
            }
            SyntaxKind::AwaitKeyword => {
                // await using declaration
                self.consume_keyword(); // consume 'await'
                self.parse_expected(SyntaxKind::UsingKeyword); // consume 'using'
                self.u16_from_node_flags(node_flags::AWAIT_USING)
            }
            _ => {
                self.consume_keyword(); // var
                0
            }
        };

        // Parse declarations with enhanced error recovery
        let mut declarations = Vec::new();
        let mut had_decl_expected_error = false;
        loop {
            // Check if we can start a variable declaration
            // Can be: identifier, keyword as identifier, or binding pattern (object/array)
            let starts_recovered_invalid_unicode_identifier =
                self.current_unknown_starts_invalid_unicode_identifier_debris();
            let can_start_decl = self.is_identifier_or_keyword()
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken)
                || starts_recovered_invalid_unicode_identifier;

            if !can_start_decl {
                if self.is_token(SyntaxKind::Unknown) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        diagnostic_codes::INVALID_CHARACTER,
                    );
                    self.next_token();

                    if self.is_identifier_or_keyword() && !self.is_reserved_word() {
                        continue;
                    }

                    if self.is_token(SyntaxKind::ColonToken) {
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        while !matches!(
                            self.token(),
                            SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        ) {
                            self.next_token();
                        }
                        had_decl_expected_error = true;
                    }
                    break;
                }

                // Invalid token for variable declaration - emit error and recover
                if !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_token(SyntaxKind::Unknown)
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                    had_decl_expected_error = true;
                }
                break;
            }

            let decl_started_at_numeric_literal_follow_error =
                self.current_token_has_numeric_literal_follow_error();
            let diag_count_before_decl = self.parse_diagnostics.len();
            let decl = self.parse_variable_declaration_with_flags(flags);
            let decl_had_error = self.parse_diagnostics.len() > diag_count_before_decl;
            // A declarator with ONLY numeric-literal-value errors (TS1121
            // legacy octal, TS1352/TS1353 bigint form, TS1489 leading-zero
            // decimal, etc.) is structurally complete — only the literal's
            // value is illegal. The next token can still kick off a missing
            // -comma recovery for the declaration list. Track this so the
            // post-decl loop can distinguish from genuine declarator-shape
            // errors (malformed name, malformed initializer expression).
            let decl_only_literal_value_errors = decl_had_error
                && {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_diagnostics[diag_count_before_decl..]
                    .iter()
                    .all(|d| {
                        matches!(
                            d.code,
                            diagnostic_codes::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX
                                | diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED
                                | diagnostic_codes::BINARY_DIGIT_EXPECTED
                                | diagnostic_codes::OCTAL_DIGIT_EXPECTED
                                | diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL
                                | diagnostic_codes::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION
                                | diagnostic_codes::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER
                                | diagnostic_codes::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED
                                | diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE
                                | diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED
                        )
                    })
                };
            declarations.push(decl);

            let comma_pos = self.token_pos();
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // If ASI applies (line break, closing brace, EOF, or semicolon),
                // just break - parse_semicolon() in the caller will handle it
                if self.can_parse_semicolon() {
                    break;
                }

                if self.is_token(SyntaxKind::ColonToken) {
                    use tsz_common::diagnostics::diagnostic_codes;

                    let recover_invalid_jsx_namespace_head =
                        self.recover_jsx_invalid_namespace_head_tail;
                    if self.recover_jsx_closing_tag_extra_namespace_tail
                        || recover_invalid_jsx_namespace_head
                    {
                        let snapshot = self.scanner.save_state();
                        let current = self.current_token;
                        self.next_token();
                        let colon_followed_by_declaration =
                            self.is_identifier_or_keyword() && !self.is_reserved_word();
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;

                        if colon_followed_by_declaration {
                            self.next_token();
                            if recover_invalid_jsx_namespace_head {
                                self.recover_jsx_invalid_namespace_head_tail = false;
                            }
                            continue;
                        }
                    }

                    let use_failed_async_arrow_recovery =
                        self.pending_failed_async_arrow_colon_recovery;
                    self.pending_failed_async_arrow_colon_recovery = false;

                    self.error_comma_expected();
                    self.next_token();

                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        self.parse_error_at_current_token(
                            "';' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        break;
                    }

                    let generic_like_type_arg_pos =
                        if use_failed_async_arrow_recovery && self.is_identifier_or_keyword() {
                            let snapshot = self.scanner.save_state();
                            let current = self.current_token;
                            self.next_token();
                            let result = self
                                .is_token(SyntaxKind::LessThanToken)
                                .then(|| self.token_pos());
                            self.scanner.restore_state(snapshot);
                            self.current_token = current;
                            result
                        } else {
                            None
                        };

                    let recover_start = self.token_pos();
                    let _ = self.parse_type();
                    if self.token_pos() == recover_start
                        && !matches!(
                            self.token(),
                            SyntaxKind::CommaToken
                                | SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        )
                    {
                        self.next_token();
                    }

                    if let Some(pos) = generic_like_type_arg_pos {
                        self.parse_error_at(pos, 1, "',' expected.", diagnostic_codes::EXPECTED);
                    }

                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        if use_failed_async_arrow_recovery {
                            self.error_expression_expected();
                        } else {
                            self.parse_error_at_current_token(
                                "';' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                        }
                    }
                    break;
                }

                // `=>` after a declaration is never a valid comma separator.
                // Break silently so parse_semicolon() in the caller can emit
                // "';' expected." at the `=` position, matching tsc's diagnostic.
                // Example: `var tt = (a, (b, c)) => ...` — rejected arrow function.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    if self.in_static_block_context() {
                        let arrow_pos = self.token_pos();
                        let already_reported_expression_at_arrow =
                            self.parse_diagnostics.last().is_some_and(|diag| {
                                diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                                    && diag.start == arrow_pos
                            });
                        if !already_reported_expression_at_arrow {
                            self.parse_error_at_current_token(
                                "';' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                        }
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenBraceToken) {
                            self.parse_block();
                        } else if self.is_expression_start() {
                            self.parse_assignment_expression();
                        }
                    }
                    break;
                }

                // When the variable name itself was erroneous (e.g., TS1389 for a
                // reserved word like `const export`), stop this declaration list so
                // the statement loop can reparse the keyword in the tsc-shaped way.
                //
                // Carve-out: when the only error came from the initializer's
                // value (e.g. TS1121 on legacy octal `0123n` — the scanner
                // returns `0123` as a complete numeric literal and leaves `n`
                // as a separate identifier token), the declarator itself is
                // structurally complete. Let the missing-comma recovery below
                // (the can_continue branch) treat the next token as the start
                // of a new declarator so the `n` produces TS1005 "',' expected"
                // at the right position, matching tsc.
                if decl_had_error && !self.is_token(SyntaxKind::CloseBracketToken) {
                    let next_starts_declarator = (self.is_identifier_or_keyword()
                        && !self.is_reserved_word())
                        || self.is_token(SyntaxKind::OpenBraceToken)
                        || self.is_token(SyntaxKind::OpenBracketToken)
                        || self.current_unknown_starts_invalid_unicode_identifier_debris();
                    if !(decl_only_literal_value_errors && next_starts_declarator) {
                        break;
                    }
                }

                // `var v: void.x;` parses `void` as the type, then tsc reports
                // a missing comma at `.` and recovers `x` as a second declarator.
                let decl_has_type_annotation = self
                    .arena
                    .get(decl)
                    .and_then(|node| self.arena.get_variable_declaration(node))
                    .is_some_and(|decl| decl.type_annotation.is_some());
                if decl_has_type_annotation && self.is_token(SyntaxKind::DotToken) {
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let dot_followed_by_declaration =
                        self.is_identifier_or_keyword() && !self.is_reserved_word();
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if dot_followed_by_declaration {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token();
                        continue;
                    }
                }

                if decl_started_at_numeric_literal_follow_error
                    && self.is_token(SyntaxKind::OpenParenToken)
                {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);

                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    if self.is_token(SyntaxKind::CloseParenToken) {
                        self.parse_error_at_current_token(
                            "Expression expected.",
                            diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                    }
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;
                    break;
                }

                // If the unexpected token can start a new variable declaration
                // (identifier/keyword, { or [) AND is not a reserved word, treat
                // the missing comma as the only error and let the loop continue to
                // parse the token as the next declarator.
                // Example: `const a number = "missing colon";`
                //   tsc treats this as `const a, number = "missing colon";`
                //   and emits only one TS1005 at `number`.
                {
                    if self.current_unknown_starts_invalid_unicode_identifier_debris() {
                        continue;
                    }

                    let can_continue = (self.is_identifier_or_keyword()
                        && !self.is_reserved_word())
                        || self.is_token(SyntaxKind::OpenBraceToken)
                        || self.is_token(SyntaxKind::OpenBracketToken);
                    if can_continue {
                        // Emit ',' expected directly, bypassing the distance-based
                        // suppression heuristic. tsc's parseDelimitedList always
                        // emits TS1005 here (it only deduplicates at the exact same
                        // position). Without force-emit, two adjacent short
                        // identifiers (e.g. `var y: z is number;`) can fall within
                        // the suppression window and lose the second error.
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        continue;
                    }
                }

                // `var a₁ = "hello";` leaves an Unknown token for the subscript
                // character between the parsed identifier and `=`.
                // tsc recovers by treating the assignment tail as malformed
                // declaration syntax and reports TS1134 at `=` and again at
                // the initializer start, instead of bubbling out as TS1005 ';'
                // from parse_semicolon.
                if self.is_token(SyntaxKind::Unknown) {
                    if self.current_unknown_starts_braced_unicode_escape_debris() {
                        self.consume_braced_unicode_escape_debris_after_unknown();
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        continue;
                    }

                    let snapshot = self.scanner.save_state();
                    let current = self.current_token;
                    let unknown_text = self.scanner.get_token_text();
                    self.next_token();
                    let unknown_followed_by_equals = self.is_token(SyntaxKind::EqualsToken);
                    self.scanner.restore_state(snapshot);
                    self.current_token = current;

                    if unknown_followed_by_equals {
                        self.parse_error_at_current_token(
                            "Invalid character.",
                            diagnostic_codes::INVALID_CHARACTER,
                        );
                        self.next_token(); // consume Unknown

                        if unknown_text.starts_with("\\u") {
                            if self.parse_optional(SyntaxKind::EqualsToken)
                                && !matches!(
                                    self.token(),
                                    SyntaxKind::SemicolonToken
                                        | SyntaxKind::CloseBraceToken
                                        | SyntaxKind::EndOfFileToken
                                )
                            {
                                self.parse_assignment_expression();
                            }
                            break;
                        }

                        if self.is_token(SyntaxKind::EqualsToken) {
                            self.parse_error_at_current_token(
                                "Variable declaration expected.",
                                diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                            );
                            self.next_token(); // consume '='

                            if !matches!(
                                self.token(),
                                SyntaxKind::SemicolonToken
                                    | SyntaxKind::CloseBraceToken
                                    | SyntaxKind::EndOfFileToken
                            ) {
                                if self.is_token(SyntaxKind::NewKeyword) {
                                    let msg = tsz_common::diagnostics::diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                                        .replace("{0}", self.current_keyword_text());
                                    self.parse_error_at_current_token(
                                        &msg,
                                        diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                                    );
                                } else {
                                    self.parse_error_at_current_token(
                                        "Variable declaration expected.",
                                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                                    );
                                }
                            }
                        }
                        break;
                    }
                }

                if self.look_ahead_is_invalid_shebang() {
                    self.recover_invalid_shebang_token();
                    if self.is_token(SyntaxKind::ExclamationToken) {
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                    }
                    break;
                }

                if self.recover_jsx_closing_tag_extra_namespace_tail
                    && self.is_token(SyntaxKind::GreaterThanToken)
                {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    self.recover_jsx_closing_namespace_tail_greater_statement();
                    self.recover_jsx_closing_tag_extra_namespace_tail = false;
                    break;
                }

                // No ASI - emit ',' expected for the unexpected token and stop.
                // Use position-only dedup for normal tokens, not the broader
                // distance heuristic: tsc still reports adjacent declaration-list
                // comma errors like `var x: typeof function f() { };` at both
                // `f` and `(`. Keep Unknown tokens on the scanner-shaped TS1127
                // path instead of forcing TS1005.
                if self.is_token(SyntaxKind::Unknown) {
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        diagnostic_codes::INVALID_CHARACTER,
                    );
                } else {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                }

                // Otherwise stop the list. We break instead of continuing to avoid
                // cascading TS1134 errors when the recovery eats into what tsc
                // treats as a separate statement.
                // Example: `var b = new C0 32, '';` - tsc emits only TS1005 at `32`.
                // Only consume the unexpected token if it cannot start a new
                // statement.  Tokens like `delete`, `typeof`, `void`, `~` etc.
                // can begin an expression statement and must be preserved so the
                // subsequent statement-parsing loop can emit them.
                // Example: `var a = q~;` → tsc emits `var a = q;\n~;`
                if !self.is_statement_start() {
                    let unexpected_token = self.token();
                    // When a `.` separates what looks like two declarations
                    // (e.g., `const x: "".typeof(...)`), tsc treats the `.` as
                    // a missing `,` and continues the declaration list. When the
                    // next token is a keyword (e.g., `typeof`), tsc's list-parse
                    // error recovery emits TS1389 "not allowed as a variable
                    // declaration name". Emit the same diagnostic here, bypassing
                    // `error_reserved_word_in_variable_declaration` which would be
                    // suppressed by `should_report_error` proximity heuristic.
                    let was_dot = unexpected_token == SyntaxKind::DotToken;
                    self.next_token();
                    if matches!(
                        unexpected_token,
                        SyntaxKind::CloseBracketToken | SyntaxKind::CloseParenToken
                    ) && matches!(
                        self.token(),
                        SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken
                    ) {
                        // Keep malformed tails like `var v = /[]/]/` inside the
                        // declaration-list recovery so the trailing slash becomes
                        // TS1134 instead of a fresh unterminated regex statement.
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        self.next_token();
                    } else if unexpected_token == SyntaxKind::CloseBracketToken
                        && self.is_token(SyntaxKind::EqualsToken)
                    {
                        // `const x: C[#bar] = 3;` is recovered as a malformed
                        // declaration tail after `]`, producing TS1134 at `=`
                        // and at the initializer start (matching tsc).
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        self.next_token();
                        if !matches!(
                            self.token(),
                            SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        ) {
                            self.parse_error_at_current_token(
                                "Variable declaration expected.",
                                diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                            );
                            self.next_token();
                        }
                    }
                    if was_dot && token_is_keyword(self.token()) {
                        use tsz_common::diagnostics::diagnostic_messages;
                        let word = self.current_keyword_text();
                        let msg =
                            diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                                .replace("{0}", word);
                        self.parse_error_at_current_token(
                            &msg,
                            diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                        );
                        // Consume the reserved word and, if followed by a call tail
                        // like `typeof(this.foo)`, silently skip it. tsc stops after
                        // the TS1389 diagnostic without cascading TS1109/TS1005 into
                        // the trailing parentheses. Example:
                        //   `const x: "".typeof(this.foo);` → TS1005 at `.`, TS1389
                        //   at `typeof`, and nothing more.
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken)
                            && !self.scanner.has_preceding_line_break()
                        {
                            self.next_token(); // consume `(`
                            let mut paren_depth = 1u32;
                            while !matches!(
                                self.token(),
                                SyntaxKind::SemicolonToken
                                    | SyntaxKind::CloseBraceToken
                                    | SyntaxKind::EndOfFileToken
                            ) && !self.scanner.has_preceding_line_break()
                            {
                                match self.token() {
                                    SyntaxKind::OpenParenToken => paren_depth += 1,
                                    SyntaxKind::CloseParenToken => {
                                        paren_depth -= 1;
                                        if paren_depth == 0 {
                                            self.next_token();
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                                self.next_token();
                            }
                        }
                    } else if was_dot && self.is_identifier_or_keyword() && !self.is_reserved_word()
                    {
                        // `declare const x: "foo".charCodeAt(0);` is recovered by tsc as if
                        // `charCodeAt` started a second declarator. Mirror that by surfacing
                        // the follow-up TS1005 at `(` and then skipping the call tail.
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.parse_error_at_current_token(
                                "',' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                            let mut paren_depth = 0u32;
                            while !matches!(
                                self.token(),
                                SyntaxKind::SemicolonToken
                                    | SyntaxKind::CloseBraceToken
                                    | SyntaxKind::EndOfFileToken
                            ) && !self.scanner.has_preceding_line_break()
                            {
                                match self.token() {
                                    SyntaxKind::OpenParenToken => paren_depth += 1,
                                    SyntaxKind::CloseParenToken => {
                                        if paren_depth == 0 {
                                            break;
                                        }
                                        paren_depth -= 1;
                                    }
                                    _ => {}
                                }
                                self.next_token();
                                if paren_depth == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }
                break;
            }

            // After comma, check if next token can start another declaration.
            // Handle cases like: let x, , y (missing declaration between commas).
            // Reserved words (return, if, while, etc.) cannot be binding identifiers,
            // so `var a, return;` should be a trailing comma error, not a new declaration.
            let can_start_next = (self.is_identifier_or_keyword() && !self.is_reserved_word())
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken);

            if !can_start_next {
                // Trailing comma in variable declaration list — emit TS1009.
                // This covers `var a,;`, `var a,}`, `var a,` (EOF), and
                // `var a,\nreturn;` (reserved word after comma = trailing comma).
                use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                if self.is_token(SyntaxKind::SemicolonToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                    || self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_reserved_word()
                {
                    self.parse_error_at(
                        comma_pos,
                        1,
                        diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                        diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                }
                break;
            }
        }

        // Check for empty declaration list: var ;
        // TSC emits TS1123 "Variable declaration list cannot be empty"
        // Skip when TS1134 was already emitted (e.g., `using 1` — TSC only emits TS1134)
        if declarations.is_empty()
            && !had_decl_expected_error
            && !self.is_token(SyntaxKind::Unknown)
        {
            use tsz_common::diagnostics::diagnostic_codes;
            let pos = self.token_full_start();
            self.parse_error_at(
                pos,
                0,
                "Variable declaration list cannot be empty.",
                diagnostic_codes::VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY,
            );
        }

        let end_pos = self.token_end();
        self.arena.add_variable_with_flags(
            syntax_kind_ext::VARIABLE_DECLARATION_LIST,
            start_pos,
            end_pos,
            VariableData {
                modifiers: None,
                declarations: self.make_node_list(declarations),
            },
            flags,
        )
    }

    /// Parse variable declaration with declaration flags (for using/await using checks)
    /// Flags: bits 0-2 used for LET/CONST/USING, bit 3 for catch-clause binding (suppresses TS1182)
    pub(crate) fn parse_variable_declaration_with_flags(&mut self, flags: u16) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_variable_declaration_with_flags_pre_checks(flags);

        let name = self.parse_variable_declaration_name();
        let exclamation_token = self.parse_optional(SyntaxKind::ExclamationToken);
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        let initializer = self.parse_variable_declaration_initializer();
        self.parse_variable_declaration_after_parse_checks(flags, start_pos, name, initializer);

        let end_pos =
            self.parse_variable_declaration_end_pos(start_pos, type_annotation, name, initializer);

        self.arena.add_variable_declaration(
            syntax_kind_ext::VARIABLE_DECLARATION,
            start_pos,
            end_pos,
            VariableDeclarationData {
                name,
                exclamation_token,
                type_annotation,
                initializer,
            },
        )
    }
}
