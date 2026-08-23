use crate::bind::{LexicalThisOwner, ScopeId};
use crate::source::{DeclId, Span};
use crate::syntax::{
    ClassDeclaration, ClassMemberKind, Expression, ExpressionKind, PropertyNameKind, TypeNode,
};

use super::object_shape::plain_type_parameters;
use super::{Checker, ConstructOrigin, DeclarationModel, is_declaration_source};
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
    /// Project one exact lexical-this method without claiming the full class shape.
    pub(super) fn lexical_this_method_type(
        &mut self,
        file: crate::source::FileId,
        scope: ScopeId,
        object: &Expression,
        name: &str,
    ) -> Option<TypeId> {
        if !matches!(object.kind, ExpressionKind::This) {
            return None;
        }
        let LexicalThisOwner::ClassInstance(owner) = self.program.files[file.0 as usize]
            .bindings
            .lexical_this_owner(scope)?
        else {
            return None;
        };
        let DeclarationModel::Class {
            declaration: class,
            scope: class_scope,
            ..
        } = self.models.get(&owner).copied()?
        else {
            return None;
        };
        if !class.type_parameters.is_empty()
            || class.extends.is_some()
            || !class.implements.is_empty()
            || !class.member_syntax_recovery_free
        {
            return None;
        }
        let mut candidates = class
            .members
            .iter()
            .filter(|member| !member.modifiers.static_member && member.name == name);
        let member = candidates.next()?;
        if candidates.next().is_some()
            || member.name_kind != PropertyNameKind::Identifier
            || member.modifiers.static_member
            || member.modifiers.readonly
            || member.modifiers.abstract_member
            || member.modifiers.declared
            || member.modifiers.async_member
            || member.modifiers.unsupported_for_overload_completion
            || !member.overload_context_is_recovery_free()
        {
            return None;
        }
        let ClassMemberKind::Method {
            type_parameters,
            parameters,
            return_type,
            body,
            has_body,
            accessor: None,
        } = &member.kind
        else {
            return None;
        };
        if !*has_body || !type_parameters.is_empty() {
            return None;
        }
        let member_scope = self.node_scope(owner.file, member.id, class_scope);
        self.contextual_function_projection(
            owner.file,
            member_scope,
            parameters,
            return_type.as_ref(),
            body.is_empty(),
        )
    }

    pub(super) fn check_class(
        &mut self,
        file: crate::source::FileId,
        class_scope: ScopeId,
        declaration: &ClassDeclaration,
    ) {
        if declaration.declared
            || is_declaration_source(&self.program.files[file.0 as usize].source.path)
        {
            return;
        }

        self.check_unconstructed_class_properties(file, class_scope, declaration);

        for (index, member) in declaration.members.iter().enumerate() {
            match &member.kind {
                ClassMemberKind::Constructor {
                    has_body: false, ..
                } if !member.modifiers.abstract_member && !member.modifiers.declared => {
                    if !member.overload_context_is_recovery_free() {
                        continue;
                    }
                    let next_is_constructor =
                        declaration.members.get(index + 1).is_some_and(|next| {
                            next.overload_context_is_recovery_free()
                                && matches!(next.kind, ClassMemberKind::Constructor { .. })
                        });
                    if !next_is_constructor {
                        self.push_diagnostic(
                            file,
                            member.name_span,
                            "Constructor implementation is missing.".to_string(),
                            2390,
                        );
                    }
                }
                ClassMemberKind::Method {
                    has_body: false,
                    accessor: None,
                    ..
                } if !member.modifiers.abstract_member && !member.modifiers.declared => {
                    if !member.overload_context_is_recovery_free() {
                        continue;
                    }
                    let Some(next) = declaration.members.get(index + 1) else {
                        self.report_missing_function_implementation(file, member.name_span);
                        continue;
                    };
                    if !next.overload_context_is_recovery_free() {
                        continue;
                    }
                    let ClassMemberKind::Method {
                        has_body: next_has_body,
                        accessor: None,
                        ..
                    } = &next.kind
                    else {
                        self.report_missing_function_implementation(file, member.name_span);
                        continue;
                    };

                    if next.name == member.name {
                        if *next_has_body
                            && next.modifiers.static_member != member.modifiers.static_member
                        {
                            let (code, message) = if member.modifiers.static_member {
                                (2387, "Function overload must be static.")
                            } else {
                                (2388, "Function overload must not be static.")
                            };
                            self.push_diagnostic(file, next.name_span, message.to_string(), code);
                        }
                    } else if *next_has_body {
                        let expected_name = self.program.files[file.0 as usize]
                            .source
                            .slice(member.name_span)
                            .to_string();
                        self.push_diagnostic(
                            file,
                            next.name_span,
                            format!("Function implementation name must be '{expected_name}'."),
                            2389,
                        );
                    } else {
                        self.report_missing_function_implementation(file, member.name_span);
                    }
                }
                ClassMemberKind::Constructor { .. }
                | ClassMemberKind::Property { .. }
                | ClassMemberKind::Method { .. } => {}
            }
        }
    }

    pub(super) fn report_missing_function_implementation(
        &mut self,
        file: crate::source::FileId,
        span: Span,
    ) {
        self.push_diagnostic(
            file,
            span,
            "Function implementation is missing or not immediately following the declaration."
                .to_string(),
            2391,
        );
    }

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
        argument_count: usize,
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
                    identity,
                    declaration: class,
                    scope,
                }) = self.models.get(&declaration).copied()
                else {
                    return Completion::Deferred;
                };
                if type_arguments.is_empty()
                    && !class.type_parameters.is_empty()
                    && plain_type_parameters(&class.type_parameters)
                    && self.is_single_type_symbol_declaration(declaration)
                    && !class.abstract_class
                    && class.extends.is_none()
                    && class.implements.is_empty()
                    && class.members.is_empty()
                {
                    // With no instance members or heritage, every instantiation
                    // has the same empty structural shape. TypeScript still
                    // records `unknown` for each omitted plain parameter so
                    // diagnostics and later symbolic operations retain the
                    // instantiated reference instead of only its shape.
                    let arguments = vec![self.store.builtins.unknown; class.type_parameters.len()];
                    return self.evaluate_class_instance(identity, class, scope, &arguments);
                }
                if !type_arguments.is_empty() && type_arguments.len() != class.type_parameters.len()
                {
                    return Completion::Deferred;
                }
                self.evaluate_reference(declaration, type_arguments)
            }
            TypeKind::Object(shape)
                if type_arguments.is_empty() && shape.construct_signatures.len() == 1 =>
            {
                let signature = &shape.construct_signatures[0];
                if argument_count != 0 || !signature.parameters.is_empty() {
                    return Completion::Deferred;
                }
                Completion::Complete(signature.return_type)
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
            let completion = self.force_type(callee, 0);
            let Completion::Complete(callee) =
                self.require_file_completion(origin.argument_span.file, completion)
            else {
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
            arguments: arguments.to_vec(),
            properties: properties.into(),
        }))
    }
}
