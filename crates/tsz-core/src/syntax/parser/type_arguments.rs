use super::Parser;
use crate::diagnostics::Diagnostic;
use crate::syntax::{TokenKind, TypeNode};

impl Parser<'_> {
    pub(super) fn parse_type_arguments(&mut self) -> Vec<TypeNode> {
        self.parse_type_arguments_with_status().0
    }

    fn parse_type_arguments_with_status(&mut self) -> (Vec<TypeNode>, bool) {
        let left = self.current().span;
        if !self.eat(TokenKind::LessThan) {
            return (Vec::new(), false);
        }
        let mut arguments = Vec::new();
        while !self.at_type_close() && !self.at(TokenKind::EndOfFile) {
            let recovering_missing_argument = self.at(TokenKind::Comma);
            arguments.push(self.parse_type());
            if recovering_missing_argument {
                continue;
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let closed = self.expect_type_close();
        if closed && arguments.is_empty() {
            self.diagnostics.push(Diagnostic::at(
                self.source,
                left,
                "Type argument list cannot be empty.".to_string(),
                1099,
            ));
        }
        (arguments, closed)
    }

    /// Disambiguate `callee<T>(value)` from relational `<`/`>` expressions.
    ///
    /// Parsing is speculative because nested generic closes can rewrite a
    /// `>>` token. Every cursor, node, diagnostic, and token mutation is
    /// restored unless the ordinary postfix parser later commits the call.
    pub(super) fn call_type_arguments_are_followed_by_left_paren(&mut self) -> bool {
        if !self.source.kind().supports_expression_type_arguments() || !self.at(TokenKind::LessThan)
        {
            return false;
        }
        let saved_index = self.index;
        let saved_next_node = self.next_node;
        let saved_diagnostics = self.diagnostics.len();
        let saved_speculating = self.speculating;
        let saved_rewrites = self.speculative_token_rewrites.len();
        self.speculating = true;
        let (_, closed) = self.parse_type_arguments_with_status();
        let is_call = closed && self.at(TokenKind::LeftParen);
        for (index, token) in self
            .speculative_token_rewrites
            .drain(saved_rewrites..)
            .rev()
        {
            self.tokens[index] = token;
        }
        self.speculating = saved_speculating;
        self.index = saved_index;
        self.next_node = saved_next_node;
        self.diagnostics.truncate(saved_diagnostics);
        is_call
    }
}
