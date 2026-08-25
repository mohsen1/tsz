//! Per-file binding. This phase owns declarations and lexical scopes, and
//! intentionally performs no type computation.

use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::FxHashMap;

use crate::source::{DeclId, FileId, NodeId, SourceKind, Span};
use crate::syntax::{
    ClassDeclaration, ClassMemberKind, Expression, ExpressionKind, FunctionDeclaration,
    FunctionLikeBody, Literal, ParameterNameKind, SourceUnit, Statement, StatementKind,
    SwitchClauseKind, TypeMember, TypeMemberKind, TypeMemberNameKind, TypeNode, TypeNodeKind,
    UnmodeledDeclarationHostFact, UnmodeledDeclarationHostKind,
};

mod flow;

pub(crate) use flow::{
    BoundFlowGraph, BoundFlowNode, FlowAssignmentSource, FlowNarrowing, FlowPathSegment,
    TypeofWitness, TypeofWitnessSet,
};
use flow::{
    FlowContainer, FlowContainerKind, FlowDemandPath, PendingFlowAssignmentSource,
    PendingFlowFacts, PendingFlowMutation, PendingFlowReference,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Variable,
    Parameter,
    Import,
    Function,
    FunctionExpression,
    JavaScriptPropertyAssignment,
    Class,
    TypeAlias,
    Interface,
    TypeMember,
    AnonymousSignature,
    UnmodeledHost,
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

/// Binder-owned lexical `this` identity. Arrow and block scopes inherit this
/// value; non-arrow function and class container scopes reset it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexicalThisOwner {
    ClassInstance(DeclId),
    ClassConstructor(DeclId),
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
    lexical_this: Option<LexicalThisOwner>,
    flow_container: Option<FlowContainer>,
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
    pub(crate) javascript_property_assignments: Vec<BoundJavaScriptPropertyAssignment>,
    pub(crate) javascript_property_uses: Vec<NodeId>,
    pub(crate) javascript_expando_initializers: BTreeSet<DeclId>,
    pub(crate) flow: BoundFlowGraph,
    flow_facts: PendingFlowFacts,
}

#[derive(Debug, Clone)]
pub(crate) struct BoundJavaScriptPropertyAssignment {
    pub(crate) left: NodeId,
    pub(crate) right: NodeId,
    pub(crate) scope: ScopeId,
    pub(crate) declaration: Option<DeclId>,
    pub(crate) root: Option<String>,
    pub(crate) properties: Vec<String>,
    pub(crate) target: JavaScriptPropertyAssignmentTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JavaScriptPropertyAssignmentTarget {
    NamedMember,
    CanonicalElementProperty,
    OrdinaryIndex,
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

    #[must_use]
    pub fn lexical_this_owner(&self, scope: ScopeId) -> Option<LexicalThisOwner> {
        self.scopes
            .get(scope.0 as usize)
            .and_then(|scope| scope.lexical_this)
    }

    pub(crate) fn finalize_flow(
        &mut self,
        unit: &SourceUnit,
        resolve_global: impl Fn(&str) -> Option<DeclId>,
    ) {
        self.flow = BoundFlowGraph::build(unit, self, &self.flow_facts, resolve_global);
    }
}

pub fn bind_source(file: FileId, unit: &SourceUnit) -> BoundFile {
    bind_source_with_kind(file, SourceKind::TypeScript, unit)
}

pub(crate) fn bind_source_with_kind(
    file: FileId,
    source_kind: SourceKind,
    unit: &SourceUnit,
) -> BoundFile {
    let mut binder = Binder {
        file,
        source_kind,
        declarations: Vec::new(),
        scopes: vec![Scope {
            id: ScopeId(0),
            parent: None,
            owner: None,
            names: BTreeMap::new(),
            lexical_this: None,
            flow_container: None,
        }],
        scope_for_node: FxHashMap::default(),
        type_members: FxHashMap::default(),
        anonymous_signatures: FxHashMap::default(),
        type_member_groups: BTreeMap::new(),
        javascript_property_assignments: Vec::new(),
        javascript_property_uses: Vec::new(),
        javascript_expando_initializers: BTreeSet::new(),
        flow_facts: PendingFlowFacts::default(),
        unmodeled_declaration_hosts: unit.unmodeled_declaration_hosts().to_vec(),
    };
    for statement in &unit.statements {
        binder.bind_statement(statement, ScopeId(0), None);
    }
    BoundFile {
        file,
        declarations: binder.declarations,
        scopes: binder.scopes,
        scope_for_node: binder.scope_for_node,
        type_members: binder.type_members,
        anonymous_signatures: binder.anonymous_signatures,
        type_member_groups: binder.type_member_groups,
        javascript_property_assignments: binder.javascript_property_assignments,
        javascript_property_uses: binder.javascript_property_uses,
        javascript_expando_initializers: binder.javascript_expando_initializers,
        flow: BoundFlowGraph::default(),
        flow_facts: binder.flow_facts,
    }
}

struct Binder {
    file: FileId,
    source_kind: SourceKind,
    declarations: Vec<BoundDeclaration>,
    scopes: Vec<Scope>,
    scope_for_node: FxHashMap<NodeId, ScopeId>,
    type_members: FxHashMap<NodeId, BoundTypeMember>,
    anonymous_signatures: FxHashMap<NodeId, DeclId>,
    type_member_groups: BTreeMap<(ScopeId, TypeMemberSymbol), Vec<DeclId>>,
    javascript_property_assignments: Vec<BoundJavaScriptPropertyAssignment>,
    javascript_property_uses: Vec<NodeId>,
    javascript_expando_initializers: BTreeSet<DeclId>,
    flow_facts: PendingFlowFacts,
    unmodeled_declaration_hosts: Vec<UnmodeledDeclarationHostFact>,
}

impl Binder {
    fn bind_statement(&mut self, statement: &Statement, scope: ScopeId, control: Option<NodeId>) {
        self.scope_for_node.insert(statement.id, scope);
        let unmodeled_hosts = self
            .unmodeled_declaration_hosts
            .iter()
            .filter(|host| host.owner_start == statement.span.start)
            .cloned()
            .collect::<Vec<_>>();
        for host in unmodeled_hosts {
            let Some(name) = host.name.as_deref() else {
                continue;
            };
            let Some(name_span) = host.name_span else {
                continue;
            };
            let meanings: &[Meaning] = match host.kind {
                UnmodeledDeclarationHostKind::Namespace | UnmodeledDeclarationHostKind::Module => {
                    &[Meaning::Value, Meaning::Type]
                }
                UnmodeledDeclarationHostKind::Using => &[Meaning::Value],
                UnmodeledDeclarationHostKind::ExternalModule
                | UnmodeledDeclarationHostKind::Global => &[],
            };
            for meaning in meanings {
                self.declare(
                    scope,
                    statement.id,
                    name,
                    name_span,
                    DeclarationKind::UnmodeledHost,
                    *meaning,
                );
            }
        }
        if self.unmodeled_declaration_hosts.iter().any(|host| {
            host.owner_start != statement.span.start
                && host.recovery_extent.start <= statement.span.start
                && statement.span.start < host.recovery_extent.end
        }) {
            return;
        }
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
                let declared = if declaration.recovered_binding_names.is_empty() {
                    Some(self.declare(
                        scope,
                        statement.id,
                        &declaration.name,
                        declaration.name_span,
                        DeclarationKind::Variable,
                        Meaning::Value,
                    ))
                } else {
                    for binding in &declaration.recovered_binding_names {
                        self.declare(
                            scope,
                            statement.id,
                            &binding.name,
                            binding.span,
                            DeclarationKind::Variable,
                            Meaning::Value,
                        );
                    }
                    None
                };
                if let Some(declared) = declared
                    && declaration.annotation.is_none()
                    && declaration
                        .initializer
                        .as_ref()
                        .is_some_and(is_empty_array_expression)
                {
                    self.flow_facts.evolving_array_declarations.push(declared);
                }
                if let (Some(declared), None, Some(initializer)) =
                    (declared, &declaration.annotation, &declaration.initializer)
                    && is_javascript_expando_initializer(initializer)
                    && !self.source_kind.supports_expression_type_arguments()
                {
                    self.javascript_expando_initializers.insert(declared);
                }
                if let Some(initializer) = &declaration.initializer {
                    self.bind_expression(initializer, scope, control);
                }
                if let (Some(declaration), Some(initializer)) = (declared, &declaration.initializer)
                    && let Some(PendingFlowAssignmentSource::Literal(source)) =
                        PendingFlowAssignmentSource::from_expression(initializer)
                {
                    self.flow_facts
                        .initializers
                        .push((declaration, source, statement.span));
                }
                if let Some(annotation) = &declaration.annotation {
                    self.bind_type_node(annotation, scope);
                }
            }
            StatementKind::Function(declaration) => {
                let declared = self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Function,
                    Meaning::Value,
                );
                if !self.source_kind.supports_expression_type_arguments() {
                    self.javascript_expando_initializers.insert(declared);
                }
                self.bind_function(statement.id, declaration, scope);
            }
            StatementKind::Class(declaration) => {
                let constructor = self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Class,
                    Meaning::Value,
                );
                let instance = self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Class,
                    Meaning::Type,
                );
                self.bind_class(statement.id, declaration, scope, instance, constructor);
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
                self.bind_type_members(&declaration.members, member_scope);
            }
            StatementKind::Block(statements) => {
                let child = self.new_scope(scope, statement.id);
                for nested in statements {
                    self.bind_statement(nested, child, control);
                }
            }
            StatementKind::If(control_flow) => {
                self.bind_expression(&control_flow.condition, scope, control);
                self.bind_statement(&control_flow.then_statement, scope, Some(statement.id));
                if let Some(else_statement) = &control_flow.else_statement {
                    self.bind_statement(else_statement, scope, Some(statement.id));
                }
            }
            StatementKind::Switch(control_flow) => {
                let child = self.new_scope(scope, statement.id);
                self.bind_expression(&control_flow.expression, scope, control);
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.bind_expression(expression, child, Some(expression.id));
                    }
                    for nested in &clause.statements {
                        self.bind_statement(nested, child, Some(statement.id));
                    }
                }
            }
            StatementKind::Export(declaration) => {
                if let Some(assignment) = &declaration.assignment {
                    self.bind_expression(assignment, scope, control);
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.bind_expression(expression, scope, control);
                }
            }
            StatementKind::Expression(expression) => {
                self.bind_expression(expression, scope, control);
            }
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
        if let TypeMemberKind::Property {
            name,
            ty,
            initializer,
            ..
        } = &member.kind
        {
            self.bind_type_member_name(name, container);
            if let Some(ty) = ty {
                self.bind_type_node(ty, member_scope);
            }
            if let Some(initializer) = initializer {
                self.bind_expression(initializer, member_scope, None);
            }
            return;
        }
        let Some((name, type_parameters, parameters, return_type)) = member.kind.signature() else {
            return;
        };
        if let Some(name) = name {
            self.bind_type_member_name(name, container);
        }
        self.bind_type_parameters(type_parameters, member_scope);
        self.bind_signature_types(parameters, return_type, member_scope, member_scope);
    }

    fn bind_type_member_name(&mut self, name: &crate::syntax::TypeMemberName, scope: ScopeId) {
        if let TypeMemberNameKind::Computed(expression) = &name.kind {
            self.bind_expression(expression, scope, None);
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
        for parameter in parameters
            .iter()
            .filter(|parameter| parameter.name_kind != ParameterNameKind::This)
        {
            if let Some(owner) = self.scopes[parameter_scope.0 as usize].owner {
                if parameter.name_kind == ParameterNameKind::Binding {
                    self.declare_parameter(
                        parameter_scope,
                        owner,
                        &parameter.name,
                        parameter.name_span,
                        DeclarationKind::Parameter,
                        Meaning::Value,
                    );
                } else {
                    for binding in &parameter.recovered_binding_names {
                        self.declare_parameter(
                            parameter_scope,
                            owner,
                            &binding.name,
                            binding.span,
                            DeclarationKind::Parameter,
                            Meaning::Value,
                        );
                    }
                }
            }
        }
        for parameter in parameters {
            if let Some(annotation) = &parameter.annotation {
                self.bind_type_node(annotation, parameter_scope);
            }
            if let Some(initializer) = &parameter.initializer {
                self.bind_expression(initializer, parameter_scope, None);
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
            TypeNodeKind::Tuple(arguments)
            | TypeNodeKind::Union(arguments)
            | TypeNodeKind::Intersection(arguments)
            | TypeNodeKind::Reference { arguments, .. } => {
                for argument in arguments {
                    self.bind_type_node(argument, scope);
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
                ..
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
                self.bind_type_parameters(type_parameters, signature_scope);
                self.bind_signature_types(
                    parameters,
                    Some(return_type),
                    signature_scope,
                    signature_scope,
                );
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

    fn bind_class(
        &mut self,
        owner: NodeId,
        declaration: &ClassDeclaration,
        parent: ScopeId,
        instance: DeclId,
        constructor: DeclId,
    ) {
        let class_scope = self.new_scope_with_lexical_this(parent, Some(owner), None);
        for member in &declaration.members {
            let lexical_this = if member.modifiers.static_member {
                LexicalThisOwner::ClassConstructor(constructor)
            } else {
                LexicalThisOwner::ClassInstance(instance)
            };
            let (type_parameters, parameters, return_type, body, lexical_this) = match &member.kind
            {
                ClassMemberKind::Property {
                    annotation,
                    initializer,
                    ..
                } => {
                    let member_scope = self.new_flow_scope(
                        class_scope,
                        member.id,
                        Some(lexical_this),
                        FlowContainerKind::Creation,
                    );
                    if let Some(annotation) = annotation {
                        self.bind_type_node(annotation, member_scope);
                    }
                    if let Some(initializer) = initializer {
                        self.bind_expression(initializer, member_scope, None);
                    }
                    continue;
                }
                ClassMemberKind::Constructor {
                    parameters, body, ..
                } => (
                    &[][..],
                    parameters.as_slice(),
                    None,
                    body.as_slice(),
                    LexicalThisOwner::ClassInstance(instance),
                ),
                ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    ..
                } => (
                    type_parameters.as_slice(),
                    parameters.as_slice(),
                    return_type.as_ref(),
                    body.as_slice(),
                    lexical_this,
                ),
            };
            let member_scope = self.new_flow_scope(
                class_scope,
                member.id,
                Some(lexical_this),
                FlowContainerKind::Ordinary,
            );
            self.bind_type_parameters(type_parameters, member_scope);
            let body_scope = self.new_anonymous_scope(member_scope);
            self.bind_signature_types(parameters, return_type, member_scope, member_scope);
            for statement in body {
                self.bind_statement(statement, body_scope, None);
            }
        }
    }

    fn bind_expression(
        &mut self,
        expression: &Expression,
        scope: ScopeId,
        control: Option<NodeId>,
    ) {
        self.bind_expression_with_demand(expression, scope, control, FlowDemandPath::root());
    }

    fn bind_expression_with_demand(
        &mut self,
        expression: &Expression,
        scope: ScopeId,
        control: Option<NodeId>,
        demand: FlowDemandPath,
    ) {
        self.scope_for_node.insert(expression.id, scope);
        match &expression.kind {
            ExpressionKind::Identifier {
                name, entity_name, ..
            } => {
                if *entity_name {
                    self.flow_facts.references.push(PendingFlowReference {
                        expression: expression.id,
                        span: expression.span,
                        scope,
                        name: name.clone(),
                        demand,
                    });
                }
            }
            ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.bind_expression(&property.value, scope, control);
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.bind_expression(element, scope, control);
                }
            }
            ExpressionKind::Call {
                callee,
                type_arguments,
                arguments,
            } => {
                self.bind_expression(callee, scope, control);
                for type_argument in type_arguments.iter().flatten() {
                    self.bind_type_node(type_argument, scope);
                }
                for argument in arguments {
                    self.bind_expression(argument, scope, control);
                }
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                self.bind_expression(callee, scope, control);
                for type_argument in type_arguments {
                    self.bind_type_node(type_argument, scope);
                }
                for argument in arguments {
                    self.bind_expression(argument, scope, control);
                }
            }
            ExpressionKind::Member { object, name, .. } => {
                self.bind_javascript_property_use(expression);
                self.bind_expression_with_demand(object, scope, control, demand.member(name));
            }
            ExpressionKind::Parenthesized(object) => {
                self.bind_expression_with_demand(object, scope, control, demand);
            }
            ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::As {
                expression: object, ..
            } => {
                self.bind_expression(object, scope, control);
            }
            ExpressionKind::ElementAccess { object, index } => {
                self.bind_expression_with_demand(object, scope, control, demand.element(index));
                self.bind_expression(index, scope, control);
            }
            ExpressionKind::FunctionLike(function) => {
                let ordinary = function.syntax.function();
                let (lexical_this, kind) = ordinary.map_or_else(
                    || {
                        (
                            self.scopes[scope.0 as usize].lexical_this,
                            FlowContainerKind::Creation,
                        )
                    },
                    |_| (None, FlowContainerKind::Ordinary),
                );
                let function_scope = self.new_flow_scope(scope, expression.id, lexical_this, kind);
                if let Some((name, _)) = ordinary {
                    if let Some(name) = name {
                        self.declare(
                            function_scope,
                            expression.id,
                            &name.name,
                            name.span,
                            DeclarationKind::FunctionExpression,
                            Meaning::Value,
                        );
                    } else {
                        let start = expression.span.start;
                        self.declare_unscoped(
                            function_scope,
                            expression.id,
                            String::new(),
                            Span {
                                file: expression.span.file,
                                start,
                                end: start,
                            },
                            DeclarationKind::FunctionExpression,
                            Meaning::Value,
                        );
                    }
                }
                self.bind_type_parameters(&function.type_parameters, function_scope);
                let body = function.syntax.body();
                let body_scope = match body {
                    FunctionLikeBody::Expression(_) => function_scope,
                    FunctionLikeBody::Statements(_) => self.new_anonymous_scope(function_scope),
                };
                self.bind_signature_types(
                    &function.parameters,
                    function.return_type.as_ref(),
                    function_scope,
                    function_scope,
                );
                match body {
                    FunctionLikeBody::Expression(body) => {
                        self.bind_expression(body, body_scope, None)
                    }
                    FunctionLikeBody::Statements(body) => {
                        for statement in body {
                            self.bind_statement(statement, body_scope, None);
                        }
                    }
                }
            }
            ExpressionKind::Binary { left, right, .. } => {
                self.bind_expression(left, scope, control);
                self.bind_expression(right, scope, control);
            }
            ExpressionKind::Assignment { left, right, .. } => {
                self.bind_expression(left, scope, control);
                self.bind_expression(right, scope, control);
                self.bind_javascript_property_assignment(expression, left, right, scope);
                if let Some(target) = flow_assignment_root(left) {
                    self.flow_facts.mutations.push(PendingFlowMutation {
                        target: target.id,
                        source: simple_assignment_target(left)
                            .and_then(|_| PendingFlowAssignmentSource::from_expression(right)),
                        control,
                        effect_span: expression.span,
                    });
                }
                if let Some(target) = element_assignment_receiver(left) {
                    self.flow_facts.evolving_array_writes.push(target.id);
                }
            }
        }
    }

    fn bind_javascript_property_use(&mut self, expression: &Expression) {
        if !self.source_kind.supports_expression_type_arguments() {
            self.javascript_property_uses.push(expression.id);
        }
    }

    fn bind_javascript_property_assignment(
        &mut self,
        expression: &Expression,
        left: &Expression,
        right: &Expression,
        scope: ScopeId,
    ) {
        if self.source_kind.supports_expression_type_arguments() {
            return;
        }
        use JavaScriptPropertyAssignmentTarget as Target;
        let target = match &left.peel_parentheses().kind {
            ExpressionKind::Member { .. } => Target::NamedMember,
            ExpressionKind::ElementAccess { index, .. } => match &index.kind {
                ExpressionKind::Literal(Literal::String(_) | Literal::Number(_)) => {
                    Target::CanonicalElementProperty
                }
                _ => Target::OrdinaryIndex,
            },
            _ => return,
        };
        let root = flow_assignment_root(left).and_then(|root| match &root.kind {
            ExpressionKind::Identifier { name, .. } => Some(name.clone()),
            _ => None,
        });
        let (declaration, properties) = javascript_named_property_path(left).map_or(
            (None, Vec::new()),
            |(_, properties, name_span)| {
                let declaration = self.declare_unscoped(
                    scope,
                    expression.id,
                    properties.last().expect("member path").clone(),
                    name_span,
                    DeclarationKind::JavaScriptPropertyAssignment,
                    Meaning::Value,
                );
                if is_javascript_expando_initializer(right) {
                    self.javascript_expando_initializers.insert(declaration);
                }
                (Some(declaration), properties)
            },
        );
        self.javascript_property_assignments
            .push(BoundJavaScriptPropertyAssignment {
                left: left.id,
                right: right.id,
                scope,
                declaration,
                root,
                properties,
                target,
            });
    }

    fn bind_function(&mut self, owner: NodeId, declaration: &FunctionDeclaration, parent: ScopeId) {
        let scope = self.new_flow_scope(parent, owner, None, FlowContainerKind::Ordinary);
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
            self.bind_statement(statement, body_scope, None);
        }
    }

    fn new_scope(&mut self, parent: ScopeId, owner: NodeId) -> ScopeId {
        let lexical_this = self.scopes[parent.0 as usize].lexical_this;
        self.new_scope_with_lexical_this(parent, Some(owner), lexical_this)
    }

    fn new_scope_with_lexical_this(
        &mut self,
        parent: ScopeId,
        owner: Option<NodeId>,
        lexical_this: Option<LexicalThisOwner>,
    ) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        let flow_container = self.scopes[parent.0 as usize].flow_container;
        self.scopes.push(Scope {
            id,
            parent: Some(parent),
            owner,
            names: BTreeMap::new(),
            lexical_this,
            flow_container,
        });
        if let Some(owner) = owner {
            self.scope_for_node.insert(owner, id);
        }
        id
    }

    fn new_flow_scope(
        &mut self,
        parent: ScopeId,
        owner: NodeId,
        lexical_this: Option<LexicalThisOwner>,
        kind: FlowContainerKind,
    ) -> ScopeId {
        let scope = self.new_scope_with_lexical_this(parent, Some(owner), lexical_this);
        self.scopes[scope.0 as usize].flow_container = Some(FlowContainer { owner, kind });
        scope
    }

    fn new_anonymous_scope(&mut self, parent: ScopeId) -> ScopeId {
        let lexical_this = self.scopes[parent.0 as usize].lexical_this;
        self.new_scope_with_lexical_this(parent, None, lexical_this)
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
        let id = self.declare_unscoped(scope, owner, name.to_string(), name_span, kind, meaning);
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

fn simple_assignment_target(expression: &Expression) -> Option<&Expression> {
    let expression = expression.peel_parentheses();
    matches!(expression.kind, ExpressionKind::Identifier { .. }).then_some(expression)
}

fn flow_assignment_root(expression: &Expression) -> Option<&Expression> {
    let expression = expression.peel_parentheses();
    match &expression.kind {
        ExpressionKind::Identifier { .. } => Some(expression),
        ExpressionKind::Member { object, .. } | ExpressionKind::ElementAccess { object, .. } => {
            flow_assignment_root(object)
        }
        _ => None,
    }
}

fn element_assignment_receiver(expression: &Expression) -> Option<&Expression> {
    let expression = expression.peel_parentheses();
    let ExpressionKind::ElementAccess { object, .. } = &expression.kind else {
        return None;
    };
    simple_assignment_target(object)
}

fn is_empty_array_expression(expression: &Expression) -> bool {
    let expression = expression.peel_parentheses();
    matches!(&expression.kind, ExpressionKind::Array(elements) if elements.is_empty())
}

fn javascript_named_property_path(
    mut expression: &Expression,
) -> Option<(String, Vec<String>, Span)> {
    let ExpressionKind::Member { name_span, .. } = &expression.peel_parentheses().kind else {
        return None;
    };
    let mut properties = Vec::new();
    loop {
        expression = expression.peel_parentheses();
        match &expression.kind {
            ExpressionKind::Member { object, name, .. } => {
                properties.push(name.clone());
                expression = object;
            }
            ExpressionKind::Identifier { name, .. } => {
                properties.reverse();
                return Some((name.clone(), properties, *name_span));
            }
            _ => return None,
        }
    }
}

fn is_javascript_expando_initializer(expression: &Expression) -> bool {
    let expression = expression.peel_parentheses();
    matches!(&expression.kind, ExpressionKind::Object(properties) if properties.is_empty())
        || matches!(expression.kind, ExpressionKind::FunctionLike(_))
}
