use crate::source::Span;
use crate::syntax::{
    Expression, ExpressionKind, Literal, StringLiteral, TypeNode, VariableDeclarator, VariableKind,
    VariableStatement, erased_expression_separated_number,
    expression_contains_no_substitution_template,
};

use super::display::RenderedType;
use super::{Printer, TYPE_PREC_LOWEST, is_quoted, quote_string, variable_kind_text};

pub(crate) fn render_inferred_expression_type(
    expression: &Expression,
    preserve_literal: bool,
) -> Option<RenderedType> {
    let (text, part_kind) = match &expression.kind {
        ExpressionKind::Literal(literal) => inferred_literal(literal, preserve_literal)?,
        ExpressionKind::Object(properties) => {
            let mut rendered = Vec::with_capacity(properties.len());
            for property in properties {
                let ty = render_inferred_expression_type(&property.value, false)?;
                let name = if property
                    .name
                    .chars()
                    .next()
                    .is_some_and(is_identifier_start)
                    && property.name.chars().skip(1).all(is_identifier_continue)
                    || !property.name.is_empty()
                        && property.name.bytes().all(|byte| byte.is_ascii_digit())
                {
                    property.name.clone()
                } else {
                    quote_string(&property.name)
                };
                rendered.push(format!("{name}: {};", ty.text));
            }
            (
                if rendered.is_empty() {
                    "{}".to_string()
                } else {
                    format!("{{ {} }}", rendered.join(" "))
                },
                "text",
            )
        }
        ExpressionKind::Parenthesized(inner) => {
            return render_inferred_expression_type(inner, preserve_literal);
        }
        _ => return None,
    };
    Some(RenderedType { text, part_kind })
}

fn inferred_literal(literal: &Literal, preserve: bool) -> Option<(String, &'static str)> {
    let primitive = match literal {
        Literal::String(_) | Literal::NoSubstitutionTemplate(_) => "string",
        Literal::Number(_) => "number",
        Literal::BigInt(_) => "bigint",
        Literal::Boolean(_) => "boolean",
        Literal::Null => "null",
    };
    if !preserve {
        return Some((primitive.to_string(), "keyword"));
    }
    match literal {
        Literal::String(crate::syntax::StringLiteral::Plain(value)) => {
            Some((quote_string(value), "stringLiteral"))
        }
        Literal::String(crate::syntax::StringLiteral::Extended(literal)) => literal
            .terminated
            .then(|| literal.cooked.as_string())
            .flatten()
            .filter(|_| !literal.contains_invalid_escape)
            .map(|value| (quote_string(&value), "stringLiteral")),
        Literal::NoSubstitutionTemplate(literal) => {
            Some((quote_string(&literal.cooked), "stringLiteral"))
        }
        Literal::Number(crate::syntax::NumberLiteral::Plain(value)) | Literal::BigInt(value) => {
            Some((value.clone(), "stringLiteral"))
        }
        Literal::Number(crate::syntax::NumberLiteral::Separated(value)) => {
            Some((value.canonical().to_string(), "stringLiteral"))
        }
        Literal::Boolean(value) => Some((value.to_string(), "text")),
        Literal::Null => Some(("null".to_string(), "keyword")),
        Literal::Number(crate::syntax::NumberLiteral::Recovery(_)) => None,
    }
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character == '$' || character.is_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_alphanumeric()
}

impl Printer<'_> {
    pub(super) fn literal_text(&self, literal: &Literal, span: Span) -> String {
        match literal {
            Literal::String(StringLiteral::Plain(value)) => {
                let raw = self.source.slice(span).trim();
                if is_quoted(raw) {
                    raw.to_string()
                } else {
                    quote_string(value)
                }
            }
            Literal::String(StringLiteral::Extended(literal)) => literal.raw.clone(),
            Literal::NoSubstitutionTemplate(literal) => literal.raw.clone(),
            Literal::Number(value) => value
                .emit_text(self.preserve_numeric_separators && !self.emitting_declaration)
                .to_string(),
            Literal::BigInt(value) => value.clone(),
            Literal::Boolean(value) => value.to_string(),
            Literal::Null => "null".to_string(),
        }
    }

    pub(super) fn write_member_object(&mut self, object: &Expression) {
        self.write_expression(object, super::PREC_POSTFIX);
        let Some(number) = erased_expression_separated_number(object) else {
            return;
        };
        if number.needs_property_access_extra_dot(self.preserve_numeric_separators)
            && self.source.text.as_bytes().get(object.span.end as usize) == Some(&b'.')
        {
            self.output.push('.');
        }
    }

    pub(super) fn write_declaration_variable(&mut self, declaration: &VariableStatement) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
        }
        self.output.push_str("declare ");
        self.output
            .push_str(variable_kind_text(declaration.declaration_kind));
        self.output.push(' ');
        for (index, declarator) in declaration.declarators.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            self.write_declaration_variable_declarator(declarator, declaration.declaration_kind);
        }
        self.output.push_str(";\n");
    }

    fn write_declaration_variable_declarator(
        &mut self,
        declaration: &VariableDeclarator,
        declaration_kind: VariableKind,
    ) {
        self.write_authored_identifier(&declaration.name, declaration.name_span);
        if let Some(annotation) = &declaration.annotation {
            self.output.push_str(": ");
            self.write_type(annotation, TYPE_PREC_LOWEST);
        } else if let Some(asserted) = declaration.initializer.as_ref().and_then(assertion_type) {
            self.output.push_str(": ");
            self.write_type(asserted, TYPE_PREC_LOWEST);
        } else if declaration
            .initializer
            .as_ref()
            .is_some_and(expression_contains_template)
        {
            self.declaration_supported = false;
            self.output.push_str(": unknown");
        } else if declaration_kind == VariableKind::Const {
            if let Some(Expression {
                kind: ExpressionKind::Literal(literal),
                span,
                ..
            }) = &declaration.initializer
            {
                if matches!(literal, Literal::Null) {
                    self.output.push_str(": null");
                } else {
                    self.output.push_str(" = ");
                    let text = self.literal_text(literal, *span);
                    self.output.push_str(&text);
                }
            } else {
                self.output.push_str(": unknown");
            }
        } else {
            self.output.push_str(": unknown");
        }
    }
}

pub(super) fn expression_contains_template(expression: &Expression) -> bool {
    expression_contains_no_substitution_template(expression)
}

fn assertion_type(expression: &Expression) -> Option<&TypeNode> {
    match &expression.kind {
        ExpressionKind::As { ty, .. } => Some(ty),
        ExpressionKind::Parenthesized(inner) => assertion_type(inner),
        _ => None,
    }
}
