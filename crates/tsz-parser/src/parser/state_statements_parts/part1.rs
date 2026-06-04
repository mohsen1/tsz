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
            *previous_statement_was_block = false;
            return true;
        }

        if resync_after_unknown {
            self.resync_after_error_with_statement_starts(false);
        }
        *previous_statement_was_block = false;
        true
    }

    fn recover_colon_after_block_statement(
        &mut self,
        previous_statement_was_block: &mut bool,
    ) -> bool {
        if !*previous_statement_was_block || !self.is_token(SyntaxKind::ColonToken) {
            return false;
        }

        self.parse_error_at_current_token(
            "Declaration or statement expected.",
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        );
        self.next_token();
        *previous_statement_was_block = false;
        true
    }

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
        // Sort diagnostics into tsc's canonical `compareDiagnostics` order after
        // merging scanner- and parser-produced entries. Sorting by `start` alone
        // left position ties resolved by production/merge order, which is not
        // guaranteed to match tsc and is fragile under reordering.
        self.parse_diagnostics.sort_by(|a, b| a.compare(b));

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

            if self.recover_colon_after_block_statement(&mut previous_statement_was_block) {
                prev_block_needs_post_equals_semi = false;
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
                } else if self.bare_hash_is_followed_by_statement_boundary() {
                    // Preserve a standalone invalid `#` as a statement expression.
                    // tsc still emits `#;` for this recovery shape.
                } else {
                    // Bare `#` — emit TS1127 and skip, matching tsc.
                    self.report_bare_hash_invalid_character();
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

            if self.recover_colon_after_block_statement(&mut previous_statement_was_block) {
                continue;
            }

            if previous_statement_was_block
                && self.orphan_case_assignment_starts_recovered_class_member()
            {
                self.parse_error_at_current_token(
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                break;
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
        if !self.current_token_starts_case_assignment() {
            return false;
        }

        self.parse_error_at_current_token(
            "Declaration or statement expected.",
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        );
        self.skip_orphan_case_assignment();
        if self.is_token(SyntaxKind::IfKeyword) {
            self.report_orphan_case_following_if_header_recovery();
        }
        true
    }

    fn orphan_case_assignment_starts_recovered_class_member(&mut self) -> bool {
        self.in_block_context()
            && self.in_class_body()
            && self.current_token_starts_case_assignment()
    }

    fn current_token_starts_case_assignment(&mut self) -> bool {
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
        has_same_line_equals
    }

    fn skip_orphan_case_assignment(&mut self) {
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
        if self.in_ambient_context() {
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

    fn variable_declaration_has_missing_typeof_target(&self, decl: NodeIndex) -> bool {
        self.arena
            .get(decl)
            .and_then(|node| self.arena.get_variable_declaration(node))
            .and_then(|decl| self.arena.get(decl.type_annotation))
            .filter(|type_node| type_node.kind == syntax_kind_ext::TYPE_QUERY)
            .and_then(|type_node| self.arena.get_type_query(type_node))
            .is_some_and(|type_query| {
                self.arena
                    .is_missing_recovery_identifier(type_query.expr_name)
            })
    }
}
