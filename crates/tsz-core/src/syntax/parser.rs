use crate::diagnostics::Diagnostic;
use crate::source::{NodeId, SourceText, Span};
mod arrows;
mod classes;
mod declarations;
mod element_access;
mod functions;
mod literals;
mod modifiers;
mod numeric_literal;
mod operators;
mod parameters;
mod recovery;
mod regular_expression;
mod source_unit;
mod statements;
mod string_literal;
mod type_arguments;
mod type_members;
mod type_parameters;
use self::{arrows::ParenthesizedArrowToken, modifiers::Modifiers};
use super::numeric_literal::{ScannedNumericLiteral, ScannedSeparatedNumberLiteral};
use super::regular_expression::ScannedRegularExpressionLiteral;
use super::scanner::ScannedIdentifierValue;
use super::string_literal::{ScannedCookedStringLiteral, ScannedStringLiteral};
use super::{
    CommentTrivia, ContextualGrammarFact, ContextualGrammarKind, ExportDeclaration,
    ExportSpecifier, Expression, ExpressionKind, ImportBinding, ImportDeclaration,
    InterfaceDeclaration, PropertyNameKind, SourceSyntaxFact, Statement, StatementKind, Token,
    TokenKind, TypeAliasDeclaration, TypeNode, TypeNodeKind, UnaryOperator,
    UnmodeledDeclarationHostFact, scan_source,
};
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
fn scan_at<T>(values: &[T], span: Span, span_of: impl Fn(&T) -> Span) -> Option<&T> {
    let value = values.get(
        values
            .binary_search_by_key(&span.start, |value| span_of(value).start)
            .ok()?,
    )?;
    (span_of(value) == span).then_some(value)
}
struct Parser<'a> {
    source: &'a SourceText,
    tokens: Vec<Token>,
    identifier_values: Vec<ScannedIdentifierValue>,
    index: usize,
    next_node: u32,
    diagnostics: Vec<Diagnostic>,
    string_literals: Vec<ScannedStringLiteral>,
    cooked_string_literals: Vec<ScannedCookedStringLiteral>,
    regular_expression_literals: Vec<ScannedRegularExpressionLiteral>,
    numeric_literals: Vec<ScannedNumericLiteral>,
    separated_numeric_literals: Vec<ScannedSeparatedNumberLiteral>,
    unterminated_template_spans: Vec<Span>,
    numeric_separator_spans: Vec<Span>,
    has_unmodeled_numeric_separator: bool,
    comments: Vec<CommentTrivia>,
    has_unicode_line_comment_terminator: bool,
    speculating: bool,
    not_parenthesized_arrows: std::collections::BTreeSet<usize>,
    speculative_token_rewrites: Vec<(usize, Token)>,
    type_member_recovery_code: u32,
    statement_nesting_depth: usize,
    in_yield_context: bool,
    in_await_context: bool,
    arrow_parameter_keyword_context: bool,
    await_binding_reserved: bool,
    yield_binding_reserved: bool,
    class_yield_binding_reserved: bool,
    pending_stray_statement_closes: usize,
    pending_stray_statement_closes_after_block: usize,
    parser_recovery_facts: Vec<recovery::PendingParserRecoveryFact>,
    unmodeled_declaration_hosts: Vec<UnmodeledDeclarationHostFact>,
    source_syntax_facts: std::collections::BTreeSet<SourceSyntaxFact>,
    contextual_grammar_facts: Vec<ContextualGrammarFact>,
}
impl<'a> Parser<'a> {
    fn new(source: &'a SourceText, scanned: super::ScanOutput) -> Self {
        let in_await_context = source_unit::source_is_external_module(source, &scanned.tokens);
        Self {
            source,
            tokens: scanned.tokens,
            identifier_values: scanned.identifier_values,
            index: 0,
            next_node: 0,
            diagnostics: scanned.diagnostics,
            string_literals: scanned.string_literals,
            cooked_string_literals: scanned.cooked_string_literals,
            regular_expression_literals: scanned.regular_expression_literals,
            numeric_literals: scanned.numeric_literals,
            separated_numeric_literals: scanned.separated_numeric_literals,
            unterminated_template_spans: scanned.unterminated_template_spans,
            numeric_separator_spans: scanned.numeric_separator_spans,
            has_unmodeled_numeric_separator: scanned.has_unmodeled_numeric_separator,
            comments: scanned.comments,
            has_unicode_line_comment_terminator: scanned.has_unicode_line_comment_terminator,
            speculating: false,
            not_parenthesized_arrows: std::collections::BTreeSet::new(),
            speculative_token_rewrites: Vec::new(),
            type_member_recovery_code: 1128,
            statement_nesting_depth: 0,
            in_yield_context: false,
            in_await_context,
            arrow_parameter_keyword_context: false,
            await_binding_reserved: false,
            yield_binding_reserved: false,
            class_yield_binding_reserved: false,
            pending_stray_statement_closes: 0,
            pending_stray_statement_closes_after_block: 0,
            parser_recovery_facts: Vec::new(),
            unmodeled_declaration_hosts: Vec::new(),
            source_syntax_facts: std::collections::BTreeSet::new(),
            contextual_grammar_facts: Vec::new(),
        }
    }
    fn record_contextual_grammar(&mut self, span: Span, kind: ContextualGrammarKind) {
        if !self.speculating {
            self.contextual_grammar_facts
                .push(ContextualGrammarFact { span, kind });
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
            let kind = StatementKind::Import(self.parse_import_declaration());
            return self.statement_from_kind(start, kind);
        }
        if self.starts_export_declaration() {
            let kind = StatementKind::Export(self.parse_export_declaration());
            return self.statement_from_kind(start, kind);
        }
        let has_leading_jsdoc = self.current_has_leading_jsdoc();
        let modifiers = self.parse_modifiers(start);
        self.observe_statement_modifiers(modifiers);
        if let Some(kind) = self.parse_opaque_host(start, modifiers) {
            return self.statement_from_kind(start, kind);
        }
        let kind = match self.kind() {
            TokenKind::Let | TokenKind::Const | TokenKind::Var => {
                StatementKind::Variable(self.parse_variable(modifiers, has_leading_jsdoc))
            }
            TokenKind::Function => {
                StatementKind::Function(self.parse_function(modifiers, has_leading_jsdoc))
            }
            TokenKind::Class => {
                let declaration = self.parse_class(modifiers);
                StatementKind::Class(declaration)
            }
            TokenKind::Type if self.starts_type_alias_declaration() => {
                StatementKind::TypeAlias(self.parse_type_alias(modifiers.exported))
            }
            TokenKind::Interface => {
                StatementKind::Interface(self.parse_interface(modifiers.exported))
            }
            TokenKind::If => StatementKind::If(self.parse_if_statement()),
            TokenKind::Switch => StatementKind::Switch(self.parse_switch_statement()),
            TokenKind::For if self.starts_unmodeled_for_binding_pattern() => {
                StatementKind::Block(self.parse_unmodeled_for_statement())
            }
            TokenKind::Break => StatementKind::Break(self.parse_jump_statement()),
            TokenKind::Continue => StatementKind::Continue(self.parse_jump_statement()),
            TokenKind::FatArrow if self.current_is_inside_rejected_generic_arrow_prefix() => {
                self.error_current("Declaration or statement expected.", 1128);
                self.bump();
                StatementKind::Unknown
            }
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
            TokenKind::LeftBrace => StatementKind::Block(self.parse_block().0),
            TokenKind::Dot => {
                self.error_current("Declaration or statement expected.", 1128);
                self.bump();
                StatementKind::Unknown
            }
            _ if self.recover_stray_statement() => StatementKind::Unknown,
            TokenKind::Semicolon => {
                self.bump();
                StatementKind::Empty
            }
            _ if modifiers.exported || modifiers.declared || modifiers.is_async => {
                self.error_modified_declaration(start);
                self.recover_statement(None);
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
        self.statement_from_kind(start, kind)
    }
    fn statement_from_kind(&mut self, start: usize, kind: StatementKind) -> Statement {
        Statement {
            id: self.alloc_node(),
            span: Span::new(self.source.id, start, self.previous_end().max(start)),
            kind,
        }
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
        if self.kind().is_identifier() {
            let (local, local_span) = self.parse_name();
            bindings.push(ImportBinding {
                imported: Some("default".to_string()),
                imported_span: None,
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
                imported_span: None,
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
                    imported_span: Some(imported_span),
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
        let following_is_identifier = following.is_identifier();
        let phase_can_precede_from = following != TokenKind::From
            || (following_is_identifier
                && matches!(self.peek_kind(2), TokenKind::From | TokenKind::Equals));
        let phase_has_clause =
            following_is_identifier || matches!(following, TokenKind::Star | TokenKind::LeftBrace);
        phase_can_precede_from && phase_has_clause
    }
    fn parse_export_declaration(&mut self) -> ExportDeclaration {
        self.source_syntax_facts
            .insert(SourceSyntaxFact::ModuleExport);
        self.expect(TokenKind::Export, "'export' expected.", 1005);
        let default_export = self.eat(TokenKind::Default);
        if default_export || self.eat(TokenKind::Equals) {
            let assignment = self.parse_assignment_expression();
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
        self.specifier_starts_with_type_modifier() && self.eat(TokenKind::Type)
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
                let operand = self.parse_postfix_type();
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
                let parameter_list_recovered = !self.at(TokenKind::LeftParen);
                let parameters = self.parse_parameters();
                self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
                let return_type = self.parse_type();
                TypeNode {
                    span: start.merge(return_type.span),
                    kind: TypeNodeKind::Constructor {
                        id: self.alloc_node(),
                        type_parameters,
                        parameters,
                        parameter_list_recovered,
                        return_type: Box::new(return_type),
                        abstract_constructor,
                    },
                }
            }
            TokenKind::This => {
                self.bump();
                TypeNode {
                    span: token.span,
                    kind: TypeNodeKind::This,
                }
            }
            _ if token.kind.is_identifier() => {
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
            TokenKind::LeftBracket => self.parse_tuple_type(),
            TokenKind::LeftParen => self.parse_parenthesized_or_function_type(),
            TokenKind::LessThan => self.parse_generic_function_type(),
            _ => self.recover_missing_type(token, true),
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
        let readonly = self.parse_mapped_type_modifier(TokenKind::Readonly);
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
        let optional = self.parse_mapped_type_modifier(TokenKind::Question);
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
    fn parse_mapped_type_modifier(&mut self, modifier: TokenKind) -> Option<bool> {
        if self.eat(modifier) {
            return Some(true);
        }
        if !self.at_any(&[TokenKind::Plus, TokenKind::Minus]) || self.peek_kind(1) != modifier {
            return None;
        }
        let add = self.eat(TokenKind::Plus);
        if !add {
            self.bump();
        }
        self.bump();
        Some(add)
    }
    fn parse_generic_function_type(&mut self) -> TypeNode {
        let start = self.current().span;
        let type_parameters = self.parse_type_parameters();
        let parameter_list_recovered = !self.at(TokenKind::LeftParen);
        let parameters = self.parse_signature_parameters();
        self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
        let return_type = self.parse_type();
        TypeNode {
            span: start.merge(return_type.span),
            kind: TypeNodeKind::Function {
                id: self.alloc_node(),
                type_parameters,
                parameters,
                parameter_list_recovered,
                return_type: Box::new(return_type),
            },
        }
    }
    fn parse_assignment_expression(&mut self) -> Expression {
        let has_leading_jsdoc = self.current_has_leading_jsdoc();
        let left = self.parse_conditional_expression();
        let operator = match self.kind() {
            TokenKind::Equals => Some(crate::syntax::AssignmentOperator::Assign),
            TokenKind::PlusEquals => Some(crate::syntax::AssignmentOperator::AddAssign),
            _ => None,
        };
        let expression = if let Some(operator) = operator {
            let operator_span = self.bump().span;
            let right = self.parse_assignment_expression();
            let span = left.span.merge(right.span);
            Expression {
                id: self.alloc_node(),
                span,
                kind: ExpressionKind::Assignment {
                    left: Box::new(left),
                    operator,
                    operator_span,
                    right: Box::new(right),
                    has_leading_jsdoc,
                },
            }
        } else {
            left
        };
        self.observe_unmodeled_template_tail(&expression);
        expression
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
            let operator_span = self.bump().span;
            let right = self.parse_binary_expression(precedence + 1);
            expression = Expression {
                id: self.alloc_node(),
                span: expression.span.merge(right.span),
                kind: ExpressionKind::Binary {
                    left: Box::new(expression),
                    operator,
                    operator_span,
                    right: Box::new(right),
                },
            };
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
            TokenKind::Await
                if self.in_await_context || self.identifier_value(token.span).is_none() =>
            {
                Some(UnaryOperator::Await)
            }
            _ => None,
        };
        let Some(operator) = operator else {
            return self.parse_postfix_expression();
        };
        self.bump();
        let operand = self.parse_unary_expression();
        Expression {
            id: self.alloc_node(),
            span: token.span.merge(operand.span),
            kind: ExpressionKind::Unary {
                operator,
                operand: Box::new(operand),
            },
        }
    }
    fn parse_postfix_expression(&mut self) -> Expression {
        let mut expression = self.parse_primary_expression();
        loop {
            if self.tag_type_arguments_are_followed_by_template() {
                self.parse_type_arguments();
                let span = self
                    .reject_tagged_template(expression.span)
                    .expect("lookahead found tagged template");
                expression = self.missing_expression(span);
                continue;
            }
            let has_type_arguments = self.call_type_arguments_are_followed_by_left_paren();
            if matches!(expression.kind, ExpressionKind::Missing)
                && self.expression_is_inside_rejected_generic_arrow_prefix(expression.span)
            {
                break;
            } else if has_type_arguments || self.at(TokenKind::LeftParen) {
                let type_arguments = if has_type_arguments {
                    self.source_syntax_facts
                        .insert(SourceSyntaxFact::ExplicitCallTypeArguments);
                    Some(self.parse_type_arguments())
                } else {
                    None
                };
                self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
                let mut arguments = Vec::new();
                let mut recover_rejected_generic_argument = false;
                while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                    if recover_rejected_generic_argument && self.at(TokenKind::FatArrow) {
                        self.error_current("Argument expression expected.", 1135);
                        self.bump();
                        continue;
                    }
                    recover_rejected_generic_argument |=
                        self.current_starts_rejected_generic_arrow_prefix();
                    let argument = self.parse_assignment_expression();
                    recover_rejected_generic_argument |=
                        self.expression_starts_rejected_generic_arrow_prefix(argument.span);
                    let representational_fragment =
                        matches!(argument.kind, ExpressionKind::Missing);
                    arguments.push(argument);
                    if self.eat(TokenKind::Comma) {
                        continue;
                    }
                    if !recover_rejected_generic_argument
                        || self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile])
                    {
                        break;
                    }
                    if !representational_fragment {
                        self.error_current("',' expected.", 1005);
                        if self.at(TokenKind::Colon) {
                            self.bump();
                        }
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
            } else if self.at_recovered_element_access(&expression) {
                break;
            } else if self.at_any(&[TokenKind::Dot, TokenKind::LeftBracket]) {
                expression = self.parse_member_access(expression);
            } else if self.eat(TokenKind::As) {
                let ty = self.parse_type();
                expression = Expression {
                    id: self.alloc_node(),
                    span: expression.span.merge(ty.span),
                    kind: ExpressionKind::As {
                        expression: Box::new(expression),
                        ty,
                    },
                };
            } else if let Some(bang) = self.consume_non_null_postfix() {
                expression = Expression {
                    id: self.alloc_node(),
                    span: expression.span.merge(bang),
                    kind: ExpressionKind::NonNull(Box::new(expression)),
                };
            } else {
                let Some(span) = self.observe_unmodeled_postfix_expression(expression.span) else {
                    break;
                };
                expression = self.missing_expression(span);
            }
        }
        expression
    }
    fn parse_name(&mut self) -> (String, Span) {
        let token = *self.current();
        if token.kind.is_identifier() {
            if self.arrow_parameter_keyword_context
                && (token.kind == TokenKind::Await && self.in_await_context
                    || token.kind == TokenKind::Yield && self.in_yield_context)
            {
                self.bump();
                return (self.text(token.span).to_string(), token.span);
            }
            if token.kind == TokenKind::Await && self.await_binding_reserved {
                self.record_contextual_grammar(token.span, ContextualGrammarKind::AwaitBinding);
            } else if token.kind == TokenKind::Yield && self.yield_binding_reserved {
                self.diagnose_strict_yield_identifier(token);
            }
        } else if token.kind.is_identifier_name() && self.identifier_value(token.span).is_some() {
            let authored = self.source.slice(token.span);
            self.diagnostics.push(Diagnostic::at(
                self.source,
                token.span,
                format!(
                    "Identifier expected. '{authored}' is a reserved word that cannot be used here."
                ),
                1359,
            ));
        } else {
            self.error_current("Identifier expected.", 1003);
            self.bump();
            return ("<missing>".to_string(), token.span);
        }
        self.bump_identifier();
        (self.text(token.span).to_string(), token.span)
    }
    fn parse_entity_name(&mut self) -> (String, Span, Vec<Span>) {
        let first = *self.current();
        self.diagnose_class_strict_yield(first);
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
    fn diagnose_class_strict_yield(&mut self, token: Token) -> bool {
        if token.kind != TokenKind::Yield || !self.class_yield_binding_reserved {
            return false;
        }
        self.record_contextual_grammar(token.span, ContextualGrammarKind::ClassStrictYieldBinding);
        true
    }
    fn diagnose_strict_yield_identifier(&mut self, token: Token) {
        if !self.diagnose_class_strict_yield(token) {
            self.record_contextual_grammar(token.span, ContextualGrammarKind::StrictYieldBinding);
        }
    }
    fn parse_identifier_name(&mut self) -> (String, Span) {
        let token = *self.current();
        if !token.kind.is_identifier_name() {
            self.error_current("Identifier expected.", 1003);
            self.bump();
            return ("<missing>".to_string(), token.span);
        }
        self.bump_identifier();
        (self.text(token.span).to_string(), token.span)
    }
    fn parse_property_name(&mut self) -> (String, Span, PropertyNameKind) {
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
                let kind = match token.kind {
                    TokenKind::StringLiteral => PropertyNameKind::StringLiteral,
                    TokenKind::NumericLiteral => PropertyNameKind::NumericLiteral,
                    TokenKind::PrivateIdentifier => PropertyNameKind::PrivateIdentifier,
                    _ => unreachable!(),
                };
                (name, token.span, kind)
            }
            TokenKind::LeftBracket => (
                "<computed>".to_string(),
                self.consume_balanced_tokens(
                    TokenKind::LeftBracket,
                    TokenKind::RightBracket,
                    "']' expected.",
                ),
                PropertyNameKind::Computed,
            ),
            _ if token.kind.is_identifier_name() => {
                let (name, span) = self.parse_identifier_name();
                (name, span, PropertyNameKind::Identifier)
            }
            _ => {
                let (name, span) = self.parse_name();
                (name, span, PropertyNameKind::Unsupported)
            }
        }
    }
    fn expect_type_close(&mut self) -> bool {
        if self.eat_type_close() {
            return true;
        }
        self.error_current("'>' expected.", 1005);
        false
    }
    fn eat_type_close(&mut self) -> bool {
        let token = *self.current();
        let kind = match token.kind {
            TokenKind::GreaterThan => {
                self.bump();
                return true;
            }
            TokenKind::GreaterThanGreaterThan => TokenKind::GreaterThan,
            TokenKind::GreaterThanGreaterThanGreaterThan => TokenKind::GreaterThanGreaterThan,
            TokenKind::GreaterThanEquals => TokenKind::Equals,
            TokenKind::GreaterThanGreaterThanEquals => TokenKind::GreaterThanEquals,
            TokenKind::GreaterThanGreaterThanGreaterThanEquals => {
                TokenKind::GreaterThanGreaterThanEquals
            }
            _ => return false,
        };
        if self.speculating {
            self.speculative_token_rewrites
                .push((self.index, self.tokens[self.index]));
        }
        self.tokens[self.index] = Token {
            kind,
            span: Span {
                start: token.span.start + 1,
                ..token.span
            },
        };
        true
    }
    fn eat(&mut self, kind: TokenKind) -> bool {
        if !self.at(kind) {
            return false;
        }
        self.bump();
        true
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
    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len() - 1)]
    }
    fn previous(&self) -> &Token {
        &self.tokens[self.index.saturating_sub(1)]
    }
    fn text(&self, span: Span) -> &str {
        self.identifier_value(span).map_or_else(
            || self.source.slice(span),
            |identifier| identifier.cooked.as_str(),
        )
    }
    fn identifier_value(&self, span: Span) -> Option<&ScannedIdentifierValue> {
        scan_at(&self.identifier_values, span, |identifier| identifier.span)
    }
    fn bump(&mut self) -> Token {
        let token = *self.current();
        if token.kind != TokenKind::Identifier
            && token.kind.is_identifier_name()
            && self.identifier_value(token.span).is_some()
        {
            self.error_current("Keywords cannot contain escape characters.", 1260);
        }
        self.bump_identifier()
    }
    /// Identifier nodes advance without TS1260; escaped keywords are illegal
    /// only when the surrounding grammar consumes them as keywords.
    fn bump_identifier(&mut self) -> Token {
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
