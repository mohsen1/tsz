use crate::bind::{LexicalThisOwner, ScopeId};
use crate::source::{DeclId, Span};
use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMember, ClassMemberKind, Expression, ExpressionKind,
    PropertyNameKind,
};

use super::object_shape::plain_type_parameters;
use super::relation_diagnostic::ContextualType;
use super::{Checker, ConstructOrigin, DeclarationModel};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, Property, TypeId, TypeKind};

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
            ..
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
            || self.program.files[file.0 as usize]
                .source
                .is_declaration_source()
        {
            return;
        }

        self.check_unconstructed_class_properties(file, class_scope, declaration);

        for (index, member) in declaration.members.iter().enumerate() {
            if self
                .capabilities
                .semantic_check_node_is_claimed(file, member.id)
            {
                self.check_accessor_body(file, class_scope, member);
            }
            if member.modifiers.abstract_member
                || member.modifiers.declared
                || !member.overload_context_is_recovery_free()
            {
                continue;
            }
            match &member.kind {
                ClassMemberKind::Constructor {
                    has_body: false, ..
                } => {
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
                } => {
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

    fn check_accessor_body(
        &mut self,
        file: crate::source::FileId,
        class_scope: ScopeId,
        member: &ClassMember,
    ) {
        let ClassMemberKind::Method {
            return_type,
            body,
            accessor: Some(accessor),
            ..
        } = &member.kind
        else {
            return;
        };
        let member_scope = self.node_scope(file, member.id, class_scope);
        let expected_return =
            match (accessor, return_type) {
                (AccessorKind::Get, Some(annotation)) => ContextualType::Known(
                    self.resolve_type_node(file, member_scope, annotation, &Default::default()),
                ),
                (AccessorKind::Get, None) | (AccessorKind::Set, _) => ContextualType::Absent,
            };
        let expected_return_order = return_type.as_ref().and_then(|annotation| {
            self.property_order_for_type_node_root(file, member_scope, annotation)
        });
        self.check_statement_list(
            file,
            member_scope,
            body,
            expected_return,
            expected_return_order.as_ref(),
        );
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
        arguments: Vec<TypeId>,
        argument_span: Span,
    ) -> TypeId {
        let query = self
            .store
            .intern(TypeKind::Deferred(DeferredType::Construct {
                callee,
                type_arguments,
                arguments,
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
        arguments: &[TypeId],
        depth: usize,
    ) -> Completion<TypeId> {
        let callee = completed!(self.force_type(callee, depth));
        match self.store.kind(callee).clone() {
            TypeKind::ClassConstructor { declaration, .. } => {
                if let Some(map_type) = self
                    .program
                    .standard_library
                    .map_type_for_value(declaration)
                {
                    if self
                        .program
                        .standard_library_type_has_authored_declarations(map_type)
                    {
                        return Completion::Deferred;
                    }
                    let valid_entries = match arguments {
                        [entry] => matches!(
                            self.store.kind(*entry),
                            TypeKind::Array(element) if *element == self.store.builtins.never
                        ),
                        _ => false,
                    };
                    let arguments = match type_arguments {
                        [] if arguments.is_empty() => vec![self.store.builtins.any; 2],
                        [key, value] if arguments.is_empty() || valid_entries => vec![*key, *value],
                        _ => return Completion::Deferred,
                    };
                    return Completion::Complete(self.library_reference(map_type, arguments));
                }
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
                self.evaluate_reference(declaration, type_arguments, depth)
            }
            TypeKind::Object(shape)
                if type_arguments.is_empty() && shape.construct_signatures.len() == 1 =>
            {
                let signature = &shape.construct_signatures[0];
                if !arguments.is_empty() || !signature.parameters.is_empty() {
                    return Completion::Deferred;
                }
                Completion::Complete(signature.return_type)
            }
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(callee),
            _ => Completion::Deferred,
        }
    }

    /// Constructor query identity contains all semantic operands. Per-use syntax
    /// retains its argument span so equal queries never share diagnostic provenance.
    pub(super) fn flush_construct_diagnostics(&mut self) {
        for origin in self.construct_origins.clone() {
            let (callee, argument_count) = match self.store.kind(origin.query) {
                TypeKind::Deferred(DeferredType::Construct {
                    callee, arguments, ..
                }) => (*callee, arguments.len()),
                _ => continue,
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
        if class.extends.is_some() {
            return Completion::Deferred;
        }
        let mut members = class.members.iter().collect::<Vec<_>>();
        if members.iter().any(|member| {
            member.modifiers.private
                || member.modifiers.protected
                || member.modifiers.static_member
                || !matches!(
                    &member.kind,
                    ClassMemberKind::Property {
                        annotation: Some(_),
                        ..
                    }
                )
        }) {
            return Completion::Deferred;
        }
        members.sort_by(|left, right| left.name.cmp(&right.name));
        let parameters = self.substitution(declaration, &class.type_parameters, arguments);
        let properties = members
            .into_iter()
            .map(|member| {
                let ClassMemberKind::Property {
                    annotation: Some(annotation),
                    optional,
                    ..
                } = &member.kind
                else {
                    unreachable!("class-property preflight rejected this member")
                };
                Property {
                    name: member.name.clone(),
                    ty: self.resolve_type_node(declaration.file, scope, annotation, &parameters),
                    optional: *optional,
                    readonly: member.modifiers.readonly,
                }
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
