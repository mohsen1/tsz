use crate::source::Span;
use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind,
    FunctionDeclaration, KeywordType, Literal, Parameter, ParameterModifier, PropertyNameKind,
    Statement, StatementKind, StringLiteral, TypeMember, TypeMemberKind, TypeMemberName,
    TypeMemberNameKind, TypeNode, TypeNodeKind,
};

use super::{PREC_LOWEST, Printer, TYPE_PREC_LOWEST, literals};

impl Printer<'_> {
    pub(super) fn write_declaration_function(&mut self, declaration: &FunctionDeclaration) {
        self.write_indent();
        self.output.push_str(if declaration.default_export {
            "export default function "
        } else if declaration.exported {
            "export declare function "
        } else {
            "declare function "
        });
        self.write_authored_identifier(&declaration.name, declaration.name_span);
        self.write_type_parameters(&declaration.type_parameters);
        self.write_declaration_parameters(&declaration.parameters);
        self.output.push_str(": ");
        if let Some(return_type) = &declaration.return_type {
            self.write_type(return_type, TYPE_PREC_LOWEST);
        } else if declaration.has_body && declaration.body.is_empty() {
            self.output.push_str("void");
        } else if !declaration.has_body {
            self.output.push_str("any");
        } else {
            self.output.push_str("unknown");
        }
        self.output.push_str(";\n");
    }

    pub(super) fn write_declaration_class(&mut self, declaration: &ClassDeclaration) {
        self.write_indent();
        self.output.push_str(if declaration.default_export {
            "export default class "
        } else if declaration.exported {
            "export declare class "
        } else {
            "declare class "
        });
        self.write_authored_identifier(&declaration.name, declaration.name_span);
        self.write_type_parameters(&declaration.type_parameters);
        if let Some(base) = &declaration.extends {
            self.output.push_str(" extends ");
            self.write_type(base, TYPE_PREC_LOWEST);
        }
        if !declaration.implements.is_empty() {
            self.output.push_str(" implements ");
            self.write_type_list(&declaration.implements, ", ", TYPE_PREC_LOWEST);
        }
        self.output.push_str(" {\n");
        self.indent += 1;
        if declaration
            .members
            .iter()
            .any(|member| member.name_kind == PropertyNameKind::PrivateIdentifier)
        {
            self.write_indent();
            self.output.push_str("#private;\n");
        }
        if let Some(parameters) = declaration
            .members
            .iter()
            .find_map(|member| match &member.kind {
                ClassMemberKind::Constructor {
                    parameters,
                    has_body: true,
                    ..
                } => Some(parameters.as_slice()),
                _ => None,
            })
        {
            self.write_parameter_property_declarations(parameters);
        }
        for member in declaration
            .members
            .iter()
            .filter(|member| member.name_kind != PropertyNameKind::PrivateIdentifier)
        {
            self.write_indent();
            self.write_parts(&[
                if member.modifiers.private {
                    "private "
                } else if member.modifiers.protected {
                    "protected "
                } else {
                    ""
                },
                if member.modifiers.static_member {
                    "static "
                } else {
                    ""
                },
                if member.modifiers.readonly {
                    "readonly "
                } else {
                    ""
                },
            ]);
            match &member.kind {
                ClassMemberKind::Constructor { parameters, .. } => {
                    self.output.push_str("constructor");
                    if member.modifiers.private {
                        self.output.push_str("()");
                    } else {
                        let previous = self.declaration_parameter_property_host;
                        self.declaration_parameter_property_host = true;
                        self.write_declaration_parameters(parameters);
                        self.declaration_parameter_property_host = previous;
                    }
                    self.output.push_str(";\n");
                }
                ClassMemberKind::Property {
                    annotation,
                    initializer,
                    optional,
                    ..
                } => {
                    self.write_property_name(&member.name, member.name_span, member.name_kind);
                    if *optional {
                        self.output.push('?');
                    }
                    if member.modifiers.private {
                        self.output.push_str(";\n");
                        continue;
                    }
                    self.output.push_str(": ");
                    if let Some(annotation) = annotation {
                        self.write_type(annotation, TYPE_PREC_LOWEST);
                    } else {
                        if initializer
                            .as_ref()
                            .is_some_and(literals::expression_contains_template)
                        {
                            self.declaration_supported = false;
                        }
                        self.output.push_str("unknown");
                    }
                    self.output.push_str(";\n");
                }
                ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    has_body,
                    accessor,
                    ..
                } => {
                    if member.modifiers.private {
                        if let Some(accessor) = accessor {
                            self.write_private_accessor_declaration(
                                *accessor,
                                &member.name,
                                member.name_span,
                                member.name_kind,
                            );
                        } else {
                            self.write_property_name(
                                &member.name,
                                member.name_span,
                                member.name_kind,
                            );
                            self.output.push_str(";\n");
                        }
                        continue;
                    }
                    if let Some(accessor) = accessor {
                        self.output.push_str(match accessor {
                            AccessorKind::Get => "get ",
                            AccessorKind::Set => "set ",
                        });
                    }
                    self.write_property_name(&member.name, member.name_span, member.name_kind);
                    if accessor.is_none() {
                        self.write_type_parameters(type_parameters);
                    }
                    self.write_declaration_parameters(parameters);
                    if matches!(accessor, Some(AccessorKind::Set)) {
                        self.output.push_str(";\n");
                        continue;
                    }
                    self.output.push_str(": ");
                    if let Some(return_type) = return_type {
                        self.write_type(return_type, TYPE_PREC_LOWEST);
                    } else if !has_body {
                        self.output.push_str("any");
                    } else if body.is_empty() {
                        self.output.push_str("void");
                    } else {
                        self.declaration_supported = false;
                        self.output.push_str("unknown");
                    }
                    self.output.push_str(";\n");
                }
            }
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("}\n");
    }

    pub(super) fn write_parameter_property_fields(&mut self, parameters: &[Parameter]) {
        for parameter in parameter_properties(parameters) {
            self.write_indent();
            self.write_authored_identifier(&parameter.name, parameter.name_span);
            self.output.push_str(";\n");
        }
    }

    pub(super) fn write_declaration_parameter_type(&mut self, parameter: &Parameter) {
        if let Some(annotation) = &parameter.annotation {
            self.write_type(annotation, TYPE_PREC_LOWEST);
            return;
        }
        if let Some(initializer) = &parameter.initializer {
            self.write_declaration_initializer_type(initializer);
        } else {
            self.output.push_str("any");
        }
    }

    fn write_declaration_initializer_type(&mut self, initializer: &crate::syntax::Expression) {
        match &initializer.kind {
            ExpressionKind::Literal(Literal::String(_)) => self.output.push_str("string"),
            ExpressionKind::Literal(Literal::Number(_)) => self.output.push_str("number"),
            ExpressionKind::Literal(Literal::BigInt(_)) => self.output.push_str("bigint"),
            ExpressionKind::Literal(Literal::Boolean(_)) => self.output.push_str("boolean"),
            ExpressionKind::Literal(Literal::Null) => self.output.push_str("null"),
            _ => {
                self.declaration_supported = false;
                self.output.push_str("any");
            }
        }
    }

    pub(super) fn write_parameter_property_declarations(&mut self, parameters: &[Parameter]) {
        for parameter in parameter_properties(parameters) {
            self.write_indent();
            let private = has_parameter_modifier(parameter, ParameterModifier::Private);
            if private {
                self.output.push_str("private ");
            } else if has_parameter_modifier(parameter, ParameterModifier::Protected) {
                self.output.push_str("protected ");
            }
            if has_parameter_modifier(parameter, ParameterModifier::Readonly) {
                self.output.push_str("readonly ");
            }
            self.write_authored_identifier(&parameter.name, parameter.name_span);
            if parameter.optional {
                self.output.push('?');
            }
            if private {
                self.output.push_str(";\n");
                continue;
            }
            self.output.push_str(": ");
            self.write_declaration_parameter_type(parameter);
            if parameter.optional
                && parameter
                    .annotation
                    .as_ref()
                    .is_none_or(|annotation| !optional_type_absorbs_undefined(annotation))
            {
                self.output.push_str(" | undefined");
            }
            self.output.push_str(";\n");
        }
    }

    pub(super) fn write_private_accessor_declaration(
        &mut self,
        accessor: AccessorKind,
        name: &str,
        name_span: Span,
        name_kind: PropertyNameKind,
    ) {
        self.output.push_str(match accessor {
            AccessorKind::Get => "get ",
            AccessorKind::Set => "set ",
        });
        self.write_property_name(name, name_span, name_kind);
        match accessor {
            AccessorKind::Get => self.output.push_str("();\n"),
            AccessorKind::Set => self.output.push_str("(value);\n"),
        }
    }

    pub(super) fn write_constructor_body(
        &mut self,
        body_span: Option<crate::source::Span>,
        statements: &[Statement],
        parameters: &[Parameter],
        derived: bool,
    ) {
        if statements.is_empty() && !parameters.iter().any(Parameter::is_property) {
            self.write_braced_statements(body_span, statements);
            return;
        }
        self.output.push_str("{\n");
        self.indent += 1;
        let directive_count = statements.iter().map_while(directive).count();
        for statement in &statements[..directive_count] {
            self.write_javascript_statement(statement, false);
        }
        let mut wrote_assignments = false;
        if !derived {
            self.write_parameter_property_assignments(parameters);
            wrote_assignments = true;
        }
        for statement in &statements[directive_count..] {
            self.write_javascript_statement(statement, false);
            if derived && !wrote_assignments && is_super_call_statement(statement) {
                self.write_parameter_property_assignments(parameters);
                wrote_assignments = true;
            }
        }
        if !wrote_assignments {
            self.write_parameter_property_assignments(parameters);
        }
        if let Some(body_span) = body_span {
            self.write_comments_before_close(body_span.end);
            self.write_newline();
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push('}');
    }

    fn write_parameter_property_assignments(&mut self, parameters: &[Parameter]) {
        for parameter in parameter_properties(parameters) {
            self.write_indent();
            self.output.push_str("this.");
            self.write_authored_identifier(&parameter.name, parameter.name_span);
            self.output.push_str(" = ");
            self.write_authored_identifier(&parameter.name, parameter.name_span);
            self.output.push_str(";\n");
        }
    }

    pub(super) fn write_type_member(&mut self, member: &TypeMember) {
        if member.modifiers.readonly {
            self.output.push_str("readonly ");
        }
        match &member.kind {
            TypeMemberKind::Property {
                name,
                ty,
                optional,
                initializer,
            } => {
                self.write_type_member_name(name);
                if *optional {
                    self.output.push('?');
                }
                if let Some(ty) = ty {
                    self.output.push_str(": ");
                    self.write_type(ty, TYPE_PREC_LOWEST);
                } else if let Some(initializer) = initializer {
                    self.output.push_str(": ");
                    self.write_declaration_initializer_type(initializer);
                    if *optional {
                        self.output.push_str(" | undefined");
                    }
                } else {
                    self.output.push_str(": any");
                }
            }
            TypeMemberKind::Method {
                name,
                optional,
                type_parameters,
                parameters,
                return_type,
            } => {
                self.write_type_member_name(name);
                if *optional {
                    self.output.push('?');
                }
                self.write_type_parameters(type_parameters);
                self.write_declaration_parameters(parameters);
                self.write_type_member_return(return_type.as_ref(), Some("any"));
            }
            TypeMemberKind::Call {
                type_parameters,
                parameters,
                return_type,
            }
            | TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
            } => {
                if matches!(&member.kind, TypeMemberKind::Construct { .. }) {
                    self.output.push_str("new ");
                }
                self.write_type_parameters(type_parameters);
                self.write_declaration_parameters(parameters);
                self.write_type_member_return(return_type.as_ref(), Some("any"));
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                self.output.push('[');
                for (index, parameter) in parameters.iter().enumerate() {
                    if index != 0 {
                        self.output.push_str(", ");
                    }
                    if parameter.rest {
                        self.output.push_str("...");
                    }
                    self.write_authored_identifier(&parameter.name, parameter.name_span);
                    if parameter.optional {
                        self.output.push('?');
                    }
                    if let Some(annotation) = &parameter.annotation {
                        self.output.push_str(": ");
                        self.write_type(annotation, TYPE_PREC_LOWEST);
                    }
                }
                self.output.push(']');
                self.write_type_member_return(value_type.as_ref(), None);
            }
            TypeMemberKind::Accessor {
                name,
                accessor,
                parameters,
                return_type,
            } => {
                self.output.push_str(match accessor {
                    AccessorKind::Get => "get ",
                    AccessorKind::Set => "set ",
                });
                self.write_type_member_name(name);
                self.write_declaration_parameters(parameters);
                if matches!(accessor, AccessorKind::Get) {
                    self.write_type_member_return(return_type.as_ref(), Some("any"));
                }
            }
        }
        self.output.push(';');
    }

    fn write_type_member_name(&mut self, name: &TypeMemberName) {
        match &name.kind {
            TypeMemberNameKind::Identifier(_)
            | TypeMemberNameKind::StringLiteral(_)
            | TypeMemberNameKind::NumericLiteral(_)
            | TypeMemberNameKind::BigIntLiteral(_) => {
                self.output.push_str(self.source.slice(name.span).trim());
            }
            TypeMemberNameKind::Computed(expression) => {
                self.output.push('[');
                self.write_expression(expression, PREC_LOWEST);
                self.output.push(']');
            }
        }
    }

    fn write_type_member_return(&mut self, ty: Option<&TypeNode>, fallback: Option<&str>) {
        let Some(ty) = ty else {
            if let Some(fallback) = fallback {
                self.write_parts(&[": ", fallback]);
            }
            return;
        };
        self.output.push_str(": ");
        self.write_type(ty, TYPE_PREC_LOWEST);
    }
}

fn has_parameter_modifier(parameter: &Parameter, expected: ParameterModifier) -> bool {
    parameter
        .modifiers
        .iter()
        .any(|modifier| modifier.kind == expected)
}

fn parameter_properties(parameters: &[Parameter]) -> impl Iterator<Item = &Parameter> {
    parameters
        .iter()
        .filter(|parameter| parameter.is_property())
}

pub(super) fn optional_type_absorbs_undefined(ty: &TypeNode) -> bool {
    match &ty.kind {
        TypeNodeKind::Keyword(KeywordType::Any | KeywordType::Unknown | KeywordType::Undefined) => {
            true
        }
        TypeNodeKind::Union(members) => members.iter().any(optional_type_absorbs_undefined),
        TypeNodeKind::Parenthesized(inner) => optional_type_absorbs_undefined(inner),
        _ => false,
    }
}

pub(super) fn directive(statement: &Statement) -> Option<bool> {
    let StatementKind::Expression(Expression {
        kind: ExpressionKind::Literal(Literal::String(literal)),
        ..
    }) = &statement.kind
    else {
        return None;
    };
    match literal {
        StringLiteral::Plain(value) => Some(value == "use strict"),
        StringLiteral::Extended(value) => (value.terminated
            && !value.contains_invalid_escape
            && value.cooked.as_string().as_deref() == Some("use strict"))
        .then_some(true),
    }
}

fn is_super_call_statement(statement: &Statement) -> bool {
    let StatementKind::Expression(Expression {
        kind: ExpressionKind::Call { callee, .. },
        ..
    }) = &statement.kind
    else {
        return false;
    };
    matches!(&callee.kind, ExpressionKind::Identifier { name, .. } if name == "super")
}

#[cfg(test)]
#[path = "../../rewrite-tests/declaration_accessors_unit.rs"]
mod declaration_accessors_unit;
