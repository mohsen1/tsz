use crate::bind::DeclarationKind;
use crate::source::Span;
use crate::syntax::{
    Expression, ExpressionKind, SourceUnit, StatementKind, VariableDeclarator, VariableStatement,
};

use super::{ModuleFormat, PREC_ASSIGNMENT, Printer};

impl Printer<'_> {
    pub(super) fn write_commonjs_declaration_prologue(&mut self, unit: &SourceUnit) {
        let output_start = self.output.len();
        for statement in unit.statements.iter().rev() {
            let emitted = self.javascript_statement_is_emitted(statement);
            match &statement.kind {
                StatementKind::Variable(declaration) if emitted && declaration.exported => {
                    for declarator in declaration.declarators.iter().rev() {
                        self.write_parts(&["exports.", &declarator.name, " = "]);
                    }
                }
                StatementKind::Class(declaration)
                    if emitted && declaration.exported && !declaration.default_export =>
                {
                    self.write_parts(&["exports.", &declaration.name, " = "]);
                }
                _ => {}
            }
        }
        if self.output.len() != output_start {
            self.output.push_str("void 0;\n");
        }
        for statement in &unit.statements {
            if let StatementKind::Function(declaration) = &statement.kind
                && self.javascript_statement_is_emitted(statement)
                && declaration.exported
            {
                let export_name = if declaration.default_export {
                    "default"
                } else {
                    declaration.name.as_str()
                };
                let runtime_name = self.declaration_runtime_name(&declaration.name);
                self.write_commonjs_export(export_name, &runtime_name);
            }
        }
    }
    pub(super) fn declaration_runtime_name(&self, authored: &str) -> String {
        if !authored.is_empty() {
            return authored.to_string();
        }
        let names = self.bindings.scopes.first().map(|scope| &scope.names);
        (1..)
            .map(|index| format!("default_{index}"))
            .find(|candidate| names.is_none_or(|names| !names.contains_key(candidate)))
            .expect("a finite declaration scope has a free generated default name")
    }
    pub(super) fn write_commonjs_exported_variable(&mut self, declaration: &VariableStatement) {
        let mut local = declaration.declarators.iter().filter(|declarator| {
            declarator.initializer.as_ref().is_some_and(|initializer| {
                matches!(initializer.kind, ExpressionKind::FunctionLike(_))
            })
        });
        if let Some(first) = local.next() {
            self.write_indent();
            self.output
                .push_str(self.runtime_variable_kind(declaration.declaration_kind));
            self.output.push(' ');
            self.write_runtime_variable_declarator(first);
            for declarator in local {
                self.output.push_str(", ");
                self.write_runtime_variable_declarator(declarator);
            }
            self.output.push_str(";\n");
        }

        let mut initializers = declaration
            .declarators
            .iter()
            .filter_map(|declarator| Some((declarator, declarator.initializer.as_ref()?)));
        if let Some((first, initializer)) = initializers.next() {
            self.write_indent();
            self.write_commonjs_variable_assignment(first, initializer);
            for (declarator, initializer) in initializers {
                self.output.push_str(", ");
                self.write_commonjs_variable_assignment(declarator, initializer);
            }
            self.output.push_str(";\n");
        }
    }
    fn write_commonjs_variable_assignment(
        &mut self,
        declarator: &VariableDeclarator,
        initializer: &Expression,
    ) {
        self.output.push_str("exports.");
        self.write_authored_identifier(&declarator.name, declarator.name_span);
        self.output.push_str(" = ");
        if matches!(initializer.kind, ExpressionKind::FunctionLike(_)) {
            self.write_authored_identifier(&declarator.name, declarator.name_span);
        } else {
            self.write_expression(initializer, PREC_ASSIGNMENT);
        }
    }
    pub(super) fn collect_commonjs_exported_variables(&mut self, unit: &SourceUnit) {
        if self.module_format != ModuleFormat::CommonJs {
            return;
        }
        for statement in &unit.statements {
            if let StatementKind::Variable(declaration) = &statement.kind
                && declaration.exported
                && self.javascript_statement_is_emitted(statement)
            {
                for declarator in &declaration.declarators {
                    if let Some(bound) = self.bindings.declarations.iter().find(|bound| {
                        bound.kind == DeclarationKind::Variable
                            && bound.name_span == declarator.name_span
                    }) {
                        self.commonjs_exported_variables.insert(bound.id);
                    }
                }
            }
        }
    }
    pub(super) fn expression_is_commonjs_exported_variable(&self, span: Span) -> bool {
        self.bindings
            .reference_declaration(span)
            .is_some_and(|declaration| self.commonjs_exported_variables.contains(&declaration))
    }
    pub(super) fn write_commonjs_export(&mut self, export_name: &str, local_name: &str) {
        self.write_indent();
        self.write_parts(&["exports.", export_name, " = ", local_name, ";\n"]);
    }
}
