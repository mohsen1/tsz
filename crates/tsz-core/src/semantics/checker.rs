use std::collections::{HashMap, HashSet};

use rustc_hash::FxHashMap;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::diagnostics::Diagnostic;
use crate::program::{CompilerOptions, Program};
use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{
    ArrowBody, BinaryOperator, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind,
    FunctionDeclaration, InterfaceDeclaration, KeywordType, Literal, Parameter, Statement,
    StatementKind, SwitchClauseKind, TypeAliasDeclaration, TypeNode, TypeNodeKind, UnaryOperator,
    VariableDeclaration, VariableKind,
};

use super::relation::{RelationContext, RelationFailureKind, RelationMode, relate};
use super::types::{
    Completion, DeferredLogicalOperator, DeferredType, DeferredUnaryOperator, ParameterType,
    Property, Signature, TypeId, TypeKind, TypeStore,
};

#[derive(Debug)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub type_count: usize,
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
}

#[derive(Debug, Clone, Copy)]
enum QueryState {
    Computing,
    Ready(TypeId),
    Failed(QueryFailure),
}

#[derive(Debug, Clone, Copy)]
enum QueryFailure {
    Cycle,
    Limit,
}

struct Checker<'a> {
    program: &'a Program,
    options: &'a CompilerOptions,
    store: TypeStore,
    models: FxHashMap<DeclId, DeclarationModel<'a>>,
    value_queries: FxHashMap<DeclId, QueryState>,
    force_queries: FxHashMap<TypeId, QueryState>,
    diagnostics: Vec<Diagnostic>,
    reported: HashSet<(FileId, u32, u32)>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a Program, options: &'a CompilerOptions) -> Self {
        let mut checker = Self {
            program,
            options,
            store: TypeStore::new(),
            models: FxHashMap::default(),
            value_queries: FxHashMap::default(),
            force_queries: FxHashMap::default(),
            diagnostics: Vec::new(),
            reported: HashSet::new(),
        };
        checker.collect_models();
        checker
    }

    fn check(mut self) -> CheckResult {
        for file in &self.program.files {
            for statement in &file.syntax.statements {
                self.check_statement(file.source.id, ScopeId(0), statement, None);
            }
        }
        CheckResult {
            diagnostics: self.diagnostics,
            type_count: self.store.len(),
        }
    }

    fn collect_models(&mut self) {
        for file in &self.program.files {
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
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::Function,
                    &declaration.name,
                ) {
                    self.models
                        .insert(id, DeclarationModel::Function { declaration, scope });
                }
                let function_scope = bound
                    .scope_for_node
                    .get(&statement.id)
                    .copied()
                    .unwrap_or(scope);
                for parameter in &declaration.parameters {
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
            | StatementKind::Class(_)
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

    fn check_statement(
        &mut self,
        file: FileId,
        scope: ScopeId,
        statement: &Statement,
        expected_return: Option<TypeId>,
    ) {
        match &statement.kind {
            StatementKind::Export(declaration) => {
                if let Some(expression) = &declaration.assignment {
                    self.infer_expression(file, scope, expression, None);
                }
            }
            StatementKind::Class(declaration) => self.check_class(file, declaration),
            StatementKind::Variable(declaration) => {
                self.check_variable(file, scope, statement.id, declaration);
            }
            StatementKind::Function(declaration) => {
                self.check_function(file, statement.id, declaration);
            }
            StatementKind::TypeAlias(declaration) => {
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::TypeAlias,
                    &declaration.name,
                ) {
                    let deferred = self
                        .store
                        .intern(TypeKind::Deferred(DeferredType::Reference {
                            declaration: id,
                            arguments: Vec::new(),
                        }));
                    let _ = self.force_type(deferred, 0);
                }
            }
            StatementKind::Interface(declaration) => {
                if let Some(id) = self.find_declaration(
                    file,
                    statement.id,
                    DeclarationKind::Interface,
                    &declaration.name,
                ) {
                    let deferred = self
                        .store
                        .intern(TypeKind::Deferred(DeferredType::Reference {
                            declaration: id,
                            arguments: Vec::new(),
                        }));
                    let _ = self.force_type(deferred, 0);
                }
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    let actual = self.infer_expression(file, scope, expression, expected_return);
                    if let Some(expected) = expected_return {
                        let return_span = Span {
                            file,
                            start: statement.span.start,
                            end: statement.span.start.saturating_add(6),
                        };
                        self.report_relation(
                            actual,
                            expected,
                            return_span,
                            Some(expression),
                            RelationMode::Assignment,
                            2322,
                            "Type",
                        );
                    }
                }
            }
            StatementKind::Block(statements) => {
                for nested in statements {
                    let nested_scope = self.program.files[file.0 as usize]
                        .bindings
                        .scope_for_node
                        .get(&nested.id)
                        .copied()
                        .unwrap_or(scope);
                    self.check_statement(file, nested_scope, nested, expected_return);
                }
            }
            StatementKind::If(control_flow) => {
                self.infer_expression(file, scope, &control_flow.condition, None);
                let then_scope = self.program.files[file.0 as usize]
                    .bindings
                    .scope_for_node
                    .get(&control_flow.then_statement.id)
                    .copied()
                    .unwrap_or(scope);
                self.check_statement(
                    file,
                    then_scope,
                    &control_flow.then_statement,
                    expected_return,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = self.program.files[file.0 as usize]
                        .bindings
                        .scope_for_node
                        .get(&else_statement.id)
                        .copied()
                        .unwrap_or(scope);
                    self.check_statement(file, else_scope, else_statement, expected_return);
                }
            }
            StatementKind::Switch(control_flow) => {
                let switch_scope = self.program.files[file.0 as usize]
                    .bindings
                    .scope_for_node
                    .get(&statement.id)
                    .copied()
                    .unwrap_or(scope);
                self.infer_expression(file, switch_scope, &control_flow.expression, None);
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.infer_expression(file, switch_scope, expression, None);
                    }
                    for nested in &clause.statements {
                        let nested_scope = self.program.files[file.0 as usize]
                            .bindings
                            .scope_for_node
                            .get(&nested.id)
                            .copied()
                            .unwrap_or(switch_scope);
                        self.check_statement(file, nested_scope, nested, expected_return);
                    }
                }
            }
            StatementKind::Expression(expression) => {
                self.infer_expression(file, scope, expression, None);
            }
            StatementKind::Import(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
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
        let annotation = declaration
            .annotation
            .as_ref()
            .map(|annotation| self.resolve_type_node(file, scope, annotation, &HashMap::new()));
        let initializer = declaration
            .initializer
            .as_ref()
            .map(|initializer| self.infer_expression(file, scope, initializer, annotation));
        if let (Some(source), Some(target), Some(initializer)) =
            (initializer, annotation, declaration.initializer.as_ref())
        {
            self.report_relation(
                source,
                target,
                declaration.name_span,
                Some(initializer),
                RelationMode::Assignment,
                2322,
                "Type",
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
            self.value_queries.insert(id, QueryState::Ready(value));
        }
    }

    fn check_function(&mut self, file: FileId, owner: NodeId, declaration: &FunctionDeclaration) {
        let Some(id) =
            self.find_declaration(file, owner, DeclarationKind::Function, &declaration.name)
        else {
            return;
        };
        let expected_return = match self.declaration_value_type(id, 0) {
            Completion::Complete(signature_type) => match self.force_type(signature_type, 0) {
                Completion::Complete(forced) => match self.store.kind(forced) {
                    TypeKind::Function(signature) => Some(signature.return_type),
                    _ => None,
                },
                Completion::Deferred | Completion::Cycle | Completion::Limit => None,
            },
            Completion::Deferred | Completion::Cycle | Completion::Limit => None,
        };
        let scope = self.program.files[file.0 as usize]
            .bindings
            .scope_for_node
            .get(&owner)
            .copied()
            .unwrap_or(ScopeId(0));
        for parameter in &declaration.parameters {
            if parameter.annotation.is_none()
                && (self.options.strict || self.options.no_implicit_any)
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
            self.check_statement(file, statement_scope, statement, expected_return);
        }
    }

    fn declaration_value_type(&mut self, id: DeclId, depth: usize) -> Completion<TypeId> {
        if self.program.standard_library_declaration(id).is_some() {
            return Completion::Deferred;
        }
        if depth > 100 {
            self.value_queries
                .insert(id, QueryState::Failed(QueryFailure::Limit));
            return Completion::Limit;
        }
        match self.value_queries.get(&id).copied() {
            Some(QueryState::Ready(ty)) => return Completion::Complete(ty),
            Some(QueryState::Computing | QueryState::Failed(QueryFailure::Cycle)) => {
                return Completion::Cycle;
            }
            Some(QueryState::Failed(QueryFailure::Limit)) => return Completion::Limit,
            None => {}
        }
        self.value_queries.insert(id, QueryState::Computing);
        let Some(model) = self.models.get(&id).copied() else {
            self.value_queries
                .insert(id, QueryState::Ready(self.store.builtins.error));
            return Completion::Complete(self.store.builtins.error);
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
                Completion::Complete(parameter.annotation.as_ref().map_or(
                    self.store.builtins.any,
                    |annotation| {
                        self.resolve_type_node(id.file, scope, annotation, &HashMap::new())
                    },
                ))
            }
            DeclarationModel::Function { declaration, scope } => {
                Completion::Complete(self.function_type(id, declaration, scope))
            }
            DeclarationModel::TypeAlias { .. } | DeclarationModel::Interface { .. } => {
                Completion::Complete(self.store.builtins.error)
            }
        };
        match result {
            Completion::Complete(ty) => {
                self.value_queries.insert(id, QueryState::Ready(ty));
            }
            Completion::Cycle => {
                self.value_queries
                    .insert(id, QueryState::Failed(QueryFailure::Cycle));
            }
            Completion::Limit => {
                self.value_queries
                    .insert(id, QueryState::Failed(QueryFailure::Limit));
            }
            Completion::Deferred => {}
        }
        result
    }

    fn function_type(
        &mut self,
        id: DeclId,
        declaration: &FunctionDeclaration,
        scope: ScopeId,
    ) -> TypeId {
        let mut type_parameters = HashMap::new();
        for (index, name) in declaration.type_parameters.iter().enumerate() {
            let ty = self.store.intern(TypeKind::TypeParameter {
                declaration: id,
                index: index as u32,
                name: name.clone(),
            });
            type_parameters.insert(name.clone(), ty);
        }
        let parameters = declaration
            .parameters
            .iter()
            .map(|parameter| ParameterType {
                name: parameter.name.clone(),
                ty: parameter
                    .annotation
                    .as_ref()
                    .map_or(self.store.builtins.any, |annotation| {
                        self.resolve_type_node(id.file, scope, annotation, &type_parameters)
                    }),
                optional: parameter.optional,
                rest: parameter.rest,
            })
            .collect();
        let return_type = declaration
            .return_type
            .as_ref()
            .map_or(self.store.builtins.any, |return_type| {
                self.resolve_type_node(id.file, scope, return_type, &type_parameters)
            });
        self.store.intern(TypeKind::Function(Signature {
            parameters,
            return_type,
        }))
    }

    fn resolve_type_node(
        &mut self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        type_parameters: &HashMap<String, TypeId>,
    ) -> TypeId {
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
            },
            TypeNodeKind::Literal(literal) => self.literal_type(literal),
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
                let members = members
                    .iter()
                    .map(|member| self.resolve_type_node(file, scope, member, type_parameters))
                    .collect::<Vec<_>>();
                self.store.union(members)
            }
            TypeNodeKind::Intersection(members) => {
                let members = members
                    .iter()
                    .map(|member| self.resolve_type_node(file, scope, member, type_parameters))
                    .collect::<Vec<_>>();
                self.store.intersection(members)
            }
            TypeNodeKind::Object(properties) => {
                let properties = properties
                    .iter()
                    .map(|property| Property {
                        name: property.name.clone(),
                        ty: self.resolve_type_node(file, scope, &property.ty, type_parameters),
                        optional: property.optional,
                        readonly: property.readonly,
                    })
                    .collect();
                self.store.object(properties)
            }
            TypeNodeKind::Function {
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| ParameterType {
                        name: parameter.name.clone(),
                        ty: parameter.annotation.as_ref().map_or(
                            self.store.builtins.any,
                            |annotation| {
                                self.resolve_type_node(file, scope, annotation, type_parameters)
                            },
                        ),
                        optional: parameter.optional,
                        rest: parameter.rest,
                    })
                    .collect();
                let return_type = self.resolve_type_node(file, scope, return_type, type_parameters);
                self.store.intern(TypeKind::Function(Signature {
                    parameters,
                    return_type,
                }))
            }
            TypeNodeKind::Constructor {
                parameters,
                return_type,
                ..
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| ParameterType {
                        name: parameter.name.clone(),
                        ty: parameter.annotation.as_ref().map_or(
                            self.store.builtins.any,
                            |annotation| {
                                self.resolve_type_node(file, scope, annotation, type_parameters)
                            },
                        ),
                        optional: parameter.optional,
                        rest: parameter.rest,
                    })
                    .collect();
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
                    return *ty;
                }
                let arguments = arguments
                    .iter()
                    .map(|argument| self.resolve_type_node(file, scope, argument, type_parameters))
                    .collect::<Vec<_>>();
                if name == "Array" && arguments.len() == 1 {
                    return self.store.intern(TypeKind::Array(arguments[0]));
                }
                let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Type) else {
                    self.push_diagnostic(
                        file,
                        *name_span,
                        format!("Cannot find name '{name}'."),
                        2304,
                    );
                    return self.store.builtins.error;
                };
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Reference {
                        declaration,
                        arguments,
                    }))
            }
            TypeNodeKind::TypeQuery { name, name_span } => {
                let root_name = name.split('.').next().unwrap_or(name);
                let Some(declaration) = self.resolve_name(file, scope, root_name, Meaning::Value)
                else {
                    self.push_diagnostic(
                        file,
                        *name_span,
                        format!("Cannot find name '{root_name}'."),
                        2304,
                    );
                    return self.store.builtins.error;
                };
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Value(declaration)))
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
            TypeNodeKind::Predicate { .. } => self.store.builtins.boolean,
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
                let true_type = self.resolve_type_node(file, scope, true_type, type_parameters);
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
                let object = self.resolve_type_node(file, scope, object, type_parameters);
                let index = self.resolve_type_node(file, scope, index, type_parameters);
                self.store
                    .intern(TypeKind::Deferred(DeferredType::IndexedAccess {
                        object,
                        index,
                    }))
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
        match &expression.kind {
            ExpressionKind::Identifier { name, name_span } => {
                if name == "undefined" {
                    return self.store.builtins.undefined;
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
                self.store
                    .intern(TypeKind::Deferred(DeferredType::Value(declaration)))
            }
            ExpressionKind::Literal(literal) => self.literal_type(literal),
            ExpressionKind::Object(properties) => {
                let expected_properties = expected
                    .and_then(|expected| self.complete_type(expected))
                    .and_then(|expected| match self.store.kind(expected) {
                        TypeKind::Object(properties) => Some(properties.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let properties = properties
                    .iter()
                    .map(|property| {
                        let expected = expected_properties
                            .iter()
                            .find(|candidate| candidate.name == property.name)
                            .map(|candidate| candidate.ty);
                        Property {
                            name: property.name.clone(),
                            ty: self.infer_expression(file, scope, &property.value, expected),
                            optional: false,
                            readonly: false,
                        }
                    })
                    .collect();
                self.store.object(properties)
            }
            ExpressionKind::Array(elements) => {
                let expected_element = expected
                    .and_then(|expected| self.complete_type(expected))
                    .and_then(|expected| match self.store.kind(expected) {
                        TypeKind::Array(element) => Some(*element),
                        _ => None,
                    });
                let elements = elements
                    .iter()
                    .map(|element| self.infer_expression(file, scope, element, expected_element))
                    .collect::<Vec<_>>();
                let element = self.store.union(elements);
                self.store.intern(TypeKind::Array(element))
            }
            ExpressionKind::Call { callee, arguments } => {
                let callee_type = self.infer_expression(file, scope, callee, None);
                let callee_type = match self.force_type(callee_type, 0) {
                    Completion::Complete(callee_type) => callee_type,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        for argument in arguments {
                            let _ = self.infer_expression(file, scope, argument, None);
                        }
                        return self
                            .store
                            .intern(TypeKind::Deferred(DeferredType::Call(callee_type)));
                    }
                };
                let TypeKind::Function(signature) = self.store.kind(callee_type).clone() else {
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
                if arguments.len() < required
                    || maximum.is_some_and(|maximum| arguments.len() > maximum)
                {
                    let expected_count = if maximum.is_none() {
                        format!("at least {required}")
                    } else if maximum == Some(required) {
                        required.to_string()
                    } else {
                        format!("{}-{}", required, maximum.unwrap_or(required))
                    };
                    self.push_diagnostic(
                        file,
                        expression.span,
                        format!(
                            "Expected {expected_count} arguments, but got {}.",
                            arguments.len()
                        ),
                        2554,
                    );
                }
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
                    self.report_relation(
                        actual,
                        expected,
                        argument.span,
                        Some(argument),
                        RelationMode::Assignment,
                        2345,
                        "Argument of type",
                    );
                }
                signature.return_type
            }
            ExpressionKind::New { callee, arguments } => {
                let _ = self.infer_expression(file, scope, callee, None);
                for argument in arguments {
                    let _ = self.infer_expression(file, scope, argument, None);
                }
                self.store.builtins.any
            }
            ExpressionKind::Member {
                object,
                name,
                name_span,
            } => {
                let object_type = self.infer_expression(file, scope, object, None);
                match self.property_type(object_type, name) {
                    Completion::Complete(Some(ty)) => ty,
                    Completion::Complete(None) => {
                        let object_name = self.type_name(object_type);
                        self.push_diagnostic(
                            file,
                            *name_span,
                            format!("Property '{name}' does not exist on type '{object_name}'."),
                            2339,
                        );
                        self.store.builtins.error
                    }
                    Completion::Deferred | Completion::Cycle | Completion::Limit => self
                        .store
                        .intern(TypeKind::Deferred(DeferredType::Property {
                            object: object_type,
                            name: name.clone(),
                        })),
                }
            }
            ExpressionKind::Arrow {
                parameters,
                return_type: annotation,
                body,
            } => {
                let expected_signature = expected
                    .and_then(|expected| self.complete_type(expected))
                    .and_then(|expected| match self.store.kind(expected) {
                        TypeKind::Function(signature) => Some(signature.clone()),
                        _ => None,
                    });
                let parameter_types = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| {
                        let ty = if let Some(annotation) = &parameter.annotation {
                            self.resolve_type_node(file, scope, annotation, &HashMap::new())
                        } else {
                            expected_signature
                                .as_ref()
                                .and_then(|signature| signature.parameters.get(index))
                                .map_or(self.store.builtins.any, |parameter| parameter.ty)
                        };
                        if parameter.annotation.is_none()
                            && expected_signature.is_none()
                            && (self.options.strict || self.options.no_implicit_any)
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
                        ParameterType {
                            name: parameter.name.clone(),
                            ty,
                            optional: parameter.optional,
                            rest: parameter.rest,
                        }
                    })
                    .collect::<Vec<_>>();
                let expected_return = annotation
                    .as_ref()
                    .map(|annotation| {
                        self.resolve_type_node(file, scope, annotation, &HashMap::new())
                    })
                    .or_else(|| {
                        expected_signature
                            .as_ref()
                            .map(|signature| signature.return_type)
                    });
                let return_type = match body {
                    ArrowBody::Expression(body) => {
                        self.infer_expression(file, scope, body, expected_return)
                    }
                    ArrowBody::Block(statements) => {
                        for statement in statements {
                            self.check_statement(file, scope, statement, expected_return);
                        }
                        expected_return.unwrap_or(self.store.builtins.void)
                    }
                };
                self.store.intern(TypeKind::Function(Signature {
                    parameters: parameter_types,
                    return_type,
                }))
            }
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
                    RelationMode::Assignment,
                    2322,
                    "Type",
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
            Some(QueryState::Failed(QueryFailure::Cycle)) => return Completion::Cycle,
            Some(QueryState::Failed(QueryFailure::Limit)) => return Completion::Limit,
            None => {}
        }
        if depth > 100 {
            self.force_queries
                .insert(ty, QueryState::Failed(QueryFailure::Limit));
            self.report_complexity(&deferred);
            return Completion::Limit;
        }
        self.force_queries.insert(ty, QueryState::Computing);
        let result = match deferred.clone() {
            DeferredType::Reference {
                declaration,
                arguments,
            } => self.evaluate_reference(declaration, &arguments),
            DeferredType::Value(declaration) => self.declaration_value_type(declaration, depth + 1),
            DeferredType::Call(callee) => self.evaluate_call(callee, depth + 1),
            DeferredType::Property { object, name } => {
                self.evaluate_property(object, &name, depth + 1)
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
            DeferredType::Conditional { .. } | DeferredType::Mapped { .. } => Completion::Deferred,
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
            Completion::Complete(result) => {
                self.force_queries.insert(ty, QueryState::Ready(result));
            }
            Completion::Cycle => {
                self.force_queries
                    .insert(ty, QueryState::Failed(QueryFailure::Cycle));
                self.report_deferred_cycle(&deferred);
            }
            Completion::Limit => {
                self.force_queries
                    .insert(ty, QueryState::Failed(QueryFailure::Limit));
                self.report_complexity(&deferred);
            }
            Completion::Deferred => {
                self.force_queries.remove(&ty);
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
            return Completion::Deferred;
        }
        let Some(model) = self.models.get(&declaration).copied() else {
            return Completion::Complete(self.store.builtins.error);
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
                let properties = interface
                    .properties
                    .iter()
                    .map(|property| Property {
                        name: property.name.clone(),
                        ty: self.resolve_type_node(
                            declaration.file,
                            scope,
                            &property.ty,
                            &parameters,
                        ),
                        optional: property.optional,
                        readonly: property.readonly,
                    })
                    .collect();
                Completion::Complete(self.store.object(properties))
            }
            DeclarationModel::Variable { .. }
            | DeclarationModel::Parameter { .. }
            | DeclarationModel::Function { .. } => Completion::Complete(self.store.builtins.error),
        }
    }

    fn substitution(
        &mut self,
        declaration: DeclId,
        names: &[String],
        arguments: &[TypeId],
    ) -> HashMap<String, TypeId> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let ty = arguments.get(index).copied().unwrap_or_else(|| {
                    self.store.intern(TypeKind::TypeParameter {
                        declaration,
                        index: index as u32,
                        name: name.clone(),
                    })
                });
                (name.clone(), ty)
            })
            .collect()
    }

    fn evaluate_keyof(&mut self, operand: TypeId, depth: usize) -> Completion<TypeId> {
        let operand = match self.force_type(operand, depth) {
            Completion::Complete(operand) => operand,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let TypeKind::Object(properties) = self.store.kind(operand).clone() else {
            return Completion::Complete(self.store.builtins.never);
        };
        let keys = properties
            .into_iter()
            .map(|property| self.store.intern(TypeKind::LiteralString(property.name)))
            .collect::<Vec<_>>();
        Completion::Complete(self.store.union(keys))
    }

    fn evaluate_call(&mut self, callee: TypeId, depth: usize) -> Completion<TypeId> {
        let callee = match self.force_type(callee, depth) {
            Completion::Complete(callee) => callee,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        match self.store.kind(callee) {
            TypeKind::Function(signature) => Completion::Complete(signature.return_type),
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
            _ => Completion::Deferred,
        }
    }

    fn evaluate_property(
        &mut self,
        object: TypeId,
        name: &str,
        depth: usize,
    ) -> Completion<TypeId> {
        let object = match self.force_type(object, depth) {
            Completion::Complete(object) => object,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        match self.store.kind(object) {
            TypeKind::Object(properties) => properties
                .iter()
                .find(|property| property.name == name)
                .map_or(Completion::Deferred, |property| {
                    Completion::Complete(property.ty)
                }),
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
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
                        | TypeKind::LiteralBoolean(_)
                        | TypeKind::LiteralNumber(_)
                        | TypeKind::LiteralString(_)
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
            TypeKind::Number | TypeKind::LiteralNumber(_) | TypeKind::Any => {
                Completion::Complete(self.store.builtins.number)
            }
            TypeKind::BigInt if operator != DeferredUnaryOperator::Plus => {
                Completion::Complete(self.store.builtins.bigint)
            }
            _ => Completion::Deferred,
        }
    }

    fn evaluate_indexed_access(
        &mut self,
        object: TypeId,
        index: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let object = match self.force_type(object, depth) {
            Completion::Complete(object) => object,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let index = match self.force_type(index, depth) {
            Completion::Complete(index) => index,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let TypeKind::Object(properties) = self.store.kind(object).clone() else {
            return Completion::Complete(self.store.builtins.error);
        };
        let keys = match self.store.kind(index).clone() {
            TypeKind::LiteralString(key) => vec![key],
            TypeKind::Union(members) => members
                .into_iter()
                .filter_map(|member| match self.store.kind(member) {
                    TypeKind::LiteralString(key) => Some(key.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let values = keys
            .iter()
            .filter_map(|key| {
                properties
                    .iter()
                    .find(|property| &property.name == key)
                    .map(|property| property.ty)
            })
            .collect::<Vec<_>>();
        Completion::Complete(self.store.union(values))
    }

    fn complete_type(&mut self, ty: TypeId) -> Option<TypeId> {
        match self.force_type(ty, 0) {
            Completion::Complete(ty) => Some(ty),
            Completion::Deferred | Completion::Cycle | Completion::Limit => None,
        }
    }

    fn property_type(&mut self, object: TypeId, name: &str) -> Completion<Option<TypeId>> {
        let object = match self.force_type(object, 0) {
            Completion::Complete(object) => object,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        match self.store.kind(object) {
            TypeKind::Object(properties) => Completion::Complete(
                properties
                    .iter()
                    .find(|property| property.name == name)
                    .map(|property| property.ty),
            ),
            TypeKind::Any => Completion::Complete(Some(self.store.builtins.any)),
            _ => Completion::Complete(None),
        }
    }

    fn report_relation(
        &mut self,
        source: TypeId,
        target: TypeId,
        span: Span,
        source_expression: Option<&Expression>,
        mode: RelationMode,
        code: u32,
        prefix: &str,
    ) {
        if let Err(failure) = relate(self, source, target, mode) {
            if failure.kind == RelationFailureKind::ComplexityLimit {
                self.push_diagnostic(
                    span.file,
                    span,
                    "Type instantiation is excessively deep and possibly infinite.".to_string(),
                    2589,
                );
                return;
            }
            let diagnostic_span = match (&failure.kind, source_expression) {
                (RelationFailureKind::Property(name), Some(expression)) => {
                    object_property_span(expression, name).unwrap_or(span)
                }
                _ => span,
            };
            let source_name = self.relation_source_name(failure.source, failure.target);
            let target_name = self.type_name(failure.target);
            let message = if prefix == "Argument of type" {
                format!(
                    "Argument of type '{source_name}' is not assignable to parameter of type '{target_name}'."
                )
            } else {
                format!("Type '{source_name}' is not assignable to type '{target_name}'.")
            };
            self.push_diagnostic(diagnostic_span.file, diagnostic_span, message, code);
        }
    }

    fn relation_source_name(&mut self, source: TypeId, target: TypeId) -> String {
        let source = self.complete_type(source).unwrap_or(source);
        let target = self.complete_type(target).unwrap_or(target);
        let preserve_literal = matches!(
            (self.store.kind(source), self.store.kind(target)),
            (TypeKind::LiteralString(_), TypeKind::LiteralString(_))
                | (TypeKind::LiteralNumber(_), TypeKind::LiteralNumber(_))
                | (TypeKind::LiteralBoolean(_), TypeKind::LiteralBoolean(_))
        );
        let display = if preserve_literal {
            source
        } else {
            self.widen(source)
        };
        self.store.display(display)
    }

    fn type_name(&mut self, ty: TypeId) -> String {
        let complete = self.complete_type(ty).unwrap_or(ty);
        self.store.display(complete)
    }

    fn literal_type(&mut self, literal: &Literal) -> TypeId {
        match literal {
            Literal::String(value) => self.store.intern(TypeKind::LiteralString(value.clone())),
            Literal::Number(value) => self.store.intern(TypeKind::LiteralNumber(value.clone())),
            Literal::Boolean(value) => self.store.intern(TypeKind::LiteralBoolean(*value)),
            Literal::Null => self.store.builtins.null,
        }
    }

    fn widen(&mut self, ty: TypeId) -> TypeId {
        match self.store.kind(ty).clone() {
            TypeKind::LiteralString(_) => self.store.builtins.string,
            TypeKind::LiteralNumber(_) => self.store.builtins.number,
            TypeKind::LiteralBoolean(_) => self.store.builtins.boolean,
            TypeKind::Array(element) => {
                let element = self.widen(element);
                self.store.intern(TypeKind::Array(element))
            }
            _ => ty,
        }
    }

    fn is_string_like(&mut self, ty: TypeId) -> bool {
        self.complete_type(ty).is_some_and(|ty| {
            matches!(
                self.store.kind(ty),
                TypeKind::String | TypeKind::LiteralString(_)
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
                })
            }
            DeferredType::Call(_)
            | DeferredType::Property { .. }
            | DeferredType::Logical { .. }
            | DeferredType::Unary { .. }
            | DeferredType::KeyOf(_)
            | DeferredType::Conditional { .. }
            | DeferredType::Mapped { .. }
            | DeferredType::IndexedAccess { .. } => None,
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
        if !self.reported.insert((file, span.start, code)) {
            return;
        }
        let source = &self.program.files[file.0 as usize].source;
        self.diagnostics
            .push(Diagnostic::at(source, span, message, code));
    }
}

fn known_truthiness(kind: &TypeKind) -> Option<bool> {
    match kind {
        TypeKind::Null | TypeKind::Undefined | TypeKind::LiteralBoolean(false) => Some(false),
        TypeKind::LiteralBoolean(true)
        | TypeKind::Array(_)
        | TypeKind::Tuple(_)
        | TypeKind::Object(_)
        | TypeKind::Function(_) => Some(true),
        TypeKind::LiteralString(value) => Some(!value.is_empty()),
        TypeKind::LiteralNumber(value) => Some(value != "0" && value != "-0" && value != "NaN"),
        _ => None,
    }
}

fn object_property_span(expression: &Expression, name: &str) -> Option<Span> {
    let ExpressionKind::Object(properties) = &expression.kind else {
        return None;
    };
    properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| property.name_span)
}

fn is_declaration_source(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

impl RelationContext for Checker<'_> {
    fn force_type(&mut self, ty: TypeId, depth: usize) -> Completion<TypeId> {
        if depth > 100 {
            return Completion::Limit;
        }
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
}
