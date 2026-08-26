use std::collections::{BTreeMap, BTreeSet};

use crate::source::{FileId, NodeId, Span};
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

#[cfg(test)]
pub(super) type RecoveryStatementRole = RecoveryRole;

#[derive(Clone, Default)]
struct RecoveryContext {
    active: bool,
    owner_subtree: bool,
    ancestors: Vec<(NodeId, Span, bool)>,
}

struct RecoveryCollector {
    owner: ParserRecoveryOwner,
    authored: Span,
    extent: Span,
    owners: BTreeMap<NodeId, RecoveryRole>,
    declaration_fragments: BTreeSet<NodeId>,
    declaration: Option<(u32, NodeId)>,
    absorbed: Option<(u32, NodeId)>,
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
        match container {
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
            }
            DescendantContainer::Function(statement, _)
            | DescendantContainer::Class(statement, _) => {
                next.active &= self.owns_authored(statement.span);
            }
            DescendantContainer::ClassMember(member) => {
                next.active &= self.owns_authored(member.span);
            }
            DescendantContainer::FunctionLike(expression, _) => {
                next.active &= self.owns_authored(expression.span);
            }
        }
        next
    }

    fn nested_statement(
        &mut self,
        context: &RecoveryContext,
        statement: &'ast Statement,
        _next: Option<&'ast Statement>,
    ) -> NestedStatement {
        if !context.active {
            return NestedStatement::Handled;
        }
        if statement.id == self.owner.statement {
            let owner_is_return = matches!(statement.kind, StatementKind::Return(_));
            self.absorbed = context.ancestors[..context.ancestors.len() - 1]
                .iter()
                .filter(|(_, span, _)| {
                    span.start < self.extent.start && span.end <= self.extent.end
                })
                .map(|(id, span, _)| (span.len(), *id))
                .min();
            self.declaration = if owner_is_return {
                context
                    .ancestors
                    .iter()
                    .filter(|(_, span, declaration)| *declaration && self.owns_authored(*span))
                    .map(|(id, span, _)| (span.len(), *id))
                    .min()
            } else {
                Self::declaration(statement).then_some((statement.span.len(), statement.id))
            };
        }
        if self.extent.start <= statement.span.start && statement.span.start < self.extent.end {
            let role = if context.owner_subtree {
                RecoveryRole::SemanticOwner
            } else {
                RecoveryRole::RepresentationalFragment
            };
            self.owners.insert(statement.id, role);
            if role == RecoveryRole::RepresentationalFragment && Self::declaration(statement) {
                self.declaration_fragments.insert(statement.id);
            }
        }
        NestedStatement::Descend
    }
}

pub(super) struct RecoveryNodes {
    pub(super) owners: BTreeMap<NodeId, RecoveryRole>,
    declaration_fragments: BTreeSet<NodeId>,
    declaration: Option<NodeId>,
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
        owners: BTreeMap::new(),
        declaration_fragments: BTreeSet::new(),
        declaration: None,
        absorbed: None,
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
        .owners
        .insert(owner.statement, RecoveryRole::SemanticOwner);
    if let Some((_, absorbed)) = collector.absorbed {
        collector
            .owners
            .insert(absorbed, RecoveryRole::SemanticOwner);
    }
    RecoveryNodes {
        owners: collector.owners,
        declaration_fragments: collector.declaration_fragments,
        declaration: collector.declaration.map(|(_, owner)| owner),
    }
}

fn add_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: FileId,
    nodes: RecoveryNodes,
    gap: SyntaxGap,
) {
    for (owner, role) in nodes.owners {
        let scope = CapabilityScope::node(file, owner);
        add_owner(nonclaims, scope, gap, role);
        if nodes.declaration_fragments.contains(&owner) {
            add_syntax(nonclaims, &ALL_TARGETS[..2], scope, gap);
        }
    }
    if let Some(owner) = nodes.declaration {
        add_nonclaims(
            nonclaims,
            if gap == SyntaxGap::GeneratorFunctionLike {
                &ALL_TARGETS[1..2]
            } else {
                &ALL_TARGETS[..2]
            },
            CapabilityScope::node(file, owner),
            NonclaimReason::Syntax(gap),
            DeletionCondition::DeepestSemanticOwner(gap),
        );
    }
}

fn add_owner(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
    role: RecoveryRole,
) {
    let semantic = role != RecoveryRole::RepresentationalFragment;
    let deletion = match role {
        RecoveryRole::SemanticOwner => DeletionCondition::DeepestSemanticOwner(gap),
        RecoveryRole::RepresentationalFragment => DeletionCondition::SyntaxOwner(gap),
    };
    add_nonclaims(
        nonclaims,
        if gap == SyntaxGap::TypeRecovery {
            &ALL_TARGETS[2..6]
        } else {
            &ALL_TARGETS[2..5]
        },
        scope,
        NonclaimReason::Syntax(gap),
        deletion,
    );
    add_service_nonclaims(nonclaims, scope, gap, semantic);
}

pub(super) fn add_parser_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    function_signatures: &[Span],
) {
    for recovery in file.syntax.parser_recovery_facts() {
        if recovery.kind != ParserRecoveryKind::GeneratorFunctionLike
            && function_signatures.iter().any(|signature| {
                signature.start <= recovery.authored_span.start
                    && recovery.authored_span.end <= signature.end
            })
        {
            continue;
        }
        let gap = match recovery.kind {
            ParserRecoveryKind::Declaration => SyntaxGap::Declaration,
            ParserRecoveryKind::GeneratorFunctionLike => SyntaxGap::GeneratorFunctionLike,
            ParserRecoveryKind::Expression => SyntaxGap::Expression,
            ParserRecoveryKind::ObjectMember => SyntaxGap::ObjectMember,
            ParserRecoveryKind::ForStatement => SyntaxGap::ForStatement,
            ParserRecoveryKind::ComputedPropertyName => SyntaxGap::ComputedPropertyName,
            ParserRecoveryKind::ClassExpression => SyntaxGap::ClassExpression,
            ParserRecoveryKind::AngleAssertion => SyntaxGap::AngleAssertion,
            ParserRecoveryKind::RejectedGenericArrowPrefix => SyntaxGap::RejectedGenericArrowPrefix,
            ParserRecoveryKind::Type => SyntaxGap::TypeRecovery,
            ParserRecoveryKind::Template => SyntaxGap::Template,
        };
        if matches!(
            recovery.kind,
            ParserRecoveryKind::ObjectMember
                | ParserRecoveryKind::ForStatement
                | ParserRecoveryKind::ComputedPropertyName
                | ParserRecoveryKind::ClassExpression
        ) {
            add_both_emit(nonclaims, CapabilityScope::File(file.source.id), gap);
        }
        let scope = CapabilityScope::node(file.source.id, recovery.owner.statement);
        if recovery.kind == ParserRecoveryKind::RejectedGenericArrowPrefix {
            add_owner(nonclaims, scope, gap, RecoveryRole::SemanticOwner);
            add_nonclaims(
                nonclaims,
                &ALL_TARGETS[1..2],
                scope,
                NonclaimReason::Syntax(gap),
                DeletionCondition::DeepestSemanticOwner(gap),
            );
            add_both_emit(nonclaims, scope, gap);
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
        );
    }
}

pub(super) fn add_literal_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    kind: AuthoredLiteralKind,
    gap: SyntaxGap,
) {
    for fact in file
        .syntax
        .authored_literal_facts()
        .iter()
        .filter(|fact| fact.kind == kind)
    {
        add_nodes(
            nonclaims,
            file.source.id,
            recovery_nodes(file, fact.owner, fact.span, fact.recovery_extent),
            gap,
        );
    }
}
