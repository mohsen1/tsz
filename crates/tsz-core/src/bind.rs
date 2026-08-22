//! Per-file binding. This phase owns declarations and lexical scopes, and
//! intentionally performs no type computation.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{
    ArrowBody, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind, FunctionDeclaration,
    SourceUnit, Statement, StatementKind, SwitchClauseKind, TypeMember, TypeMemberKind,
    TypeMemberNameKind, TypeNode, TypeNodeKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Variable,
    Parameter,
    Import,
    Function,
    Class,
    TypeAlias,
    Interface,
    TypeMember,
    AnonymousSignature,
}

/// Binder-owned symbol category for a type element. Internal signature
/// symbols are typed variants so a user property named `__call` cannot
/// collide with a call-signature group.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeMemberSymbol {
    Named(String),
    Call,
    Construct,
    Index,
}

#[derive(Debug, Clone)]
pub struct BoundTypeMember {
    pub declaration: DeclId,
    pub container: ScopeId,
    pub symbol: Option<TypeMemberSymbol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meaning {
    Value,
    Type,
}

#[derive(Debug, Clone)]
pub struct BoundDeclaration {
    pub id: DeclId,
    pub scope: ScopeId,
    pub owner: NodeId,
    pub name: String,
    pub name_span: Span,
    pub kind: DeclarationKind,
    pub meaning: Meaning,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub owner: Option<NodeId>,
    pub names: BTreeMap<String, Vec<DeclId>>,
}

#[derive(Debug, Clone)]
pub struct BoundFile {
    pub file: FileId,
    pub declarations: Vec<BoundDeclaration>,
    pub scopes: Vec<Scope>,
    pub scope_for_node: FxHashMap<NodeId, ScopeId>,
    pub type_members: FxHashMap<NodeId, BoundTypeMember>,
    pub anonymous_signatures: FxHashMap<NodeId, DeclId>,
    pub type_member_groups: BTreeMap<(ScopeId, TypeMemberSymbol), Vec<DeclId>>,
}

impl BoundFile {
    #[must_use]
    pub fn declaration(&self, id: DeclId) -> Option<&BoundDeclaration> {
        (id.file == self.file)
            .then(|| self.declarations.get(id.local as usize))
            .flatten()
    }

    #[must_use]
    pub fn resolve(&self, mut scope: ScopeId, name: &str, meaning: Meaning) -> Option<DeclId> {
        loop {
            let current = self.scopes.get(scope.0 as usize)?;
            if let Some(ids) = current.names.get(name)
                && let Some(id) = ids.iter().rev().find(|id| {
                    self.declaration(**id)
                        .is_some_and(|declaration| declaration.meaning == meaning)
                })
            {
                return Some(*id);
            }
            scope = current.parent?;
        }
    }

    #[must_use]
    pub fn type_member_group(&self, node: NodeId) -> Option<&[DeclId]> {
        let member = self.type_members.get(&node)?;
        let symbol = member.symbol.as_ref()?;
        self.type_member_groups
            .get(&(member.container, symbol.clone()))
            .map(Vec::as_slice)
    }

    #[must_use]
    pub fn canonical_type_member_declaration(&self, node: NodeId) -> Option<DeclId> {
        self.type_member_group(node)?.first().copied()
    }
}

pub fn bind_source(file: FileId, unit: &SourceUnit) -> BoundFile {
    let mut binder = Binder {
        file,
        declarations: Vec::new(),
        scopes: vec![Scope {
            id: ScopeId(0),
            parent: None,
            owner: None,
            names: BTreeMap::new(),
        }],
        scope_for_node: FxHashMap::default(),
        type_members: FxHashMap::default(),
        anonymous_signatures: FxHashMap::default(),
        type_member_groups: BTreeMap::new(),
    };
    for statement in &unit.statements {
        binder.bind_statement(statement, ScopeId(0));
    }
    BoundFile {
        file,
        declarations: binder.declarations,
        scopes: binder.scopes,
        scope_for_node: binder.scope_for_node,
        type_members: binder.type_members,
        anonymous_signatures: binder.anonymous_signatures,
        type_member_groups: binder.type_member_groups,
    }
}

struct Binder {
    file: FileId,
    declarations: Vec<BoundDeclaration>,
    scopes: Vec<Scope>,
    scope_for_node: FxHashMap<NodeId, ScopeId>,
    type_members: FxHashMap<NodeId, BoundTypeMember>,
    anonymous_signatures: FxHashMap<NodeId, DeclId>,
    type_member_groups: BTreeMap<(ScopeId, TypeMemberSymbol), Vec<DeclId>>,
}

impl Binder {
    fn bind_statement(&mut self, statement: &Statement, scope: ScopeId) {
        self.scope_for_node.insert(statement.id, scope);
        match &statement.kind {
            StatementKind::Import(declaration) => {
                for binding in &declaration.bindings {
                    if !binding.type_only {
                        self.declare(
                            scope,
                            statement.id,
                            &binding.local,
                            binding.local_span,
                            DeclarationKind::Import,
                            Meaning::Value,
                        );
                    }
                    self.declare(
                        scope,
                        statement.id,
                        &binding.local,
                        binding.local_span,
                        DeclarationKind::Import,
                        Meaning::Type,
                    );
                }
            }
            StatementKind::Variable(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Variable,
                    Meaning::Value,
                );
                if let Some(initializer) = &declaration.initializer {
                    self.bind_expression(initializer, scope);
                }
                if let Some(annotation) = &declaration.annotation {
                    self.bind_type_node(annotation, scope);
                }
            }
            StatementKind::Function(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Function,
                    Meaning::Value,
                );
                self.bind_function(statement.id, declaration, scope);
            }
            StatementKind::Class(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Class,
                    Meaning::Value,
                );
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Class,
                    Meaning::Type,
                );
                self.bind_class(statement.id, declaration, scope);
            }
            StatementKind::TypeAlias(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::TypeAlias,
                    Meaning::Type,
                );
                self.bind_type_node(&declaration.ty, scope);
            }
            StatementKind::Interface(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Interface,
                    Meaning::Type,
                );
                let member_scope = self.new_scope(scope, statement.id);
                self.scope_for_node.insert(statement.id, member_scope);
                self.bind_type_members(&declaration.members, member_scope);
            }
            StatementKind::Block(statements) => {
                let child = self.new_scope(scope, statement.id);
                for nested in statements {
                    self.bind_statement(nested, child);
                }
            }
            StatementKind::If(control_flow) => {
                self.bind_expression(&control_flow.condition, scope);
                self.bind_statement(&control_flow.then_statement, scope);
                if let Some(else_statement) = &control_flow.else_statement {
                    self.bind_statement(else_statement, scope);
                }
            }
            StatementKind::Switch(control_flow) => {
                let child = self.new_scope(scope, statement.id);
                self.scope_for_node.insert(statement.id, child);
                self.bind_expression(&control_flow.expression, child);
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.bind_expression(expression, child);
                    }
                    for nested in &clause.statements {
                        self.bind_statement(nested, child);
                    }
                }
            }
            StatementKind::Export(declaration) => {
                if let Some(assignment) = &declaration.assignment {
                    self.bind_expression(assignment, scope);
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.bind_expression(expression, scope);
                }
            }
            StatementKind::Expression(expression) => self.bind_expression(expression, scope),
            StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
    }

    fn bind_type_members(&mut self, members: &[TypeMember], container: ScopeId) {
        for member in members.iter().filter(|member| !member.recovered) {
            self.bind_type_member(member, container);
        }
    }

    fn bind_type_member(&mut self, member: &TypeMember, container: ScopeId) {
        let named_symbol = |name: &crate::syntax::TypeMemberName| match &name.kind {
            TypeMemberNameKind::Identifier(name) => Some(TypeMemberSymbol::Named(name.clone())),
            TypeMemberNameKind::StringLiteral(_)
            | TypeMemberNameKind::NumericLiteral(_)
            | TypeMemberNameKind::BigIntLiteral(_)
            | TypeMemberNameKind::Computed(_) => None,
        };
        let symbol = match &member.kind {
            TypeMemberKind::Property { name, .. }
            | TypeMemberKind::Method { name, .. }
            | TypeMemberKind::Accessor { name, .. } => named_symbol(name),
            TypeMemberKind::Call { .. } => Some(TypeMemberSymbol::Call),
            TypeMemberKind::Construct { .. } => Some(TypeMemberSymbol::Construct),
            TypeMemberKind::Index { .. } => Some(TypeMemberSymbol::Index),
        };
        let (name, name_span) = match &symbol {
            Some(symbol) => match symbol {
                TypeMemberSymbol::Named(name) => {
                    let span = match &member.kind {
                        TypeMemberKind::Property { name, .. }
                        | TypeMemberKind::Method { name, .. }
                        | TypeMemberKind::Accessor { name, .. } => name.span,
                        TypeMemberKind::Call { .. }
                        | TypeMemberKind::Construct { .. }
                        | TypeMemberKind::Index { .. } => member.span,
                    };
                    (name.clone(), span)
                }
                TypeMemberSymbol::Call => ("__call".to_string(), member.span),
                TypeMemberSymbol::Construct => ("__new".to_string(), member.span),
                TypeMemberSymbol::Index => ("__index".to_string(), member.span),
            },
            None => (String::new(), member.span),
        };
        let declaration = self.declare_unscoped(
            container,
            member.id,
            name,
            name_span,
            DeclarationKind::TypeMember,
            Meaning::Type,
        );
        if let Some(symbol) = &symbol {
            self.type_member_groups
                .entry((container, symbol.clone()))
                .or_default()
                .push(declaration);
        }
        self.type_members.insert(
            member.id,
            BoundTypeMember {
                declaration,
                container,
                symbol,
            },
        );

        let member_scope = self.new_scope(container, member.id);
        self.scope_for_node.insert(member.id, member_scope);
        match &member.kind {
            TypeMemberKind::Property {
                name,
                ty,
                initializer,
                ..
            } => {
                self.bind_type_member_name(name, container);
                if let Some(ty) = ty {
                    self.bind_type_node(ty, member_scope);
                }
                if let Some(initializer) = initializer {
                    self.bind_expression(initializer, member_scope);
                }
            }
            TypeMemberKind::Method {
                name,
                type_parameters,
                parameters,
                return_type,
                ..
            } => {
                self.bind_type_member_name(name, container);
                self.bind_type_parameters(type_parameters, member_scope);
                self.bind_signature_types(
                    parameters,
                    return_type.as_ref(),
                    member_scope,
                    member_scope,
                );
            }
            TypeMemberKind::Accessor {
                name,
                parameters,
                return_type,
                ..
            } => {
                self.bind_type_member_name(name, container);
                self.bind_signature_types(
                    parameters,
                    return_type.as_ref(),
                    member_scope,
                    member_scope,
                );
            }
            TypeMemberKind::Call {
                type_parameters,
                parameters,
                return_type,
            }
            | TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
            } => {
                self.bind_type_parameters(type_parameters, member_scope);
                self.bind_signature_types(
                    parameters,
                    return_type.as_ref(),
                    member_scope,
                    member_scope,
                );
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => self.bind_signature_types(
                parameters,
                value_type.as_ref(),
                member_scope,
                member_scope,
            ),
        }
    }

    fn bind_type_member_name(&mut self, name: &crate::syntax::TypeMemberName, scope: ScopeId) {
        if let TypeMemberNameKind::Computed(expression) = &name.kind {
            self.bind_expression(expression, scope);
        }
    }

    fn bind_type_parameters(
        &mut self,
        parameters: &[crate::syntax::TypeParameterDeclaration],
        scope: ScopeId,
    ) {
        for parameter in parameters {
            if let Some(constraint) = &parameter.constraint {
                self.bind_type_node(constraint, scope);
            }
            if let Some(default) = &parameter.default {
                self.bind_type_node(default, scope);
            }
        }
    }

    fn bind_signature_types(
        &mut self,
        parameters: &[crate::syntax::Parameter],
        return_type: Option<&TypeNode>,
        parameter_scope: ScopeId,
        return_scope: ScopeId,
    ) {
        for parameter in parameters {
            if let Some(owner) = self.scopes[parameter_scope.0 as usize].owner {
                self.declare_parameter(
                    parameter_scope,
                    owner,
                    &parameter.name,
                    parameter.name_span,
                    DeclarationKind::Parameter,
                    Meaning::Value,
                );
            }
        }
        for parameter in parameters {
            if let Some(annotation) = &parameter.annotation {
                self.bind_type_node(annotation, parameter_scope);
            }
            if let Some(initializer) = &parameter.initializer {
                self.bind_expression(initializer, parameter_scope);
            }
        }
        if let Some(return_type) = return_type {
            self.bind_type_node(return_type, return_scope);
        }
    }

    fn bind_type_node(&mut self, node: &TypeNode, scope: ScopeId) {
        match &node.kind {
            TypeNodeKind::Array(element)
            | TypeNodeKind::KeyOf(element)
            | TypeNodeKind::Readonly(element)
            | TypeNodeKind::Parenthesized(element) => self.bind_type_node(element, scope),
            TypeNodeKind::Tuple(elements)
            | TypeNodeKind::Union(elements)
            | TypeNodeKind::Intersection(elements) => {
                for element in elements {
                    self.bind_type_node(element, scope);
                }
            }
            TypeNodeKind::Object(members) => {
                let member_scope = self.new_anonymous_scope(scope);
                self.bind_type_members(members, member_scope);
            }
            TypeNodeKind::Function {
                id,
                type_parameters,
                parameters,
                return_type,
            }
            | TypeNodeKind::Constructor {
                id,
                type_parameters,
                parameters,
                return_type,
                ..
            } => {
                let declaration = self.declare_unscoped(
                    scope,
                    *id,
                    String::new(),
                    node.span,
                    DeclarationKind::AnonymousSignature,
                    Meaning::Type,
                );
                self.anonymous_signatures.insert(*id, declaration);
                let signature_scope = self.new_scope(scope, *id);
                self.scope_for_node.insert(*id, signature_scope);
                self.bind_type_parameters(type_parameters, signature_scope);
                self.bind_signature_types(
                    parameters,
                    Some(return_type),
                    signature_scope,
                    signature_scope,
                );
            }
            TypeNodeKind::Reference { arguments, .. } => {
                for argument in arguments {
                    self.bind_type_node(argument, scope);
                }
            }
            TypeNodeKind::Infer { constraint, .. } => {
                if let Some(constraint) = constraint {
                    self.bind_type_node(constraint, scope);
                }
            }
            TypeNodeKind::Predicate { ty, .. } => {
                if let Some(ty) = ty {
                    self.bind_type_node(ty, scope);
                }
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                self.bind_type_node(check_type, scope);
                self.bind_type_node(extends_type, scope);
                self.bind_type_node(true_type, scope);
                self.bind_type_node(false_type, scope);
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                self.bind_type_node(constraint, scope);
                if let Some(name_type) = name_type {
                    self.bind_type_node(name_type, scope);
                }
                self.bind_type_node(value_type, scope);
                let member_scope = self.new_anonymous_scope(scope);
                self.bind_type_members(members, member_scope);
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                self.bind_type_node(object, scope);
                self.bind_type_node(index, scope);
            }
            TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Missing => {}
        }
    }

    fn bind_class(&mut self, owner: NodeId, declaration: &ClassDeclaration, parent: ScopeId) {
        let class_scope = self.new_scope(parent, owner);
        self.scope_for_node.insert(owner, class_scope);
        for member in &declaration.members {
            match &member.kind {
                ClassMemberKind::Property {
                    annotation,
                    initializer,
                    ..
                } => {
                    if let Some(annotation) = annotation {
                        self.bind_type_node(annotation, class_scope);
                    }
                    if let Some(initializer) = initializer {
                        self.bind_expression(initializer, class_scope);
                    }
                }
                ClassMemberKind::Constructor {
                    parameters, body, ..
                } => {
                    let member_scope = self.new_scope(class_scope, member.id);
                    self.scope_for_node.insert(member.id, member_scope);
                    let body_scope = self.new_anonymous_scope(member_scope);
                    self.bind_signature_types(parameters, None, member_scope, member_scope);
                    for statement in body {
                        self.bind_statement(statement, body_scope);
                    }
                }
                ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    ..
                } => {
                    let member_scope = self.new_scope(class_scope, member.id);
                    self.scope_for_node.insert(member.id, member_scope);
                    self.bind_type_parameters(type_parameters, member_scope);
                    let body_scope = self.new_anonymous_scope(member_scope);
                    self.bind_signature_types(
                        parameters,
                        return_type.as_ref(),
                        member_scope,
                        member_scope,
                    );
                    for statement in body {
                        self.bind_statement(statement, body_scope);
                    }
                }
            }
        }
    }

    fn bind_expression(&mut self, expression: &Expression, scope: ScopeId) {
        self.scope_for_node.insert(expression.id, scope);
        match &expression.kind {
            ExpressionKind::Identifier { .. }
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.bind_expression(&property.value, scope);
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.bind_expression(element, scope);
                }
            }
            ExpressionKind::Call {
                callee,
                type_arguments,
                arguments,
            } => {
                self.bind_expression(callee, scope);
                for type_argument in type_arguments.iter().flatten() {
                    self.bind_type_node(type_argument, scope);
                }
                for argument in arguments {
                    self.bind_expression(argument, scope);
                }
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                self.bind_expression(callee, scope);
                for type_argument in type_arguments {
                    self.bind_type_node(type_argument, scope);
                }
                for argument in arguments {
                    self.bind_expression(argument, scope);
                }
            }
            ExpressionKind::Member { object, .. }
            | ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::As {
                expression: object, ..
            }
            | ExpressionKind::Parenthesized(object) => self.bind_expression(object, scope),
            ExpressionKind::Arrow {
                parameters,
                return_type,
                body,
            } => {
                let arrow_scope = self.new_scope(scope, expression.id);
                self.scope_for_node.insert(expression.id, arrow_scope);
                let body_scope = match body {
                    ArrowBody::Expression(_) => arrow_scope,
                    ArrowBody::Block(_) => self.new_anonymous_scope(arrow_scope),
                };
                self.bind_signature_types(
                    parameters,
                    return_type.as_ref(),
                    arrow_scope,
                    arrow_scope,
                );
                match body {
                    ArrowBody::Expression(body) => self.bind_expression(body, body_scope),
                    ArrowBody::Block(statements) => {
                        for statement in statements {
                            self.bind_statement(statement, body_scope);
                        }
                    }
                }
            }
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right } => {
                self.bind_expression(left, scope);
                self.bind_expression(right, scope);
            }
        }
    }

    fn bind_function(&mut self, owner: NodeId, declaration: &FunctionDeclaration, parent: ScopeId) {
        let scope = self.new_scope(parent, owner);
        self.scope_for_node.insert(owner, scope);
        self.bind_type_parameters(&declaration.type_parameters, scope);
        let body_scope = if declaration.has_body {
            self.new_anonymous_scope(scope)
        } else {
            scope
        };
        self.bind_signature_types(
            &declaration.parameters,
            declaration.return_type.as_ref(),
            scope,
            scope,
        );
        for statement in &declaration.body {
            self.bind_statement(statement, body_scope);
        }
    }

    fn new_scope(&mut self, parent: ScopeId, owner: NodeId) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            id,
            parent: Some(parent),
            owner: Some(owner),
            names: BTreeMap::new(),
        });
        id
    }

    fn new_anonymous_scope(&mut self, parent: ScopeId) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            id,
            parent: Some(parent),
            owner: None,
            names: BTreeMap::new(),
        });
        id
    }

    fn declare_unscoped(
        &mut self,
        scope: ScopeId,
        owner: NodeId,
        name: String,
        name_span: Span,
        kind: DeclarationKind,
        meaning: Meaning,
    ) -> DeclId {
        let id = DeclId {
            file: self.file,
            local: self.declarations.len() as u32,
        };
        self.declarations.push(BoundDeclaration {
            id,
            scope,
            owner,
            name,
            name_span,
            kind,
            meaning,
        });
        id
    }

    fn declare(
        &mut self,
        scope: ScopeId,
        owner: NodeId,
        name: &str,
        name_span: Span,
        kind: DeclarationKind,
        meaning: Meaning,
    ) -> DeclId {
        let id = DeclId {
            file: self.file,
            local: self.declarations.len() as u32,
        };
        self.declarations.push(BoundDeclaration {
            id,
            scope,
            owner,
            name: name.to_string(),
            name_span,
            kind,
            meaning,
        });
        self.scopes[scope.0 as usize]
            .names
            .entry(name.to_string())
            .or_default()
            .push(id);
        id
    }

    fn declare_parameter(
        &mut self,
        scope: ScopeId,
        owner: NodeId,
        name: &str,
        name_span: Span,
        kind: DeclarationKind,
        meaning: Meaning,
    ) -> DeclId {
        let duplicate = self.scopes[scope.0 as usize]
            .names
            .get(name)
            .is_some_and(|ids| {
                ids.iter().any(|id| {
                    self.declarations[id.local as usize].kind == DeclarationKind::Parameter
                })
            });
        if duplicate {
            self.declare_unscoped(scope, owner, name.to_string(), name_span, kind, meaning)
        } else {
            self.declare(scope, owner, name, name_span, kind, meaning)
        }
    }
}
