use crate::syntax::{
    AccessorKind, ExpressionKind, FunctionDeclaration, KeywordType, Literal, Parameter,
    ParameterModifier, Statement, StatementKind, TypeMember, TypeMemberKind, TypeMemberName,
    TypeMemberNameKind, TypeNode, TypeNodeKind,
};

use super::{PREC_LOWEST, Printer, TYPE_PREC_LOWEST};

impl Printer<'_> {
    pub(super) fn write_declaration_function(&mut self, declaration: &FunctionDeclaration) {
        self.write_indent();
        self.output.push_str(if declaration.exported {
            "export declare function "
        } else {
            "declare function "
        });
        self.output.push_str(&declaration.name);
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

    pub(super) fn write_parameter_property_fields(&mut self, parameters: &[Parameter]) {
        for parameter in parameters
            .iter()
            .filter(|parameter| is_parameter_property(parameter))
        {
            self.write_indent();
            self.write_parts(&[&parameter.name, ";\n"]);
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
        for parameter in parameters
            .iter()
            .filter(|parameter| is_parameter_property(parameter))
        {
            self.write_indent();
            if has_parameter_modifier(parameter, ParameterModifier::Private) {
                self.output.push_str("private ");
            } else if has_parameter_modifier(parameter, ParameterModifier::Protected) {
                self.output.push_str("protected ");
            }
            if has_parameter_modifier(parameter, ParameterModifier::Readonly) {
                self.output.push_str("readonly ");
            }
            self.output.push_str(&parameter.name);
            if parameter.optional {
                self.output.push('?');
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

    pub(super) fn write_constructor_body(
        &mut self,
        body_span: Option<crate::source::Span>,
        statements: &[Statement],
        parameters: &[Parameter],
        derived: bool,
    ) {
        if statements.is_empty() && !parameters.iter().any(is_parameter_property) {
            self.write_braced_statements(body_span, statements);
            return;
        }
        self.output.push_str("{\n");
        self.indent += 1;
        let directive_count = statements
            .iter()
            .take_while(|statement| is_directive_statement(statement))
            .count();
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
        for parameter in parameters
            .iter()
            .filter(|parameter| is_parameter_property(parameter))
        {
            self.write_indent();
            self.write_parts(&["this.", &parameter.name, " = ", &parameter.name, ";\n"]);
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
                if let Some(return_type) = return_type {
                    self.output.push_str(": ");
                    self.write_type(return_type, TYPE_PREC_LOWEST);
                } else {
                    self.output.push_str(": any");
                }
            }
            TypeMemberKind::Call {
                type_parameters,
                parameters,
                return_type,
            } => {
                self.write_type_parameters(type_parameters);
                self.write_declaration_parameters(parameters);
                if let Some(return_type) = return_type {
                    self.output.push_str(": ");
                    self.write_type(return_type, TYPE_PREC_LOWEST);
                } else {
                    self.output.push_str(": any");
                }
            }
            TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
            } => {
                self.output.push_str("new ");
                self.write_type_parameters(type_parameters);
                self.write_declaration_parameters(parameters);
                if let Some(return_type) = return_type {
                    self.output.push_str(": ");
                    self.write_type(return_type, TYPE_PREC_LOWEST);
                } else {
                    self.output.push_str(": any");
                }
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
                    self.output.push_str(&parameter.name);
                    if parameter.optional {
                        self.output.push('?');
                    }
                    if let Some(annotation) = &parameter.annotation {
                        self.output.push_str(": ");
                        self.write_type(annotation, TYPE_PREC_LOWEST);
                    }
                }
                self.output.push(']');
                if let Some(value_type) = value_type {
                    self.output.push_str(": ");
                    self.write_type(value_type, TYPE_PREC_LOWEST);
                }
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
                if let Some(return_type) = return_type {
                    self.output.push_str(": ");
                    self.write_type(return_type, TYPE_PREC_LOWEST);
                } else if matches!(accessor, AccessorKind::Get) {
                    self.output.push_str(": any");
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
}

fn has_parameter_modifier(parameter: &Parameter, expected: ParameterModifier) -> bool {
    parameter
        .modifiers
        .iter()
        .any(|modifier| modifier.kind == expected)
}

pub(super) fn is_parameter_property(parameter: &Parameter) -> bool {
    parameter.modifiers.iter().any(|modifier| {
        matches!(
            modifier.kind,
            ParameterModifier::Public
                | ParameterModifier::Protected
                | ParameterModifier::Private
                | ParameterModifier::Readonly
                | ParameterModifier::Override
        )
    })
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

const fn is_directive_statement(statement: &Statement) -> bool {
    matches!(
        &statement.kind,
        StatementKind::Expression(crate::syntax::Expression {
            kind: ExpressionKind::Literal(Literal::String(_)),
            ..
        })
    )
}

fn is_super_call_statement(statement: &Statement) -> bool {
    let StatementKind::Expression(expression) = &statement.kind else {
        return false;
    };
    let ExpressionKind::Call { callee, .. } = &expression.kind else {
        return false;
    };
    matches!(
        &callee.kind,
        ExpressionKind::Identifier { name, .. } if name == "super"
    )
}
