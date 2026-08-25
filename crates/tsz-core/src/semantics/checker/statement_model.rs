use crate::bind::ScopeId;
use crate::program::SemanticCompletion;
use crate::source::{FileId, Span};
use crate::syntax::{
    ClassMemberKind, DescendantAdapter, DescendantContainer, Expression, ExpressionKind,
    FunctionLikeExpression, NestedStatement, Parameter, Statement, StatementKind, SwitchClauseKind,
    walk_expression_descendants, walk_function_like_descendants, walk_statement_descendants,
};

use super::Checker;
use super::projection_model::PropertyOrderTree;
use super::relation_diagnostic::{ContextualType, RelationDiagnosticStyle};
use crate::semantics::relation::RelationMode;

#[derive(Clone, Copy)]
enum FunctionLikeExpressionAction {
    BodyOnly,
    SemanticOwner,
    DeferredSemanticOwner,
}

#[derive(Clone, Copy)]
struct SemanticDescendantContext<'a> {
    scope: ScopeId,
    expected_return: ContextualType,
    expected_return_order: Option<&'a PropertyOrderTree>,
    reenter_all_statements: bool,
}

impl SemanticDescendantContext<'_> {
    const fn deferred(scope: ScopeId) -> Self {
        Self {
            scope,
            expected_return: ContextualType::Deferred,
            expected_return_order: None,
            reenter_all_statements: false,
        }
    }
}

#[derive(Clone, Copy)]
enum NestedSemanticAction {
    Recovery,
    FunctionLikeOwners,
}

struct SemanticDescendantAdapter<'checker, 'program, 'order> {
    checker: &'checker mut Checker<'program>,
    file: FileId,
    function_action: FunctionLikeExpressionAction,
    nested_action: NestedSemanticAction,
    allow_identifier_semantics: bool,
    _order: std::marker::PhantomData<&'order PropertyOrderTree>,
}

impl Checker<'_> {
    pub(super) fn check_statement_list(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statements: &[Statement],
        expected_return: ContextualType,
        expected_return_order: Option<&PropertyOrderTree>,
    ) {
        for statement in statements {
            let statement_scope = self.node_scope(file, statement.id, scope);
            self.check_statement(
                file,
                statement_scope,
                statement,
                expected_return,
                expected_return_order,
            );
        }
    }

    fn check_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        expected_return: ContextualType,
        expected_return_order: Option<&PropertyOrderTree>,
    ) {
        if !self
            .capabilities
            .semantic_check_node_is_claimed(file, statement.id)
        {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            if self
                .capabilities
                .semantic_check_node_allows_claimed_descendants(file, statement.id)
            {
                self.check_recovery_statement_descendants(
                    file,
                    scope,
                    statement,
                    expected_return,
                    expected_return_order,
                );
            } else {
                let (allows_descendants, allow_identifier_semantics) = self
                    .capabilities
                    .semantic_check_node_function_like_descendant_permissions(file, statement.id);
                if allows_descendants {
                    self.check_nonclaimed_function_like_descendants(
                        file,
                        scope,
                        statement,
                        allow_identifier_semantics,
                    );
                }
            }
            return;
        }

        match &statement.kind {
            StatementKind::Export(declaration) => {
                if let Some(expression) = &declaration.assignment {
                    self.infer_expression(file, scope, expression, None);
                }
            }
            StatementKind::Class(declaration) => {
                let class_scope = self.node_scope(file, statement.id, scope);
                self.check_class(file, class_scope, declaration);
                self.check_class_initializer_statement_descendants(file, class_scope, declaration);
            }
            StatementKind::Variable(declaration) => {
                self.check_variable(file, scope, statement.id, declaration);
            }
            StatementKind::Function(declaration) => {
                let function_scope = self.node_scope(file, statement.id, scope);
                self.check_parameter_initializer_statement_descendants(
                    file,
                    function_scope,
                    &declaration.parameters,
                );
                self.check_function(file, statement.id, declaration);
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    let actual =
                        self.infer_expression_contextual(file, scope, expression, expected_return);
                    if let ContextualType::Known(expected) = expected_return {
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
            StatementKind::Block(statements) => self.check_statement_list(
                file,
                scope,
                statements,
                expected_return,
                expected_return_order,
            ),
            StatementKind::If(control_flow) => {
                self.infer_expression(file, scope, &control_flow.condition, None);
                let then_scope = self.node_scope(file, control_flow.then_statement.id, scope);
                self.check_statement(
                    file,
                    then_scope,
                    &control_flow.then_statement,
                    expected_return,
                    expected_return_order,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = self.node_scope(file, else_statement.id, scope);
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
                let switch_scope = self.node_scope(file, statement.id, scope);
                self.infer_expression(file, switch_scope, &control_flow.expression, None);
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.infer_expression(file, switch_scope, expression, None);
                    }
                    self.check_statement_list(
                        file,
                        switch_scope,
                        &clause.statements,
                        expected_return,
                        expected_return_order,
                    );
                }
            }
            StatementKind::Expression(expression) => {
                self.infer_expression_statement(file, scope, expression);
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

    fn check_recovery_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        expected_return: ContextualType,
        expected_return_order: Option<&PropertyOrderTree>,
    ) {
        let context = SemanticDescendantContext {
            scope,
            expected_return,
            expected_return_order,
            reenter_all_statements: false,
        };
        let allow_identifier_semantics = self
            .capabilities
            .semantic_check_node_allows_recovery_identifiers(file, statement.id);
        let mut adapter = SemanticDescendantAdapter {
            checker: self,
            file,
            function_action: FunctionLikeExpressionAction::SemanticOwner,
            nested_action: NestedSemanticAction::Recovery,
            allow_identifier_semantics,
            _order: std::marker::PhantomData,
        };
        walk_statement_descendants(&mut adapter, &context, statement);
    }

    pub(super) fn check_parameter_initializer_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
    ) {
        let context = SemanticDescendantContext::deferred(scope);
        let mut adapter = SemanticDescendantAdapter {
            checker: self,
            file,
            function_action: FunctionLikeExpressionAction::SemanticOwner,
            nested_action: NestedSemanticAction::FunctionLikeOwners,
            allow_identifier_semantics: false,
            _order: std::marker::PhantomData,
        };
        for parameter in parameters {
            if let Some(initializer) = &parameter.initializer {
                walk_expression_descendants(&mut adapter, &context, initializer);
            }
        }
    }

    /// A nonclaimed flow-region statement owns no semantic expression or
    /// relation work. Function-like expressions and declarations nested in
    /// that statement are separate execution owners, however, so discover
    /// their statement bodies without checking the withheld host nodes on the
    /// path to them.
    fn check_nonclaimed_function_like_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        allow_identifier_semantics: bool,
    ) {
        let context = SemanticDescendantContext::deferred(scope);
        let mut adapter = SemanticDescendantAdapter {
            checker: self,
            file,
            function_action: FunctionLikeExpressionAction::DeferredSemanticOwner,
            nested_action: NestedSemanticAction::FunctionLikeOwners,
            allow_identifier_semantics,
            _order: std::marker::PhantomData,
        };
        walk_statement_descendants(&mut adapter, &context, statement);
    }

    fn check_class_initializer_statement_descendants(
        &mut self,
        file: FileId,
        class_scope: ScopeId,
        declaration: &crate::syntax::ClassDeclaration,
    ) {
        for member in &declaration.members {
            match &member.kind {
                ClassMemberKind::Constructor { parameters, .. }
                | ClassMemberKind::Method { parameters, .. } => {
                    let member_scope = self.node_scope(file, member.id, class_scope);
                    self.check_parameter_initializer_statement_descendants(
                        file,
                        member_scope,
                        parameters,
                    );
                }
                ClassMemberKind::Property {
                    annotation,
                    initializer,
                    ..
                } => {
                    if let Some(initializer) = initializer {
                        let member_scope = self.node_scope(file, member.id, class_scope);
                        if let Some(annotation) = annotation {
                            let expected = if declaration.type_parameters.is_empty() {
                                let expected = self.resolve_type_node(
                                    file,
                                    member_scope,
                                    annotation,
                                    &std::collections::HashMap::new(),
                                );
                                ContextualType::Known(expected)
                            } else {
                                let _ = self.require_file_completion(
                                    file,
                                    crate::semantics::types::Completion::<()>::Deferred,
                                );
                                ContextualType::Deferred
                            };
                            self.infer_expression_contextual(
                                file,
                                member_scope,
                                initializer,
                                expected,
                            );
                        } else {
                            if is_lexical_this_call_host(initializer) {
                                self.infer_expression(file, member_scope, initializer, None);
                            } else {
                                self.check_function_like_expression_descendants(
                                    file,
                                    member_scope,
                                    initializer,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_function_like_expression_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) {
        let context = SemanticDescendantContext::deferred(scope);
        let mut adapter = SemanticDescendantAdapter {
            checker: self,
            file,
            function_action: FunctionLikeExpressionAction::SemanticOwner,
            nested_action: NestedSemanticAction::FunctionLikeOwners,
            allow_identifier_semantics: false,
            _order: std::marker::PhantomData,
        };
        walk_expression_descendants(&mut adapter, &context, expression);
    }

    pub(super) fn check_function_like_expression_body_only(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        function: &FunctionLikeExpression,
    ) {
        let context = SemanticDescendantContext::deferred(scope);
        let (allows_descendants, allow_identifier_semantics) = self
            .capabilities
            .semantic_check_node_function_like_descendant_permissions(file, expression.id);
        let function_action = if allows_descendants {
            FunctionLikeExpressionAction::SemanticOwner
        } else {
            FunctionLikeExpressionAction::BodyOnly
        };
        let mut adapter = SemanticDescendantAdapter {
            checker: self,
            file,
            function_action,
            nested_action: NestedSemanticAction::Recovery,
            allow_identifier_semantics,
            _order: std::marker::PhantomData,
        };
        walk_function_like_descendants(&mut adapter, &context, expression, function);
    }
}

impl<'ast, 'checker, 'program, 'order> DescendantAdapter<'ast>
    for SemanticDescendantAdapter<'checker, 'program, 'order>
{
    type Context = SemanticDescendantContext<'order>;

    fn context(
        &mut self,
        context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context {
        let (owner, resets_return, reenter_all) = match container {
            DescendantContainer::Statement(statement)
            | DescendantContainer::Class(statement, _) => (statement.id, false, false),
            DescendantContainer::Function(statement, _) => (statement.id, true, true),
            DescendantContainer::ClassMember(member) => (member.id, true, true),
            DescendantContainer::FunctionLike(expression, _) => (expression.id, true, true),
        };
        let mut next = SemanticDescendantContext {
            scope: self.checker.node_scope(self.file, owner, context.scope),
            ..*context
        };
        if resets_return {
            next.expected_return = ContextualType::Deferred;
            next.expected_return_order = None;
        }
        next.reenter_all_statements |= reenter_all;
        next
    }

    fn nested_statement(
        &mut self,
        context: &Self::Context,
        statement: &'ast Statement,
        _next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        match self.nested_action {
            NestedSemanticAction::Recovery => {
                self.checker.check_statement(
                    self.file,
                    context.scope,
                    statement,
                    context.expected_return,
                    context.expected_return_order,
                );
                NestedStatement::Handled
            }
            NestedSemanticAction::FunctionLikeOwners
                if context.reenter_all_statements
                    || matches!(
                        statement.kind,
                        StatementKind::Function(_) | StatementKind::Class(_)
                    ) =>
            {
                self.checker.check_statement(
                    self.file,
                    context.scope,
                    statement,
                    ContextualType::Deferred,
                    None,
                );
                NestedStatement::Handled
            }
            NestedSemanticAction::FunctionLikeOwners => NestedStatement::Descend,
        }
    }

    fn function_like(
        &mut self,
        context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        match self.function_action {
            FunctionLikeExpressionAction::BodyOnly => {
                walk_function_like_descendants(self, context, expression, function);
            }
            FunctionLikeExpressionAction::SemanticOwner
                if matches!(
                    &function.syntax,
                    crate::syntax::FunctionLikeSyntax::Arrow(_)
                ) && matches!(self.nested_action, NestedSemanticAction::Recovery) =>
            {
                walk_function_like_descendants(self, context, expression, function);
            }
            FunctionLikeExpressionAction::SemanticOwner
            | FunctionLikeExpressionAction::DeferredSemanticOwner => {
                let expected = if matches!(
                    self.function_action,
                    FunctionLikeExpressionAction::DeferredSemanticOwner
                ) {
                    ContextualType::Deferred
                } else {
                    ContextualType::Absent
                };
                let _ = self.checker.infer_function_like_expression(
                    self.file,
                    context.scope,
                    expression,
                    function,
                    expected,
                );
            }
        }
    }

    fn identifier(&mut self, context: &Self::Context, expression: &'ast Expression) {
        if self.allow_identifier_semantics {
            let _ = self
                .checker
                .infer_identifier(self.file, context.scope, expression);
        }
    }
}

fn is_lexical_this_call_host(expression: &Expression) -> bool {
    let ExpressionKind::Call { callee, .. } = &expression.peel_parentheses().kind else {
        return false;
    };
    let ExpressionKind::Member { object, .. } = &callee.peel_parentheses().kind else {
        return false;
    };
    matches!(object.kind, ExpressionKind::This)
}
