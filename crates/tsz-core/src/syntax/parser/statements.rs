use super::{Parser, token_is_binding_identifier};
use crate::source::Span;
use crate::syntax::{JumpStatement, TokenKind};

impl Parser<'_> {
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
