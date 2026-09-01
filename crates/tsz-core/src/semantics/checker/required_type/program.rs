use std::collections::HashMap;
use std::rc::Rc;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::program::{CapabilityScope, CapabilityTarget};
use crate::semantics::types::{Completion, TypeId};
use crate::source::FileId;
use crate::syntax::{
    ClassMemberKind, DescendantAdapter, DescendantContainer, Expression, FunctionLikeExpression,
    NestedStatement, ParameterNameKind, Statement, TypeNode, TypeNodeKind,
    TypeParameterDeclaration, walk_expression_descendants, walk_function_like_descendants,
    walk_statement_descendants,
};

use super::super::{Checker, declaration_value::ValueQueryState};
use super::{ParameterGrammarHost, synthetic_identity};

#[derive(Clone)]
struct RequiredDescendantContext {
    scope: ScopeId,
    type_parameters: Rc<HashMap<String, TypeId>>,
    traversal: RequiredTraversal,
}

#[derive(Clone, Copy)]
enum RequiredTraversal {
    Claimed,
    Nonclaimed { reenter_function_like: bool },
    BlockedMember,
}

struct RequiredDescendantAdapter<'checker, 'program> {
    checker: &'checker mut Checker<'program>,
    file: FileId,
}

impl Checker<'_> {
    pub(super) fn visit_required_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let context = RequiredDescendantContext {
            scope,
            type_parameters: Rc::new(type_parameters.clone()),
            traversal: RequiredTraversal::Claimed,
        };
        let mut adapter = RequiredDescendantAdapter {
            checker: self,
            file,
        };
        walk_expression_descendants(&mut adapter, &context, expression);
    }

    pub(super) fn visit_required_function_like_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        function: &FunctionLikeExpression,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let owner = expression.id;
        for parameter in &function.parameters {
            if parameter.name_kind == ParameterNameKind::Binding
                && parameter.annotation.is_none()
                && parameter.initializer.is_none()
                && !(parameter.optional && self.options.effective_strict_null_checks())
                && let Some(declaration) =
                    self.find_declaration(file, owner, DeclarationKind::Parameter, &parameter.name)
            {
                self.value_queries
                    .entry(declaration)
                    .or_insert(ValueQueryState::Provisional);
            }
        }
        let claimed = self
            .capabilities
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file, owner),
            )
            .is_claimed();
        if !claimed {
            let _ = self.require_completion(Completion::<()>::Deferred);
        } else {
            let function_scope = self.node_scope(file, owner, scope);
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
                    self.visit_required_type_node(
                        file,
                        function_scope,
                        return_type,
                        type_parameters,
                    );
                }
            }
        }
        let context = RequiredDescendantContext {
            scope,
            type_parameters: Rc::new(type_parameters.clone()),
            traversal: if claimed {
                RequiredTraversal::Claimed
            } else {
                RequiredTraversal::Nonclaimed {
                    reenter_function_like: self
                        .capabilities
                        .claim(
                            CapabilityTarget::RequiredType,
                            CapabilityScope::required_function_like(file, owner),
                        )
                        .is_claimed(),
                }
            },
        };
        let mut adapter = RequiredDescendantAdapter {
            checker: self,
            file,
        };
        walk_function_like_descendants(&mut adapter, &context, expression, function);
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
        let traversal = if required_type_claimed {
            self.visit_required_statement_claimed(
                file,
                scope,
                statement,
                next_statement,
                type_parameters,
            );
            RequiredTraversal::Claimed
        } else {
            let _ = self.require_completion(Completion::<()>::Deferred);
            RequiredTraversal::Nonclaimed {
                reenter_function_like: self
                    .capabilities
                    .claim(
                        CapabilityTarget::RequiredType,
                        CapabilityScope::required_function_like(file, statement.id),
                    )
                    .is_claimed(),
            }
        };
        let context = RequiredDescendantContext {
            scope,
            type_parameters: Rc::new(type_parameters.clone()),
            traversal,
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

    fn class_member(
        &mut self,
        context: &Self::Context,
        member: &'ast crate::syntax::ClassMember,
    ) -> Self::Context {
        self.context(context, DescendantContainer::ClassMember(member))
    }

    fn context(
        &mut self,
        context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context {
        if matches!(context.traversal, RequiredTraversal::BlockedMember) {
            return context.clone();
        }
        let (owner, identity, declarations, class_member): (_, _, &[TypeParameterDeclaration], _) =
            match container {
                DescendantContainer::Statement(statement) => (Some(statement.id), None, &[], None),
                DescendantContainer::Function(statement, declaration) => {
                    let identity = self
                        .checker
                        .find_declaration(
                            self.file,
                            statement.id,
                            DeclarationKind::Function,
                            &declaration.name,
                        )
                        .unwrap_or_else(|| {
                            synthetic_identity(self.file, declaration.name_span.start)
                        });
                    (
                        Some(statement.id),
                        Some(identity),
                        &declaration.type_parameters,
                        None,
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
                        .unwrap_or_else(|| {
                            synthetic_identity(self.file, declaration.name_span.start)
                        });
                    (
                        Some(statement.id),
                        Some(identity),
                        &declaration.type_parameters,
                        None,
                    )
                }
                DescendantContainer::ClassMember(member) => match &member.kind {
                    ClassMemberKind::Method {
                        type_parameters, ..
                    } => (
                        Some(member.id),
                        Some(synthetic_identity(self.file, member.name_span.start)),
                        type_parameters.as_slice(),
                        Some(member),
                    ),
                    ClassMemberKind::Constructor { .. } => {
                        (Some(member.id), None, &[], Some(member))
                    }
                    ClassMemberKind::Property { .. } => (None, None, &[], Some(member)),
                },
                DescendantContainer::FunctionLike(expression, function) => {
                    let owner = expression.id;
                    let identity = match function.syntax.function() {
                        Some((name, _)) => self.checker.find_declaration(
                            self.file,
                            owner,
                            DeclarationKind::FunctionExpression,
                            name.as_ref().map_or("", |name| name.name.as_str()),
                        ),
                        None => Some(synthetic_identity(self.file, expression.span.start)),
                    };
                    (Some(owner), identity, &function.type_parameters, None)
                }
            };
        let type_parameters = if declarations.is_empty() {
            Rc::clone(&context.type_parameters)
        } else if let Some(identity) = identity {
            Rc::new(self.checker.extend_type_parameters(
                identity,
                declarations,
                &context.type_parameters,
            ))
        } else {
            let _ = self.checker.require_completion(Completion::<()>::Deferred);
            Rc::clone(&context.type_parameters)
        };
        let scope = owner.map_or(context.scope, |owner| {
            self.checker.node_scope(self.file, owner, context.scope)
        });
        let mut traversal = context.traversal;
        if let Some(member) = class_member
            && matches!(traversal, RequiredTraversal::Claimed)
            && !self.checker.visit_required_class_member(
                self.file,
                context.scope,
                scope,
                member,
                &type_parameters,
            )
        {
            traversal = RequiredTraversal::BlockedMember;
        }
        RequiredDescendantContext {
            scope,
            type_parameters,
            traversal,
        }
    }

    fn nested_statement(
        &mut self,
        context: &Self::Context,
        statement: &'ast Statement,
        next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        if matches!(context.traversal, RequiredTraversal::BlockedMember) {
            return NestedStatement::Handled;
        }
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
        match context.traversal {
            RequiredTraversal::Claimed => self.checker.visit_required_function_like_expression(
                self.file,
                context.scope,
                expression,
                function,
                &context.type_parameters,
            ),
            RequiredTraversal::Nonclaimed {
                reenter_function_like: true,
            } => self.checker.visit_required_expression(
                self.file,
                context.scope,
                expression,
                &context.type_parameters,
            ),
            RequiredTraversal::Nonclaimed {
                reenter_function_like: false,
            } => walk_function_like_descendants(self, context, expression, function),
            RequiredTraversal::BlockedMember => {}
        }
    }

    fn type_node(&mut self, context: &Self::Context, node: &'ast TypeNode) {
        if !matches!(context.traversal, RequiredTraversal::Claimed) {
            return;
        }
        self.checker.visit_required_type_node(
            self.file,
            context.scope,
            node,
            &context.type_parameters,
        );
    }
}
