impl ParserState {
    // Parse export declaration
    // export { x, y };
    // export { x } from "mod";
    // export * from "mod";
    // export default x;
    // export function `f()` {}
    // export class C {}
    pub(crate) fn parse_for_of_statement_rest(
        &mut self,
        start_pos: u32,
        initializer: NodeIndex,
        await_modifier: bool,
    ) -> NodeIndex {
        // Check for multiple variable declarations in for-of: for (var a, b of X)
        // TSC emits TS1188 "Only a single variable declaration is allowed in a 'for...of' statement"
        if let Some(node) = self.arena.get(initializer)
            && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(data) = self.arena.get_variable(node)
            && data.declarations.nodes.len() > 1
        {
            // Report error at the second declaration
            if let Some(&second_decl) = data.declarations.nodes.get(1)
                && let Some(second_node) = self.arena.get(second_decl)
            {
                self.parse_error_at(
                                    second_node.pos,
                                    second_node.end - second_node.pos,
                                    "Only a single variable declaration is allowed in a 'for...of' statement.",
                                    diagnostic_codes::ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_OF_STATEMENT,
                                );
            }
        }
        self.parse_expected(SyntaxKind::OfKeyword);
        let expression = self.parse_assignment_expression();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let statement = self.parse_statement();
        self.check_using_outside_block(statement);

        let end_pos = self.token_end();
        self.arena.add_for_in_of(
            syntax_kind_ext::FOR_OF_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::ForInOfData {
                await_modifier,
                initializer,
                expression,
                statement,
            },
        )
    }

    pub(crate) fn parse_break_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::BreakKeyword);

        // For restricted productions (break), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        // Optional label — matching tsc's isIdentifier() which returns false for
        // `await` in await/static-block context and `yield` in generator context.
        // When the label would be a contextually reserved word (e.g., `break await;` in a
        // static block), tsc's parseIdentifier emits TS1003 "Identifier expected" and
        // leaves the token unconsumed. The outer statement loop then re-parses the
        // reserved word as an expression statement (e.g. `await` as an await expression
        // with a missing operand), which is where TS1109 originates.
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            if self.is_contextually_reserved_label() {
                // Emit TS1003 matching tsc's createIdentifier(false) behavior
                self.error_identifier_expected();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        self.arena.add_jump(
            syntax_kind_ext::BREAK_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JumpData { label },
        )
    }

    pub(crate) fn parse_continue_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ContinueKeyword);

        // For restricted productions (continue), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon().
        // For contextually reserved-word labels (e.g. `continue await` in a static block),
        // see `parse_break_statement` above for the full rationale: emit TS1003 and leave
        // the token unconsumed so the outer loop can re-parse it as an expression.
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            if self.is_contextually_reserved_label() {
                self.error_identifier_expected();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        self.arena.add_jump(
            syntax_kind_ext::CONTINUE_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JumpData { label },
        )
    }

    pub(crate) fn parse_throw_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ThrowKeyword);

        // TypeScript requires an expression after throw
        // If there's a line break immediately after throw, emit TS1142
        let expression = if self.scanner.has_preceding_line_break()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Line break after throw - TS1142: Line break not permitted here
            // The error position should be at the end of the `throw` keyword
            let throw_end = start_pos + keyword_text_len(SyntaxKind::ThrowKeyword);
            self.parse_error_at(
                throw_end,
                0,
                "Line break not permitted here.",
                diagnostic_codes::LINE_BREAK_NOT_PERMITTED_HERE,
            );
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::SemicolonToken)
            || self.is_token(SyntaxKind::CloseBraceToken)
            || self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Explicit semicolon, closing brace, or EOF after throw without expression
            // TypeScript requires an expression after throw
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at(
                start,
                end - start,
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            NodeIndex::NONE
        } else if !self.can_parse_semicolon_for_restricted_production() {
            self.parse_expression()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        // Use return statement node type for throw (same structure)
        self.arena.add_return(
            syntax_kind_ext::THROW_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    pub(crate) fn parse_do_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DoKeyword);

        let statement = self.parse_statement();
        self.check_using_outside_block(statement);

        self.parse_expected(SyntaxKind::WhileKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();

        // Check for missing condition expression: do { } while ()
        if condition == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        // Per ECMAScript spec, semicolons are always auto-inserted after do-while.
        // TypeScript uses parseOptional(SemicolonToken) here, not parseSemicolon().
        self.parse_optional(SyntaxKind::SemicolonToken);
        let end_pos = self.token_end();

        self.arena.add_loop(
            syntax_kind_ext::DO_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer: NodeIndex::NONE,
                condition,
                incrementor: NodeIndex::NONE,
                statement,
            },
        )
    }

    pub(crate) fn parse_switch_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::SwitchKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let clauses = self.parse_switch_case_clauses();

        let case_block_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        let case_block = self.arena.add_block(
            syntax_kind_ext::CASE_BLOCK,
            start_pos,
            case_block_end,
            BlockData {
                statements: self.make_node_list(clauses),
                multi_line: true,
            },
        );

        self.arena.add_switch(
            syntax_kind_ext::SWITCH_STATEMENT,
            start_pos,
            end_pos,
            SwitchData {
                expression,
                case_block,
            },
        )
    }
}
