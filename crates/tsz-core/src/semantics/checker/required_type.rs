use std::collections::{HashMap, HashSet};

use crate::bind::{DeclarationKind, ScopeId};
use crate::program::{CapabilityScope, CapabilityTarget, SemanticCompletion};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind, TypeStore};
use crate::source::{DeclId, FileId, NodeId, SourceKind};
use crate::syntax::{
    AuthoredTypeItem, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind, Literal,
    Parameter, Statement, StatementKind, TypeMember, TypeMemberKind, TypeMemberName,
    TypeMemberNameKind, TypeNode, TypeNodeKind, TypeParameterDeclaration, UnaryOperator,
};

use super::{
    Checker,
    recursion::{ReferenceDemand, ReferenceExpansionStack},
    relation_diagnostic::ContextualType,
    type_member_grammar::ParameterGrammarHost,
};

mod operands;
mod program;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeMemberContainerKind {
    Interface,
    TypeLiteral,
}
impl Checker<'_> {
    pub(super) fn require_type_completion(&mut self, ty: TypeId) -> Completion<TypeId> {
        let mut active = HashSet::new();
        let mut references = ReferenceExpansionStack::new(ReferenceDemand::RequiredType);
        let completion = self.visit_required_type(ty, &mut active, &mut references);
        self.require_completion(completion)
    }
    pub(super) fn require_function_signature(&mut self, id: DeclId) -> ContextualType {
        let declaration_completion = self.declared_function_type(id);
        let signature_type = match self.require_completion(declaration_completion) {
            Completion::Complete(signature_type) => signature_type,
            Completion::Deferred | Completion::Cycle | Completion::Limit => {
                return ContextualType::Deferred;
            }
        };
        let signature_type = match self.require_type_completion(signature_type) {
            Completion::Complete(signature_type) => signature_type,
            Completion::Deferred | Completion::Cycle | Completion::Limit => {
                self.value_queries.remove(&id);
                return ContextualType::Deferred;
            }
        };
        let TypeKind::Function(signature) = self.store.kind(signature_type) else {
            return ContextualType::Absent;
        };
        ContextualType::Known(signature.return_type)
    }
    fn visit_required_statement_claimed(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        next_statement: Option<&Statement>,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        match &statement.kind {
            StatementKind::Import(_)
            | StatementKind::Export(_)
            | StatementKind::Variable(_)
            | StatementKind::If(_)
            | StatementKind::Switch(_)
            | StatementKind::Return(_)
            | StatementKind::Block(_)
            | StatementKind::Expression(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
            StatementKind::Function(declaration) => {
                let source = &self.program.files[file.0 as usize].source;
                let declaration_source = source.is_declaration_source();
                let javascript_source = matches!(
                    source.kind(),
                    SourceKind::JavaScript | SourceKind::JavaScriptJsx
                );
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Function,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(file, declaration.name_span.start));
                if !declaration.overload_context_is_recovery_free() {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                if javascript_source && !declaration.has_body {
                    // JavaScript overload syntax requires TS8017 rather than
                    // the TypeScript overload diagnostics modeled below.
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                if declaration.is_async || declaration.abstract_declaration {
                    // Async return validation (TS1064) and invalid abstract modifiers (TS1242)
                    // fail closed until their hosts are owned.
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                if declaration.bodyless_overload_is_recovery_free()
                    && !declaration.declared
                    && !declaration.abstract_declaration
                    && !declaration_source
                    && self.required_declaration_model_is_claimed(identity)
                {
                    match next_statement.map(|next| &next.kind) {
                        Some(StatementKind::Function(next)) if next.name == declaration.name => {}
                        Some(StatementKind::Function(next)) if next.has_body => {
                            self.push_diagnostic(
                                file,
                                next.name_span,
                                format!(
                                    "Function implementation name must be '{}'.",
                                    declaration.name
                                ),
                                2389,
                            );
                        }
                        _ => {
                            self.report_missing_function_implementation(file, declaration.name_span)
                        }
                    }
                }
                if declaration.bodyless_overload_is_recovery_free()
                    && declaration.return_type.is_none()
                    && !declaration.abstract_declaration
                    && self.options.effective_no_implicit_any()
                {
                    self.push_diagnostic(
                        file,
                        declaration.name_span,
                        format!(
                            "'{}', which lacks return-type annotation, implicitly has an 'any' return type.",
                            declaration.name
                        ),
                        7010,
                    );
                }
                if !declaration.has_body
                    && !declaration.declared
                    && !declaration.exported
                    && declaration_source
                {
                    // TS1046 is not owned yet. A declaration-file function
                    // without `declare` must not become a false Complete.
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                let function_scope = self.node_scope(file, statement.id, scope);
                if !self.declaration_value_host_is_modeled(identity, DeclarationKind::Function) {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                if declaration.has_body && (declaration.declared || declaration_source) {
                    // TS1183 and the surrounding ambient-host rules are not
                    // owned by the ordinary overload validator.
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                self.validate_function_overload_group(identity);
                let function_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                self.visit_type_parameter_declarations(
                    file,
                    scope,
                    &declaration.type_parameters,
                    &function_types,
                );
                self.visit_required_parameters(
                    file,
                    function_scope,
                    &declaration.parameters,
                    &function_types,
                    if declaration.has_body {
                        ParameterGrammarHost::Implementation { constructor: false }
                    } else {
                        ParameterGrammarHost::Signature
                    },
                );
                if let Some(return_type) = &declaration.return_type {
                    if declaration.has_body && return_type.contains_type_query() {
                        let _ = self.require_completion(Completion::<()>::Deferred);
                    } else {
                        self.visit_required_type_node(
                            file,
                            function_scope,
                            return_type,
                            &function_types,
                        );
                    }
                }
            }
            StatementKind::Class(declaration) => {
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Class,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(file, declaration.name_span.start));
                let source = &self.program.files[file.0 as usize].source;
                let declaration_source = source.is_declaration_source();
                let javascript_source = matches!(
                    source.kind(),
                    SourceKind::JavaScript | SourceKind::JavaScriptJsx
                );
                if !self.declaration_value_host_is_modeled(identity, DeclarationKind::Class)
                    || !self.is_single_type_symbol_declaration(identity)
                    || !class_member_declaration_groups_are_modeled(
                        &self.program.files[file.0 as usize].bindings,
                        declaration,
                    )
                    || javascript_source && class_has_bodyless_member(declaration)
                    || (declaration_source && !declaration.declared && !declaration.exported)
                    || (declaration.declared || declaration_source)
                        && class_has_ambient_implementation(declaration)
                {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                if !declaration.declared
                    && !declaration_source
                    && !self.class_bodyless_hosts_are_modeled(file, declaration)
                {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                let class_scope = self.node_scope(file, statement.id, scope);
                let class_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                self.visit_type_parameter_declarations(
                    file,
                    class_scope,
                    &declaration.type_parameters,
                    &class_types,
                );
                if let Some(heritage) = &declaration.extends {
                    self.visit_required_class_heritage(file, class_scope, heritage, &class_types);
                }
                for heritage in &declaration.implements {
                    self.visit_required_type_node(file, class_scope, heritage, &class_types);
                }
            }
            StatementKind::TypeAlias(declaration) => {
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::TypeAlias,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(file, declaration.name_span.start));
                let alias_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                self.visit_type_parameter_declarations(
                    file,
                    scope,
                    &declaration.type_parameters,
                    &alias_types,
                );
                self.visit_required_type_node(file, scope, &declaration.ty, &alias_types);
            }
            StatementKind::Interface(declaration) => {
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Interface,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(file, declaration.name_span.start));
                let interface_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                self.visit_type_parameter_declarations(
                    file,
                    scope,
                    &declaration.type_parameters,
                    &interface_types,
                );
                for heritage in &declaration.extends {
                    self.visit_required_type_node(file, scope, heritage, &interface_types);
                }
                self.validate_type_member_list(file, scope, &declaration.members, &interface_types);
                for member in &declaration.members {
                    self.visit_required_type_member(
                        file,
                        scope,
                        member,
                        &interface_types,
                        TypeMemberContainerKind::Interface,
                    );
                }
            }
        }
    }

    fn visit_required_class_member(
        &mut self,
        file: FileId,
        class_scope: ScopeId,
        member_scope: ScopeId,
        member: &crate::syntax::ClassMember,
        type_parameters: &HashMap<String, TypeId>,
    ) -> bool {
        if !self
            .capabilities
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file, member.id),
            )
            .is_claimed()
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
            return false;
        }
        match &member.kind {
            ClassMemberKind::Property { annotation, .. } => {
                if let Some(annotation) = annotation {
                    self.visit_required_type_node(file, class_scope, annotation, type_parameters);
                }
            }
            ClassMemberKind::Constructor {
                parameters,
                has_body,
                ..
            } => {
                self.visit_required_parameters(
                    file,
                    member_scope,
                    parameters,
                    type_parameters,
                    if *has_body {
                        ParameterGrammarHost::Implementation { constructor: true }
                    } else {
                        ParameterGrammarHost::Signature
                    },
                );
            }
            ClassMemberKind::Method {
                type_parameters: declarations,
                parameters,
                return_type,
                has_body,
                ..
            } => {
                self.visit_type_parameter_declarations(
                    file,
                    class_scope,
                    declarations,
                    type_parameters,
                );
                self.visit_required_parameters(
                    file,
                    member_scope,
                    parameters,
                    type_parameters,
                    if *has_body {
                        ParameterGrammarHost::Implementation { constructor: false }
                    } else {
                        ParameterGrammarHost::Signature
                    },
                );
                if let Some(return_type) = return_type {
                    if *has_body && return_type.contains_type_query() {
                        let _ = self.require_completion(Completion::<()>::Deferred);
                    } else {
                        self.visit_required_type_node(
                            file,
                            member_scope,
                            return_type,
                            type_parameters,
                        );
                    }
                }
            }
        }
        true
    }

    fn class_bodyless_hosts_are_modeled(
        &mut self,
        file: FileId,
        declaration: &crate::syntax::ClassDeclaration,
    ) -> bool {
        let has_bodyless = declaration.members.iter().any(|member| {
            matches!(
                member.kind,
                ClassMemberKind::Constructor {
                    has_body: false,
                    ..
                } | ClassMemberKind::Method {
                    has_body: false,
                    ..
                }
            )
        });
        if !has_bodyless {
            return true;
        }
        if declaration.abstract_class || !declaration.type_parameters.is_empty() {
            return false;
        }

        // `check_class` skips bodies. Only ordinary, nongeneric overload syntax with an empty
        // body is promoted; every other class host remains deferred pending a body summary.
        for member in &declaration.members {
            let supported_modifiers = !member.modifiers.readonly
                && !member.modifiers.abstract_member
                && !member.modifiers.declared
                && !member.modifiers.async_member
                && !member.modifiers.unsupported_for_overload_completion
                && !(member.modifiers.public && member.modifiers.protected)
                && !(member.modifiers.public && member.modifiers.private)
                && !(member.modifiers.protected && member.modifiers.private);
            match &member.kind {
                ClassMemberKind::Constructor {
                    parameters,
                    body,
                    has_body,
                    ..
                } => {
                    if !supported_modifiers
                        || !member.overload_context_is_recovery_free()
                        || member.modifiers.static_member
                        || (*has_body && !body.is_empty())
                        || !bounded_class_parameters(parameters, self.options)
                    {
                        return false;
                    }
                }
                ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    has_body,
                    accessor,
                    ..
                } => {
                    if !supported_modifiers
                        || !member.overload_context_is_recovery_free()
                        || accessor.is_some()
                        || !type_parameters.is_empty()
                        || (*has_body && !body.is_empty())
                        || (!*has_body
                            && return_type.is_none()
                            && self.options.effective_no_implicit_any())
                        || !bounded_class_parameters(parameters, self.options)
                    {
                        return false;
                    }
                }
                ClassMemberKind::Property { .. } => return false,
            }
        }

        for (index, member) in declaration.members.iter().enumerate() {
            match &member.kind {
                ClassMemberKind::Constructor {
                    has_body: false, ..
                } => {
                    let implementations = declaration.members[index + 1..]
                        .iter()
                        .take_while(|next| matches!(next.kind, ClassMemberKind::Constructor { .. }))
                        .filter(|next| {
                            matches!(
                                next.kind,
                                ClassMemberKind::Constructor { has_body: true, .. }
                            )
                        })
                        .collect::<Vec<_>>();
                    match implementations.as_slice() {
                        [] => {}
                        [implementation] => {
                            if class_member_access(&member.modifiers)
                                != class_member_access(&implementation.modifiers)
                                || !self.class_overload_is_compatibly_modeled(
                                    file,
                                    member,
                                    implementation,
                                )
                            {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                }
                ClassMemberKind::Method {
                    has_body: false,
                    accessor: None,
                    ..
                } => {
                    let implementations = declaration.members[index + 1..]
                        .iter()
                        .take_while(|next| {
                            matches!(
                                &next.kind,
                                ClassMemberKind::Method { accessor: None, .. }
                                    if next.name == member.name
                            )
                        })
                        .filter(|next| {
                            matches!(next.kind, ClassMemberKind::Method { has_body: true, .. })
                        })
                        .collect::<Vec<_>>();
                    match implementations.as_slice() {
                        [] => {}
                        [implementation] => {
                            if class_member_access(&member.modifiers)
                                != class_member_access(&implementation.modifiers)
                                || member.modifiers.static_member
                                    != implementation.modifiers.static_member
                                || !self.class_overload_is_compatibly_modeled(
                                    file,
                                    member,
                                    implementation,
                                )
                            {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                }
                ClassMemberKind::Constructor { .. }
                | ClassMemberKind::Property { .. }
                | ClassMemberKind::Method { .. } => {}
            }
        }
        true
    }

    fn visit_type_parameter_declarations(
        &mut self,
        file: FileId,
        scope: ScopeId,
        declarations: &[TypeParameterDeclaration],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let mut occurrences = HashMap::<&str, usize>::new();
        for declaration in declarations {
            *occurrences.entry(declaration.name.as_str()).or_default() += 1;
        }
        if occurrences.values().any(|count| *count > 1) {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
        for (index, declaration) in declarations.iter().enumerate() {
            if let Some(constraint) = &declaration.constraint {
                self.visit_required_type_node(file, scope, constraint, type_parameters);
            }
            if let Some(default) = &declaration.default {
                let forbidden = declarations[index..]
                    .iter()
                    .filter(|declaration| occurrences[declaration.name.as_str()] == 1)
                    .filter_map(|declaration| type_parameters.get(&declaration.name).copied())
                    .collect();
                self.forbidden_default_type_parameters.push(forbidden);
                self.visit_required_type_node(file, scope, default, type_parameters);
                self.forbidden_default_type_parameters.pop();
            }
        }
    }

    fn visit_required_parameters(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
        host: ParameterGrammarHost,
    ) {
        self.validate_authored_parameters(file, scope, parameters, type_parameters);
        self.validate_parameter_host_grammar(file, parameters, host);
        let implementation = matches!(host, ParameterGrammarHost::Implementation { .. });
        self.visit_required_parameter_types(
            file,
            scope,
            parameters,
            type_parameters,
            implementation,
        );
        if implementation {
            for parameter in parameters {
                if let Some(initializer) = &parameter.initializer
                    && matches!(
                        initializer.kind,
                        ExpressionKind::Literal(Literal::BigInt(_))
                    )
                {
                    let completion = self.signature_initializer_type(file, scope, initializer);
                    let _ = self.require_completion(completion);
                }
            }
        }
    }

    fn visit_required_parameter_types(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
        implementation: bool,
    ) {
        for parameter in parameters {
            if let Some(annotation) = &parameter.annotation {
                if implementation && annotation.contains_type_query() {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                } else {
                    self.visit_required_type_node(file, scope, annotation, type_parameters);
                }
            }
        }
    }

    fn visit_required_type_node(
        &mut self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        self.required_type_contexts
            .entry(node.span)
            .or_insert_with(|| type_parameters.clone());
        let ty = self.resolve_type_node(file, scope, node, type_parameters);
        if let TypeNodeKind::Reference { arguments, .. } = &node.kind
            && let TypeKind::Deferred(DeferredType::Reference {
                declaration,
                arguments: resolved_arguments,
            }) = self.store.kind(ty).clone()
            && let Completion::Complete(Err(failure)) =
                self.record_key_constraint_check(declaration, &resolved_arguments, 0)
            && let Some(argument) = arguments.get(failure.argument_index)
        {
            self.report_constraint_failure(failure.reason, argument.span);
        }
        let completion = self.require_type_completion(ty);
        if matches!(completion, Completion::Complete(_)) {
            self.complete_required_type_nodes.insert(node.span);
        }
        match &node.kind {
            TypeNodeKind::Object(members) => {
                if members.iter().any(|member| member.recovered) {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                self.validate_type_member_list(file, scope, members, type_parameters);
                for member in members {
                    self.visit_required_type_member(
                        file,
                        scope,
                        member,
                        type_parameters,
                        TypeMemberContainerKind::TypeLiteral,
                    );
                }
                return;
            }
            TypeNodeKind::Function {
                id: signature_id,
                type_parameters: signature_type_parameters,
                parameters,
                return_type,
                ..
            }
            | TypeNodeKind::Constructor {
                id: signature_id,
                type_parameters: signature_type_parameters,
                parameters,
                return_type,
                ..
            } => {
                let signature_scope = self.node_scope(file, *signature_id, scope);
                let signature_types = if signature_type_parameters.is_empty() {
                    type_parameters.clone()
                } else {
                    let identity = self.program.files[file.0 as usize]
                        .bindings
                        .anonymous_signatures
                        .get(signature_id)
                        .copied();
                    let Some(identity) = identity else {
                        let _ = self.require_completion(Completion::<()>::Deferred);
                        return;
                    };
                    self.extend_type_parameters(
                        identity,
                        signature_type_parameters,
                        type_parameters,
                    )
                };
                self.validate_implicit_any_parameters(file, parameters);
                self.visit_required_parameters(
                    file,
                    signature_scope,
                    parameters,
                    &signature_types,
                    ParameterGrammarHost::Signature,
                );
                for initializer in parameters
                    .iter()
                    .filter_map(|parameter| parameter.initializer.as_ref())
                {
                    self.visit_required_expression(
                        file,
                        signature_scope,
                        initializer,
                        &signature_types,
                    );
                }
                self.visit_type_parameter_declarations(
                    file,
                    scope,
                    signature_type_parameters,
                    &signature_types,
                );
                self.visit_required_type_node(file, signature_scope, return_type, &signature_types);
                return;
            }
            TypeNodeKind::Reference {
                name, arguments, ..
            } => {
                let applied_type_parameter =
                    !arguments.is_empty() && type_parameters.contains_key(name);
                if !applied_type_parameter
                    && let Completion::Complete(resolved) = completion
                    && self
                        .forbidden_default_type_parameters
                        .iter()
                        .any(|forbidden| forbidden.contains(&resolved))
                {
                    self.push_diagnostic(
                        file,
                        node.span,
                        "Type parameter defaults can only reference previously declared type parameters."
                            .to_string(),
                        2744,
                    );
                }
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                self.visit_required_type_node(file, scope, check_type, type_parameters);
                self.visit_required_type_node(file, scope, extends_type, type_parameters);
                let true_parameters =
                    self.conditional_true_type_parameters(file, extends_type, type_parameters);
                self.visit_required_type_node(file, scope, true_type, &true_parameters);
                self.visit_required_type_node(file, scope, false_type, type_parameters);
                return;
            }
            TypeNodeKind::Mapped {
                parameter,
                parameter_span,
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                self.visit_required_type_node(file, scope, constraint, type_parameters);
                let mut mapped_types = type_parameters.clone();
                let ty = self.store.type_parameter(
                    synthetic_identity(file, parameter_span.start),
                    0,
                    parameter,
                );
                mapped_types.insert(parameter.clone(), ty);
                if let Some(name_type) = name_type {
                    self.visit_required_type_node(file, scope, name_type, &mapped_types);
                }
                self.visit_required_type_node(file, scope, value_type, &mapped_types);
                self.validate_mapped_type_members(file, members);
                return;
            }
            _ => {}
        }
        let mut children = Vec::new();
        node.push_authored_children(&mut children);
        for child in children.into_iter().rev() {
            let AuthoredTypeItem::Type(child, _) = child else {
                unreachable!()
            };
            self.visit_required_type_node(file, scope, child, type_parameters);
        }
    }

    fn visit_required_type_member(
        &mut self,
        file: FileId,
        scope: ScopeId,
        member: &TypeMember,
        type_parameters: &HashMap<String, TypeId>,
        container: TypeMemberContainerKind,
    ) {
        if member.recovered {
            return;
        }
        let member_scope = self.node_scope(file, member.id, scope);
        let member_name = match &member.kind {
            TypeMemberKind::Property { name, .. }
            | TypeMemberKind::Method { name, .. }
            | TypeMemberKind::Accessor { name, .. } => Some(name),
            TypeMemberKind::Call { .. }
            | TypeMemberKind::Construct { .. }
            | TypeMemberKind::Index { .. } => None,
        };
        if let Some(TypeMemberName {
            kind: TypeMemberNameKind::BigIntLiteral(_),
            span,
        }) = member_name
        {
            self.push_diagnostic(
                file,
                *span,
                "A 'bigint' literal cannot be used as a property name.".to_string(),
                1539,
            );
        }
        if let Some(name) = member_name
            && let TypeMemberNameKind::Computed(expression) = &name.kind
        {
            if !is_bindable_computed_name(expression) {
                let (code, container_phrase) = match container {
                    TypeMemberContainerKind::Interface => (1169, "an interface"),
                    TypeMemberContainerKind::TypeLiteral => (1170, "a type literal"),
                };
                self.push_diagnostic(
                    file,
                    name.span,
                    format!(
                        "A computed property name in {container_phrase} must refer to an expression whose type is a literal type or a 'unique symbol' type."
                    ),
                    code,
                );
            }
            if container == TypeMemberContainerKind::TypeLiteral
                || !matches!(&expression.peel_parentheses().kind, ExpressionKind::This)
            {
                self.infer_expression(file, scope, expression, None);
            }
        }
        match &member.kind {
            TypeMemberKind::Property {
                name,
                ty,
                optional,
                initializer,
            } => {
                if let Some(initializer) = initializer
                    && (ty.is_some()
                        || *optional
                        || matches!(name.kind, TypeMemberNameKind::Computed(_)))
                {
                    let (code, message) = match container {
                        TypeMemberContainerKind::Interface => {
                            (1246, "An interface property cannot have an initializer.")
                        }
                        TypeMemberContainerKind::TypeLiteral => {
                            (1247, "A type literal property cannot have an initializer.")
                        }
                    };
                    self.push_diagnostic(file, initializer.span, message.to_string(), code);
                    self.infer_expression(file, member_scope, initializer, None);
                }
                if let Some(ty) = ty {
                    self.visit_required_type_node(file, member_scope, ty, type_parameters);
                }
            }
            TypeMemberKind::Method {
                type_parameters: member_type_parameters,
                parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Call {
                type_parameters: member_type_parameters,
                parameters,
                return_type,
            }
            | TypeMemberKind::Construct {
                type_parameters: member_type_parameters,
                parameters,
                return_type,
            } => {
                let identity = self.program.files[file.0 as usize]
                    .bindings
                    .type_members
                    .get(&member.id)
                    .map(|bound| bound.declaration);
                let member_types = if let Some(identity) = identity {
                    self.extend_type_parameters(identity, member_type_parameters, type_parameters)
                } else {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                    type_parameters.clone()
                };
                self.validate_authored_parameters(file, member_scope, parameters, &member_types);
                self.validate_parameter_host_grammar(
                    file,
                    parameters,
                    ParameterGrammarHost::Signature,
                );
                self.validate_implicit_any_parameters(file, parameters);
                self.visit_type_parameter_declarations(
                    file,
                    scope,
                    member_type_parameters,
                    &member_types,
                );
                self.visit_required_parameter_types(
                    file,
                    member_scope,
                    parameters,
                    &member_types,
                    false,
                );
                if let Some(return_type) = return_type {
                    self.visit_required_type_node(file, member_scope, return_type, &member_types);
                }
            }
            TypeMemberKind::Accessor {
                parameters,
                return_type,
                ..
            } => {
                self.validate_authored_parameters(file, member_scope, parameters, type_parameters);
                self.validate_parameter_host_grammar(
                    file,
                    parameters,
                    ParameterGrammarHost::Signature,
                );
                self.validate_implicit_any_parameters(file, parameters);
                self.visit_required_parameter_types(
                    file,
                    member_scope,
                    parameters,
                    type_parameters,
                    false,
                );
                if let Some(return_type) = return_type {
                    self.visit_required_type_node(file, member_scope, return_type, type_parameters);
                }
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                // Index signatures use their own short-circuit grammar owner;
                // ordinary rest-parameter grammar must not add cascades.
                self.validate_implicit_any_parameters(file, parameters);
                self.visit_required_parameter_types(
                    file,
                    member_scope,
                    parameters,
                    type_parameters,
                    false,
                );
                if let Some(value_type) = value_type {
                    self.visit_required_type_node(file, member_scope, value_type, type_parameters);
                }
            }
        }
    }

    fn extend_type_parameters(
        &mut self,
        identity: DeclId,
        declarations: &[TypeParameterDeclaration],
        outer: &HashMap<String, TypeId>,
    ) -> HashMap<String, TypeId> {
        let mut parameters = outer.clone();
        let mut seen = HashSet::new();
        for (index, declaration) in declarations.iter().enumerate() {
            let ty = self
                .store
                .type_parameter(identity, index as u32, &declaration.name);
            if seen.insert(declaration.name.as_str()) {
                parameters.insert(declaration.name.clone(), ty);
            }
        }
        parameters
    }

    /// `infer` declarations belong to the surrounding conditional's extends
    /// pattern and are visible only in that conditional's true branch. The
    /// identity comes from the declaration span, so ordinary resolution and
    /// the required-type prepass meet the same symbolic parameter.
    pub(super) fn conditional_true_type_parameters(
        &mut self,
        file: FileId,
        extends_type: &TypeNode,
        outer: &HashMap<String, TypeId>,
    ) -> HashMap<String, TypeId> {
        let mut parameters = outer.clone();
        extends_type.for_each_conditional_infer(&mut |name, name_span| {
            let ty = self
                .store
                .type_parameter(synthetic_identity(file, name_span.start), 0, name);
            parameters.insert(name.to_string(), ty);
        });
        parameters
    }

    pub(super) fn node_scope(&self, file: FileId, node: NodeId, fallback: ScopeId) -> ScopeId {
        self.program.files[file.0 as usize]
            .bindings
            .scope_for_node
            .get(&node)
            .copied()
            .unwrap_or(fallback)
    }

    fn visit_required_type(
        &mut self,
        ty: TypeId,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
    ) -> Completion<TypeId> {
        if !active.insert(ty) {
            return Completion::Complete(ty);
        }
        let completion = match self.store.kind(ty).clone() {
            TypeKind::Deferred(deferred) => {
                self.visit_required_deferred(ty, deferred, active, references)
            }
            TypeKind::Error | TypeKind::Invalid(_) | non_recursive_type_kind!() => {
                Completion::Complete(ty)
            }
            kind => {
                let mut children = Vec::new();
                TypeStore::push_type_children(&kind, &mut children);
                self.visit_required_children(ty, children, active, references)
            }
        };
        active.remove(&ty);
        completion
    }

    fn visit_required_deferred(
        &mut self,
        ty: TypeId,
        deferred: DeferredType,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
    ) -> Completion<TypeId> {
        let mut state = SemanticCompletion::Complete;
        self.visit_deferred_operands(&deferred, active, references, &mut state);

        // Authored symbolic owners validate their operands here; a later
        // relation or inference demand remains responsible for forcing them.
        let generic_projection = matches!(
            &deferred,
            DeferredType::KeyOf(operand) | DeferredType::IndexedAccess { object: operand, .. }
                if matches!(self.store.kind(*operand), TypeKind::TypeParameter { .. })
        );
        if generic_projection
            || matches!(
                &deferred,
                DeferredType::Conditional { .. }
                    | DeferredType::Mapped { .. }
                    | DeferredType::Predicate {
                        parameter_is_bound: true,
                        ..
                    }
            )
        {
            return completion_from_state(state, ty);
        }

        let reference = match &deferred {
            DeferredType::Reference {
                declaration,
                arguments,
            } => Some((*declaration, arguments.as_slice())),
            _ => None,
        };
        // A reference cannot be instantiated definitively while one of its
        // authored arguments is incomplete. Propagate that operand state
        // before expansion so fresh opaque arguments cannot evade recursion
        // identity and restart forcing indefinitely.
        if reference.is_some() && !state.is_complete() {
            self.force_queries.remove(&ty);
            return completion_from_state(state, ty);
        }
        if let Some((declaration, arguments)) = reference
            && let Some(expansion) =
                references.generative_expansion(ty, declaration, arguments, &|ty| {
                    self.store.kind(ty).clone()
                })
        {
            // The arguments were required above. Only a supported sole
            // interface origin may keep the growing edge symbolic; an
            // unsupported generative revisit is a typed nonclaim and must not
            // be forced or cached.
            return if self.generative_reference_supported(declaration, arguments)
                && references.expansion_segment_supports(
                    &expansion,
                    |frame_declaration, frame_arguments| {
                        self.reference_expansion_frame_supported(frame_declaration, frame_arguments)
                    },
                ) {
                completion_from_state(state, ty)
            } else {
                completion_from_state(state.combine(SemanticCompletion::Deferred), ty)
            };
        }

        let checkpoint = references.checkpoint();
        if let Some((declaration, arguments)) = reference {
            references.push(ty, declaration, arguments);
        }
        let mut resolved = ty;
        match self.force_type(ty, 0) {
            Completion::Complete(forced) => {
                resolved = forced;
                state = state.combine(super::capabilities::completion_state(
                    &self.visit_required_type(forced, active, references),
                ));
            }
            Completion::Deferred => state = state.combine(SemanticCompletion::Deferred),
            Completion::Cycle => state = state.combine(SemanticCompletion::Cycle),
            Completion::Limit => state = state.combine(SemanticCompletion::Limit),
        }
        references.restore(checkpoint);
        let completion = completion_from_state(state, resolved);
        if !matches!(completion, Completion::Complete(_)) {
            self.force_queries.remove(&ty);
        }
        completion
    }

    fn visit_required_children(
        &mut self,
        owner: TypeId,
        children: impl IntoIterator<Item = TypeId>,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
    ) -> Completion<TypeId> {
        let mut state = SemanticCompletion::Complete;
        self.combine_required_children(children, active, references, &mut state);
        completion_from_state(state, owner)
    }

    fn combine_required_children(
        &mut self,
        children: impl IntoIterator<Item = TypeId>,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
        state: &mut SemanticCompletion,
    ) {
        for child in children {
            let completion = self.visit_required_type(child, active, references);
            *state = state.combine(super::capabilities::completion_state(&completion));
        }
    }
}

fn bounded_class_parameters(
    parameters: &[Parameter],
    options: &crate::program::CompilerOptions,
) -> bool {
    parameters.iter().all(|parameter| {
        !parameter.rest
            && parameter.initializer.is_none()
            && parameter.modifiers.is_empty()
            && parameter.overload_context_is_recovery_free()
            && (!options.effective_no_implicit_any() || parameter.annotation.is_some())
    })
}

fn class_has_bodyless_member(declaration: &ClassDeclaration) -> bool {
    declaration.members.iter().any(|member| {
        matches!(
            member.kind,
            ClassMemberKind::Constructor {
                has_body: false,
                ..
            } | ClassMemberKind::Method {
                has_body: false,
                ..
            }
        )
    })
}

fn class_member_declaration_groups_are_modeled(
    bound: &crate::bind::BoundFile,
    declaration: &ClassDeclaration,
) -> bool {
    let mut checked = HashSet::<DeclId>::new();
    let mut uncanonical_members = 0;
    for member in &declaration.members {
        let Some(group) = bound.class_member_group(member.id) else {
            if matches!(member.kind, ClassMemberKind::Constructor { .. })
                || member.overload_context_is_recovery_free()
            {
                continue;
            }
            uncanonical_members += 1;
            if uncanonical_members >= 2 {
                return false;
            }
            continue;
        };
        let Some(canonical) = group.first().copied() else {
            return false;
        };
        if !checked.insert(canonical) {
            continue;
        }
        let Some(facts) = bound.class_member_group_facts(member.id) else {
            return false;
        };
        if if matches!(member.kind, ClassMemberKind::Constructor { .. }) {
            facts.implementations > 1
        } else if facts.callables > 0 {
            facts.implementations > 1
                || facts.getters > 0
                || facts.setters > 0
                || facts.properties > 0
        } else if facts.properties > 0 {
            facts.properties > 1 || facts.getters > 0 || facts.setters > 0
        } else {
            facts.getters > 1 || facts.setters > 1
        } {
            return false;
        }
    }
    true
}

fn class_has_ambient_implementation(declaration: &ClassDeclaration) -> bool {
    declaration.members.iter().any(|member| match &member.kind {
        ClassMemberKind::Constructor { has_body, .. }
        | ClassMemberKind::Method { has_body, .. } => *has_body,
        ClassMemberKind::Property { initializer, .. } => initializer.is_some(),
    })
}

const fn class_member_access(modifiers: &crate::syntax::ClassMemberModifiers) -> u8 {
    modifiers.private as u8 + ((!modifiers.private && modifiers.protected) as u8) * 2
}

fn is_bindable_computed_name(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Identifier { entity_name, .. } => *entity_name,
        ExpressionKind::Member { object, .. } => is_entity_name_expression(object),
        ExpressionKind::Literal(
            Literal::String(_) | Literal::NoSubstitutionTemplate(_) | Literal::Number(_),
        ) => true,
        ExpressionKind::Unary {
            operator: UnaryOperator::Plus | UnaryOperator::Minus,
            operand,
        } => matches!(operand.kind, ExpressionKind::Literal(Literal::Number(_))),
        _ => false,
    }
}

fn is_entity_name_expression(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Identifier { entity_name, .. } => *entity_name,
        ExpressionKind::Member { object, .. } => is_entity_name_expression(object),
        _ => false,
    }
}

const fn synthetic_identity(file: FileId, start: u32) -> DeclId {
    DeclId {
        file,
        local: start | (1 << 31),
    }
}
const fn completion_from_state(state: SemanticCompletion, ty: TypeId) -> Completion<TypeId> {
    match state {
        SemanticCompletion::Complete => Completion::Complete(ty),
        SemanticCompletion::Deferred => Completion::Deferred,
        SemanticCompletion::Cycle => Completion::Cycle,
        SemanticCompletion::Limit => Completion::Limit,
    }
}
