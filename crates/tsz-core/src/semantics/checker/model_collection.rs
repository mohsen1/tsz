use std::collections::HashMap;

use crate::bind::{DeclarationKind, ScopeId};
use crate::source::FileId;
use crate::syntax::{
    ClassDeclaration, DescendantAdapter, DescendantContainer, Expression, FunctionDeclaration,
    FunctionLikeExpression, FunctionLikeSyntax, InterfaceDeclaration, NestedStatement, Parameter,
    Statement, StatementKind, TypeAliasDeclaration, VariableDeclaration,
    walk_function_like_descendants, walk_statement_descendants,
};

use super::Checker;

#[derive(Clone, Copy)]
pub(super) enum DeclarationModel<'a> {
    Variable {
        declaration: &'a VariableDeclaration,
        scope: ScopeId,
    },
    Parameter {
        parameter: &'a Parameter,
        scope: ScopeId,
    },
    Function {
        declaration: &'a FunctionDeclaration,
        scope: ScopeId,
    },
    TypeAlias {
        declaration: &'a TypeAliasDeclaration,
        scope: ScopeId,
    },
    Interface {
        declaration: &'a InterfaceDeclaration,
        scope: ScopeId,
    },
    Class {
        identity: crate::source::DeclId,
        declaration: &'a ClassDeclaration,
        scope: ScopeId,
    },
    JavaScriptProperty(&'a Expression, ScopeId),
}

impl<'a> Checker<'a> {
    pub(super) fn collect_statement_model(&mut self, file: FileId, statement: &'a Statement) {
        self.register_statement_model(file, statement);
        let mut collector = ModelCollector {
            checker: self,
            file,
        };
        walk_statement_descendants(&mut collector, &true, statement);
    }

    fn register_statement_model(&mut self, file: FileId, statement: &'a Statement) {
        let bound = &self.program.files[file.0 as usize].bindings;
        let mut parameters = HashMap::new();
        if let StatementKind::Function(declaration) = &statement.kind {
            for parameter in &declaration.parameters {
                parameters
                    .entry(parameter.name.as_str())
                    .or_insert(parameter);
            }
        }
        let mut primary_modeled = false;
        let mut class_identity = None;
        for candidate in &bound.declarations {
            if candidate.owner != statement.id {
                continue;
            }
            let model = match (&statement.kind, candidate.kind) {
                (StatementKind::Variable(declaration), DeclarationKind::Variable)
                    if !primary_modeled && candidate.name == declaration.name =>
                {
                    primary_modeled = true;
                    Some(DeclarationModel::Variable {
                        declaration,
                        scope: candidate.scope,
                    })
                }
                (StatementKind::Function(declaration), DeclarationKind::Function)
                    if !primary_modeled && candidate.name == declaration.name =>
                {
                    primary_modeled = true;
                    Some(DeclarationModel::Function {
                        declaration,
                        scope: bound
                            .scope_for_node
                            .get(&statement.id)
                            .copied()
                            .unwrap_or(candidate.scope),
                    })
                }
                (StatementKind::Function(_), DeclarationKind::Parameter) => parameters
                    .remove(candidate.name.as_str())
                    .map(|parameter| DeclarationModel::Parameter {
                        parameter,
                        scope: candidate.scope,
                    }),
                (StatementKind::TypeAlias(declaration), DeclarationKind::TypeAlias)
                    if !primary_modeled && candidate.name == declaration.name =>
                {
                    primary_modeled = true;
                    Some(DeclarationModel::TypeAlias {
                        declaration,
                        scope: candidate.scope,
                    })
                }
                (StatementKind::Interface(declaration), DeclarationKind::Interface)
                    if !primary_modeled && candidate.name == declaration.name =>
                {
                    primary_modeled = true;
                    Some(DeclarationModel::Interface {
                        declaration,
                        scope: candidate.scope,
                    })
                }
                (StatementKind::Class(declaration), DeclarationKind::Class)
                    if candidate.name == declaration.name =>
                {
                    let (identity, scope) =
                        *class_identity.get_or_insert((candidate.id, candidate.scope));
                    Some(DeclarationModel::Class {
                        identity,
                        declaration,
                        scope,
                    })
                }
                _ => None,
            };
            if let Some(model) = model {
                self.models.insert(candidate.id, model);
            }
        }
    }
}

struct ModelCollector<'checker, 'program> {
    checker: &'checker mut Checker<'program>,
    file: FileId,
}

impl<'program> DescendantAdapter<'program> for ModelCollector<'_, 'program> {
    type Context = bool;

    fn context(&mut self, context: &bool, container: DescendantContainer<'program>) -> bool {
        *context
            && match container {
                DescendantContainer::Class(..) | DescendantContainer::ClassMember(_) => false,
                DescendantContainer::FunctionLike(_, function) => {
                    matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
                }
                DescendantContainer::Statement(_) | DescendantContainer::Function(..) => true,
            }
    }

    fn nested_statement(
        &mut self,
        context: &bool,
        statement: &'program Statement,
        _next_statement: Option<&'program Statement>,
    ) -> NestedStatement {
        if *context {
            self.checker.register_statement_model(self.file, statement);
        }
        NestedStatement::Descend
    }

    fn function_like(
        &mut self,
        context: &bool,
        expression: &'program Expression,
        function: &'program FunctionLikeExpression,
    ) {
        walk_function_like_descendants(self, context, expression, function);
    }

    fn expression(&mut self, _context: &bool, expression: &'program Expression) {
        let Some((declaration, scope)) = self
            .checker
            .program
            .javascript_assignments
            .rhs_declaration(self.file, expression.id)
            .and_then(|declaration| {
                self.checker.program.files[self.file.0 as usize]
                    .bindings
                    .declaration(declaration)
                    .map(|bound| (declaration, bound.scope))
            })
        else {
            return;
        };
        self.checker.models.insert(
            declaration,
            DeclarationModel::JavaScriptProperty(expression, scope),
        );
    }
}
