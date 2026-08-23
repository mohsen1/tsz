use std::collections::BTreeSet;

use crate::source::{NodeId, Span};
use crate::syntax::{
    ArrowBody, ClassMember, ClassMemberKind, Expression, ExpressionKind, Parameter,
    ParserRecoveryFact, Statement, StatementKind, SwitchClauseKind,
};

/// Computes immutable statement ownership for flow-sensitive type-of-reference
/// work that the checker does not own yet. A region begins at an unsupported
/// narrowing entry and covers the executable suffix of the same container.
/// Function-like bodies are inventoried as fresh containers.
pub(super) fn flow_region_nodes(
    statements: &[Statement],
    recoveries: &[ParserRecoveryFact],
) -> BTreeSet<NodeId> {
    let mut collector = FlowRegionCollector {
        recoveries,
        nodes: BTreeSet::new(),
    };
    collector.visit_statement_list(statements, false);
    collector.nodes
}

struct FlowRegionCollector<'a> {
    recoveries: &'a [ParserRecoveryFact],
    nodes: BTreeSet<NodeId>,
}

impl FlowRegionCollector<'_> {
    /// Returns whether the containing executable suffix is flow-dependent
    /// after this list. Callers use that result to close nested joins over the
    /// remainder of their own container.
    fn visit_statement_list(&mut self, statements: &[Statement], mut active: bool) -> bool {
        for statement in statements {
            active = self.visit_statement(statement, active);
        }
        active
    }

    fn visit_statement(&mut self, statement: &Statement, active: bool) -> bool {
        let entry = self.statement_starts_flow_region(statement);
        let local_active = active || entry;
        if local_active && statement_is_executable_region_member(statement) {
            self.nodes.insert(statement.id);
        }

        match &statement.kind {
            StatementKind::Import(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => local_active,
            StatementKind::Export(declaration) => {
                if let Some(expression) = &declaration.assignment {
                    self.visit_expression(expression);
                }
                local_active
            }
            StatementKind::Variable(declaration) => {
                if let Some(initializer) = &declaration.initializer {
                    self.visit_expression(initializer);
                }
                local_active
            }
            StatementKind::Function(declaration) => {
                self.visit_parameter_initializers(&declaration.parameters);
                let recovered = self.signature_contains_recovery(
                    statement.span,
                    first_statement_start(&declaration.body, statement.span.end),
                );
                self.visit_statement_list(&declaration.body, recovered);
                local_active
            }
            StatementKind::Class(declaration) => {
                for member in &declaration.members {
                    self.visit_class_member(member);
                }
                local_active
            }
            StatementKind::If(if_statement) => {
                self.visit_expression(&if_statement.condition);
                let then_active = self.visit_statement(&if_statement.then_statement, local_active);
                let else_active = if_statement
                    .else_statement
                    .as_deref()
                    .is_some_and(|statement| self.visit_statement(statement, local_active));
                local_active || then_active || else_active
            }
            StatementKind::Switch(switch_statement) => {
                self.visit_expression(&switch_statement.expression);
                let mut clause_active = local_active;
                for clause in &switch_statement.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.visit_expression(expression);
                    }
                    clause_active = self.visit_statement_list(&clause.statements, clause_active);
                }
                local_active || clause_active
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.visit_expression(expression);
                }
                local_active
            }
            StatementKind::Block(statements) => self.visit_statement_list(statements, local_active),
            StatementKind::Expression(expression) => {
                self.visit_expression(expression);
                local_active
            }
        }
    }

    fn visit_class_member(&mut self, member: &ClassMember) {
        match &member.kind {
            ClassMemberKind::Constructor {
                parameters, body, ..
            }
            | ClassMemberKind::Method {
                parameters, body, ..
            } => {
                self.visit_parameter_initializers(parameters);
                let recovered = self.signature_contains_recovery(
                    member.span,
                    first_statement_start(body, member.span.end),
                );
                self.visit_statement_list(body, recovered);
            }
            ClassMemberKind::Property { initializer, .. } => {
                if let Some(initializer) = initializer {
                    self.visit_expression(initializer);
                }
            }
        }
    }

    fn visit_parameter_initializers(&mut self, parameters: &[Parameter]) {
        for parameter in parameters {
            if let Some(initializer) = &parameter.initializer {
                self.visit_expression(initializer);
            }
        }
    }

    fn visit_expression(&mut self, expression: &Expression) {
        match &expression.kind {
            ExpressionKind::Identifier { .. }
            | ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.visit_expression(&property.value);
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.visit_expression(element);
                }
            }
            ExpressionKind::Call {
                callee, arguments, ..
            }
            | ExpressionKind::New {
                callee, arguments, ..
            } => {
                self.visit_expression(callee);
                for argument in arguments {
                    self.visit_expression(argument);
                }
            }
            ExpressionKind::Member { object, .. }
            | ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::Parenthesized(object) => self.visit_expression(object),
            ExpressionKind::ElementAccess { object, index } => {
                self.visit_expression(object);
                self.visit_expression(index);
            }
            ExpressionKind::Arrow {
                parameters, body, ..
            } => {
                self.visit_parameter_initializers(parameters);
                let recovered = self.signature_contains_recovery(
                    expression.span,
                    match body {
                        ArrowBody::Expression(body) => body.span.start,
                        ArrowBody::Block(statements) => {
                            first_statement_start(statements, expression.span.end)
                        }
                    },
                );
                match body {
                    ArrowBody::Expression(body) => self.visit_expression(body),
                    ArrowBody::Block(statements) => {
                        self.visit_statement_list(statements, recovered);
                    }
                }
            }
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }
            ExpressionKind::As { expression, .. } => self.visit_expression(expression),
        }
    }

    fn signature_contains_recovery(&self, span: Span, body_start: u32) -> bool {
        self.recoveries.iter().any(|recovery| {
            span.start <= recovery.authored_span.start
                && recovery.authored_span.end <= span.end
                && recovery.authored_span.start < body_start
        })
    }

    fn statement_starts_flow_region(&self, statement: &Statement) -> bool {
        let expression = match &statement.kind {
            StatementKind::If(statement) => &statement.condition,
            StatementKind::Switch(statement) if statement.recovered_discriminant => return true,
            StatementKind::Switch(statement) => &statement.expression,
            StatementKind::Import(_)
            | StatementKind::Export(_)
            | StatementKind::Variable(_)
            | StatementKind::Function(_)
            | StatementKind::Class(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Return(_)
            | StatementKind::Block(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty
            | StatementKind::Unknown => return false,
        };
        self.recoveries.iter().any(|recovery| {
            recovery.owner.statement == statement.id
                && expression.span.start <= recovery.authored_span.start
                && recovery.authored_span.start <= expression.span.end
        })
    }
}

fn first_statement_start(statements: &[Statement], fallback: u32) -> u32 {
    statements
        .first()
        .map_or(fallback, |statement| statement.span.start)
}

const fn statement_is_executable_region_member(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Import(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Empty => false,
        StatementKind::Export(declaration) => declaration.assignment.is_some(),
        StatementKind::Variable(_)
        | StatementKind::Function(_)
        | StatementKind::Class(_)
        | StatementKind::If(_)
        | StatementKind::Switch(_)
        | StatementKind::Break(_)
        | StatementKind::Continue(_)
        | StatementKind::Return(_)
        | StatementKind::Block(_)
        | StatementKind::Expression(_)
        | StatementKind::Unknown => true,
    }
}
