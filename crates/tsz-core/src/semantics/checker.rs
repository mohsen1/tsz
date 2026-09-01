use std::collections::{HashMap, HashSet};

use rustc_hash::FxHashMap;
mod binary_expression;
mod cache_model;
mod call_model;
mod capabilities;
mod class_model;
mod class_property_initialization;
mod contextual_grammar;
mod declaration_value;
mod element_access;
mod entry;
mod flow_reference;
mod function_model;
mod generic_call_instantiation;
mod import_alias;
mod model_collection;
mod object_shape;
mod primary_reference;
mod projection_model;
pub(super) mod recursion;
mod reference_instantiation;
mod regular_expression;
mod relation_diagnostic;
mod required_type;
mod statement_model;
mod string_literal;
mod type_member_grammar;
mod unary_expression;

pub use entry::{CheckResult, check_program, summarize_program};
use model_collection::DeclarationModel;
use object_shape::authored_structural_union_member;
use relation_diagnostic::ContextualType;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::diagnostics::Diagnostic;
use crate::program::{CapabilityAnalysis, CompilerOptions, Program};
use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{
    AssignmentOperator, Expression, ExpressionKind, FunctionDeclaration, TypeNode, TypeNodeKind,
    UnaryOperator,
};

use super::relation::RelationContext;
use super::types::{
    Completion, DeferredType, DeferredUnaryOperator, ElementAccessMode, IndexKeyKind,
    IndexSignature, LiteralProvenance, ObjectShape, Property, TypeId, TypeKind, TypeStore,
    UnionPolicy,
};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedAccessOrigin {
    query: TypeId,
    span: Span,
    receiver_display: Option<projection_model::ObjectDisplayOrigin>,
    receiver_alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConstructOrigin {
    query: TypeId,
    argument_span: Span,
}

struct Checker<'a> {
    program: &'a Program,
    options: &'a CompilerOptions,
    capabilities: &'a CapabilityAnalysis,
    store: TypeStore,
    models: FxHashMap<DeclId, DeclarationModel<'a>>,
    parameter_type_overrides: FxHashMap<DeclId, TypeId>,
    forbidden_default_type_parameters: Vec<HashSet<TypeId>>,
    value_queries: FxHashMap<DeclId, declaration_value::ValueQueryState>,
    force_queries: FxHashMap<TypeId, QueryState>,
    force_reference_stack: recursion::ReferenceExpansionStack,
    // Session provenance, never semantic interning or query identity.
    required_type_contexts: FxHashMap<Span, HashMap<String, TypeId>>,
    complete_required_type_nodes: HashSet<Span>,
    property_query_origins: Vec<PropertyQueryOrigin>,
    indexed_access_origins: Vec<IndexedAccessOrigin>,
    construct_origins: Vec<ConstructOrigin>,
    // Per-use diagnostic provenance for the immediately following relation.
    expression_type_origins: FxHashMap<(FileId, NodeId), TypeId>,
    diagnostics: Vec<Diagnostic>,
    completion: capabilities::CompletionTracker,
}

impl<'a> Checker<'a> {
    fn new(
        program: &'a Program,
        options: &'a CompilerOptions,
        capabilities: &'a CapabilityAnalysis,
    ) -> Self {
        let mut checker = Self {
            program,
            options,
            capabilities,
            store: TypeStore::new(),
            models: FxHashMap::default(),
            parameter_type_overrides: FxHashMap::default(),
            forbidden_default_type_parameters: Vec::new(),
            value_queries: FxHashMap::default(),
            force_queries: FxHashMap::default(),
            force_reference_stack: recursion::ReferenceExpansionStack::new(
                recursion::ReferenceDemand::ShapeSupport,
            ),
            required_type_contexts: FxHashMap::default(),
            complete_required_type_nodes: HashSet::new(),
            property_query_origins: Vec::new(),
            indexed_access_origins: Vec::new(),
            construct_origins: Vec::new(),
            expression_type_origins: FxHashMap::default(),
            diagnostics: Vec::new(),
            completion: capabilities::CompletionTracker::new(program.files.len()),
        };
        checker.collect_models();
        checker
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

    fn check_function(&mut self, file: FileId, owner: NodeId, declaration: &FunctionDeclaration) {
        let Some(id) =
            self.find_declaration(file, owner, DeclarationKind::Function, &declaration.name)
        else {
            return;
        };
        let expected_return = self.require_function_signature(id);
        let scope = self.node_scope(file, owner, ScopeId(0));
        for parameter in &declaration.parameters {
            if declaration.overload_context_is_recovery_free()
                && parameter.implementation_name_is_recovery_free()
                && parameter.annotation.is_none()
                && parameter.initializer.is_none()
                && self.options.effective_no_implicit_any()
            {
                self.push_diagnostic(
                    file,
                    Self::implicit_any_parameter_span(parameter),
                    format!(
                        "Parameter '{}' implicitly has an 'any' type.",
                        parameter.name
                    ),
                    7006,
                );
            }
        }
        self.check_statement_list(
            file,
            scope,
            &declaration.body,
            expected_return,
            statement_model::ROOT_JUMP_TARGETS,
        );
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
            TypeNodeKind::Keyword(keyword) => self
                .store
                .builtins
                .keyword(*keyword)
                .unwrap_or_else(|| self.store.deferred_unique_symbol()),
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
                ..
            }
            | TypeNodeKind::Constructor {
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
                let resolved_parameters = match self.anonymous_signature_parameters(
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
                self.store
                    .function(None, false, resolved_parameters, return_type)
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
                let Some(declaration) =
                    self.resolve_semantic_name(file, scope, name, Meaning::Type)
                else {
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
                if self.program.standard_library.is_map_type(declaration) && arguments.len() == 2 {
                    return self.library_reference(declaration, arguments);
                }
                self.store.symbolic_reference(declaration, arguments)
            }
            TypeNodeKind::This => self.resolve_this_type(file, node.span),
            TypeNodeKind::TypeQuery {
                name,
                name_span,
                segment_spans,
            } => self.resolve_type_query_node(file, scope, name, *name_span, segment_spans),
            TypeNodeKind::Infer {
                name,
                name_span,
                constraint,
            } => {
                if let Some(ty) = type_parameters.get(name) {
                    return *ty;
                }
                if let Some(constraint) = constraint {
                    let _ = self.resolve_type_node(file, scope, constraint, type_parameters);
                }
                self.store.type_parameter(
                    DeclId {
                        file,
                        local: name_span.start | (1 << 31),
                    },
                    0,
                    name,
                )
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
                let parameter_type = self.store.type_parameter(
                    DeclId {
                        file,
                        local: parameter_span.start | (1 << 31),
                    },
                    0,
                    parameter,
                );
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
                let receiver_display = projection_model::direct_object_display_origin(object);
                let receiver_alias = projection_model::authored_type_reference_name(object);
                let object = self.resolve_type_node(file, scope, object, type_parameters);
                let index = self.resolve_type_node(file, scope, index, type_parameters);
                self.deferred_indexed_access_type(
                    object,
                    index,
                    index_span,
                    receiver_display,
                    receiver_alias,
                )
            }
            TypeNodeKind::Missing => self.store.builtins.error,
        }
    }

    pub(super) fn library_reference(
        &mut self,
        declaration: DeclId,
        arguments: Vec<TypeId>,
    ) -> TypeId {
        let library_declaration = self
            .program
            .standard_library_declaration(declaration)
            .expect("a canonical reference needs its program-owned library declaration");
        self.store.intern(TypeKind::LibraryReference {
            declaration,
            name: library_declaration.name.clone(),
            arguments,
        })
    }

    fn infer_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        expected: Option<TypeId>,
    ) -> TypeId {
        self.infer_expression_contextual(
            file,
            scope,
            expression,
            ContextualType::from_option(expected),
        )
    }

    fn infer_expression_contextual(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        expected: ContextualType,
    ) -> TypeId {
        let inferred = self.infer_expression_inner(file, scope, expression, expected);
        self.expression_type_origins
            .insert((file, expression.id), inferred);
        inferred
    }

    fn infer_expression_inner(
        &mut self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        expected: ContextualType,
    ) -> TypeId {
        match &expression.kind {
            ExpressionKind::Identifier { .. } => self.infer_identifier(file, scope, expression),
            ExpressionKind::This => self.infer_this_expression(file, scope, expression.id),
            ExpressionKind::Literal(literal) => {
                self.literal_type(literal, LiteralProvenance::Fresh)
            }
            ExpressionKind::Template(template) => self.infer_template(file, scope, template),
            ExpressionKind::RegularExpression(literal) => {
                self.infer_regular_expression(file, literal)
            }
            ExpressionKind::Object(properties) => {
                let properties = properties
                    .iter()
                    .map(|property| {
                        self.report_object_literal_shorthand_default(file, property);
                        let expected = match expected {
                            ContextualType::Known(expected) => self
                                .contextual_object_property_type(
                                    expected,
                                    &property.name,
                                    Some(expression),
                                ),
                            other => other,
                        };
                        let inferred = self.infer_expression_contextual(
                            file,
                            scope,
                            &property.value,
                            expected,
                        );
                        let inferred = if expected.is_known() {
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
                let expected_element = match expected {
                    ContextualType::Known(expected) => self.contextual_array_element_type(expected),
                    other => other,
                };
                self.infer_array_expression(file, scope, elements, expected_element)
            }
            ExpressionKind::Call {
                callee,
                type_arguments,
                arguments,
            } => {
                self.infer_call_expression(file, scope, callee, type_arguments.is_some(), arguments)
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
                ..
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
                let mut argument_types = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    argument_types.push(self.infer_expression_contextual(
                        file,
                        scope,
                        argument,
                        ContextualType::Deferred,
                    ));
                }
                let query = self.deferred_construct_type(
                    callee_type,
                    type_arguments,
                    argument_types,
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
            } => self.infer_member_expression(file, scope, object, name, *name_span, None),
            ExpressionKind::ElementAccess { object, index } => self
                .infer_element_access_expression(
                    file,
                    scope,
                    object,
                    index,
                    ElementAccessMode::Read,
                ),
            ExpressionKind::FunctionLike(function) => {
                self.infer_function_like_expression(file, scope, expression, function, expected)
            }
            ExpressionKind::Binary { .. } => {
                self.infer_authored_binary_expression(file, scope, expression, expected)
            }
            ExpressionKind::Conditional { .. } => {
                self.infer_conditional_expression(file, scope, expression, expected)
            }
            ExpressionKind::Missing => self.store.builtins.error,
            ExpressionKind::Unary { operator, operand } => {
                let operand = self.infer_expression(file, scope, operand, None);
                match operator {
                    UnaryOperator::Not => self.store.builtins.boolean,
                    UnaryOperator::Delete => {
                        self.observe_delete_operand(operand);
                        self.store.builtins.boolean
                    }
                    UnaryOperator::TypeOf => self.store.builtins.string,
                    UnaryOperator::Void => self.store.builtins.undefined,
                    UnaryOperator::Await => self
                        .store
                        .deferred_unary(DeferredUnaryOperator::Await, operand),
                    UnaryOperator::Plus | UnaryOperator::Minus | UnaryOperator::BitwiseNot => {
                        self.store.deferred_unary(
                            match operator {
                                UnaryOperator::Plus => DeferredUnaryOperator::Plus,
                                UnaryOperator::Minus => DeferredUnaryOperator::Minus,
                                UnaryOperator::BitwiseNot => DeferredUnaryOperator::BitwiseNot,
                                _ => unreachable!(),
                            },
                            operand,
                        )
                    }
                }
            }
            ExpressionKind::Assignment {
                left,
                operator,
                operator_span,
                right,
                ..
            } => match operator {
                AssignmentOperator::Assign => self.infer_assignment(file, scope, left, right),
                AssignmentOperator::AddAssign => self.infer_compound_add_assignment(
                    file,
                    scope,
                    expression.span,
                    *operator_span,
                    left,
                    right,
                ),
            },
            ExpressionKind::As { expression, ty } => {
                self.infer_expression(file, scope, expression, None);
                self.resolve_type_node(file, scope, ty, &HashMap::new())
            }
            ExpressionKind::NonNull(inner) => {
                let inferred = self.infer_expression_contextual(file, scope, inner, expected);
                if self.options.effective_strict_null_checks() {
                    self.store
                        .deferred_unary(DeferredUnaryOperator::NonNull, inferred)
                } else {
                    inferred
                }
            }
            ExpressionKind::Parenthesized(inner) => {
                self.infer_expression_contextual(file, scope, inner, expected)
            }
        }
    }
    fn force_deferred(
        &mut self,
        ty: TypeId,
        deferred: DeferredType,
        depth: usize,
    ) -> Completion<TypeId> {
        match self.force_queries.get(&ty).copied() {
            Some(QueryState::Ready(result)) => return Completion::Complete(result),
            Some(QueryState::Computing) => return Completion::Cycle,
            None => {}
        }
        // Sole evaluator-expansion budget; adapters propagate its completion.
        if depth > 100 {
            self.report_complexity(&deferred);
            return Completion::Limit;
        }
        self.completion.begin_capture();
        let reference_instantiation = match &deferred {
            DeferredType::Reference {
                declaration,
                arguments,
            } => Some(self.reference_instantiation(*declaration, arguments)),
            _ => None,
        };
        let result_is_query_local = matches!(
            &reference_instantiation,
            Some(Completion::Complete(instantiation)) if instantiation.is_query_local()
        ) || self.deferred_result_is_query_local(&deferred);
        let reference_checkpoint = self.force_reference_stack.checkpoint();
        if let DeferredType::Reference {
            declaration,
            arguments,
        } = &deferred
        {
            self.force_reference_stack.push(ty, *declaration, arguments);
        }
        self.force_queries.insert(ty, QueryState::Computing);
        let result = match deferred.clone() {
            DeferredType::Reference {
                declaration,
                arguments,
            } => self.evaluate_reference_instantiation(
                declaration,
                &arguments,
                reference_instantiation.unwrap_or(Completion::Deferred),
                depth + 1,
            ),
            DeferredType::Value(declaration) => self.declaration_value_type(declaration),
            DeferredType::ImportedTypeQuery(declaration) => {
                self.imported_type_query_value(declaration)
            }
            query @ DeferredType::FlowReference { .. } => self.force_flow(query, depth + 1),
            DeferredType::Call {
                callee,
                argument_count,
            } => self.evaluate_call(callee, argument_count, depth + 1),
            DeferredType::Construct {
                callee,
                type_arguments,
                arguments,
            } => self.evaluate_construct(callee, &type_arguments, &arguments, depth + 1),
            DeferredType::Property { object, name } => {
                self.evaluate_property(ty, object, &name, depth + 1)
            }
            DeferredType::ElementAccess {
                object,
                index,
                mode,
            } => self.evaluate_element_access(object, index, mode, depth + 1),
            DeferredType::Binary {
                operator,
                left,
                right,
            } => self.force_binary(operator, left, right, depth + 1),
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
            | DeferredType::LexicalThis { .. }
            | DeferredType::GenericCall
            | DeferredType::BigIntLiteral
            | DeferredType::NumericRecovery
            | DeferredType::Utf16StringLiteral
            | DeferredType::TemplateValue
            | DeferredType::UniqueSymbol
            | DeferredType::GenericFunction
            | DeferredType::ObjectShape => Completion::Deferred,
            DeferredType::IndexedAccess { object, index } => {
                self.evaluate_indexed_access(object, index, depth + 1)
            }
        };
        let result = match result {
            Completion::Complete(result)
                if matches!(self.store.kind(result), TypeKind::Deferred(_))
                    && self.productive_alias_edge_is_provisional(&deferred, result) =>
            {
                Completion::Deferred
            }
            Completion::Complete(result)
                if matches!(self.store.kind(result), TypeKind::Deferred(_)) =>
            {
                self.force_type(result, depth + 1)
            }
            other => other,
        };
        let captured = self.completion.finish_capture();
        self.force_reference_stack.restore(reference_checkpoint);
        match result {
            Completion::Complete(result)
                if captured.is_complete()
                    && !result_is_query_local
                    && self.is_cacheable_type(result) =>
            {
                self.force_queries.insert(ty, QueryState::Ready(result));
            }
            Completion::Complete(_) | Completion::Deferred | Completion::Cycle => {
                self.force_queries.remove(&ty);
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
        depth: usize,
    ) -> Completion<TypeId> {
        if !self.semantic_declaration_is_claimed(declaration) {
            return Completion::Deferred;
        }
        if self
            .program
            .standard_library_declaration(declaration)
            .is_some()
        {
            if self
                .program
                .standard_library
                .is_property_key_type(declaration)
                && arguments.is_empty()
            {
                return self.property_key_type();
            }
            if self
                .program
                .standard_library
                .is_homogeneous_record_type(declaration)
            {
                if completed!(self.record_key_constraint_check(declaration, arguments, depth,))
                    .is_err()
                {
                    return Completion::Complete(self.store.builtins.error);
                }
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
                return Completion::Complete(
                    self.library_reference(declaration, arguments.to_vec()),
                );
            }
            return Completion::Deferred;
        }
        let Some(model) = self.models.get(&declaration).copied() else {
            return Completion::Deferred;
        };
        if matches!(
            model,
            DeclarationModel::TypeAlias { .. }
                | DeclarationModel::Interface { .. }
                | DeclarationModel::Class { .. }
        ) && !self.is_single_type_symbol_declaration(declaration)
        {
            return Completion::Deferred;
        }
        let reference_parameters = model.type_parameters().map(|(parameters, _)| parameters);
        if reference_parameters.is_some_and(|parameters| {
            arguments.len() != parameters.len() || !object_shape::plain_type_parameters(parameters)
        }) {
            // Unmodeled parameters cannot enter a definitive reference cache.
            return Completion::Deferred;
        }
        self.evaluate_reference_model(declaration, model, arguments)
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
                    self.store
                        .type_parameter(declaration, index as u32, &parameter.name)
                });
                (parameter.name.clone(), ty)
            })
            .collect()
    }

    fn report_deferred_cycle(&mut self, deferred: &DeferredType) {
        let DeferredType::Reference { declaration, .. } = deferred else {
            return;
        };
        if let Some(DeclarationModel::TypeAlias {
            declaration: alias, ..
        }) = self.models.get(declaration).copied()
        {
            self.push_diagnostic(
                declaration.file,
                alias.name_span,
                format!("Type alias '{}' circularly references itself.", alias.name),
                2456,
            );
        }
    }

    fn push_diagnostic(&mut self, file: FileId, span: Span, message: String, code: u32) {
        // Contextual grammar owns this token; do not add missing-name cascades.
        if code == 2304
            && self.program.files[file.0 as usize]
                .syntax
                .contextual_grammar_facts()
                .iter()
                .any(|fact| fact.span == span)
        {
            return;
        }
        if !self.semantic_diagnostic_is_enabled(file) {
            return;
        }
        // Program boundary deduplicates the complete public identity.
        let source = &self.program.files[file.0 as usize].source;
        self.diagnostics
            .push(Diagnostic::at(source, span, message, code));
    }
}
