use crate::bind::ScopeId;
use crate::source::{FileId, Span};
use crate::syntax::{Statement, StatementKind, SwitchClauseKind};

use super::Checker;
use super::projection_model::PropertyOrderTree;
use super::relation_diagnostic::RelationDiagnosticStyle;
use crate::semantics::relation::RelationMode;
use crate::semantics::types::TypeId;

impl Checker<'_> {
    pub(super) fn check_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        expected_return: Option<TypeId>,
        expected_return_order: Option<&PropertyOrderTree>,
    ) {
        match &statement.kind {
            StatementKind::Export(declaration) => {
                if let Some(expression) = &declaration.assignment {
                    self.infer_expression(file, scope, expression, None);
                }
            }
            StatementKind::Class(declaration) => {
                let class_scope = self.node_scope(file, statement.id, scope);
                self.check_class(file, class_scope, declaration);
            }
            StatementKind::Variable(declaration) => {
                self.check_variable(file, scope, statement.id, declaration);
            }
            StatementKind::Function(declaration) => {
                self.check_function(file, statement.id, declaration);
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    let actual = self.infer_expression(file, scope, expression, expected_return);
                    if let Some(expected) = expected_return {
                        let return_span = Span {
                            file,
                            start: statement.span.start,
                            end: statement.span.start.saturating_add(6),
                        };
                        self.report_relation(
                            actual,
                            expected,
                            return_span,
                            Some(expression),
                            expected_return_order.cloned(),
                            RelationMode::Assignment,
                            RelationDiagnosticStyle::Type,
                        );
                    }
                }
            }
            StatementKind::Block(statements) => {
                for nested in statements {
                    let nested_scope = self.program.files[file.0 as usize]
                        .bindings
                        .scope_for_node
                        .get(&nested.id)
                        .copied()
                        .unwrap_or(scope);
                    self.check_statement(
                        file,
                        nested_scope,
                        nested,
                        expected_return,
                        expected_return_order,
                    );
                }
            }
            StatementKind::If(control_flow) => {
                self.infer_expression(file, scope, &control_flow.condition, None);
                let then_scope = self.program.files[file.0 as usize]
                    .bindings
                    .scope_for_node
                    .get(&control_flow.then_statement.id)
                    .copied()
                    .unwrap_or(scope);
                self.check_statement(
                    file,
                    then_scope,
                    &control_flow.then_statement,
                    expected_return,
                    expected_return_order,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = self.program.files[file.0 as usize]
                        .bindings
                        .scope_for_node
                        .get(&else_statement.id)
                        .copied()
                        .unwrap_or(scope);
                    self.check_statement(
                        file,
                        else_scope,
                        else_statement,
                        expected_return,
                        expected_return_order,
                    );
                }
            }
            StatementKind::Switch(control_flow) => {
                let switch_scope = self.program.files[file.0 as usize]
                    .bindings
                    .scope_for_node
                    .get(&statement.id)
                    .copied()
                    .unwrap_or(scope);
                self.infer_expression(file, switch_scope, &control_flow.expression, None);
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.infer_expression(file, switch_scope, expression, None);
                    }
                    for nested in &clause.statements {
                        let nested_scope = self.program.files[file.0 as usize]
                            .bindings
                            .scope_for_node
                            .get(&nested.id)
                            .copied()
                            .unwrap_or(switch_scope);
                        self.check_statement(
                            file,
                            nested_scope,
                            nested,
                            expected_return,
                            expected_return_order,
                        );
                    }
                }
            }
            StatementKind::Expression(expression) => {
                self.infer_expression(file, scope, expression, None);
            }
            StatementKind::Import(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
    }
}
