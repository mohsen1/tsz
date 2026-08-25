use super::Parser;
use super::modifiers::Modifiers;
use crate::syntax::{
    AuthoredBindingName, Expression, ExpressionKind, FunctionDeclaration, FunctionLikeExpression,
    FunctionLikeSyntax, ParserRecoveryKind, TokenKind,
};

impl Parser<'_> {
    pub(super) fn parse_function(
        &mut self,
        modifiers: Modifiers,
        has_leading_jsdoc: bool,
    ) -> FunctionDeclaration {
        let diagnostic_count = self.diagnostics.len();
        let function_keyword = self.current().span;
        let unmodeled_generator = self.peek_kind(1) == TokenKind::Star;
        self.expect(TokenKind::Function, "'function' expected.", 1005);
        if unmodeled_generator {
            self.bump();
        }
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_signature_parameters();
        let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let has_body = self.at(TokenKind::LeftBrace);
        let body_extent = self.balanced_recovery_brace_extent(self.index);
        let (body, body_span) = if has_body {
            self.parse_block()
        } else {
            self.eat(TokenKind::Semicolon);
            (Vec::new(), None)
        };
        if unmodeled_generator && let Some(extent) = body_extent {
            while self.current().span.start < extent.end {
                self.bump();
            }
        }
        let overload_completion_supported = !unmodeled_generator
            && !modifiers.unsupported_for_overload_completion
            && parameters.iter().all(|parameter| {
                if has_body {
                    parameter.function_implementation_completion_supported
                } else {
                    parameter.overload_completion_supported
                }
            })
            && self.diagnostics.len() == diagnostic_count;
        if unmodeled_generator {
            let kind = ParserRecoveryKind::GeneratorFunctionLike;
            let span = function_keyword.merge(self.previous().span);
            self.retain_parser_recovery(kind, function_keyword, span);
        }
        FunctionDeclaration {
            name,
            name_span,
            type_parameters,
            parameters,
            return_type,
            body,
            has_body,
            body_span,
            has_leading_jsdoc,
            exported: modifiers.exported,
            default_export: modifiers.default_export,
            is_async: modifiers.is_async,
            declared: modifiers.declared,
            abstract_declaration: modifiers.abstract_declaration,
            overload_completion_supported,
        }
    }
    pub(super) fn parse_function_expression(&mut self) -> Expression {
        let diagnostic_count = self.diagnostics.len();
        let recovery_fact_start = self.parser_recovery_facts.len();
        let has_leading_jsdoc = self.current_has_leading_jsdoc();
        let function_keyword = self.bump().span;
        let unmodeled_generator = self.eat(TokenKind::Star);
        let name = self.kind().is_identifier().then(|| {
            let token_kind = self.kind();
            let (name, span) = self.parse_name();
            AuthoredBindingName {
                name,
                span,
                token_kind,
            }
        });
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_signature_parameters();
        let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let header_recovered = unmodeled_generator
            || self.diagnostics.len() != diagnostic_count
            || self.parser_recovery_facts.len() != recovery_fact_start;
        let has_opening_brace = self.at(TokenKind::LeftBrace);
        let authored_body_extent = has_opening_brace
            .then(|| self.balanced_recovery_brace_extent(self.index))
            .flatten();
        let (body, body_span) = if has_opening_brace {
            self.parse_block()
        } else {
            self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
            (Vec::new(), None)
        };
        if header_recovered && let Some(extent) = authored_body_extent {
            while self.current().span.start < extent.end {
                self.bump();
            }
        }
        let has_closing_brace = has_opening_brace && self.previous().kind == TokenKind::RightBrace;
        let span = function_keyword.merge(self.previous().span);
        let expression = Expression {
            id: self.alloc_node(),
            span,
            kind: ExpressionKind::FunctionLike(Box::new(FunctionLikeExpression {
                type_parameters,
                parameters,
                return_type,
                body_span,
                has_leading_jsdoc,
                syntax: FunctionLikeSyntax::Function { name, body },
            })),
        };
        if header_recovered || !has_opening_brace || !has_closing_brace {
            let kind = if unmodeled_generator {
                ParserRecoveryKind::GeneratorFunctionLike
            } else {
                ParserRecoveryKind::Expression
            };
            self.retain_parser_recovery(kind, function_keyword, span);
        }
        expression
    }
}
