use crate::diagnostics::{Diagnostic, RelatedInformation};
use crate::program::SemanticCompletion;
use crate::source::Span;
use crate::syntax::{Expression, ExpressionKind, Literal};

use super::Checker;
use super::projection_model::PropertyOrderTree;
use crate::semantics::relation::{
    RelationFailure, RelationFailureKind, RelationMode, RelationPropertyOrder,
    relate_with_property_order,
};
use crate::semantics::types::{Completion, IndexKeyKind, TypeId, TypeKind, UnionPolicy};

#[derive(Clone, Copy)]
pub(super) enum RelationDiagnosticStyle {
    Type,
    Argument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RelationDiagnosticOutcome {
    Compatible,
    Reported,
    Deferred,
}

const fn combine_diagnostic_outcomes(
    left: RelationDiagnosticOutcome,
    right: RelationDiagnosticOutcome,
) -> RelationDiagnosticOutcome {
    if matches!(left, RelationDiagnosticOutcome::Reported)
        || matches!(right, RelationDiagnosticOutcome::Reported)
    {
        RelationDiagnosticOutcome::Reported
    } else if matches!(left, RelationDiagnosticOutcome::Deferred)
        || matches!(right, RelationDiagnosticOutcome::Deferred)
    {
        RelationDiagnosticOutcome::Deferred
    } else {
        RelationDiagnosticOutcome::Compatible
    }
}

pub(super) enum ContextualPropertyType {
    Known(TypeId),
    Absent,
    Deferred,
}

impl RelationDiagnosticStyle {
    const fn code(self) -> u32 {
        match self {
            Self::Type => 2322,
            Self::Argument => 2345,
        }
    }
}

impl Checker<'_> {
    pub(super) fn report_relation(
        &mut self,
        source: TypeId,
        target: TypeId,
        span: Span,
        source_expression: Option<&Expression>,
        target_order: Option<PropertyOrderTree>,
        mode: RelationMode,
        style: RelationDiagnosticStyle,
    ) -> RelationDiagnosticOutcome {
        let (property_order, target_origins) = self.relation_origins(target, target_order.as_ref());
        let Err(failure) = relate_with_property_order(self, source, target, mode, property_order)
        else {
            return RelationDiagnosticOutcome::Compatible;
        };
        if let Some(expression) = source_expression {
            for outcome in [
                self.report_contextual_array_elements(
                    expression,
                    target,
                    target_order.as_ref(),
                    mode,
                ),
                self.report_contextual_object_properties(
                    expression,
                    target,
                    target_order.as_ref(),
                    mode,
                ),
            ] {
                if !matches!(outcome, RelationDiagnosticOutcome::Compatible) {
                    return outcome;
                }
            }
        }
        match failure.kind {
            RelationFailureKind::Deferred => {
                self.semantic_completion = self
                    .semantic_completion
                    .combine(SemanticCompletion::Deferred);
                return RelationDiagnosticOutcome::Deferred;
            }
            RelationFailureKind::Cycle => {
                self.semantic_completion =
                    self.semantic_completion.combine(SemanticCompletion::Cycle);
                return RelationDiagnosticOutcome::Deferred;
            }
            RelationFailureKind::InvalidProjection => {
                return RelationDiagnosticOutcome::Deferred;
            }
            RelationFailureKind::ComplexityLimit => {
                self.semantic_completion =
                    self.semantic_completion.combine(SemanticCompletion::Limit);
                self.push_diagnostic(
                    span.file,
                    span,
                    "Type instantiation is excessively deep and possibly infinite.".to_string(),
                    2589,
                );
                return RelationDiagnosticOutcome::Reported;
            }
            RelationFailureKind::Incompatible
            | RelationFailureKind::MissingProperty(_)
            | RelationFailureKind::MissingProperties(_)
            | RelationFailureKind::Property(_)
            | RelationFailureKind::Object
            | RelationFailureKind::ArrayElement
            | RelationFailureKind::TupleElement(_)
            | RelationFailureKind::ArrayToTupleLength { .. }
            | RelationFailureKind::UnionMember
            | RelationFailureKind::AliasExpansion
            | RelationFailureKind::Parameter(_)
            | RelationFailureKind::Return => {}
        }

        for ty in [source, target] {
            match self.requires_authored_shape_display(ty) {
                Completion::Complete(false) => {}
                Completion::Complete(true) | Completion::Deferred => {
                    self.semantic_completion = self
                        .semantic_completion
                        .combine(SemanticCompletion::Deferred);
                    return RelationDiagnosticOutcome::Deferred;
                }
                Completion::Cycle => {
                    self.semantic_completion =
                        self.semantic_completion.combine(SemanticCompletion::Cycle);
                    return RelationDiagnosticOutcome::Deferred;
                }
                Completion::Limit => {
                    self.semantic_completion =
                        self.semantic_completion.combine(SemanticCompletion::Limit);
                    return RelationDiagnosticOutcome::Deferred;
                }
            }
        }

        let diagnostic_span = source_expression
            .and_then(|expression| {
                relation_property_name(&failure)
                    .and_then(|name| object_property_span(expression, name))
            })
            .unwrap_or(span);
        let primary = &failure;
        let diagnostic_code = style.code();
        let source_order = source_expression.and_then(|expression| {
            self.expression_order_origins
                .get(&(expression.span.file, expression.id))
                .cloned()
        });
        let (_, source_origins) = self.relation_origins(source, source_order.as_ref());
        let source_name =
            self.relation_source_name(primary.source, primary.target, source_order.as_ref());
        let target_name = self.type_name_with_order(primary.target, target_order.as_ref());
        let message = match style {
            RelationDiagnosticStyle::Argument => format!(
                "Argument of type '{source_name}' is not assignable to parameter of type '{target_name}'."
            ),
            RelationDiagnosticStyle::Type => {
                format!("Type '{source_name}' is not assignable to type '{target_name}'.")
            }
        };
        let related =
            self.relation_continuations(primary, diagnostic_code, &source_origins, &target_origins);
        self.push_relation_diagnostic(
            diagnostic_span,
            message,
            diagnostic_code,
            related,
            primary.clone(),
        );
        RelationDiagnosticOutcome::Reported
    }

    fn relation_origins(
        &mut self,
        target: TypeId,
        origin: Option<&PropertyOrderTree>,
    ) -> (RelationPropertyOrder, HashMap<TypeId, PropertyOrderTree>) {
        let mut order = RelationPropertyOrder::default();
        let mut origins = HashMap::new();
        if let Some(origin) = origin {
            self.collect_relation_property_order(target, origin, &mut order, &mut origins, 0);
        }
        (order, origins)
    }

    fn collect_relation_property_order(
        &mut self,
        ty: TypeId,
        origin: &PropertyOrderTree,
        order: &mut RelationPropertyOrder,
        origins: &mut HashMap<TypeId, PropertyOrderTree>,
        depth: usize,
    ) {
        if depth > 24 {
            return;
        }
        origins.insert(ty, origin.clone());
        let Some(ty) = self.complete_type(ty) else {
            return;
        };
        origins.insert(ty, origin.clone());
        match origin {
            PropertyOrderTree::Alias { target, .. } => {
                self.collect_relation_property_order(ty, target, order, origins, depth + 1);
            }
            PropertyOrderTree::Object(properties) => {
                let semantic_properties = match self.store.kind(ty).clone() {
                    TypeKind::Object(properties) | TypeKind::ClassInstance { properties, .. } => {
                        properties
                    }
                    _ => return,
                };
                order.insert(
                    ty,
                    properties.iter().map(|(name, _)| name.clone()).collect(),
                );
                for (name, child) in properties {
                    if let Some(property) = semantic_properties
                        .properties
                        .iter()
                        .find(|property| &property.name == name)
                    {
                        self.collect_relation_property_order(
                            property.ty,
                            child,
                            order,
                            origins,
                            depth + 1,
                        );
                    }
                }
            }
            PropertyOrderTree::Array(element_origin) => {
                if let TypeKind::Array(element) = self.store.kind(ty).clone() {
                    self.collect_relation_property_order(
                        element,
                        element_origin,
                        order,
                        origins,
                        depth + 1,
                    );
                }
            }
            PropertyOrderTree::Tuple(element_origins) => {
                if let TypeKind::Tuple(elements) = self.store.kind(ty).clone() {
                    for (element, element_origin) in elements.into_iter().zip(element_origins) {
                        self.collect_relation_property_order(
                            element,
                            element_origin,
                            order,
                            origins,
                            depth + 1,
                        );
                    }
                }
            }
            PropertyOrderTree::Union(member_origins) => {
                if let TypeKind::Union(members) = self.store.kind(ty).clone() {
                    let mut aliases = member_origins
                        .iter()
                        .filter_map(|origin| match origin {
                            PropertyOrderTree::Alias {
                                name, declaration, ..
                            } => Some((name.clone(), *declaration)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if aliases.len() == member_origins.len() {
                        aliases.sort_by(|left, right| left.0.cmp(&right.0));
                        order.insert_union(
                            ty,
                            aliases
                                .into_iter()
                                .map(|(_, declaration)| declaration)
                                .collect(),
                        );
                    }
                    for (index, member) in members.into_iter().enumerate() {
                        let declaration = match self.store.kind(member) {
                            TypeKind::Deferred(
                                crate::semantics::types::DeferredType::Reference {
                                    declaration,
                                    ..
                                },
                            ) => Some(*declaration),
                            _ => None,
                        };
                        let member_origin = declaration
                            .and_then(|declaration| {
                                member_origins.iter().find(|origin| {
                                    matches!(
                                        origin,
                                        PropertyOrderTree::Alias {
                                            declaration: candidate,
                                            ..
                                        } if *candidate == declaration
                                    )
                                })
                            })
                            .or_else(|| member_origins.get(index));
                        let Some(member_origin) = member_origin else {
                            continue;
                        };
                        self.collect_relation_property_order(
                            member,
                            member_origin,
                            order,
                            origins,
                            depth + 1,
                        );
                    }
                }
            }
            PropertyOrderTree::AuthoredTypeName(_) | PropertyOrderTree::Unknown => {}
        }
    }

    /// TypeScript contextually checks every array element. A relation over the
    /// aggregate element union is still useful for assignability, but one
    /// failed union member must not collapse several source-local diagnostics
    /// into a single array-level error.
    fn report_contextual_array_elements(
        &mut self,
        expression: &Expression,
        target: TypeId,
        target_order: Option<&PropertyOrderTree>,
        mode: RelationMode,
    ) -> RelationDiagnosticOutcome {
        let expression = peel_parentheses(expression);
        let ExpressionKind::Array(elements) = &expression.kind else {
            return RelationDiagnosticOutcome::Compatible;
        };
        let Some(target_element) = self.contextual_array_element_type(target) else {
            return RelationDiagnosticOutcome::Compatible;
        };
        let Some(element_types) = elements
            .iter()
            .map(|element| {
                self.expression_type_origins
                    .get(&(element.span.file, element.id))
                    .copied()
            })
            .collect::<Option<Vec<_>>>()
        else {
            return RelationDiagnosticOutcome::Compatible;
        };

        let mut outcome = RelationDiagnosticOutcome::Compatible;
        for (element, source_element) in elements.iter().zip(element_types) {
            let diagnostic_expression = peel_parentheses(element);
            let child = self.report_relation(
                source_element,
                target_element,
                diagnostic_expression.span,
                Some(element),
                target_order.and_then(PropertyOrderTree::element_owned),
                mode,
                RelationDiagnosticStyle::Type,
            );
            outcome = combine_diagnostic_outcomes(outcome, child);
        }
        outcome
    }

    /// An array target supplies its element type as context. A union made
    /// entirely of array types supplies the union of their element types, as
    /// in TypeScript 7's contextual typing of `A[] | B[]`.
    pub(super) fn contextual_array_element_type(&mut self, target: TypeId) -> Option<TypeId> {
        let target = self.complete_type(target)?;
        match self.store.kind(target).clone() {
            TypeKind::Array(element) => Some(element),
            TypeKind::Union(members) => {
                let elements = members
                    .into_iter()
                    .map(|member| {
                        let member = self.complete_type(member)?;
                        match self.store.kind(member) {
                            TypeKind::Array(element) => Some(*element),
                            _ => None,
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(self.store.union(elements, UnionPolicy::Canonical))
            }
            _ => None,
        }
    }

    /// Return a property context from modeled object members. Shared
    /// properties keep the full property union, while source-property
    /// presence or a literal discriminant can select a sole applicable member
    /// for a property that only some members declare.
    /// This mirrors TypeScript's bounded discriminated contextual typing
    /// without guessing through an incomplete member.
    pub(super) fn contextual_object_property_type(
        &mut self,
        target: TypeId,
        name: &str,
        source: Option<&Expression>,
    ) -> ContextualPropertyType {
        let Some(target) = self.complete_type(target) else {
            return ContextualPropertyType::Deferred;
        };
        match self.store.kind(target).clone() {
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => shape
                .properties
                .iter()
                .find(|property| property.name == name)
                .map(|property| ContextualPropertyType::Known(property.ty))
                .or_else(|| {
                    shape
                        .index(IndexKeyKind::String)
                        .map(|index| ContextualPropertyType::Known(index.value))
                })
                .unwrap_or(ContextualPropertyType::Absent),
            TypeKind::Union(members) => {
                let Some(member_properties) = members
                    .into_iter()
                    .map(|member| {
                        let member = self.complete_type(member)?;
                        match self.store.kind(member) {
                            TypeKind::Object(shape)
                            | TypeKind::ClassInstance {
                                properties: shape, ..
                            } if shape.index_signatures.is_empty() => {
                                Some(shape.properties.clone())
                            }
                            _ => None,
                        }
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    return ContextualPropertyType::Deferred;
                };
                let property_is_shared = member_properties
                    .iter()
                    .all(|properties| properties.iter().any(|property| property.name == name));
                let applicable = if property_is_shared {
                    (0..member_properties.len()).collect::<Vec<_>>()
                } else {
                    let Some(source) = source else {
                        return ContextualPropertyType::Deferred;
                    };
                    let Some(applicable) =
                        self.contextually_applicable_members(&member_properties, source)
                    else {
                        return ContextualPropertyType::Deferred;
                    };
                    applicable
                };
                let property_types = applicable
                    .into_iter()
                    .filter_map(|index| {
                        member_properties[index]
                            .iter()
                            .find(|property| property.name == name)
                            .map(|property| property.ty)
                    })
                    .collect::<Vec<_>>();
                if property_types.is_empty() {
                    return ContextualPropertyType::Deferred;
                }
                ContextualPropertyType::Known(
                    self.store.union(property_types, UnionPolicy::Canonical),
                )
            }
            TypeKind::Any => ContextualPropertyType::Known(self.store.builtins.any),
            _ => ContextualPropertyType::Absent,
        }
    }

    fn contextually_applicable_members(
        &mut self,
        members: &[Vec<crate::semantics::types::Property>],
        source: &Expression,
    ) -> Option<Vec<usize>> {
        let ExpressionKind::Object(source_properties) = &peel_parentheses(source).kind else {
            return None;
        };
        let mut applicable = (0..members.len()).collect::<Vec<_>>();
        let mut found_applicability_evidence = false;

        // An authored property that only some union members declare excludes
        // the members that cannot accept it. This is enough to contextually
        // type `{ right: value }` against `{ left: L } | { right: R }`
        // without requiring a separate literal tag.
        for source_property in source_properties {
            let declared = members
                .iter()
                .map(|properties| {
                    properties
                        .iter()
                        .any(|property| property.name == source_property.name)
                })
                .collect::<Vec<_>>();
            if declared.iter().all(|declared| !declared)
                || declared.iter().all(|declared| *declared)
            {
                continue;
            }
            found_applicability_evidence = true;
            applicable.retain(|index| declared[*index]);
            if applicable.is_empty() {
                return None;
            }
        }

        for source_property in source_properties {
            let ExpressionKind::Literal(literal) = &peel_parentheses(&source_property.value).kind
            else {
                continue;
            };
            let Some(target_types) = applicable
                .iter()
                .map(|index| {
                    members[*index]
                        .iter()
                        .find(|property| property.name == source_property.name)
                        .map(|property| property.ty)
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if target_types.windows(2).all(|pair| pair[0] == pair[1]) {
                continue;
            }
            let matches = target_types
                .iter()
                .map(|target| self.literal_matches_type(literal, *target))
                .collect::<Option<Vec<_>>>()?;
            if matches.iter().all(|matches| !matches) {
                continue;
            }
            found_applicability_evidence = true;
            applicable = applicable
                .into_iter()
                .zip(matches)
                .filter_map(|(index, matches)| matches.then_some(index))
                .collect();
            if applicable.is_empty() {
                return None;
            }
        }
        (found_applicability_evidence && applicable.len() == 1).then_some(applicable)
    }

    fn literal_matches_type(&mut self, literal: &Literal, target: TypeId) -> Option<bool> {
        let target = self.complete_type(target)?;
        match (literal, self.store.kind(target).clone()) {
            (Literal::String(left), TypeKind::LiteralString(right, _)) => Some(left == &right),
            (Literal::NoSubstitutionTemplate(left), TypeKind::LiteralString(right, _)) => {
                Some(left.cooked == right)
            }
            (Literal::Number(left), TypeKind::LiteralNumber(right, _)) => {
                let left = self
                    .store
                    .numeric_literal(left, crate::semantics::types::LiteralProvenance::Regular);
                let TypeKind::LiteralNumber(left, _) = self.store.kind(left) else {
                    return None;
                };
                Some(left == &right)
            }
            (Literal::BigInt(_), TypeKind::BigInt) | (Literal::Null, TypeKind::Null) => Some(true),
            (Literal::Boolean(left), TypeKind::LiteralBoolean(right, _)) => Some(*left == right),
            (_, TypeKind::Union(members)) => {
                for member in members {
                    if self.literal_matches_type(literal, member)? {
                        return Some(true);
                    }
                }
                Some(false)
            }
            (
                Literal::String(_)
                | Literal::NoSubstitutionTemplate(_)
                | Literal::Number(_)
                | Literal::BigInt(_)
                | Literal::Boolean(_)
                | Literal::Null,
                TypeKind::LiteralString(_, _)
                | TypeKind::LiteralNumber(_, _)
                | TypeKind::LiteralBoolean(_, _)
                | TypeKind::Null,
            ) => Some(false),
            _ => None,
        }
    }

    fn report_contextual_object_properties(
        &mut self,
        expression: &Expression,
        target: TypeId,
        target_order: Option<&PropertyOrderTree>,
        mode: RelationMode,
    ) -> RelationDiagnosticOutcome {
        let expression = peel_parentheses(expression);
        let ExpressionKind::Object(properties) = &expression.kind else {
            return RelationDiagnosticOutcome::Compatible;
        };
        let mut outcome = RelationDiagnosticOutcome::Compatible;
        for property in properties {
            let target_property = match self.contextual_object_property_type(
                target,
                &property.name,
                Some(expression),
            ) {
                ContextualPropertyType::Known(target_property) => target_property,
                ContextualPropertyType::Absent => continue,
                ContextualPropertyType::Deferred => {
                    outcome =
                        combine_diagnostic_outcomes(outcome, RelationDiagnosticOutcome::Deferred);
                    continue;
                }
            };
            let Some(source_property) = self
                .expression_type_origins
                .get(&(property.value.span.file, property.value.id))
                .copied()
            else {
                continue;
            };
            let child = self.report_relation(
                source_property,
                target_property,
                property.name_span,
                Some(&property.value),
                target_order.and_then(|order| order.property_owned(&property.name)),
                mode,
                RelationDiagnosticStyle::Type,
            );
            outcome = combine_diagnostic_outcomes(outcome, child);
        }
        if matches!(outcome, RelationDiagnosticOutcome::Deferred) {
            self.semantic_completion = self
                .semantic_completion
                .combine(SemanticCompletion::Deferred);
        }
        outcome
    }

    fn relation_continuations(
        &mut self,
        failure: &RelationFailure,
        code: u32,
        source_origins: &HashMap<TypeId, PropertyOrderTree>,
        target_origins: &HashMap<TypeId, PropertyOrderTree>,
    ) -> Vec<RelatedInformation> {
        if failure.kind == RelationFailureKind::UnionMember {
            let source = self.complete_type(failure.source).unwrap_or(failure.source);
            let target = self.complete_type(failure.target).unwrap_or(failure.target);
            let target_is_union = matches!(self.store.kind(target), TypeKind::Union(_));
            let source_is_structured = matches!(
                self.store.kind(source),
                TypeKind::Array(_)
                    | TypeKind::Tuple(_)
                    | TypeKind::Object(_)
                    | TypeKind::ClassInstance { .. }
                    | TypeKind::ClassConstructor { .. }
                    | TypeKind::Function(_)
                    | TypeKind::ShapeFunction(_)
            );
            if target_is_union && !source_is_structured {
                return Vec::new();
            }
        }
        let mut related = Vec::new();
        let mut depth = 1;
        if let RelationFailureKind::ArrayToTupleLength { required } = &failure.kind {
            related.push(RelatedInformation::unlocated(
                format!("Target requires {required} element(s) but source may have fewer."),
                code,
                depth,
            ));
            depth += 1;
        }
        let mut child = failure.child.as_deref();
        while let Some(reason) = child {
            if reason.kind == RelationFailureKind::AliasExpansion {
                let source_origin = self.origin_for(reason.source, source_origins).cloned();
                let target_origin = self.origin_for(reason.target, target_origins).cloned();
                let source_name =
                    self.relation_source_name(reason.source, reason.target, source_origin.as_ref());
                let target_name = self.type_name_with_order(reason.target, target_origin.as_ref());
                related.push(RelatedInformation::unlocated(
                    format!("Type '{source_name}' is not assignable to type '{target_name}'."),
                    code,
                    depth,
                ));
                depth += 1;
                child = reason
                    .child
                    .as_deref()
                    .and_then(|expanded| expanded.child.as_deref());
                continue;
            }
            if reason.kind == RelationFailureKind::Object
                && reason.child.as_deref().is_some_and(|child| {
                    matches!(child.kind, RelationFailureKind::MissingProperties(_))
                })
            {
                child = reason.child.as_deref();
                continue;
            }
            if let RelationFailureKind::Property(first) = &reason.kind {
                let mut names = vec![first.clone()];
                let mut leaf = reason.child.as_deref();
                while let Some(object) = leaf {
                    if object.kind != RelationFailureKind::Object {
                        break;
                    }
                    let Some(property) = object.child.as_deref() else {
                        break;
                    };
                    let RelationFailureKind::Property(name) = &property.kind else {
                        break;
                    };
                    names.push(name.clone());
                    leaf = property.child.as_deref();
                }
                let message = if names.len() == 1 {
                    format!("Types of property '{}' are incompatible.", names[0])
                } else {
                    format!(
                        "The types of '{}' are incompatible between these types.",
                        names.join(".")
                    )
                };
                related.push(RelatedInformation::unlocated(message, code, depth));
                depth += 1;
                child = leaf;
                continue;
            }
            let message = match &reason.kind {
                RelationFailureKind::MissingProperty(name) => {
                    let source_origin = self.origin_for(reason.source, source_origins).cloned();
                    let target_origin = self.origin_for(reason.target, target_origins).cloned();
                    let source_name = self.relation_source_name(
                        reason.source,
                        reason.target,
                        source_origin.as_ref(),
                    );
                    let target_name =
                        self.type_name_with_order(reason.target, target_origin.as_ref());
                    format!(
                        "Property '{name}' is missing in type '{source_name}' but required in type '{target_name}'."
                    )
                }
                RelationFailureKind::MissingProperties(names) => {
                    let source_origin = self.origin_for(reason.source, source_origins).cloned();
                    let target_origin = self.origin_for(reason.target, target_origins).cloned();
                    let source_name = self.relation_source_name(
                        reason.source,
                        reason.target,
                        source_origin.as_ref(),
                    );
                    let target_name =
                        self.type_name_with_order(reason.target, target_origin.as_ref());
                    format!(
                        "Type '{source_name}' is missing the following properties from type '{target_name}': {}",
                        names.join(", ")
                    )
                }
                RelationFailureKind::ArrayToTupleLength { required } => {
                    format!("Target requires {required} element(s) but source may have fewer.")
                }
                _ => {
                    let source_origin = self.origin_for(reason.source, source_origins).cloned();
                    let target_origin = self.origin_for(reason.target, target_origins).cloned();
                    let source_name = self.relation_source_name(
                        reason.source,
                        reason.target,
                        source_origin.as_ref(),
                    );
                    let target_name =
                        self.type_name_with_order(reason.target, target_origin.as_ref());
                    format!("Type '{source_name}' is not assignable to type '{target_name}'.")
                }
            };
            related.push(RelatedInformation::unlocated(message, code, depth));
            depth += 1;
            child = reason.child.as_deref();
        }
        related
    }

    fn origin_for<'a>(
        &mut self,
        ty: TypeId,
        origins: &'a HashMap<TypeId, PropertyOrderTree>,
    ) -> Option<&'a PropertyOrderTree> {
        origins.get(&ty).or_else(|| {
            let complete = self.complete_type(ty)?;
            origins.get(&complete)
        })
    }

    fn relation_source_name(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_order: Option<&PropertyOrderTree>,
    ) -> String {
        let source = self.complete_type(source).unwrap_or(source);
        let target = self.complete_type(target).unwrap_or(target);
        let preserve_literal = self.target_preserves_literal_family(source, target);
        let display = if preserve_literal {
            source
        } else {
            self.widen(source)
        };
        self.display_type_with_property_order(display, source_order, 0)
    }

    fn target_preserves_literal_family(&mut self, source: TypeId, target: TypeId) -> bool {
        let family = match self.store.kind(source) {
            TypeKind::LiteralString(_, _) => 0,
            TypeKind::LiteralNumber(_, _) => 1,
            TypeKind::LiteralBoolean(_, _) => 2,
            _ => return false,
        };
        self.target_contains_literal_family(target, family)
    }

    fn target_contains_literal_family(&mut self, target: TypeId, family: u8) -> bool {
        let target = self.complete_type(target).unwrap_or(target);
        match self.store.kind(target).clone() {
            TypeKind::LiteralString(_, _) => family == 0,
            TypeKind::LiteralNumber(_, _) => family == 1,
            TypeKind::LiteralBoolean(_, _) => family == 2,
            TypeKind::Union(members) => members
                .into_iter()
                .any(|member| self.target_contains_literal_family(member, family)),
            _ => false,
        }
    }

    fn type_name_with_order(&mut self, ty: TypeId, order: Option<&PropertyOrderTree>) -> String {
        let complete = self.complete_type(ty).unwrap_or(ty);
        if let Some(PropertyOrderTree::Union(origins)) = order
            && origins.iter().all(|origin| {
                matches!(
                    origin,
                    PropertyOrderTree::Alias {
                        preserve_name: false,
                        ..
                    }
                )
            })
            && let TypeKind::Union(members) = self.store.kind(complete).clone()
        {
            let completed = members
                .into_iter()
                .map(|member| self.complete_type(member))
                .collect::<Option<Vec<_>>>();
            if let Some(completed) = completed {
                let union = self.store.union(completed, UnionPolicy::Canonical);
                return self.store.display(union);
            }
        }
        self.display_type_with_property_order(complete, order, 0)
    }

    fn push_relation_diagnostic(
        &mut self,
        span: Span,
        message: String,
        code: u32,
        related: Vec<RelatedInformation>,
        reason: RelationFailure,
    ) {
        if !self.reported.insert((
            span.file,
            span.start,
            code,
            super::DiagnosticIdentity::Relation(reason),
        )) {
            return;
        }
        let source = &self.program.files[span.file.0 as usize].source;
        self.diagnostics
            .push(Diagnostic::at(source, span, message, code).with_related_information(related));
    }
}

fn peel_parentheses(mut expression: &Expression) -> &Expression {
    while let ExpressionKind::Parenthesized(inner) = &expression.kind {
        expression = inner;
    }
    expression
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

fn relation_property_name(failure: &RelationFailure) -> Option<&str> {
    let mut reason = Some(failure);
    while let Some(current) = reason {
        if let RelationFailureKind::Property(name) = &current.kind {
            return Some(name);
        }
        reason = current.child.as_deref();
    }
    None
}
use std::collections::HashMap;
