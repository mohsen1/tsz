use crate::bind::ScopeId;
use crate::program::SemanticCompletion;
use crate::semantics::relation::{RelationContext, RelationMode};
use crate::semantics::types::{
    Completion, DeferredType, ShapeParameter, ShapeSignature, TypeId, TypeKind,
};
use crate::source::{DeclId, FileId};
use crate::syntax::{Expression, ExpressionKind};

use super::{
    Checker, DeclarationModel,
    generic_call_instantiation::IdentityCallInstantiation,
    projection_model::peel_expression_parentheses,
    relation_diagnostic::{ContextualType, RelationDiagnosticOutcome, RelationDiagnosticStyle},
};

impl Checker<'_> {
    pub(super) fn callable_signature(&self, ty: TypeId) -> Option<ShapeSignature> {
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

    pub(super) fn infer_call_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        callee: &Expression,
        has_type_arguments: bool,
        arguments: &[Expression],
    ) -> TypeId {
        let member_callee = peel_expression_parentheses(callee);
        let callee_query = if let ExpressionKind::Member {
            object,
            name,
            name_span,
        } = &member_callee.kind
        {
            let ty = self
                .lexical_this_method_type(file, scope, object, name)
                .unwrap_or_else(|| {
                    self.infer_member_expression(file, scope, object, name, *name_span, true)
                });
            self.expression_type_origins.insert((file, callee.id), ty);
            ty
        } else {
            self.infer_expression(file, scope, callee, None)
        };
        if has_type_arguments {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            self.infer_deferred_call_arguments(file, scope, arguments);
            return self.store.deferred_generic_call();
        }
        let completion = self.force_type(callee_query, 0);
        let callee_type = match self.require_completion(completion) {
            Completion::Complete(callee_type) => callee_type,
            Completion::Deferred | Completion::Cycle | Completion::Limit => {
                self.infer_deferred_call_arguments(file, scope, arguments);
                return self.deferred_call_type(callee_query, arguments.len());
            }
        };
        let Some(signature) = self.callable_signature(callee_type) else {
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
            if self.authored_shape_display_is_unavailable(callee_type) {
                self.infer_deferred_call_arguments(file, scope, arguments);
                return self.deferred_call_type(callee_type, arguments.len());
            }
            if !matches!(
                self.store.kind(callee_type),
                TypeKind::Any | TypeKind::Error
            ) {
                let name = self.store.display(callee_type);
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
        let signature_owner = match self.store.kind(callee_type) {
            TypeKind::Function(signature) => signature.generic_declaration,
            _ => None,
        };
        let rest_index = signature
            .parameters
            .iter()
            .position(|parameter| parameter.rest);
        let fixed_rest_arity =
            rest_index.and_then(
                |index| match self.store.kind(signature.parameters[index].ty) {
                    TypeKind::Tuple(elements) => Some(elements.len()),
                    _ => None,
                },
            );
        let required = signature
            .parameters
            .iter()
            .take(rest_index.unwrap_or(signature.parameters.len()))
            .filter(|parameter| !parameter.optional)
            .count()
            + fixed_rest_arity.unwrap_or(0);
        let maximum = rest_index.map_or(Some(signature.parameters.len()), |index| {
            fixed_rest_arity.map(|arity| index + arity)
        });
        let too_few = arguments.len() < required;
        let too_many = maximum.is_some_and(|maximum| arguments.len() > maximum);
        let mut generic_instantiation = signature_owner
            .map(|owner| IdentityCallInstantiation::new(&self.store, owner, &signature));
        if too_many && let Some(instantiation) = &mut generic_instantiation {
            instantiation.reject();
        }
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
            let target_order =
                self.relation_order_for_call_argument(file, scope, callee, index, parameter.rest);
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
        signature: &ShapeSignature,
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
        let callee = match self.force_type(callee, depth) {
            Completion::Complete(callee) => callee,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
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
        peel_expression_parentheses(expression).kind,
        ExpressionKind::Identifier { entity_name, .. } if entity_name
    )
}
