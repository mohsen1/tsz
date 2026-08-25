use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMember, ClassMemberKind, ExportDeclaration,
    ImportDeclaration, Statement, StatementKind, SwitchClauseKind,
};

use super::{ModuleFormat, PREC_LOWEST, Printer};

impl Printer<'_> {
    pub(super) fn write_javascript_statement(&mut self, statement: &Statement, top_level: bool) {
        let emitted = self.javascript_statement_is_emitted(statement);
        self.write_comments_before_node(statement.span, emitted);
        if !emitted {
            self.write_comments_after_node(statement.span, false);
            return;
        }

        match &statement.kind {
            StatementKind::Import(declaration) => {
                self.write_javascript_import(statement, declaration);
            }
            StatementKind::Export(declaration) => {
                self.write_javascript_export(statement, declaration);
            }
            StatementKind::Variable(declaration) => {
                self.write_javascript_variable(declaration, top_level);
            }
            StatementKind::Function(declaration) => {
                self.write_javascript_function(statement, declaration, top_level);
            }
            StatementKind::Class(declaration) => {
                self.write_javascript_class(declaration, top_level);
            }
            StatementKind::Return(expression) => {
                self.write_indent();
                self.output.push_str("return");
                if let Some(expression) = expression {
                    self.output.push(' ');
                    self.write_expression(expression, PREC_LOWEST);
                }
                self.output.push_str(";\n");
            }
            StatementKind::If(control_flow) => self.write_javascript_if(control_flow),
            StatementKind::Switch(control_flow) => {
                self.write_indent();
                self.output.push_str("switch (");
                self.write_expression(&control_flow.expression, PREC_LOWEST);
                self.output.push_str(") {\n");
                self.indent += 1;
                for clause in &control_flow.clauses {
                    self.write_indent();
                    match &clause.kind {
                        SwitchClauseKind::Case(expression) => {
                            self.output.push_str("case ");
                            self.write_expression(expression, PREC_LOWEST);
                            self.output.push(':');
                        }
                        SwitchClauseKind::Default => self.output.push_str("default:"),
                    }
                    self.output.push('\n');
                    self.indent += 1;
                    for nested in &clause.statements {
                        self.write_javascript_statement(nested, false);
                    }
                    self.indent = self.indent.saturating_sub(1);
                }
                self.indent = self.indent.saturating_sub(1);
                self.write_indent();
                self.output.push_str("}\n");
            }
            StatementKind::Break(jump) => {
                self.write_jump_statement("break", jump.label.as_deref());
            }
            StatementKind::Continue(jump) => {
                self.write_jump_statement("continue", jump.label.as_deref());
            }
            StatementKind::Block(statements) => {
                self.write_indent();
                self.write_braced_statements(Some(statement.span), statements);
                self.output.push('\n');
            }
            StatementKind::Expression(expression) => {
                self.write_commented_expression_statement(statement, expression);
            }
            StatementKind::Empty => {
                self.write_indent();
                self.output.push_str(";\n");
            }
            StatementKind::TypeAlias(_) | StatementKind::Interface(_) | StatementKind::Unknown => {
                unreachable!("not-emitted statement entered the JavaScript writer")
            }
        }
        self.write_comments_after_node(statement.span, true);
    }

    fn javascript_statement_is_emitted(&self, statement: &Statement) -> bool {
        match &statement.kind {
            StatementKind::Import(declaration) => self.javascript_import_is_emitted(declaration),
            StatementKind::Export(declaration) => self.javascript_export_is_emitted(declaration),
            StatementKind::Variable(declaration) => !declaration.declared,
            StatementKind::Function(declaration) => !declaration.declared && declaration.has_body,
            StatementKind::Class(declaration) => !declaration.declared,
            StatementKind::TypeAlias(_) | StatementKind::Interface(_) | StatementKind::Unknown => {
                false
            }
            StatementKind::If(_)
            | StatementKind::Switch(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Return(_)
            | StatementKind::Block(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty => true,
        }
    }

    fn javascript_import_is_emitted(&self, declaration: &ImportDeclaration) -> bool {
        !declaration.type_only
            && (declaration.side_effect_only
                || declaration
                    .bindings
                    .iter()
                    .any(|binding| !binding.type_only))
    }

    fn javascript_export_is_emitted(&self, declaration: &ExportDeclaration) -> bool {
        let has_runtime_product = !declaration.type_only
            && (declaration.export_all
                || declaration.assignment.is_some()
                || declaration
                    .specifiers
                    .iter()
                    .any(|specifier| !specifier.type_only));
        if !has_runtime_product {
            return false;
        }
        match self.module_format {
            ModuleFormat::EsModule => {
                declaration.assignment.is_none() || declaration.default_export
            }
            ModuleFormat::CommonJs => {
                declaration.assignment.is_some()
                    || declaration.export_all && declaration.module_specifier.is_some()
                    || declaration
                        .specifiers
                        .iter()
                        .any(|specifier| !specifier.type_only)
            }
        }
    }

    fn write_javascript_if(&mut self, control_flow: &crate::syntax::IfStatement) {
        self.write_indent();
        self.output.push_str("if (");
        self.write_expression(&control_flow.condition, PREC_LOWEST);
        self.output.push(')');
        self.write_control_flow_body(&control_flow.then_statement);
        if let Some(else_statement) = &control_flow.else_statement {
            self.write_indent();
            self.output.push_str("else");
            self.write_control_flow_body(else_statement);
        }
    }

    fn write_control_flow_body(&mut self, statement: &Statement) {
        if let StatementKind::Block(statements) = &statement.kind {
            self.output.push(' ');
            self.write_braced_statements(Some(statement.span), statements);
            self.output.push('\n');
        } else {
            self.output.push('\n');
            self.indent += 1;
            self.write_javascript_statement(statement, false);
            self.indent = self.indent.saturating_sub(1);
        }
    }

    fn write_jump_statement(&mut self, keyword: &str, label: Option<&str>) {
        self.write_indent();
        self.output.push_str(keyword);
        if let Some(label) = label {
            self.output.push(' ');
            self.output.push_str(label);
        }
        self.output.push_str(";\n");
    }

    fn write_javascript_class(&mut self, declaration: &ClassDeclaration, top_level: bool) {
        self.write_indent();
        if top_level && declaration.exported && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export ");
            if declaration.default_export {
                self.output.push_str("default ");
            }
        }
        self.output.push_str("class ");
        self.output.push_str(&declaration.name);
        if let Some(base) = &declaration.extends {
            self.output.push_str(" extends ");
            self.write_heritage_type(base);
        }
        self.output.push_str(" {\n");
        self.indent += 1;
        for member in &declaration.members {
            let emitted = Self::javascript_class_member_is_emitted(member);
            self.write_comments_before_node(member.span, emitted);
            if !emitted {
                self.write_comments_after_node(member.span, false);
                continue;
            }
            self.write_javascript_class_member(member, declaration.extends.is_some());
            self.write_comments_after_node(member.span, true);
        }
        if let Some(body_span) = declaration.body_span {
            let ended_on_line = self.write_comments_before_close(body_span.end);
            if !ended_on_line && !self.output.ends_with('\n') {
                self.output.push('\n');
            }
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("}\n");
        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            self.write_commonjs_export(&declaration.name);
        }
    }

    const fn javascript_class_member_is_emitted(member: &ClassMember) -> bool {
        if member.modifiers.declared || member.modifiers.abstract_member {
            return false;
        }
        !matches!(
            &member.kind,
            ClassMemberKind::Constructor {
                has_body: false,
                ..
            } | ClassMemberKind::Method {
                has_body: false,
                ..
            }
        )
    }

    fn write_javascript_class_member(&mut self, member: &ClassMember, derived: bool) {
        if self.preserve_class_fields
            && let ClassMemberKind::Constructor { parameters, .. } = &member.kind
        {
            self.write_parameter_property_fields(parameters);
        }
        self.write_indent();
        if member.modifiers.static_member {
            self.output.push_str("static ");
        }
        match &member.kind {
            ClassMemberKind::Constructor {
                parameters,
                body,
                body_span,
                ..
            } => {
                self.output.push_str("constructor");
                self.write_runtime_parameters(parameters);
                self.output.push(' ');
                self.write_constructor_body(*body_span, body, parameters, derived);
                self.output.push('\n');
            }
            ClassMemberKind::Property { initializer, .. } => {
                self.write_property_name(&member.name, member.name_span);
                if let Some(initializer) = initializer {
                    self.output.push_str(" = ");
                    self.write_expression(initializer, super::PREC_ASSIGNMENT);
                }
                self.output.push_str(";\n");
            }
            ClassMemberKind::Method {
                parameters,
                body,
                body_span,
                accessor,
                ..
            } => {
                if member.modifiers.async_member {
                    self.output.push_str("async ");
                }
                if let Some(accessor) = accessor {
                    self.output.push_str(match accessor {
                        AccessorKind::Get => "get ",
                        AccessorKind::Set => "set ",
                    });
                }
                self.write_property_name(&member.name, member.name_span);
                self.write_runtime_parameters(parameters);
                self.output.push(' ');
                self.write_braced_statements(*body_span, body);
                self.output.push('\n');
            }
        }
    }

    pub(super) fn write_braced_statements(
        &mut self,
        body_span: Option<crate::source::Span>,
        statements: &[Statement],
    ) {
        if statements.is_empty() {
            self.output.push('{');
            let boundary = self.output.len();
            self.indent += 1;
            let ended_on_line =
                body_span.is_some_and(|span| self.write_comments_before_close(span.end));
            self.indent = self.indent.saturating_sub(1);
            if self.output.len() == boundary {
                self.output.push_str(" }");
            } else {
                if ended_on_line {
                    self.write_indent();
                } else if !self.output.chars().last().is_some_and(char::is_whitespace) {
                    self.output.push(' ');
                }
                self.output.push('}');
            }
            return;
        }
        self.output.push_str("{\n");
        self.indent += 1;
        for statement in statements {
            self.write_javascript_statement(statement, false);
        }
        if let Some(body_span) = body_span {
            let ended_on_line = self.write_comments_before_close(body_span.end);
            if !ended_on_line && !self.output.ends_with('\n') {
                self.output.push('\n');
            }
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push('}');
    }
}
