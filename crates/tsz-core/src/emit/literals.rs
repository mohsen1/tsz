use crate::source::Span;
use crate::syntax::{
    Expression, ExpressionKind, Literal, StringLiteral, TypeNode, VariableDeclarator, VariableKind,
    VariableStatement, erased_expression_separated_number,
    expression_contains_no_substitution_template,
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
                .emit_text(self.preserve_numeric_separators && !self.emitting_declaration())
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
        } else if let (
            VariableKind::Const,
            Some(Expression {
                kind: ExpressionKind::Literal(literal),
                span,
                ..
            }),
        ) = (declaration_kind, &declaration.initializer)
        {
            if matches!(literal, Literal::Null) {
                self.output.push_str(": null");
            } else {
                self.output.push_str(" = ");
                let text = self.literal_text(literal, *span);
                self.output.push_str(&text);
            }
        } else if !self.write_checked_declaration_type(declaration) {
            self.reject_declaration();
        }
    }
    fn write_checked_declaration_type(&mut self, declaration: &VariableDeclarator) -> bool {
        let declaration_summary = self
            .bindings
            .declarations
            .iter()
            .find(|bound| {
                bound.kind == crate::bind::DeclarationKind::Variable
                    && bound.name_span == declaration.name_span
            })
            .and_then(|bound| {
                self.declaration_summaries()?
                    .get(&bound.id)?
                    .declaration_type
                    .as_ref()
            });
        let Some(summary) = declaration_summary else {
            return false;
        };
        self.output.push_str(": ");
        self.output.push_str(&summary.text);
        true
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
