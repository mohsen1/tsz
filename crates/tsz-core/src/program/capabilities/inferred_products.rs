use crate::syntax::{
    BinaryOperator, ClassMember, ClassMemberKind, Expression, ExpressionKind, ExpressionRoot,
    ExpressionTraversal::All, Literal, NumberLiteral, Parameter, SourceSyntaxFact, Statement,
    StatementKind, StringLiteral, contains_matching_expression,
};

use super::emit_targets::{
    class_member_declaration_type_is_erased, class_parameter_property_type_is_published,
};
use super::{
    CapabilityNonclaim, CapabilityScope, CapabilityTarget, DeletionCondition, NonclaimReason,
    ProgramFile, SemanticGap, SyntaxGap, add_both_emit, add_nonclaims as record_nonclaims,
    add_semantic,
};

pub(super) fn add_nonclaims(nonclaims: &mut Vec<CapabilityNonclaim>, file: &ProgramFile) {
    add_unsigned_shift_nonclaims(nonclaims, file);
    add_declaration_expression_nonclaims(nonclaims, file);
}

fn add_unsigned_shift_nonclaims(nonclaims: &mut Vec<CapabilityNonclaim>, file: &ProgramFile) {
    let id = file.source.id;
    let inference = InferredExpression::Shift;
    let record = |nonclaims: &mut Vec<CapabilityNonclaim>, target, owner| {
        add_semantic(
            nonclaims,
            &[target],
            CapabilityScope::node(id, owner),
            SemanticGap::UnsignedRightShift,
        );
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
                record_nonclaims(
                    nonclaims,
                    &[CapabilityTarget::QuickInfo],
                    CapabilityScope::node(id, statement.id),
                    NonclaimReason::Syntax(SyntaxGap::Template),
                    DeletionCondition::DeepestSemanticOwner(SyntaxGap::Template),
                );
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
    let scope = CapabilityScope::File(id);
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::UnsignedRightShiftAssignmentRecovery)
    {
        add_both_emit(
            nonclaims,
            scope,
            SyntaxGap::UnsignedRightShiftAssignmentRecovery,
        );
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::UnsignedRightShiftOperandRecovery)
        || contains_recovered_shift(&file.syntax.statements)
    {
        add_both_emit(
            nonclaims,
            scope,
            SyntaxGap::UnsignedRightShiftOperandRecovery,
        );
    }
}

fn add_declaration_expression_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
) {
    let mut record = |owner| {
        add_semantic(
            nonclaims,
            &[CapabilityTarget::Declaration],
            CapabilityScope::node(file.source.id, owner),
            SemanticGap::DeclarationExpressionSummary,
        );
    };
    for inference in [InferredExpression::Call, InferredExpression::LiteralSummary] {
        for root in &file.syntax.statements {
            if inferred_statement(root, inference)
                || matches!(&root.kind, StatementKind::Export(value) if inference.occurs_in(value.assignment.as_ref()))
            {
                record(root.id);
            }
            if let StatementKind::Class(class) = &root.kind {
                for member in &class.members {
                    if inferred_member(member, inference) {
                        record(member.id);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum InferredExpression {
    Shift,
    Call,
    LiteralSummary,
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
                    operator: BinaryOperator::UnsignedRightShift,
                    ..
                }
            ),
            Self::Call => matches!(expression.kind, ExpressionKind::Call { .. }),
            Self::LiteralSummary => matches!(
                expression.kind,
                ExpressionKind::NonNull(_)
                    | ExpressionKind::RegularExpression(_)
                    | ExpressionKind::Literal(
                        Literal::NoSubstitutionTemplate(_)
                            | Literal::String(StringLiteral::Extended(_))
                            | Literal::Number(NumberLiteral::Recovery(_))
                    )
            ),
            Self::Template => matches!(expression.kind, ExpressionKind::Template(_)),
        }
    }
}

fn inferred_statement(statement: &Statement, inference: InferredExpression) -> bool {
    match &statement.kind {
        StatementKind::Variable(value) => value.declarators.iter().any(|declarator| {
            declarator.annotation.is_none() && inference.occurs_in(declarator.initializer.as_ref())
        }),
        StatementKind::Function(value) => value
            .parameters
            .iter()
            .any(|parameter| inferred_parameter(parameter, inference)),
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
        } => annotation.is_none() && inference.occurs_in(initializer.as_ref()),
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
                operator: BinaryOperator::UnsignedRightShift,
                right,
                ..
            } => expression_contains(left, is_missing) || expression_contains(right, is_missing),
            _ => false,
        }
    })
}
