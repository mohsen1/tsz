use crate::syntax::{
    BinaryOperator as Op, ClassMember, ClassMemberKind, Expression, ExpressionKind, ExpressionRoot,
    ExpressionTraversal::All, Literal, NumberLiteral, Parameter, SourceSyntaxFact, Statement,
    StatementKind, StringLiteral, contains_matching_expression,
};

use super::emit_targets::{
    class_member_declaration_type_is_erased, class_parameter_property_type_is_published,
};
use super::{CapabilityTarget, ProgramFile, ScopedNonclaims, SemanticGap, SyntaxGap};

pub(super) fn add_nonclaims(nonclaims: &mut ScopedNonclaims<'_>, file: &ProgramFile) {
    add_unsigned_shift_nonclaims(nonclaims, file);
    add_declaration_expression_nonclaims(nonclaims, file);
}

fn add_unsigned_shift_nonclaims(nonclaims: &mut ScopedNonclaims<'_>, file: &ProgramFile) {
    let id = file.source.id;
    let inference = InferredExpression::Shift;
    let record = |nonclaims: &mut ScopedNonclaims<'_>, target, owner| {
        nonclaims
            .node(id, owner)
            .semantic(&[target], SemanticGap::UnsignedRightShift);
    };
    for root in &file.syntax.statements {
        if inferred_statement(root, inference)
            || matches!(&root.kind, StatementKind::Export(value) if inference.occurs_in(value.assignment.as_ref()))
        {
            record(nonclaims, CapabilityTarget::Declaration, root.id);
        }
        root.for_each_statement(&mut |statement| {
            if inferred_statement(statement, inference)
                || matches!(&statement.kind, StatementKind::Class(value) if value.members.iter().any(|member| inferred_member(member, inference)))
            {
                record(nonclaims, CapabilityTarget::QuickInfo, statement.id);
            }
            if inferred_statement(statement, InferredExpression::Template)
                || matches!(&statement.kind, StatementKind::Class(value) if value.members.iter().any(|member| inferred_member(member, InferredExpression::Template)))
            {
                nonclaims.node(id, statement.id).syntax_owned_by(
                    &[CapabilityTarget::QuickInfo],
                    SyntaxGap::Template,
                    true,
                );
                nonclaims.syntax(&[CapabilityTarget::References], SyntaxGap::Template);
            }
        });
        if let StatementKind::Class(class) = &root.kind {
            for member in &class.members {
                if inferred_member(member, inference) {
                    record(nonclaims, CapabilityTarget::Declaration, member.id);
                }
            }
        }
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::UnsignedRightShiftAssignmentRecovery)
    {
        nonclaims.emit(SyntaxGap::UnsignedRightShiftAssignmentRecovery);
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::UnsignedRightShiftOperandRecovery)
        || contains_recovered_shift(&file.syntax.statements)
    {
        nonclaims.emit(SyntaxGap::UnsignedRightShiftOperandRecovery);
    }
}

fn add_declaration_expression_nonclaims(nonclaims: &mut ScopedNonclaims<'_>, file: &ProgramFile) {
    let mut record = |owner, targets: &[CapabilityTarget]| {
        nonclaims
            .node(file.source.id, owner)
            .semantic(targets, SemanticGap::DeclarationExpressionSummary);
    };
    let inference = InferredExpression::DeclarationSummary;
    let products = &[CapabilityTarget::Declaration, CapabilityTarget::References];
    for root in &file.syntax.statements {
        if let StatementKind::Export(value) = &root.kind
            && inference.occurs_in(value.assignment.as_ref())
        {
            record(
                root.id,
                if value.default_export {
                    &[CapabilityTarget::References]
                } else {
                    products
                },
            );
        }
        root.for_each_statement(&mut |statement| {
            if inferred_statement(statement, inference) {
                record(statement.id, products);
            }
            if inferred_statement(statement, InferredExpression::DeclarationValue) {
                record(statement.id, &[CapabilityTarget::DeclarationValue]);
            }
            if inferred_statement(statement, InferredExpression::Array) {
                record(statement.id, &[CapabilityTarget::QuickInfo]);
            }
        });
        if let StatementKind::Class(class) = &root.kind {
            for member in &class.members {
                if inferred_member(member, inference) {
                    record(member.id, products);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum InferredExpression {
    Shift,
    Array,
    DeclarationValue,
    DeclarationSummary,
    Template,
}

impl InferredExpression {
    fn occurs_in(self, expression: Option<&Expression>) -> bool {
        expression.is_some_and(|root| {
            contains_matching_expression(ExpressionRoot::Expression(root), All, |expression| {
                self.matches(expression)
            })
        })
    }

    const fn matches(self, expression: &Expression) -> bool {
        match self {
            Self::Shift => matches!(
                expression.kind,
                ExpressionKind::Binary {
                    operator: Op::UnsignedRightShift,
                    ..
                }
            ),
            Self::Array => matches!(expression.kind, ExpressionKind::Array(_)),
            Self::DeclarationValue => matches!(
                &expression.kind,
                ExpressionKind::Conditional { when_true, when_false, .. }
                    if !matches!(when_true.kind, ExpressionKind::Missing)
                        && !matches!(when_false.kind, ExpressionKind::Missing)
            ),
            Self::DeclarationSummary => match expression.kind {
                ExpressionKind::Binary { operator, .. } => {
                    !matches!(operator, Op::UnsignedRightShift)
                }
                ExpressionKind::Call { .. }
                | ExpressionKind::Conditional { .. }
                | ExpressionKind::NonNull(_)
                | ExpressionKind::RegularExpression(_)
                | ExpressionKind::Literal(
                    Literal::NoSubstitutionTemplate(_)
                    | Literal::String(StringLiteral::Extended(_))
                    | Literal::Number(NumberLiteral::Recovery(_)),
                ) => true,
                _ => false,
            },
            Self::Template => matches!(expression.kind, ExpressionKind::Template(_)),
        }
    }
}

fn inferred_statement(statement: &Statement, inference: InferredExpression) -> bool {
    let inferred_parameter = |parameter: &Parameter| inferred_parameter(parameter, inference);
    match &statement.kind {
        StatementKind::Variable(value) => value.declarators.iter().any(|declarator| {
            declarator.annotation.is_none() && inference.occurs_in(declarator.initializer.as_ref())
        }),
        StatementKind::Function(value) => value.parameters.iter().any(inferred_parameter),
        _ => false,
    }
}

fn inferred_member(member: &ClassMember, inference: InferredExpression) -> bool {
    if class_member_declaration_type_is_erased(member) {
        return match &member.kind {
            ClassMemberKind::Constructor { parameters, .. } => parameters.iter().any(|parameter| {
                class_parameter_property_type_is_published(parameter)
                    && inferred_parameter(parameter, inference)
            }),
            ClassMemberKind::Property { .. } | ClassMemberKind::Method { .. } => false,
        };
    }
    match &member.kind {
        ClassMemberKind::Property {
            annotation,
            initializer,
            ..
        } => {
            annotation.is_none()
                && (matches!(inference, InferredExpression::DeclarationSummary)
                    || inference.occurs_in(initializer.as_ref()))
        }
        ClassMemberKind::Constructor { parameters, .. }
        | ClassMemberKind::Method { parameters, .. } => parameters
            .iter()
            .any(|parameter| inferred_parameter(parameter, inference)),
    }
}

fn inferred_parameter(parameter: &Parameter, inference: InferredExpression) -> bool {
    parameter.annotation.is_none() && inference.occurs_in(parameter.initializer.as_ref())
}

fn expression_contains(expression: &Expression, predicate: fn(&Expression) -> bool) -> bool {
    contains_matching_expression(ExpressionRoot::Expression(expression), All, predicate)
}

const fn is_missing(expression: &Expression) -> bool {
    matches!(expression.kind, ExpressionKind::Missing)
}

fn contains_recovered_shift(statements: &[Statement]) -> bool {
    contains_matching_expression(ExpressionRoot::Statements(statements), All, |expression| {
        match &expression.kind {
            ExpressionKind::Binary {
                left,
                operator: Op::UnsignedRightShift,
                right,
                ..
            } => expression_contains(left, is_missing) || expression_contains(right, is_missing),
            _ => false,
        }
    })
}
