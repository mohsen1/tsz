use std::collections::{HashMap, HashSet};

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::program::{CapabilityScope, CapabilityTarget, SemanticCompletion};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};
use crate::source::{DeclId, FileId, NodeId, SourceKind};
use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind, Literal,
    Parameter, ParameterNameKind, Statement, StatementKind, SwitchClauseKind, TypeMember,
    TypeMemberKind, TypeMemberName, TypeMemberNameKind, TypeNode, TypeNodeKind,
    TypeParameterDeclaration, UnaryOperator,
};

use super::{
    Checker, is_declaration_source,
    recursion::{ReferenceDemand, ReferenceExpansionStack},
    relation_diagnostic::ContextualType,
    type_member_grammar::ParameterGrammarHost,
};

mod operands;
mod program;

#[derive(Debug, Clone, Copy)]
enum TypeMemberContainerKind {
    Interface,
    TypeLiteral,
}

impl Checker<'_> {
    /// Require a type at a declaration boundary.
    ///
    /// A concrete object, signature, tuple, or union can contain deferred components.
    /// Its program-local `TypeId` active set closes productive recursion coinductively without a
    /// second depth or fuel policy. Deferred evaluation keeps its own budget.
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
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
            StatementKind::Export(declaration) => {
                if let Some(assignment) = &declaration.assignment {
                    self.visit_required_expression(file, scope, assignment, type_parameters);
                }
            }
            StatementKind::Variable(declaration) => {
                if let Some(annotation) = &declaration.annotation {
                    self.visit_required_type_node(file, scope, annotation, type_parameters);
                }
                if let Some(initializer) = &declaration.initializer {
                    self.visit_required_expression(file, scope, initializer, type_parameters);
                }
            }
            StatementKind::Function(declaration) => {
                let source = &self.program.files[file.0 as usize].source;
                let declaration_source = is_declaration_source(&source.path);
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
                self.visit_required_body(file, function_scope, &declaration.body, &function_types);
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
                let declaration_source = is_declaration_source(&source.path);
                let javascript_source = matches!(
                    source.kind(),
                    SourceKind::JavaScript | SourceKind::JavaScriptJsx
                );
                if !self.declaration_value_host_is_modeled(identity, DeclarationKind::Class)
                    || !self.is_single_type_symbol_declaration(identity)
                    || class_has_multiple_constructor_implementations(declaration)
                    || !class_member_declaration_groups_are_modeled(declaration)
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
                for member in &declaration.members {
                    self.visit_required_class_member(file, class_scope, member, &class_types);
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
            StatementKind::If(control_flow) => {
                self.visit_required_expression(
                    file,
                    scope,
                    &control_flow.condition,
                    type_parameters,
                );
                let then_scope = self.node_scope(file, control_flow.then_statement.id, scope);
                self.visit_required_statement(
                    file,
                    then_scope,
                    &control_flow.then_statement,
                    None,
                    type_parameters,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = self.node_scope(file, else_statement.id, scope);
                    self.visit_required_statement(
                        file,
                        else_scope,
                        else_statement,
                        None,
                        type_parameters,
                    );
                }
            }
            StatementKind::Switch(control_flow) => {
                let switch_scope = self.node_scope(file, statement.id, scope);
                self.visit_required_expression(
                    file,
                    switch_scope,
                    &control_flow.expression,
                    type_parameters,
                );
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.visit_required_expression(
                            file,
                            switch_scope,
                            expression,
                            type_parameters,
                        );
                    }
                    for (index, nested) in clause.statements.iter().enumerate() {
                        let nested_scope = self.node_scope(file, nested.id, switch_scope);
                        self.visit_required_statement(
                            file,
                            nested_scope,
                            nested,
                            clause.statements.get(index + 1),
                            type_parameters,
                        );
                    }
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.visit_required_expression(file, scope, expression, type_parameters);
                }
            }
            StatementKind::Block(statements) => {
                for (index, nested) in statements.iter().enumerate() {
                    let nested_scope = self.node_scope(file, nested.id, scope);
                    self.visit_required_statement(
                        file,
                        nested_scope,
                        nested,
                        statements.get(index + 1),
                        type_parameters,
                    );
                }
            }
            StatementKind::Expression(expression) => {
                self.visit_required_expression(file, scope, expression, type_parameters);
            }
        }
    }

    fn visit_required_class_member(
        &mut self,
        file: FileId,
        class_scope: ScopeId,
        member: &crate::syntax::ClassMember,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        if !self
            .capabilities
            .claim(
                CapabilityTarget::RequiredType,
                CapabilityScope::node(file, member.id),
            )
            .is_claimed()
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
            return;
        }
        match &member.kind {
            ClassMemberKind::Property {
                annotation,
                initializer,
                ..
            } => {
                if let Some(annotation) = annotation {
                    self.visit_required_type_node(file, class_scope, annotation, type_parameters);
                }
                if let Some(initializer) = initializer {
                    self.visit_required_expression(file, class_scope, initializer, type_parameters);
                }
            }
            ClassMemberKind::Constructor {
                parameters,
                body,
                has_body,
                ..
            } => {
                let member_scope = self.node_scope(file, member.id, class_scope);
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
                self.visit_required_body(file, member_scope, body, type_parameters);
            }
            ClassMemberKind::Method {
                type_parameters: declarations,
                parameters,
                return_type,
                body,
                has_body,
                ..
            } => {
                let member_scope = self.node_scope(file, member.id, class_scope);
                let method_types = self.extend_type_parameters(
                    synthetic_identity(file, member.name_span.start),
                    declarations,
                    type_parameters,
                );
                self.visit_type_parameter_declarations(
                    file,
                    class_scope,
                    declarations,
                    &method_types,
                );
                self.visit_required_parameters(
                    file,
                    member_scope,
                    parameters,
                    &method_types,
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
                            &method_types,
                        );
                    }
                }
                self.visit_required_body(file, member_scope, body, &method_types);
            }
        }
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

    fn visit_required_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        match &expression.kind {
            ExpressionKind::Identifier { .. }
            | ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.visit_required_expression(file, scope, &property.value, type_parameters);
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.visit_required_expression(file, scope, element, type_parameters);
                }
            }
            ExpressionKind::Call {
                callee,
                type_arguments: _,
                arguments,
            } => {
                self.visit_required_expression(file, scope, callee, type_parameters);
                // Explicit call type arguments are bound and navigable syntax,
                // but generic signature instantiation is not yet an owned
                // semantic query. Resolving them here can emit the wrong
                // diagnostic family (for example TS2304 instead of TS2749),
                // so the call itself supplies the typed Deferred boundary.
                for argument in arguments {
                    self.visit_required_expression(file, scope, argument, type_parameters);
                }
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                self.visit_required_expression(file, scope, callee, type_parameters);
                for type_argument in type_arguments {
                    self.visit_required_type_node(file, scope, type_argument, type_parameters);
                }
                for argument in arguments {
                    self.visit_required_expression(file, scope, argument, type_parameters);
                }
            }
            ExpressionKind::Member { object, .. } => {
                self.visit_required_expression(file, scope, object, type_parameters);
            }
            ExpressionKind::ElementAccess { object, index } => {
                self.visit_required_expression(file, scope, object, type_parameters);
                self.visit_required_expression(file, scope, index, type_parameters);
            }
            ExpressionKind::FunctionLike(function) => self.visit_required_function_like_expression(
                file,
                scope,
                expression,
                function,
                type_parameters,
            ),
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right, .. } => {
                self.visit_required_expression(file, scope, left, type_parameters);
                self.visit_required_expression(file, scope, right, type_parameters);
            }
            ExpressionKind::Unary { operand, .. } | ExpressionKind::Parenthesized(operand) => {
                self.visit_required_expression(file, scope, operand, type_parameters);
            }
            ExpressionKind::As { expression, ty } => {
                self.visit_required_expression(file, scope, expression, type_parameters);
                self.visit_required_type_node(file, scope, ty, type_parameters);
            }
        }
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
        for parameter in parameters {
            if let Some(initializer) = &parameter.initializer
                && !matches!(initializer.kind, ExpressionKind::Literal(_))
            {
                self.visit_required_expression(file, scope, initializer, type_parameters);
            }
        }
        if implementation {
            for parameter in parameters {
                if let Some(initializer) = &parameter.initializer
                    && matches!(initializer.kind, ExpressionKind::Literal(_))
                {
                    if matches!(
                        initializer.kind,
                        ExpressionKind::Literal(Literal::BigInt(_))
                    ) {
                        let completion = self.signature_initializer_type(file, scope, initializer);
                        let _ = self.require_completion(completion);
                    } else {
                        self.visit_required_expression(file, scope, initializer, type_parameters);
                    }
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
        let completion = self.require_type_completion(ty);
        if matches!(completion, Completion::Complete(_)) {
            self.complete_required_type_nodes.insert(node.span);
        }
        match &node.kind {
            TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Missing => {}
            TypeNodeKind::Array(child)
            | TypeNodeKind::KeyOf(child)
            | TypeNodeKind::Readonly(child)
            | TypeNodeKind::Parenthesized(child) => {
                self.visit_required_type_node(file, scope, child, type_parameters);
            }
            TypeNodeKind::Tuple(children)
            | TypeNodeKind::Union(children)
            | TypeNodeKind::Intersection(children) => {
                for child in children {
                    self.visit_required_type_node(file, scope, child, type_parameters);
                }
            }
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
                self.register_anonymous_parameter_types(
                    file,
                    signature_scope,
                    parameters,
                    &signature_types,
                );
                self.validate_implicit_any_parameters(file, parameters);
                self.visit_required_parameters(
                    file,
                    signature_scope,
                    parameters,
                    &signature_types,
                    ParameterGrammarHost::Signature,
                );
                self.visit_type_parameter_declarations(
                    file,
                    scope,
                    signature_type_parameters,
                    &signature_types,
                );
                self.visit_required_type_node(file, signature_scope, return_type, &signature_types);
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
                for argument in arguments {
                    self.visit_required_type_node(file, scope, argument, type_parameters);
                }
            }
            TypeNodeKind::Infer { constraint, .. } => {
                if let Some(constraint) = constraint {
                    self.visit_required_type_node(file, scope, constraint, type_parameters);
                }
            }
            TypeNodeKind::Predicate { ty, .. } => {
                if let Some(ty) = ty {
                    self.visit_required_type_node(file, scope, ty, type_parameters);
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
                let ty = self.store.intern(TypeKind::TypeParameter {
                    declaration: synthetic_identity(file, parameter_span.start),
                    index: 0,
                    name: parameter.clone(),
                });
                mapped_types.insert(parameter.clone(), ty);
                if let Some(name_type) = name_type {
                    self.visit_required_type_node(file, scope, name_type, &mapped_types);
                }
                self.visit_required_type_node(file, scope, value_type, &mapped_types);
                self.validate_mapped_type_members(file, members);
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                self.visit_required_type_node(file, scope, object, type_parameters);
                self.visit_required_type_node(file, scope, index, type_parameters);
            }
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
            self.infer_expression(file, scope, expression, None);
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
            let ty = self.store.intern(TypeKind::TypeParameter {
                declaration: identity,
                index: index as u32,
                name: declaration.name.clone(),
            });
            if seen.insert(declaration.name.as_str()) {
                parameters.insert(declaration.name.clone(), ty);
            }
        }
        parameters
    }

    pub(super) fn register_anonymous_parameter_types(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        if !type_parameters.is_empty() {
            return;
        }
        let mut seen = HashSet::new();
        let declarations = parameters
            .iter()
            .filter(|parameter| parameter.name_kind == ParameterNameKind::Binding)
            .filter(|parameter| seen.insert(parameter.name.as_str()))
            .filter_map(|parameter| {
                self.resolve_name(file, scope, &parameter.name, Meaning::Value)
                    .map(|declaration| (parameter, declaration))
            })
            .collect::<Vec<_>>();
        for (parameter, declaration) in declarations {
            let ty = if let Some(annotation) = &parameter.annotation {
                self.resolve_type_node(file, scope, annotation, type_parameters)
            } else if let Some(initializer) = &parameter.initializer {
                match self.signature_initializer_type(file, scope, initializer) {
                    Completion::Complete(ty) => ty,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => continue,
                }
            } else {
                self.store.builtins.any
            };
            if self.is_cacheable_type(ty) {
                self.parameter_type_overrides.insert(declaration, ty);
            }
        }
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
        self.collect_conditional_infer_parameters(file, extends_type, &mut parameters);
        parameters
    }

    fn collect_conditional_infer_parameters(
        &mut self,
        file: FileId,
        node: &TypeNode,
        parameters: &mut HashMap<String, TypeId>,
    ) {
        match &node.kind {
            TypeNodeKind::Infer {
                name, name_span, ..
            } => {
                let ty = self.store.intern(TypeKind::TypeParameter {
                    declaration: synthetic_identity(file, name_span.start),
                    index: 0,
                    name: name.clone(),
                });
                parameters.insert(name.clone(), ty);
            }
            TypeNodeKind::Array(child)
            | TypeNodeKind::KeyOf(child)
            | TypeNodeKind::Readonly(child)
            | TypeNodeKind::Parenthesized(child) => {
                self.collect_conditional_infer_parameters(file, child, parameters);
            }
            TypeNodeKind::Tuple(children)
            | TypeNodeKind::Union(children)
            | TypeNodeKind::Intersection(children) => {
                for child in children {
                    self.collect_conditional_infer_parameters(file, child, parameters);
                }
            }
            TypeNodeKind::Object(members) => {
                for member in members {
                    self.collect_conditional_infer_type_member(file, member, parameters);
                }
            }
            TypeNodeKind::Function {
                type_parameters: _,
                parameters: signature_parameters,
                return_type,
                ..
            }
            | TypeNodeKind::Constructor {
                type_parameters: _,
                parameters: signature_parameters,
                return_type,
                ..
            } => {
                for parameter in signature_parameters {
                    if let Some(annotation) = &parameter.annotation {
                        self.collect_conditional_infer_parameters(file, annotation, parameters);
                    }
                }
                self.collect_conditional_infer_parameters(file, return_type, parameters);
            }
            TypeNodeKind::Reference { arguments, .. } => {
                for argument in arguments {
                    self.collect_conditional_infer_parameters(file, argument, parameters);
                }
            }
            TypeNodeKind::Predicate { ty, .. } => {
                if let Some(ty) = ty {
                    self.collect_conditional_infer_parameters(file, ty, parameters);
                }
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                self.collect_conditional_infer_parameters(file, constraint, parameters);
                if let Some(name_type) = name_type {
                    self.collect_conditional_infer_parameters(file, name_type, parameters);
                }
                self.collect_conditional_infer_parameters(file, value_type, parameters);
                for member in members {
                    self.collect_conditional_infer_type_member(file, member, parameters);
                }
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                self.collect_conditional_infer_parameters(file, object, parameters);
                self.collect_conditional_infer_parameters(file, index, parameters);
            }
            // A nested conditional owns its own infer declarations. They must
            // not leak into the outer conditional's true branch.
            TypeNodeKind::Conditional { .. }
            | TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Missing => {}
        }
    }

    fn collect_conditional_infer_type_member(
        &mut self,
        file: FileId,
        member: &TypeMember,
        parameters: &mut HashMap<String, TypeId>,
    ) {
        match &member.kind {
            TypeMemberKind::Property { ty, .. } => {
                if let Some(ty) = ty {
                    self.collect_conditional_infer_parameters(file, ty, parameters);
                }
            }
            TypeMemberKind::Method {
                parameters: signature_parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Call {
                parameters: signature_parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Construct {
                parameters: signature_parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Accessor {
                parameters: signature_parameters,
                return_type,
                ..
            } => {
                for parameter in signature_parameters {
                    if let Some(annotation) = &parameter.annotation {
                        self.collect_conditional_infer_parameters(file, annotation, parameters);
                    }
                }
                if let Some(return_type) = return_type {
                    self.collect_conditional_infer_parameters(file, return_type, parameters);
                }
            }
            TypeMemberKind::Index {
                parameters: signature_parameters,
                value_type,
            } => {
                for parameter in signature_parameters {
                    if let Some(annotation) = &parameter.annotation {
                        self.collect_conditional_infer_parameters(file, annotation, parameters);
                    }
                }
                if let Some(value_type) = value_type {
                    self.collect_conditional_infer_parameters(file, value_type, parameters);
                }
            }
        }
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
            TypeKind::Array(element) => {
                self.visit_required_children(ty, [element], active, references)
            }
            TypeKind::Tuple(elements)
            | TypeKind::Union(elements)
            | TypeKind::Intersection(elements) => {
                self.visit_required_children(ty, elements, active, references)
            }
            TypeKind::Object(shape) => {
                let mut children = shape
                    .properties
                    .into_iter()
                    .map(|property| property.ty)
                    .collect::<Vec<_>>();
                for signature in shape
                    .call_signatures
                    .into_iter()
                    .chain(shape.construct_signatures)
                {
                    children.extend(
                        signature
                            .parameters
                            .into_iter()
                            .map(|parameter| parameter.ty),
                    );
                    children.push(signature.return_type);
                }
                children.extend(shape.index_signatures.into_iter().map(|index| index.value));
                self.visit_required_children(ty, children, active, references)
            }
            TypeKind::ClassInstance {
                arguments,
                properties: shape,
                ..
            } => {
                let mut children = arguments;
                children.extend(shape.properties.into_iter().map(|property| property.ty));
                for signature in shape
                    .call_signatures
                    .into_iter()
                    .chain(shape.construct_signatures)
                {
                    children.extend(
                        signature
                            .parameters
                            .into_iter()
                            .map(|parameter| parameter.ty),
                    );
                    children.push(signature.return_type);
                }
                children.extend(shape.index_signatures.into_iter().map(|index| index.value));
                self.visit_required_children(ty, children, active, references)
            }
            TypeKind::Function(signature) => {
                let children = signature
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty)
                    .chain(std::iter::once(signature.return_type));
                self.visit_required_children(ty, children, active, references)
            }
            TypeKind::ShapeFunction(signature) => {
                let children = signature
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty)
                    .chain(std::iter::once(signature.return_type));
                self.visit_required_children(ty, children, active, references)
            }
            TypeKind::Deferred(deferred) => {
                self.visit_required_deferred(ty, deferred, active, references)
            }
            TypeKind::Error
            | TypeKind::Invalid(_)
            | TypeKind::Any
            | TypeKind::Unknown
            | TypeKind::Never
            | TypeKind::Void
            | TypeKind::Undefined
            | TypeKind::Null
            | TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::ObjectKeyword
            | TypeKind::Symbol
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::TypeParameter { .. }
            | TypeKind::ClassConstructor { .. } => Completion::Complete(ty),
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

        // Conditional, mapped, and predicate nodes are authored symbolic
        // owners. Requiring their declaration position validates every child,
        // but does not itself demand evaluation of the owner. A later
        // relation or inference query remains responsible for forcing it.
        if matches!(
            &deferred,
            DeferredType::Conditional { .. }
                | DeferredType::Mapped { .. }
                | DeferredType::Predicate {
                    parameter_is_bound: true,
                    ..
                }
        ) {
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
                state = state.combine(completion_state(
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
            *state = state.combine(completion_state(&completion));
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

fn class_has_multiple_constructor_implementations(declaration: &ClassDeclaration) -> bool {
    declaration
        .members
        .iter()
        .filter(|member| {
            matches!(
                member.kind,
                ClassMemberKind::Constructor { has_body: true, .. }
            )
        })
        .take(2)
        .count()
        > 1
}

#[derive(Default)]
struct ClassMemberDeclarationGroup {
    methods: usize,
    method_implementations: usize,
    getters: usize,
    setters: usize,
    properties: usize,
}

fn class_member_declaration_groups_are_modeled(declaration: &ClassDeclaration) -> bool {
    let mut groups = HashMap::<(bool, &str), ClassMemberDeclarationGroup>::new();
    let mut uncanonical_members = 0;
    for member in &declaration.members {
        if !matches!(member.kind, ClassMemberKind::Constructor { .. })
            && !member.overload_context_is_recovery_free()
        {
            uncanonical_members += 1;
            if uncanonical_members >= 2 {
                return false;
            }
        }
        let group = groups
            .entry((member.modifiers.static_member, member.name.as_str()))
            .or_default();
        match &member.kind {
            ClassMemberKind::Constructor { .. } => continue,
            ClassMemberKind::Property { .. } => group.properties += 1,
            ClassMemberKind::Method {
                has_body,
                accessor: None,
                ..
            } => {
                group.methods += 1;
                group.method_implementations += usize::from(*has_body);
            }
            ClassMemberKind::Method {
                accessor: Some(AccessorKind::Get),
                ..
            } => group.getters += 1,
            ClassMemberKind::Method {
                accessor: Some(AccessorKind::Set),
                ..
            } => group.setters += 1,
        }
    }
    groups.values().all(|group| {
        if group.methods > 0 {
            group.method_implementations <= 1
                && group.getters == 0
                && group.setters == 0
                && group.properties == 0
        } else if group.properties > 0 {
            group.properties == 1 && group.getters == 0 && group.setters == 0
        } else {
            group.getters <= 1 && group.setters <= 1
        }
    })
}

fn class_has_ambient_implementation(declaration: &ClassDeclaration) -> bool {
    declaration.members.iter().any(|member| match &member.kind {
        ClassMemberKind::Constructor { has_body, .. }
        | ClassMemberKind::Method { has_body, .. } => *has_body,
        ClassMemberKind::Property { initializer, .. } => initializer.is_some(),
    })
}

const fn class_member_access(modifiers: &crate::syntax::ClassMemberModifiers) -> u8 {
    match (modifiers.private, modifiers.protected) {
        (true, _) => 1,
        (false, true) => 2,
        (false, false) => 0,
    }
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
        ExpressionKind::This
        | ExpressionKind::Literal(Literal::BigInt(_) | Literal::Boolean(_) | Literal::Null)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Object(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::ElementAccess { .. }
        | ExpressionKind::Call { .. }
        | ExpressionKind::New { .. }
        | ExpressionKind::FunctionLike(_)
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Assignment { .. }
        | ExpressionKind::As { .. }
        | ExpressionKind::Parenthesized(_)
        | ExpressionKind::Missing => false,
    }
}

fn is_entity_name_expression(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Identifier { entity_name, .. } => *entity_name,
        ExpressionKind::Member { object, .. } => is_entity_name_expression(object),
        ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Object(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::ElementAccess { .. }
        | ExpressionKind::Call { .. }
        | ExpressionKind::New { .. }
        | ExpressionKind::FunctionLike(_)
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Assignment { .. }
        | ExpressionKind::As { .. }
        | ExpressionKind::Parenthesized(_)
        | ExpressionKind::Missing => false,
    }
}

const fn completion_state<T>(completion: &Completion<T>) -> SemanticCompletion {
    match completion {
        Completion::Complete(_) => SemanticCompletion::Complete,
        Completion::Deferred => SemanticCompletion::Deferred,
        Completion::Cycle => SemanticCompletion::Cycle,
        Completion::Limit => SemanticCompletion::Limit,
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
