use std::collections::{HashMap, HashSet};

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::source::{DeclId, FileId, SourceKind};
use crate::syntax::{
    ArrowBody, ClassMember, ClassMemberKind, Expression, ExpressionKind, FunctionDeclaration,
    FunctionLikeExpression, FunctionLikeSyntax, Parameter, ParameterNameKind, Statement,
    StatementKind,
};

use super::{Checker, DeclarationModel, relation_diagnostic::ContextualType};
use crate::semantics::relation::{RelationMode, RelationPropertyOrder, relate_with_property_order};
use crate::semantics::types::{
    Completion, ParameterType, Signature, TypeId, TypeKind, UnionPolicy,
};

struct ReturnSite<'a> {
    statement: &'a Statement,
    expression: Option<&'a Expression>,
}

impl Checker<'_> {
    pub(super) fn declared_function_type(&mut self, id: DeclId) -> Completion<TypeId> {
        let Some(DeclarationModel::Function { declaration, scope }) = self.models.get(&id).copied()
        else {
            return Completion::Deferred;
        };
        self.function_type(id, declaration, scope)
    }

    pub(super) fn function_value_requires_overload_resolution(&self, id: DeclId) -> bool {
        self.function_group_ids(id).into_iter().take(2).count() > 1
    }

    fn value_group_ids(&self, id: DeclId) -> Vec<DeclId> {
        let bindings = &self.program.files[id.file.0 as usize].bindings;
        let Some(declaration) = bindings.declaration(id) else {
            return Vec::new();
        };
        if declaration.meaning != Meaning::Value {
            return Vec::new();
        }
        let global_script_group = declaration.scope == ScopeId(0)
            && !self.program.files[id.file.0 as usize].is_external_module();
        let group = if global_script_group {
            self.program.global_values.get(&declaration.name)
        } else {
            bindings
                .scopes
                .get(declaration.scope.0 as usize)
                .and_then(|scope| scope.names.get(&declaration.name))
        };
        group
            .into_iter()
            .flatten()
            .copied()
            .filter(|candidate| {
                let candidate_file = &self.program.files[candidate.file.0 as usize];
                (!global_script_group || !candidate_file.is_external_module())
                    && candidate_file
                        .bindings
                        .declaration(*candidate)
                        .is_some_and(|candidate| candidate.meaning == Meaning::Value)
            })
            .collect()
    }

    fn function_group_ids(&self, id: DeclId) -> Vec<DeclId> {
        self.value_group_ids(id)
            .into_iter()
            .filter(|candidate| {
                self.program.files[candidate.file.0 as usize]
                    .bindings
                    .declaration(*candidate)
                    .is_some_and(|candidate| candidate.kind == DeclarationKind::Function)
            })
            .collect()
    }

    fn javascript_function_redeclaration_group_is_modeled(&self, group: &[DeclId]) -> bool {
        let mut has_function = false;
        let modeled = group.iter().all(|candidate| {
            let Some(file) = self.program.files.get(candidate.file.0 as usize) else {
                return false;
            };
            if !matches!(
                file.source.kind(),
                SourceKind::JavaScript | SourceKind::JavaScriptJsx
            ) {
                return false;
            }
            match self.models.get(candidate) {
                Some(DeclarationModel::Function { declaration, .. }) => {
                    has_function = true;
                    declaration.has_body
                        && !declaration.exported
                        && !declaration.is_async
                        && !declaration.default_export
                        && !declaration.declared
                        && !declaration.abstract_declaration
                        && declaration.type_parameters.is_empty()
                        && declaration.return_type.is_none()
                        && declaration
                            .parameters
                            .iter()
                            .all(|parameter| parameter.annotation.is_none() && !parameter.optional)
                        && declaration.overload_context_is_recovery_free()
                }
                _ => false,
            }
        });
        modeled && has_function
    }

    /// Whether the binder-owned value group has a modeled declaration host.
    /// Cross-kind peers, repeated classes, and multiple implementations do not.
    pub(super) fn declaration_value_host_is_modeled(
        &self,
        id: DeclId,
        expected_kind: DeclarationKind,
    ) -> bool {
        let group = self.value_group_ids(id);
        if group.is_empty() {
            return false;
        }
        if expected_kind == DeclarationKind::Function
            && self
                .program
                .file(id.file)
                .and_then(|file| {
                    file.bindings
                        .declaration(id)
                        .map(|declaration| (file, declaration))
                })
                .is_none_or(|(file, declaration)| {
                    declaration.scope == ScopeId(0)
                        && !file.is_external_module()
                        && self
                            .program
                            .standard_library
                            .resolve(&declaration.name, Meaning::Value)
                            .is_some()
                })
        {
            return false;
        }
        if expected_kind == DeclarationKind::Function
            && self.javascript_function_redeclaration_group_is_modeled(&group)
        {
            return true;
        }
        if expected_kind == DeclarationKind::Class && group.len() != 1 {
            return false;
        }
        let mut function_implementations = 0;
        group.into_iter().all(|candidate| {
            let Some(bound) = self.program.files[candidate.file.0 as usize]
                .bindings
                .declaration(candidate)
            else {
                return false;
            };
            if bound.kind != expected_kind {
                return false;
            }
            if expected_kind != DeclarationKind::Function {
                return true;
            }
            let Some(DeclarationModel::Function { declaration, .. }) = self.models.get(&candidate)
            else {
                return false;
            };
            function_implementations += usize::from(declaration.has_body);
            function_implementations <= 1
        })
    }

    /// Validate only modeled overload owners; unsupported compatibility defers.
    pub(super) fn validate_function_overload_group(&mut self, id: DeclId) {
        let group = self.function_group_ids(id);
        if group.len() < 2 {
            return;
        }
        if group
            .iter()
            .any(|candidate| !self.semantic_declaration_is_claimed(*candidate))
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
            return;
        }
        if group.first() != Some(&id) {
            return;
        }
        if self.javascript_function_redeclaration_group_is_modeled(&self.value_group_ids(id)) {
            return;
        }
        if group.iter().any(|candidate| {
            matches!(
                self.models.get(candidate),
                Some(DeclarationModel::Function { declaration, .. })
                    if declaration.is_async
                        || declaration.default_export
                        || declaration.abstract_declaration
                        || !declaration.overload_context_is_recovery_free()
            )
        }) {
            // This checkpoint does not own TS1064, default export, or abstract functions.
            let _ = self.require_completion(Completion::<()>::Deferred);
            return;
        }
        self.validate_function_overload_modifiers(&group);
        let implementations = group
            .iter()
            .copied()
            .filter(|candidate| {
                matches!(
                    self.models.get(candidate),
                    Some(DeclarationModel::Function { declaration, .. }) if declaration.has_body
                )
            })
            .collect::<Vec<_>>();
        let [implementation] = implementations.as_slice() else {
            if !implementations.is_empty() {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            return;
        };
        let overloads = group
            .iter()
            .copied()
            .filter(|candidate| {
                matches!(
                    self.models.get(candidate),
                    Some(DeclarationModel::Function { declaration, .. }) if !declaration.has_body
                )
            })
            .collect::<Vec<_>>();
        for overload in overloads {
            if !self.function_overload_is_compatibly_modeled(*implementation, overload) {
                // TS2394 awaits the full erased-signature owner.
                let _ = self.require_completion(Completion::<()>::Deferred);
                return;
            }
        }
    }

    fn validate_function_overload_modifiers(&mut self, group: &[DeclId]) {
        let mut files = group.iter().map(|id| id.file).collect::<Vec<_>>();
        files.sort_unstable();
        files.dedup();
        for file in files {
            let declarations = group
                .iter()
                .copied()
                .filter(|id| id.file == file)
                .collect::<Vec<_>>();
            let canonical = declarations
                .iter()
                .copied()
                .find(|id| {
                    matches!(
                        self.models.get(id),
                        Some(DeclarationModel::Function { declaration, .. }) if declaration.has_body
                    )
                })
                .unwrap_or(declarations[0]);
            let Some(DeclarationModel::Function {
                declaration: canonical,
                ..
            }) = self.models.get(&canonical).copied()
            else {
                let _ = self.require_completion(Completion::<()>::Deferred);
                continue;
            };
            let declaration_source = self.program.files[file.0 as usize]
                .source
                .is_declaration_source();
            let canonical_ambient = canonical.declared || declaration_source;
            for id in declarations {
                let Some(DeclarationModel::Function { declaration, .. }) =
                    self.models.get(&id).copied()
                else {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                    continue;
                };
                if declaration.exported != canonical.exported {
                    self.push_diagnostic(
                        file,
                        declaration.name_span,
                        "Overload signatures must all be exported or non-exported.".to_string(),
                        2383,
                    );
                } else {
                    let ambient = declaration.declared || declaration_source;
                    if ambient != canonical_ambient {
                        self.push_diagnostic(
                            file,
                            declaration.name_span,
                            "Overload signatures must all be ambient or non-ambient.".to_string(),
                            2384,
                        );
                    }
                }
            }
        }
    }

    fn function_overload_is_compatibly_modeled(
        &mut self,
        implementation: DeclId,
        overload: DeclId,
    ) -> bool {
        let Some(DeclarationModel::Function {
            declaration: implementation_declaration,
            ..
        }) = self.models.get(&implementation).copied()
        else {
            return false;
        };
        let Some(DeclarationModel::Function {
            declaration: overload_declaration,
            ..
        }) = self.models.get(&overload).copied()
        else {
            return false;
        };
        if !implementation_declaration.type_parameters.is_empty()
            || !overload_declaration.type_parameters.is_empty()
            || implementation_declaration
                .parameters
                .iter()
                .chain(&overload_declaration.parameters)
                .any(|parameter| parameter.rest || parameter.initializer.is_some())
        {
            return false;
        }
        let Completion::Complete(implementation_type) = self.declared_function_type(implementation)
        else {
            return false;
        };
        let Completion::Complete(overload_type) = self.declared_function_type(overload) else {
            return false;
        };
        let TypeKind::Function(implementation_signature) =
            self.store.kind(implementation_type).clone()
        else {
            return false;
        };
        let TypeKind::Function(overload_signature) = self.store.kind(overload_type).clone() else {
            return false;
        };
        self.signatures_are_compatibly_modeled(&implementation_signature, &overload_signature)
    }
    pub(super) fn class_overload_is_compatibly_modeled(
        &mut self,
        file: FileId,
        overload: &ClassMember,
        implementation: &ClassMember,
    ) -> bool {
        let Completion::Complete(overload_signature) =
            self.class_member_overload_signature(file, overload, self.store.builtins.any)
        else {
            return false;
        };
        let Completion::Complete(implementation_signature) =
            self.class_member_overload_signature(file, implementation, self.store.builtins.void)
        else {
            return false;
        };
        self.signatures_are_compatibly_modeled(&implementation_signature, &overload_signature)
    }

    fn class_member_overload_signature(
        &mut self,
        file: FileId,
        member: &ClassMember,
        missing_method_return: TypeId,
    ) -> Completion<Signature> {
        let Some(scope) = self.program.files[file.0 as usize]
            .bindings
            .scope_for_node
            .get(&member.id)
            .copied()
        else {
            return Completion::Deferred;
        };
        let (parameters, return_type) = match &member.kind {
            ClassMemberKind::Constructor { parameters, .. } => {
                (parameters.as_slice(), self.store.builtins.void)
            }
            ClassMemberKind::Method {
                parameters,
                return_type,
                ..
            } => (
                parameters.as_slice(),
                return_type.as_ref().map_or(missing_method_return, |node| {
                    self.resolve_type_node(file, scope, node, &HashMap::new())
                }),
            ),
            ClassMemberKind::Property { .. } => return Completion::Deferred,
        };
        let parameters = completed!(self.anonymous_signature_parameters(
            file,
            scope,
            parameters,
            &HashMap::new(),
        ));
        Completion::Complete(Signature {
            generic_declaration: None,
            untyped_javascript: false,
            parameters,
            return_type,
        })
    }

    fn signatures_are_compatibly_modeled(
        &mut self,
        implementation_signature: &Signature,
        overload_signature: &Signature,
    ) -> bool {
        let implementation_required = implementation_signature
            .parameters
            .iter()
            .filter(|parameter| !parameter.optional)
            .count();
        let overload_required = overload_signature
            .parameters
            .iter()
            .filter(|parameter| !parameter.optional)
            .count();
        if implementation_required > overload_required {
            return false;
        }
        for (implementation_parameter, overload_parameter) in implementation_signature
            .parameters
            .iter()
            .zip(&overload_signature.parameters)
        {
            if !self.types_are_assignable(overload_parameter.ty, implementation_parameter.ty) {
                return false;
            }
        }
        matches!(
            self.store.kind(overload_signature.return_type),
            TypeKind::Void
        ) || self.types_are_assignable(
            overload_signature.return_type,
            implementation_signature.return_type,
        ) || self.types_are_assignable(
            implementation_signature.return_type,
            overload_signature.return_type,
        )
    }

    fn types_are_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        relate_with_property_order(
            self,
            source,
            target,
            RelationMode::Assignment,
            RelationPropertyOrder::default(),
        )
        .is_ok()
    }

    pub(super) fn infer_function_like_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        function: &FunctionLikeExpression,
        expected: ContextualType,
    ) -> TypeId {
        let owner = expression.id;
        if !self
            .capabilities
            .semantic_check_node_is_claimed(file, owner)
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
            if self
                .capabilities
                .semantic_check_node_function_like_descendant_permissions(file, owner)
                .0
            {
                self.check_function_like_expression_body_only(file, scope, expression, function);
            }
            return self.store.deferred_generic_function();
        }
        self.infer_function_like_expression_claimed(file, scope, expression, function, expected)
    }

    fn infer_function_like_expression_claimed(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        function: &FunctionLikeExpression,
        expected: ContextualType,
    ) -> TypeId {
        let owner = expression.id;
        let function_scope = self.program.files[file.0 as usize]
            .bindings
            .scope_for_node
            .get(&owner)
            .copied()
            .unwrap_or(scope);
        self.check_parameter_initializer_statement_descendants(
            file,
            function_scope,
            &function.parameters,
        );
        let (function_identity, named_function) = match &function.syntax {
            FunctionLikeSyntax::Arrow(_) => (None, false),
            FunctionLikeSyntax::Function { name, .. } => (
                self.find_declaration(
                    file,
                    owner,
                    DeclarationKind::FunctionExpression,
                    name.as_ref().map_or("", |name| name.name.as_str()),
                ),
                name.is_some(),
            ),
        };
        if matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
            && function_identity.is_none()
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
            self.check_function_like_expression_body_only(file, scope, expression, function);
            return self.store.deferred_generic_function();
        }
        if function_identity.is_some() {
            self.completion.begin_capture();
        }
        let (expected_signature, signature_context) = match (
            function_expression_has_authored_signature(function),
            expected,
        ) {
            (true, _) | (false, ContextualType::Absent) => (None, ContextualType::Absent),
            (false, ContextualType::Known(expected)) => {
                if let Some(expected) = self.complete_type(expected) {
                    if let Some(signature) = self.callable_signature(expected) {
                        (Some(signature), ContextualType::Known(expected))
                    } else {
                        (None, ContextualType::Absent)
                    }
                } else {
                    (None, ContextualType::Deferred)
                }
            }
            (false, ContextualType::Deferred) => (None, ContextualType::Deferred),
        };
        if matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
            && matches!(signature_context, ContextualType::Deferred)
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
        let runtime_parameters = function
            .parameters
            .iter()
            .filter(|parameter| parameter.name_kind == ParameterNameKind::Binding)
            .collect::<Vec<_>>();
        let mut resolved = Vec::with_capacity(runtime_parameters.len());
        for (index, parameter) in runtime_parameters.iter().copied().enumerate() {
            if parameter.initializer.is_some()
                && (parameter.annotation.is_some()
                    || parameter.initializer.as_ref().is_some_and(|initializer| {
                        !matches!(initializer.kind, ExpressionKind::Literal(_))
                    }))
            {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            let ty = if let Some(annotation) = &parameter.annotation {
                if annotation.contains_type_query() {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                self.resolve_type_node(file, function_scope, annotation, &HashMap::new())
            } else if let Some(initializer) = &parameter.initializer {
                let completion = self.signature_initializer_type(file, function_scope, initializer);
                match self.require_completion(completion) {
                    Completion::Complete(ty) => ty,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        self.store.builtins.any
                    }
                }
            } else {
                expected_signature
                    .as_ref()
                    .and_then(|signature| signature.parameters.get(index))
                    .map_or(self.store.builtins.any, |parameter| parameter.ty)
            };
            if parameter.annotation.is_none()
                && parameter.initializer.is_none()
                && expected_signature.is_none()
                && matches!(signature_context, ContextualType::Absent)
                && self.options.effective_no_implicit_any()
            {
                if matches!(&function.syntax, FunctionLikeSyntax::Function { .. }) {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                } else {
                    self.push_diagnostic(
                        file,
                        parameter.name_span,
                        format!(
                            "Parameter '{}' implicitly has an 'any' type.",
                            parameter.name
                        ),
                        7006,
                    );
                }
            }
            resolved.push(ParameterType {
                name: Some(parameter.name.clone()),
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        let expected_return = if let Some(annotation) = function.return_type.as_ref() {
            if annotation.contains_type_query() {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            ContextualType::Known(self.resolve_type_node(
                file,
                function_scope,
                annotation,
                &HashMap::new(),
            ))
        } else if let Some(signature) = &expected_signature {
            ContextualType::Known(signature.return_type)
        } else {
            signature_context
        };
        let expected_return_type = match expected_return {
            ContextualType::Known(expected_return) => Some(expected_return),
            ContextualType::Absent | ContextualType::Deferred => None,
        };
        let expected_return_order = function.return_type.as_ref().and_then(|annotation| {
            self.property_order_for_type_node_root(file, function_scope, annotation)
        });
        let untyped_javascript = javascript_signature_is_untyped(
            self.program.files[file.0 as usize].source.kind(),
            &function.parameters,
            function.type_parameters.is_empty(),
            function.return_type.is_none(),
            matches!(signature_context, ContextualType::Absent),
        );
        for (parameter, resolved) in runtime_parameters.iter().copied().zip(&resolved) {
            if parameter.initializer.is_some()
                || parameter.optional && self.options.effective_strict_null_checks()
                || parameter.annotation.is_none()
                    && expected_signature.is_none()
                    && (!matches!(signature_context, ContextualType::Absent)
                        || self.options.effective_no_implicit_any())
            {
                continue;
            }
            if let Some(declaration) =
                self.find_declaration(file, owner, DeclarationKind::Parameter, &parameter.name)
            {
                self.parameter_type_overrides
                    .insert(declaration, resolved.ty);
            }
        }
        match &function.syntax {
            FunctionLikeSyntax::Arrow(ArrowBody::Expression(body)) => {
                let return_type =
                    self.infer_expression_contextual(file, function_scope, body, expected_return);
                self.store
                    .function(None, untyped_javascript, resolved, return_type)
            }
            FunctionLikeSyntax::Arrow(ArrowBody::Block(statements)) => {
                self.check_statement_list(
                    file,
                    function_scope,
                    statements,
                    expected_return,
                    expected_return_order.as_ref(),
                );
                self.store.function(
                    None,
                    untyped_javascript,
                    resolved,
                    expected_return_type.unwrap_or(self.store.builtins.void),
                )
            }
            FunctionLikeSyntax::Function { body, .. } => {
                let previous_self_query =
                    if named_function && let Some(identity) = function_identity {
                        Some((
                            identity,
                            self.value_queries.insert(
                                identity,
                                super::declaration_value::ValueQueryState::Computing,
                            ),
                        ))
                    } else {
                        None
                    };
                let return_type = match expected_return_type {
                    Some(return_type) => Completion::Complete(return_type),
                    None => self.infer_block_return(file, body, function_scope),
                };
                let return_type = self.require_completion(return_type);
                let signature_completion = self.completion.finish_capture();
                let inferred = match return_type {
                    Completion::Complete(return_type) if signature_completion.is_complete() => {
                        let inferred =
                            self.store
                                .function(None, untyped_javascript, resolved, return_type);
                        if named_function
                            && function_expression_has_authored_signature(function)
                            && let Some(identity) = function_identity
                        {
                            self.value_queries.insert(
                                identity,
                                super::declaration_value::ValueQueryState::Ready(inferred),
                            );
                        }
                        let body_expected_return = if function.return_type.is_none()
                            && matches!(self.store.kind(return_type), TypeKind::Void)
                        {
                            ContextualType::Absent
                        } else {
                            ContextualType::Known(return_type)
                        };
                        self.check_statement_list(
                            file,
                            function_scope,
                            body,
                            body_expected_return,
                            expected_return_order.as_ref(),
                        );
                        inferred
                    }
                    Completion::Complete(_)
                    | Completion::Deferred
                    | Completion::Cycle
                    | Completion::Limit => {
                        self.check_statement_list(
                            file,
                            function_scope,
                            body,
                            ContextualType::Deferred,
                            None,
                        );
                        self.store.deferred_generic_function()
                    }
                };
                if let Some((identity, previous)) = previous_self_query {
                    if let Some(previous) = previous {
                        self.value_queries.insert(identity, previous);
                    } else {
                        self.value_queries.remove(&identity);
                    }
                }
                inferred
            }
        }
    }

    pub(super) fn parameter_value_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameter: &Parameter,
    ) -> Completion<TypeId> {
        if parameter.annotation.is_some() && parameter.initializer.is_some() {
            return Completion::Deferred;
        }
        if let Some(annotation) = &parameter.annotation {
            let type_parameters = self.enclosing_function_type_parameters(file, scope);
            let mut ty = self.resolve_type_node(file, scope, annotation, &type_parameters);
            if parameter.optional && self.options.effective_strict_null_checks() {
                ty = self
                    .store
                    .union([ty, self.store.builtins.undefined], UnionPolicy::Canonical);
            }
            return Completion::Complete(ty);
        }
        parameter.initializer.as_ref().map_or(
            Completion::Complete(self.store.builtins.any),
            |initializer| self.signature_initializer_type(file, scope, initializer),
        )
    }

    pub(super) fn signature_initializer_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        initializer: &Expression,
    ) -> Completion<TypeId> {
        if matches!(
            initializer.kind,
            ExpressionKind::Literal(crate::syntax::Literal::BigInt(_))
        ) {
            return if matches!(
                self.options.target.as_str(),
                "es2020" | "es2021" | "es2022" | "es2023" | "es2024" | "es2025" | "esnext"
            ) {
                Completion::Complete(self.store.builtins.bigint)
            } else {
                Completion::Deferred
            };
        }
        if !matches!(initializer.kind, ExpressionKind::Literal(_)) {
            return Completion::Deferred;
        }
        let inferred = self.infer_expression(file, scope, initializer, None);
        Completion::Complete(self.widen(inferred))
    }

    pub(super) fn anonymous_signature_parameters(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
    ) -> Completion<Vec<ParameterType>> {
        if parameters
            .iter()
            .any(|parameter| parameter.name_kind == ParameterNameKind::This)
        {
            return Completion::Deferred;
        }
        let mut resolved = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let ty = if let Some(annotation) = &parameter.annotation {
                self.resolve_type_node(file, scope, annotation, type_parameters)
            } else if let Some(initializer) = &parameter.initializer {
                completed!(self.signature_initializer_type(file, scope, initializer))
            } else {
                self.store.builtins.any
            };
            resolved.push(ParameterType {
                name: Some(parameter.name.clone()),
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        Completion::Complete(resolved)
    }

    pub(super) fn function_type(
        &mut self,
        id: DeclId,
        declaration: &FunctionDeclaration,
        scope: ScopeId,
    ) -> Completion<TypeId> {
        let type_parameters = self.function_type_parameters(id, declaration);
        let mut parameters = Vec::with_capacity(declaration.parameters.len());
        for parameter in &declaration.parameters {
            if parameter.annotation.is_some() && parameter.initializer.is_some() {
                return Completion::Deferred;
            }
            let ty = if let Some(annotation) = &parameter.annotation {
                if declaration.has_body && annotation.contains_type_query() {
                    return Completion::Deferred;
                }
                self.resolve_type_node(id.file, scope, annotation, &type_parameters)
            } else if let Some(initializer) = &parameter.initializer {
                completed!(self.signature_initializer_type(id.file, scope, initializer))
            } else {
                self.store.builtins.any
            };
            parameters.push(ParameterType {
                name: Some(parameter.name.clone()),
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        let return_type = if let Some(return_type) = &declaration.return_type {
            if declaration.has_body && return_type.contains_type_query() {
                return Completion::Deferred;
            }
            self.resolve_type_node(id.file, scope, return_type, &type_parameters)
        } else if !declaration.has_body {
            // Bodyless signatures recover with `any`; TS7010 owns strict diagnostics.
            self.store.builtins.any
        } else if declaration.declared || declaration.is_async {
            return Completion::Deferred;
        } else {
            completed!(self.infer_block_return(id.file, &declaration.body, scope))
        };
        Completion::Complete(self.store.function(
            (!declaration.type_parameters.is_empty()).then_some(id),
            javascript_signature_is_untyped(
                self.program.files[id.file.0 as usize].source.kind(),
                &declaration.parameters,
                declaration.type_parameters.is_empty(),
                declaration.return_type.is_none(),
                true,
            ),
            parameters,
            return_type,
        ))
    }

    fn enclosing_function_type_parameters(
        &mut self,
        file: FileId,
        scope: ScopeId,
    ) -> HashMap<String, TypeId> {
        let bindings = &self.program.files[file.0 as usize].bindings;
        let Some(owner) = bindings
            .scopes
            .get(scope.0 as usize)
            .and_then(|scope| scope.owner)
        else {
            return HashMap::new();
        };
        let Some(id) = bindings
            .declarations
            .iter()
            .find(|declaration| {
                declaration.owner == owner && declaration.kind == DeclarationKind::Function
            })
            .map(|declaration| declaration.id)
        else {
            return HashMap::new();
        };
        let Some(DeclarationModel::Function { declaration, .. }) = self.models.get(&id).copied()
        else {
            return HashMap::new();
        };
        self.function_type_parameters(id, declaration)
    }

    fn function_type_parameters(
        &mut self,
        id: DeclId,
        declaration: &FunctionDeclaration,
    ) -> HashMap<String, TypeId> {
        let mut type_parameters = HashMap::new();
        let mut seen = HashSet::new();
        for (index, parameter) in declaration.type_parameters.iter().enumerate() {
            let ty = self.store.type_parameter(id, index as u32, &parameter.name);
            if seen.insert(parameter.name.as_str()) {
                type_parameters.insert(parameter.name.clone(), ty);
            }
        }
        type_parameters
    }

    fn infer_block_return(
        &mut self,
        file: FileId,
        body: &[Statement],
        scope: ScopeId,
    ) -> Completion<TypeId> {
        let mut sites = Vec::new();
        let Some(definitely_returns) = collect_return_sites(body, &mut sites) else {
            return Completion::Deferred;
        };
        if sites.iter().any(|site| {
            !self
                .capabilities
                .semantic_check_node_is_claimed(file, site.statement.id)
        }) {
            return Completion::Deferred;
        }
        if sites.is_empty() || sites.iter().all(|site| site.expression.is_none()) {
            return Completion::Complete(self.store.builtins.void);
        }
        if !definitely_returns {
            return Completion::Deferred;
        }
        let mut return_types = Vec::with_capacity(sites.len());
        for site in sites {
            let Some(expression) = site.expression else {
                return_types.push(self.store.builtins.undefined);
                continue;
            };
            let expression_scope = self.program.files[file.0 as usize]
                .bindings
                .scope_for_node
                .get(&site.statement.id)
                .copied()
                .unwrap_or(scope);
            let inferred = self.infer_expression(file, expression_scope, expression, None);
            let Some(inferred) = self.complete_type(inferred) else {
                return Completion::Deferred;
            };
            if !bounded_inferred_return(self.store.kind(inferred)) {
                return Completion::Deferred;
            }
            return_types.push(self.widen(inferred));
        }
        Completion::Complete(self.store.union(return_types, UnionPolicy::Canonical))
    }
}

fn function_expression_has_authored_signature(function: &FunctionLikeExpression) -> bool {
    function.type_parameters.is_empty()
        && function.return_type.is_some()
        && function.parameters.iter().all(|parameter| {
            parameter.name_kind == ParameterNameKind::Binding && parameter.annotation.is_some()
        })
        && matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
}

fn javascript_signature_is_untyped(
    source_kind: SourceKind,
    parameters: &[Parameter],
    has_no_type_parameters: bool,
    has_no_return_type: bool,
    has_no_contextual_signature: bool,
) -> bool {
    matches!(
        source_kind,
        SourceKind::JavaScript | SourceKind::JavaScriptJsx
    ) && has_no_type_parameters
        && has_no_return_type
        && has_no_contextual_signature
        && parameters
            .iter()
            .all(|parameter| parameter.annotation.is_none())
}

const fn bounded_inferred_return(kind: &TypeKind) -> bool {
    matches!(
        kind,
        TypeKind::Array(_)
            | TypeKind::Tuple(_)
            | TypeKind::ClassInstance { .. }
            | non_recursive_type_kind!()
    ) && !matches!(kind, TypeKind::TypeParameter { .. })
}

fn collect_return_sites<'a>(
    statements: &'a [Statement],
    sites: &mut Vec<ReturnSite<'a>>,
) -> Option<bool> {
    let mut definitely_returns = false;
    for statement in statements {
        definitely_returns |= match &statement.kind {
            StatementKind::Return(expression) => {
                sites.push(ReturnSite {
                    statement,
                    expression: expression.as_ref(),
                });
                true
            }
            StatementKind::Block(statements) => collect_return_sites(statements, sites)?,
            StatementKind::If(control_flow) => {
                let then_returns = collect_return_sites(
                    std::slice::from_ref(control_flow.then_statement.as_ref()),
                    sites,
                )?;
                let else_returns = control_flow
                    .else_statement
                    .as_deref()
                    .map_or(Some(false), |statement| {
                        collect_return_sites(std::slice::from_ref(statement), sites)
                    })?;
                then_returns && else_returns
            }
            StatementKind::Switch(_) | StatementKind::Unknown => return None,
            StatementKind::Import(_)
            | StatementKind::Export(_)
            | StatementKind::Variable(_)
            | StatementKind::Function(_)
            | StatementKind::Class(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty => false,
        };
    }
    Some(definitely_returns)
}
