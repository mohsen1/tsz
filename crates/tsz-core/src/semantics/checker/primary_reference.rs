use crate::bind::{LexicalThisOwner, Meaning, ScopeId};
use crate::program::SemanticCompletion;
use crate::semantics::types::{DeferredType, TypeId, TypeKind};
use crate::source::{FileId, NodeId};
use crate::syntax::{Expression, ExpressionKind};

use super::{Checker, DeclarationModel};

impl Checker<'_> {
    pub(super) fn infer_identifier(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> TypeId {
        let ExpressionKind::Identifier {
            name,
            name_span,
            entity_name,
        } = &expression.kind
        else {
            unreachable!("identifier inference requires identifier syntax");
        };
        if !*entity_name {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            return self.store.builtins.error;
        }
        let Some(declaration) = self.resolve_semantic_name(file, scope, name, Meaning::Value)
        else {
            self.push_diagnostic(
                file,
                *name_span,
                format!("Cannot find name '{name}'."),
                2304,
            );
            return self.store.builtins.error;
        };
        if self
            .program
            .standard_library
            .is_undefined_value(declaration)
        {
            return self.store.builtins.undefined;
        }
        let declared = self
            .store
            .intern(TypeKind::Deferred(DeferredType::Value(declaration)));
        if self.program.files[file.0 as usize]
            .bindings
            .flow
            .reference_node(expression.id, declaration)
            .is_some()
        {
            self.store
                .intern(TypeKind::Deferred(DeferredType::FlowReference {
                    file,
                    expression: expression.id,
                    declaration,
                    declared,
                }))
        } else {
            declared
        }
    }

    pub(super) fn infer_this_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: NodeId,
    ) -> TypeId {
        let owner = self.program.files[file.0 as usize]
            .bindings
            .lexical_this_owner(scope);
        match owner {
            Some(LexicalThisOwner::ClassInstance(declaration)) => {
                let Some(DeclarationModel::Class {
                    identity,
                    declaration: class,
                    ..
                }) = self.models.get(&declaration).copied()
                else {
                    return self.deferred_lexical_this(file, expression);
                };
                let arguments = class
                    .type_parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| {
                        self.store.intern(TypeKind::TypeParameter {
                            declaration: identity,
                            index: index as u32,
                            name: parameter.name.clone(),
                        })
                    })
                    .collect();
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Reference {
                        declaration,
                        arguments,
                    }))
            }
            Some(LexicalThisOwner::ClassConstructor(declaration)) => self
                .store
                .intern(TypeKind::Deferred(DeferredType::Value(declaration))),
            None => self.deferred_lexical_this(file, expression),
        }
    }

    fn deferred_lexical_this(&mut self, file: FileId, expression: NodeId) -> TypeId {
        self.observe_file_completion(file, SemanticCompletion::Deferred);
        self.store
            .intern(TypeKind::Deferred(DeferredType::LexicalThis {
                file,
                expression,
            }))
    }
}
