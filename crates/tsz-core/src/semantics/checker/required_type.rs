use std::collections::{HashMap, HashSet};

use crate::bind::{DeclarationKind, ScopeId};
use crate::program::SemanticCompletion;
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{
    ArrowBody, ClassMemberKind, Expression, ExpressionKind, Parameter, Statement, StatementKind,
    SwitchClauseKind, TypeNode, TypeNodeKind, TypeParameterDeclaration,
};

use super::Checker;

impl Checker<'_> {
    /// Walk every parsed explicit type position before ordinary checking.
    ///
    /// This is intentionally a syntax boundary, not an inference or relation
    /// side effect. It preserves the lexical value scope and the declaration-
    /// owned type-parameter identities for annotations that otherwise might
    /// never be observed (ambient declarations, class bodies, assertions, and
    /// nested arrows included).
    pub(super) fn require_explicit_type_positions(&mut self) {
        let empty = HashMap::new();
        for file_id in &self.program.source_order {
            let file_id = *file_id;
            let statements = &self.program.files[file_id.0 as usize].syntax.statements;
            for statement in statements {
                self.visit_required_statement(file_id, ScopeId(0), statement, &empty);
            }
        }
    }

    /// Require a type at a declaration boundary.
    ///
    /// Forcing a deferred outer type is not enough: a concrete object,
    /// signature, tuple, or union can still contain deferred required
    /// components. This visitor is the single boundary that walks those
    /// components. Its active set is keyed by program-local `TypeId`, so
    /// productive recursive structural types close coinductively without a
    /// second depth or fuel policy. Deferred evaluation keeps its own budget.
    pub(super) fn require_type_completion(&mut self, ty: TypeId) -> Completion<TypeId> {
        let mut active = HashSet::new();
        let completion = self.visit_required_type(ty, &mut active);
        self.require_completion(completion)
    }

    pub(super) fn require_function_signature(&mut self, id: DeclId) -> Option<TypeId> {
        let declaration_completion = self.declaration_value_type(id);
        let signature_type = match self.require_completion(declaration_completion) {
            Completion::Complete(signature_type) => signature_type,
            Completion::Deferred | Completion::Cycle | Completion::Limit => return None,
        };
        let signature_type = match self.require_type_completion(signature_type) {
            Completion::Complete(signature_type) => signature_type,
            Completion::Deferred | Completion::Cycle | Completion::Limit => {
                self.value_queries.remove(&id);
                return None;
            }
        };
        let TypeKind::Function(signature) = self.store.kind(signature_type) else {
            return None;
        };
        Some(signature.return_type)
    }

    fn visit_required_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
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
                let function_scope = self.node_scope(file, statement.id, scope);
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Function,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(file, declaration.name_span.start));
                let function_types = self.extend_type_parameters(
                    identity,
                    &declaration.type_parameters,
                    type_parameters,
                );
                self.visit_type_parameter_declarations(
                    file,
                    function_scope,
                    &declaration.type_parameters,
                    &function_types,
                );
                self.visit_required_parameters(
                    file,
                    function_scope,
                    &declaration.parameters,
                    &function_types,
                );
                if let Some(return_type) = &declaration.return_type {
                    self.visit_required_type_node(
                        file,
                        function_scope,
                        return_type,
                        &function_types,
                    );
                }
                for nested in &declaration.body {
                    let nested_scope = self.node_scope(file, nested.id, function_scope);
                    self.visit_required_statement(file, nested_scope, nested, &function_types);
                }
            }
            StatementKind::Class(declaration) => {
                let class_scope = self.node_scope(file, statement.id, scope);
                let identity = self
                    .find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Class,
                        &declaration.name,
                    )
                    .unwrap_or_else(|| synthetic_identity(file, declaration.name_span.start));
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
                    self.visit_required_type_node(file, class_scope, heritage, &class_types);
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
                for property in &declaration.properties {
                    self.visit_required_type_node(file, scope, &property.ty, &interface_types);
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
                    type_parameters,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = self.node_scope(file, else_statement.id, scope);
                    self.visit_required_statement(
                        file,
                        else_scope,
                        else_statement,
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
                    for nested in &clause.statements {
                        let nested_scope = self.node_scope(file, nested.id, switch_scope);
                        self.visit_required_statement(file, nested_scope, nested, type_parameters);
                    }
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.visit_required_expression(file, scope, expression, type_parameters);
                }
            }
            StatementKind::Block(statements) => {
                for nested in statements {
                    let nested_scope = self.node_scope(file, nested.id, scope);
                    self.visit_required_statement(file, nested_scope, nested, type_parameters);
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
                parameters, body, ..
            } => {
                let member_scope = self.node_scope(file, member.id, class_scope);
                self.visit_required_parameters(file, member_scope, parameters, type_parameters);
                self.visit_required_body(file, member_scope, body, type_parameters);
            }
            ClassMemberKind::Method {
                type_parameters: declarations,
                parameters,
                return_type,
                body,
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
                    member_scope,
                    declarations,
                    &method_types,
                );
                self.visit_required_parameters(file, member_scope, parameters, &method_types);
                if let Some(return_type) = return_type {
                    self.visit_required_type_node(file, member_scope, return_type, &method_types);
                }
                self.visit_required_body(file, member_scope, body, &method_types);
            }
        }
    }

    fn visit_required_body(
        &mut self,
        file: FileId,
        scope: ScopeId,
        body: &[Statement],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        for statement in body {
            let statement_scope = self.node_scope(file, statement.id, scope);
            self.visit_required_statement(file, statement_scope, statement, type_parameters);
        }
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
            | ExpressionKind::Literal(_)
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
            ExpressionKind::Call { callee, arguments } => {
                self.visit_required_expression(file, scope, callee, type_parameters);
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
            ExpressionKind::Arrow {
                parameters,
                return_type,
                body,
            } => {
                let arrow_scope = self.node_scope(file, expression.id, scope);
                self.visit_required_parameters(file, arrow_scope, parameters, type_parameters);
                if let Some(return_type) = return_type {
                    self.visit_required_type_node(file, arrow_scope, return_type, type_parameters);
                }
                match body {
                    ArrowBody::Expression(body) => {
                        self.visit_required_expression(file, arrow_scope, body, type_parameters)
                    }
                    ArrowBody::Block(statements) => {
                        self.visit_required_body(file, arrow_scope, statements, type_parameters);
                    }
                }
            }
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right } => {
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
        for declaration in declarations {
            if let Some(constraint) = &declaration.constraint {
                self.visit_required_type_node(file, scope, constraint, type_parameters);
            }
            if let Some(default) = &declaration.default {
                self.visit_required_type_node(file, scope, default, type_parameters);
            }
        }
    }

    fn visit_required_parameters(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        for parameter in parameters {
            if let Some(annotation) = &parameter.annotation {
                self.visit_required_type_node(file, scope, annotation, type_parameters);
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
        if matches!(self.require_type_completion(ty), Completion::Complete(_)) {
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
            TypeNodeKind::Object(properties) => {
                for property in properties {
                    self.visit_required_type_node(file, scope, &property.ty, type_parameters);
                }
            }
            TypeNodeKind::Function {
                parameters,
                return_type,
            }
            | TypeNodeKind::Constructor {
                parameters,
                return_type,
                ..
            } => {
                self.visit_required_parameters(file, scope, parameters, type_parameters);
                self.visit_required_type_node(file, scope, return_type, type_parameters);
            }
            TypeNodeKind::Reference { arguments, .. } => {
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
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                self.visit_required_type_node(file, scope, object, type_parameters);
                self.visit_required_type_node(file, scope, index, type_parameters);
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
        for (index, declaration) in declarations.iter().enumerate() {
            let ty = self.store.intern(TypeKind::TypeParameter {
                declaration: identity,
                index: index as u32,
                name: declaration.name.clone(),
            });
            parameters.insert(declaration.name.clone(), ty);
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
            TypeNodeKind::Object(properties) => {
                for property in properties {
                    self.collect_conditional_infer_parameters(file, &property.ty, parameters);
                }
            }
            TypeNodeKind::Function {
                parameters: signature_parameters,
                return_type,
            }
            | TypeNodeKind::Constructor {
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
                ..
            } => {
                self.collect_conditional_infer_parameters(file, constraint, parameters);
                if let Some(name_type) = name_type {
                    self.collect_conditional_infer_parameters(file, name_type, parameters);
                }
                self.collect_conditional_infer_parameters(file, value_type, parameters);
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

    fn node_scope(&self, file: FileId, node: NodeId, fallback: ScopeId) -> ScopeId {
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
    ) -> Completion<TypeId> {
        if !active.insert(ty) {
            return Completion::Complete(ty);
        }
        let completion = match self.store.kind(ty).clone() {
            TypeKind::Array(element) | TypeKind::StringIndex(element) => {
                self.visit_required_children(ty, [element], active)
            }
            TypeKind::Tuple(elements)
            | TypeKind::Union(elements)
            | TypeKind::Intersection(elements) => {
                self.visit_required_children(ty, elements, active)
            }
            TypeKind::Object(properties) | TypeKind::ClassInstance { properties, .. } => self
                .visit_required_children(
                    ty,
                    properties.into_iter().map(|property| property.ty),
                    active,
                ),
            TypeKind::Function(signature) => {
                let children = signature
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty)
                    .chain(std::iter::once(signature.return_type));
                self.visit_required_children(ty, children, active)
            }
            TypeKind::Deferred(deferred) => self.visit_required_deferred(ty, deferred, active),
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
    ) -> Completion<TypeId> {
        let mut state = SemanticCompletion::Complete;
        self.visit_deferred_operands(&deferred, active, &mut state);

        // Conditional, mapped, and predicate nodes are authored symbolic
        // owners. Requiring their declaration position validates every child,
        // but does not itself demand evaluation of the owner. A later
        // relation or inference query remains responsible for forcing it.
        if matches!(
            deferred,
            DeferredType::Conditional { .. }
                | DeferredType::Mapped { .. }
                | DeferredType::Predicate {
                    parameter_is_bound: true,
                    ..
                }
        ) {
            return completion_from_state(state, ty);
        }

        let mut resolved = ty;
        match self.force_type(ty, 0) {
            Completion::Complete(forced) => {
                resolved = forced;
                state = state.combine(completion_state(&self.visit_required_type(forced, active)));
            }
            Completion::Deferred => state = state.combine(SemanticCompletion::Deferred),
            Completion::Cycle => state = state.combine(SemanticCompletion::Cycle),
            Completion::Limit => state = state.combine(SemanticCompletion::Limit),
        }
        let completion = completion_from_state(state, resolved);
        if !matches!(completion, Completion::Complete(_)) {
            self.force_queries.remove(&ty);
        }
        completion
    }

    fn visit_deferred_operands(
        &mut self,
        deferred: &DeferredType,
        active: &mut HashSet<TypeId>,
        state: &mut SemanticCompletion,
    ) {
        match deferred {
            DeferredType::Reference { arguments, .. } => {
                self.combine_required_children(arguments.iter().copied(), active, state);
            }
            DeferredType::Value(_) => {}
            DeferredType::Call(callee)
            | DeferredType::Unary {
                operand: callee, ..
            }
            | DeferredType::KeyOf(callee)
            | DeferredType::Property { object: callee, .. } => {
                self.combine_required_children([*callee], active, state);
            }
            DeferredType::Construct {
                callee,
                type_arguments,
                ..
            } => {
                self.combine_required_children([*callee], active, state);
                self.combine_required_children(type_arguments.iter().copied(), active, state);
            }
            DeferredType::Predicate { asserted, .. } => {
                self.combine_required_children(asserted.iter().copied(), active, state);
            }
            DeferredType::Logical { left, right, .. } => {
                self.combine_required_children([*left, *right], active, state);
            }
            DeferredType::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } => {
                self.combine_required_children(
                    [*check, *extends, *when_true, *when_false],
                    active,
                    state,
                );
            }
            DeferredType::Mapped {
                constraint,
                name_type,
                value,
                ..
            } => {
                self.combine_required_children([*constraint, *value], active, state);
                self.combine_required_children(name_type.iter().copied(), active, state);
            }
            DeferredType::IndexedAccess { object, index, .. } => {
                self.combine_required_children([*object, *index], active, state);
            }
        }
    }

    fn visit_required_children(
        &mut self,
        owner: TypeId,
        children: impl IntoIterator<Item = TypeId>,
        active: &mut HashSet<TypeId>,
    ) -> Completion<TypeId> {
        let mut state = SemanticCompletion::Complete;
        self.combine_required_children(children, active, &mut state);
        completion_from_state(state, owner)
    }

    fn combine_required_children(
        &mut self,
        children: impl IntoIterator<Item = TypeId>,
        active: &mut HashSet<TypeId>,
        state: &mut SemanticCompletion,
    ) {
        for child in children {
            let completion = self.visit_required_type(child, active);
            *state = state.combine(completion_state(&completion));
        }
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
