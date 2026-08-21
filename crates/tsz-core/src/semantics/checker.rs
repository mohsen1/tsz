use std::collections::{HashMap, HashSet};

use rustc_hash::FxHashMap;

mod cache_model;
mod class_model;
mod function_model;
mod object_shape;
mod projection_model;
mod relation_diagnostic;
mod required_type;
mod statement_model;
mod type_member_grammar;

use relation_diagnostic::{
    ContextualPropertyType, RelationDiagnosticOutcome, RelationDiagnosticStyle,
};

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::diagnostics::Diagnostic;
use crate::program::{CompilerOptions, Program, SemanticCompletion};
use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{
    BinaryOperator, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind,
    FunctionDeclaration, InterfaceDeclaration, KeywordType, Literal, Parameter, Statement,
    StatementKind, TypeAliasDeclaration, TypeNode, TypeNodeKind, UnaryOperator,
    VariableDeclaration, VariableKind,
};

use super::relation::{RelationContext, RelationMode};
use super::types::{
    Completion, DeferredLogicalOperator, DeferredType, DeferredUnaryOperator, IndexKeyKind,
    IndexSignature, LiteralProvenance, ObjectShape, Property, ShapeParameter, ShapeSignature,
    Signature, TypeId, TypeKind, TypeStore, UnionPolicy,
};

#[derive(Debug)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub type_count: usize,
    pub semantic_completion: SemanticCompletion,
}

pub fn check_program(program: &Program, options: &CompilerOptions) -> CheckResult {
    Checker::new(program, options).check()
}

#[derive(Clone, Copy)]
enum DeclarationModel<'a> {
    Variable {
        declaration: &'a VariableDeclaration,
        scope: ScopeId,
    },
    Parameter {
        parameter: &'a Parameter,
        scope: ScopeId,
    },
    Function {
        declaration: &'a FunctionDeclaration,
        scope: ScopeId,
    },
    TypeAlias {
        declaration: &'a TypeAliasDeclaration,
        scope: ScopeId,
    },
    Interface {
        declaration: &'a InterfaceDeclaration,
        scope: ScopeId,
    },
    Class {
        identity: DeclId,
        declaration: &'a ClassDeclaration,
        scope: ScopeId,
    },
}

#[derive(Debug, Clone, Copy)]
enum QueryState {
    /// Active-frame marker only; it is removed when the query is incomplete.
    Computing,
    /// The sole persistent cache state. Only complete answers may enter it.
    Ready(TypeId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PropertyQueryOrigin {
    query: TypeId,
    name: String,
    span: Span,
    property_order: Option<projection_model::PropertyOrderTree>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedAccessOrigin {
    query: TypeId,
    span: Span,
    receiver_order: Option<projection_model::PropertyOrderTree>,
    receiver_alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DiagnosticIdentity {
    DiagnosticText(String),
    Relation(super::relation::RelationFailure),
    MissingProperty(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConstructOrigin {
    query: TypeId,
    argument_span: Span,
}

struct Checker<'a> {
    program: &'a Program,
    options: &'a CompilerOptions,
    store: TypeStore,
    models: FxHashMap<DeclId, DeclarationModel<'a>>,
    parameter_type_overrides: FxHashMap<DeclId, TypeId>,
    deferred_anonymous_parameters: HashSet<DeclId>,
    forbidden_default_type_parameters: Vec<HashSet<TypeId>>,
    value_queries: FxHashMap<DeclId, QueryState>,
    force_queries: FxHashMap<TypeId, QueryState>,
    // Syntax contexts and diagnostic origins are session data. Neither is
    // part of semantic interning or query identity.
    required_type_contexts: FxHashMap<Span, HashMap<String, TypeId>>,
    complete_required_type_nodes: HashSet<Span>,
    property_query_origins: Vec<PropertyQueryOrigin>,
    indexed_access_origins: Vec<IndexedAccessOrigin>,
    construct_origins: Vec<ConstructOrigin>,
    // Per-use inferred types are diagnostic provenance, not a semantic cache:
    // the immediately following relation query uses them to elaborate every
    // contextual array element at its own source span.
    expression_type_origins: FxHashMap<(FileId, NodeId), TypeId>,
    expression_order_origins: FxHashMap<(FileId, NodeId), projection_model::PropertyOrderTree>,
    diagnostics: Vec<Diagnostic>,
    reported: HashSet<(FileId, u32, u32, DiagnosticIdentity)>,
    semantic_completion: SemanticCompletion,
}

impl<'a> Checker<'a> {
    fn new(program: &'a Program, options: &'a CompilerOptions) -> Self {
        let mut checker = Self {
            program,
            options,
            store: TypeStore::new(),
            models: FxHashMap::default(),
            parameter_type_overrides: FxHashMap::default(),
            deferred_anonymous_parameters: HashSet::new(),
            forbidden_default_type_parameters: Vec::new(),
            value_queries: FxHashMap::default(),
            force_queries: FxHashMap::default(),
            required_type_contexts: FxHashMap::default(),
            complete_required_type_nodes: HashSet::new(),
            property_query_origins: Vec::new(),
            indexed_access_origins: Vec::new(),
            construct_origins: Vec::new(),
            expression_type_origins: FxHashMap::default(),
            expression_order_origins: FxHashMap::default(),
            diagnostics: Vec::new(),
            reported: HashSet::new(),
            semantic_completion: SemanticCompletion::Complete,
        };
        checker.collect_models();
        checker
    }

    fn check(mut self) -> CheckResult {
        self.require_explicit_type_positions();
        for file_id in &self.program.source_order {
            let file = &self.program.files[file_id.0 as usize];
            for statement in &file.syntax.statements {
                self.check_statement(file.source.id, ScopeId(0), statement, None, None);
            }
        }
        self.flush_property_diagnostics();
        self.flush_indexed_access_diagnostics();
        self.flush_construct_diagnostics();
        CheckResult {
            diagnostics: self.diagnostics,
            type_count: self.store.len(),
            semantic_completion: self.semantic_completion,
        }
    }

    fn collect_models(&mut self) {
        for file_id in &self.program.source_order {
            let file = &self.program.files[file_id.0 as usize];
            for statement in &file.syntax.statements {
                self.collect_statement_model(file.source.id, statement, ScopeId(0));
            }
        }
    }

    fn collect_statement_model(&mut self, file: FileId, statement: &'a Statement, scope: ScopeId) {
        let bound = &self.program.files[file.0 as usize].bindings;
        match &statement.kind {
            StatementKind::Variable(declaration) => {
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::Variable,
                    &declaration.name,
                ) {
                    self.models
                        .insert(id, DeclarationModel::Variable { declaration, scope });
                }
            }
            StatementKind::Function(declaration) => {
                let function_scope = bound
                    .scope_for_node
                    .get(&statement.id)
                    .copied()
                    .unwrap_or(scope);
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::Function,
                    &declaration.name,
                ) {
                    self.models.insert(
                        id,
                        DeclarationModel::Function {
                            declaration,
                            scope: function_scope,
                        },
                    );
                }
                let mut seen_parameters = HashSet::new();
                for parameter in declaration
                    .parameters
                    .iter()
                    .filter(|parameter| seen_parameters.insert(parameter.name.as_str()))
                {
                    if let Some(id) = self.find_declaration(
                        file,
                        statement.id,
                        DeclarationKind::Parameter,
                        &parameter.name,
                    ) {
                        self.models.insert(
                            id,
                            DeclarationModel::Parameter {
                                parameter,
                                scope: function_scope,
                            },
                        );
                    }
                }
                for nested in &declaration.body {
                    let nested_scope = bound
                        .scope_for_node
                        .get(&nested.id)
                        .copied()
                        .unwrap_or(function_scope);
                    self.collect_statement_model(file, nested, nested_scope);
                }
            }
            StatementKind::TypeAlias(declaration) => {
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::TypeAlias,
                    &declaration.name,
                ) {
                    self.models
                        .insert(id, DeclarationModel::TypeAlias { declaration, scope });
                }
            }
            StatementKind::Interface(declaration) => {
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::Interface,
                    &declaration.name,
                ) {
                    self.models
                        .insert(id, DeclarationModel::Interface { declaration, scope });
                }
            }
            StatementKind::Class(declaration) => {
                let ids = bound
                    .declarations
                    .iter()
                    .filter(|candidate| {
                        candidate.owner == statement.id
                            && candidate.kind == DeclarationKind::Class
                            && candidate.name == declaration.name
                    })
                    .map(|candidate| candidate.id)
                    .collect::<Vec<_>>();
                let Some(identity) = ids.first().copied() else {
                    return;
                };
                for id in ids {
                    self.models.insert(
                        id,
                        DeclarationModel::Class {
                            identity,
                            declaration,
                            scope,
                        },
                    );
                }
            }
            StatementKind::Block(statements) => {
                for nested in statements {
                    let nested_scope = bound
                        .scope_for_node
                        .get(&nested.id)
                        .copied()
                        .unwrap_or(scope);
                    self.collect_statement_model(file, nested, nested_scope);
                }
            }
            StatementKind::If(control_flow) => {
                let then_scope = bound
                    .scope_for_node
                    .get(&control_flow.then_statement.id)
                    .copied()
                    .unwrap_or(scope);
                self.collect_statement_model(file, &control_flow.then_statement, then_scope);
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = bound
                        .scope_for_node
                        .get(&else_statement.id)
                        .copied()
                        .unwrap_or(scope);
                    self.collect_statement_model(file, else_statement, else_scope);
                }
            }
            StatementKind::Switch(control_flow) => {
                let switch_scope = bound
                    .scope_for_node
                    .get(&statement.id)
                    .copied()
                    .unwrap_or(scope);
                for clause in &control_flow.clauses {
                    for nested in &clause.statements {
                        let nested_scope = bound
                            .scope_for_node
                            .get(&nested.id)
                            .copied()
                            .unwrap_or(switch_scope);
                        self.collect_statement_model(file, nested, nested_scope);
                    }
                }
            }
            StatementKind::Import(_)
            | StatementKind::Export(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Return(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
    }

    fn find_declaration(
        &self,
        file: FileId,
        owner: NodeId,
        kind: DeclarationKind,
        name: &str,
    ) -> Option<DeclId> {
        self.program.files[file.0 as usize]
            .bindings
            .declarations
            .iter()
            .find(|declaration| {
                declaration.owner == owner && declaration.kind == kind && declaration.name == name
            })
            .map(|declaration| declaration.id)
    }

    fn check_class(&mut self, file: FileId, declaration: &ClassDeclaration) {
        if declaration.declared
            || is_declaration_source(&self.program.files[file.0 as usize].source.path)
        {
            return;
        }

        for (index, member) in declaration.members.iter().enumerate() {
            match &member.kind {
                ClassMemberKind::Constructor {
                    has_body: false, ..
                } if !member.modifiers.abstract_member && !member.modifiers.declared => {
                    let next_is_constructor =
                        declaration.members.get(index + 1).is_some_and(|next| {
                            matches!(next.kind, ClassMemberKind::Constructor { .. })
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
                    let Some(next) = declaration.members.get(index + 1) else {
                        self.report_missing_method_implementation(file, member.name_span);
                        continue;
                    };
                    let ClassMemberKind::Method {
                        has_body: next_has_body,
                        accessor: None,
                        ..
                    } = &next.kind
                    else {
                        self.report_missing_method_implementation(file, member.name_span);
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
                        self.report_missing_method_implementation(file, member.name_span);
                    }
                }
                ClassMemberKind::Constructor { .. }
                | ClassMemberKind::Property { .. }
                | ClassMemberKind::Method { .. } => {}
            }
        }
    }

    fn report_missing_method_implementation(&mut self, file: FileId, span: Span) {
        self.push_diagnostic(
            file,
            span,
            "Function implementation is missing or not immediately following the declaration."
                .to_string(),
            2391,
        );
    }

    fn check_variable(
        &mut self,
        file: FileId,
        scope: ScopeId,
        owner: NodeId,
        declaration: &VariableDeclaration,
    ) {
        let (annotation, annotation_is_complete) =
            declaration
                .annotation
                .as_ref()
                .map_or((None, true), |annotation| {
                    let ty = self.resolve_type_node(file, scope, annotation, &HashMap::new());
                    let is_complete = self.complete_required_type_nodes.contains(&annotation.span);
                    (Some(ty), is_complete)
                });
        let initializer = declaration
            .initializer
            .as_ref()
            .map(|initializer| self.infer_expression(file, scope, initializer, annotation));
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
        if let Some(id) =
            self.find_declaration(file, owner, DeclarationKind::Variable, &declaration.name)
        {
            let initializer = initializer.map(|inferred| {
                if declaration.declaration_kind == VariableKind::Const {
                    inferred
                } else {
                    self.widen(inferred)
                }
            });
            let value = annotation
                .or(initializer)
                .unwrap_or(self.store.builtins.any);
            if annotation_is_complete && self.is_cacheable_type(value) {
                self.value_queries.insert(id, QueryState::Ready(value));
            } else {
                self.value_queries.remove(&id);
            }
        }
    }

    fn check_function(&mut self, file: FileId, owner: NodeId, declaration: &FunctionDeclaration) {
        let Some(id) =
            self.find_declaration(file, owner, DeclarationKind::Function, &declaration.name)
        else {
            return;
        };
        let expected_return = self.require_function_signature(id);
        let scope = self.program.files[file.0 as usize]
            .bindings
            .scope_for_node
            .get(&owner)
            .copied()
            .unwrap_or(ScopeId(0));
        let expected_return_order = declaration.return_type.as_ref().and_then(|return_type| {
            self.property_order_for_type_node_root(file, scope, return_type)
        });
        for parameter in &declaration.parameters {
            if parameter.annotation.is_none()
                && parameter.initializer.is_none()
                && self.options.effective_no_implicit_any()
            {
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
        for statement in &declaration.body {
            let statement_scope = self.program.files[file.0 as usize]
                .bindings
                .scope_for_node
                .get(&statement.id)
                .copied()
                .unwrap_or(scope);
            self.check_statement(
                file,
                statement_scope,
                statement,
                expected_return,
                expected_return_order.as_ref(),
            );
        }
    }

    fn declaration_value_type(&mut self, id: DeclId) -> Completion<TypeId> {
        if self.program.standard_library_declaration(id).is_some() {
            return Completion::Deferred;
        }
        if self.deferred_anonymous_parameters.contains(&id) {
            return Completion::Deferred;
        }
        if let Some(ty) = self.parameter_type_overrides.get(&id) {
            return Completion::Complete(*ty);
        }
        match self.value_queries.get(&id).copied() {
            Some(QueryState::Ready(ty)) => return Completion::Complete(ty),
            Some(QueryState::Computing) => return Completion::Cycle,
            None => {}
        }
        self.value_queries.insert(id, QueryState::Computing);
        let Some(model) = self.models.get(&id).copied() else {
            self.value_queries.remove(&id);
            return Completion::Deferred;
        };
        let result = match model {
            DeclarationModel::Variable { declaration, scope } => {
                if let Some(annotation) = &declaration.annotation {
                    Completion::Complete(self.resolve_type_node(
                        id.file,
                        scope,
                        annotation,
                        &HashMap::new(),
                    ))
                } else if let Some(initializer) = &declaration.initializer {
                    let inferred = self.infer_expression(id.file, scope, initializer, None);
                    let inferred = if declaration.declaration_kind == VariableKind::Const {
                        inferred
                    } else {
                        self.widen(inferred)
                    };
                    Completion::Complete(inferred)
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
            DeclarationModel::TypeAlias { .. } | DeclarationModel::Interface { .. } => {
                Completion::Deferred
            }
        };
        match result {
            Completion::Complete(ty) if self.is_cacheable_type(ty) => {
                self.value_queries.insert(id, QueryState::Ready(ty));
            }
            Completion::Complete(_)
            | Completion::Deferred
            | Completion::Cycle
            | Completion::Limit => {
                self.value_queries.remove(&id);
            }
        }
        result
    }

    fn resolve_type_node(
        &mut self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        type_parameters: &HashMap<String, TypeId>,
    ) -> TypeId {
        let lexical_parameters = self.required_type_contexts.get(&node.span).cloned();
        let merged_parameters;
        let type_parameters = if let Some(mut lexical_parameters) = lexical_parameters {
            lexical_parameters.extend(type_parameters.iter().map(|(name, ty)| (name.clone(), *ty)));
            merged_parameters = lexical_parameters;
            &merged_parameters
        } else {
            type_parameters
        };
        match &node.kind {
            TypeNodeKind::Keyword(keyword) => match keyword {
                KeywordType::Any => self.store.builtins.any,
                KeywordType::Unknown => self.store.builtins.unknown,
                KeywordType::Never => self.store.builtins.never,
                KeywordType::Void => self.store.builtins.void,
                KeywordType::Undefined => self.store.builtins.undefined,
                KeywordType::Null => self.store.builtins.null,
                KeywordType::Boolean => self.store.builtins.boolean,
                KeywordType::Number => self.store.builtins.number,
                KeywordType::String => self.store.builtins.string,
                KeywordType::BigInt => self.store.builtins.bigint,
                KeywordType::Object => self.store.builtins.object,
                KeywordType::Symbol => self.store.builtins.symbol,
                KeywordType::UniqueSymbol => self.store.deferred_unique_symbol(),
            },
            TypeNodeKind::Literal(literal) => {
                self.literal_type(literal, LiteralProvenance::Regular)
            }
            TypeNodeKind::Array(element) => {
                let element = self.resolve_type_node(file, scope, element, type_parameters);
                self.store.intern(TypeKind::Array(element))
            }
            TypeNodeKind::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| self.resolve_type_node(file, scope, element, type_parameters))
                    .collect();
                self.store.intern(TypeKind::Tuple(elements))
            }
            TypeNodeKind::Union(members) => {
                let policy = if members.iter().all(authored_structural_union_member) {
                    UnionPolicy::PreserveAuthoredStructuralOrder
                } else {
                    UnionPolicy::Canonical
                };
                let resolved_members = members
                    .iter()
                    .map(|member| self.resolve_type_node(file, scope, member, type_parameters))
                    .collect::<Vec<_>>();
                self.store.union(resolved_members, policy)
            }
            TypeNodeKind::Intersection(members) => {
                let members = members
                    .iter()
                    .map(|member| self.resolve_type_node(file, scope, member, type_parameters))
                    .collect::<Vec<_>>();
                self.store.intersection(members)
            }
            TypeNodeKind::Object(members) => {
                match self.resolve_object_members(file, scope, members, type_parameters) {
                    Completion::Complete(shape) => self.store.object_shape(shape),
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        self.store.deferred_object_shape()
                    }
                }
            }
            TypeNodeKind::Function {
                id,
                type_parameters: signature_type_parameters,
                parameters,
                return_type,
            } => {
                if !signature_type_parameters.is_empty() {
                    return self.store.deferred_generic_function();
                }
                let scope = self.node_scope(file, *id, scope);
                self.register_anonymous_parameter_types(file, scope, parameters, type_parameters);
                let parameters = match self.anonymous_signature_parameters(
                    file,
                    scope,
                    parameters,
                    type_parameters,
                ) {
                    Completion::Complete(parameters) => parameters,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        return self.store.deferred_generic_function();
                    }
                };
                let return_type = self.resolve_type_node(file, scope, return_type, type_parameters);
                self.store.intern(TypeKind::Function(Signature {
                    parameters,
                    return_type,
                }))
            }
            TypeNodeKind::Constructor {
                id,
                type_parameters: signature_type_parameters,
                parameters,
                return_type,
                ..
            } => {
                if !signature_type_parameters.is_empty() {
                    return self.store.deferred_generic_function();
                }
                let scope = self.node_scope(file, *id, scope);
                self.register_anonymous_parameter_types(file, scope, parameters, type_parameters);
                let parameters = match self.anonymous_signature_parameters(
                    file,
                    scope,
                    parameters,
                    type_parameters,
                ) {
                    Completion::Complete(parameters) => parameters,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        return self.store.deferred_generic_function();
                    }
                };
                let return_type = self.resolve_type_node(file, scope, return_type, type_parameters);
                self.store.intern(TypeKind::Function(Signature {
                    parameters,
                    return_type,
                }))
            }
            TypeNodeKind::Reference {
                name,
                name_span,
                arguments,
            } => {
                if let Some(ty) = type_parameters.get(name) {
                    if !arguments.is_empty() {
                        self.push_diagnostic(
                            file,
                            node.span,
                            format!("Type '{name}' is not generic."),
                            2315,
                        );
                        return self.store.builtins.error;
                    }
                    return *ty;
                }
                let arguments = arguments
                    .iter()
                    .map(|argument| self.resolve_type_node(file, scope, argument, type_parameters))
                    .collect::<Vec<_>>();
                let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Type) else {
                    self.push_diagnostic(
                        file,
                        *name_span,
                        format!("Cannot find name '{name}'."),
                        2304,
                    );
                    return self.store.builtins.error;
                };
                if self.program.standard_library.is_array_type(declaration) && arguments.len() == 1
                {
                    return self.store.intern(TypeKind::Array(arguments[0]));
                }
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Reference {
                        declaration,
                        arguments,
                    }))
            }
            TypeNodeKind::TypeQuery {
                name,
                name_span,
                segment_spans,
            } => {
                let mut segments = name.split('.');
                let root_name = segments.next().unwrap_or(name);
                let root_span = segment_spans.first().copied().unwrap_or(*name_span);
                let Some(declaration) = self.resolve_name(file, scope, root_name, Meaning::Value)
                else {
                    self.push_diagnostic(
                        file,
                        root_span,
                        format!("Cannot find name '{root_name}'."),
                        2304,
                    );
                    return self.store.builtins.error;
                };
                let root = self
                    .store
                    .intern(TypeKind::Deferred(DeferredType::Value(declaration)));
                let mut property_order = self.property_order_for_declaration(declaration);
                segments
                    .enumerate()
                    .fold(root, |object, (index, property)| {
                        let name_span = segment_spans.get(index + 1).copied().unwrap_or(*name_span);
                        let receiver_order = property_order.clone();
                        property_order = property_order
                            .as_ref()
                            .and_then(|order| order.property(property))
                            .cloned();
                        self.deferred_property_type_with_order(
                            object,
                            property,
                            name_span,
                            receiver_order,
                        )
                    })
            }
            TypeNodeKind::Infer {
                name,
                name_span,
                constraint,
            } => {
                if let Some(ty) = type_parameters.get(name) {
                    *ty
                } else {
                    if let Some(constraint) = constraint {
                        let _ = self.resolve_type_node(file, scope, constraint, type_parameters);
                    }
                    self.store.intern(TypeKind::TypeParameter {
                        declaration: DeclId {
                            file,
                            local: name_span.start | (1 << 31),
                        },
                        index: 0,
                        name: name.clone(),
                    })
                }
            }
            TypeNodeKind::Predicate {
                parameter,
                asserts,
                ty,
                ..
            } => {
                let asserted = ty
                    .as_ref()
                    .map(|ty| self.resolve_type_node(file, scope, ty, type_parameters));
                let parameter_is_bound = self
                    .resolve_name(file, scope, parameter, Meaning::Value)
                    .is_some();
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Predicate {
                        parameter: parameter.clone(),
                        asserted,
                        asserts: *asserts,
                        parameter_is_bound,
                    }))
            }
            TypeNodeKind::KeyOf(operand) => {
                let operand = self.resolve_type_node(file, scope, operand, type_parameters);
                self.store
                    .intern(TypeKind::Deferred(DeferredType::KeyOf(operand)))
            }
            TypeNodeKind::Readonly(inner) | TypeNodeKind::Parenthesized(inner) => {
                self.resolve_type_node(file, scope, inner, type_parameters)
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                let check = self.resolve_type_node(file, scope, check_type, type_parameters);
                let extends = self.resolve_type_node(file, scope, extends_type, type_parameters);
                let true_parameters =
                    self.conditional_true_type_parameters(file, extends_type, type_parameters);
                let true_type = self.resolve_type_node(file, scope, true_type, &true_parameters);
                let false_type = self.resolve_type_node(file, scope, false_type, type_parameters);
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Conditional {
                        check,
                        extends,
                        when_true: true_type,
                        when_false: false_type,
                    }))
            }
            TypeNodeKind::Mapped {
                parameter,
                parameter_span,
                constraint,
                name_type,
                value_type,
                readonly,
                optional,
                ..
            } => {
                let constraint = self.resolve_type_node(file, scope, constraint, type_parameters);
                let parameter_type = self.store.intern(TypeKind::TypeParameter {
                    declaration: DeclId {
                        file,
                        local: parameter_span.start | (1 << 31),
                    },
                    index: 0,
                    name: parameter.clone(),
                });
                let mut mapped_parameters = type_parameters.clone();
                mapped_parameters.insert(parameter.clone(), parameter_type);
                let name_type = name_type.as_ref().map(|name_type| {
                    self.resolve_type_node(file, scope, name_type, &mapped_parameters)
                });
                let value = self.resolve_type_node(file, scope, value_type, &mapped_parameters);
                self.store.intern(TypeKind::Deferred(DeferredType::Mapped {
                    constraint,
                    name_type,
                    value,
                    readonly: *readonly,
                    optional: *optional,
                }))
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                let index_span = index.span;
                let receiver_order = self.property_order_for_type_node_root(file, scope, object);
                let receiver_alias = projection_model::authored_type_reference_name(object);
                let object = self.resolve_type_node(file, scope, object, type_parameters);
                let index = self.resolve_type_node(file, scope, index, type_parameters);
                self.deferred_indexed_access_type(
                    object,
                    index,
                    index_span,
                    receiver_order,
                    receiver_alias,
                )
            }
            TypeNodeKind::Missing => self.store.builtins.error,
        }
    }

    fn infer_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        expected: Option<TypeId>,
    ) -> TypeId {
        let order = self.property_order_for_expression(file, scope, expression);
        let inferred = self.infer_expression_inner(file, scope, expression, expected);
        self.expression_type_origins
            .insert((file, expression.id), inferred);
        if let Some(order) = order {
            self.expression_order_origins
                .insert((file, expression.id), order);
        }
        inferred
    }

    fn infer_expression_inner(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        expected: Option<TypeId>,
    ) -> TypeId {
        match &expression.kind {
            ExpressionKind::Identifier {
                name,
                name_span,
                entity_name,
            } => {
                if !entity_name {
                    self.semantic_completion = self
                        .semantic_completion
                        .combine(SemanticCompletion::Deferred);
                    return self.store.builtins.error;
                }
                let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Value) else {
                    self.push_diagnostic(
                        file,
                        *name_span,
                        format!("Cannot find name '{name}'."),
                        2304,
                    );
                    return self.store.builtins.error;
                };
                if self
                    .program
                    .standard_library
                    .is_undefined_value(declaration)
                {
                    return self.store.builtins.undefined;
                }
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Value(declaration)))
            }
            ExpressionKind::Literal(literal) => {
                self.literal_type(literal, LiteralProvenance::Fresh)
            }
            ExpressionKind::Object(properties) => {
                let properties = properties
                    .iter()
                    .map(|property| {
                        let expected = expected.and_then(|expected| {
                            match self.contextual_object_property_type(
                                expected,
                                &property.name,
                                Some(expression),
                            ) {
                                ContextualPropertyType::Known(expected) => Some(expected),
                                ContextualPropertyType::Absent => None,
                                ContextualPropertyType::Deferred => {
                                    self.semantic_completion = self
                                        .semantic_completion
                                        .combine(SemanticCompletion::Deferred);
                                    None
                                }
                            }
                        });
                        let inferred =
                            self.infer_expression(file, scope, &property.value, expected);
                        let inferred = if expected.is_some() {
                            inferred
                        } else {
                            self.widen(inferred)
                        };
                        Property {
                            name: property.name.clone(),
                            ty: inferred,
                            optional: false,
                            readonly: false,
                        }
                    })
                    .collect();
                self.store.object(properties)
            }
            ExpressionKind::Array(elements) => {
                let expected_element =
                    expected.and_then(|expected| self.contextual_array_element_type(expected));
                let elements = elements
                    .iter()
                    .map(|element| {
                        let inferred =
                            self.infer_expression(file, scope, element, expected_element);
                        if expected_element.is_some() {
                            inferred
                        } else {
                            self.widen(inferred)
                        }
                    })
                    .collect::<Vec<_>>();
                let element = self.store.union(elements, UnionPolicy::Canonical);
                self.store.intern(TypeKind::Array(element))
            }
            ExpressionKind::Call { callee, arguments } => {
                let callee_type = if let ExpressionKind::Member {
                    object,
                    name,
                    name_span,
                } = &callee.kind
                {
                    let ty =
                        self.infer_member_expression(file, scope, object, name, *name_span, true);
                    self.expression_type_origins.insert((file, callee.id), ty);
                    ty
                } else {
                    self.infer_expression(file, scope, callee, None)
                };
                let completion = self.force_type(callee_type, 0);
                let callee_type = match self.require_completion(completion) {
                    Completion::Complete(callee_type) => callee_type,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        for argument in arguments {
                            let _ = self.infer_expression(file, scope, argument, None);
                        }
                        return self.store.intern(TypeKind::Deferred(DeferredType::Call {
                            callee: callee_type,
                            argument_count: arguments.len(),
                        }));
                    }
                };
                let Some(signature) = self.callable_signature(callee_type) else {
                    if self.authored_shape_display_is_unavailable(callee_type) {
                        for argument in arguments {
                            let _ = self.infer_expression(file, scope, argument, None);
                        }
                        return self.store.intern(TypeKind::Deferred(DeferredType::Call {
                            callee: callee_type,
                            argument_count: arguments.len(),
                        }));
                    }
                    if !matches!(
                        self.store.kind(callee_type),
                        TypeKind::Any | TypeKind::Error
                    ) {
                        let name = self.store.display(callee_type);
                        self.push_diagnostic(
                            file,
                            callee.span,
                            format!("This expression is not callable. Type '{name}' has no call signatures."),
                            2349,
                        );
                    }
                    return self.store.builtins.any;
                };
                let rest_index = signature
                    .parameters
                    .iter()
                    .position(|parameter| parameter.rest);
                let required = signature
                    .parameters
                    .iter()
                    .take(rest_index.unwrap_or(signature.parameters.len()))
                    .filter(|parameter| !parameter.optional)
                    .count();
                let maximum =
                    rest_index.map_or(Some(signature.parameters.len()), |index| {
                        match self.store.kind(signature.parameters[index].ty) {
                            TypeKind::Tuple(elements) => Some(index + elements.len()),
                            _ => None,
                        }
                    });
                let too_few = arguments.len() < required;
                let too_many = maximum.is_some_and(|maximum| arguments.len() > maximum);
                if too_few || too_many {
                    let expected_count = if maximum.is_none() {
                        format!("at least {required}")
                    } else if maximum == Some(required) {
                        required.to_string()
                    } else {
                        format!("{}-{}", required, maximum.unwrap_or(required))
                    };
                    self.push_diagnostic(
                        file,
                        if too_many {
                            arguments[maximum.unwrap_or(arguments.len())].span
                        } else {
                            callee.span
                        },
                        format!(
                            "Expected {expected_count} arguments, but got {}.",
                            arguments.len()
                        ),
                        2554,
                    );
                }
                let mut stopped_argument_relations = too_few || too_many;
                for (index, argument) in arguments.iter().enumerate() {
                    let Some(parameter) = signature.parameters.get(index).or_else(|| {
                        rest_index.and_then(|rest_index| signature.parameters.get(rest_index))
                    }) else {
                        let _ = self.infer_expression(file, scope, argument, None);
                        continue;
                    };
                    let expected = if parameter.rest {
                        match self.store.kind(parameter.ty) {
                            TypeKind::Array(element) => *element,
                            TypeKind::Tuple(elements) => rest_index
                                .and_then(|rest_index| index.checked_sub(rest_index))
                                .and_then(|index| elements.get(index))
                                .copied()
                                .unwrap_or(parameter.ty),
                            _ => parameter.ty,
                        }
                    } else {
                        parameter.ty
                    };
                    let actual = self.infer_expression(file, scope, argument, Some(expected));
                    let target_order = self.relation_order_for_call_argument(
                        file,
                        scope,
                        callee,
                        index,
                        parameter.rest,
                    );
                    if !stopped_argument_relations {
                        stopped_argument_relations = !matches!(
                            self.report_relation(
                                actual,
                                expected,
                                argument.span,
                                Some(argument),
                                target_order,
                                RelationMode::Assignment,
                                RelationDiagnosticStyle::Argument,
                            ),
                            RelationDiagnosticOutcome::Compatible
                        );
                    }
                }
                signature.return_type
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                let callee_type = self.infer_expression(file, scope, callee, None);
                let type_arguments = type_arguments
                    .iter()
                    .map(|argument| self.resolve_type_node(file, scope, argument, &HashMap::new()))
                    .collect::<Vec<_>>();
                let argument_span = arguments
                    .first()
                    .zip(arguments.last())
                    .map_or(callee.span, |(first, last)| first.span.merge(last.span));
                for argument in arguments {
                    let _ = self.infer_expression(file, scope, argument, None);
                }
                let query = self.deferred_construct_type(
                    callee_type,
                    type_arguments,
                    arguments.len(),
                    argument_span,
                );
                let completion = self.force_type(query, 0);
                match self.require_completion(completion) {
                    Completion::Complete(instance) => instance,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => query,
                }
            }
            ExpressionKind::Member {
                object,
                name,
                name_span,
            } => self.infer_member_expression(file, scope, object, name, *name_span, false),
            ExpressionKind::Arrow {
                parameters,
                return_type: annotation,
                body,
            } => self.infer_arrow_expression(
                file,
                scope,
                expression.id,
                parameters,
                annotation.as_ref(),
                body,
                expected,
            ),
            ExpressionKind::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.infer_expression(file, scope, left, None);
                let right = self.infer_expression(file, scope, right, None);
                if *operator == BinaryOperator::Add
                    && (self.is_string_like(left) || self.is_string_like(right))
                {
                    self.store.builtins.string
                } else if matches!(
                    operator,
                    BinaryOperator::LessThan
                        | BinaryOperator::LessThanEquals
                        | BinaryOperator::GreaterThan
                        | BinaryOperator::GreaterThanEquals
                        | BinaryOperator::Equals
                        | BinaryOperator::NotEquals
                        | BinaryOperator::StrictEquals
                        | BinaryOperator::StrictNotEquals
                        | BinaryOperator::In
                        | BinaryOperator::InstanceOf
                ) {
                    self.store.builtins.boolean
                } else if matches!(
                    operator,
                    BinaryOperator::LogicalAnd
                        | BinaryOperator::LogicalOr
                        | BinaryOperator::NullishCoalesce
                ) {
                    let operator = match operator {
                        BinaryOperator::LogicalAnd => DeferredLogicalOperator::And,
                        BinaryOperator::LogicalOr => DeferredLogicalOperator::Or,
                        BinaryOperator::NullishCoalesce => DeferredLogicalOperator::Nullish,
                        _ => unreachable!(),
                    };
                    self.store.intern(TypeKind::Deferred(DeferredType::Logical {
                        operator,
                        left,
                        right,
                    }))
                } else {
                    self.store.builtins.number
                }
            }
            ExpressionKind::Unary { operator, operand } => {
                let operand = self.infer_expression(file, scope, operand, None);
                match operator {
                    UnaryOperator::Not | UnaryOperator::Delete => self.store.builtins.boolean,
                    UnaryOperator::TypeOf => self.store.builtins.string,
                    UnaryOperator::Void => self.store.builtins.undefined,
                    UnaryOperator::Await => {
                        self.store.intern(TypeKind::Deferred(DeferredType::Unary {
                            operator: DeferredUnaryOperator::Await,
                            operand,
                        }))
                    }
                    UnaryOperator::Plus | UnaryOperator::Minus | UnaryOperator::BitwiseNot => {
                        self.store.intern(TypeKind::Deferred(DeferredType::Unary {
                            operator: match operator {
                                UnaryOperator::Plus => DeferredUnaryOperator::Plus,
                                UnaryOperator::Minus => DeferredUnaryOperator::Minus,
                                UnaryOperator::BitwiseNot => DeferredUnaryOperator::BitwiseNot,
                                _ => unreachable!(),
                            },
                            operand,
                        }))
                    }
                }
            }
            ExpressionKind::Assignment { left, right } => {
                let target = self.infer_expression(file, scope, left, None);
                let source = self.infer_expression(file, scope, right, Some(target));
                self.report_relation(
                    source,
                    target,
                    right.span,
                    Some(right),
                    self.expression_order_origins.get(&(file, left.id)).cloned(),
                    RelationMode::Assignment,
                    RelationDiagnosticStyle::Type,
                );
                target
            }
            ExpressionKind::As { expression, ty } => {
                self.infer_expression(file, scope, expression, None);
                self.resolve_type_node(file, scope, ty, &HashMap::new())
            }
            ExpressionKind::Parenthesized(inner) => {
                self.infer_expression(file, scope, inner, expected)
            }
            ExpressionKind::Missing => self.store.builtins.error,
        }
    }

    fn resolve_name(
        &self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        meaning: Meaning,
    ) -> Option<DeclId> {
        self.program.files[file.0 as usize]
            .bindings
            .resolve(scope, name, meaning)
            .or_else(|| self.program.resolve_global(name, meaning))
    }

    fn force_deferred(
        &mut self,
        ty: TypeId,
        deferred: DeferredType,
        depth: usize,
    ) -> Completion<TypeId> {
        match self.force_queries.get(&ty).copied() {
            Some(QueryState::Ready(result)) => return Completion::Complete(result),
            Some(QueryState::Computing) => {
                self.report_deferred_cycle(&deferred);
                return Completion::Cycle;
            }
            None => {}
        }
        // This is the sole evaluator-expansion budget. Declaration lookup and
        // the RelationContext adapter must propagate this completion instead
        // of imposing competing limits on already-complete values.
        if depth > 100 {
            self.report_complexity(&deferred);
            return Completion::Limit;
        }
        self.force_queries.insert(ty, QueryState::Computing);
        let result = match deferred.clone() {
            DeferredType::Reference {
                declaration,
                arguments,
            } => self.evaluate_reference(declaration, &arguments),
            DeferredType::Value(declaration) => self.declaration_value_type(declaration),
            DeferredType::Call {
                callee,
                argument_count,
            } => self.evaluate_call(callee, argument_count, depth + 1),
            DeferredType::Construct {
                callee,
                type_arguments,
                argument_count,
            } => self.evaluate_construct(callee, &type_arguments, argument_count, depth + 1),
            DeferredType::Property { object, name } => {
                self.evaluate_property(ty, object, &name, depth + 1)
            }
            DeferredType::Logical {
                operator,
                left,
                right,
            } => self.evaluate_logical(operator, left, right, depth + 1),
            DeferredType::Unary { operator, operand } => {
                self.evaluate_unary(operator, operand, depth + 1)
            }
            DeferredType::KeyOf(operand) => self.evaluate_keyof(operand, depth + 1),
            DeferredType::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } if matches!(
                self.store.kind(check),
                TypeKind::Error | TypeKind::Invalid(_)
            ) || matches!(
                self.store.kind(extends),
                TypeKind::Error | TypeKind::Invalid(_)
            ) =>
            {
                Completion::Complete(
                    self.store
                        .union([when_true, when_false], UnionPolicy::Canonical),
                )
            }
            DeferredType::Predicate { .. }
            | DeferredType::Conditional { .. }
            | DeferredType::Mapped { .. }
            | DeferredType::BigIntLiteral
            | DeferredType::UniqueSymbol
            | DeferredType::GenericFunction
            | DeferredType::ObjectShape => Completion::Deferred,
            DeferredType::IndexedAccess { object, index } => {
                self.evaluate_indexed_access(object, index, depth + 1)
            }
        };
        let result = match result {
            Completion::Complete(result)
                if matches!(self.store.kind(result), TypeKind::Deferred(_)) =>
            {
                self.force_type(result, depth + 1)
            }
            other => other,
        };
        match result {
            Completion::Complete(result) if self.is_cacheable_type(result) => {
                self.force_queries.insert(ty, QueryState::Ready(result));
            }
            Completion::Complete(_) | Completion::Deferred => {
                self.force_queries.remove(&ty);
            }
            Completion::Cycle => {
                self.force_queries.remove(&ty);
                self.report_deferred_cycle(&deferred);
            }
            Completion::Limit => {
                self.force_queries.remove(&ty);
                self.report_complexity(&deferred);
            }
        }
        result
    }

    fn evaluate_reference(
        &mut self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> Completion<TypeId> {
        if self
            .program
            .standard_library_declaration(declaration)
            .is_some()
        {
            if self
                .program
                .standard_library
                .is_string_record_type(declaration)
            {
                let [key, value] = arguments else {
                    return Completion::Deferred;
                };
                if matches!(self.store.kind(*key), TypeKind::String) {
                    return Completion::Complete(self.store.object_shape(ObjectShape {
                        index_signatures: vec![IndexSignature {
                            key: IndexKeyKind::String,
                            value: *value,
                            readonly: false,
                        }],
                        ..ObjectShape::default()
                    }));
                }
            }
            return Completion::Deferred;
        }
        let Some(model) = self.models.get(&declaration).copied() else {
            return Completion::Deferred;
        };
        match model {
            DeclarationModel::TypeAlias {
                declaration: alias,
                scope,
            } => {
                let parameters = self.substitution(declaration, &alias.type_parameters, arguments);
                Completion::Complete(self.resolve_type_node(
                    declaration.file,
                    scope,
                    &alias.ty,
                    &parameters,
                ))
            }
            DeclarationModel::Interface {
                declaration: interface,
                scope,
            } => {
                let parameters =
                    self.substitution(declaration, &interface.type_parameters, arguments);
                match self.resolve_object_members(
                    declaration.file,
                    scope,
                    &interface.members,
                    &parameters,
                ) {
                    Completion::Complete(shape) => {
                        Completion::Complete(self.store.object_shape(shape))
                    }
                    Completion::Deferred => Completion::Deferred,
                    Completion::Cycle => Completion::Cycle,
                    Completion::Limit => Completion::Limit,
                }
            }
            DeclarationModel::Class {
                identity,
                declaration: class,
                scope,
            } => self.evaluate_class_instance(identity, class, scope, arguments),
            DeclarationModel::Variable { .. }
            | DeclarationModel::Parameter { .. }
            | DeclarationModel::Function { .. } => Completion::Deferred,
        }
    }

    fn substitution(
        &mut self,
        declaration: DeclId,
        parameters: &[crate::syntax::TypeParameterDeclaration],
        arguments: &[TypeId],
    ) -> HashMap<String, TypeId> {
        parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let ty = arguments.get(index).copied().unwrap_or_else(|| {
                    self.store.intern(TypeKind::TypeParameter {
                        declaration,
                        index: index as u32,
                        name: parameter.name.clone(),
                    })
                });
                (parameter.name.clone(), ty)
            })
            .collect()
    }

    fn evaluate_call(
        &mut self,
        callee: TypeId,
        argument_count: usize,
        depth: usize,
    ) -> Completion<TypeId> {
        let callee = match self.force_type(callee, depth) {
            Completion::Complete(callee) => callee,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        match self.store.kind(callee) {
            TypeKind::Function(signature)
                if argument_count == 0 && signature.parameters.is_empty() =>
            {
                Completion::Complete(signature.return_type)
            }
            TypeKind::ShapeFunction(signature)
                if argument_count == 0 && signature.parameters.is_empty() =>
            {
                Completion::Complete(signature.return_type)
            }
            TypeKind::Object(shape)
                if argument_count == 0
                    && shape.call_signatures.len() == 1
                    && shape.call_signatures[0].parameters.is_empty() =>
            {
                Completion::Complete(shape.call_signatures[0].return_type)
            }
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(callee),
            _ => Completion::Deferred,
        }
    }

    fn evaluate_logical(
        &mut self,
        operator: DeferredLogicalOperator,
        left: TypeId,
        right: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let left = match self.force_type(left, depth) {
            Completion::Complete(left) => left,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let left_kind = self.store.kind(left);
        if matches!(left_kind, TypeKind::Error | TypeKind::Invalid(_)) {
            return Completion::Complete(left);
        }
        match operator {
            DeferredLogicalOperator::And => match known_truthiness(left_kind) {
                Some(true) => Completion::Complete(right),
                Some(false) => Completion::Complete(left),
                None => Completion::Deferred,
            },
            DeferredLogicalOperator::Or => match known_truthiness(left_kind) {
                Some(true) => Completion::Complete(left),
                Some(false) => Completion::Complete(right),
                None => Completion::Deferred,
            },
            DeferredLogicalOperator::Nullish => {
                if matches!(left_kind, TypeKind::Null | TypeKind::Undefined) {
                    Completion::Complete(right)
                } else if matches!(
                    left_kind,
                    TypeKind::Boolean
                        | TypeKind::Number
                        | TypeKind::String
                        | TypeKind::BigInt
                        | TypeKind::ObjectKeyword
                        | TypeKind::Symbol
                        | TypeKind::LiteralBoolean(_, _)
                        | TypeKind::LiteralNumber(_, _)
                        | TypeKind::LiteralString(_, _)
                        | TypeKind::Array(_)
                        | TypeKind::Tuple(_)
                        | TypeKind::Object(_)
                        | TypeKind::Function(_)
                ) {
                    Completion::Complete(left)
                } else {
                    Completion::Deferred
                }
            }
        }
    }

    fn evaluate_unary(
        &mut self,
        operator: DeferredUnaryOperator,
        operand: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let operand = match self.force_type(operand, depth) {
            Completion::Complete(operand) => operand,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        if operator == DeferredUnaryOperator::Await {
            return Completion::Complete(operand);
        }
        match self.store.kind(operand) {
            TypeKind::Number | TypeKind::LiteralNumber(_, _) | TypeKind::Any => {
                Completion::Complete(self.store.builtins.number)
            }
            TypeKind::BigInt if operator != DeferredUnaryOperator::Plus => {
                Completion::Complete(self.store.builtins.bigint)
            }
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(operand),
            _ => Completion::Deferred,
        }
    }

    fn complete_type(&mut self, ty: TypeId) -> Option<TypeId> {
        let completion = self.force_type(ty, 0);
        match self.require_completion(completion) {
            Completion::Complete(ty) => Some(ty),
            Completion::Deferred | Completion::Cycle | Completion::Limit => None,
        }
    }

    /// Aggregate only results that escape at a required checking boundary.
    /// Creating or recursively carrying a symbolic deferred type is not itself
    /// incomplete; callers invoke this after forcing is necessary to decide a
    /// diagnostic, contextual type, or other user-visible checked result.
    const fn require_completion<T>(&mut self, completion: Completion<T>) -> Completion<T> {
        let observed = match &completion {
            Completion::Complete(_) => SemanticCompletion::Complete,
            Completion::Deferred => SemanticCompletion::Deferred,
            Completion::Cycle => SemanticCompletion::Cycle,
            Completion::Limit => SemanticCompletion::Limit,
        };
        self.semantic_completion = self.semantic_completion.combine(observed);
        completion
    }

    fn callable_signature(&self, ty: TypeId) -> Option<ShapeSignature> {
        match self.store.kind(ty) {
            TypeKind::Function(signature) => Some(ShapeSignature {
                parameters: signature
                    .parameters
                    .iter()
                    .map(|parameter| ShapeParameter {
                        ty: parameter.ty,
                        optional: parameter.optional,
                        rest: parameter.rest,
                    })
                    .collect(),
                return_type: signature.return_type,
            }),
            TypeKind::ShapeFunction(signature) => Some(signature.clone()),
            TypeKind::Object(shape) if shape.call_signatures.len() == 1 => {
                shape.call_signatures.first().cloned()
            }
            _ => None,
        }
    }

    fn literal_type(&mut self, literal: &Literal, provenance: LiteralProvenance) -> TypeId {
        match literal {
            Literal::String(value) => self
                .store
                .intern(TypeKind::LiteralString(value.clone(), provenance)),
            Literal::Number(value) => self.store.numeric_literal(value, provenance),
            Literal::BigInt(_) => self.store.deferred_bigint_literal(),
            Literal::Boolean(value) => self
                .store
                .intern(TypeKind::LiteralBoolean(*value, provenance)),
            Literal::Null => self.store.builtins.null,
        }
    }

    fn widen(&mut self, ty: TypeId) -> TypeId {
        let completion = self.force_type(ty, 0);
        let ty = match self.require_completion(completion) {
            Completion::Complete(ty) => ty,
            Completion::Deferred | Completion::Cycle | Completion::Limit => return ty,
        };
        self.store.widened_literal_type(ty)
    }

    fn is_string_like(&mut self, ty: TypeId) -> bool {
        self.complete_type(ty).is_some_and(|ty| {
            matches!(
                self.store.kind(ty),
                TypeKind::String | TypeKind::LiteralString(_, _)
            )
        })
    }

    fn report_deferred_cycle(&mut self, deferred: &DeferredType) {
        let DeferredType::Reference { declaration, .. } = deferred else {
            return;
        };
        let Some(model) = self.models.get(declaration).copied() else {
            return;
        };
        if let DeclarationModel::TypeAlias {
            declaration: alias, ..
        } = model
        {
            self.push_diagnostic(
                declaration.file,
                alias.name_span,
                format!("Type alias '{}' circularly references itself.", alias.name),
                2456,
            );
        }
    }

    fn report_complexity(&mut self, deferred: &DeferredType) {
        let span = match deferred {
            DeferredType::Reference { declaration, .. } | DeferredType::Value(declaration) => {
                self.models.get(declaration).map(|model| match model {
                    DeclarationModel::TypeAlias { declaration, .. } => declaration.name_span,
                    DeclarationModel::Interface { declaration, .. } => declaration.name_span,
                    DeclarationModel::Variable { declaration, .. } => declaration.name_span,
                    DeclarationModel::Parameter { parameter, .. } => parameter.name_span,
                    DeclarationModel::Function { declaration, .. } => declaration.name_span,
                    DeclarationModel::Class { declaration, .. } => declaration.name_span,
                })
            }
            DeferredType::Call { .. }
            | DeferredType::Construct { .. }
            | DeferredType::Property { .. }
            | DeferredType::Predicate { .. }
            | DeferredType::Logical { .. }
            | DeferredType::Unary { .. }
            | DeferredType::KeyOf(_)
            | DeferredType::Conditional { .. }
            | DeferredType::Mapped { .. }
            | DeferredType::IndexedAccess { .. }
            | DeferredType::BigIntLiteral
            | DeferredType::UniqueSymbol
            | DeferredType::GenericFunction
            | DeferredType::ObjectShape => None,
        };
        if let Some(span) = span {
            self.push_diagnostic(
                span.file,
                span,
                "Type instantiation is excessively deep and possibly infinite.".to_string(),
                2589,
            );
        }
    }

    fn push_diagnostic(&mut self, file: FileId, span: Span, message: String, code: u32) {
        self.push_diagnostic_with_identity(
            file,
            span,
            message.clone(),
            code,
            DiagnosticIdentity::DiagnosticText(message),
        );
    }

    fn push_diagnostic_with_identity(
        &mut self,
        file: FileId,
        span: Span,
        message: String,
        code: u32,
        identity: DiagnosticIdentity,
    ) {
        if !self.reported.insert((file, span.start, code, identity)) {
            return;
        }
        let source = &self.program.files[file.0 as usize].source;
        self.diagnostics
            .push(Diagnostic::at(source, span, message, code));
    }
}

fn known_truthiness(kind: &TypeKind) -> Option<bool> {
    match kind {
        TypeKind::Null | TypeKind::Undefined | TypeKind::LiteralBoolean(false, _) => Some(false),
        TypeKind::LiteralBoolean(true, _)
        | TypeKind::Array(_)
        | TypeKind::Tuple(_)
        | TypeKind::Object(_)
        | TypeKind::Function(_)
        | TypeKind::ShapeFunction(_) => Some(true),
        TypeKind::LiteralString(value, _) => Some(!value.is_empty()),
        TypeKind::LiteralNumber(value, _) => Some(value.is_truthy()),
        _ => None,
    }
}

fn authored_structural_union_member(node: &TypeNode) -> bool {
    match &node.kind {
        TypeNodeKind::Object(_) => true,
        TypeNodeKind::Array(element)
        | TypeNodeKind::Readonly(element)
        | TypeNodeKind::Parenthesized(element) => authored_structural_union_member(element),
        TypeNodeKind::Tuple(elements) => {
            !elements.is_empty() && elements.iter().all(authored_structural_union_member)
        }
        _ => false,
    }
}

fn is_declaration_source(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

impl RelationContext for Checker<'_> {
    fn force_type(&mut self, ty: TypeId, depth: usize) -> Completion<TypeId> {
        match self.store.kind(ty).clone() {
            TypeKind::Deferred(deferred) => self.force_deferred(ty, deferred, depth),
            _ => Completion::Complete(ty),
        }
    }

    fn type_kind(&self, ty: TypeId) -> TypeKind {
        self.store.kind(ty).clone()
    }

    fn strict_null_checks(&self) -> bool {
        self.options.strict
    }

    fn canonical_union(&mut self, members: &[TypeId]) -> TypeId {
        self.store
            .union(members.iter().copied(), UnionPolicy::Canonical)
    }
}
