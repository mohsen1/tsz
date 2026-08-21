use crate::bind::ScopeId;
use crate::source::{DeclId, Span};
use crate::syntax::{ClassDeclaration, ClassMemberKind, TypeNode};

use super::{Checker, ConstructOrigin, DeclarationModel};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, Property, TypeId, TypeKind};

pub(super) struct ClassInstanceProperty<'a> {
    pub name: &'a str,
    pub annotation: &'a TypeNode,
    pub optional: bool,
    pub readonly: bool,
}

/// Return the bounded class-instance shape this rewrite can decide today.
///
/// Extends clauses, nominal members, static members, inferred fields, and
/// methods remain deferred until their models are implemented; they must not
/// be approximated with `any`, `unknown`, or an error type.
pub(super) fn class_instance_properties(
    class: &ClassDeclaration,
) -> Option<Vec<ClassInstanceProperty<'_>>> {
    if class.extends.is_some() {
        return None;
    }
    let mut properties = Vec::with_capacity(class.members.len());
    for member in &class.members {
        if member.modifiers.private || member.modifiers.protected || member.modifiers.static_member
        {
            return None;
        }
        let ClassMemberKind::Property {
            annotation: Some(annotation),
            optional,
            ..
        } = &member.kind
        else {
            return None;
        };
        properties.push(ClassInstanceProperty {
            name: &member.name,
            annotation,
            optional: *optional,
            readonly: member.modifiers.readonly,
        });
    }
    properties.sort_by(|left, right| left.name.cmp(right.name));
    Some(properties)
}

impl Checker<'_> {
    pub(super) fn deferred_construct_type(
        &mut self,
        callee: TypeId,
        type_arguments: Vec<TypeId>,
        argument_count: usize,
        argument_span: Span,
    ) -> TypeId {
        let query = self
            .store
            .intern(TypeKind::Deferred(DeferredType::Construct {
                callee,
                type_arguments,
                argument_count,
            }));
        let origin = ConstructOrigin {
            query,
            argument_span,
        };
        if !self.construct_origins.contains(&origin) {
            self.construct_origins.push(origin);
        }
        query
    }

    pub(super) fn evaluate_construct(
        &mut self,
        callee: TypeId,
        type_arguments: &[TypeId],
        _argument_count: usize,
        depth: usize,
    ) -> Completion<TypeId> {
        let callee = match self.force_type(callee, depth) {
            Completion::Complete(callee) => callee,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        match self.store.kind(callee).clone() {
            TypeKind::ClassConstructor { declaration, .. } => {
                let Some(DeclarationModel::Class {
                    declaration: class, ..
                }) = self.models.get(&declaration).copied()
                else {
                    return Completion::Deferred;
                };
                if !type_arguments.is_empty() && type_arguments.len() != class.type_parameters.len()
                {
                    return Completion::Deferred;
                }
                self.evaluate_reference(declaration, type_arguments)
            }
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(callee),
            _ => Completion::Deferred,
        }
    }

    /// Constructor query identity contains only the semantic callee, type
    /// arguments, and arity. Each syntax use retains its own argument span so
    /// equal `new` queries share evaluation without sharing diagnostics.
    pub(super) fn flush_construct_diagnostics(&mut self) {
        for origin in self.construct_origins.clone() {
            let TypeKind::Deferred(DeferredType::Construct {
                callee,
                argument_count,
                ..
            }) = self.store.kind(origin.query).clone()
            else {
                continue;
            };
            if argument_count == 0 {
                continue;
            }
            let Completion::Complete(callee) = self.force_type(callee, 0) else {
                continue;
            };
            let TypeKind::ClassConstructor { declaration, .. } = self.store.kind(callee) else {
                continue;
            };
            let Some(DeclarationModel::Class {
                declaration: class, ..
            }) = self.models.get(declaration).copied()
            else {
                continue;
            };
            if class.extends.is_some()
                || class
                    .members
                    .iter()
                    .any(|member| matches!(&member.kind, ClassMemberKind::Constructor { .. }))
            {
                continue;
            }
            self.push_diagnostic(
                origin.argument_span.file,
                origin.argument_span,
                format!("Expected 0 arguments, but got {argument_count}."),
                2554,
            );
        }
    }

    pub(super) fn evaluate_class_instance(
        &mut self,
        declaration: DeclId,
        class: &ClassDeclaration,
        scope: ScopeId,
        arguments: &[TypeId],
    ) -> Completion<TypeId> {
        let Some(instance_properties) = class_instance_properties(class) else {
            return Completion::Deferred;
        };
        let parameters = self.substitution(declaration, &class.type_parameters, arguments);
        let properties = instance_properties
            .into_iter()
            .map(|property| Property {
                name: property.name.to_string(),
                ty: self.resolve_type_node(
                    declaration.file,
                    scope,
                    property.annotation,
                    &parameters,
                ),
                optional: property.optional,
                readonly: property.readonly,
            })
            .collect::<Vec<_>>();
        Completion::Complete(self.store.intern(TypeKind::ClassInstance {
            declaration,
            name: class.name.clone(),
            properties,
        }))
    }
}
