use std::collections::HashMap;
use std::rc::Rc;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::program::{CapabilityScope, CapabilityTarget};
use crate::semantics::types::{Completion, TypeId};
use crate::source::FileId;
use crate::syntax::{
    ArrowBody, ClassMemberKind, DescendantAdapter, DescendantContainer, Expression,
    FunctionLikeExpression, FunctionLikeSyntax, NestedStatement, Statement, TypeNode, TypeNodeKind,
    walk_function_like_descendants, walk_statement_descendants,
};

use super::super::Checker;
use super::{ParameterGrammarHost, synthetic_identity};

#[derive(Clone)]
struct RequiredDescendantContext {
    scope: ScopeId,
    type_parameters: Rc<HashMap<String, TypeId>>,
    reenter_function_like_required_type: bool,
}

struct RequiredDescendantAdapter<'checker, 'program> {
    checker: &'checker mut Checker<'program>,
    file: FileId,
}

impl Checker<'_> {
    pub(super) fn visit_required_function_like_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        function: &FunctionLikeExpression,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        if !self
            .capabilities
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file, expression.id),
            )
            .is_claimed()
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
            self.visit_nonclaimed_required_function_like_descendants(
                file,
                scope,
                expression,
                function,
                type_parameters,
            );
            return;
        }
        let function_scope = self.node_scope(file, expression.id, scope);
        self.visit_required_parameters(
            file,
            function_scope,
            &function.parameters,
            type_parameters,
            ParameterGrammarHost::Implementation { constructor: false },
        );
        if let Some(return_type) = &function.return_type {
            if return_type.contains_type_query() {
                let _ = self.require_completion(Completion::<()>::Deferred);
            } else {
                self.visit_required_type_node(file, function_scope, return_type, type_parameters);
            }
        }
        match &function.syntax {
            FunctionLikeSyntax::Arrow(ArrowBody::Expression(body)) => {
                self.visit_required_expression(file, function_scope, body, type_parameters);
            }
            FunctionLikeSyntax::Arrow(ArrowBody::Block(statements))
            | FunctionLikeSyntax::Function {
                body: statements, ..
            } => self.visit_required_body(file, function_scope, statements, type_parameters),
        }
    }

    pub(super) fn visit_nonclaimed_required_function_like_descendants(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        function: &FunctionLikeExpression,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let context = RequiredDescendantContext {
            scope,
            type_parameters: Rc::new(type_parameters.clone()),
            reenter_function_like_required_type: self
                .capabilities
                .required_type_node_allows_function_like_reentry(file, expression.id),
        };
        let mut adapter = RequiredDescendantAdapter {
            checker: self,
            file,
        };
        walk_function_like_descendants(&mut adapter, &context, expression, function);
    }

    pub(super) fn visit_required_body(
        &mut self,
        file: FileId,
        scope: ScopeId,
        body: &[Statement],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        for (index, statement) in body.iter().enumerate() {
            let statement_scope = self.node_scope(file, statement.id, scope);
            self.visit_required_statement(
                file,
                statement_scope,
                statement,
                body.get(index + 1),
                type_parameters,
            );
        }
    }

    pub(super) fn visit_required_class_heritage(
        &mut self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let TypeNodeKind::Reference { name, .. } = &node.kind else {
            self.visit_required_type_node(file, scope, node, type_parameters);
            return;
        };
        let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Value) else {
            self.visit_required_type_node(file, scope, node, type_parameters);
            return;
        };
        let value = self.declaration_value_type(declaration);
        let Completion::Complete(value) = self.require_completion(value) else {
            return;
        };
        if !matches!(
            self.store.kind(value),
            crate::semantics::types::TypeKind::ClassConstructor { .. }
        ) {
            let _ = self.require_completion(Completion::<()>::Deferred);
            return;
        }
        self.visit_required_type_node(file, scope, node, type_parameters);
    }

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
        let context = RequiredDescendantContext {
            scope,
            type_parameters: Rc::new(type_parameters.clone()),
            reenter_function_like_required_type: self
                .capabilities
                .required_type_node_allows_function_like_reentry(file, statement.id),
        };
        let mut adapter = RequiredDescendantAdapter {
            checker: self,
            file,
        };
        walk_statement_descendants(&mut adapter, &context, statement);
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

impl<'ast, 'checker, 'program> DescendantAdapter<'ast>
    for RequiredDescendantAdapter<'checker, 'program>
{
    type Context = RequiredDescendantContext;

    fn context(
        &mut self,
        context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context {
        let (owner, type_parameters) = match container {
            DescendantContainer::Statement(statement) => {
                (statement.id, Rc::clone(&context.type_parameters))
            }
            DescendantContainer::Function(statement, declaration) => {
                let identity = self
                    .checker
                    .find_declaration(
                        self.file,
                        statement.id,
                        DeclarationKind::Function,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(self.file, declaration.name_span.start));
                (
                    statement.id,
                    Rc::new(self.checker.extend_type_parameters(
                        identity,
                        &declaration.type_parameters,
                        &context.type_parameters,
                    )),
                )
            }
            DescendantContainer::Class(statement, declaration) => {
                let identity = self
                    .checker
                    .find_declaration(
                        self.file,
                        statement.id,
                        DeclarationKind::Class,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(self.file, declaration.name_span.start));
                (
                    statement.id,
                    Rc::new(self.checker.extend_type_parameters(
                        identity,
                        &declaration.type_parameters,
                        &context.type_parameters,
                    )),
                )
            }
            DescendantContainer::ClassMember(member) => {
                let types = match &member.kind {
                    ClassMemberKind::Method {
                        type_parameters, ..
                    } => Rc::new(self.checker.extend_type_parameters(
                        super::synthetic_identity(self.file, member.name_span.start),
                        type_parameters,
                        &context.type_parameters,
                    )),
                    ClassMemberKind::Constructor { .. } | ClassMemberKind::Property { .. } => {
                        Rc::clone(&context.type_parameters)
                    }
                };
                (member.id, types)
            }
            DescendantContainer::FunctionLike(expression, function) => {
                let owner = expression.id;
                let identity = match &function.syntax {
                    FunctionLikeSyntax::Function { name, .. } => self.checker.find_declaration(
                        self.file,
                        owner,
                        DeclarationKind::FunctionExpression,
                        name.as_ref().map_or("", |name| name.name.as_str()),
                    ),
                    FunctionLikeSyntax::Arrow(_) => {
                        Some(super::synthetic_identity(self.file, expression.span.start))
                    }
                };
                let types = if function.type_parameters.is_empty() {
                    Rc::clone(&context.type_parameters)
                } else if let Some(identity) = identity {
                    Rc::new(self.checker.extend_type_parameters(
                        identity,
                        &function.type_parameters,
                        &context.type_parameters,
                    ))
                } else {
                    let _ = self.checker.require_completion(Completion::<()>::Deferred);
                    Rc::clone(&context.type_parameters)
                };
                (owner, types)
            }
        };
        RequiredDescendantContext {
            scope: self.checker.node_scope(self.file, owner, context.scope),
            type_parameters,
            reenter_function_like_required_type: context.reenter_function_like_required_type,
        }
    }

    fn nested_statement(
        &mut self,
        context: &Self::Context,
        statement: &'ast Statement,
        next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        self.checker.visit_required_statement(
            self.file,
            context.scope,
            statement,
            next_statement,
            &context.type_parameters,
        );
        NestedStatement::Handled
    }

    fn function_like(
        &mut self,
        context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        if context.reenter_function_like_required_type {
            self.checker.visit_required_expression(
                self.file,
                context.scope,
                expression,
                &context.type_parameters,
            );
        } else {
            walk_function_like_descendants(self, context, expression, function);
        }
    }
}
