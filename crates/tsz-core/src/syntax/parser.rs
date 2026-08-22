use crate::diagnostics::Diagnostic;
use crate::source::{NodeId, SourceText, Span};

mod literals;
mod modifiers;
mod numeric_literal;
mod operators;
mod parameters;
mod regular_expression;
mod statements;
mod string_literal;
mod type_arguments;
mod type_members;
mod type_parameters;

use super::numeric_literal::{ScannedNumericLiteral, ScannedSeparatedNumberLiteral};
use super::regular_expression::ScannedRegularExpressionLiteral;
use super::string_literal::{ScannedLineContinuationStringLiteral, ScannedStringLiteral};
use super::template_literal::ScannedTemplateLiteral;
use super::{
    AccessorKind, ArrowBody, ClassDeclaration, ClassMember, ClassMemberKind, ClassMemberModifiers,
    CommentTrivia, ExportDeclaration, ExportSpecifier, Expression, ExpressionKind,
    FunctionDeclaration, IfStatement, ImportBinding, ImportDeclaration, InterfaceDeclaration,
    ObjectProperty, ParameterModifier, Statement, StatementKind, SwitchClause, SwitchClauseKind,
    SwitchStatement, Token, TokenKind, TypeAliasDeclaration, TypeNode, TypeNodeKind, UnaryOperator,
    VariableDeclaration, VariableKind, scan_source,
};
use modifiers::{Modifiers, ProductCapabilities};
use operators::{binary_operator, expression_has_recovered_left_edge};

#[derive(Debug)]
pub struct ParseOutput {
    pub unit: super::SourceUnit,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse_source(source: &SourceText) -> ParseOutput {
    let scanned = scan_source(source);
    Parser::new(source, scanned).parse()
}
struct Parser<'a> {
    source: &'a SourceText,
    tokens: Vec<Token>,
    index: usize,
    next_node: u32,
    diagnostics: Vec<Diagnostic>,
    template_literals: Vec<ScannedTemplateLiteral>,
    string_literals: Vec<ScannedStringLiteral>,
    line_continuation_string_literals: Vec<ScannedLineContinuationStringLiteral>,
    regular_expression_literals: Vec<ScannedRegularExpressionLiteral>,
    numeric_literals: Vec<ScannedNumericLiteral>,
    separated_numeric_literals: Vec<ScannedSeparatedNumberLiteral>,
    has_unmodeled_numeric_separator: bool,
    numeric_parser_diagnostics: Vec<Diagnostic>,
    comments: Vec<CommentTrivia>,
    has_unicode_line_comment_terminator: bool,
    has_unmodeled_trivia: bool,
    speculating: bool,
    speculative_token_rewrites: Vec<(usize, Token)>,
    type_member_recovery_code: u32,
    statement_nesting_depth: usize,
    has_unmodeled_top_level_syntax: bool,
    product_capabilities: ProductCapabilities,
}

impl<'a> Parser<'a> {
    fn new(source: &'a SourceText, scanned: super::ScanOutput) -> Self {
        Self {
            source,
            tokens: scanned.tokens,
            index: 0,
            next_node: 0,
            diagnostics: scanned.diagnostics,
            template_literals: scanned.template_literals,
            string_literals: scanned.string_literals,
            line_continuation_string_literals: scanned.line_continuation_string_literals,
            regular_expression_literals: scanned.regular_expression_literals,
            numeric_literals: scanned.numeric_literals,
            separated_numeric_literals: scanned.separated_numeric_literals,
            has_unmodeled_numeric_separator: scanned.has_unmodeled_numeric_separator,
            numeric_parser_diagnostics: Vec::new(),
            comments: scanned.comments,
            has_unicode_line_comment_terminator: scanned.has_unicode_line_comment_terminator,
            has_unmodeled_trivia: scanned.has_unmodeled_trivia,
            speculating: false,
            speculative_token_rewrites: Vec::new(),
            type_member_recovery_code: 1128,
            statement_nesting_depth: 0,
            has_unmodeled_top_level_syntax: false,
            product_capabilities: ProductCapabilities::all_supported(),
        }
    }

    fn parse(mut self) -> ParseOutput {
        let mut statements = Vec::new();
        while !self.at(TokenKind::EndOfFile) {
            let before = self.index;
            statements.push(self.parse_statement_at_current_depth());
            if self.index == before {
                self.bump();
            }
        }
        let has_authored_no_substitution_template =
            self.finish_no_substitution_template_source(&statements);
        let has_authored_extended_unicode_string =
            self.finish_extended_unicode_string_source(&statements);
        let has_authored_regular_expression = self.finish_regular_expression_source(&statements);
        let has_authored_numeric_recovery = self.finish_numeric_recovery_source(&statements);
        let has_authored_numeric_separator = self.finish_numeric_separator_source();
        let end = self.source.text.len();
        ParseOutput {
            unit: super::SourceUnit {
                statements,
                span: Span::new(self.source.id, 0, end),
                function_products_supported: self.product_capabilities.functions_supported,
                class_products_supported: self.product_capabilities.classes_supported,
                declaration_products_supported: self.product_capabilities.declarations_supported,
                declaration_hosts_supported: self.product_capabilities.declaration_hosts_supported,
                default_export_hosts_supported: self
                    .product_capabilities
                    .default_export_hosts_supported,
                expression_products_supported: self
                    .product_capabilities
                    .expression_products_supported,
                comments: self.comments,
                has_unicode_line_comment_terminator: self.has_unicode_line_comment_terminator,
                has_authored_no_substitution_template,
                template_products_supported: self.product_capabilities.template_products_supported,
                has_authored_extended_unicode_string,
                extended_unicode_string_products_supported: self
                    .product_capabilities
                    .extended_unicode_string_products_supported,
                has_authored_regular_expression,
                regular_expression_products_supported: self
                    .product_capabilities
                    .regular_expression_products_supported,
                has_authored_numeric_recovery,
                numeric_recovery_products_supported: self
                    .product_capabilities
                    .numeric_recovery_products_supported,
                has_authored_numeric_separator,
                numeric_separator_products_supported: self
                    .product_capabilities
                    .numeric_separator_products_supported,
                commonjs_class_products_supported: self
                    .product_capabilities
                    .commonjs_classes_supported(),
            },
            diagnostics: self.diagnostics,
        }
    }

    fn parse_statement(&mut self) -> Statement {
        self.statement_nesting_depth += 1;
        let statement = self.parse_statement_at_current_depth();
        self.statement_nesting_depth -= 1;
        statement
    }

    fn parse_statement_at_current_depth(&mut self) -> Statement {
        let start = self.current().span.start as usize;
        self.observe_regular_expression_in_unsupported_statement();
        if self.starts_import_declaration() {
            if self.statement_nesting_depth > 0 {
                self.has_unmodeled_top_level_syntax = true;
            }
            let kind = StatementKind::Import(self.parse_product_owned_import_declaration());
            let end = self.previous_end().max(start);
            return Statement {
                id: self.alloc_node(),
                span: Span::new(self.source.id, start, end),
                kind,
            };
        }
        if self.starts_export_declaration() {
            if self.statement_nesting_depth > 0 {
                self.has_unmodeled_top_level_syntax = true;
            }
            let kind = StatementKind::Export(self.parse_product_owned_export_declaration());
            let end = self.previous_end().max(start);
            return Statement {
                id: self.alloc_node(),
                span: Span::new(self.source.id, start, end),
                kind,
            };
        }
        let modifiers = self.parse_modifiers();
        self.observe_statement_modifiers(modifiers);
        let kind = match self.kind() {
            TokenKind::Let | TokenKind::Const | TokenKind::Var => {
                StatementKind::Variable(self.parse_variable(modifiers.exported))
            }
            TokenKind::Function => StatementKind::Function(self.parse_function(modifiers)),
            TokenKind::Class => {
                let declaration = self.parse_class(modifiers);
                self.observe_class_template_semantics(&declaration);
                StatementKind::Class(declaration)
            }
            TokenKind::Type if self.starts_type_alias_declaration() => {
                StatementKind::TypeAlias(self.parse_product_owned_type_alias(modifiers.exported))
            }
            TokenKind::Interface => {
                StatementKind::Interface(self.parse_product_owned_interface(modifiers.exported))
            }
            TokenKind::If => StatementKind::If(self.parse_if_statement()),
            TokenKind::Switch => StatementKind::Switch(self.parse_switch_statement()),
            TokenKind::Break => StatementKind::Break(self.parse_jump_statement()),
            TokenKind::Continue => StatementKind::Continue(self.parse_jump_statement()),
            TokenKind::Return => {
                self.bump();
                let expression = if self.at_any(&[
                    TokenKind::Semicolon,
                    TokenKind::RightBrace,
                    TokenKind::EndOfFile,
                ]) || !self
                    .tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
                {
                    None
                } else {
                    Some(self.parse_expression())
                };
                self.eat(TokenKind::Semicolon);
                StatementKind::Return(expression)
            }
            TokenKind::LeftBrace => StatementKind::Block(self.parse_block()),
            TokenKind::Semicolon => {
                self.bump();
                StatementKind::Empty
            }
            _ if modifiers.exported || modifiers.declared || modifiers.is_async => {
                self.error_current("Declaration expected.", 1146);
                self.recover_statement();
                StatementKind::Unknown
            }
            _ => {
                let expression = self.parse_expression();
                if !self.finish_numeric_recovery_expression_statement(&expression) {
                    self.finish_expression_statement();
                }
                StatementKind::Expression(expression)
            }
        };
        let end = self.previous_end().max(start);
        Statement {
            id: self.alloc_node(),
            span: Span::new(self.source.id, start, end),
            kind,
        }
    }

    fn parse_if_statement(&mut self) -> IfStatement {
        self.bump();
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let condition = self.parse_expression();
        self.observe_template_expression_semantics(&condition);
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        let then_statement = Box::new(self.parse_statement());
        let else_statement = self
            .eat(TokenKind::Else)
            .then(|| Box::new(self.parse_statement()));
        IfStatement {
            condition,
            then_statement,
            else_statement,
        }
    }

    fn parse_switch_statement(&mut self) -> SwitchStatement {
        self.bump();
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let expression = self.parse_expression();
        self.observe_template_expression_semantics(&expression);
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);

        let mut clauses = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let kind = if self.eat(TokenKind::Case) {
                let expression = self.parse_expression();
                self.observe_template_expression_semantics(&expression);
                self.expect(TokenKind::Colon, "':' expected.", 1005);
                SwitchClauseKind::Case(expression)
            } else if self.eat(TokenKind::Default) {
                self.expect(TokenKind::Colon, "':' expected.", 1005);
                SwitchClauseKind::Default
            } else {
                self.error_current("'case' or 'default' expected.", 1130);
                self.bump();
                continue;
            };

            let mut statements = Vec::new();
            while !self.at_any(&[
                TokenKind::Case,
                TokenKind::Default,
                TokenKind::RightBrace,
                TokenKind::EndOfFile,
            ]) {
                let before = self.index;
                statements.push(self.parse_statement());
                if self.index == before {
                    self.bump();
                }
            }
            let end = statements
                .last()
                .map_or_else(|| self.previous().span, |statement| statement.span);
            clauses.push(SwitchClause {
                span: start.merge(end),
                kind,
                statements,
            });
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        SwitchStatement {
            expression,
            clauses,
        }
    }

    fn starts_import_declaration(&self) -> bool {
        self.at(TokenKind::Import)
            && !matches!(
                self.peek_kind(1),
                TokenKind::LeftParen | TokenKind::LessThan | TokenKind::Dot
            )
    }

    fn starts_type_alias_declaration(&self) -> bool {
        self.at(TokenKind::Type)
            && token_is_binding_identifier(self.peek_kind(1))
            && self.tokens_are_on_same_line(self.index, self.index + 1)
    }

    fn parse_import_declaration(&mut self) -> ImportDeclaration {
        self.bump();
        if self.at(TokenKind::StringLiteral) {
            let (module_specifier, module_span) = self.parse_module_specifier();
            self.eat(TokenKind::Semicolon);
            return ImportDeclaration {
                bindings: Vec::new(),
                module_specifier,
                module_span,
                type_only: false,
                side_effect_only: true,
            };
        }

        let type_only = self.import_starts_with_type_only_clause();
        if type_only {
            self.bump();
        }
        let mut bindings = Vec::new();
        if token_is_binding_identifier(self.kind()) {
            let (local, local_span) = self.parse_name();
            bindings.push(ImportBinding {
                imported: Some("default".to_string()),
                local,
                local_span,
                type_only,
                namespace: false,
            });
            if self.eat(TokenKind::Equals) {
                self.eat(TokenKind::Require);
                self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
                let (module_specifier, module_span) = self.parse_module_specifier();
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                self.eat(TokenKind::Semicolon);
                return ImportDeclaration {
                    bindings,
                    module_specifier,
                    module_span,
                    type_only,
                    side_effect_only: false,
                };
            }
            self.eat(TokenKind::Comma);
        }

        if self.eat(TokenKind::Star) {
            self.expect(TokenKind::As, "'as' expected.", 1005);
            let (local, local_span) = self.parse_name();
            bindings.push(ImportBinding {
                imported: None,
                local,
                local_span,
                type_only,
                namespace: true,
            });
        } else if self.eat(TokenKind::LeftBrace) {
            while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
                let specifier_type_only = type_only || self.parse_specifier_type_modifier();
                let (imported, imported_span) = self.parse_identifier_name();
                let (local, local_span) = if self.eat(TokenKind::As) {
                    self.parse_name()
                } else {
                    (imported.clone(), imported_span)
                };
                bindings.push(ImportBinding {
                    imported: Some(imported),
                    local,
                    local_span,
                    type_only: specifier_type_only,
                    namespace: false,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        }

        self.expect(TokenKind::From, "'from' expected.", 1005);
        let (module_specifier, module_span) = self.parse_module_specifier();
        self.eat(TokenKind::Semicolon);
        ImportDeclaration {
            bindings,
            module_specifier,
            module_span,
            type_only,
            side_effect_only: false,
        }
    }

    fn import_starts_with_type_only_clause(&self) -> bool {
        if !self.at(TokenKind::Type) {
            return false;
        }

        let following = self.peek_kind(1);
        let following_is_identifier = token_is_binding_identifier(following);
        let phase_can_precede_from = following != TokenKind::From
            || (following_is_identifier
                && matches!(self.peek_kind(2), TokenKind::From | TokenKind::Equals));
        let phase_has_clause =
            following_is_identifier || matches!(following, TokenKind::Star | TokenKind::LeftBrace);
        phase_can_precede_from && phase_has_clause
    }

    fn parse_export_declaration(&mut self) -> ExportDeclaration {
        self.product_capabilities.observe_module_export();
        self.expect(TokenKind::Export, "'export' expected.", 1005);
        let default_export = self.eat(TokenKind::Default);
        if default_export || self.eat(TokenKind::Equals) {
            let assignment = self.parse_expression();
            self.finish_expression_statement();
            return ExportDeclaration {
                specifiers: Vec::new(),
                module_specifier: None,
                module_span: None,
                type_only: false,
                export_all: false,
                default_export,
                assignment: Some(assignment),
            };
        }
        if self.eat(TokenKind::As) {
            self.eat(TokenKind::Namespace);
            let _ = self.parse_identifier_name();
            self.eat(TokenKind::Semicolon);
            return ExportDeclaration {
                specifiers: Vec::new(),
                module_specifier: None,
                module_span: None,
                type_only: false,
                export_all: false,
                default_export: false,
                assignment: None,
            };
        }

        let type_only = self.eat(TokenKind::Type);
        let export_all = self.eat(TokenKind::Star);
        let mut specifiers = Vec::new();
        if export_all {
            if self.eat(TokenKind::As) {
                let (exported, exported_span) = self.parse_identifier_name();
                specifiers.push(ExportSpecifier {
                    local: exported.clone(),
                    local_span: exported_span,
                    exported,
                    exported_span,
                    type_only,
                });
            }
        } else {
            self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
            while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
                let specifier_type_only = type_only || self.parse_specifier_type_modifier();
                let (local, local_span) = self.parse_identifier_name();
                let (exported, exported_span) = if self.eat(TokenKind::As) {
                    self.parse_identifier_name()
                } else {
                    (local.clone(), local_span)
                };
                specifiers.push(ExportSpecifier {
                    local,
                    local_span,
                    exported,
                    exported_span,
                    type_only: specifier_type_only,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        }
        let (module_specifier, module_span) = if self.eat(TokenKind::From) {
            let (text, span) = self.parse_module_specifier();
            (Some(text), Some(span))
        } else {
            (None, None)
        };
        self.eat(TokenKind::Semicolon);
        ExportDeclaration {
            specifiers,
            module_specifier,
            module_span,
            type_only,
            export_all,
            default_export: false,
            assignment: None,
        }
    }

    fn parse_specifier_type_modifier(&mut self) -> bool {
        if !self.specifier_starts_with_type_modifier() {
            return false;
        }
        self.bump();
        true
    }

    fn specifier_starts_with_type_modifier(&self) -> bool {
        if !self.at(TokenKind::Type) {
            return false;
        }

        match self.peek_kind(1) {
            TokenKind::RightBrace | TokenKind::Comma | TokenKind::EndOfFile => false,
            TokenKind::As => match self.peek_kind(2) {
                TokenKind::RightBrace | TokenKind::Comma | TokenKind::EndOfFile => true,
                TokenKind::As => !matches!(
                    self.peek_kind(3),
                    TokenKind::RightBrace | TokenKind::Comma | TokenKind::EndOfFile
                ),
                _ => false,
            },
            _ => true,
        }
    }

    fn parse_variable(&mut self, exported: bool) -> VariableDeclaration {
        let declaration_kind = match self.kind() {
            TokenKind::Const => VariableKind::Const,
            TokenKind::Var => VariableKind::Var,
            _ => VariableKind::Let,
        };
        self.bump();
        let (name, name_span) = self.parse_name();
        let annotation = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        self.eat(TokenKind::Semicolon);
        VariableDeclaration {
            declaration_kind,
            name,
            name_span,
            annotation,
            initializer,
            exported,
        }
    }

    fn parse_function(&mut self, modifiers: Modifiers) -> FunctionDeclaration {
        let diagnostic_count = self.diagnostics.len();
        self.expect(TokenKind::Function, "'function' expected.", 1005);
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters();
        let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let has_body = self.at(TokenKind::LeftBrace);
        let body = if has_body {
            self.parse_block()
        } else {
            self.eat(TokenKind::Semicolon);
            Vec::new()
        };
        let overload_completion_supported = !modifiers.unsupported_for_overload_completion
            && parameters.iter().all(|parameter| {
                if has_body {
                    parameter.function_implementation_completion_supported
                } else {
                    parameter.overload_completion_supported
                }
            })
            && self.diagnostics.len() == diagnostic_count;
        self.product_capabilities
            .observe_function(modifiers, overload_completion_supported);
        FunctionDeclaration {
            name,
            name_span,
            type_parameters,
            parameters,
            return_type,
            body,
            has_body,
            exported: modifiers.exported,
            default_export: modifiers.default_export,
            is_async: modifiers.is_async,
            declared: modifiers.declared,
            abstract_declaration: modifiers.abstract_declaration,
            overload_completion_supported,
        }
    }

    fn parse_class(&mut self, modifiers: Modifiers) -> ClassDeclaration {
        self.expect(TokenKind::Class, "'class' expected.", 1005);
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        let extends = if self.eat(TokenKind::Extends) {
            Some(self.parse_type())
        } else {
            None
        };
        let mut implements = Vec::new();
        if self.eat(TokenKind::Implements) {
            loop {
                implements.push(self.parse_type());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut members = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            if self.eat(TokenKind::Semicolon) {
                continue;
            }
            let before = self.index;
            if let Some(member) = self.parse_class_member() {
                members.push(member);
            }
            if self.index == before {
                self.bump();
            }
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        self.product_capabilities.observe_class(modifiers, &members);
        ClassDeclaration {
            name,
            name_span,
            type_parameters,
            extends,
            implements,
            members,
            exported: modifiers.exported,
            default_export: modifiers.default_export,
            declared: modifiers.declared,
            abstract_class: modifiers.abstract_declaration,
        }
    }

    fn parse_class_member(&mut self) -> Option<ClassMember> {
        let diagnostic_count = self.diagnostics.len();
        let start = self.current().span;
        let mut modifiers = ClassMemberModifiers::default();
        loop {
            match self.kind() {
                TokenKind::Public => modifiers.observe(ParameterModifier::Public),
                TokenKind::Protected => modifiers.observe(ParameterModifier::Protected),
                TokenKind::Private => modifiers.observe(ParameterModifier::Private),
                TokenKind::Readonly => modifiers.observe(ParameterModifier::Readonly),
                TokenKind::Static => modifiers.observe(ParameterModifier::Static),
                TokenKind::Abstract => modifiers.observe(ParameterModifier::Abstract),
                TokenKind::Declare => modifiers.observe(ParameterModifier::Declare),
                TokenKind::Async => modifiers.observe(ParameterModifier::Async),
                TokenKind::Override => modifiers.observe(ParameterModifier::Override),
                TokenKind::Accessor => modifiers.observe(ParameterModifier::Accessor),
                _ => break,
            }
            self.bump();
        }

        if self.at(TokenKind::Constructor) {
            let name_span = self.bump().span;
            let parameters = self.parse_parameters();
            let has_body = self.at(TokenKind::LeftBrace);
            let body = if has_body {
                self.parse_block()
            } else {
                self.eat(TokenKind::Semicolon);
                Vec::new()
            };
            return Some(ClassMember {
                id: self.alloc_node(),
                name: "constructor".to_string(),
                name_span,
                span: start.merge(self.previous().span),
                overload_completion_supported: !modifiers.unsupported_for_overload_completion
                    && self.diagnostics.len() == diagnostic_count,
                emit_products_supported: modifiers.constructor_products_supported()
                    && self.diagnostics.len() == diagnostic_count,
                modifiers,
                kind: ClassMemberKind::Constructor {
                    parameters,
                    body,
                    has_body,
                },
            });
        }

        let accessor = match self.kind() {
            TokenKind::Get => {
                self.bump();
                Some(AccessorKind::Get)
            }
            TokenKind::Set => {
                self.bump();
                Some(AccessorKind::Set)
            }
            _ => None,
        };
        let (name, name_span, identifier_name) = self.parse_property_name();
        let optional = self.eat(TokenKind::Question);
        let definite = self.eat(TokenKind::Bang);
        let type_parameters = self.parse_type_parameters();
        if self.at(TokenKind::LeftParen) || accessor.is_some() {
            let parameters = self.parse_parameters();
            let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
            let has_body = self.at(TokenKind::LeftBrace);
            let body = if has_body {
                self.parse_block()
            } else {
                self.eat(TokenKind::Semicolon);
                Vec::new()
            };
            return Some(ClassMember {
                id: self.alloc_node(),
                name,
                name_span,
                span: start.merge(self.previous().span),
                overload_completion_supported: identifier_name
                    && !optional
                    && !definite
                    && self.diagnostics.len() == diagnostic_count,
                emit_products_supported: modifiers.method_products_supported()
                    && !optional
                    && !definite
                    && self.diagnostics.len() == diagnostic_count,
                modifiers,
                kind: ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    has_body,
                    accessor,
                },
            });
        }
        let annotation = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        self.eat(TokenKind::Semicolon);
        Some(ClassMember {
            id: self.alloc_node(),
            name,
            name_span,
            span: start.merge(self.previous().span),
            overload_completion_supported: identifier_name
                && self.diagnostics.len() == diagnostic_count,
            emit_products_supported: modifiers.property_products_supported()
                && self.diagnostics.len() == diagnostic_count,
            modifiers,
            kind: ClassMemberKind::Property {
                annotation,
                initializer,
                optional,
                definite,
            },
        })
    }

    fn parse_type_alias(&mut self, exported: bool) -> TypeAliasDeclaration {
        self.bump();
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        self.expect(TokenKind::Equals, "'=' expected.", 1005);
        let ty = self.parse_type();
        self.eat(TokenKind::Semicolon);
        TypeAliasDeclaration {
            name,
            name_span,
            type_parameters,
            ty,
            exported,
        }
    }

    fn parse_interface(&mut self, exported: bool) -> InterfaceDeclaration {
        self.bump();
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        let mut extends = Vec::new();
        if self.eat(TokenKind::Extends) {
            loop {
                extends.push(self.parse_type());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        let members = self.parse_type_members();
        InterfaceDeclaration {
            name,
            name_span,
            type_parameters,
            extends,
            members,
            exported,
        }
    }

    fn parse_block(&mut self) -> Vec<Statement> {
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut statements = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let before = self.index;
            statements.push(self.parse_statement());
            if before == self.index {
                self.bump();
            }
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        statements
    }

    fn parse_type(&mut self) -> TypeNode {
        if self.eat(TokenKind::Asserts) {
            let start = *self.previous();
            let (parameter, parameter_span) = self.parse_identifier_name();
            let ty = self.eat(TokenKind::Is).then(|| Box::new(self.parse_type()));
            let end = ty.as_ref().map_or(parameter_span, |ty| ty.span);
            return TypeNode {
                span: start.span.merge(end),
                kind: TypeNodeKind::Predicate {
                    parameter,
                    parameter_span,
                    asserts: true,
                    ty,
                },
            };
        }
        let check_type = self.parse_union_type();
        if self.eat(TokenKind::Is) {
            let (parameter, parameter_span) = match &check_type.kind {
                TypeNodeKind::Reference {
                    name,
                    name_span,
                    arguments,
                } if arguments.is_empty() => (name.clone(), *name_span),
                _ => (self.text(check_type.span).to_string(), check_type.span),
            };
            let ty = self.parse_type();
            return TypeNode {
                span: check_type.span.merge(ty.span),
                kind: TypeNodeKind::Predicate {
                    parameter,
                    parameter_span,
                    asserts: false,
                    ty: Some(Box::new(ty)),
                },
            };
        }
        if !self.eat(TokenKind::Extends) {
            return check_type;
        }
        let extends_type = self.parse_union_type();
        if !self.eat(TokenKind::Question) {
            return check_type;
        }
        let true_type = self.parse_type();
        self.expect(TokenKind::Colon, "':' expected.", 1005);
        let false_type = self.parse_type();
        let span = check_type.span.merge(false_type.span);
        TypeNode {
            span,
            kind: TypeNodeKind::Conditional {
                check_type: Box::new(check_type),
                extends_type: Box::new(extends_type),
                true_type: Box::new(true_type),
                false_type: Box::new(false_type),
            },
        }
    }

    fn parse_union_type(&mut self) -> TypeNode {
        self.eat(TokenKind::Bar);
        let first = self.parse_intersection_type();
        if !self.eat(TokenKind::Bar) {
            return first;
        }
        let start = first.span;
        let mut members = vec![first];
        loop {
            members.push(self.parse_intersection_type());
            if !self.eat(TokenKind::Bar) {
                break;
            }
        }
        let end = members.last().map_or(start, |member| member.span);
        TypeNode {
            span: start.merge(end),
            kind: TypeNodeKind::Union(members),
        }
    }

    fn parse_intersection_type(&mut self) -> TypeNode {
        self.eat(TokenKind::Ampersand);
        let first = self.parse_postfix_type();
        if !self.eat(TokenKind::Ampersand) {
            return first;
        }
        let start = first.span;
        let mut members = vec![first];
        loop {
            members.push(self.parse_postfix_type());
            if !self.eat(TokenKind::Ampersand) {
                break;
            }
        }
        let end = members.last().map_or(start, |member| member.span);
        TypeNode {
            span: start.merge(end),
            kind: TypeNodeKind::Intersection(members),
        }
    }

    fn parse_postfix_type(&mut self) -> TypeNode {
        let mut ty = self.parse_primary_type();
        loop {
            if self.at(TokenKind::LeftBracket) && self.peek_kind(1) == TokenKind::RightBracket {
                self.bump();
                let right = self.bump().span;
                let span = ty.span.merge(right);
                ty = TypeNode {
                    span,
                    kind: TypeNodeKind::Array(Box::new(ty)),
                };
            } else if self.eat(TokenKind::LeftBracket) {
                let index = self.parse_type();
                let right = self.current().span;
                self.expect(TokenKind::RightBracket, "']' expected.", 1005);
                let span = ty.span.merge(right);
                ty = TypeNode {
                    span,
                    kind: TypeNodeKind::IndexedAccess {
                        object: Box::new(ty),
                        index: Box::new(index),
                    },
                };
            } else {
                break;
            }
        }
        ty
    }

    fn parse_primary_type(&mut self) -> TypeNode {
        let token = *self.current();
        if let Some(keyword) = self.parse_keyword_type() {
            return keyword;
        }
        match token.kind {
            TokenKind::True
            | TokenKind::False
            | TokenKind::NumericLiteral
            | TokenKind::BigIntLiteral
            | TokenKind::StringLiteral => {
                self.observe_unmodeled_numeric_separator_if_current();
                self.bump();
                TypeNode {
                    span: token.span,
                    kind: TypeNodeKind::Literal(self.literal_from(token)),
                }
            }
            TokenKind::KeyOf => {
                self.bump();
                let operand = self.parse_primary_type();
                TypeNode {
                    span: token.span.merge(operand.span),
                    kind: TypeNodeKind::KeyOf(Box::new(operand)),
                }
            }
            TokenKind::Readonly => {
                self.bump();
                let operand = self.parse_postfix_type();
                TypeNode {
                    span: token.span.merge(operand.span),
                    kind: TypeNodeKind::Readonly(Box::new(operand)),
                }
            }
            TokenKind::TypeOf => {
                let start = self.bump().span;
                let (name, name_span, segment_spans) = self.parse_entity_name();
                TypeNode {
                    span: start.merge(name_span),
                    kind: TypeNodeKind::TypeQuery {
                        name,
                        name_span,
                        segment_spans,
                    },
                }
            }
            TokenKind::Infer => {
                let start = self.bump().span;
                let (name, name_span) = self.parse_name();
                let constraint = self
                    .eat(TokenKind::Extends)
                    .then(|| Box::new(self.parse_union_type()));
                let end = constraint.as_ref().map_or(name_span, |ty| ty.span);
                TypeNode {
                    span: start.merge(end),
                    kind: TypeNodeKind::Infer {
                        name,
                        name_span,
                        constraint,
                    },
                }
            }
            TokenKind::New | TokenKind::Abstract
                if token.kind == TokenKind::New || self.peek_kind(1) == TokenKind::New =>
            {
                let start = token.span;
                let abstract_constructor = self.eat(TokenKind::Abstract);
                self.expect(TokenKind::New, "'new' expected.", 1005);
                let type_parameters = self.parse_type_parameters();
                let parameters = self.parse_parameters();
                self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
                let return_type = self.parse_type();
                TypeNode {
                    span: start.merge(return_type.span),
                    kind: TypeNodeKind::Constructor {
                        id: self.alloc_node(),
                        type_parameters,
                        parameters,
                        return_type: Box::new(return_type),
                        abstract_constructor,
                    },
                }
            }
            _ if token.kind == TokenKind::This || token.kind.is_identifier() => {
                let (name, name_span, _) = self.parse_entity_name();
                let has_arguments = self.at(TokenKind::LessThan);
                let arguments = self.parse_type_arguments();
                let end = if has_arguments {
                    self.previous().span
                } else {
                    name_span
                };
                TypeNode {
                    span: name_span.merge(end),
                    kind: TypeNodeKind::Reference {
                        name,
                        name_span,
                        arguments,
                    },
                }
            }
            TokenKind::LeftBrace if self.brace_starts_mapped_type() => self.parse_mapped_type(),
            TokenKind::LeftBrace => {
                let members = self.parse_type_members();
                TypeNode {
                    span: token.span.merge(self.previous().span),
                    kind: TypeNodeKind::Object(members),
                }
            }
            TokenKind::LeftBracket => {
                self.bump();
                let mut members = Vec::new();
                while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
                    members.push(self.parse_type());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let end = self.current().span;
                self.expect(TokenKind::RightBracket, "']' expected.", 1005);
                TypeNode {
                    span: token.span.merge(end),
                    kind: TypeNodeKind::Tuple(members),
                }
            }
            TokenKind::LeftParen => self.parse_parenthesized_or_function_type(),
            TokenKind::LessThan => self.parse_generic_function_type(),
            _ => {
                self.observe_unmodeled_regular_expression_if_current();
                self.observe_unmodeled_template_if_current();
                self.error_current("Type expected.", 1110);
                self.bump();
                TypeNode {
                    span: token.span,
                    kind: TypeNodeKind::Missing,
                }
            }
        }
    }

    fn brace_starts_mapped_type(&self) -> bool {
        if !self.at(TokenKind::LeftBrace) {
            return false;
        }
        let mut cursor = self.index + 1;
        match self.token_kind_at(cursor) {
            TokenKind::Readonly => cursor += 1,
            TokenKind::Plus | TokenKind::Minus
                if self.token_kind_at(cursor + 1) == TokenKind::Readonly =>
            {
                cursor += 2;
            }
            _ => {}
        }
        self.token_kind_at(cursor) == TokenKind::LeftBracket
            && self.token_kind_at(cursor + 1).is_identifier()
            && self.token_kind_at(cursor + 2) == TokenKind::In
    }

    fn parse_mapped_type(&mut self) -> TypeNode {
        let left = self.bump().span;
        let readonly = if self.eat(TokenKind::Readonly) {
            Some(true)
        } else if self.at_any(&[TokenKind::Plus, TokenKind::Minus])
            && self.peek_kind(1) == TokenKind::Readonly
        {
            let add = self.eat(TokenKind::Plus);
            if !add {
                self.bump();
            }
            self.bump();
            Some(add)
        } else {
            None
        };
        self.expect(TokenKind::LeftBracket, "'[' expected.", 1005);
        let (parameter, parameter_span) = self.parse_name();
        self.expect(TokenKind::In, "'in' expected.", 1005);
        let constraint = self.parse_type();
        let name_type = if self.eat(TokenKind::As) {
            Some(Box::new(self.parse_type()))
        } else {
            None
        };
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        let optional = if self.eat(TokenKind::Question) {
            Some(true)
        } else if self.at_any(&[TokenKind::Plus, TokenKind::Minus])
            && self.peek_kind(1) == TokenKind::Question
        {
            let add = self.eat(TokenKind::Plus);
            if !add {
                self.bump();
            }
            self.bump();
            Some(add)
        } else {
            None
        };
        self.expect(TokenKind::Colon, "':' expected.", 1005);
        let value_type = self.parse_type();
        self.eat(TokenKind::Semicolon);
        let mut members = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let before = self.index;
            members.push(self.parse_type_member());
            if self.index == before {
                self.bump();
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        TypeNode {
            span: left.merge(right),
            kind: TypeNodeKind::Mapped {
                parameter,
                parameter_span,
                constraint: Box::new(constraint),
                name_type,
                value_type: Box::new(value_type),
                readonly,
                optional,
                members,
            },
        }
    }

    fn parse_generic_function_type(&mut self) -> TypeNode {
        let start = self.current().span;
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters();
        self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
        let return_type = self.parse_type();
        TypeNode {
            span: start.merge(return_type.span),
            kind: TypeNodeKind::Function {
                id: self.alloc_node(),
                type_parameters,
                parameters,
                return_type: Box::new(return_type),
            },
        }
    }

    fn parse_parenthesized_or_function_type(&mut self) -> TypeNode {
        let left = self.bump().span;
        if self.paren_is_parameter_list() {
            let mut parameters = Vec::new();
            while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                parameters.push(self.parse_parameter());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RightParen, "')' expected.", 1005);
            self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
            let return_type = self.parse_type();
            return TypeNode {
                span: left.merge(return_type.span),
                kind: TypeNodeKind::Function {
                    id: self.alloc_node(),
                    type_parameters: Vec::new(),
                    parameters,
                    return_type: Box::new(return_type),
                },
            };
        }
        let inner = self.parse_type();
        let right = self.current().span;
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        TypeNode {
            span: left.merge(right),
            kind: TypeNodeKind::Parenthesized(Box::new(inner)),
        }
    }

    fn paren_is_parameter_list(&self) -> bool {
        let mut depth = 1_u32;
        let mut cursor = self.index;
        while let Some(token) = self.tokens.get(cursor) {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return self.tokens.get(cursor + 1).map(|token| token.kind)
                            == Some(TokenKind::FatArrow);
                    }
                }
                TokenKind::EndOfFile => return false,
                _ => {}
            }
            cursor += 1;
        }
        false
    }

    fn parse_expression(&mut self) -> Expression {
        let expression = self.parse_assignment_expression();
        self.observe_unmodeled_template_tail(&expression);
        expression
    }

    fn parse_assignment_expression(&mut self) -> Expression {
        let left = self.parse_binary_expression(0);
        if self.eat(TokenKind::Equals) {
            self.observe_template_expression_semantics(&left);
            let right = self.parse_assignment_expression();
            let span = left.span.merge(right.span);
            Expression {
                id: self.alloc_node(),
                span,
                kind: ExpressionKind::Assignment {
                    left: Box::new(left),
                    right: Box::new(right),
                },
            }
        } else {
            left
        }
    }

    fn parse_binary_expression(&mut self, minimum_precedence: u8) -> Expression {
        let mut expression = self.parse_unary_expression();
        if expression_has_recovered_left_edge(&expression) {
            self.observe_unmodeled_regular_expression_if_current();
        }
        while let Some((operator, precedence)) = binary_operator(self.kind()) {
            if precedence < minimum_precedence {
                break;
            }
            self.bump();
            let right = self.parse_binary_expression(precedence + 1);
            let span = expression.span.merge(right.span);
            expression = Expression {
                id: self.alloc_node(),
                span,
                kind: ExpressionKind::Binary {
                    left: Box::new(expression),
                    operator,
                    right: Box::new(right),
                },
            };
            self.observe_template_expression_semantics(&expression);
        }
        expression
    }

    fn parse_unary_expression(&mut self) -> Expression {
        if let Some(expression) = self.parse_unsupported_await_template() {
            return expression;
        }
        let token = *self.current();
        let operator = match token.kind {
            TokenKind::Plus => Some(UnaryOperator::Plus),
            TokenKind::Minus => Some(UnaryOperator::Minus),
            TokenKind::Bang => Some(UnaryOperator::Not),
            TokenKind::Tilde => Some(UnaryOperator::BitwiseNot),
            TokenKind::TypeOf => Some(UnaryOperator::TypeOf),
            TokenKind::Void => Some(UnaryOperator::Void),
            TokenKind::Delete => Some(UnaryOperator::Delete),
            TokenKind::Await => Some(UnaryOperator::Await),
            _ => None,
        };
        let Some(operator) = operator else {
            return self.parse_postfix_expression();
        };
        self.bump();
        let operand = self.parse_unary_expression();
        let expression = Expression {
            id: self.alloc_node(),
            span: token.span.merge(operand.span),
            kind: ExpressionKind::Unary {
                operator,
                operand: Box::new(operand),
            },
        };
        self.observe_template_expression_semantics(&expression);
        expression
    }

    fn parse_postfix_expression(&mut self) -> Expression {
        let mut expression = self.parse_primary_expression();
        loop {
            if self.tag_type_arguments_are_followed_by_template() {
                self.parse_type_arguments();
                self.reject_tagged_template();
                break;
            }
            let has_type_arguments = self.call_type_arguments_are_followed_by_left_paren();
            if has_type_arguments || self.at(TokenKind::LeftParen) {
                let type_arguments = if has_type_arguments {
                    self.product_capabilities
                        .observe_explicit_call_type_arguments();
                    Some(self.parse_type_arguments())
                } else {
                    None
                };
                self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
                let mut arguments = Vec::new();
                while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                    arguments.push(self.parse_expression());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let right = self.current().span;
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                let span = expression.span.merge(right);
                expression = Expression {
                    id: self.alloc_node(),
                    span,
                    kind: ExpressionKind::Call {
                        callee: Box::new(expression),
                        type_arguments,
                        arguments,
                    },
                };
            } else if self.at(TokenKind::Dot) {
                let dot = self.current().span;
                if super::erased_expression_separated_number(&expression).is_some()
                    && expression.span.end != dot.start
                {
                    self.product_capabilities
                        .observe_unmodeled_numeric_separator();
                }
                self.bump();
                let (name, name_span) = self.parse_identifier_name();
                let span = expression.span.merge(name_span);
                expression = Expression {
                    id: self.alloc_node(),
                    span,
                    kind: ExpressionKind::Member {
                        object: Box::new(expression),
                        name,
                        name_span,
                    },
                };
            } else if self.eat(TokenKind::As) {
                let ty = self.parse_type();
                let span = expression.span.merge(ty.span);
                expression = Expression {
                    id: self.alloc_node(),
                    span,
                    kind: ExpressionKind::As {
                        expression: Box::new(expression),
                        ty,
                    },
                };
                self.observe_template_expression_semantics(&expression);
            } else if self.consume_non_null_template_host() {
            } else {
                self.observe_unmodeled_non_null_template_adjacency();
                self.reject_tagged_template();
                break;
            }
        }
        expression
    }

    fn parse_arrow_body(&mut self) -> ArrowBody {
        if self.at(TokenKind::LeftBrace) {
            ArrowBody::Block(self.parse_block())
        } else {
            ArrowBody::Expression(Box::new(self.parse_expression()))
        }
    }

    fn paren_expression_is_arrow(&mut self) -> bool {
        if !self.at(TokenKind::LeftParen) {
            return false;
        }
        let mut depth = 0_u32;
        for (cursor, token) in self.tokens.iter().enumerate().skip(self.index) {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let following = self.tokens.get(cursor + 1).map(|token| token.kind);
                        if following == Some(TokenKind::FatArrow) {
                            return true;
                        }
                        if following != Some(TokenKind::Colon) {
                            return false;
                        }
                        return self.type_annotation_is_followed_by_arrow(cursor + 2);
                    }
                }
                TokenKind::EndOfFile => break,
                _ => {}
            }
        }
        false
    }

    fn type_annotation_is_followed_by_arrow(&mut self, start: usize) -> bool {
        let saved_index = self.index;
        let saved_next_node = self.next_node;
        let saved_diagnostics = self.diagnostics.len();
        self.index = start;
        self.speculating = true;
        let _ = self.parse_type();
        let followed_by_arrow = self.at(TokenKind::FatArrow);
        for (index, token) in self.speculative_token_rewrites.drain(..).rev() {
            self.tokens[index] = token;
        }
        self.speculating = false;
        self.index = saved_index;
        self.next_node = saved_next_node;
        self.diagnostics.truncate(saved_diagnostics);
        followed_by_arrow
    }

    fn parse_object_literal(&mut self) -> Expression {
        let left = self.bump().span;
        let mut properties = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let (name, name_span, _) = self.parse_property_name();
            let value = if self.eat(TokenKind::Colon) {
                self.parse_expression()
            } else {
                Expression {
                    id: self.alloc_node(),
                    span: name_span,
                    kind: ExpressionKind::Identifier {
                        name: name.clone(),
                        name_span,
                        entity_name: true,
                    },
                }
            };
            let span = start.merge(value.span);
            properties.push(ObjectProperty {
                name,
                name_span,
                value,
                span,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Object(properties),
        }
    }

    fn parse_array_literal(&mut self) -> Expression {
        let left = self.bump().span;
        let mut elements = Vec::new();
        while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
            elements.push(self.parse_expression());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Array(elements),
        }
    }

    fn parse_name(&mut self) -> (String, Span) {
        let token = *self.current();
        if token_is_binding_identifier(token.kind) {
            self.bump();
            (self.text(token.span).to_string(), token.span)
        } else {
            self.error_current("Identifier expected.", 1003);
            self.bump();
            ("<missing>".to_string(), token.span)
        }
    }

    fn parse_entity_name(&mut self) -> (String, Span, Vec<Span>) {
        let (mut name, mut span) = self.parse_identifier_name();
        let mut segment_spans = vec![span];
        while self.eat(TokenKind::Dot) {
            let (right, right_span) = self.parse_identifier_name();
            name.push('.');
            name.push_str(&right);
            span = span.merge(right_span);
            segment_spans.push(right_span);
        }
        (name, span, segment_spans)
    }

    fn parse_identifier_name(&mut self) -> (String, Span) {
        let token = *self.current();
        if token_is_identifier_name(token.kind) {
            self.bump();
            (self.text(token.span).to_string(), token.span)
        } else {
            self.error_current("Identifier expected.", 1003);
            self.bump();
            ("<missing>".to_string(), token.span)
        }
    }

    fn parse_property_name(&mut self) -> (String, Span, bool) {
        let token = *self.current();
        self.observe_unmodeled_numeric_separator_if_current();
        match token.kind {
            TokenKind::StringLiteral | TokenKind::NumericLiteral | TokenKind::PrivateIdentifier => {
                self.bump();
                let name = if token.kind == TokenKind::StringLiteral {
                    self.ordinary_string_literal_value(token)
                } else {
                    self.text(token.span).to_string()
                };
                (name, token.span, false)
            }
            _ if token_is_identifier_name(token.kind) => {
                self.bump();
                (self.text(token.span).to_string(), token.span, true)
            }
            _ => {
                let (name, span) = self.parse_name();
                (name, span, false)
            }
        }
    }

    fn recover_statement(&mut self) {
        while !self.at_any(&[
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::EndOfFile,
        ]) {
            self.observe_unmodeled_regular_expression_if_current();
            self.observe_unmodeled_template_if_current();
            self.bump();
        }
        self.eat(TokenKind::Semicolon);
    }

    fn error_current(&mut self, message: &str, code: u32) {
        self.diagnostics.push(Diagnostic::at(
            self.source,
            self.current().span,
            message.to_string(),
            code,
        ));
    }

    fn expect(&mut self, kind: TokenKind, message: &str, code: u32) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error_current(message, code);
            false
        }
    }

    fn expect_type_close(&mut self) -> bool {
        if self.eat_type_close() {
            true
        } else {
            self.error_current("'>' expected.", 1005);
            false
        }
    }

    fn eat_type_close(&mut self) -> bool {
        let token = *self.current();
        match token.kind {
            TokenKind::GreaterThan => {
                self.bump();
                true
            }
            TokenKind::GreaterThanGreaterThan => {
                if self.speculating {
                    self.speculative_token_rewrites
                        .push((self.index, self.tokens[self.index]));
                }
                self.tokens[self.index] = Token {
                    kind: TokenKind::GreaterThan,
                    span: Span {
                        file: token.span.file,
                        start: token.span.start + 1,
                        end: token.span.end,
                    },
                };
                true
            }
            TokenKind::GreaterThanGreaterThanGreaterThan => {
                if self.speculating {
                    self.speculative_token_rewrites
                        .push((self.index, self.tokens[self.index]));
                }
                self.tokens[self.index] = Token {
                    kind: TokenKind::GreaterThanGreaterThan,
                    span: Span {
                        file: token.span.file,
                        start: token.span.start + 1,
                        end: token.span.end,
                    },
                };
                true
            }
            TokenKind::GreaterThanEquals => {
                if self.speculating {
                    self.speculative_token_rewrites
                        .push((self.index, self.tokens[self.index]));
                }
                self.tokens[self.index] = Token {
                    kind: TokenKind::Equals,
                    span: Span {
                        file: token.span.file,
                        start: token.span.start + 1,
                        end: token.span.end,
                    },
                };
                true
            }
            TokenKind::GreaterThanGreaterThanEquals => {
                if self.speculating {
                    self.speculative_token_rewrites
                        .push((self.index, self.tokens[self.index]));
                }
                self.tokens[self.index] = Token {
                    kind: TokenKind::GreaterThanEquals,
                    span: Span {
                        file: token.span.file,
                        start: token.span.start + 1,
                        end: token.span.end,
                    },
                };
                true
            }
            TokenKind::GreaterThanGreaterThanGreaterThanEquals => {
                if self.speculating {
                    self.speculative_token_rewrites
                        .push((self.index, self.tokens[self.index]));
                }
                self.tokens[self.index] = Token {
                    kind: TokenKind::GreaterThanGreaterThanEquals,
                    span: Span {
                        file: token.span.file,
                        start: token.span.start + 1,
                        end: token.span.end,
                    },
                };
                true
            }
            _ => false,
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.kind() == kind
    }

    fn at_type_close(&self) -> bool {
        matches!(
            self.kind(),
            TokenKind::GreaterThan
                | TokenKind::GreaterThanGreaterThan
                | TokenKind::GreaterThanGreaterThanGreaterThan
                | TokenKind::GreaterThanEquals
                | TokenKind::GreaterThanGreaterThanEquals
                | TokenKind::GreaterThanGreaterThanGreaterThanEquals
        )
    }

    fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.kind())
    }

    fn kind(&self) -> TokenKind {
        self.current().kind
    }

    fn peek_kind(&self, distance: usize) -> TokenKind {
        self.tokens
            .get(self.index + distance)
            .map_or(TokenKind::EndOfFile, |token| token.kind)
    }

    fn tokens_are_on_same_line(&self, left: usize, right: usize) -> bool {
        let Some(left) = self.tokens.get(left) else {
            return false;
        };
        let Some(right) = self.tokens.get(right) else {
            return false;
        };
        !self
            .source
            .slice(Span::new(
                self.source.id,
                left.span.end as usize,
                right.span.start as usize,
            ))
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len() - 1)]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index.saturating_sub(1)]
    }

    fn text(&self, span: Span) -> &str {
        self.source.slice(span)
    }

    fn bump(&mut self) -> Token {
        let token = *self.current();
        if token.kind != TokenKind::EndOfFile {
            self.index += 1;
        }
        token
    }

    fn previous_end(&self) -> usize {
        self.previous().span.end as usize
    }

    const fn alloc_node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }
}

const fn token_is_binding_identifier(kind: TokenKind) -> bool {
    kind.is_identifier()
}

const fn token_is_identifier_name(kind: TokenKind) -> bool {
    !matches!(
        kind,
        TokenKind::EndOfFile
            | TokenKind::PrivateIdentifier
            | TokenKind::NumericLiteral
            | TokenKind::BigIntLiteral
            | TokenKind::StringLiteral
            | TokenKind::RegularExpressionLiteral
            | TokenKind::NoSubstitutionTemplateLiteral
            | TokenKind::TemplateHead
            | TokenKind::TemplateMiddle
            | TokenKind::TemplateTail
            | TokenKind::LeftBrace
            | TokenKind::RightBrace
            | TokenKind::LeftParen
            | TokenKind::RightParen
            | TokenKind::LeftBracket
            | TokenKind::RightBracket
            | TokenKind::Colon
            | TokenKind::Semicolon
            | TokenKind::Comma
            | TokenKind::Dot
            | TokenKind::DotDotDot
            | TokenKind::Question
            | TokenKind::QuestionDot
            | TokenKind::QuestionQuestion
            | TokenKind::Equals
            | TokenKind::FatArrow
            | TokenKind::Plus
            | TokenKind::PlusPlus
            | TokenKind::PlusEquals
            | TokenKind::Minus
            | TokenKind::MinusMinus
            | TokenKind::MinusEquals
            | TokenKind::Star
            | TokenKind::StarStar
            | TokenKind::StarEquals
            | TokenKind::StarStarEquals
            | TokenKind::Slash
            | TokenKind::SlashEquals
            | TokenKind::Percent
            | TokenKind::PercentEquals
            | TokenKind::Bar
            | TokenKind::BarBar
            | TokenKind::BarEquals
            | TokenKind::BarBarEquals
            | TokenKind::Ampersand
            | TokenKind::AmpersandAmpersand
            | TokenKind::AmpersandEquals
            | TokenKind::AmpersandAmpersandEquals
            | TokenKind::Caret
            | TokenKind::CaretEquals
            | TokenKind::LessThan
            | TokenKind::LessThanSlash
            | TokenKind::LessThanEquals
            | TokenKind::LessThanLessThan
            | TokenKind::LessThanLessThanEquals
            | TokenKind::GreaterThan
            | TokenKind::GreaterThanEquals
            | TokenKind::GreaterThanGreaterThan
            | TokenKind::GreaterThanGreaterThanEquals
            | TokenKind::GreaterThanGreaterThanGreaterThan
            | TokenKind::GreaterThanGreaterThanGreaterThanEquals
            | TokenKind::Bang
            | TokenKind::BangEquals
            | TokenKind::BangEqualsEquals
            | TokenKind::EqualsEquals
            | TokenKind::EqualsEqualsEquals
            | TokenKind::QuestionQuestionEquals
            | TokenKind::Tilde
            | TokenKind::At
            | TokenKind::Hash
    )
}
