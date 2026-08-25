use std::collections::HashMap;

use crate::program::SemanticCompletion;
use crate::semantics::types::{Completion, ObjectShape, Property, TypeId, TypeKind, UnionPolicy};
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
        let javascript_root = self.program.javascript_assignments.root(id);
        let javascript_expando = javascript_root.is_some()
            || !self
                .program
                .javascript_assignments
                .declarations(id)
                .is_empty();
        if javascript_root == Some(true) {
            return Completion::Deferred;
        }
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
            DeclarationModel::JavaScriptProperty(..) => self.javascript_property_value_type(id),
            DeclarationModel::TypeAlias { .. } | DeclarationModel::Interface { .. } => {
                Completion::Deferred
            }
        };
        let mut result = match result {
            Completion::Complete(base) => self.javascript_expando_value_type(id, base),
            Completion::Deferred => Completion::Deferred,
            Completion::Cycle => Completion::Cycle,
            Completion::Limit => Completion::Limit,
        };
        let captured = self.completion.finish_capture();
        self.observe_file_completion(id.file, captured);
        if javascript_expando {
            result = match (captured, result) {
                (SemanticCompletion::Deferred, Completion::Complete(_) | Completion::Deferred) => {
                    Completion::Deferred
                }
                (SemanticCompletion::Complete | SemanticCompletion::Deferred, result) => result,
                (SemanticCompletion::Cycle, Completion::Limit) | (SemanticCompletion::Limit, _) => {
                    Completion::Limit
                }
                (SemanticCompletion::Cycle, _) => Completion::Cycle,
            };
        }
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

    fn javascript_property_value_type(&mut self, canonical: DeclId) -> Completion<TypeId> {
        let declarations = self
            .program
            .javascript_assignments
            .declarations(canonical)
            .to_vec();
        if declarations.is_empty() {
            return Completion::Deferred;
        }
        if declarations
            .iter()
            .any(|declaration| !self.semantic_declaration_is_claimed(*declaration))
        {
            return Completion::Deferred;
        }
        let mut values = Vec::with_capacity(declarations.len());
        for declaration in declarations {
            let Some(DeclarationModel::JavaScriptProperty(right, scope)) =
                self.models.get(&declaration).copied()
            else {
                return Completion::Deferred;
            };
            let inferred = self.infer_expression(declaration.file, scope, right, None);
            values.push(self.widen(inferred));
        }
        Completion::Complete(self.store.union(values, UnionPolicy::Canonical))
    }

    fn javascript_expando_value_type(
        &mut self,
        declaration: DeclId,
        base: TypeId,
    ) -> Completion<TypeId> {
        let children = self
            .program
            .javascript_assignments
            .children(declaration)
            .to_vec();
        if children.is_empty() {
            return Completion::Complete(base);
        }
        let mut properties = Vec::with_capacity(children.len());
        for child in children {
            let Some(bound) = self.program.files[child.file.0 as usize]
                .bindings
                .declaration(child)
            else {
                return Completion::Deferred;
            };
            properties.push(Property {
                name: bound.name.clone(),
                ty: completed!(self.declaration_value_type(child)),
                optional: false,
                readonly: false,
            });
        }
        let value = match self.store.kind(base).clone() {
            TypeKind::Object(_) => self.store.object(properties),
            TypeKind::Function(_) | TypeKind::ShapeFunction(_) => {
                let Some(signature) = self.callable_signature(base) else {
                    return Completion::Deferred;
                };
                self.store.intern(TypeKind::Object(ObjectShape {
                    properties,
                    call_signatures: vec![signature],
                    ..ObjectShape::default()
                }))
            }
            _ => return Completion::Deferred,
        };
        Completion::Complete(value)
    }
}
