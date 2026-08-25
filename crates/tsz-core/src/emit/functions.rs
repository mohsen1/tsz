use crate::syntax::{
    ArrowBody, Expression, ExpressionKind, FunctionLikeExpression, FunctionLikeSyntax, Statement,
    StatementKind, erased_assertion_expression,
};

use super::{PREC_ASSIGNMENT, PREC_LOWEST, Printer};

impl Printer<'_> {
    pub(super) fn write_function_like(&mut self, function: &FunctionLikeExpression) {
        match &function.syntax {
            FunctionLikeSyntax::Arrow(body) => {
                self.write_arrow(&function.parameters, body, function.body_span);
            }
            FunctionLikeSyntax::Function { name, body } => {
                self.output.push_str("function ");
                if let Some(name) = name {
                    self.output.push_str(&name.name);
                }
                self.write_runtime_parameters(&function.parameters);
                self.output.push(' ');
                self.write_inline_function_body(function.body_span, body);
            }
        }
    }

    pub(super) fn write_inline_function_body(
        &mut self,
        body_span: Option<crate::source::Span>,
        body: &[Statement],
    ) {
        let inline = body_span.is_some_and(|body_span| {
            self.source
                .slice(body_span)
                .bytes()
                .all(|byte| !matches!(byte, b'\n' | b'\r'))
                && !self
                    .comment_index
                    .has_comment_within(body_span.start, body_span.end)
        }) && body.iter().all(|statement| {
            matches!(
                statement.kind,
                StatementKind::Variable(_)
                    | StatementKind::Return(_)
                    | StatementKind::Expression(_)
                    | StatementKind::Empty
            )
        });
        if !inline || body.is_empty() {
            self.write_braced_statements(body_span, body);
            return;
        }
        self.output.push_str("{ ");
        for (index, statement) in body.iter().enumerate() {
            if index != 0 {
                self.output.push(' ');
            }
            match &statement.kind {
                StatementKind::Variable(declaration) => {
                    self.write_runtime_variable(declaration);
                }
                StatementKind::Return(value) => {
                    self.output.push_str("return");
                    if let Some(value) = value {
                        self.output.push(' ');
                        self.write_expression(value, PREC_LOWEST);
                    }
                    self.output.push(';');
                }
                StatementKind::Expression(value) => {
                    self.write_expression_statement_expression(value);
                    self.output.push(';');
                }
                StatementKind::Empty => self.output.push(';'),
                _ => unreachable!(),
            }
        }
        self.output.push_str(" }");
    }

    pub(super) fn write_expression_statement_expression(&mut self, expression: &Expression) {
        let erased = erased_assertion_expression(expression).unwrap_or(expression);
        if let ExpressionKind::Call {
            callee, arguments, ..
        } = &erased.kind
            && is_erased_function_expression(callee)
        {
            self.output.push('(');
            self.write_expression(callee, PREC_LOWEST);
            self.output.push_str(")(");
            self.write_expression_list(arguments);
            self.output.push(')');
            return;
        }
        let parenthesize = starts_with_erased_function_expression(expression);
        if parenthesize {
            self.output.push('(');
        }
        self.write_expression(expression, PREC_LOWEST);
        if parenthesize {
            self.output.push(')');
        }
    }

    fn write_arrow(
        &mut self,
        parameters: &[crate::syntax::Parameter],
        body: &ArrowBody,
        body_span: Option<crate::source::Span>,
    ) {
        let preserve = self.preserve_arrows;
        if preserve {
            self.write_runtime_parameters(parameters);
            self.output.push_str(" => ");
        } else {
            self.output.push_str("function ");
            self.write_runtime_parameters(parameters);
            self.output.push(' ');
        }
        match body {
            ArrowBody::Expression(expression) if preserve => {
                self.write_expression(expression, PREC_ASSIGNMENT);
            }
            ArrowBody::Expression(expression) => {
                self.output.push_str("{\n");
                self.indent += 1;
                self.write_indent();
                self.output.push_str("return ");
                self.write_expression(expression, PREC_LOWEST);
                self.output.push_str(";\n");
                self.indent = self.indent.saturating_sub(1);
                self.write_indent();
                self.output.push('}');
            }
            ArrowBody::Block(statements) => self.write_braced_statements(body_span, statements),
        }
    }
}

fn starts_with_erased_function_expression(expression: &Expression) -> bool {
    let expression = erased_assertion_expression(expression).unwrap_or(expression);
    if is_erased_function_expression(expression) {
        return true;
    }
    match &expression.kind {
        ExpressionKind::Call { callee, .. } => starts_with_erased_function_expression(callee),
        ExpressionKind::Member { object, .. } | ExpressionKind::ElementAccess { object, .. } => {
            starts_with_erased_function_expression(object)
        }
        ExpressionKind::Binary { left, .. } => starts_with_erased_function_expression(left),
        _ => false,
    }
}

fn is_erased_function_expression(expression: &Expression) -> bool {
    matches!(
        &erased_assertion_expression(expression)
            .unwrap_or(expression)
            .kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
    )
}
