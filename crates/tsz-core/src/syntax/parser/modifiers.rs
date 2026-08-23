use super::{Parser, token_is_binding_identifier};
use crate::source::SourceKind;
use crate::syntax::{
    ClassMember, ClassMemberKind, ExportDeclaration, ImportDeclaration, InterfaceDeclaration,
    TokenKind, TypeAliasDeclaration, UnmodeledDeclarationHostFact, UnmodeledDeclarationHostKind,
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

#[derive(Debug, Clone, Copy)]
enum StatementHost {
    Variable,
    Function,
    Class,
    TypeAlias,
    Interface,
    Other,
}

impl Modifiers {
    const fn products_owned_by(self, host: StatementHost) -> bool {
        if self.invalid_sequence {
            return false;
        }
        match host {
            StatementHost::Variable => {
                !self.default_export
                    && !self.declared
                    && !self.is_async
                    && !self.abstract_declaration
            }
            StatementHost::Function
            | StatementHost::Class
            | StatementHost::TypeAlias
            | StatementHost::Interface => {
                !self.default_export && !self.is_async && !self.abstract_declaration
            }
            StatementHost::Other => {
                !self.exported
                    && !self.default_export
                    && !self.declared
                    && !self.is_async
                    && !self.abstract_declaration
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProductCapabilities {
    pub(super) functions_supported: bool,
    pub(super) classes_supported: bool,
    pub(super) declarations_supported: bool,
    pub(super) commonjs_classes_supported: bool,
    pub(super) declaration_hosts_supported: bool,
    pub(super) default_export_hosts_supported: bool,
    pub(super) expression_products_supported: bool,
    pub(super) template_products_supported: bool,
    pub(super) extended_unicode_string_products_supported: bool,
    pub(super) regular_expression_products_supported: bool,
    pub(super) numeric_recovery_products_supported: bool,
    pub(super) numeric_separator_products_supported: bool,
    has_bodyless_class: bool,
    has_module_export: bool,
}

impl ProductCapabilities {
    pub(super) const fn all_supported() -> Self {
        Self {
            functions_supported: true,
            classes_supported: true,
            declarations_supported: true,
            commonjs_classes_supported: true,
            declaration_hosts_supported: true,
            default_export_hosts_supported: true,
            expression_products_supported: true,
            template_products_supported: true,
            extended_unicode_string_products_supported: true,
            regular_expression_products_supported: true,
            numeric_recovery_products_supported: true,
            numeric_separator_products_supported: true,
            has_bodyless_class: false,
            has_module_export: false,
        }
    }

    pub(super) const fn observe_function(&mut self, modifiers: Modifiers, supported: bool) {
        self.functions_supported &=
            !modifiers.default_export && !modifiers.abstract_declaration && supported;
    }

    pub(super) const fn observe_module_export(&mut self) {
        self.has_module_export = true;
        self.commonjs_classes_supported &= !self.has_bodyless_class;
    }

    pub(super) const fn observe_explicit_call_type_arguments(&mut self) {
        self.declarations_supported = false;
    }

    pub(super) const fn observe_unmodeled_declaration_host(&mut self) {
        self.declaration_hosts_supported = false;
    }

    pub(super) const fn observe_unmodeled_default_export_host(&mut self) {
        self.default_export_hosts_supported = false;
    }

    pub(super) const fn observe_unmodeled_expression_products(&mut self) {
        self.expression_products_supported = false;
    }

    pub(super) const fn observe_unmodeled_template(&mut self) {
        self.template_products_supported = false;
    }

    pub(super) const fn observe_unmodeled_extended_unicode_string(&mut self) {
        self.extended_unicode_string_products_supported = false;
    }

    pub(super) const fn observe_unmodeled_regular_expression(&mut self) {
        self.regular_expression_products_supported = false;
    }

    pub(super) const fn observe_unmodeled_numeric_recovery(&mut self) {
        self.numeric_recovery_products_supported = false;
    }

    pub(super) const fn observe_unmodeled_numeric_separator(&mut self) {
        self.numeric_separator_products_supported = false;
    }

    pub(super) const fn commonjs_classes_supported(&self) -> bool {
        self.commonjs_classes_supported
    }

    pub(super) fn observe_class(&mut self, modifiers: Modifiers, members: &[ClassMember]) {
        self.classes_supported &= !modifiers.abstract_declaration
            && !modifiers.is_async
            && !modifiers.unsupported_for_overload_completion
            && members.iter().all(|member| member.emit_products_supported);
        let has_bodyless_member = members.iter().any(|member| {
            matches!(
                &member.kind,
                ClassMemberKind::Constructor {
                    has_body: false,
                    ..
                } | ClassMemberKind::Method {
                    has_body: false,
                    ..
                }
            )
        });
        self.has_bodyless_class |= has_bodyless_member;
        self.commonjs_classes_supported &= !(has_bodyless_member && self.has_module_export);
    }
}

impl Parser<'_> {
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
                token_is_binding_identifier(next)
                    && self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1)
            }
            TokenKind::Module | TokenKind::Namespace => {
                self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1)
                    && (next.is_identifier() || next == TokenKind::StringLiteral)
            }
            TokenKind::Global => {
                matches!(
                    next,
                    TokenKind::LeftBrace | TokenKind::Identifier | TokenKind::Export
                ) || self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1)
                    && token_is_binding_identifier(next)
            }
            TokenKind::Using => {
                self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1)
                    && token_is_binding_identifier(next)
            }
            TokenKind::Await => {
                next == TokenKind::Using
                    && self.tokens_are_on_same_line(self.index + offset, self.index + offset + 1)
                    && self
                        .tokens_are_on_same_line(self.index + offset + 1, self.index + offset + 2)
                    && token_is_binding_identifier(self.peek_kind(offset + 2))
            }
            TokenKind::Import => !matches!(
                next,
                TokenKind::LeftParen | TokenKind::LessThan | TokenKind::Dot
            ),
            _ => false,
        }
    }

    pub(super) fn observe_statement_modifiers(&mut self, modifiers: Modifiers) {
        let host = match self.kind() {
            TokenKind::Let | TokenKind::Const | TokenKind::Var => StatementHost::Variable,
            TokenKind::Function => StatementHost::Function,
            TokenKind::Class => StatementHost::Class,
            TokenKind::Type => StatementHost::TypeAlias,
            TokenKind::Interface => StatementHost::Interface,
            _ => StatementHost::Other,
        };
        if ((modifiers.exported || modifiers.declared) && self.statement_nesting_depth > 0)
            || !modifiers.products_owned_by(host)
        {
            self.has_unmodeled_top_level_syntax = true;
        }
        if modifiers.default_export
            && (modifiers.invalid_sequence
                || !matches!(host, StatementHost::Function | StatementHost::Class))
        {
            self.product_capabilities
                .observe_unmodeled_default_export_host();
        }
    }

    pub(super) fn parse_product_owned_import_declaration(&mut self) -> ImportDeclaration {
        let declaration = self.parse_import_declaration();
        if declaration.type_only || declaration.bindings.iter().any(|binding| binding.type_only) {
            self.observe_javascript_declaration_host();
        }
        declaration
    }

    pub(super) fn parse_product_owned_export_declaration(&mut self) -> ExportDeclaration {
        let declaration = self.parse_export_declaration();
        if declaration.type_only
            || declaration
                .specifiers
                .iter()
                .any(|specifier| specifier.type_only)
        {
            self.observe_javascript_declaration_host();
        }
        declaration
    }

    pub(super) fn parse_product_owned_type_alias(
        &mut self,
        exported: bool,
    ) -> TypeAliasDeclaration {
        let declaration = self.parse_type_alias(exported);
        self.observe_javascript_declaration_host();
        declaration
    }

    pub(super) fn parse_product_owned_interface(&mut self, exported: bool) -> InterfaceDeclaration {
        let declaration = self.parse_interface(exported);
        self.observe_javascript_declaration_host();
        declaration
    }

    fn observe_javascript_declaration_host(&mut self) {
        if matches!(
            self.source.kind(),
            SourceKind::JavaScript | SourceKind::JavaScriptJsx
        ) {
            self.product_capabilities
                .observe_unmodeled_declaration_host();
        }
    }

    pub(super) fn parse_modifiers(&mut self, statement_start: usize) -> Modifiers {
        let mut modifiers = Modifiers::default();
        let mut modifier_order = 0;
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
                    observe_modifier_order(&mut modifiers, &mut modifier_order, 1);
                    self.product_capabilities.observe_module_export();
                    modifiers.unsupported_for_overload_completion |= modifiers.exported;
                    modifiers.exported = true;
                    self.bump();
                    let default_export = self.eat(TokenKind::Default);
                    if default_export {
                        observe_modifier_order(&mut modifiers, &mut modifier_order, 2);
                    }
                    modifiers.unsupported_for_overload_completion |=
                        default_export && modifiers.default_export;
                    modifiers.default_export |= default_export;
                }
                TokenKind::Declare => {
                    observe_modifier_order(&mut modifiers, &mut modifier_order, 3);
                    modifiers.unsupported_for_overload_completion |= modifiers.declared;
                    modifiers.declared = true;
                    self.bump();
                }
                TokenKind::Async => {
                    observe_modifier_order(&mut modifiers, &mut modifier_order, 4);
                    modifiers.unsupported_for_overload_completion |= modifiers.is_async;
                    modifiers.is_async = true;
                    self.bump();
                }
                TokenKind::Abstract => {
                    observe_modifier_order(&mut modifiers, &mut modifier_order, 4);
                    modifiers.unsupported_for_overload_completion |= modifiers.abstract_declaration;
                    modifiers.abstract_declaration = true;
                    self.bump();
                }
                _ => break,
            }
        }
        if self.starts_unmodeled_declaration_host() {
            self.product_capabilities
                .observe_unmodeled_declaration_host();
            self.retain_unmodeled_declaration_host(statement_start, modifiers.exported);
        }
        modifiers
    }

    fn retain_unmodeled_declaration_host(&mut self, owner_start: usize, exported: bool) {
        let (name_index, kind) = match self.kind() {
            TokenKind::Module | TokenKind::Namespace if self.peek_kind(1).is_identifier() => (
                Some(self.index + 1),
                if self.kind() == TokenKind::Module {
                    UnmodeledDeclarationHostKind::Module
                } else {
                    UnmodeledDeclarationHostKind::Namespace
                },
            ),
            TokenKind::Module if self.peek_kind(1) == TokenKind::StringLiteral => (
                Some(self.index + 1),
                UnmodeledDeclarationHostKind::ExternalModule,
            ),
            TokenKind::Using if token_is_binding_identifier(self.peek_kind(1)) => {
                (Some(self.index + 1), UnmodeledDeclarationHostKind::Using)
            }
            TokenKind::Await
                if self.peek_kind(1) == TokenKind::Using
                    && token_is_binding_identifier(self.peek_kind(2)) =>
            {
                (Some(self.index + 2), UnmodeledDeclarationHostKind::Using)
            }
            TokenKind::Global => (None, UnmodeledDeclarationHostKind::Global),
            _ => return,
        };
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
        let mut saw_body = false;
        let mut using_depth = 0_u32;
        let mut previous = self.index;
        for (index, token) in self.tokens.iter().enumerate().skip(self.index) {
            if kind == UnmodeledDeclarationHostKind::Using
                && using_depth == 0
                && index > self.index
                && self.later_line_starts_declaration(previous, index)
            {
                return start.merge(self.tokens[previous].span);
            }
            match token.kind {
                TokenKind::LeftBrace if kind != UnmodeledDeclarationHostKind::Using => {
                    saw_body = true;
                    braces += 1;
                }
                TokenKind::RightBrace if saw_body => {
                    braces = braces.saturating_sub(1);
                    if braces == 0 {
                        return start.merge(token.span);
                    }
                }
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace
                    if kind == UnmodeledDeclarationHostKind::Using =>
                {
                    using_depth += 1;
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
                    if kind == UnmodeledDeclarationHostKind::Using && using_depth > 0 =>
                {
                    using_depth -= 1;
                }
                TokenKind::RightBrace if kind == UnmodeledDeclarationHostKind::Using => {
                    return start.merge(self.tokens[previous].span);
                }
                TokenKind::Semicolon
                    if !saw_body
                        && (kind != UnmodeledDeclarationHostKind::Using || using_depth == 0) =>
                {
                    return start.merge(token.span);
                }
                TokenKind::EndOfFile => return start.merge(token.span),
                _ => {}
            }
            previous = index;
        }
        start
    }

    fn starts_unmodeled_declaration_host(&self) -> bool {
        match self.kind() {
            TokenKind::Module | TokenKind::Namespace => {
                self.tokens_are_on_same_line(self.index, self.index + 1)
                    && (self.peek_kind(1).is_identifier()
                        || self.peek_kind(1) == TokenKind::StringLiteral)
            }
            TokenKind::Global => {
                let next = self.peek_kind(1);
                matches!(
                    next,
                    TokenKind::LeftBrace | TokenKind::Identifier | TokenKind::Export
                ) || self.tokens_are_on_same_line(self.index, self.index + 1)
                    && token_is_binding_identifier(next)
            }
            TokenKind::Using => {
                self.tokens_are_on_same_line(self.index, self.index + 1)
                    && token_is_binding_identifier(self.peek_kind(1))
            }
            TokenKind::Await => {
                self.peek_kind(1) == TokenKind::Using
                    && self.tokens_are_on_same_line(self.index, self.index + 1)
                    && self.tokens_are_on_same_line(self.index + 1, self.index + 2)
                    && token_is_binding_identifier(self.peek_kind(2))
            }
            _ => false,
        }
    }
}

const fn observe_modifier_order(modifiers: &mut Modifiers, previous: &mut u8, current: u8) {
    modifiers.invalid_sequence |= *previous >= current;
    *previous = current;
}
