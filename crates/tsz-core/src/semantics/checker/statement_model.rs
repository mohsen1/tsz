use crate::bind::ScopeId;
use crate::program::SemanticCompletion;
use crate::source::{FileId, Span};
use crate::syntax::{
    ArrowBody, ClassMemberKind, Expression, ExpressionKind, Parameter, Statement, StatementKind,
    SwitchClauseKind,
};

use super::Checker;
use super::projection_model::{PropertyOrderTree, peel_expression_parentheses};
use super::relation_diagnostic::{ContextualType, RelationDiagnosticStyle};
use crate::semantics::relation::RelationMode;

#[derive(Clone, Copy)]
enum FunctionLikeExpressionAction {
    BodyOnly,
    SemanticOwner,
    DeferredSemanticOwner,
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
            } else if self
                .capabilities
                .semantic_check_node_allows_function_like_expression_semantics(file, statement.id)
            {
                self.check_nonclaimed_function_like_descendants(
                    file,
                    scope,
                    statement,
                    FunctionLikeExpressionAction::DeferredSemanticOwner,
                );
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
        match &statement.kind {
            StatementKind::Import(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
            StatementKind::Export(declaration) => {
                if let Some(expression) = &declaration.assignment {
                    self.discover_expression_statement_descendants(file, scope, expression);
                }
            }
            StatementKind::Variable(declaration) => {
                if let Some(expression) = &declaration.initializer {
                    self.discover_expression_statement_descendants(file, scope, expression);
                }
            }
            StatementKind::Function(declaration) => {
                let function_scope = self.node_scope(file, statement.id, scope);
                self.discover_parameter_initializer_statement_descendants(
                    file,
                    function_scope,
                    &declaration.parameters,
                );
                self.check_statement_list(
                    file,
                    function_scope,
                    &declaration.body,
                    ContextualType::Deferred,
                    None,
                );
            }
            StatementKind::Class(declaration) => {
                let class_scope = self.node_scope(file, statement.id, scope);
                for member in &declaration.members {
                    match &member.kind {
                        ClassMemberKind::Constructor {
                            parameters, body, ..
                        }
                        | ClassMemberKind::Method {
                            parameters, body, ..
                        } => {
                            let member_scope = self.node_scope(file, member.id, class_scope);
                            self.discover_parameter_initializer_statement_descendants(
                                file,
                                member_scope,
                                parameters,
                            );
                            self.check_statement_list(
                                file,
                                member_scope,
                                body,
                                ContextualType::Deferred,
                                None,
                            );
                        }
                        ClassMemberKind::Property { initializer, .. } => {
                            if let Some(initializer) = initializer {
                                self.discover_expression_statement_descendants(
                                    file,
                                    class_scope,
                                    initializer,
                                );
                            }
                        }
                    }
                }
            }
            StatementKind::If(control_flow) => {
                self.discover_expression_statement_descendants(
                    file,
                    scope,
                    &control_flow.condition,
                );
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
                self.discover_expression_statement_descendants(
                    file,
                    switch_scope,
                    &control_flow.expression,
                );
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.discover_expression_statement_descendants(
                            file,
                            switch_scope,
                            expression,
                        );
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
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.discover_expression_statement_descendants(file, scope, expression);
                }
            }
            StatementKind::Block(statements) => self.check_statement_list(
                file,
                scope,
                statements,
                expected_return,
                expected_return_order,
            ),
            StatementKind::Expression(expression) => {
                self.discover_expression_statement_descendants(file, scope, expression);
            }
        }
    }

    pub(super) fn check_parameter_initializer_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
    ) {
        self.check_parameter_initializer_statement_descendants_with_action(
            file,
            scope,
            parameters,
            FunctionLikeExpressionAction::SemanticOwner,
        );
    }

    fn discover_parameter_initializer_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
    ) {
        self.check_parameter_initializer_statement_descendants_with_action(
            file,
            scope,
            parameters,
            FunctionLikeExpressionAction::BodyOnly,
        );
    }

    fn check_parameter_initializer_statement_descendants_with_action(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        action: FunctionLikeExpressionAction,
    ) {
        for parameter in parameters {
            if let Some(initializer) = &parameter.initializer {
                self.check_expression_statement_descendants_with_action(
                    file,
                    scope,
                    initializer,
                    action,
                );
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
        action: FunctionLikeExpressionAction,
    ) {
        match &statement.kind {
            StatementKind::Import(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
            StatementKind::Export(declaration) => {
                if let Some(expression) = &declaration.assignment {
                    self.check_expression_statement_descendants_with_action(
                        file, scope, expression, action,
                    );
                }
            }
            StatementKind::Variable(declaration) => {
                if let Some(initializer) = &declaration.initializer {
                    self.check_expression_statement_descendants_with_action(
                        file,
                        scope,
                        initializer,
                        action,
                    );
                }
            }
            StatementKind::Function(declaration) => {
                let function_scope = self.node_scope(file, statement.id, scope);
                self.check_parameter_initializer_statement_descendants_with_action(
                    file,
                    function_scope,
                    &declaration.parameters,
                    action,
                );
                self.check_statement_list(
                    file,
                    function_scope,
                    &declaration.body,
                    ContextualType::Deferred,
                    None,
                );
            }
            StatementKind::Class(declaration) => {
                let class_scope = self.node_scope(file, statement.id, scope);
                for member in &declaration.members {
                    match &member.kind {
                        ClassMemberKind::Constructor {
                            parameters, body, ..
                        }
                        | ClassMemberKind::Method {
                            parameters, body, ..
                        } => {
                            let member_scope = self.node_scope(file, member.id, class_scope);
                            self.check_parameter_initializer_statement_descendants_with_action(
                                file,
                                member_scope,
                                parameters,
                                action,
                            );
                            self.check_statement_list(
                                file,
                                member_scope,
                                body,
                                ContextualType::Deferred,
                                None,
                            );
                        }
                        ClassMemberKind::Property { initializer, .. } => {
                            if let Some(initializer) = initializer {
                                self.check_expression_statement_descendants_with_action(
                                    file,
                                    class_scope,
                                    initializer,
                                    action,
                                );
                            }
                        }
                    }
                }
            }
            StatementKind::If(control_flow) => {
                self.check_expression_statement_descendants_with_action(
                    file,
                    scope,
                    &control_flow.condition,
                    action,
                );
                self.check_nested_function_like_statement(
                    file,
                    scope,
                    &control_flow.then_statement,
                    action,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    self.check_nested_function_like_statement(file, scope, else_statement, action);
                }
            }
            StatementKind::Switch(control_flow) => {
                self.check_expression_statement_descendants_with_action(
                    file,
                    scope,
                    &control_flow.expression,
                    action,
                );
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.check_expression_statement_descendants_with_action(
                            file, scope, expression, action,
                        );
                    }
                    for statement in &clause.statements {
                        self.check_nested_function_like_statement(file, scope, statement, action);
                    }
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.check_expression_statement_descendants_with_action(
                        file, scope, expression, action,
                    );
                }
            }
            StatementKind::Block(statements) => {
                for statement in statements {
                    self.check_nested_function_like_statement(file, scope, statement, action);
                }
            }
            StatementKind::Expression(expression) => {
                self.check_expression_statement_descendants_with_action(
                    file, scope, expression, action,
                );
            }
        }
    }

    fn check_nested_function_like_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        action: FunctionLikeExpressionAction,
    ) {
        match &statement.kind {
            StatementKind::Function(_) | StatementKind::Class(_) => {
                let statement_scope = self.node_scope(file, statement.id, scope);
                self.check_statement(
                    file,
                    statement_scope,
                    statement,
                    ContextualType::Deferred,
                    None,
                );
            }
            _ => self.check_nonclaimed_function_like_descendants(file, scope, statement, action),
        }
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
                                self.check_expression_statement_descendants_with_action(
                                    file,
                                    member_scope,
                                    initializer,
                                    FunctionLikeExpressionAction::SemanticOwner,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn discover_expression_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) {
        self.check_expression_statement_descendants_with_action(
            file,
            scope,
            expression,
            FunctionLikeExpressionAction::BodyOnly,
        );
    }

    fn check_expression_statement_descendants_with_action(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        action: FunctionLikeExpressionAction,
    ) {
        match &expression.kind {
            ExpressionKind::Identifier { .. } => {
                if matches!(
                    action,
                    FunctionLikeExpressionAction::BodyOnly
                        | FunctionLikeExpressionAction::DeferredSemanticOwner
                ) {
                    let _ = self.infer_identifier(file, scope, expression);
                }
            }
            ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.check_expression_statement_descendants_with_action(
                        file,
                        scope,
                        &property.value,
                        action,
                    );
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.check_expression_statement_descendants_with_action(
                        file, scope, element, action,
                    );
                }
            }
            ExpressionKind::Call {
                callee, arguments, ..
            }
            | ExpressionKind::New {
                callee, arguments, ..
            } => {
                self.check_expression_statement_descendants_with_action(
                    file, scope, callee, action,
                );
                for argument in arguments {
                    self.check_expression_statement_descendants_with_action(
                        file, scope, argument, action,
                    );
                }
            }
            ExpressionKind::Member { object, .. }
            | ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::Parenthesized(object)
            | ExpressionKind::As {
                expression: object, ..
            } => {
                self.check_expression_statement_descendants_with_action(file, scope, object, action)
            }
            ExpressionKind::ElementAccess { object, index } => {
                self.check_expression_statement_descendants_with_action(
                    file, scope, object, action,
                );
                self.check_expression_statement_descendants_with_action(file, scope, index, action);
            }
            ExpressionKind::Arrow {
                parameters,
                return_type,
                body,
            } => match action {
                FunctionLikeExpressionAction::SemanticOwner
                | FunctionLikeExpressionAction::DeferredSemanticOwner => {
                    let context =
                        if matches!(action, FunctionLikeExpressionAction::DeferredSemanticOwner) {
                            ContextualType::Deferred
                        } else {
                            ContextualType::Absent
                        };
                    let _ = self.infer_arrow_expression(
                        file,
                        scope,
                        expression.id,
                        parameters,
                        return_type.as_ref(),
                        body,
                        context,
                    );
                }
                FunctionLikeExpressionAction::BodyOnly => {
                    let arrow_scope = self.node_scope(file, expression.id, scope);
                    self.check_parameter_initializer_statement_descendants_with_action(
                        file,
                        arrow_scope,
                        parameters,
                        action,
                    );
                    match body {
                        ArrowBody::Expression(expression) => {
                            self.check_expression_statement_descendants_with_action(
                                file,
                                arrow_scope,
                                expression,
                                action,
                            );
                        }
                        ArrowBody::Block(statements) => self.check_statement_list(
                            file,
                            arrow_scope,
                            statements,
                            ContextualType::Deferred,
                            None,
                        ),
                    }
                }
            },
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right } => {
                self.check_expression_statement_descendants_with_action(file, scope, left, action);
                self.check_expression_statement_descendants_with_action(file, scope, right, action);
            }
        }
    }
}

fn is_lexical_this_call_host(expression: &Expression) -> bool {
    let ExpressionKind::Call { callee, .. } = &peel_expression_parentheses(expression).kind else {
        return false;
    };
    let ExpressionKind::Member { object, .. } = &peel_expression_parentheses(callee).kind else {
        return false;
    };
    matches!(object.kind, ExpressionKind::This)
}
