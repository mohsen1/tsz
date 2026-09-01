use crate::program::DefaultExportDeclaration;
use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMember, ClassMemberKind, ExportDeclaration,
    ExpressionKind, ImportDeclaration, Statement, StatementKind, SwitchClauseKind, TokenKind,
};

use super::{End, Gap, Kind, ModuleFormat, PREC_LOWEST, Printer};

impl Printer<'_> {
    pub(super) fn write_declaration_statement(&mut self, statement: &Statement) {
        use StatementKind::*;
        match &statement.kind {
            Import(_) => self.write_raw_statement(statement),
            Export(declaration) => self.write_declaration_export(statement, declaration),
            Variable(declaration) => self.write_declaration_variable(declaration),
            Function(declaration) => self.write_declaration_function(declaration),
            Class(declaration) => self.write_declaration_class(declaration),
            TypeAlias(declaration) => self.write_declaration_type_alias(declaration),
            Interface(declaration) => self.write_declaration_interface(declaration),
            Return(_) | If(_) | Switch(_) | Break(_) | Continue(_) | Block(_) | Expression(_)
            | Empty | Unknown => {}
        }
    }
    fn write_declaration_export(&mut self, statement: &Statement, declaration: &ExportDeclaration) {
        let Some(expression) = declaration.assignment.as_ref() else {
            self.write_esmodule_export(declaration);
            return;
        };
        if !declaration.default_export {
            self.reject_declaration();
            return;
        }
        if matches!(expression.kind, ExpressionKind::Identifier { .. }) {
            self.write_raw_statement(statement);
            return;
        }
        let Some(summary) = self
            .declaration_summaries()
            .and_then(|summaries| summaries.default_export(statement.span.file, statement.id))
        else {
            self.reject_declaration();
            return;
        };
        let (literal, ty, preferred_name) = match summary {
            DefaultExportDeclaration::Literal => (true, None, None),
            DefaultExportDeclaration::Typed {
                ty, preferred_name, ..
            } => (false, Some(ty.text.clone()), preferred_name.as_deref()),
        };
        let name = self.default_export_name(preferred_name);
        self.write_indent();
        self.output.push_str("declare const ");
        self.output.push_str(&name);
        if literal {
            self.output.push_str(" = ");
            self.write_expression(expression.peel_parentheses(), super::PREC_LOWEST);
        } else if let Some(ty) = ty {
            self.output.push_str(": ");
            self.output.push_str(&ty);
        }
        self.output.push_str(";\n");
        self.write_indent();
        self.output.push_str("export default ");
        self.output.push_str(&name);
        self.output.push_str(";\n");
    }

    fn default_export_name(&self, preferred: Option<&str>) -> String {
        let base = preferred.unwrap_or("_default");
        if !self
            .bindings
            .scopes
            .first()
            .is_some_and(|scope| scope.names.contains_key(base))
        {
            return base.to_string();
        }
        (1..)
            .map(|index| format!("{base}_{index}"))
            .find(|candidate| {
                !self
                    .bindings
                    .scopes
                    .first()
                    .is_some_and(|scope| scope.names.contains_key(candidate))
            })
            .expect("a finite declaration scope has a free generated name")
    }
    pub(super) fn write_javascript_statement(&mut self, statement: &Statement, top_level: bool) {
        let emitted = self.javascript_statement_is_emitted(statement);
        self.write_comments_before_node(statement.span, emitted);
        if !emitted {
            self.write_comments_after_node(statement.span, false);
            return;
        }

        match &statement.kind {
            StatementKind::Import(declaration) => {
                self.write_javascript_import(statement, declaration)
            }
            StatementKind::Export(declaration) => {
                self.write_javascript_export(statement, declaration)
            }
            StatementKind::Variable(declaration) => {
                self.write_javascript_variable(declaration, top_level)
            }
            StatementKind::Function(declaration) => {
                self.write_javascript_function(statement, declaration, top_level)
            }
            StatementKind::Class(declaration) => {
                self.write_javascript_class(declaration, top_level)
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
                self.write_jump_statement("break", jump.label.as_deref(), jump.label_span)
            }
            StatementKind::Continue(jump) => {
                self.write_jump_statement("continue", jump.label.as_deref(), jump.label_span)
            }
            StatementKind::Block(statements) => {
                self.write_indent();
                self.write_braced_statements(Some(statement.span), statements);
                self.output.push('\n');
            }
            StatementKind::Expression(expression) => {
                self.write_commented_expression_statement(statement, expression)
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
    pub(super) fn javascript_statement_is_emitted(&self, statement: &Statement) -> bool {
        use StatementKind::*;
        match &statement.kind {
            Import(declaration) => self.javascript_import_is_emitted(declaration),
            Export(declaration) => self.javascript_export_is_emitted(declaration),
            Variable(declaration) => !declaration.declared,
            Function(declaration) => !declaration.declared && declaration.has_body,
            Class(declaration) => !declaration.declared,
            TypeAlias(_) | Interface(_) | Unknown => false,
            If(_) | Switch(_) | Break(_) | Continue(_) | Return(_) | Block(_) | Expression(_)
            | Empty => true,
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
        if !export_has_runtime_product(declaration) {
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
        let mut control_flow = control_flow;
        self.write_indent();
        loop {
            self.output.push_str("if");
            self.indent += 1;
            let condition_start = control_flow.condition.span.start;
            self.write_gap(Kind(TokenKind::If, condition_start), true, Gap::Space);
            self.output.push('(');
            self.write_gap(
                Kind(TokenKind::LeftParen, condition_start),
                true,
                Gap::Indent,
            );
            self.write_expression(&control_flow.condition, PREC_LOWEST);
            self.write_gap(End(control_flow.condition.span.end), true, Gap::Indent);
            self.output.push(')');
            self.write_body_gap(TokenKind::RightParen, &control_flow.then_statement);
            self.indent = self.indent.saturating_sub(1);
            self.write_control_flow_body(&control_flow.then_statement);
            let Some(else_statement) = &control_flow.else_statement else {
                break;
            };
            let then_end = control_flow.then_statement.span.end;
            self.write_gap(End(then_end), true, Gap::Newline);
            self.write_indent();
            self.output.push_str("else");
            if let StatementKind::If(nested) = &else_statement.kind {
                self.write_gap(
                    Kind(TokenKind::Else, else_statement.span.start),
                    true,
                    Gap::Space,
                );
                control_flow = nested;
            } else {
                self.write_body_gap(TokenKind::Else, else_statement);
                self.write_control_flow_body(else_statement);
                break;
            }
        }
        self.write_newline();
    }
    fn write_body_gap(&mut self, kind: TokenKind, statement: &Statement) {
        let separator = if matches!(&statement.kind, StatementKind::Block(_)) {
            Gap::Space
        } else {
            Gap::Newline
        };
        self.write_gap(Kind(kind, statement.span.start), true, separator);
    }
    fn write_control_flow_body(&mut self, statement: &Statement) {
        if let StatementKind::Block(statements) = &statement.kind {
            self.write_braced_statements(Some(statement.span), statements);
        } else {
            self.indent += 1;
            self.write_javascript_statement(statement, false);
            self.indent = self.indent.saturating_sub(1);
        }
    }
    fn write_jump_statement(
        &mut self,
        keyword: &str,
        label: Option<&str>,
        label_span: Option<crate::source::Span>,
    ) {
        self.write_indent();
        self.output.push_str(keyword);
        if let Some(label) = label {
            self.output.push(' ');
            self.write_authored_identifier(
                label,
                label_span.expect("an authored jump label must retain its span"),
            );
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
        self.output.push_str("class");
        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            self.output.push(' ');
            self.output
                .push_str(&self.declaration_runtime_name(&declaration.name));
        } else if !declaration.name.is_empty() {
            self.output.push(' ');
            self.write_authored_identifier(&declaration.name, declaration.name_span);
        }
        if let Some(base) = &declaration.extends {
            self.output.push_str(" extends ");
            self.write_heritage_type(base);
        }
        self.output.push_str(" {\n");
        self.indent += 1;
        let mut empty_index = 0;
        for member in &declaration.members {
            while declaration
                .empty_elements
                .get(empty_index)
                .is_some_and(|span| span.start < member.span.start)
            {
                self.write_javascript_empty_class_element(declaration.empty_elements[empty_index]);
                empty_index += 1;
            }
            let emitted = Self::javascript_class_member_is_emitted(member);
            self.write_comments_before_node(member.span, emitted);
            if !emitted {
                self.write_comments_after_node(member.span, false);
                continue;
            }
            self.write_javascript_class_member(member, declaration.extends.is_some());
            self.write_comments_after_node(member.span, true);
        }
        for span in &declaration.empty_elements[empty_index..] {
            self.write_javascript_empty_class_element(*span);
        }
        if let Some(body_span) = declaration.body_span {
            self.write_comments_before_close(body_span.end);
            self.write_newline();
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("}\n");
        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            let export_name = if declaration.default_export {
                "default"
            } else {
                &declaration.name
            };
            let runtime_name = self.declaration_runtime_name(&declaration.name);
            self.write_commonjs_export(export_name, &runtime_name);
        }
    }

    fn write_javascript_empty_class_element(&mut self, span: crate::source::Span) {
        self.write_comments_before_node(span, true);
        self.write_indent();
        self.output.push_str(";\n");
        self.write_comments_after_node(span, true);
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
                self.write_runtime_parameters(parameters, true);
                self.output.push(' ');
                if parameters.iter().any(|parameter| parameter.is_property()) {
                    self.write_constructor_body(*body_span, body, parameters, derived);
                } else {
                    self.write_function_body(*body_span, body);
                }
                self.output.push('\n');
            }
            ClassMemberKind::Property { initializer, .. } => {
                self.write_property_name(&member.name, member.name_span, member.name_kind);
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
                self.write_property_name(&member.name, member.name_span, member.name_kind);
                self.write_runtime_parameters(parameters, true);
                self.output.push(' ');
                self.write_function_body(*body_span, body);
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
                if body_span.is_some_and(|span| !self.body_span_is_single_line(span)) {
                    self.output.push('\n');
                    self.write_indent();
                } else {
                    self.output.push(' ');
                }
            } else if ended_on_line {
                self.write_indent();
            } else if !self.output.chars().last().is_some_and(char::is_whitespace) {
                self.output.push(' ');
            }
            self.output.push('}');
            return;
        }
        self.output.push_str("{\n");
        self.indent += 1;
        for statement in statements {
            self.write_javascript_statement(statement, false);
        }
        if let Some(body_span) = body_span {
            self.write_comments_before_close(body_span.end);
            self.write_newline();
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push('}');
    }
}

pub(super) const fn declaration_statement_is_emitted(statement: &Statement) -> bool {
    use StatementKind::*;
    match &statement.kind {
        Import(_) | Variable(_) | Function(_) | Class(_) | TypeAlias(_) | Interface(_) => true,
        Export(declaration) => declaration.assignment.is_none() || declaration.default_export,
        _ => false,
    }
}

pub(super) fn export_has_runtime_product(declaration: &ExportDeclaration) -> bool {
    let has_runtime_specifier = declaration
        .specifiers
        .iter()
        .any(|specifier| !specifier.type_only);
    !declaration.type_only
        && (declaration.export_all || declaration.assignment.is_some() || has_runtime_specifier)
}

pub(super) fn module_export_facts(statements: &[Statement]) -> (bool, bool) {
    use StatementKind::*;

    statements.iter().fold((false, false), |facts, statement| {
        let next = match &statement.kind {
            Import(_) => (true, false),
            Export(declaration) => (true, export_has_runtime_product(declaration)),
            Variable(declaration) => (
                declaration.exported,
                declaration.exported && !declaration.declared,
            ),
            Function(declaration) => (
                declaration.exported,
                declaration.exported && !declaration.declared && declaration.has_body,
            ),
            Class(declaration) => (
                declaration.exported,
                declaration.exported && !declaration.declared,
            ),
            TypeAlias(declaration) => (declaration.exported, false),
            Interface(declaration) => (declaration.exported, false),
            If(_) | Switch(_) | Break(_) | Continue(_) | Return(_) | Block(_) | Expression(_)
            | Empty | Unknown => (false, false),
        };
        (facts.0 || next.0, facts.1 || next.1)
    })
}
