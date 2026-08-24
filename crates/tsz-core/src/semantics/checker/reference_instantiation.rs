use std::collections::HashSet;

use crate::semantics::types::{Completion, LiteralProvenance, TypeId};
use crate::source::DeclId;
use crate::syntax::{KeywordType, Literal, TypeNode, TypeNodeKind, TypeParameterDeclaration};

use super::{Checker, DeclarationModel};

/// The finite argument decision made by a reference application.
///
/// `Exact` preserves the existing b17 exact-arity path. `Defaulted` is
/// query-local: the authored reference and its normalized argument vector
/// have different identities, so the raw reference must not enter `Ready`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReferenceInstantiation {
    Exact,
    Defaulted { arguments: Vec<TypeId> },
}

impl ReferenceInstantiation {
    pub(super) const fn is_query_local(&self) -> bool {
        matches!(self, Self::Defaulted { .. })
    }
}

impl Checker<'_> {
    pub(super) fn evaluate_reference_instantiation(
        &mut self,
        declaration: DeclId,
        supplied: &[TypeId],
        instantiation: Completion<ReferenceInstantiation>,
    ) -> Completion<TypeId> {
        match instantiation {
            Completion::Complete(ReferenceInstantiation::Exact) => {
                self.evaluate_reference(declaration, supplied)
            }
            Completion::Complete(ReferenceInstantiation::Defaulted { arguments }) => {
                self.evaluate_defaulted_reference(declaration, &arguments)
            }
            Completion::Deferred => Completion::Deferred,
            Completion::Cycle => Completion::Cycle,
            Completion::Limit => Completion::Limit,
        }
    }

    /// Normalize only a trailing suffix of structurally closed defaults.
    ///
    /// This deliberately does not resolve arbitrary type syntax. References,
    /// earlier-parameter substitution, constraints, and deferred literal
    /// forms need a provenance-bearing instantiation query and remain
    /// `Deferred` until that owner exists.
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
        let parameters = match model {
            DeclarationModel::TypeAlias {
                declaration: alias, ..
            } => alias.type_parameters.as_slice(),
            DeclarationModel::Interface {
                declaration: interface,
                ..
            } => interface.type_parameters.as_slice(),
            DeclarationModel::Class {
                declaration: class, ..
            } => class.type_parameters.as_slice(),
            DeclarationModel::Variable { .. }
            | DeclarationModel::Parameter { .. }
            | DeclarationModel::Function { .. } => {
                return Completion::Complete(ReferenceInstantiation::Exact);
            }
        };

        if supplied.len() == parameters.len()
            && super::object_shape::plain_type_parameters(parameters)
        {
            return Completion::Complete(ReferenceInstantiation::Exact);
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
            TypeNodeKind::Keyword(keyword) => Some(match keyword {
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
                KeywordType::UniqueSymbol => return None,
            }),
            TypeNodeKind::Literal(Literal::BigInt(_)) => None,
            TypeNodeKind::Literal(literal) => {
                Some(self.literal_type(literal, LiteralProvenance::Regular))
            }
            TypeNodeKind::Parenthesized(inner) => self.closed_default_type(inner),
            _ => None,
        }
    }

    /// Evaluate a normalized default application without weakening the plain
    /// reference gate used by exact-arity callers.
    pub(super) fn evaluate_defaulted_reference(
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
            | DeclarationModel::Function { .. } => Completion::Deferred,
        }
    }
}
