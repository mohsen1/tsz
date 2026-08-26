use super::Parser;
use crate::syntax::{
    AuthoredLiteralKind, LiteralSyntaxBoundary, SourceSyntaxFact, TokenKind,
    UnmodeledDeclarationHostFact, UnmodeledDeclarationHostKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Modifiers {
    pub(super) exported: bool,
    pub(super) default_export: bool,
    pub(super) declared: bool,
    pub(super) is_async: bool,
    pub(super) abstract_declaration: bool,
    pub(super) unsupported_for_overload_completion: bool,
    invalid_sequence: bool,
}

impl Parser<'_> {
    pub(super) fn class_member_starts_accessor(&self) -> bool {
        token_starts_property_name(self.peek_kind(1))
            && matches!(
                self.peek_kind(2),
                TokenKind::LeftParen | TokenKind::LessThan
            )
            || self.peek_kind(1) == TokenKind::LeftBracket
    }

    fn observe_literal_syntax_boundary(
        &mut self,
        family: AuthoredLiteralKind,
        boundary: LiteralSyntaxBoundary,
    ) {
        self.source_syntax_facts
            .insert(SourceSyntaxFact::LiteralBoundary(family, boundary));
    }

    pub(super) fn observe_literal_lexical_recovery(&mut self, family: AuthoredLiteralKind) {
        self.observe_literal_syntax_boundary(family, LiteralSyntaxBoundary::LexicalRecovery);
    }

    pub(super) fn observe_literal_validation_gap(&mut self, family: AuthoredLiteralKind) {
        self.observe_literal_syntax_boundary(family, LiteralSyntaxBoundary::SemanticValidation);
    }

    pub(super) fn observe_literal_unsupported_host(&mut self, family: AuthoredLiteralKind) {
        self.observe_literal_syntax_boundary(family, LiteralSyntaxBoundary::UnsupportedHost);
    }

    pub(super) fn starts_export_declaration(&self) -> bool {
        if !self.at(TokenKind::Export) {
            return false;
        }
        match self.peek_kind(1) {
            TokenKind::LeftBrace | TokenKind::Star | TokenKind::Equals | TokenKind::As => true,
            TokenKind::Type => matches!(self.peek_kind(2), TokenKind::LeftBrace | TokenKind::Star),
            TokenKind::Default => {
                let mut offset = 2;
                while matches!(
                    self.peek_kind(offset),
                    TokenKind::Export
                        | TokenKind::Default
                        | TokenKind::Declare
                        | TokenKind::Async
                        | TokenKind::Abstract
                ) {
                    offset += 1;
                }
                !self.starts_statement_host_at(offset)
            }
            _ => false,
        }
    }

    fn starts_statement_host_at(&self, offset: usize) -> bool {
        let kind = self.peek_kind(offset);
        let next = self.peek_kind(offset + 1);
        match kind {
            TokenKind::Let
            | TokenKind::Const
            | TokenKind::Var
            | TokenKind::Function
            | TokenKind::Class
            | TokenKind::Interface
            | TokenKind::Enum => true,
            TokenKind::Type => {
                next.is_identifier()
                    && self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1)
            }
            TokenKind::Module
            | TokenKind::Namespace
            | TokenKind::Global
            | TokenKind::Using
            | TokenKind::Await => self.unmodeled_declaration_host_at(offset).is_some(),
            TokenKind::Import => !matches!(
                next,
                TokenKind::LeftParen | TokenKind::LessThan | TokenKind::Dot
            ),
            _ => false,
        }
    }

    pub(super) fn observe_statement_modifiers(&mut self, modifiers: Modifiers) {
        let host = self.kind();
        if host == TokenKind::Class {
            if modifiers.is_async {
                self.source_syntax_facts
                    .insert(SourceSyntaxFact::AsyncClassModifier);
            }
            if modifiers.invalid_sequence {
                self.source_syntax_facts
                    .insert(SourceSyntaxFact::InvalidClassModifierOrder);
            }
        }
        if modifiers.default_export
            && (modifiers.invalid_sequence
                || !matches!(host, TokenKind::Function | TokenKind::Class))
        {
            self.source_syntax_facts
                .insert(SourceSyntaxFact::DefaultExportOnUnsupportedHost);
        }
    }

    pub(super) fn parse_modifiers(&mut self, statement_start: usize) -> Modifiers {
        let mut modifiers = Modifiers::default();
        let mut modifier_order = 0;
        macro_rules! take_modifier {
            ($field:ident, $order:expr) => {{
                observe_modifier_order(&mut modifiers, &mut modifier_order, $order);
                modifiers.unsupported_for_overload_completion |= modifiers.$field;
                modifiers.$field = true;
                self.bump();
            }};
        }
        loop {
            if matches!(
                self.kind(),
                TokenKind::Declare | TokenKind::Async | TokenKind::Abstract
            ) && matches!(
                self.peek_kind(1),
                TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
            ) {
                // These contextual keywords remain identifier-like when they
                // are the tag expression. Do not reinterpret the template as
                // a recovered declaration tail.
                break;
            }
            match self.kind() {
                TokenKind::Export => {
                    self.source_syntax_facts
                        .insert(SourceSyntaxFact::ModuleExport);
                    take_modifier!(exported, 1);
                    let default_export = self.eat(TokenKind::Default);
                    if default_export {
                        observe_modifier_order(&mut modifiers, &mut modifier_order, 2);
                    }
                    modifiers.unsupported_for_overload_completion |=
                        default_export && modifiers.default_export;
                    modifiers.default_export |= default_export;
                }
                TokenKind::Declare => {
                    take_modifier!(declared, 3);
                }
                TokenKind::Async => {
                    take_modifier!(is_async, 4);
                }
                TokenKind::Abstract => {
                    take_modifier!(abstract_declaration, 4);
                }
                _ => break,
            }
        }
        self.retain_unmodeled_declaration_host(statement_start, modifiers.exported);
        modifiers
    }

    fn retain_unmodeled_declaration_host(&mut self, owner_start: usize, exported: bool) {
        let Some((name_offset, Some(kind))) = self.unmodeled_declaration_host_at(0) else {
            return;
        };
        let name_index = (name_offset > 0).then_some(self.index + name_offset);
        let name_token = name_index.and_then(|index| self.tokens.get(index).copied());
        let recovery_extent = self.unmodeled_declaration_recovery_extent(kind);
        self.unmodeled_declaration_hosts
            .push(UnmodeledDeclarationHostFact {
                owner_start: owner_start as u32,
                recovery_extent,
                name: name_token.map(|token| self.text(token.span).to_string()),
                name_span: name_token.map(|token| token.span),
                kind,
                exported,
            });
    }

    fn unmodeled_declaration_recovery_extent(
        &self,
        kind: UnmodeledDeclarationHostKind,
    ) -> crate::source::Span {
        let start = self.current().span;
        let mut braces = 0usize;
        let mut using_depth = 0_u32;
        let mut previous = self.index;
        let using = kind == UnmodeledDeclarationHostKind::Using;
        for (index, token) in self.tokens.iter().enumerate().skip(self.index) {
            if using
                && using_depth == 0
                && index > self.index
                && self.later_line_starts_declaration(previous, index)
            {
                return start.merge(self.tokens[previous].span);
            }
            match token.kind {
                TokenKind::LeftBrace if !using => braces += 1,
                TokenKind::RightBrace if braces > 0 => {
                    braces = braces.saturating_sub(1);
                    if braces == 0 {
                        return start.merge(token.span);
                    }
                }
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace if using => {
                    using_depth += 1;
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
                    if using && using_depth > 0 =>
                {
                    using_depth -= 1;
                }
                TokenKind::RightBrace if using => {
                    return start.merge(self.tokens[previous].span);
                }
                TokenKind::Semicolon if braces == 0 && (!using || using_depth == 0) => {
                    return start.merge(token.span);
                }
                TokenKind::EndOfFile => return start.merge(token.span),
                _ => {}
            }
            previous = index;
        }
        start
    }

    fn unmodeled_declaration_host_at(
        &self,
        offset: usize,
    ) -> Option<(usize, Option<UnmodeledDeclarationHostKind>)> {
        let kind = self.peek_kind(offset);
        let next = self.peek_kind(offset + 1);
        let same_line = self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1);
        match kind {
            TokenKind::Enum if next.is_identifier() => {
                Some((offset + 1, Some(UnmodeledDeclarationHostKind::Enum)))
            }
            TokenKind::Const
                if next == TokenKind::Enum && self.peek_kind(offset + 2).is_identifier() =>
            {
                Some((offset + 2, Some(UnmodeledDeclarationHostKind::Enum)))
            }
            TokenKind::Module if same_line && next.is_identifier() => {
                Some((offset + 1, Some(UnmodeledDeclarationHostKind::Module)))
            }
            TokenKind::Module if same_line && next == TokenKind::StringLiteral => Some((
                offset + 1,
                Some(UnmodeledDeclarationHostKind::ExternalModule),
            )),
            TokenKind::Namespace if same_line && next.is_identifier() => {
                Some((offset + 1, Some(UnmodeledDeclarationHostKind::Namespace)))
            }
            TokenKind::Namespace if same_line && next == TokenKind::StringLiteral => {
                Some((offset + 1, None))
            }
            TokenKind::Global
                if matches!(
                    next,
                    TokenKind::LeftBrace | TokenKind::Identifier | TokenKind::Export
                ) || same_line && next.is_identifier() =>
            {
                Some((0, Some(UnmodeledDeclarationHostKind::Global)))
            }
            TokenKind::Using if same_line && next.is_identifier() => {
                Some((offset + 1, Some(UnmodeledDeclarationHostKind::Using)))
            }
            TokenKind::Await
                if next == TokenKind::Using
                    && same_line
                    && self.tokens_are_on_same_line(
                        self.index + offset + 1,
                        self.index + offset + 2,
                    )
                    && self.peek_kind(offset + 2).is_identifier() =>
            {
                Some((offset + 2, Some(UnmodeledDeclarationHostKind::Using)))
            }
            _ => None,
        }
    }
}

const fn token_starts_property_name(kind: TokenKind) -> bool {
    kind.is_identifier_name()
        || matches!(
            kind,
            TokenKind::PrivateIdentifier | TokenKind::NumericLiteral | TokenKind::StringLiteral
        )
}

const fn observe_modifier_order(modifiers: &mut Modifiers, previous: &mut u8, current: u8) {
    modifiers.invalid_sequence |= *previous >= current;
    *previous = current;
}
