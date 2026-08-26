use crate::syntax::{
    ArrowBody, Expression, ExpressionKind, FunctionDeclaration, FunctionLikeExpression,
    FunctionLikeFunctionKind, FunctionLikeSyntax, ObjectProperty, SourceUnit, Statement,
    StatementKind, erased_assertion_expression,
};

use super::{End, Gap, ModuleFormat, PREC_ASSIGNMENT, PREC_LOWEST, Printer};

impl Printer<'_> {
    pub(super) fn write_commonjs_declaration_prologue(&mut self, unit: &SourceUnit) {
        let output_start = self.output.len();
        for statement in unit.statements.iter().rev() {
            if let StatementKind::Class(declaration) = &statement.kind
                && declaration.exported
                && !declaration.default_export
                && !declaration.declared
            {
                self.write_parts(&["exports.", &declaration.name, " = "]);
            }
        }
        if self.output.len() != output_start {
            self.output.push_str("void 0;\n");
        }
        for statement in &unit.statements {
            if let StatementKind::Function(declaration) = &statement.kind
                && declaration.exported
                && !declaration.declared
                && declaration.has_body
            {
                let export_name = if declaration.default_export {
                    "default"
                } else {
                    declaration.name.as_str()
                };
                self.write_commonjs_export(export_name, &declaration.name, None);
            }
        }
    }

    pub(super) fn write_javascript_function(
        &mut self,
        _statement: &Statement,
        declaration: &FunctionDeclaration,
        top_level: bool,
    ) {
        self.write_indent();
        if top_level && declaration.exported && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export ");
            if declaration.default_export {
                self.output.push_str("default ");
            }
        }
        if declaration.is_async {
            self.output.push_str("async ");
        }
        self.output.push_str("function ");
        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            self.output.push_str(&declaration.name);
        } else {
            self.write_authored_identifier(&declaration.name, declaration.name_span);
        }
        self.write_runtime_parameters(&declaration.parameters, true);
        self.output.push(' ');
        self.write_braced_statements(declaration.body_span, &declaration.body);
        self.output.push('\n');
    }

    pub(super) fn write_object_property(&mut self, property: &ObjectProperty) {
        self.write_property_name(&property.name, property.name_span, property.name_kind);
        if matches!(
            &property.value.kind,
            ExpressionKind::FunctionLike(function) if function.syntax.is_object_method()
        ) {
            self.write_expression(&property.value, PREC_LOWEST);
        } else if !property.shorthand {
            self.output.push_str(": ");
            self.write_expression(&property.value, PREC_LOWEST);
        } else if let (Some(_), ExpressionKind::Assignment { right, .. }) =
            (property.shorthand_equals_span, &property.value.kind)
        {
            self.output.push_str(" = ");
            self.write_expression(right, PREC_ASSIGNMENT);
        }
    }

    pub(super) fn write_function_like(
        &mut self,
        function: &FunctionLikeExpression,
        expression_span: crate::source::Span,
    ) {
        match &function.syntax {
            FunctionLikeSyntax::Arrow(body) => {
                self.write_arrow(
                    &function.parameters,
                    body,
                    function.body_span,
                    expression_span,
                );
            }
            FunctionLikeSyntax::Function { kind, name, body } => {
                if *kind == FunctionLikeFunctionKind::Expression {
                    self.output.push_str("function ");
                    if let Some(name) = name {
                        self.write_authored_identifier(&name.name, name.span);
                    }
                }
                self.write_runtime_parameters(
                    &function.parameters,
                    *kind == FunctionLikeFunctionKind::Expression,
                );
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
        let inline = body_span.is_some_and(|span| {
            self.body_span_is_single_line(span)
                && !self.comment_index.has_comment_within(span.start, span.end)
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

    pub(super) fn body_span_is_single_line(&self, span: crate::source::Span) -> bool {
        !self
            .source
            .slice(span)
            .contains(['\n', '\r', '\u{2028}', '\u{2029}'])
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
        expression_span: crate::source::Span,
    ) {
        if self.preserve_arrows {
            if let [parameter] = parameters
                && parameter.name_span.start == expression_span.start
            {
                self.write_authored_identifier(&parameter.name, parameter.name_span);
                self.write_gap(End(parameter.name_span.end), true, Gap::Indent);
            } else {
                self.write_runtime_parameters(parameters, true);
            }
            self.output.push_str(" => ");
        } else {
            self.output.push_str("function ");
            self.write_runtime_parameters(parameters, true);
            self.output.push(' ');
        }
        match body {
            ArrowBody::Expression(expression) if self.preserve_arrows => {
                self.write_arrow_expression(expression, PREC_ASSIGNMENT);
            }
            ArrowBody::Expression(expression) => {
                self.output.push_str("{\n");
                self.indent += 1;
                self.write_indent();
                self.output.push_str("return ");
                self.write_arrow_expression(expression, PREC_LOWEST);
                self.output.push_str(";\n");
                self.indent = self.indent.saturating_sub(1);
                self.write_indent();
                self.output.push('}');
            }
            ArrowBody::Block(statements) => self.write_braced_statements(body_span, statements),
        }
    }

    fn write_arrow_expression(&mut self, expression: &Expression, precedence: u8) {
        let parenthesize = starts_with_assertion_erased_object_literal(expression);
        if parenthesize {
            self.output.push('(');
        }
        self.write_expression(expression, precedence);
        if parenthesize {
            self.output.push(')');
        }
    }
}

fn starts_with_assertion_erased_object_literal(expression: &Expression) -> bool {
    starts_with_erased_expression(expression, |expression| {
        matches!(expression.kind, ExpressionKind::Object(_))
    })
}

fn starts_with_erased_function_expression(expression: &Expression) -> bool {
    starts_with_erased_expression(expression, is_function_expression)
}

fn starts_with_erased_expression(
    expression: &Expression,
    matches: fn(&Expression) -> bool,
) -> bool {
    let expression = erased_assertion_expression(expression).unwrap_or(expression);
    matches(expression)
        || match &expression.kind {
            ExpressionKind::Call { callee, .. } => starts_with_erased_expression(callee, matches),
            ExpressionKind::Member { object, .. }
            | ExpressionKind::ElementAccess { object, .. } => {
                starts_with_erased_expression(object, matches)
            }
            ExpressionKind::Binary { left, .. } => starts_with_erased_expression(left, matches),
            _ => false,
        }
}

fn is_erased_function_expression(expression: &Expression) -> bool {
    let expression = erased_assertion_expression(expression).unwrap_or(expression);
    is_function_expression(expression)
}

fn is_function_expression(expression: &Expression) -> bool {
    matches!(
        &expression.kind,
        ExpressionKind::FunctionLike(function) if function.syntax.function().is_some()
    )
}
