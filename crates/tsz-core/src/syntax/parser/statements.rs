use super::Parser;
use crate::source::Span;
use crate::syntax::{
    IfStatement, JumpStatement, Statement, StatementKind, SwitchClause, SwitchClauseKind,
    SwitchStatement, TokenKind, VariableDeclarator, VariableKind, VariableStatement,
};

impl Parser<'_> {
    pub(super) fn parse_block(&mut self) -> (Vec<Statement>, Option<Span>) {
        let promotes_stray_close = self.pending_stray_statement_closes_after_block != 0;
        if promotes_stray_close {
            self.pending_stray_statement_closes_after_block -= 1;
        }
        let has_opening_brace = self.at(TokenKind::LeftBrace);
        let opening_brace = self.current().span;
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut statements = Vec::new();
        while !self.at(TokenKind::EndOfFile)
            && (!self.at(TokenKind::RightBrace) || self.pending_stray_statement_closes != 0)
        {
            let before = self.index;
            statements.push(self.parse_statement());
            if before == self.index {
                self.bump();
            }
        }
        let has_closing_brace = self.at(TokenKind::RightBrace);
        let closing_brace = self.current().span;
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        if promotes_stray_close {
            self.pending_stray_statement_closes += 1;
        }
        (
            statements,
            (has_opening_brace && has_closing_brace).then(|| opening_brace.merge(closing_brace)),
        )
    }

    pub(super) fn starts_unmodeled_for_binding_pattern(&self) -> bool {
        let mut cursor = self.index + 1;
        if self.token_kind_at(cursor) == TokenKind::Await {
            cursor += 1;
        }
        self.token_kind_at(cursor) == TokenKind::LeftParen
            && self.token_kind_at(cursor + 1) == TokenKind::Const
            && matches!(
                self.token_kind_at(cursor + 2),
                TokenKind::LeftBrace | TokenKind::LeftBracket
            )
    }

    pub(super) fn parse_unmodeled_for_statement(&mut self) -> Vec<Statement> {
        let authored_span = self.bump().span;
        self.eat(TokenKind::Await);
        let mut statements = Vec::new();
        if self.at(TokenKind::LeftParen) {
            self.bump();
            if matches!(
                self.kind(),
                TokenKind::Let | TokenKind::Const | TokenKind::Var
            ) {
                let declaration_start = self.current().span;
                let declaration_kind = match self.kind() {
                    TokenKind::Const => VariableKind::Const,
                    TokenKind::Var => VariableKind::Var,
                    _ => VariableKind::Let,
                };
                self.bump();
                let binding_start = self.current().span;
                let recovered_binding_names = self.recovered_binding_names_in_target(self.index);
                let (name, name_span) = self.parse_recovered_binding_head();
                let declaration_span = declaration_start.merge(name_span);
                statements.push(Statement {
                    id: self.alloc_node(),
                    span: declaration_span,
                    kind: StatementKind::Variable(VariableStatement {
                        declaration_kind,
                        declarators: vec![VariableDeclarator {
                            name,
                            name_span,
                            recovered_binding_names,
                            annotation: None,
                            initializer: None,
                        }],
                        has_leading_jsdoc: false,
                        exported: false,
                        declared: false,
                    }),
                });
                self.note_recovery(
                    crate::syntax::ParserRecoveryKind::ForStatement,
                    binding_start,
                    declaration_span,
                );
                debug_assert!(binding_start.start <= name_span.start);
            }
            let mut depth = 1_u32;
            while depth != 0 && !self.at(TokenKind::EndOfFile) {
                let kind = self.kind();
                self.bump();
                if kind == TokenKind::LeftParen {
                    depth += 1;
                } else if kind == TokenKind::RightParen {
                    depth -= 1;
                }
            }
            if depth != 0 {
                self.error_current("')' expected.", 1005);
            }
        } else {
            self.error_current("'(' expected.", 1005);
        }
        let body = if self.at(TokenKind::LeftBrace) {
            self.parse_block().0
        } else if self.at(TokenKind::EndOfFile) {
            Vec::new()
        } else {
            vec![self.parse_statement()]
        };
        statements.extend(body);
        let recovery_extent = authored_span.merge(self.previous().span);
        self.note_recovery(
            crate::syntax::ParserRecoveryKind::ForStatement,
            authored_span,
            recovery_extent,
        );
        statements
    }

    pub(super) fn parse_if_statement(&mut self) -> IfStatement {
        self.bump();
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let condition = self.parse_expression();
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        let then_statement = Box::new(self.parse_statement());
        let else_statement = self
            .eat(TokenKind::Else)
            .then(|| Box::new(self.parse_statement()));
        IfStatement {
            condition,
            then_statement,
            else_statement,
        }
    }

    pub(super) fn parse_switch_statement(&mut self) -> SwitchStatement {
        self.bump();
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let expression = self.parse_expression();
        let recovered_discriminant = !self.at(TokenKind::RightParen);
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);

        let mut clauses = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let kind = if self.eat(TokenKind::Case) {
                let expression = self.parse_expression();
                self.expect(TokenKind::Colon, "':' expected.", 1005);
                SwitchClauseKind::Case(expression)
            } else if self.eat(TokenKind::Default) {
                self.expect(TokenKind::Colon, "':' expected.", 1005);
                SwitchClauseKind::Default
            } else {
                self.error_current("'case' or 'default' expected.", 1130);
                self.bump();
                continue;
            };

            let mut statements = Vec::new();
            while !self.at_any(&[
                TokenKind::Case,
                TokenKind::Default,
                TokenKind::RightBrace,
                TokenKind::EndOfFile,
            ]) {
                let before = self.index;
                statements.push(self.parse_statement());
                if self.index == before {
                    self.bump();
                }
            }
            let end = statements
                .last()
                .map_or_else(|| self.previous().span, |statement| statement.span);
            clauses.push(SwitchClause {
                span: start.merge(end),
                kind,
                statements,
            });
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        SwitchStatement {
            expression,
            clauses,
            recovered_discriminant,
        }
    }

    pub(super) fn finish_expression_statement(&mut self) {
        if self.eat(TokenKind::Semicolon)
            || self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile])
        {
            return;
        }
        if self.at(TokenKind::Colon) && self.current_is_inside_rejected_generic_arrow_prefix() {
            self.error_current("';' expected.", 1005);
            self.bump();
        }
    }

    pub(super) fn parse_jump_statement(&mut self) -> JumpStatement {
        self.bump();
        let same_line = self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index);
        let (label, label_span) = if same_line && self.kind().is_identifier() {
            let (label, span) = self.parse_name();
            (Some(label), Some(span))
        } else {
            (None, None)
        };
        if same_line && self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index) {
            self.observe_unmodeled_template_if_current();
        }
        self.eat(TokenKind::Semicolon);
        JumpStatement { label, label_span }
    }
}
