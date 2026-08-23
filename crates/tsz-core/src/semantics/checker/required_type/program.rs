use std::collections::HashMap;

use crate::bind::{DeclarationKind, ScopeId};
use crate::program::{CapabilityScope, CapabilityTarget};
use crate::semantics::types::{Completion, TypeId};
use crate::source::FileId;
use crate::syntax::{
    ArrowBody, ClassMemberKind, Expression, ExpressionKind, Statement, StatementKind,
    SwitchClauseKind,
};

use super::super::Checker;

impl Checker<'_> {
    /// Walk every parsed explicit type position before ordinary checking.
    pub(in crate::semantics::checker) fn require_explicit_type_positions(&mut self) {
        let empty = HashMap::new();
        for file_id in &self.program.source_order {
            let file_id = *file_id;
            if !self.capabilities.semantic_check_file_is_enabled(file_id) {
                continue;
            }
            let statements = &self.program.files[file_id.0 as usize].syntax.statements;
            for (index, statement) in statements.iter().enumerate() {
                self.completion.set_current(Some(file_id));
                self.visit_required_statement(
                    file_id,
                    ScopeId(0),
                    statement,
                    statements.get(index + 1),
                    &empty,
                );
            }
        }
        self.completion.set_current(None);
    }

    pub(super) fn visit_required_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        next_statement: Option<&Statement>,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let required_type_claimed = self
            .capabilities
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file, statement.id),
            )
            .is_claimed();
        if !required_type_claimed {
            let _ = self.require_completion(Completion::<()>::Deferred);
            self.visit_nonclaimed_required_statement_descendants(
                file,
                scope,
                statement,
                type_parameters,
            );
            return;
        }
        self.visit_required_statement_claimed(
            file,
            scope,
            statement,
            next_statement,
            type_parameters,
        );
    }

    fn visit_nonclaimed_required_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        type_parameters: &HashMap<String, TypeId>,
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
                    self.visit_required_expression_statement_descendants(
                        file,
                        scope,
                        expression,
                        type_parameters,
                    );
                }
            }
            StatementKind::Variable(declaration) => {
                if let Some(expression) = &declaration.initializer {
                    self.visit_required_expression_statement_descendants(
                        file,
                        scope,
                        expression,
                        type_parameters,
                    );
                }
            }
            StatementKind::Function(declaration) => {
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Function,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| {
                        super::synthetic_identity(file, declaration.name_span.start)
                    });
                let function_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                let function_scope = self.node_scope(file, statement.id, scope);
                for parameter in &declaration.parameters {
                    if let Some(initializer) = &parameter.initializer {
                        self.visit_required_expression_statement_descendants(
                            file,
                            function_scope,
                            initializer,
                            &function_types,
                        );
                    }
                }
                self.visit_required_body(file, function_scope, &declaration.body, &function_types);
            }
            StatementKind::Class(declaration) => {
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Class,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| {
                        super::synthetic_identity(file, declaration.name_span.start)
                    });
                let class_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                let class_scope = self.node_scope(file, statement.id, scope);
                for member in &declaration.members {
                    match &member.kind {
                        ClassMemberKind::Constructor {
                            parameters, body, ..
                        } => {
                            let member_scope = self.node_scope(file, member.id, class_scope);
                            for parameter in parameters {
                                if let Some(initializer) = &parameter.initializer {
                                    self.visit_required_expression_statement_descendants(
                                        file,
                                        member_scope,
                                        initializer,
                                        &class_types,
                                    );
                                }
                            }
                            self.visit_required_body(file, member_scope, body, &class_types);
                        }
                        ClassMemberKind::Property { initializer, .. } => {
                            if let Some(initializer) = initializer {
                                self.visit_required_expression_statement_descendants(
                                    file,
                                    class_scope,
                                    initializer,
                                    &class_types,
                                );
                            }
                        }
                        ClassMemberKind::Method {
                            type_parameters: declarations,
                            parameters,
                            body,
                            ..
                        } => {
                            let member_scope = self.node_scope(file, member.id, class_scope);
                            let method_types = self.extend_type_parameters(
                                super::synthetic_identity(file, member.name_span.start),
                                declarations,
                                &class_types,
                            );
                            for parameter in parameters {
                                if let Some(initializer) = &parameter.initializer {
                                    self.visit_required_expression_statement_descendants(
                                        file,
                                        member_scope,
                                        initializer,
                                        &method_types,
                                    );
                                }
                            }
                            self.visit_required_body(file, member_scope, body, &method_types);
                        }
                    }
                }
            }
            StatementKind::If(control_flow) => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    &control_flow.condition,
                    type_parameters,
                );
                self.visit_required_body(
                    file,
                    scope,
                    std::slice::from_ref(control_flow.then_statement.as_ref()),
                    type_parameters,
                );
                if let Some(statement) = &control_flow.else_statement {
                    self.visit_required_body(
                        file,
                        scope,
                        std::slice::from_ref(statement.as_ref()),
                        type_parameters,
                    );
                }
            }
            StatementKind::Switch(control_flow) => {
                let switch_scope = self.node_scope(file, statement.id, scope);
                self.visit_required_expression_statement_descendants(
                    file,
                    switch_scope,
                    &control_flow.expression,
                    type_parameters,
                );
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.visit_required_expression_statement_descendants(
                            file,
                            switch_scope,
                            expression,
                            type_parameters,
                        );
                    }
                    self.visit_required_body(
                        file,
                        switch_scope,
                        &clause.statements,
                        type_parameters,
                    );
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.visit_required_expression_statement_descendants(
                        file,
                        scope,
                        expression,
                        type_parameters,
                    );
                }
            }
            StatementKind::Block(statements) => {
                self.visit_required_body(file, scope, statements, type_parameters);
            }
            StatementKind::Expression(expression) => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    expression,
                    type_parameters,
                );
            }
        }
    }

    pub(super) fn visit_required_expression_statement_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        match &expression.kind {
            ExpressionKind::Identifier { .. }
            | ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.visit_required_expression_statement_descendants(
                        file,
                        scope,
                        &property.value,
                        type_parameters,
                    );
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.visit_required_expression_statement_descendants(
                        file,
                        scope,
                        element,
                        type_parameters,
                    );
                }
            }
            ExpressionKind::Call {
                callee, arguments, ..
            }
            | ExpressionKind::New {
                callee, arguments, ..
            } => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    callee,
                    type_parameters,
                );
                for argument in arguments {
                    self.visit_required_expression_statement_descendants(
                        file,
                        scope,
                        argument,
                        type_parameters,
                    );
                }
            }
            ExpressionKind::Member { object, .. }
            | ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::Parenthesized(object) => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    object,
                    type_parameters,
                );
            }
            ExpressionKind::ElementAccess { object, index } => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    object,
                    type_parameters,
                );
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    index,
                    type_parameters,
                );
            }
            ExpressionKind::Arrow {
                parameters, body, ..
            } => {
                let arrow_scope = self.node_scope(file, expression.id, scope);
                for parameter in parameters {
                    if let Some(initializer) = &parameter.initializer {
                        self.visit_required_expression_statement_descendants(
                            file,
                            arrow_scope,
                            initializer,
                            type_parameters,
                        );
                    }
                }
                match body {
                    ArrowBody::Expression(expression) => {
                        self.visit_required_expression_statement_descendants(
                            file,
                            arrow_scope,
                            expression,
                            type_parameters,
                        );
                    }
                    ArrowBody::Block(statements) => {
                        self.visit_required_body(file, arrow_scope, statements, type_parameters)
                    }
                }
            }
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right } => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    left,
                    type_parameters,
                );
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    right,
                    type_parameters,
                );
            }
            ExpressionKind::As { expression, .. } => {
                self.visit_required_expression_statement_descendants(
                    file,
                    scope,
                    expression,
                    type_parameters,
                );
            }
        }
    }

    pub(super) fn required_declaration_model_is_claimed(
        &mut self,
        declaration: crate::source::DeclId,
    ) -> bool {
        let claimed = self.semantic_declaration_is_claimed(declaration);
        if !claimed {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
        claimed
    }
}
