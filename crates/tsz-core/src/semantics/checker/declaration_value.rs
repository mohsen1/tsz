use std::collections::HashMap;

use crate::program::SemanticCompletion;
use crate::semantics::types::{Completion, TypeId, TypeKind};
use crate::source::DeclId;
use crate::syntax::VariableKind;

use super::{Checker, DeclarationModel};

#[derive(Debug, Clone, Copy)]
pub(super) enum ValueQueryState {
    Computing,
    Ready(TypeId),
}

impl Checker<'_> {
    pub(super) fn declaration_value_type(&mut self, id: DeclId) -> Completion<TypeId> {
        if let Some(declaration) = self.program.standard_library_declaration(id) {
            if self.program.standard_library.is_array_value(id) {
                // Preserve the ambient constructor's declaration identity so
                // operation-local queries can recognize their owned subset of
                // its value shape without inventing a global `any` value.
                return Completion::Complete(self.store.intern(TypeKind::ClassConstructor {
                    declaration: id,
                    name: declaration.name.clone(),
                }));
            }
            return Completion::Deferred;
        }
        if self.program.files[id.file.0 as usize]
            .bindings
            .flow
            .declaration_value_is_incomplete(id)
            || !self.semantic_declaration_is_claimed(id)
            || self.function_value_requires_overload_resolution(id)
        {
            return Completion::Deferred;
        }
        if let Some(ty) = self.parameter_type_overrides.get(&id) {
            return Completion::Complete(*ty);
        }
        match self.value_queries.get(&id).copied() {
            Some(ValueQueryState::Ready(value)) => return Completion::Complete(value),
            Some(ValueQueryState::Computing) => return Completion::Cycle,
            None => {}
        }
        self.value_queries.insert(id, ValueQueryState::Computing);
        let Some(model) = self.models.get(&id).copied() else {
            self.value_queries.remove(&id);
            return Completion::Deferred;
        };
        self.completion.begin_capture();
        let result = match model {
            DeclarationModel::Variable { declaration, scope } => {
                if let Some(annotation) = &declaration.annotation {
                    Completion::Complete(self.resolve_type_node(
                        id.file,
                        scope,
                        annotation,
                        &HashMap::new(),
                    ))
                } else if let Some(completion) = self.extended_unicode_variable_type(declaration) {
                    completion
                } else if let Some(initializer) = &declaration.initializer {
                    let inferred = self.infer_expression(id.file, scope, initializer, None);
                    Completion::Complete(if declaration.declaration_kind == VariableKind::Const {
                        inferred
                    } else {
                        self.widen(inferred)
                    })
                } else {
                    Completion::Complete(self.store.builtins.any)
                }
            }
            DeclarationModel::Parameter { parameter, scope } => {
                self.parameter_value_type(id.file, scope, parameter)
            }
            DeclarationModel::Function { declaration, scope } => {
                self.function_type(id, declaration, scope)
            }
            DeclarationModel::Class {
                identity,
                declaration,
                ..
            } => Completion::Complete(self.store.intern(TypeKind::ClassConstructor {
                declaration: identity,
                name: declaration.name.clone(),
            })),
            DeclarationModel::TypeAlias { .. } | DeclarationModel::Interface { .. } => {
                Completion::Deferred
            }
        };
        let captured = self.completion.finish_capture();
        self.observe_file_completion(id.file, captured);
        match result {
            Completion::Complete(value)
                if captured == SemanticCompletion::Complete && self.is_cacheable_type(value) =>
            {
                self.value_queries.insert(id, ValueQueryState::Ready(value));
            }
            Completion::Complete(_)
            | Completion::Deferred
            | Completion::Cycle
            | Completion::Limit => {
                self.value_queries.remove(&id);
            }
        }
        result
    }
}
