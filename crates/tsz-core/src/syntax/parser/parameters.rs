use super::Parser;
use crate::source::Span;
use crate::syntax::{Parameter, ParameterModifier, ParameterModifierNode, TokenKind};

impl Parser<'_> {
    pub(super) fn parse_parameters(&mut self) -> Vec<Parameter> {
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let mut parameters = Vec::new();
        while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
            parameters.push(self.parse_parameter());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        parameters
    }

    pub(super) fn parse_parameter(&mut self) -> Parameter {
        let diagnostic_count = self.diagnostics.len();
        let start = self.current().span.start as usize;
        let mut modifiers = Vec::new();
        while let Some(modifier) = parameter_modifier(self.kind()) {
            if !self.parameter_modifier_has_follower() {
                break;
            }
            let span = self.bump().span;
            modifiers.push(ParameterModifierNode {
                kind: modifier,
                span,
            });
        }
        let rest_span = self.at(TokenKind::DotDotDot).then(|| self.bump().span);
        let rest = rest_span.is_some();
        let name_kind = self.kind();
        let ordinary_identifier = name_kind == TokenKind::Identifier;
        let (name, name_span) = self.parse_name();
        let optional_span = self.at(TokenKind::Question).then(|| self.bump().span);
        let optional = optional_span.is_some();
        let annotation = if self.eat(TokenKind::Colon) {
            let previous = self.type_member_recovery_code;
            self.type_member_recovery_code = 1005;
            let annotation = self.parse_type();
            self.type_member_recovery_code = previous;
            Some(annotation)
        } else {
            None
        };
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        let end = self.previous_end().max(start);
        let overload_completion_supported = ordinary_identifier
            && modifiers.is_empty()
            && self.diagnostics.len() == diagnostic_count;
        let function_implementation_completion_supported = name_kind.is_identifier()
            && modifiers.is_empty()
            && self.diagnostics.len() == diagnostic_count;
        Parameter {
            name,
            name_span,
            annotation,
            initializer,
            optional,
            optional_span,
            rest,
            rest_span,
            modifiers,
            overload_completion_supported,
            function_implementation_completion_supported,
            span: Span::new(self.source.id, start, end),
        }
    }

    fn parameter_modifier_has_follower(&self) -> bool {
        !matches!(
            self.peek_kind(1),
            TokenKind::Colon
                | TokenKind::Question
                | TokenKind::Equals
                | TokenKind::Comma
                | TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::EndOfFile
        )
    }
}

const fn parameter_modifier(kind: TokenKind) -> Option<ParameterModifier> {
    Some(match kind {
        TokenKind::Abstract => ParameterModifier::Abstract,
        TokenKind::Accessor => ParameterModifier::Accessor,
        TokenKind::Async => ParameterModifier::Async,
        TokenKind::Const => ParameterModifier::Const,
        TokenKind::Declare => ParameterModifier::Declare,
        TokenKind::Default => ParameterModifier::Default,
        TokenKind::Export => ParameterModifier::Export,
        TokenKind::In => ParameterModifier::In,
        TokenKind::Out => ParameterModifier::Out,
        TokenKind::Override => ParameterModifier::Override,
        TokenKind::Public => ParameterModifier::Public,
        TokenKind::Protected => ParameterModifier::Protected,
        TokenKind::Private => ParameterModifier::Private,
        TokenKind::Readonly => ParameterModifier::Readonly,
        TokenKind::Static => ParameterModifier::Static,
        _ => return None,
    })
}
