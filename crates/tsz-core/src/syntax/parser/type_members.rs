use super::Parser;
use crate::source::Span;
use crate::syntax::{
    AccessorKind, ParserRecoveryKind, TokenKind, TypeMember, TypeMemberKind, TypeMemberModifier,
    TypeMemberModifierNode, TypeMemberModifiers, TypeMemberName, TypeMemberNameKind, TypeNode,
    TypeNodeKind,
};

#[derive(Debug, Clone, Copy)]
enum SignatureMemberKind {
    Call,
    Construct,
}

macro_rules! type_member_modifiers {
    ($($token:ident => $modifier:ident, $field:ident;)*) => {
        const fn type_member_modifier(kind: TokenKind) -> Option<TypeMemberModifier> {
            match kind {
                $(TokenKind::$token => Some(TypeMemberModifier::$modifier),)*
                _ => None,
            }
        }

        const fn apply_modifier(modifiers: &mut TypeMemberModifiers, kind: TypeMemberModifier) {
            match kind {
                $(TypeMemberModifier::$modifier => modifiers.$field = true,)*
            }
        }
    };
}

type_member_modifiers! {
    Public => Public, public;
    Protected => Protected, protected;
    Private => Private, private;
    Readonly => Readonly, readonly;
    Static => Static, static_member;
    Abstract => Abstract, abstract_member;
    Declare => Declare, declared;
    Accessor => Accessor, accessor;
    Async => Async, async_member;
    Const => Const, const_member;
    Default => Default, default_member;
    Export => Export, exported;
    In => In, in_variance;
    Out => Out, out_variance;
    Override => Override, override_member;
}

impl Parser<'_> {
    pub(super) fn parse_tuple_type(&mut self) -> TypeNode {
        let left = self.bump().span;
        let mut members = Vec::new();
        while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
            let label = self.current().span;
            let named = self.kind().is_identifier()
                && (self.peek_kind(1) == TokenKind::Colon
                    || self.peek_kind(1) == TokenKind::Question
                        && self.peek_kind(2) == TokenKind::Colon)
                || self.at(TokenKind::DotDotDot)
                    && self.peek_kind(1).is_identifier()
                    && (self.peek_kind(2) == TokenKind::Colon
                        || self.peek_kind(2) == TokenKind::Question
                            && self.peek_kind(3) == TokenKind::Colon);
            if named {
                while !self.at_any(&[TokenKind::Colon, TokenKind::EndOfFile]) {
                    self.bump();
                }
                let colon = self.bump().span;
                self.retain_parser_recovery(ParserRecoveryKind::Type, label, label.merge(colon));
            }
            let rest = (!named && self.at(TokenKind::DotDotDot)).then(|| self.bump().span);
            let member = self.parse_type();
            if let Some(rest) = rest {
                self.retain_parser_recovery(
                    ParserRecoveryKind::Type,
                    rest,
                    rest.merge(member.span),
                );
            }
            members.push(member);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = self.current().span;
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        TypeNode {
            span: left.merge(end),
            kind: TypeNodeKind::Tuple(members),
        }
    }

    /// Parse TypeScript's single ordered `TypeElement` list.
    ///
    /// The branch order follows the pinned TypeScript parser: call signature,
    /// construct signature, modifiers, get/set accessors, index-signature
    /// lookahead, then property-or-method signature. Keeping that order is important for
    /// recovery because a bracketed computed property is not always an index
    /// signature.
    pub(super) fn parse_type_members(&mut self) -> Vec<TypeMember> {
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut members = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            if self.eat(TokenKind::Semicolon) || self.eat(TokenKind::Comma) {
                continue;
            }
            if !self.is_type_member_start() {
                self.error_current("Property or signature expected.", 1131);
                if is_type_member_modifier(self.kind()) && self.peek_kind(1) != TokenKind::Equals {
                    self.bump();
                    continue;
                }
                members.push(self.parse_recovered_type_member());
                break;
            }
            let before = self.index;
            members.push(self.parse_type_member());
            if self.index == before {
                self.bump();
            }
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        members
    }

    pub(super) fn parse_type_member(&mut self) -> TypeMember {
        if self.at_any(&[TokenKind::LeftParen, TokenKind::LessThan]) {
            return self.parse_signature_member(SignatureMemberKind::Call);
        }
        if self.at(TokenKind::New)
            && matches!(
                self.peek_kind(1),
                TokenKind::LeftParen | TokenKind::LessThan
            )
        {
            return self.parse_signature_member(SignatureMemberKind::Construct);
        }

        let start = self.current().span;
        let modifiers = self.parse_type_member_modifiers();
        if self.at(TokenKind::Get) && self.type_member_modifier_has_follower() {
            return self.parse_accessor_signature(start, modifiers, AccessorKind::Get);
        }
        if self.at(TokenKind::Set) && self.type_member_modifier_has_follower() {
            return self.parse_accessor_signature(start, modifiers, AccessorKind::Set);
        }
        if self.is_index_signature() {
            return self.parse_index_signature(start, modifiers);
        }
        self.parse_property_or_method_signature(start, modifiers)
    }

    fn parse_accessor_signature(
        &mut self,
        start: Span,
        modifiers: TypeMemberModifiers,
        accessor: AccessorKind,
    ) -> TypeMember {
        self.bump();
        let name = self.parse_type_member_name();
        let parameters = self.parse_parameters();
        let return_type = self.parse_type_annotation();
        self.parse_type_member_separator();
        self.finish_type_member(
            start,
            modifiers,
            TypeMemberKind::Accessor {
                name,
                accessor,
                parameters,
                return_type,
            },
        )
    }

    fn parse_signature_member(&mut self, kind: SignatureMemberKind) -> TypeMember {
        let start = self.current().span;
        if matches!(kind, SignatureMemberKind::Construct) {
            self.expect(TokenKind::New, "'new' expected.", 1005);
        }
        let type_parameters = self.parse_type_parameters();
        let parameters = match kind {
            SignatureMemberKind::Call => self.parse_signature_parameters(),
            SignatureMemberKind::Construct => self.parse_parameters(),
        };
        let return_type = self.parse_type_annotation();
        self.parse_type_member_separator();
        let kind = match kind {
            SignatureMemberKind::Call => TypeMemberKind::Call {
                type_parameters,
                parameters,
                return_type,
            },
            SignatureMemberKind::Construct => TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
            },
        };
        self.finish_type_member(start, TypeMemberModifiers::default(), kind)
    }

    fn parse_index_signature(&mut self, start: Span, modifiers: TypeMemberModifiers) -> TypeMember {
        self.expect(TokenKind::LeftBracket, "'[' expected.", 1005);
        let mut parameters = Vec::new();
        while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
            parameters.push(self.parse_parameter());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        let value_type = self.parse_type_annotation();
        self.parse_type_member_separator();
        self.finish_type_member(
            start,
            modifiers,
            TypeMemberKind::Index {
                parameters,
                value_type,
            },
        )
    }

    fn parse_property_or_method_signature(
        &mut self,
        start: Span,
        modifiers: TypeMemberModifiers,
    ) -> TypeMember {
        let name = self.parse_type_member_name();
        let optional = self.eat(TokenKind::Question);
        let kind = if self.at_any(&[TokenKind::LeftParen, TokenKind::LessThan]) {
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameters();
            let return_type = self.parse_type_annotation();
            TypeMemberKind::Method {
                name,
                optional,
                type_parameters,
                parameters,
                return_type,
            }
        } else {
            let ty = self.parse_type_annotation();
            let initializer_allowed =
                ty.is_some() || optional || matches!(name.kind, TypeMemberNameKind::Computed(_));
            let initializer = initializer_allowed
                .then(|| self.eat(TokenKind::Equals).then(|| self.parse_expression()))
                .flatten();
            TypeMemberKind::Property {
                name,
                ty,
                optional,
                initializer,
            }
        };
        self.parse_type_member_separator();
        self.finish_type_member(start, modifiers, kind)
    }

    fn finish_type_member(
        &mut self,
        start: Span,
        modifiers: TypeMemberModifiers,
        kind: TypeMemberKind,
    ) -> TypeMember {
        TypeMember {
            id: self.alloc_node(),
            span: start.merge(self.previous().span),
            recovered: false,
            recovery_incomplete: false,
            modifiers,
            kind,
        }
    }

    fn parse_type_annotation(&mut self) -> Option<TypeNode> {
        self.eat(TokenKind::Colon).then(|| self.parse_type())
    }

    fn parse_recovered_type_member(&mut self) -> TypeMember {
        let start = self.current().span;
        let name = self.parse_type_member_name();
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        let recovery_incomplete = !self.at(TokenKind::RightBrace);
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            self.bump();
        }
        if self.at(TokenKind::RightBrace) {
            if self.type_member_recovery_code == 1005 {
                self.error_current("',' expected.", 1005);
            } else {
                self.error_current("Declaration or statement expected.", 1128);
            }
        }
        TypeMember {
            id: self.alloc_node(),
            span: start.merge(self.previous().span),
            recovered: true,
            recovery_incomplete,
            modifiers: TypeMemberModifiers::default(),
            kind: TypeMemberKind::Property {
                name,
                ty: None,
                optional: false,
                initializer,
            },
        }
    }

    fn is_type_member_start(&self) -> bool {
        if type_member_start_at(self, 0) {
            return true;
        }
        let mut offset = 0;
        while is_type_member_modifier(self.peek_kind(offset)) {
            offset += 1;
            if type_member_start_at(self, offset) {
                return true;
            }
        }
        false
    }

    fn parse_type_member_separator(&mut self) {
        let _ = self.eat(TokenKind::Comma) || self.eat(TokenKind::Semicolon);
    }

    fn parse_type_member_name(&mut self) -> TypeMemberName {
        let token = *self.current();
        self.observe_unmodeled_numeric_separator_if_current();
        match token.kind {
            TokenKind::StringLiteral => {
                self.bump();
                TypeMemberName {
                    span: token.span,
                    kind: TypeMemberNameKind::StringLiteral(
                        self.ordinary_string_literal_value(token),
                    ),
                }
            }
            TokenKind::NumericLiteral | TokenKind::BigIntLiteral => {
                self.bump();
                let name = self.text(token.span).to_string();
                TypeMemberName {
                    span: token.span,
                    kind: match token.kind {
                        TokenKind::NumericLiteral => TypeMemberNameKind::NumericLiteral(name),
                        TokenKind::BigIntLiteral => TypeMemberNameKind::BigIntLiteral(name),
                        _ => unreachable!("matched numeric type-member name"),
                    },
                }
            }
            TokenKind::LeftBracket => {
                let start = self.bump().span;
                let expression = self.parse_expression();
                let end = self.current().span;
                self.observe_unmodeled_numeric_separator_in_span(start.merge(end));
                self.expect(TokenKind::RightBracket, "']' expected.", 1005);
                TypeMemberName {
                    span: start.merge(end),
                    kind: TypeMemberNameKind::Computed(expression),
                }
            }
            _ => {
                let (name, span) = self.parse_identifier_name();
                TypeMemberName {
                    span,
                    kind: TypeMemberNameKind::Identifier(name),
                }
            }
        }
    }

    fn parse_type_member_modifiers(&mut self) -> TypeMemberModifiers {
        let mut modifiers = TypeMemberModifiers::default();
        while let Some(kind) = type_member_modifier(self.kind()) {
            if !self.type_member_modifier_has_follower() {
                break;
            }
            let span = self.bump().span;
            apply_modifier(&mut modifiers, kind);
            modifiers.nodes.push(TypeMemberModifierNode { kind, span });
        }
        modifiers
    }

    fn type_member_modifier_has_follower(&self) -> bool {
        !matches!(
            self.peek_kind(1),
            TokenKind::Colon
                | TokenKind::Question
                | TokenKind::Equals
                | TokenKind::LeftParen
                | TokenKind::LessThan
                | TokenKind::Semicolon
                | TokenKind::Comma
                | TokenKind::RightBrace
                | TokenKind::EndOfFile
        )
    }

    fn is_index_signature(&self) -> bool {
        if !self.at(TokenKind::LeftBracket) {
            return false;
        }
        let mut cursor = self.index + 1;
        let mut kind = self.token_kind_at(cursor);
        if matches!(kind, TokenKind::DotDotDot | TokenKind::RightBracket) {
            return true;
        }

        if is_type_member_modifier(kind) {
            cursor += 1;
            kind = self.token_kind_at(cursor);
            // TypeScript deliberately commits to index-signature recovery as
            // soon as a modifier is followed by an identifier. `[public a]`
            // must not be reinterpreted as a computed property.
            return kind.is_identifier();
        } else if !kind.is_identifier() {
            return false;
        }
        cursor += 1;
        kind = self.token_kind_at(cursor);
        if matches!(kind, TokenKind::Colon | TokenKind::Comma) {
            return true;
        }
        if kind != TokenKind::Question {
            return false;
        }
        matches!(
            self.token_kind_at(cursor + 1),
            TokenKind::Colon | TokenKind::Comma | TokenKind::RightBracket
        )
    }

    pub(super) fn token_kind_at(&self, index: usize) -> TokenKind {
        self.tokens
            .get(index)
            .map_or(TokenKind::EndOfFile, |token| token.kind)
    }
}

const fn is_type_member_modifier(kind: TokenKind) -> bool {
    type_member_modifier(kind).is_some()
}

fn type_member_start_at(parser: &Parser<'_>, offset: usize) -> bool {
    let kind = parser.peek_kind(offset);
    if matches!(
        kind,
        TokenKind::LeftParen | TokenKind::LessThan | TokenKind::LeftBracket
    ) {
        return true;
    }
    if kind == TokenKind::New
        && matches!(
            parser.peek_kind(offset + 1),
            TokenKind::LeftParen | TokenKind::LessThan
        )
    {
        return true;
    }
    if matches!(kind, TokenKind::Get | TokenKind::Set)
        && is_type_member_name_token(parser.peek_kind(offset + 1))
        && parser.peek_kind(offset + 2) == TokenKind::LeftParen
    {
        return true;
    }
    is_type_member_name_token(kind)
        && matches!(
            parser.peek_kind(offset + 1),
            TokenKind::LeftParen
                | TokenKind::LessThan
                | TokenKind::Question
                | TokenKind::Colon
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::RightBrace
        )
}

const fn is_type_member_name_token(kind: TokenKind) -> bool {
    kind.is_identifier()
        || matches!(
            kind,
            TokenKind::StringLiteral | TokenKind::NumericLiteral | TokenKind::BigIntLiteral
        )
}
