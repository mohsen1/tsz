use super::{Modifiers, Parser, recovery};
use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMember, ClassMemberKind, ClassMemberModifiers,
    ContextualGrammarKind, ParameterModifier, ParserRecoveryKind, PropertyNameKind, Token,
    TokenKind,
};

impl Parser<'_> {
    pub(super) fn parse_class(&mut self, modifiers: Modifiers) -> ClassDeclaration {
        let diagnostic_count = self.diagnostics.len();
        let recovery_fact_start = self.parser_recovery_facts.len();
        let previous_yield_binding_reserved = self.yield_binding_reserved;
        let previous_class_yield_binding_reserved = self.class_yield_binding_reserved;
        let inherited_yield_context = self.in_yield_context;
        self.yield_binding_reserved = true;
        self.class_yield_binding_reserved = true;
        self.expect(TokenKind::Class, "'class' expected.", 1005);
        let (name, name_span) = self.parse_name();
        self.yield_binding_reserved = !inherited_yield_context;
        self.class_yield_binding_reserved = !inherited_yield_context;
        let type_parameters = self.parse_type_parameters();
        let extends = self.eat(TokenKind::Extends).then(|| self.parse_type());
        let mut implements = Vec::new();
        if self.eat(TokenKind::Implements) {
            loop {
                implements.push(self.parse_type());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.yield_binding_reserved = true;
        self.class_yield_binding_reserved = true;
        let has_opening_brace = self.at(TokenKind::LeftBrace);
        let authored_body_extent = has_opening_brace
            .then(|| self.balanced_recovery_brace_extent(self.index))
            .flatten();
        let opening_brace = self.current().span;
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut members = Vec::new();
        let mut member_list_aborted = false;
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            if self.eat(TokenKind::Semicolon) {
                continue;
            }
            let before = self.index;
            let decorator_recovery = self.at(TokenKind::At);
            if decorator_recovery {
                self.yield_binding_reserved = false;
                self.class_yield_binding_reserved = false;
            }
            let (member, recovery) = self.parse_class_member();
            if decorator_recovery {
                self.yield_binding_reserved = true;
                self.class_yield_binding_reserved = true;
            }
            members.push(member);
            member_list_aborted = recovery.aborts_list();
            if member_list_aborted {
                break;
            }
            if self.index == before {
                self.bump();
            }
        }
        let has_closing_brace = !member_list_aborted && self.at(TokenKind::RightBrace);
        let closing_brace = self.current().span;
        if !member_list_aborted {
            self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        }
        self.yield_binding_reserved = previous_yield_binding_reserved;
        self.class_yield_binding_reserved = previous_class_yield_binding_reserved;
        if let Some(extent) = authored_body_extent
            && self.previous().span.end < extent.end
        {
            self.promote_parser_recovery_extent(recovery_fact_start, extent);
        }
        ClassDeclaration {
            name,
            name_span,
            type_parameters,
            extends,
            implements,
            members,
            body_span: (has_opening_brace && has_closing_brace)
                .then(|| opening_brace.merge(closing_brace)),
            exported: modifiers.exported,
            default_export: modifiers.default_export,
            declared: modifiers.declared,
            abstract_class: modifiers.abstract_declaration,
            member_syntax_recovery_free: self.diagnostics.len() == diagnostic_count,
        }
    }

    fn parse_class_member(&mut self) -> (ClassMember, recovery::ClassMemberListRecovery) {
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
            let previous_yield_context = self.in_yield_context;
            let previous_await_context = self.in_await_context;
            let previous_await_binding_reserved = self.await_binding_reserved;
            self.in_yield_context = false;
            self.in_await_context = false;
            self.await_binding_reserved = false;
            let parameters = self.parse_parameters();
            let has_body = self.at(TokenKind::LeftBrace);
            let (body, body_span) = if has_body {
                self.parse_block()
            } else {
                self.eat(TokenKind::Semicolon);
                (Vec::new(), None)
            };
            self.in_yield_context = previous_yield_context;
            self.in_await_context = previous_await_context;
            self.await_binding_reserved = previous_await_binding_reserved;
            return (
                ClassMember {
                    id: self.alloc_node(),
                    name: "constructor".to_string(),
                    name_span,
                    name_kind: PropertyNameKind::Identifier,
                    string_name_value: None,
                    span: start.merge(self.previous().span),
                    overload_completion_supported: !modifiers.unsupported_for_overload_completion
                        && self.diagnostics.len() == diagnostic_count,
                    emit_products_supported: modifiers.constructor_modifiers_are_modeled()
                        && self.diagnostics.len() == diagnostic_count,
                    modifiers,
                    kind: ClassMemberKind::Constructor {
                        type_parameters: Vec::new(),
                        parameters,
                        return_type: None,
                        body,
                        has_body,
                        body_span,
                    },
                },
                recovery::ClassMemberListRecovery::Continue,
            );
        }
        let generator_span = self.at(TokenKind::Star).then(|| self.bump().span);
        let generator_recovery = ParserRecoveryKind::GeneratorFunctionLike;
        let accessor = match self.kind() {
            TokenKind::Get if generator_span.is_none() && self.class_member_starts_accessor() => {
                self.bump();
                Some(AccessorKind::Get)
            }
            TokenKind::Set if generator_span.is_none() && self.class_member_starts_accessor() => {
                self.bump();
                Some(AccessorKind::Set)
            }
            _ => None,
        };
        let (name, name_span, name_kind) = self.parse_property_name();
        let string_name_value = (name_kind == PropertyNameKind::StringLiteral)
            .then(|| {
                self.cooked_string_literal(Token {
                    kind: TokenKind::StringLiteral,
                    span: name_span,
                })
            })
            .flatten();
        let quoted_constructor_name = accessor.is_none()
            && generator_span.is_none()
            && string_name_value.as_ref().is_some_and(|value| {
                value
                    .units()
                    .iter()
                    .copied()
                    .eq("constructor".encode_utf16())
            });
        let (optional, definite) = (self.eat(TokenKind::Question), self.eat(TokenKind::Bang));
        let previous_yield_context = self.in_yield_context;
        let previous_await_context = self.in_await_context;
        let previous_await_binding_reserved = self.await_binding_reserved;
        self.in_yield_context = generator_span.is_some();
        self.in_await_context = false;
        self.await_binding_reserved = false;
        let type_parameters = self.parse_type_parameters();
        if accessor.is_some() && !type_parameters.is_empty() {
            self.record_contextual_grammar(
                name_span,
                ContextualGrammarKind::AccessorTypeParameters,
            );
        }
        let quoted_constructor =
            quoted_constructor_name && type_parameters.is_empty() && !optional && !definite;
        self.in_await_context = modifiers.async_member;
        self.await_binding_reserved = modifiers.async_member;
        if self.at(TokenKind::LeftParen) && !(quoted_constructor_name && definite)
            || accessor.is_some()
        {
            let parameters = if accessor.is_some() {
                self.parse_accessor_parameters()
            } else {
                self.parse_parameters()
            };
            let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
            let has_body = self.at(TokenKind::LeftBrace);
            let generator_body_extent = generator_span.filter(|_| has_body).and_then(|generator| {
                self.balanced_recovery_brace_extent(self.index)
                    .map(|body| generator.merge(body))
            });
            if let (Some(generator), Some(extent)) = (generator_span, generator_body_extent) {
                self.retain_parser_recovery(generator_recovery, generator, extent);
            }
            let (body, body_span) = if has_body {
                self.parse_block()
            } else {
                self.eat(TokenKind::Semicolon);
                (Vec::new(), None)
            };
            self.in_yield_context = previous_yield_context;
            self.in_await_context = previous_await_context;
            self.await_binding_reserved = previous_await_binding_reserved;
            let constructor_products_supported = quoted_constructor
                && type_parameters.is_empty()
                && return_type.is_none()
                && !optional
                && !definite
                && self.diagnostics.len() == diagnostic_count;
            let kind = if quoted_constructor {
                ClassMemberKind::Constructor {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    has_body,
                    body_span,
                }
            } else {
                ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    has_body,
                    body_span,
                    accessor,
                }
            };
            let member = ClassMember {
                id: self.alloc_node(),
                name,
                name_span,
                name_kind,
                string_name_value,
                span: start.merge(self.previous().span),
                overload_completion_supported: if quoted_constructor {
                    constructor_products_supported && !modifiers.unsupported_for_overload_completion
                } else {
                    matches!(name_kind, PropertyNameKind::Identifier)
                        && generator_span.is_none()
                        && !optional
                        && !definite
                        && self.diagnostics.len() == diagnostic_count
                },
                emit_products_supported: if quoted_constructor {
                    constructor_products_supported && modifiers.constructor_modifiers_are_modeled()
                } else {
                    modifiers.method_modifiers_are_modeled()
                        && name_kind != PropertyNameKind::Unsupported
                        && generator_span.is_none()
                        && !optional
                        && !definite
                        && self.diagnostics.len() == diagnostic_count
                },
                modifiers,
                kind,
            };
            if let Some(generator_span) = generator_span
                && generator_body_extent.is_none()
            {
                self.retain_parser_recovery(generator_recovery, generator_span, member.span);
            }
            if name_kind == PropertyNameKind::Computed {
                self.record_parser_recovery_for_analysis(
                    ParserRecoveryKind::ComputedPropertyName,
                    name_span,
                    member.span,
                );
            }
            return (member, recovery::ClassMemberListRecovery::Continue);
        }
        let annotation = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        self.eat(TokenKind::Semicolon);
        let property_products_supported = self.diagnostics.len() == diagnostic_count;
        let member_list_recovery = if quoted_constructor_name && definite {
            self.recover_definite_property_call()
        } else {
            recovery::ClassMemberListRecovery::Continue
        };
        self.in_yield_context = previous_yield_context;
        self.in_await_context = previous_await_context;
        self.await_binding_reserved = previous_await_binding_reserved;
        let member = ClassMember {
            id: self.alloc_node(),
            name,
            name_span,
            name_kind,
            string_name_value,
            span: start.merge(self.previous().span),
            overload_completion_supported: matches!(name_kind, PropertyNameKind::Identifier)
                && generator_span.is_none()
                && property_products_supported,
            emit_products_supported: modifiers.property_modifiers_are_modeled()
                && name_kind != PropertyNameKind::Unsupported
                && generator_span.is_none()
                && property_products_supported,
            modifiers,
            kind: ClassMemberKind::Property {
                annotation,
                initializer,
                optional,
                definite,
            },
        };
        if let Some(generator_span) = generator_span {
            self.retain_parser_recovery(generator_recovery, generator_span, member.span);
        }
        if name_kind == PropertyNameKind::Computed {
            self.record_parser_recovery_for_analysis(
                ParserRecoveryKind::ComputedPropertyName,
                name_span,
                member.span,
            );
        }
        (member, member_list_recovery)
    }
}
