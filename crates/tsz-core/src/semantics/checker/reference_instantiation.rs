use std::collections::HashSet;

use crate::semantics::relation::{
    EvaluationDepth, RelationFailure, RelationFailureKind, RelationMode, RelationPropertyOrder,
    relate_with_property_order_at_evaluation_depth,
};
use crate::semantics::types::{
    Completion, IndexKeyKind, IndexSignature, LiteralProvenance, ObjectShape, TypeId, TypeKind,
};
use crate::source::DeclId;
use crate::syntax::{Literal, TypeNode, TypeNodeKind, TypeParameterDeclaration};

use super::{Checker, DeclarationModel};

/// Whether a reference is exact, fully explicit, or normalized with defaults.
/// Normalized arguments have query-local identity and never cache under the raw reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReferenceInstantiation {
    Exact,
    Explicit,
    Defaulted { arguments: Vec<TypeId> },
}

#[derive(Debug, Clone)]
pub(super) struct ReferenceConstraintFailure {
    pub(super) argument_index: usize,
    pub(super) reason: RelationFailure,
}

impl ReferenceInstantiation {
    pub(super) const fn is_query_local(&self) -> bool {
        matches!(self, Self::Defaulted { .. })
    }
}

impl Checker<'_> {
    pub(super) fn record_key_constraint_check(
        &mut self,
        declaration: DeclId,
        arguments: &[TypeId],
        depth: usize,
    ) -> Completion<Result<(), ReferenceConstraintFailure>> {
        if !self.owns_record_key_constraint(declaration) {
            return Completion::Deferred;
        }
        let [key, _] = arguments else {
            return Completion::Deferred;
        };
        let constraint = completed!(self.property_key_type());
        let relation =
            completed!(self.reference_argument_constraint_relation(*key, constraint, depth + 1,));
        constraint_check(0, relation)
    }

    pub(super) fn evaluate_reference_model(
        &mut self,
        declaration: DeclId,
        model: DeclarationModel<'_>,
        arguments: &[TypeId],
    ) -> Completion<TypeId> {
        match model {
            DeclarationModel::TypeAlias {
                declaration: alias,
                scope,
            } => self.evaluate_type_alias_reference(declaration, alias, scope, arguments),
            DeclarationModel::Interface {
                declaration: interface,
                scope,
            } => {
                let parameters =
                    self.substitution(declaration, &interface.type_parameters, arguments);
                match self.resolve_interface_shape(declaration, interface, scope, &parameters) {
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
            | DeclarationModel::Function { .. }
            | DeclarationModel::JavaScriptProperty(..) => Completion::Deferred,
        }
    }

    pub(super) fn evaluate_reference_instantiation(
        &mut self,
        declaration: DeclId,
        supplied: &[TypeId],
        instantiation: Completion<ReferenceInstantiation>,
        depth: usize,
    ) -> Completion<TypeId> {
        match instantiation {
            Completion::Complete(ReferenceInstantiation::Exact) => {
                self.evaluate_reference(declaration, supplied, depth)
            }
            Completion::Complete(ReferenceInstantiation::Explicit) => {
                completed!(self.explicit_reference_constraints_supported(
                    declaration,
                    supplied,
                    depth,
                ));
                self.evaluate_local_reference_instantiation(declaration, supplied)
            }
            Completion::Complete(ReferenceInstantiation::Defaulted { arguments }) => {
                self.evaluate_local_reference_instantiation(declaration, &arguments)
            }
            Completion::Deferred => Completion::Deferred,
            Completion::Cycle => Completion::Cycle,
            Completion::Limit => Completion::Limit,
        }
    }

    /// Normalize only a trailing suffix of closed defaults; references,
    /// substitutions, constraints, and deferred literals remain symbolic.
    pub(super) fn reference_instantiation(
        &mut self,
        declaration: DeclId,
        supplied: &[TypeId],
    ) -> Completion<ReferenceInstantiation> {
        if !self.semantic_declaration_is_claimed(declaration) {
            return Completion::Deferred;
        }
        let Some(model) = self.models.get(&declaration).copied() else {
            return Completion::Complete(ReferenceInstantiation::Exact);
        };
        let Some((parameters, _)) = model.type_parameters() else {
            return Completion::Complete(ReferenceInstantiation::Exact);
        };

        // Fully supplied arguments do not materialize defaults. Plain
        // constraints advance only through the bounded relation query below;
        // parameter modifiers retain their existing deferred owner.
        if supplied.len() == parameters.len() {
            if super::object_shape::plain_type_parameters(parameters) {
                return Completion::Complete(ReferenceInstantiation::Exact);
            }
            if explicit_type_parameters_supported(parameters) {
                return Completion::Complete(ReferenceInstantiation::Explicit);
            }
        }

        let defaults = match self.closed_trailing_defaults(parameters) {
            Some(defaults) => defaults,
            None => return Completion::Deferred,
        };
        if supplied.len() > parameters.len()
            || parameters[supplied.len()..]
                .iter()
                .any(|parameter| parameter.default.is_none())
        {
            return Completion::Deferred;
        }

        let mut arguments = supplied.to_vec();
        arguments.extend(defaults.into_iter().skip(supplied.len()).flatten());
        debug_assert_eq!(arguments.len(), parameters.len());
        Completion::Complete(ReferenceInstantiation::Defaulted { arguments })
    }

    fn closed_trailing_defaults(
        &mut self,
        parameters: &[TypeParameterDeclaration],
    ) -> Option<Vec<Option<TypeId>>> {
        let mut names = HashSet::new();
        let mut defaults_started = false;
        let mut saw_default = false;
        let mut defaults = Vec::with_capacity(parameters.len());

        for parameter in parameters {
            if !names.insert(parameter.name.as_str())
                || parameter.constraint.is_some()
                || parameter.const_parameter
                || parameter.in_variance
                || parameter.out_variance
            {
                return None;
            }
            match &parameter.default {
                Some(default) => {
                    defaults_started = true;
                    saw_default = true;
                    defaults.push(Some(self.closed_default_type(default)?));
                }
                None if defaults_started => return None,
                None => defaults.push(None),
            }
        }

        saw_default.then_some(defaults)
    }

    fn closed_default_type(&mut self, node: &TypeNode) -> Option<TypeId> {
        match &node.kind {
            TypeNodeKind::Keyword(keyword) => self.store.builtins.keyword(*keyword),
            TypeNodeKind::Literal(Literal::BigInt(_)) => None,
            TypeNodeKind::Literal(literal) => {
                Some(self.literal_type(literal, LiteralProvenance::Regular))
            }
            TypeNodeKind::Parenthesized(inner) => self.closed_default_type(inner),
            _ => None,
        }
    }

    /// Evaluate a local application after parameter/default ownership is known.
    fn evaluate_local_reference_instantiation(
        &mut self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> Completion<TypeId> {
        if !self.semantic_declaration_is_claimed(declaration) {
            return Completion::Deferred;
        }
        let Some(model) = self.models.get(&declaration).copied() else {
            return Completion::Deferred;
        };
        if !self.is_single_type_symbol_declaration(declaration) {
            return Completion::Deferred;
        }
        self.evaluate_reference_model(declaration, model, arguments)
    }

    fn explicit_reference_constraints_supported(
        &mut self,
        declaration: DeclId,
        arguments: &[TypeId],
        depth: usize,
    ) -> Completion<()> {
        let Some((parameters, scope)) = self
            .models
            .get(&declaration)
            .copied()
            .and_then(DeclarationModel::type_parameters)
        else {
            return Completion::Deferred;
        };
        let substitutions = self.substitution(declaration, parameters, arguments);
        for (argument_index, (parameter, argument)) in parameters.iter().zip(arguments).enumerate()
        {
            let Some(constraint) = &parameter.constraint else {
                continue;
            };
            let constraint =
                self.resolve_type_node(declaration.file, scope, constraint, &substitutions);
            let relation = completed!(self.reference_argument_constraint_relation(
                *argument,
                constraint,
                depth + 1,
            ));
            if completed!(constraint_check(argument_index, relation)).is_err() {
                return Completion::Deferred;
            }
        }
        Completion::Complete(())
    }

    fn reference_argument_constraint_relation(
        &mut self,
        argument: TypeId,
        mut constraint: TypeId,
        depth: usize,
    ) -> Completion<Result<(), RelationFailure>> {
        let mut relation_source = argument;
        let mut constrained_parameter = false;
        if let TypeKind::TypeParameter {
            declaration, index, ..
        } = self.store.kind(argument).clone()
        {
            let Some((parameters, scope)) = self
                .models
                .get(&declaration)
                .copied()
                .and_then(DeclarationModel::type_parameters)
            else {
                return Completion::Deferred;
            };
            let Some(parameter) = parameters.get(index as usize) else {
                return Completion::Deferred;
            };
            if let Some(parameter_constraint) = &parameter.constraint {
                let substitutions = self.substitution(declaration, parameters, &[]);
                relation_source = self.resolve_type_node(
                    declaration.file,
                    scope,
                    parameter_constraint,
                    &substitutions,
                );
                constrained_parameter = true;
            }
        }
        relation_source = completed!(self.force_operand(relation_source, depth));
        constraint = completed!(self.force_operand(constraint, depth));
        let relation = if relation_source == constraint {
            Ok(())
        } else {
            relate_with_property_order_at_evaluation_depth(
                self,
                relation_source,
                constraint,
                RelationMode::Assignment,
                RelationPropertyOrder::default(),
                EvaluationDepth::from_active_depth(depth),
            )
        };
        if relation.is_ok() {
            return Completion::Complete(relation);
        }
        if let Some(target) = completed!(self.broad_unknown_record_target(constraint, depth + 1))
            && relate_with_property_order_at_evaluation_depth(
                self,
                relation_source,
                target,
                RelationMode::Assignment,
                RelationPropertyOrder::default(),
                EvaluationDepth::from_active_depth(depth),
            )
            .is_ok()
        {
            return Completion::Complete(Ok(()));
        }
        let mut reason = relation.unwrap_err();
        if constrained_parameter
            && !matches!(
                reason.kind,
                RelationFailureKind::Deferred
                    | RelationFailureKind::InvalidProjection
                    | RelationFailureKind::Cycle
                    | RelationFailureKind::ComplexityLimit
            )
        {
            reason = RelationFailure {
                source: argument,
                target: constraint,
                kind: RelationFailureKind::Incompatible,
                child: Some(Box::new(reason)),
            };
        }
        Completion::Complete(Err(reason))
    }

    pub(super) fn owns_record_key_constraint(&self, declaration: DeclId) -> bool {
        self.program
            .standard_library
            .is_homogeneous_record_type(declaration)
            && !self
                .program
                .standard_library_type_has_authored_declarations(declaration)
    }

    fn broad_unknown_record_target(
        &mut self,
        constraint: TypeId,
        depth: usize,
    ) -> Completion<Option<TypeId>> {
        let TypeKind::LibraryReference {
            declaration,
            arguments,
            ..
        } = self.store.kind(constraint).clone()
        else {
            return Completion::Complete(None);
        };
        let [key, value] = arguments.as_slice() else {
            return Completion::Complete(None);
        };
        let value = completed!(self.force_operand(*value, depth + 1));
        if !self
            .program
            .standard_library
            .is_homogeneous_record_type(declaration)
            || !matches!(self.store.kind(value), TypeKind::Unknown)
        {
            return Completion::Complete(None);
        }
        let key = completed!(self.force_operand(*key, depth + 1));
        let accepts_string_properties = match self.store.kind(key) {
            TypeKind::String | TypeKind::Any => true,
            TypeKind::Union(members) => members
                .iter()
                .any(|member| matches!(self.store.kind(*member), TypeKind::String | TypeKind::Any)),
            _ => false,
        };
        if !accepts_string_properties {
            return Completion::Complete(None);
        }
        Completion::Complete(Some(self.store.object_shape(ObjectShape {
            index_signatures: vec![IndexSignature {
                key: IndexKeyKind::String,
                value,
                readonly: false,
            }],
            ..ObjectShape::default()
        })))
    }

    pub(super) fn symbolic_keyof_operand_supported(&self, operand: TypeId) -> Completion<()> {
        let TypeKind::TypeParameter {
            declaration, index, ..
        } = self.store.kind(operand)
        else {
            return Completion::Deferred;
        };
        self.models
            .get(declaration)
            .copied()
            .and_then(DeclarationModel::type_parameters)
            .and_then(|(parameters, _)| parameters.get(*index as usize))
            .map_or(Completion::Deferred, |_| Completion::Complete(()))
    }
}

fn constraint_check(
    argument_index: usize,
    relation: Result<(), RelationFailure>,
) -> Completion<Result<(), ReferenceConstraintFailure>> {
    let Err(reason) = relation else {
        return Completion::Complete(Ok(()));
    };
    match reason.kind {
        RelationFailureKind::Deferred | RelationFailureKind::InvalidProjection => {
            Completion::Deferred
        }
        RelationFailureKind::Cycle => Completion::Cycle,
        RelationFailureKind::ComplexityLimit => Completion::Limit,
        _ => Completion::Complete(Err(ReferenceConstraintFailure {
            argument_index,
            reason,
        })),
    }
}

fn explicit_type_parameters_supported(parameters: &[TypeParameterDeclaration]) -> bool {
    let mut names = HashSet::new();
    parameters.iter().all(|parameter| {
        names.insert(parameter.name.as_str())
            && !parameter.const_parameter
            && !parameter.in_variance
            && !parameter.out_variance
    })
}
