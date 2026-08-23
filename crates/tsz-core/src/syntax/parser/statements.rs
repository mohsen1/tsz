use super::{Parser, token_is_binding_identifier};
use crate::source::Span;
use crate::syntax::{
    IfStatement, JumpStatement, SwitchClause, SwitchClauseKind, SwitchStatement, TokenKind,
};

impl Parser<'_> {
    pub(super) fn parse_if_statement(&mut self) -> IfStatement {
        self.bump();
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let condition = self.parse_expression();
        self.observe_template_expression_semantics(&condition);
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
        self.observe_template_expression_semantics(&expression);
        let recovered_discriminant = !self.at(TokenKind::RightParen);
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);

        let mut clauses = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let kind = if self.eat(TokenKind::Case) {
                let expression = self.parse_expression();
                self.observe_template_expression_semantics(&expression);
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
        if self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index) {
            self.has_unmodeled_top_level_syntax = true;
        }
    }

    pub(super) fn parse_jump_statement(&mut self) -> JumpStatement {
        let keyword = self.bump();
        let has_line_break = self
            .source
            .slice(Span::new(
                self.source.id,
                keyword.span.end as usize,
                self.current().span.start as usize,
            ))
            .bytes()
            .any(|byte| matches!(byte, b'\n' | b'\r'));
        let (label, label_span) = if !has_line_break && token_is_binding_identifier(self.kind()) {
            let (label, span) = self.parse_name();
            (Some(label), Some(span))
        } else {
            (None, None)
        };
        if !has_line_break && self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
        {
            self.observe_unmodeled_template_if_current();
        }
        self.eat(TokenKind::Semicolon);
        JumpStatement { label, label_span }
    }
}
