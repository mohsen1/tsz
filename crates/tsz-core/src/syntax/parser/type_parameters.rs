use super::Parser;
use crate::syntax::{KeywordType, TokenKind, TypeNode, TypeNodeKind, TypeParameterDeclaration};

impl Parser<'_> {
    pub(super) fn parse_keyword_type(&mut self) -> Option<TypeNode> {
        let first = *self.current();
        let keyword = match first.kind {
            TokenKind::Any => KeywordType::Any,
            TokenKind::Unknown => KeywordType::Unknown,
            TokenKind::Never => KeywordType::Never,
            TokenKind::Void => KeywordType::Void,
            TokenKind::Undefined => KeywordType::Undefined,
            TokenKind::Null => KeywordType::Null,
            TokenKind::Boolean => KeywordType::Boolean,
            TokenKind::Number => KeywordType::Number,
            TokenKind::String => KeywordType::String,
            TokenKind::BigInt => KeywordType::BigInt,
            TokenKind::Object => KeywordType::Object,
            TokenKind::Symbol => KeywordType::Symbol,
            TokenKind::Unique if self.peek_kind(1) == TokenKind::Symbol => {
                self.bump();
                KeywordType::UniqueSymbol
            }
            _ => return None,
        };
        let last = self.bump();
        Some(TypeNode {
            span: first.span.merge(last.span),
            kind: TypeNodeKind::Keyword(keyword),
        })
    }

    pub(super) fn parse_type_parameters(&mut self) -> Vec<TypeParameterDeclaration> {
        if !self.eat(TokenKind::LessThan) {
            return Vec::new();
        }
        let mut parameters = Vec::new();
        while !self.at_type_close() && !self.at(TokenKind::EndOfFile) {
            let start = self.current().span;
            let mut const_parameter = false;
            let mut in_variance = false;
            let mut out_variance = false;
            loop {
                match self.kind() {
                    TokenKind::Const => const_parameter = self.eat(TokenKind::Const),
                    TokenKind::In => in_variance = self.eat(TokenKind::In),
                    TokenKind::Out => out_variance = self.eat(TokenKind::Out),
                    _ => break,
                }
            }
            let (name, name_span) = self.parse_name();
            let constraint = self.eat(TokenKind::Extends).then(|| self.parse_type());
            let default = self.eat(TokenKind::Equals).then(|| self.parse_type());
            let end = default
                .as_ref()
                .or(constraint.as_ref())
                .map_or(name_span, |node| node.span);
            parameters.push(TypeParameterDeclaration {
                name,
                name_span,
                span: start.merge(end),
                constraint,
                default,
                const_parameter,
                in_variance,
                out_variance,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect_type_close();
        parameters
    }
}
