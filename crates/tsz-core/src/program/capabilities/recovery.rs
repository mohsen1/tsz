use std::collections::{BTreeMap, BTreeSet};

use crate::source::{FileId, NodeId, Span};
use crate::syntax::ParserRecoveryKind::{ConditionalExpression, Expression, MissingExpression};
use crate::syntax::{
    DescendantAdapter, DescendantContainer, NestedStatement, ParserRecoveryKind,
    ParserRecoveryOwner, Statement, StatementKind, walk_statement_list,
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryRole {
    SemanticOwner,
    RepresentationalFragment,
}

#[derive(Clone, Default)]
struct RecoveryContext {
    active: bool,
    owner_subtree: bool,
    inferred_return: Option<NodeId>,
    ancestors: Vec<(NodeId, Span, bool)>,
}

#[derive(Default)]
pub(super) struct RecoveryNodes {
    pub(super) owners: BTreeMap<NodeId, RecoveryRole>,
    declaration_fragments: BTreeSet<NodeId>,
    declaration: Option<NodeId>,
    inferred_value: Option<NodeId>,
}

struct RecoveryCollector {
    owner: ParserRecoveryOwner,
    authored: Span,
    extent: Span,
    nodes: RecoveryNodes,
}

impl RecoveryCollector {
    const fn owns_authored(&self, span: Span) -> bool {
        span.start <= self.authored.start && self.authored.end <= span.end
    }

    const fn declaration(statement: &Statement) -> bool {
        matches!(
            statement.kind,
            StatementKind::Import(_)
                | StatementKind::Variable(_)
                | StatementKind::Function(_)
                | StatementKind::Class(_)
                | StatementKind::TypeAlias(_)
                | StatementKind::Interface(_)
        )
    }
}

impl<'ast> DescendantAdapter<'ast> for RecoveryCollector {
    type Context = RecoveryContext;

    fn context(
        &mut self,
        context: &RecoveryContext,
        container: DescendantContainer<'ast>,
    ) -> RecoveryContext {
        let mut next = context.clone();
        let (span, inferred_return) = match container {
            DescendantContainer::Statement(statement) => {
                next.active &= !matches!(
                    statement.kind,
                    StatementKind::Function(_) | StatementKind::Class(_)
                ) || self.owns_authored(statement.span);
                next.owner_subtree |= statement.id == self.owner.statement;
                if next.active {
                    next.ancestors.push((
                        statement.id,
                        statement.span,
                        Self::declaration(statement),
                    ));
                }
                return next;
            }
            DescendantContainer::Function(statement, function) => (
                statement.span,
                (function.return_type.is_none()
                    && function
                        .body_span
                        .is_some_and(|span| self.owns_authored(span)))
                .then_some(statement.id),
            ),
            DescendantContainer::Class(statement, _) => (statement.span, None),
            DescendantContainer::ClassMember(member) => (member.span, None),
            DescendantContainer::FunctionLike(expression, _) => (expression.span, None),
        };
        next.active &= self.owns_authored(span);
        next.inferred_return = inferred_return;
        next
    }

    fn nested_statement(
        &mut self,
        context: &RecoveryContext,
        statement: &'ast Statement,
        next: Option<&'ast Statement>,
    ) -> NestedStatement {
        if !context.active {
            return NestedStatement::Handled;
        }
        let owner_return = context.owner_subtree
            && matches!(statement.kind, StatementKind::Return(_))
            && !contains_matching_expression(
                ExpressionRoot::Statement(statement),
                ExpressionTraversal::All,
                |expression| {
                    matches!(expression.kind, ExpressionKind::FunctionLike(_))
                        && self.owns_authored(expression.span)
                },
            );
        let adjacent_return = matches!(statement.kind, StatementKind::Return(_))
            && next.is_some_and(|next| next.id == self.owner.statement)
            && statement.span.end <= self.authored.start;
        if let Some(owner) = context
            .inferred_return
            .filter(|_| owner_return || adjacent_return)
        {
            self.nodes.inferred_value.get_or_insert(owner);
        }
        if statement.id == self.owner.statement {
            let owner_is_return = matches!(statement.kind, StatementKind::Return(_));
            if let StatementKind::Variable(variable) = &statement.kind {
                self.nodes.inferred_value = variable
                    .declarators
                    .iter()
                    .any(|declarator| {
                        declarator.annotation.is_none()
                            && declarator
                                .initializer
                                .as_ref()
                                .is_some_and(|initializer| self.owns_authored(initializer.span))
                    })
                    .then_some(statement.id);
            }
            if let Some((_, absorbed)) = context.ancestors[..context.ancestors.len() - 1]
                .iter()
                .filter(|(_, span, _)| {
                    span.start < self.extent.start && span.end <= self.extent.end
                })
                .map(|(id, span, _)| (span.len(), *id))
                .min()
            {
                self.nodes
                    .owners
                    .insert(absorbed, RecoveryRole::SemanticOwner);
            }
            self.nodes.declaration = if owner_is_return {
                context
                    .ancestors
                    .iter()
                    .filter(|(_, span, declaration)| *declaration && self.owns_authored(*span))
                    .min_by_key(|(_, span, _)| span.len())
                    .map(|(id, _, _)| *id)
            } else {
                Self::declaration(statement).then_some(statement.id)
            };
        }
        if self.extent.start <= statement.span.start && statement.span.start < self.extent.end {
            let role = if context.owner_subtree {
                RecoveryRole::SemanticOwner
            } else {
                RecoveryRole::RepresentationalFragment
            };
            self.nodes.owners.insert(statement.id, role);
            if role == RecoveryRole::RepresentationalFragment && Self::declaration(statement) {
                self.nodes.declaration_fragments.insert(statement.id);
            }
        }
        NestedStatement::Descend
    }
}

pub(super) fn recovery_nodes(
    file: &ProgramFile,
    owner: ParserRecoveryOwner,
    authored: Span,
    extent: Span,
) -> RecoveryNodes {
    debug_assert!(
        file.syntax
            .statements
            .iter()
            .any(|statement| statement.id == owner.root_statement)
    );
    let mut collector = RecoveryCollector {
        owner,
        authored,
        extent,
        nodes: RecoveryNodes::default(),
    };
    walk_statement_list(
        &mut collector,
        &RecoveryContext {
            active: true,
            ..RecoveryContext::default()
        },
        &file.syntax.statements,
    );
    collector
        .nodes
        .owners
        .insert(owner.statement, RecoveryRole::SemanticOwner);
    collector.nodes
}

fn add_nodes(
    nonclaims: &mut ScopedNonclaims<'_>,
    file: FileId,
    nodes: RecoveryNodes,
    gap: SyntaxGap,
    unmodeled_syntax: bool,
) {
    for (owner, role) in nodes.owners {
        add_owner(nonclaims.node(file, owner), gap, role);
        if nodes.declaration_fragments.contains(&owner) {
            nonclaims.node(file, owner).syntax(&ALL_TARGETS[..2], gap);
        }
    }
    if let Some(owner) = nodes.declaration.filter(|_| unmodeled_syntax) {
        nonclaims.node(file, owner).syntax_owned_by(
            if gap == SyntaxGap::GeneratorFunctionLike {
                &ALL_TARGETS[1..2]
            } else {
                &ALL_TARGETS[..2]
            },
            gap,
            true,
        );
    }
    if let Some(owner) = nodes.inferred_value {
        nonclaims
            .node(file, owner)
            .syntax_owned_by(&ALL_TARGETS[1..2], gap, true);
    }
}

fn add_owner(mut nonclaims: ScopedNonclaims<'_>, gap: SyntaxGap, role: RecoveryRole) {
    let semantic = role != RecoveryRole::RepresentationalFragment;
    nonclaims.syntax_owned_by(
        if gap == SyntaxGap::TypeRecovery {
            &ALL_TARGETS[2..6]
        } else {
            &ALL_TARGETS[2..5]
        },
        gap,
        semantic,
    );
    nonclaims.syntax_owned_by(&ALL_TARGETS[7..], gap, semantic);
}

const fn parser_recovery_gap(kind: ParserRecoveryKind) -> SyntaxGap {
    match kind {
        ParserRecoveryKind::Declaration => SyntaxGap::Declaration,
        ParserRecoveryKind::GeneratorFunctionLike => SyntaxGap::GeneratorFunctionLike,
        Expression | MissingExpression | ConditionalExpression => SyntaxGap::Expression,
        ParserRecoveryKind::ObjectMember => SyntaxGap::ObjectMember,
        ParserRecoveryKind::ForStatement => SyntaxGap::ForStatement,
        ParserRecoveryKind::ComputedPropertyName => SyntaxGap::ComputedPropertyName,
        ParserRecoveryKind::ClassMember => SyntaxGap::Class,
        ParserRecoveryKind::ClassExpression => SyntaxGap::ClassExpression,
        ParserRecoveryKind::AngleAssertion => SyntaxGap::AngleAssertion,
        ParserRecoveryKind::RejectedGenericArrowPrefix => SyntaxGap::RejectedGenericArrowPrefix,
        ParserRecoveryKind::Type | ParserRecoveryKind::MissingType => SyntaxGap::TypeRecovery,
        ParserRecoveryKind::Template => SyntaxGap::Template,
    }
}

pub(super) fn add_parser_nodes(
    nonclaims: &mut ScopedNonclaims<'_>,
    file: &ProgramFile,
    function_signatures: &[Span],
) {
    for recovery in &file.syntax.parser_recovery_facts {
        let gap = parser_recovery_gap(recovery.kind);
        let literal_owned = file
            .syntax
            .authored_literal_facts
            .iter()
            .any(|fact| fact.owner == recovery.owner && fact.span == recovery.authored_span);
        if !literal_owned
            && !matches!(
                recovery.kind,
                ParserRecoveryKind::Declaration
                    | ConditionalExpression
                    | MissingExpression
                    | ParserRecoveryKind::MissingType
                    | ParserRecoveryKind::Template
            )
        {
            nonclaims
                .node(file.source.id, recovery.owner.statement)
                .syntactic_diagnostics(gap);
        }
        if recovery.kind == ParserRecoveryKind::Template {
            nonclaims.emit(SyntaxGap::Template);
        }
        if matches!(recovery.kind, ConditionalExpression | MissingExpression) {
            nonclaims
                .node(file.source.id, recovery.owner.statement)
                .javascript(gap);
        }
        if recovery.kind != ParserRecoveryKind::GeneratorFunctionLike
            && function_signatures.iter().any(|signature| {
                signature.start <= recovery.authored_span.start
                    && recovery.authored_span.end <= signature.end
            })
        {
            continue;
        }
        if matches!(
            recovery.kind,
            ParserRecoveryKind::ObjectMember
                | ParserRecoveryKind::ForStatement
                | ParserRecoveryKind::ComputedPropertyName
                | ParserRecoveryKind::ClassExpression
        ) {
            nonclaims.emit(gap);
        }
        if recovery.kind == ParserRecoveryKind::RejectedGenericArrowPrefix {
            let owner = recovery.owner.statement;
            add_owner(
                nonclaims.node(file.source.id, owner),
                gap,
                RecoveryRole::SemanticOwner,
            );
            let mut owner_nonclaims = nonclaims.node(file.source.id, owner);
            owner_nonclaims.syntax_owned_by(&ALL_TARGETS[1..2], gap, true);
            owner_nonclaims.emit(gap);
            continue;
        }
        add_nodes(
            nonclaims,
            file.source.id,
            recovery_nodes(
                file,
                recovery.owner,
                recovery.authored_span,
                recovery.recovery_extent,
            ),
            gap,
            recovery.kind != ConditionalExpression,
        );
    }
}

pub(super) fn add_literal_nodes(
    nonclaims: &mut ScopedNonclaims<'_>,
    file: &ProgramFile,
    kind: AuthoredLiteralKind,
    gap: SyntaxGap,
) {
    for fact in file
        .syntax
        .authored_literal_facts
        .iter()
        .filter(|fact| fact.kind == kind)
    {
        let nodes = recovery_nodes(file, fact.owner, fact.span, fact.recovery_extent);
        add_nodes(nonclaims, file.source.id, nodes, gap, true);
    }
}
