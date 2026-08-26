use std::collections::HashMap;

use crate::bind::{DeclarationKind, ScopeId};
use crate::program::SemanticCompletion;
use crate::semantics::relation::RelationMode;
use crate::semantics::types::{Completion, ObjectShape, Property, TypeId, TypeKind, UnionPolicy};
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{VariableDeclarator, VariableKind, VariableStatement};

use super::{Checker, DeclarationModel, relation_diagnostic::RelationDiagnosticStyle};

#[derive(Debug, Clone, Copy)]
pub(super) enum ValueQueryState {
    Computing,
    Ready(TypeId),
}

impl Checker<'_> {
    pub(super) fn check_variable(
        &mut self,
        file: FileId,
        scope: ScopeId,
        owner: NodeId,
        statement: &VariableStatement,
    ) {
        let ambient = statement.declared
            || self.program.files[file.0 as usize]
                .source
                .is_declaration_source();
        for declarator in &statement.declarators {
            self.check_variable_declarator(
                file,
                scope,
                owner,
                statement.declaration_kind,
                ambient,
                declarator,
            );
        }
    }

    fn check_variable_declarator(
        &mut self,
        file: FileId,
        scope: ScopeId,
        owner: NodeId,
        kind: VariableKind,
        ambient: bool,
        declaration: &VariableDeclarator,
    ) {
        if !ambient && kind == VariableKind::Const && declaration.initializer.is_none() {
            let message = "'const' declarations must be initialized.".into();
            self.push_diagnostic(file, declaration.name_span, message, 1155);
        }
        if ambient
            && declaration.annotation.is_none()
            && declaration.initializer.is_none()
            && self.options.effective_no_implicit_any()
        {
            self.push_diagnostic(
                file,
                declaration.name_span,
                format!(
                    "Variable '{}' implicitly has an 'any' type.",
                    declaration.name
                ),
                7005,
            );
        }
        let (annotation, annotation_is_complete) =
            declaration
                .annotation
                .as_ref()
                .map_or((None, true), |annotation| {
                    let ty = self.resolve_type_node(file, scope, annotation, &HashMap::new());
                    let is_complete = self.complete_required_type_nodes.contains(&annotation.span);
                    (Some(ty), is_complete)
                });
        self.completion.begin_capture();
        let initializer = declaration
            .initializer
            .as_ref()
            .map(|initializer| self.infer_expression(file, scope, initializer, annotation));
        let initializer_completion = self.completion.finish_capture();
        if let (Some(source), Some(target), Some(initializer)) =
            (initializer, annotation, declaration.initializer.as_ref())
        {
            let target_order = declaration.annotation.as_ref().and_then(|annotation| {
                self.property_order_for_type_node_root(file, scope, annotation)
            });
            self.report_relation(
                source,
                target,
                declaration.name_span,
                Some(initializer),
                target_order,
                RelationMode::Assignment,
                RelationDiagnosticStyle::Type,
            );
        }
        if let Some(id) = self.program.files[file.0 as usize]
            .bindings
            .declarations
            .iter()
            .find(|candidate| {
                candidate.owner == owner
                    && candidate.kind == DeclarationKind::Variable
                    && candidate.name_span == declaration.name_span
            })
            .map(|candidate| candidate.id)
        {
            let initializer = initializer.map(|inferred| {
                if kind == VariableKind::Const {
                    inferred
                } else {
                    self.widen(inferred)
                }
            });
            let value = annotation
                .or(initializer)
                .unwrap_or(self.store.builtins.any);
            if annotation_is_complete
                && (annotation.is_some() || initializer_completion.is_complete())
                && self.is_cacheable_type(value)
                && self.semantic_declaration_is_claimed(id)
                && self.program.javascript_assignments.root(id).is_none()
            {
                self.value_queries.insert(id, ValueQueryState::Ready(value));
            } else {
                self.value_queries.remove(&id);
            }
        }
    }

    pub(super) fn declaration_value_type(&mut self, id: DeclId) -> Completion<TypeId> {
        let assignments = &self.program.javascript_assignments;
        let javascript_root = assignments.root(id);
        if javascript_root == Some(true) {
            return Completion::Deferred;
        }
        let javascript_expando =
            javascript_root.is_some() || !assignments.declarations(id).is_empty();
        if let Some(declaration) = self.program.standard_library_declaration(id) {
            if self.program.standard_library.is_array_value(id)
                || self
                    .program
                    .standard_library
                    .map_type_for_value(id)
                    .is_some()
            {
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
            DeclarationModel::Variable {
                declaration,
                declaration_kind,
                scope,
            } => {
                if let Some(annotation) = &declaration.annotation {
                    Completion::Complete(self.resolve_type_node(
                        id.file,
                        scope,
                        annotation,
                        &HashMap::new(),
                    ))
                } else if let Some(initializer) = &declaration.initializer {
                    let inferred = self.infer_expression(id.file, scope, initializer, None);
                    Completion::Complete(if declaration_kind == VariableKind::Const {
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
            incomplete => incomplete,
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
            _ => {
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
        if declarations.is_empty()
            || declarations
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
                let Some(mut signature) = self.callable_signature(base) else {
                    return Completion::Deferred;
                };
                signature.generic_declaration = None;
                for parameter in &mut signature.parameters {
                    parameter.name = None;
                }
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
