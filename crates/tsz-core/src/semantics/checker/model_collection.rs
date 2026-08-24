use std::collections::HashMap;

use crate::bind::{DeclarationKind, ScopeId};
use crate::source::FileId;
use crate::syntax::{DescendantContainer, FunctionLikeSyntax, Statement, StatementKind};

use super::{Checker, DeclarationModel};

impl<'a> Checker<'a> {
    pub(super) fn collect_statement_model(
        &mut self,
        file: FileId,
        statement: &'a Statement,
        scope: ScopeId,
    ) {
        statement.for_each_statement_where(
            &mut |container| match container {
                DescendantContainer::Class(..) | DescendantContainer::ClassMember(_) => false,
                DescendantContainer::FunctionLike(_, function) => {
                    matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
                }
                DescendantContainer::Statement(_) | DescendantContainer::Function(..) => true,
            },
            &mut |statement| self.register_statement_model(file, statement, scope),
        );
    }

    fn register_statement_model(
        &mut self,
        file: FileId,
        statement: &'a Statement,
        fallback_scope: ScopeId,
    ) {
        let bound = &self.program.files[file.0 as usize].bindings;
        let function_scope = bound
            .scope_for_node
            .get(&statement.id)
            .copied()
            .unwrap_or(fallback_scope);
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
                        scope: function_scope,
                    })
                }
                (StatementKind::Function(_), DeclarationKind::Parameter) => parameters
                    .remove(candidate.name.as_str())
                    .map(|parameter| DeclarationModel::Parameter {
                        parameter,
                        scope: function_scope,
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
