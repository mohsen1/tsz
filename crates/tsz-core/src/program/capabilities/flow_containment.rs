use std::collections::BTreeSet;

use crate::source::{NodeId, Span};
use crate::syntax::{
    ClassMember, ClassMemberKind, Expression, ExpressionKind, Parameter, ParameterNameKind,
    ParserRecoveryFact, ParserRecoveryKind, Statement, StatementKind, SwitchClauseKind, TokenKind,
};

use super::{SemanticGap, SyntaxGap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FileBoundary {
    ClassProduct,
    CommonJsClass,
    Declaration,
    ClassProperty,
}

/// Computes immutable statement ownership for flow-sensitive type-of-reference
/// work that the checker does not own yet. A region begins at an unsupported
/// narrowing entry and covers the executable suffix of the same container.
/// Function-like bodies are inventoried as fresh containers.
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
    collector.visit_statement_list(statements, false);
    collector.out
}

struct FlowRegionCollector<'a> {
    recoveries: &'a [ParserRecoveryFact],
    javascript_jsdoc_casts: &'a BTreeSet<NodeId>,
    out: SemanticNodeInventory,
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
            self.out.flow_regions.insert(statement.id);
        }
        let owners = (statement.id, statement.id);

        match &statement.kind {
            StatementKind::Import(declaration) => {
                if declaration.type_only
                    || declaration.bindings.iter().any(|binding| binding.type_only)
                {
                    self.out.boundaries.insert(FileBoundary::Declaration);
                }
                local_active
            }
            StatementKind::TypeAlias(_) | StatementKind::Interface(_) => {
                self.out.boundaries.insert(FileBoundary::Declaration);
                local_active
            }
            StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => local_active,
            StatementKind::Export(declaration) => {
                if declaration.type_only
                    || declaration
                        .specifiers
                        .iter()
                        .any(|specifier| specifier.type_only)
                {
                    self.out.boundaries.insert(FileBoundary::Declaration);
                }
                if let Some(expression) = &declaration.assignment {
                    self.visit_expression(expression, owners);
                }
                local_active
            }
            StatementKind::Variable(declaration) => {
                if declaration.has_leading_jsdoc {
                    self.out.javascript_jsdoc_values.insert(statement.id);
                }
                if let Some(initializer) = &declaration.initializer {
                    self.visit_expression(initializer, owners);
                }
                local_active
            }
            StatementKind::Function(declaration) => {
                if declaration
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name_kind == ParameterNameKind::This)
                {
                    self.out
                        .function_like_gaps
                        .push((statement.id, SemanticGap::ExplicitThisParameter));
                }
                if declaration.has_leading_jsdoc {
                    self.out
                        .function_like_gaps
                        .push((statement.id, SemanticGap::JavaScriptJSDocSignature));
                }
                self.visit_parameter_initializers(&declaration.parameters, owners);
                let recovered = self.record_recovery(
                    statement.id,
                    statement.span,
                    first_statement_start(&declaration.body, statement.span.end),
                );
                self.visit_statement_list(&declaration.body, recovered);
                local_active
            }
            StatementKind::Class(declaration) => {
                if declaration.abstract_class {
                    self.out.boundaries.insert(FileBoundary::ClassProduct);
                }
                for member in &declaration.members {
                    self.visit_class_member(member);
                }
                local_active
            }
            StatementKind::If(if_statement) => {
                self.visit_expression(&if_statement.condition, owners);
                let then_active = self.visit_statement(&if_statement.then_statement, local_active);
                let else_active = if_statement
                    .else_statement
                    .as_deref()
                    .is_some_and(|statement| self.visit_statement(statement, local_active));
                local_active || then_active || else_active
            }
            StatementKind::Switch(switch_statement) => {
                self.visit_expression(&switch_statement.expression, owners);
                let mut clause_active = local_active;
                for clause in &switch_statement.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.visit_expression(expression, owners);
                    }
                    clause_active = self.visit_statement_list(&clause.statements, clause_active);
                }
                local_active || clause_active
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.visit_expression(expression, owners);
                }
                local_active
            }
            StatementKind::Block(statements) => self.visit_statement_list(statements, local_active),
            StatementKind::Expression(expression) => {
                self.visit_expression(expression, owners);
                local_active
            }
        }
    }

    fn visit_class_member(&mut self, member: &ClassMember) {
        if !member.emit_products_supported {
            self.out.boundaries.insert(FileBoundary::ClassProduct);
        }
        let owners = (member.id, member.id);
        match &member.kind {
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
            } => {
                if !has_body {
                    self.out.boundaries.insert(FileBoundary::CommonJsClass);
                }
                self.visit_parameter_initializers(parameters, owners);
                let recovered = self.record_recovery(
                    member.id,
                    member.span,
                    first_statement_start(body, member.span.end),
                );
                self.visit_statement_list(body, recovered);
            }
            ClassMemberKind::Property { initializer, .. } => {
                self.out.boundaries.insert(FileBoundary::ClassProperty);
                if let Some(initializer) = initializer {
                    self.visit_expression(initializer, owners);
                }
            }
        }
    }

    fn visit_parameter_initializers(&mut self, parameters: &[Parameter], owners: (NodeId, NodeId)) {
        for parameter in parameters {
            if let Some(initializer) = &parameter.initializer {
                self.visit_expression(initializer, owners);
            }
        }
    }

    fn visit_expression(&mut self, expression: &Expression, owners: (NodeId, NodeId)) {
        if self.javascript_jsdoc_casts.contains(&expression.id) {
            self.out.javascript_jsdoc_values.insert(owners.0);
            self.out.javascript_jsdoc_checks.insert(owners.1);
        }
        match &expression.kind {
            ExpressionKind::Identifier { .. }
            | ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.visit_expression(&property.value, owners);
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.visit_expression(element, owners);
                }
            }
            ExpressionKind::Call {
                callee, arguments, ..
            }
            | ExpressionKind::New {
                callee, arguments, ..
            } => {
                self.visit_expression(callee, owners);
                for argument in arguments {
                    self.visit_expression(argument, owners);
                }
            }
            ExpressionKind::Member { object, .. }
            | ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::Parenthesized(object) => self.visit_expression(object, owners),
            ExpressionKind::ElementAccess { object, index } => {
                self.visit_expression(object, owners);
                self.visit_expression(index, owners);
            }
            ExpressionKind::FunctionLike(function) => {
                self.out.function_likes.insert(expression.id);
                let owners = (expression.id, expression.id);
                if function.has_leading_jsdoc {
                    self.out
                        .function_like_gaps
                        .push((expression.id, SemanticGap::JavaScriptJSDocSignature));
                }
                let object_method = function.syntax.is_object_method();
                if object_method {
                    self.out.object_method_owners.insert(expression.id);
                }
                if let Some((_, body)) = function.syntax.function()
                    && let Some(body_span) = function.body_span
                {
                    let products = FunctionExpressionProducts {
                        owner: expression.id,
                        span: expression.span,
                        body_span,
                        inline_body_supported: inline_body_supported(body),
                    };
                    if object_method {
                        self.out.object_methods.push(products);
                    } else {
                        self.out.function_expressions.push(products);
                    }
                }
                if matches!(
                    function.syntax.function(),
                    Some((Some(name), _))
                        if name.token_kind != TokenKind::Identifier
                ) {
                    self.out
                        .function_like_gaps
                        .push((expression.id, SemanticGap::FunctionExpressionBindingName));
                }
                if !function.type_parameters.is_empty() {
                    self.out
                        .function_like_gaps
                        .push((expression.id, SemanticGap::FunctionLikeTypeParameters));
                }
                if function
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name_kind == ParameterNameKind::This)
                {
                    self.out
                        .function_like_gaps
                        .push((expression.id, SemanticGap::ExplicitThisParameter));
                }
                if function
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name_kind == ParameterNameKind::BindingPattern)
                {
                    self.out
                        .function_like_binding_patterns
                        .insert(expression.id);
                }
                let body_start = match function.syntax.body() {
                    crate::syntax::FunctionLikeBody::Expression(body) => body.span.start,
                    crate::syntax::FunctionLikeBody::Statements(body) => {
                        first_statement_start(body, expression.span.end)
                    }
                };
                self.out.function_like_signatures.push(Span {
                    file: expression.span.file,
                    start: expression.span.start,
                    end: body_start,
                });
                let recovered = self.record_recovery(expression.id, expression.span, body_start);
                self.visit_parameter_initializers(&function.parameters, owners);
                match function.syntax.body() {
                    crate::syntax::FunctionLikeBody::Expression(body) => {
                        self.visit_expression(body, owners)
                    }
                    crate::syntax::FunctionLikeBody::Statements(body) => {
                        self.visit_statement_list(body, recovered);
                    }
                }
            }
            ExpressionKind::Binary { left, right, .. } => {
                self.visit_expression(left, owners);
                self.visit_expression(right, owners);
            }
            ExpressionKind::Assignment {
                left,
                right,
                has_leading_jsdoc,
            } => {
                if *has_leading_jsdoc {
                    self.out.javascript_jsdoc_values.insert(expression.id);
                }
                self.visit_expression(left, owners);
                self.visit_expression(right, (expression.id, owners.1));
            }
            ExpressionKind::As { expression, .. } => self.visit_expression(expression, owners),
        }
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
