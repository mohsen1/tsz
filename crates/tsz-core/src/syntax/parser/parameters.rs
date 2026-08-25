use super::Parser;
use crate::source::Span;
use crate::syntax::{
    Parameter, ParameterModifier, ParameterModifierNode, ParameterNameKind, TokenKind,
};

impl Parser<'_> {
    pub(super) fn parse_parameters(&mut self) -> Vec<Parameter> {
        self.parse_parameters_with_this(false, false)
    }

    pub(super) fn parse_arrow_parameters(&mut self) -> Vec<Parameter> {
        self.parse_parameters_with_this(false, true)
    }

    pub(super) fn parse_signature_parameters(&mut self) -> Vec<Parameter> {
        self.parse_parameters_with_this(true, false)
    }

    fn parse_parameters_with_this(
        &mut self,
        allow_this: bool,
        mut recover_missing_separator: bool,
    ) -> Vec<Parameter> {
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let mut parameters = Vec::new();
        while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
            recover_missing_separator |=
                matches!(self.kind(), TokenKind::Const | TokenKind::Default);
            parameters.push(self.parse_parameter_with_this(allow_this));
            if !self.eat(TokenKind::Comma) {
                if !recover_missing_separator
                    || self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile])
                    || !self.parameter_starts_arrow_speculation()
                {
                    break;
                }
                self.error_current("',' expected.", 1005);
            }
        }
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        parameters
    }

    pub(super) fn parse_parameter(&mut self) -> Parameter {
        self.parse_parameter_with_this(false)
    }

    pub(super) fn parse_parameter_with_this(&mut self, allow_this: bool) -> Parameter {
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
        let token_kind = self.kind();
        let ordinary_identifier = token_kind == TokenKind::Identifier;
        let this_parameter = allow_this && token_kind == TokenKind::This;
        let binding_start = self.index;
        let (name, name_span, name_kind) = if this_parameter {
            let token = self.bump();
            (
                self.text(token.span).to_string(),
                token.span,
                ParameterNameKind::This,
            )
        } else if modifiers.is_empty() {
            if matches!(token_kind, TokenKind::Const | TokenKind::Default) {
                let token = self.bump();
                let name = self.text(token.span).to_string();
                self.diagnostics.push(crate::diagnostics::Diagnostic::at(
                    self.source,
                    token.span,
                    format!(
                        "Identifier expected. '{name}' is a reserved word that cannot be used here."
                    ),
                    1359,
                ));
                (name, token.span, ParameterNameKind::Binding)
            } else {
                let (name, span) = self.parse_recovered_binding_head();
                (name, span, ParameterNameKind::Binding)
            }
        } else {
            let (name, span) = self.parse_name();
            (name, span, ParameterNameKind::Binding)
        };
        let recovered_binding_pattern =
            matches!(token_kind, TokenKind::LeftBrace | TokenKind::LeftBracket)
                && name_span.end > self.tokens[binding_start].span.end;
        let name_kind = if recovered_binding_pattern {
            ParameterNameKind::BindingPattern
        } else {
            name_kind
        };
        let recovered_binding_names = if recovered_binding_pattern {
            self.recovered_binding_names_in_target(binding_start)
        } else {
            Vec::new()
        };
        let optional_span = self.at(TokenKind::Question).then(|| self.bump().span);
        let optional = optional_span.is_some();
        let annotation = if self.eat(TokenKind::Colon) {
            let previous = self.type_member_recovery_code;
            self.type_member_recovery_code = 1005;
            let token = *self.current();
            let annotation = if matches!(
                token.kind,
                TokenKind::Comma
                    | TokenKind::Equals
                    | TokenKind::RightParen
                    | TokenKind::RightBracket
            ) {
                self.recover_missing_type(token, false)
            } else {
                self.parse_type()
            };
            self.type_member_recovery_code = previous;
            Some(annotation)
        } else {
            None
        };
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        let end = self.previous_end().max(start);
        let completion_supported =
            modifiers.is_empty() && self.diagnostics.len() == diagnostic_count;
        let overload_completion_supported =
            (ordinary_identifier || this_parameter) && completion_supported;
        let function_implementation_completion_supported =
            (token_kind.is_identifier() || this_parameter) && completion_supported;
        Parameter {
            name,
            name_span,
            recovered_binding_names,
            name_kind,
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
        !matches!(self.kind(), TokenKind::Const | TokenKind::Default)
            && (matches!(self.kind(), TokenKind::Export | TokenKind::Static)
                || self.tokens_are_on_same_line(self.index, self.index + 1))
            && !matches!(
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

pub(super) const fn parameter_modifier(kind: TokenKind) -> Option<ParameterModifier> {
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
