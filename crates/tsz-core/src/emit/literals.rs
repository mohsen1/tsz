use crate::source::Span;
use crate::syntax::{
    Expression, ExpressionKind, Literal, StringLiteral, VariableDeclaration, VariableKind,
    erased_expression_separated_number, expression_contains_no_substitution_template,
};

use super::{Printer, TYPE_PREC_LOWEST, is_quoted, quote_string, variable_kind_text};

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

    pub(super) fn write_declaration_variable(&mut self, declaration: &VariableDeclaration) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
        }
        self.output.push_str("declare ");
        self.output
            .push_str(variable_kind_text(declaration.declaration_kind));
        self.output.push(' ');
        self.output.push_str(&declaration.name);
        if let Some(annotation) = &declaration.annotation {
            self.output.push_str(": ");
            self.write_type(annotation, TYPE_PREC_LOWEST);
        } else if declaration
            .initializer
            .as_ref()
            .is_some_and(expression_contains_template)
        {
            self.declaration_supported = false;
            self.output.push_str(": unknown");
        } else if declaration.declaration_kind == VariableKind::Const {
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
        self.output.push_str(";\n");
    }
}

pub(super) fn expression_contains_template(expression: &Expression) -> bool {
    expression_contains_no_substitution_template(expression)
}
