use super::Parser;
use crate::syntax::TokenKind::{LeftBrace, RightBrace};
use crate::syntax::UnmodeledDeclarationHostKind::{
    Enum, ExternalModule, Global, Module, Namespace, Using,
};
use crate::syntax::{
    AuthoredLiteralKind, DeclarationHostBodyRepresentation, LiteralSyntaxBoundary,
    SourceSyntaxFact, StatementKind, TokenKind, UnmodeledDeclarationHostFact,
    UnmodeledDeclarationHostKind,
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
            let identifier_like = matches!(
                self.kind(),
                TokenKind::Declare | TokenKind::Async | TokenKind::Abstract
            );
            if identifier_like
                && (!self.tokens_are_on_same_line(self.index, self.index + 1)
                    || matches!(
                        self.peek_kind(1),
                        TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
                    ))
            {
                // ASI and tagged templates keep these contextual keywords identifier-like.
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
        self.retain_unmodeled_declaration_host(statement_start, modifiers);
        modifiers
    }

    pub(super) fn error_modified_declaration(&mut self, statement_start: usize) {
        let modifier = self.tokens[..self.index].iter().copied().find(|token| {
            token.span.start >= statement_start as u32
                && matches!(token.kind, TokenKind::Export | TokenKind::Declare)
        });
        let (Some(modifier), Some((_, Using))) = (modifier, self.unmodeled_declaration_host_at(0))
        else {
            self.error_current("Declaration expected.", 1146);
            return;
        };
        let modifier_name = match modifier.kind {
            TokenKind::Export => "export",
            TokenKind::Declare => "declare",
            _ => unreachable!(),
        };
        let (code, declaration) = if self.at(TokenKind::Await) {
            (1495, "an 'await using'")
        } else {
            (1491, "a 'using'")
        };
        self.diagnostics.push(crate::diagnostics::Diagnostic::at(
            self.source,
            modifier.span,
            format!("'{modifier_name}' modifier cannot appear on {declaration} declaration."),
            code,
        ));
    }

    fn retain_unmodeled_declaration_host(&mut self, owner_start: usize, modifiers: Modifiers) {
        let Some((name_offset, kind)) = self.unmodeled_declaration_host_at(0) else {
            return;
        };
        let name_token = (name_offset > 0).then(|| self.tokens[self.index + name_offset]);
        let recovery_extent = self.unmodeled_declaration_recovery_extent(kind);
        self.unmodeled_declaration_hosts
            .push(UnmodeledDeclarationHostFact {
                owner_start: owner_start as u32,
                recovery_extent,
                name: name_token.map(|token| self.text(token.span).to_string()),
                name_span: name_token.map(|token| token.span),
                kind,
                body: DeclarationHostBodyRepresentation::Omitted,
                declared: modifiers.declared,
                exported: modifiers.exported,
            });
    }

    pub(super) fn parse_opaque_host(
        &mut self,
        owner_start: usize,
        modifiers: Modifiers,
    ) -> Option<StatementKind> {
        let (name_offset, kind) = self.unmodeled_declaration_host_at(0)?;
        let statement = match kind {
            Module | Namespace | ExternalModule => Some(self.parse_opaque_module(kind)),
            Enum => {
                let body = self.index + name_offset + 1;
                self.balanced_recovery_brace_extent(body)?;
                self.eat(TokenKind::Const);
                self.bump();
                self.parse_name();
                self.consume_balanced_tokens(LeftBrace, RightBrace, "'}' expected.");
                self.eat(TokenKind::Semicolon);
                Some(StatementKind::Unknown)
            }
            Global if modifiers.declared && self.peek_kind(1) == LeftBrace => {
                self.bump();
                let body = StatementKind::Block(self.parse_block().0);
                self.eat(TokenKind::Semicolon);
                Some(body)
            }
            Global | Using => None,
        };
        if matches!(statement, Some(StatementKind::Block(_)))
            && let Some(host) = self
                .unmodeled_declaration_hosts
                .iter_mut()
                .rev()
                .find(|host| host.owner_start == owner_start as u32 && host.kind == kind)
        {
            host.body = DeclarationHostBodyRepresentation::ParsedStatements;
        }
        statement
    }

    fn parse_opaque_module(&mut self, kind: UnmodeledDeclarationHostKind) -> StatementKind {
        debug_assert!(matches!(kind, Module | Namespace | ExternalModule));
        self.bump();

        let mut malformed_name = kind == Namespace && self.at(TokenKind::StringLiteral);
        if kind == ExternalModule {
            debug_assert!(self.at(TokenKind::StringLiteral));
            self.bump();
        } else {
            self.parse_name();
            while self.eat(TokenKind::Dot) {
                if !self.kind().is_identifier_name() {
                    self.error_current("Identifier expected.", 1003);
                    malformed_name = true;
                    break;
                }
                self.parse_identifier_name();
            }
        }

        if self.at(LeftBrace) {
            return StatementKind::Block(self.parse_block().0);
        }

        if kind != ExternalModule && !malformed_name {
            self.error_current("'{' expected.", 1005);
        }
        self.eat(TokenKind::Semicolon);
        StatementKind::Unknown
    }

    fn unmodeled_declaration_recovery_extent(
        &self,
        kind: UnmodeledDeclarationHostKind,
    ) -> crate::source::Span {
        let start = self.current().span;
        if kind != UnmodeledDeclarationHostKind::Using {
            let balanced_body = self
                .tokens
                .iter()
                .enumerate()
                .skip(self.index)
                .take_while(|(_, token)| {
                    !matches!(token.kind, TokenKind::Semicolon | TokenKind::EndOfFile)
                })
                .find(|(_, token)| token.kind == LeftBrace)
                .and_then(|(index, _)| self.balanced_recovery_brace_extent(index));
            if let Some(extent) = balanced_body {
                return start.merge(extent);
            }
        }
        self.recovery_extent_from_current(start)
    }

    fn unmodeled_declaration_host_at(
        &self,
        offset: usize,
    ) -> Option<(usize, UnmodeledDeclarationHostKind)> {
        let kind = self.peek_kind(offset);
        let next = self.peek_kind(offset + 1);
        let same_line = self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1);
        match kind {
            TokenKind::Enum if next.is_identifier() => Some((offset + 1, Enum)),
            TokenKind::Const
                if next == TokenKind::Enum && self.peek_kind(offset + 2).is_identifier() =>
            {
                Some((offset + 2, Enum))
            }
            TokenKind::Module | TokenKind::Namespace
                if same_line && (next.is_identifier() || next == TokenKind::StringLiteral) =>
            {
                let host = match kind {
                    TokenKind::Module if next == TokenKind::StringLiteral => ExternalModule,
                    TokenKind::Module => Module,
                    TokenKind::Namespace => Namespace,
                    _ => unreachable!(),
                };
                Some((offset + 1, host))
            }
            TokenKind::Global
                if matches!(
                    next,
                    TokenKind::LeftBrace | TokenKind::Identifier | TokenKind::Export
                ) || same_line && next.is_identifier() =>
            {
                Some((0, Global))
            }
            TokenKind::Using if same_line && next.is_identifier() => Some((offset + 1, Using)),
            TokenKind::Await
                if next == TokenKind::Using
                    && same_line
                    && self.tokens_are_on_same_line(
                        self.index + offset + 1,
                        self.index + offset + 2,
                    )
                    && self.peek_kind(offset + 2).is_identifier() =>
            {
                Some((offset + 2, Using))
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
