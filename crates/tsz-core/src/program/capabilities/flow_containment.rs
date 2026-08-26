use std::collections::BTreeSet;

use crate::source::{NodeId, Span};
use crate::syntax::{
    ClassMemberKind, DescendantAdapter, DescendantContainer, Expression, ExpressionEdge,
    ExpressionKind, FunctionLikeBody, NestedStatement, ParameterNameKind, ParserRecoveryFact,
    ParserRecoveryKind, Statement, StatementKind, TokenKind, walk_statement_list,
};

use super::{SemanticGap, SyntaxGap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FileBoundary {
    ClassProduct,
    CommonJsClass,
    Declaration,
    ClassProperty,
}

/// Immutable ownership inventory for capability decisions. Statement-list
/// state is folded in source order; each function, class, member, and
/// function-like expression starts a fresh executable container.
#[derive(Default)]
pub(super) struct SemanticNodeInventory {
    pub(super) flow_regions: BTreeSet<NodeId>,
    pub(super) function_likes: BTreeSet<NodeId>,
    pub(super) function_like_gaps: Vec<(NodeId, SemanticGap)>,
    pub(super) javascript_jsdoc_values: BTreeSet<NodeId>,
    pub(super) javascript_jsdoc_checks: BTreeSet<NodeId>,
    pub(super) function_like_binding_patterns: BTreeSet<NodeId>,
    pub(super) function_like_signatures: Vec<Span>,
    pub(super) function_expressions: Vec<FunctionExpressionProducts>,
    pub(super) object_method_owners: BTreeSet<NodeId>,
    pub(super) object_methods: Vec<FunctionExpressionProducts>,
    pub(super) recovered_function_likes: BTreeSet<(NodeId, SyntaxGap)>,
    pub(super) boundaries: BTreeSet<FileBoundary>,
}

pub(super) struct FunctionExpressionProducts {
    pub(super) owner: NodeId,
    pub(super) span: Span,
    pub(super) body_span: Span,
    pub(super) inline_body_supported: bool,
}

pub(super) fn semantic_node_inventory(
    statements: &[Statement],
    recoveries: &[ParserRecoveryFact],
    javascript_jsdoc_casts: &BTreeSet<NodeId>,
) -> SemanticNodeInventory {
    let mut collector = FlowRegionCollector {
        recoveries,
        javascript_jsdoc_casts,
        out: SemanticNodeInventory::default(),
    };
    walk_statement_list(&mut collector, &FlowContext::ROOT, statements);
    collector.out
}

#[derive(Clone, Copy)]
struct FlowContext {
    active: bool,
    owners: (NodeId, NodeId),
}

impl FlowContext {
    const ROOT: Self = Self::fresh(NodeId(0), false);

    const fn owner(self, owner: NodeId) -> Self {
        Self {
            owners: (owner, owner),
            ..self
        }
    }

    const fn fresh(owner: NodeId, active: bool) -> Self {
        Self {
            active,
            owners: (owner, owner),
        }
    }
}

struct FlowRegionCollector<'a> {
    recoveries: &'a [ParserRecoveryFact],
    javascript_jsdoc_casts: &'a BTreeSet<NodeId>,
    out: SemanticNodeInventory,
}

impl<'ast> DescendantAdapter<'ast> for FlowRegionCollector<'_> {
    type Context = FlowContext;

    fn context(
        &mut self,
        context: &FlowContext,
        container: DescendantContainer<'ast>,
    ) -> FlowContext {
        match container {
            DescendantContainer::Statement(statement) => FlowContext {
                active: context.active || self.statement_starts_flow_region(statement),
                ..context.owner(statement.id)
            },
            DescendantContainer::Function(statement, declaration) => self.function_context(
                statement.id,
                statement.span,
                &declaration.body,
                declaration.has_leading_jsdoc,
                &declaration.parameters,
            ),
            DescendantContainer::Class(statement, declaration) => {
                self.out.boundaries.extend(
                    (declaration.abstract_class
                        || declaration
                            .members
                            .iter()
                            .any(|member| !member.emit_products_supported))
                    .then_some(FileBoundary::ClassProduct),
                );
                self.out.boundaries.extend(
                    declaration
                        .members
                        .iter()
                        .any(|member| matches!(member.kind, ClassMemberKind::Property { .. }))
                        .then_some(FileBoundary::ClassProperty),
                );
                FlowContext::fresh(statement.id, false)
            }
            DescendantContainer::ClassMember(member) => {
                let (parameters, body, has_body) = match &member.kind {
                    ClassMemberKind::Constructor {
                        parameters,
                        body,
                        has_body,
                        ..
                    }
                    | ClassMemberKind::Method {
                        parameters,
                        body,
                        has_body,
                        ..
                    } => (parameters.as_slice(), body.as_slice(), *has_body),
                    ClassMemberKind::Property { .. } => unreachable!(),
                };
                if !has_body {
                    self.out.boundaries.insert(FileBoundary::CommonJsClass);
                }
                self.function_context(member.id, member.span, body, false, parameters)
            }
            DescendantContainer::FunctionLike(expression, function) => {
                let body_start = self.record_function_like(expression, function);
                FlowContext::fresh(
                    expression.id,
                    self.record_recovery(expression.id, expression.span, body_start),
                )
            }
        }
    }

    fn nested_statement(
        &mut self,
        context: &FlowContext,
        statement: &'ast Statement,
        _next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        self.out.flow_regions.extend(
            (context.active && statement_is_executable_region_member(statement))
                .then_some(statement.id),
        );
        let declaration_boundary = match &statement.kind {
            StatementKind::Import(item) => {
                item.type_only || item.bindings.iter().any(|binding| binding.type_only)
            }
            StatementKind::Export(item) => {
                item.type_only || item.specifiers.iter().any(|specifier| specifier.type_only)
            }
            StatementKind::TypeAlias(_) | StatementKind::Interface(_) => true,
            _ => false,
        };
        self.out
            .boundaries
            .extend(declaration_boundary.then_some(FileBoundary::Declaration));
        self.out.javascript_jsdoc_values.extend(
            matches!(&statement.kind, StatementKind::Variable(item) if item.has_leading_jsdoc)
                .then_some(statement.id),
        );
        NestedStatement::Descend
    }

    fn expression_edge(
        &mut self,
        context: &FlowContext,
        edge: ExpressionEdge<'ast>,
    ) -> FlowContext {
        match edge {
            ExpressionEdge::AssignmentRight(expression) => FlowContext {
                owners: (expression.id, context.owners.1),
                ..*context
            },
            ExpressionEdge::PropertyInitializer(member) => FlowContext::fresh(member.id, false),
        }
    }

    fn fold_context(&mut self, context: &FlowContext, nested: &FlowContext) -> FlowContext {
        FlowContext {
            active: context.active || nested.active,
            ..*context
        }
    }

    fn expression(&mut self, context: &FlowContext, expression: &'ast Expression) {
        if self.javascript_jsdoc_casts.contains(&expression.id) {
            self.out.javascript_jsdoc_values.insert(context.owners.0);
            self.out.javascript_jsdoc_checks.insert(context.owners.1);
        }
        self.out.javascript_jsdoc_values.extend(
            matches!(
                expression.kind,
                ExpressionKind::Assignment {
                    has_leading_jsdoc: true,
                    ..
                }
            )
            .then_some(expression.id),
        );
    }
}

impl FlowRegionCollector<'_> {
    fn function_context(
        &mut self,
        owner: NodeId,
        span: Span,
        body: &[Statement],
        jsdoc: bool,
        parameters: &[crate::syntax::Parameter],
    ) -> FlowContext {
        self.record_signature_gaps(owner, jsdoc, parameters);
        FlowContext::fresh(
            owner,
            self.record_recovery(owner, span, first_statement_start(body, span.end)),
        )
    }

    fn record_signature_gaps(
        &mut self,
        owner: NodeId,
        has_leading_jsdoc: bool,
        parameters: &[crate::syntax::Parameter],
    ) {
        self.out
            .function_like_gaps
            .extend(has_leading_jsdoc.then_some((owner, SemanticGap::JavaScriptJSDocSignature)));
        self.out.function_like_gaps.extend(
            parameters
                .iter()
                .any(|parameter| parameter.name_kind == ParameterNameKind::This)
                .then_some((owner, SemanticGap::ExplicitThisParameter)),
        );
    }

    fn record_function_like(
        &mut self,
        expression: &Expression,
        function: &crate::syntax::FunctionLikeExpression,
    ) -> u32 {
        let owner = expression.id;
        self.out.function_likes.insert(owner);
        self.record_signature_gaps(owner, function.has_leading_jsdoc, &function.parameters);
        let object_method = function.syntax.is_object_method();
        self.out
            .object_method_owners
            .extend(object_method.then_some(owner));
        if let Some((_, body)) = function.syntax.function()
            && let Some(body_span) = function.body_span
        {
            let products = FunctionExpressionProducts {
                owner,
                span: expression.span,
                body_span,
                inline_body_supported: inline_body_supported(body),
            };
            (if object_method {
                &mut self.out.object_methods
            } else {
                &mut self.out.function_expressions
            })
            .push(products);
        }
        self.out.function_like_gaps.extend(
            matches!(function.syntax.function(), Some((Some(name), _)) if name.token_kind != TokenKind::Identifier)
                .then_some((owner, SemanticGap::FunctionExpressionBindingName)),
        );
        self.out.function_like_gaps.extend(
            (!function.type_parameters.is_empty())
                .then_some((owner, SemanticGap::FunctionLikeTypeParameters)),
        );
        self.out.function_like_binding_patterns.extend(
            function
                .parameters
                .iter()
                .any(|parameter| parameter.name_kind == ParameterNameKind::BindingPattern)
                .then_some(owner),
        );
        let body_start = match function.syntax.body() {
            FunctionLikeBody::Expression(body) => body.span.start,
            FunctionLikeBody::Statements(body) => first_statement_start(body, expression.span.end),
        };
        self.out.function_like_signatures.push(Span {
            file: expression.span.file,
            start: expression.span.start,
            end: body_start,
        });
        body_start
    }

    fn record_recovery(&mut self, owner: NodeId, span: Span, body_start: u32) -> bool {
        let gap = self
            .recoveries
            .iter()
            .filter(|recovery| {
                span.start <= recovery.authored_span.start
                    && recovery.authored_span.end <= span.end
                    && recovery.authored_span.start < body_start
            })
            .max_by_key(|recovery| recovery.kind == ParserRecoveryKind::GeneratorFunctionLike)
            .map(|recovery| match recovery.kind {
                ParserRecoveryKind::GeneratorFunctionLike => SyntaxGap::GeneratorFunctionLike,
                _ => SyntaxGap::Expression,
            });
        if let Some(gap) = gap {
            self.out.recovered_function_likes.insert((owner, gap));
        }
        gap.is_some()
    }

    fn statement_starts_flow_region(&self, statement: &Statement) -> bool {
        let expression = match &statement.kind {
            StatementKind::If(statement) => &statement.condition,
            StatementKind::Switch(statement) if statement.recovered_discriminant => return true,
            StatementKind::Switch(statement) => &statement.expression,
            _ => return false,
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

fn inline_body_supported(body: &[Statement]) -> bool {
    body.iter().all(|statement| {
        matches!(
            statement.kind,
            StatementKind::Variable(_)
                | StatementKind::Return(_)
                | StatementKind::Expression(_)
                | StatementKind::Empty
        )
    })
}

const fn statement_is_executable_region_member(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Import(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Empty => false,
        StatementKind::Export(declaration) => declaration.assignment.is_some(),
        _ => true,
    }
}
