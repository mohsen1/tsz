use std::collections::{HashMap, HashSet};

use crate::bind::ScopeId;
use crate::program::SemanticCompletion;
use crate::semantics::relation::{RelationContext, RelationMode};
use crate::semantics::types::{
    CallArityGap, CallArityResolution, Completion, DeferredType, Signature, TypeId, TypeKind,
};
use crate::source::{DeclId, FileId};
use crate::standard_library::LibraryCallMember;
use crate::syntax::{Expression, ExpressionKind, FunctionLikeExpression, ParameterNameKind};

use super::{
    Checker, DeclarationModel,
    generic_call_instantiation::IdentityCallInstantiation,
    relation_diagnostic::{ContextualType, RelationDiagnosticOutcome, RelationDiagnosticStyle},
};

#[derive(Debug, Clone)]
pub(super) struct InferredCallCallee {
    pub(super) ty: TypeId,
    pub(super) library_member: Completion<Option<LibraryCallMember>>,
}

impl Checker<'_> {
    pub(super) fn callable_signature(&self, ty: TypeId) -> Option<Signature> {
        match self.store.kind(ty) {
            TypeKind::Function(signature) | TypeKind::ShapeFunction(signature) => {
                Some(signature.clone())
            }
            TypeKind::Object(shape) if shape.call_signatures.len() == 1 => {
                shape.call_signatures.first().cloned()
            }
            _ => None,
        }
    }

    pub(super) fn infer_call_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        callee: &Expression,
        has_type_arguments: bool,
        arguments: &[Expression],
    ) -> TypeId {
        let member_callee = callee.peel_parentheses();
        let member_name_span = match &member_callee.kind {
            ExpressionKind::Member { name_span, .. } => Some(*name_span),
            _ => None,
        };
        let inferred_callee = match &member_callee.kind {
            ExpressionKind::Member {
                object,
                name,
                name_span,
            } => self
                .lexical_this_method_type(file, scope, object, name)
                .map_or_else(
                    || {
                        let mut library_member = Completion::Complete(None);
                        let ty = self.infer_member_expression(
                            file,
                            scope,
                            object,
                            name,
                            *name_span,
                            Some(&mut library_member),
                        );
                        InferredCallCallee { ty, library_member }
                    },
                    |ty| InferredCallCallee {
                        ty,
                        library_member: Completion::Complete(None),
                    },
                ),
            ExpressionKind::ElementAccess { object, index } => {
                self.infer_element_access_call_callee(file, scope, object, index)
            }
            _ => InferredCallCallee {
                ty: self.infer_expression(file, scope, callee, None),
                library_member: Completion::Complete(None),
            },
        };
        let callee_query = inferred_callee.ty;
        self.expression_type_origins
            .insert((file, callee.id), callee_query);
        if has_type_arguments {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            self.infer_deferred_call_arguments(file, scope, arguments);
            return self.store.deferred_generic_call();
        }
        let Completion::Complete(standard_library_member) = inferred_callee.library_member else {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            self.infer_deferred_call_arguments(file, scope, arguments);
            return self.deferred_call_type(callee_query, arguments.len());
        };
        let completion = self.force_type(callee_query, 0);
        let callee_type = match self.require_completion(completion) {
            Completion::Complete(callee_type) => callee_type,
            Completion::Deferred | Completion::Cycle | Completion::Limit => {
                self.infer_deferred_call_arguments(file, scope, arguments);
                return self.deferred_call_type(callee_query, arguments.len());
            }
        };
        let Some(mut signature) = self.callable_signature(callee_type) else {
            if matches!(
                self.store.kind(callee_type),
                TypeKind::Boolean
                    | TypeKind::Number
                    | TypeKind::String
                    | TypeKind::BigInt
                    | TypeKind::ObjectKeyword
                    | TypeKind::Symbol
                    | TypeKind::Unknown
                    | TypeKind::Undefined
                    | TypeKind::Null
                    | TypeKind::TypeParameter { .. }
                    | TypeKind::Union(_)
                    | TypeKind::Intersection(_)
                    | TypeKind::ClassConstructor { .. }
            ) {
                // These identities require either an apparent composite call
                // signature or a dedicated unknown/nullish call diagnostic.
                // Until those queries exist, the bare type must not publish a
                // definitive TS2349.
                self.observe_file_completion(file, SemanticCompletion::Deferred);
                self.infer_deferred_call_arguments(file, scope, arguments);
                return self.deferred_call_type(callee_type, arguments.len());
            }
            let display = self.store.display(callee_type);
            let Completion::Complete(name) = self.require_file_completion(file, display) else {
                self.infer_deferred_call_arguments(file, scope, arguments);
                return self.deferred_call_type(callee_type, arguments.len());
            };
            if !matches!(
                self.store.kind(callee_type),
                TypeKind::Any | TypeKind::Error
            ) {
                self.push_diagnostic(
                    file,
                    callee.span,
                    format!(
                        "This expression is not callable. Type '{name}' has no call signatures."
                    ),
                    2349,
                );
            }
            self.infer_deferred_call_arguments(file, scope, arguments);
            return self.store.builtins.any;
        };
        let diagnostic_parameters = signature
            .parameters
            .iter()
            .map(|parameter| (parameter.ty, parameter.rest))
            .collect::<Vec<_>>();
        let signature_owner = match self.store.kind(callee_type) {
            TypeKind::Function(signature) => signature.generic_declaration,
            _ => None,
        };
        let direct_function = match &member_callee.kind {
            ExpressionKind::FunctionLike(function) => Some(function.as_ref()),
            _ => None,
        };
        let arity = self.effective_call_arity(direct_function, arguments.len(), &mut signature);
        let arity = match self.require_completion(arity) {
            Completion::Complete(arity) => Some(arity),
            Completion::Deferred | Completion::Cycle | Completion::Limit => None,
        };
        let rest_index = signature
            .parameters
            .iter()
            .position(|parameter| parameter.rest);
        let too_few = arity.is_some_and(|(minimum, _)| arguments.len() < minimum);
        let too_many = arity
            .is_some_and(|(_, maximum)| maximum.is_some_and(|maximum| arguments.len() > maximum));
        let standard_library_array_search = matches!(
            standard_library_member,
            Some(LibraryCallMember::IndexOf | LibraryCallMember::LastIndexOf)
        );
        if (too_few || too_many) && standard_library_array_search {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            self.infer_deferred_call_arguments(file, scope, arguments);
            return self.deferred_call_type(callee_type, arguments.len());
        }
        let mut generic_instantiation = signature_owner
            .map(|owner| IdentityCallInstantiation::new(&self.store, owner, &signature));
        if too_many && let Some(instantiation) = &mut generic_instantiation {
            instantiation.reject();
        }
        if too_few || too_many {
            let (required, maximum) = arity.expect("a count mismatch needs arity");
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
                    member_name_span.unwrap_or(callee.span)
                },
                format!(
                    "Expected {expected_count} arguments, but got {}.",
                    arguments.len()
                ),
                if maximum.is_none() { 2555 } else { 2554 },
            );
        }
        let mut deferred_standard_library_argument = false;
        let mut stopped_argument_relations = arity.is_none() || too_few || too_many;
        for (index, argument) in arguments.iter().enumerate() {
            let Some(parameter) = signature
                .parameters
                .get(index)
                .or_else(|| rest_index.and_then(|rest_index| signature.parameters.get(rest_index)))
            else {
                if let Some(instantiation) = &mut generic_instantiation {
                    instantiation.reject();
                    let _ = self.infer_expression_contextual(
                        file,
                        scope,
                        argument,
                        ContextualType::Deferred,
                    );
                } else {
                    let _ = self.infer_expression(file, scope, argument, None);
                }
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
            let diagnostic_expected = diagnostic_parameters
                .get(index)
                .or_else(|| rest_index.and_then(|rest_index| diagnostic_parameters.get(rest_index)))
                .map_or(expected, |(ty, rest)| {
                    if !rest {
                        return *ty;
                    }
                    match self.store.kind(*ty) {
                        TypeKind::Array(element) => *element,
                        TypeKind::Tuple(elements) => rest_index
                            .and_then(|rest_index| index.checked_sub(rest_index))
                            .and_then(|index| elements.get(index))
                            .copied()
                            .unwrap_or(expected),
                        _ => expected,
                    }
                });
            if let Some(instantiation) = &mut generic_instantiation
                && signature_owner.is_some_and(|owner| {
                    !self.store.type_parameters_from(expected, owner).is_empty()
                })
            {
                if transparent_identity_argument(argument) {
                    if let Completion::Complete(actual) =
                        self.identity_argument_type(file, scope, argument)
                        && instantiation.observe(&self.store, expected, actual)
                    {
                        continue;
                    }
                    instantiation.reject();
                } else {
                    instantiation.reject();
                    let _ = self.infer_expression_contextual(
                        file,
                        scope,
                        argument,
                        ContextualType::Deferred,
                    );
                }
                continue;
            }
            let context = ContextualType::Known(expected);
            let actual = self.infer_expression_contextual(file, scope, argument, context);
            if standard_library_member == Some(LibraryCallMember::Map) && index == 0 {
                let actual = self.complete_type(actual).unwrap_or(actual);
                if let Some(callback) = self.callable_signature(actual) {
                    signature.return_type =
                        self.store.intern(TypeKind::Array(callback.return_type));
                }
            }
            if let Some(member) = standard_library_member {
                match self.standard_library_argument_disposition(
                    member,
                    index,
                    arguments.len(),
                    expected,
                    actual,
                ) {
                    Completion::Complete(true) => continue,
                    Completion::Complete(false) => {}
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        deferred_standard_library_argument = true;
                        stopped_argument_relations = true;
                        continue;
                    }
                }
            }
            if !stopped_argument_relations {
                stopped_argument_relations = !matches!(
                    self.report_relation_with_diagnostic_target(
                        actual,
                        expected,
                        diagnostic_expected,
                        argument.span,
                        Some(argument),
                        RelationMode::Assignment,
                        RelationDiagnosticStyle::Argument,
                    ),
                    RelationDiagnosticOutcome::Compatible
                );
            }
        }
        if deferred_standard_library_argument {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            return self.deferred_call_type(callee_type, arguments.len());
        }
        let direct_declaration = match self.store.kind(callee_query) {
            TypeKind::Deferred(DeferredType::Value(declaration)) => Some(*declaration),
            _ => None,
        };
        if self.direct_result(
            direct_declaration,
            signature_owner,
            &signature,
            arguments.len(),
        ) == Some(true)
            && transparent_identity_argument(&arguments[0])
        {
            return self.infer_expression(file, scope, &arguments[0], None);
        }
        if let Some(instantiation) = generic_instantiation
            && !instantiation.is_complete()
        {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            return self.store.deferred_generic_call();
        }
        signature.return_type
    }

    fn effective_call_arity(
        &mut self,
        direct_function: Option<&FunctionLikeExpression>,
        argument_count: usize,
        signature: &mut Signature,
    ) -> Completion<(usize, Option<usize>)> {
        if let Some(function) = direct_function {
            signature.untyped_javascript = false;
            let authored = function
                .parameters
                .iter()
                .filter(|parameter| parameter.name_kind == ParameterNameKind::Binding);
            if authored.clone().count() != signature.parameters.len() {
                return Completion::Deferred;
            }
            for (parameter, authored) in signature
                .parameters
                .iter_mut()
                .zip(authored)
                .skip(argument_count)
            {
                parameter.optional |= authored.annotation.is_none() && !authored.rest;
            }
        }
        let mut references = HashMap::new();
        let arity = loop {
            match self.store.effective_call_arity(signature, &references) {
                Ok((minimum, maximum)) => {
                    break Completion::Complete((minimum, maximum));
                }
                Err(CallArityGap::Deferred) => break Completion::Deferred,
                Err(CallArityGap::Type(ty)) => {
                    let resolution = self.complete_type(ty).map_or(Completion::Deferred, |ty| {
                        Completion::Complete(CallArityResolution::Expanded(ty))
                    });
                    references.insert(ty, resolution);
                }
                Err(CallArityGap::Reference(reference, declaration, arguments)) => {
                    let resolution = match self.models.get(&declaration) {
                        Some(DeclarationModel::TypeAlias { .. }) => {
                            let instantiation =
                                self.reference_instantiation(declaration, &arguments);
                            let expansion = self.evaluate_reference_instantiation(
                                declaration,
                                &arguments,
                                instantiation,
                                0,
                            );
                            match self.require_completion(expansion) {
                                Completion::Complete(ty) => {
                                    Completion::Complete(CallArityResolution::Expanded(ty))
                                }
                                Completion::Deferred => Completion::Deferred,
                                Completion::Cycle => Completion::Cycle,
                                Completion::Limit => Completion::Limit,
                            }
                        }
                        Some(
                            DeclarationModel::Interface { .. } | DeclarationModel::Class { .. },
                        ) => Completion::Complete(CallArityResolution::OpaqueRequired),
                        _ if self
                            .program
                            .standard_library
                            .is_rest_array_type(declaration)
                            && arguments.len() == 1 =>
                        {
                            Completion::Complete(CallArityResolution::RestArray(arguments[0]))
                        }
                        _ if self
                            .program
                            .standard_library
                            .is_homogeneous_record_type(declaration)
                            && arguments.len() == 2 =>
                        {
                            Completion::Complete(CallArityResolution::OpaqueRequired)
                        }
                        _ => Completion::Deferred,
                    };
                    references.insert(reference, resolution);
                }
            }
        };
        for parameter in &mut signature.parameters {
            let mut seen = HashSet::new();
            while let Some(Completion::Complete(resolution)) = references.get(&parameter.ty) {
                match *resolution {
                    CallArityResolution::Expanded(resolved) => {
                        if !seen.insert(parameter.ty) {
                            return Completion::Deferred;
                        }
                        parameter.ty = resolved;
                    }
                    CallArityResolution::RestArray(element) if parameter.rest => {
                        parameter.ty = self.store.intern(TypeKind::Array(element));
                        break;
                    }
                    CallArityResolution::OpaqueRequired | CallArityResolution::RestArray(_) => {
                        break;
                    }
                }
            }
        }
        arity
    }

    fn infer_deferred_call_arguments(
        &mut self,
        file: FileId,
        scope: ScopeId,
        arguments: &[Expression],
    ) {
        for argument in arguments {
            let _ =
                self.infer_expression_contextual(file, scope, argument, ContextualType::Deferred);
        }
    }

    fn standard_library_argument_disposition(
        &mut self,
        member: LibraryCallMember,
        index: usize,
        argument_count: usize,
        expected: TypeId,
        actual: TypeId,
    ) -> Completion<bool> {
        let element_argument = match member {
            LibraryCallMember::IndexOf | LibraryCallMember::LastIndexOf => index == 0,
            LibraryCallMember::Push => true,
            LibraryCallMember::Splice => index >= 2,
            LibraryCallMember::Map
            | LibraryCallMember::Slice
            | LibraryCallMember::MapGet
            | LibraryCallMember::MapSet
            | LibraryCallMember::ToString => false,
        };
        if element_argument && matches!(self.store.kind(expected), TypeKind::TypeParameter { .. }) {
            let actual = (actual == expected)
                .then_some(actual)
                .or_else(|| self.complete_type(actual));
            return if actual == Some(expected) {
                Completion::Complete(true)
            } else {
                Completion::Deferred
            };
        }
        if matches!(self.store.kind(actual), TypeKind::Undefined)
            && (member == LibraryCallMember::Slice
                || matches!(
                    member,
                    LibraryCallMember::IndexOf | LibraryCallMember::LastIndexOf
                ) && index == 1
                || member == LibraryCallMember::Splice && index == 1 && argument_count == 2)
        {
            return Completion::Complete(true);
        }
        if member == LibraryCallMember::Splice
            && index < 2
            && !matches!(
                self.store.kind(actual),
                TypeKind::Number | TypeKind::LiteralNumber(_, _)
            )
        {
            return Completion::Deferred;
        }
        if !matches!(
            member,
            LibraryCallMember::IndexOf | LibraryCallMember::LastIndexOf
        ) || index != 1
        {
            return Completion::Complete(false);
        }
        let actual = match self.store.kind(actual) {
            TypeKind::Deferred(_) => self.complete_type(actual),
            _ => Some(actual),
        };
        let Some(actual) = actual else {
            return Completion::Deferred;
        };
        match self.store.kind(actual) {
            TypeKind::Undefined => Completion::Complete(true),
            TypeKind::Null if self.options.effective_strict_null_checks() => Completion::Deferred,
            TypeKind::TypeParameter { .. } | TypeKind::Union(_) => Completion::Deferred,
            _ => Completion::Complete(false),
        }
    }

    fn identity_argument_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        argument: &Expression,
    ) -> Completion<TypeId> {
        let query = self.infer_expression(file, scope, argument, None);
        let declaration = match self.store.kind(query) {
            TypeKind::Deferred(DeferredType::Value(declaration)) => Some(*declaration),
            _ => None,
        };
        declaration.map_or(Completion::Complete(query), |declaration| {
            self.declaration_value_type(declaration)
        })
    }

    fn deferred_call_type(&mut self, callee: TypeId, argument_count: usize) -> TypeId {
        self.store.intern(TypeKind::Deferred(DeferredType::Call {
            callee,
            argument_count,
        }))
    }

    pub(super) fn direct_result(
        &self,
        declaration: Option<DeclId>,
        signature_owner: Option<DeclId>,
        signature: &Signature,
        argument_count: usize,
    ) -> Option<bool> {
        if signature.parameters.len() != argument_count
            || signature.parameters.iter().any(|p| p.optional || p.rest)
            || declaration.is_some_and(|id| {
                !self.semantic_declaration_is_claimed(id)
                    || self.function_value_requires_overload_resolution(id)
            })
        {
            return None;
        }
        let Some(owner) = signature_owner else {
            return Some(false);
        };
        let plain = declaration == Some(owner)
            && argument_count == 1
            && signature.parameters[0].ty == signature.return_type
            && matches!(self.store.kind(signature.return_type), TypeKind::TypeParameter { declaration, index: 0, .. } if *declaration == owner)
            && matches!(self.models.get(&owner), Some(DeclarationModel::Function { declaration, .. }) if matches!(declaration.type_parameters.as_slice(), [parameter] if parameter.constraint.is_none() && parameter.default.is_none() && !parameter.const_parameter && !parameter.in_variance && !parameter.out_variance));
        if plain {
            Some(true)
        } else {
            self.store
                .type_parameters_from(signature.return_type, owner)
                .is_empty()
                .then_some(false)
        }
    }

    pub(super) fn evaluate_call(
        &mut self,
        callee: TypeId,
        argument_count: usize,
        depth: usize,
    ) -> Completion<TypeId> {
        let declaration = match self.store.kind(callee) {
            TypeKind::Deferred(DeferredType::Value(declaration)) => Some(*declaration),
            _ => None,
        };
        let callee = completed!(self.force_type(callee, depth));
        self.direct_call_type(declaration, callee, None, argument_count)
    }

    pub(super) fn direct_call_type(
        &mut self,
        declaration: Option<DeclId>,
        callee: TypeId,
        identity_argument: Option<TypeId>,
        argument_count: usize,
    ) -> Completion<TypeId> {
        match self.store.kind(callee) {
            TypeKind::Any => return Completion::Complete(self.store.builtins.any),
            TypeKind::Error | TypeKind::Invalid(_) => return Completion::Complete(callee),
            _ => {}
        }
        let signature_owner = match self.store.kind(callee) {
            TypeKind::Function(signature) => signature.generic_declaration,
            _ => None,
        };
        let Some(signature) = self.callable_signature(callee) else {
            return Completion::Deferred;
        };
        match self.direct_result(declaration, signature_owner, &signature, argument_count) {
            Some(false) => Completion::Complete(signature.return_type),
            Some(true) => identity_argument.map_or(Completion::Deferred, Completion::Complete),
            None => Completion::Deferred,
        }
    }
}

fn transparent_identity_argument(expression: &Expression) -> bool {
    matches!(
        expression.peel_parentheses().kind,
        ExpressionKind::Identifier { entity_name, .. } if entity_name
    )
}
