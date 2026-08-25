use crate::syntax::{
    BinaryOperator, ClassMember, ClassMemberKind, Expression, ExpressionKind, ExpressionRoot,
    ExpressionTraversal::All, Parameter, SourceSyntaxFact, Statement, StatementKind,
    contains_matching_expression,
};

use super::{
    CapabilityNonclaim, CapabilityScope, CapabilityTarget, ProgramFile, SemanticGap, SyntaxGap,
    add_both_emit, add_semantic,
};

pub(super) fn add_inferred_product_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
) {
    let id = file.source.id;
    let mut record = |target, owner| {
        add_semantic(
            nonclaims,
            &[target],
            CapabilityScope::node(id, owner),
            SemanticGap::UnsignedRightShift,
        );
    };
    for root in &file.syntax.statements {
        if inferred_statement(root)
            || matches!(&root.kind, StatementKind::Export(value) if has_shift(value.assignment.as_ref()))
        {
            record(CapabilityTarget::Declaration, root.id);
        }
        root.for_each_statement(&mut |statement| {
            if inferred_statement(statement)
                || matches!(&statement.kind, StatementKind::Class(value) if value.members.iter().any(member_depends))
            {
                record(CapabilityTarget::QuickInfo, statement.id);
            }
        });
        if let StatementKind::Class(class) = &root.kind {
            for member in class.members.iter().filter(|member| member_depends(member)) {
                record(CapabilityTarget::Declaration, member.id);
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

fn inferred_statement(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Variable(value) => {
            value.annotation.is_none() && has_shift(value.initializer.as_ref())
        }
        StatementKind::Function(value) => value.parameters.iter().any(parameter_depends),
        _ => false,
    }
}

fn member_depends(member: &ClassMember) -> bool {
    match &member.kind {
        ClassMemberKind::Property {
            annotation,
            initializer,
            ..
        } => annotation.is_none() && has_shift(initializer.as_ref()),
        ClassMemberKind::Constructor { parameters, .. }
        | ClassMemberKind::Method { parameters, .. } => parameters.iter().any(parameter_depends),
    }
}

fn parameter_depends(parameter: &Parameter) -> bool {
    parameter.annotation.is_none() && has_shift(parameter.initializer.as_ref())
}

fn has_shift(expression: Option<&Expression>) -> bool {
    expression.is_some_and(|expression| expression_contains(expression, is_shift))
}

fn expression_contains(expression: &Expression, predicate: fn(&Expression) -> bool) -> bool {
    contains_matching_expression(ExpressionRoot::Expression(expression), All, predicate)
}

const fn is_shift(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::UnsignedRightShift,
            ..
        }
    )
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
