use super::{Checker, DeclarationModel, declaration_value::ValueQueryState};
use crate::diagnostics::{Diagnostic, RelatedInformation};
use crate::program::SemanticCompletion;
use crate::semantics::relation::{
    RelationContext, RelationFailure, RelationFailureKind, RelationMode, relate_types,
};
use crate::semantics::types::{
    Completion, DeferredType, IndexKeyKind, Signature, TypeId, TypeKind, UnionPolicy,
};
use crate::source::{DeclId, Span};
use crate::syntax::{ClassMemberKind, Expression, ExpressionKind, Literal};
#[derive(Clone, Copy)]
pub(super) enum RelationDiagnosticStyle {
    Type,
    Argument,
    Constraint,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RelationDiagnosticOutcome {
    Compatible,
    Deferred,
    Reported,
}
fn combine_diagnostic_outcomes(
    left: RelationDiagnosticOutcome,
    right: RelationDiagnosticOutcome,
) -> RelationDiagnosticOutcome {
    left.max(right)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextualType {
    Known(TypeId),
    Absent,
    Deferred,
}
impl ContextualType {
    pub(super) const fn from_option(expected: Option<TypeId>) -> Self {
        match expected {
            Some(expected) => Self::Known(expected),
            None => Self::Absent,
        }
    }
    pub(super) const fn is_known(self) -> bool {
        matches!(self, Self::Known(_))
    }
}
impl Checker<'_> {
    pub(super) fn report_relation(
        &mut self,
        source: TypeId,
        target: TypeId,
        span: Span,
        source_expression: Option<&Expression>,
        mode: RelationMode,
        style: RelationDiagnosticStyle,
    ) -> RelationDiagnosticOutcome {
        self.report_relation_with_diagnostic_target(
            source,
            target,
            target,
            span,
            source_expression,
            mode,
            style,
        )
    }
    /// Ephemeral authored provenance for an expanded call-shape target.
    pub(super) fn report_relation_with_diagnostic_target(
        &mut self,
        source: TypeId,
        target: TypeId,
        diagnostic_target: TypeId,
        span: Span,
        source_expression: Option<&Expression>,
        mode: RelationMode,
        style: RelationDiagnosticStyle,
    ) -> RelationDiagnosticOutcome {
        if source_expression.is_some_and(expression_is_recovered_number) {
            self.observe_file_completion(span.file, SemanticCompletion::Deferred);
            return RelationDiagnosticOutcome::Deferred;
        }
        let Err(failure) = relate_types(self, source, target, mode) else {
            return RelationDiagnosticOutcome::Compatible;
        };
        if let Some(expression) = source_expression {
            // A request-local projection deliberately has no definitive Ready
            // cache entry. Relation already forced its semantic target, so
            // contextual diagnostics must reuse that owned result instead of
            // treating the absent cache entry as fresh incompleteness.
            let contextual_target = match self.ready_type_for_display(target) {
                Completion::Complete(_) => target,
                Completion::Deferred | Completion::Cycle | Completion::Limit => failure.target,
            };
            for outcome in [
                self.report_contextual_array_elements(expression, contextual_target, mode),
                self.report_contextual_object_properties(expression, contextual_target, mode),
            ] {
                if !matches!(outcome, RelationDiagnosticOutcome::Compatible) {
                    return outcome;
                }
            }
        }
        let failure = self
            .named_union_diagnostic_failure(source, target, mode)
            .unwrap_or(failure);
        self.report_relation_failure(
            (source, target),
            diagnostic_target,
            span,
            source_expression,
            failure,
            style,
        )
    }
    /// Rebuild a failed union reason for stable alias spelling only.
    fn named_union_diagnostic_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        mode: RelationMode,
    ) -> Option<RelationFailure> {
        let TypeKind::Union(members) = self.store.kind(target).clone() else {
            return None;
        };
        let mut named_members = members
            .into_iter()
            .map(|member| {
                let TypeKind::Deferred(DeferredType::Reference { declaration, .. }) =
                    self.store.kind(member)
                else {
                    return None;
                };
                if !self.declaration_preserves_alias_name(*declaration) {
                    return None;
                }
                Some((self.declaration_name(*declaration)?.to_owned(), member))
            })
            .collect::<Option<Vec<_>>>()?;
        named_members.sort_by(|left, right| left.0.cmp(&right.0));
        let member = named_members.first()?.1;
        let Err(selected) = relate_types(self, source, member, mode) else {
            return None;
        };
        let failure_source = selected.source;
        let selected = if selected.target == member {
            selected
        } else {
            RelationFailure {
                source: failure_source,
                target: member,
                kind: RelationFailureKind::AliasExpansion,
                child: Some(Box::new(selected)),
            }
        };
        Some(RelationFailure {
            source: failure_source,
            target,
            kind: RelationFailureKind::UnionMember,
            child: Some(Box::new(selected)),
        })
    }
    pub(super) fn report_constraint_failure(
        &mut self,
        failure: RelationFailure,
        span: Span,
    ) -> RelationDiagnosticOutcome {
        self.report_relation_failure(
            (failure.source, failure.target),
            failure.target,
            span,
            None,
            failure,
            RelationDiagnosticStyle::Constraint,
        )
    }
    fn report_relation_failure(
        &mut self,
        root: (TypeId, TypeId),
        diagnostic_target: TypeId,
        span: Span,
        source_expression: Option<&Expression>,
        failure: RelationFailure,
        style: RelationDiagnosticStyle,
    ) -> RelationDiagnosticOutcome {
        let (source, target) = root;
        match failure.kind {
            RelationFailureKind::Deferred => {
                self.observe_file_completion(span.file, SemanticCompletion::Deferred);
                return RelationDiagnosticOutcome::Deferred;
            }
            RelationFailureKind::Cycle => {
                self.observe_file_completion(span.file, SemanticCompletion::Cycle);
                return RelationDiagnosticOutcome::Deferred;
            }
            RelationFailureKind::InvalidProjection => {
                return RelationDiagnosticOutcome::Deferred;
            }
            RelationFailureKind::ComplexityLimit => {
                self.observe_file_completion(span.file, SemanticCompletion::Limit);
                self.push_diagnostic(
                    span.file,
                    span,
                    "Type instantiation is excessively deep and possibly infinite.".to_string(),
                    2589,
                );
                return RelationDiagnosticOutcome::Reported;
            }
            RelationFailureKind::Incompatible
            | RelationFailureKind::SignatureArityMismatch { .. }
            | RelationFailureKind::MissingProperty(_)
            | RelationFailureKind::MissingProperties(_)
            | RelationFailureKind::Property(_)
            | RelationFailureKind::Object
            | RelationFailureKind::ArrayElement
            | RelationFailureKind::TupleElement(_)
            | RelationFailureKind::TypeArgument(_)
            | RelationFailureKind::ArrayToTupleLength { .. }
            | RelationFailureKind::UnionMember
            | RelationFailureKind::AliasExpansion
            | RelationFailureKind::Parameter(_)
            | RelationFailureKind::Return => {}
        }
        let diagnostic_span = source_expression
            .and_then(|expression| relation_property_span(&failure, expression))
            .unwrap_or(span);
        let primary = &failure;
        let source_name = if primary.source != source
            && let Some(alias) = self.root_source_alias(source, primary.source)
        {
            Completion::Complete(alias)
        } else {
            self.source_name(primary.source, primary.target)
        };
        let target_name = self.root_target_name(diagnostic_target, target, primary.target);
        let (Completion::Complete(source_name), Completion::Complete(target_name)) = (
            self.require_file_completion(span.file, source_name),
            self.require_file_completion(span.file, target_name),
        ) else {
            return RelationDiagnosticOutcome::Deferred;
        };
        let (diagnostic_code, message) = match style {
            RelationDiagnosticStyle::Type if source_name == target_name => (
                2719,
                format!(
                    "Type '{source_name}' is not assignable to type '{target_name}'. Two different types with this name exist, but they are unrelated."
                ),
            ),
            RelationDiagnosticStyle::Type => (
                2322,
                format!("Type '{source_name}' is not assignable to type '{target_name}'."),
            ),
            RelationDiagnosticStyle::Argument => (
                2345,
                format!(
                    "Argument of type '{source_name}' is not assignable to parameter of type '{target_name}'."
                ),
            ),
            RelationDiagnosticStyle::Constraint => (
                2344,
                format!("Type '{source_name}' does not satisfy the constraint '{target_name}'."),
            ),
        };
        let related = match style {
            RelationDiagnosticStyle::Constraint => {
                self.constraint_continuations(primary, diagnostic_code)
            }
            RelationDiagnosticStyle::Type | RelationDiagnosticStyle::Argument => {
                self.relation_continuations(primary, diagnostic_code)
            }
        };
        let Completion::Complete(related) = self.require_file_completion(span.file, related) else {
            return RelationDiagnosticOutcome::Deferred;
        };
        if self.semantic_diagnostic_is_enabled(diagnostic_span.file) {
            // Program boundary deduplicates the complete public diagnostic identity.
            let source = &self.program.files[diagnostic_span.file.0 as usize].source;
            self.diagnostics.push(
                Diagnostic::at(source, diagnostic_span, message, diagnostic_code)
                    .with_related_information(related),
            );
        }
        RelationDiagnosticOutcome::Reported
    }
    /// Preserve each contextually checked array element's source-local failure.
    fn report_contextual_array_elements(
        &mut self,
        expression: &Expression,
        target: TypeId,
        mode: RelationMode,
    ) -> RelationDiagnosticOutcome {
        let expression = expression.peel_parentheses();
        let ExpressionKind::Array(elements) = &expression.kind else {
            return RelationDiagnosticOutcome::Compatible;
        };
        let target_element = match self.contextual_array_element_type(target) {
            ContextualType::Known(target_element) => target_element,
            ContextualType::Absent => return RelationDiagnosticOutcome::Compatible,
            ContextualType::Deferred => return RelationDiagnosticOutcome::Deferred,
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
            let diagnostic_expression = element.peel_parentheses();
            let child = self.report_relation(
                source_element,
                target_element,
                diagnostic_expression.span,
                Some(element),
                mode,
                RelationDiagnosticStyle::Type,
            );
            outcome = combine_diagnostic_outcomes(outcome, child);
        }
        outcome
    }
    /// An array union context supplies the union of its element types.
    pub(super) fn contextual_array_element_type(&mut self, target: TypeId) -> ContextualType {
        let Completion::Complete(target) = self.ready_type_for_display(target) else {
            return ContextualType::Deferred;
        };
        match self.store.kind(target).clone() {
            TypeKind::Array(element) => ContextualType::Known(element),
            TypeKind::Union(members) => {
                let Some(elements) = self.complete_union_members(members, |kind| match kind {
                    TypeKind::Array(element) => Some(*element),
                    _ => None,
                }) else {
                    return ContextualType::Deferred;
                };
                ContextualType::Known(self.store.union(elements, UnionPolicy::Canonical))
            }
            _ => ContextualType::Absent,
        }
    }
    /// Shared properties keep their union; structural or literal evidence may
    /// select one member without guessing through an incomplete member.
    pub(super) fn contextual_object_property_type(
        &mut self,
        target: TypeId,
        name: &str,
        source: Option<&Expression>,
    ) -> ContextualType {
        let Completion::Complete(target) = self.ready_type_for_display(target) else {
            return ContextualType::Deferred;
        };
        match self.store.kind(target).clone() {
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => shape
                .properties
                .iter()
                .find(|property| property.name == name)
                .map(|property| ContextualType::Known(property.ty))
                .or_else(|| {
                    shape
                        .index(IndexKeyKind::String)
                        .map(|index| ContextualType::Known(index.value))
                })
                .unwrap_or(ContextualType::Absent),
            TypeKind::Union(members) => {
                let Some(member_properties) =
                    self.complete_union_members(members, |kind| match kind {
                        TypeKind::Object(shape)
                        | TypeKind::ClassInstance {
                            properties: shape, ..
                        } if shape.index_signatures.is_empty() => Some(shape.properties.clone()),
                        _ => None,
                    })
                else {
                    return ContextualType::Deferred;
                };
                let property_is_shared = member_properties
                    .iter()
                    .all(|properties| properties.iter().any(|property| property.name == name));
                let applicable = if property_is_shared {
                    (0..member_properties.len()).collect::<Vec<_>>()
                } else {
                    let Some(source) = source else {
                        return ContextualType::Deferred;
                    };
                    let Some(applicable) =
                        self.contextually_applicable_members(&member_properties, source)
                    else {
                        return ContextualType::Deferred;
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
                    return ContextualType::Deferred;
                }
                ContextualType::Known(self.store.union(property_types, UnionPolicy::Canonical))
            }
            TypeKind::Any => ContextualType::Known(self.store.builtins.any),
            _ => ContextualType::Absent,
        }
    }
    fn complete_union_members<T>(
        &mut self,
        members: Vec<TypeId>,
        select: impl Fn(&TypeKind) -> Option<T>,
    ) -> Option<Vec<T>> {
        members
            .into_iter()
            .map(|member| {
                let Completion::Complete(member) = self.ready_type_for_display(member) else {
                    return None;
                };
                select(self.store.kind(member))
            })
            .collect()
    }
    fn contextually_applicable_members(
        &mut self,
        members: &[Vec<crate::semantics::types::Property>],
        source: &Expression,
    ) -> Option<Vec<usize>> {
        let ExpressionKind::Object(source_properties) = &source.peel_parentheses().kind else {
            return None;
        };
        let mut applicable = (0..members.len()).collect::<Vec<_>>();
        let mut found_applicability_evidence = false;
        // A uniquely declared source property excludes the other members.
        for source_property in source_properties {
            let declared = members
                .iter()
                .map(|properties| {
                    properties
                        .iter()
                        .any(|property| property.name == source_property.name)
                })
                .collect::<Vec<_>>();
            if declared.windows(2).all(|pair| pair[0] == pair[1]) {
                continue;
            }
            found_applicability_evidence = true;
            applicable.retain(|index| declared[*index]);
            if applicable.is_empty() {
                return None;
            }
        }
        for source_property in source_properties {
            let ExpressionKind::Literal(literal) = &source_property.value.peel_parentheses().kind
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
        let Completion::Complete(target) = self.ready_type_for_display(target) else {
            return None;
        };
        match (literal, self.store.kind(target).clone()) {
            (
                Literal::String(crate::syntax::StringLiteral::Plain(left)),
                TypeKind::LiteralString(right, _),
            ) => Some(left == &right),
            (
                Literal::String(crate::syntax::StringLiteral::Extended(_)),
                TypeKind::LiteralString(_, _),
            ) => None,
            (Literal::NoSubstitutionTemplate(left), TypeKind::LiteralString(right, _)) => {
                Some(left.cooked == right)
            }
            (Literal::Number(left), TypeKind::LiteralNumber(right, _)) => {
                if matches!(left, crate::syntax::NumberLiteral::Recovery(_)) {
                    return None;
                }
                let left = self.store.numeric_literal(
                    left.semantic_text(),
                    crate::semantics::types::LiteralProvenance::Regular,
                );
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
        mode: RelationMode,
    ) -> RelationDiagnosticOutcome {
        let expression = expression.peel_parentheses();
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
                ContextualType::Known(target_property) => target_property,
                ContextualType::Absent => continue,
                ContextualType::Deferred => {
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
                mode,
                RelationDiagnosticStyle::Type,
            );
            outcome = combine_diagnostic_outcomes(outcome, child);
        }
        if matches!(outcome, RelationDiagnosticOutcome::Deferred) {
            self.observe_completion(SemanticCompletion::Deferred);
        }
        outcome
    }
    fn relation_continuations(
        &mut self,
        failure: &RelationFailure,
        code: u32,
    ) -> Completion<Vec<RelatedInformation>> {
        if failure.kind == RelationFailureKind::UnionMember {
            let source = completed!(self.ready_type_for_display(failure.source));
            let target = completed!(self.ready_type_for_display(failure.target));
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
                return Completion::Complete(Vec::new());
            }
        }
        let mut related = Vec::new();
        let mut current = if matches!(failure.kind, RelationFailureKind::ArrayToTupleLength { .. })
        {
            Some(failure)
        } else {
            failure.child.as_deref()
        };
        while let Some(reason) = current {
            current = reason.child.as_deref();
            let message = match &reason.kind {
                RelationFailureKind::AliasExpansion => {
                    current = current.and_then(|expanded| expanded.child.as_deref());
                    let source = completed!(self.source_name(reason.source, reason.target));
                    let target = completed!(self.type_name(reason.target));
                    format!("Type '{source}' is not assignable to type '{target}'.")
                }
                RelationFailureKind::Object
                    if current.as_ref().is_some_and(|child| {
                        matches!(child.kind, RelationFailureKind::MissingProperties(_))
                    }) =>
                {
                    continue;
                }
                RelationFailureKind::Property(first) => {
                    let mut names = vec![first.clone()];
                    while let Some(object) = current {
                        let (RelationFailureKind::Object, Some(property)) =
                            (&object.kind, object.child.as_deref())
                        else {
                            break;
                        };
                        let RelationFailureKind::Property(name) = &property.kind else {
                            break;
                        };
                        names.push(name.clone());
                        current = property.child.as_deref();
                    }
                    if names.len() == 1 {
                        format!("Types of property '{}' are incompatible.", names[0])
                    } else {
                        format!(
                            "The types of '{}' are incompatible between these types.",
                            names.join(".")
                        )
                    }
                }
                RelationFailureKind::ArrayToTupleLength { required } => {
                    format!("Target requires {required} element(s) but source may have fewer.")
                }
                RelationFailureKind::SignatureArityMismatch {
                    source_minimum,
                    target_parameter_count,
                } => format!(
                    "Target signature provides too few arguments. Expected {source_minimum} or more, but got {target_parameter_count}."
                ),
                kind => {
                    let source = completed!(self.source_name(reason.source, reason.target));
                    let target = completed!(self.type_name(reason.target));
                    match kind {
                        RelationFailureKind::MissingProperty(name) => format!(
                            "Property '{name}' is missing in type '{source}' but required in type '{target}'."
                        ),
                        RelationFailureKind::MissingProperties(names) => format!(
                            "Type '{source}' is missing the following properties from type '{target}': {}",
                            names.join(", ")
                        ),
                        _ => format!("Type '{source}' is not assignable to type '{target}'."),
                    }
                }
            };
            related.push(RelatedInformation::unlocated(
                message,
                code,
                related.len() as u32 + 1,
            ));
        }
        Completion::Complete(related)
    }
    fn constraint_continuations(
        &mut self,
        failure: &RelationFailure,
        code: u32,
    ) -> Completion<Vec<RelatedInformation>> {
        let source = completed!(self.ready_type_for_display(failure.source));
        if !matches!(
            self.store.kind(source),
            TypeKind::TypeParameter { .. } | TypeKind::Union(_)
        ) {
            return Completion::Complete(Vec::new());
        }
        let target_name = completed!(self.type_name(failure.target));
        let mut previous = failure.source;
        let mut child = failure.child.as_deref();
        let mut related = Vec::new();
        while let Some(reason) = child {
            let source = completed!(self.ready_type_for_display(reason.source));
            if matches!(self.store.kind(source), TypeKind::Unknown) {
                break;
            }
            if source != previous {
                let source_name = completed!(self.source_name(reason.source, failure.target));
                related.push(RelatedInformation::unlocated(
                    format!("Type '{source_name}' is not assignable to type '{target_name}'."),
                    code,
                    related.len() as u32 + 1,
                ));
                previous = source;
            }
            child = reason.child.as_deref();
        }
        Completion::Complete(related)
    }
    fn source_name(&mut self, source: TypeId, target: TypeId) -> Completion<String> {
        if matches!(
            self.store.kind(source),
            TypeKind::Deferred(DeferredType::Reference { .. })
        ) && let Completion::Complete(display) = self.display_type_for_diagnostic(source)
        {
            return Completion::Complete(display);
        }
        let source = completed!(self.ready_type_for_display(source));
        let preserve_literal = if matches!(
            self.store.kind(source),
            TypeKind::LiteralString(_, _)
                | TypeKind::LiteralNumber(_, _)
                | TypeKind::LiteralBoolean(_, _)
        ) {
            let target = completed!(self.ready_type_for_display(target));
            self.target_preserves_literal_family(source, target)
        } else {
            false
        };
        let display = if preserve_literal {
            source
        } else {
            self.widen(source)
        };
        self.display_type_for_diagnostic(display)
    }
    /// Display-only alias recovery; never force, cache, or affect membership.
    fn root_source_alias(&mut self, root: TypeId, narrowed: TypeId) -> Option<String> {
        let (declaration, declared, flow) = match self.store.kind(root).clone() {
            TypeKind::Deferred(DeferredType::FlowReference {
                declaration,
                declared,
                ..
            }) => (declaration, declared, true),
            TypeKind::Deferred(DeferredType::Value(declaration)) => (declaration, root, false),
            _ => return None,
        };
        let authored = self.authored_value_type(declaration, declared);
        if matches!(
            self.store.kind(authored),
            TypeKind::Deferred(DeferredType::Reference { .. })
        ) && (!flow
            || authored == narrowed
            || matches!(self.ready_type_for_display(authored), Completion::Complete(ready) if ready == narrowed))
        {
            return match self.display_type_for_diagnostic(authored) {
                Completion::Complete(name) => Some(name),
                Completion::Deferred | Completion::Cycle | Completion::Limit => None,
            };
        }
        if !flow {
            return None;
        }
        let TypeKind::Union(authored) = self.store.kind(authored) else {
            return None;
        };
        let live = match self.store.kind(narrowed) {
            TypeKind::Union(members) => members.as_slice(),
            _ => std::slice::from_ref(&narrowed),
        };
        let mut names = Vec::with_capacity(live.len());
        for live_member in live {
            let mut matches = authored.iter().filter(|candidate| {
                matches!(
                    self.ready_type_for_display(**candidate),
                    Completion::Complete(ready) if ready == *live_member
                )
            });
            let candidate = *matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            let Completion::Complete(name) = self.display_type_for_diagnostic(candidate) else {
                return None;
            };
            names.push(name);
        }
        (names.len() == live.len()).then(|| names.join(" | "))
    }
    fn authored_value_type(
        &mut self,
        declaration: crate::source::DeclId,
        fallback: TypeId,
    ) -> TypeId {
        let authored = self
            .parameter_type_overrides
            .get(&declaration)
            .copied()
            .or_else(|| match self.models.get(&declaration).copied() {
                Some(DeclarationModel::Variable {
                    declaration: variable,
                    scope,
                    ..
                }) => variable.annotation.as_ref().map(|annotation| {
                    self.resolve_type_node(declaration.file, scope, annotation, &Default::default())
                }),
                Some(DeclarationModel::Parameter { parameter, scope }) => {
                    match self.parameter_value_type(declaration.file, scope, parameter) {
                        Completion::Complete(ty) => Some(ty),
                        Completion::Deferred | Completion::Cycle | Completion::Limit => None,
                    }
                }
                _ => None,
            })
            .or_else(|| match self.value_queries.get(&declaration).copied() {
                Some(ValueQueryState::Ready(value)) => Some(value),
                Some(ValueQueryState::Provisional | ValueQueryState::Computing) | None => None,
            });
        authored.unwrap_or(fallback)
    }
    /// Prefer authored spelling only for the exact relation-failure root.
    fn root_target_name(
        &mut self,
        diagnostic_root: TypeId,
        semantic_root: TypeId,
        failure_target: TypeId,
    ) -> Completion<String> {
        let matches_failure = diagnostic_root == semantic_root
            || semantic_root == failure_target
            || matches!(
                self.ready_type_for_display(semantic_root),
                Completion::Complete(ready) if ready == failure_target
            );
        if matches_failure
            && let Completion::Complete(display) = self.display_type_for_diagnostic(diagnostic_root)
        {
            return Completion::Complete(display);
        }
        self.type_name(failure_target)
    }
    fn target_preserves_literal_family(&self, source: TypeId, target: TypeId) -> bool {
        let family = match self.store.kind(source) {
            TypeKind::LiteralString(_, _) => 0,
            TypeKind::LiteralNumber(_, _) => 1,
            TypeKind::LiteralBoolean(_, _) => 2,
            _ => return false,
        };
        self.target_contains_literal_family(target, family)
    }
    fn target_contains_literal_family(&self, target: TypeId, family: u8) -> bool {
        match self.store.kind(target) {
            TypeKind::LiteralString(_, _) => family == 0,
            TypeKind::LiteralNumber(_, _) => family == 1,
            TypeKind::LiteralBoolean(_, _) => family == 2,
            TypeKind::Union(members) => members
                .iter()
                .any(|member| self.target_contains_literal_family(*member, family)),
            _ => false,
        }
    }
    fn type_name(&mut self, ty: TypeId) -> Completion<String> {
        if let Completion::Complete(display) = self.display_type_for_diagnostic(ty) {
            return Completion::Complete(display);
        }
        let ty = completed!(self.ready_type_for_display(ty));
        self.display_type_for_diagnostic(ty)
    }
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
    fn generative_reference_supported(&self, declaration: DeclId, arguments: &[TypeId]) -> bool {
        Checker::generative_reference_supported(self, declaration, arguments)
    }
    fn generative_relation_frame_supported(
        &self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> bool {
        Checker::generative_relation_frame_supported(self, declaration, arguments)
    }
    fn library_reference_arguments_are_covariant(&self, declaration: DeclId) -> bool {
        self.program.standard_library.is_map_type(declaration)
            && !self
                .program
                .standard_library_type_has_authored_declarations(declaration)
    }
    fn class_constructor_signature(&mut self, declaration: DeclId) -> Completion<Signature> {
        let Some(DeclarationModel::Class {
            declaration: class, ..
        }) = self.models.get(&declaration).copied()
        else {
            return Completion::Deferred;
        };
        if !class.member_syntax_recovery_free {
            return Completion::Deferred;
        }
        match class.members.as_slice() {
            [] if class.extends.is_none() => Completion::Complete(Signature {
                generic_declaration: None,
                untyped_javascript: false,
                parameters: Vec::new(),
                return_type: self.store.builtins.void,
            }),
            [member]
                if member.overload_context_is_recovery_free()
                    && member.modifiers.constructor_modifiers_are_modeled()
                    && matches!(
                        &member.kind,
                        ClassMemberKind::Constructor { type_parameters, return_type, has_body: true, .. }
                            if type_parameters.is_empty() && return_type.is_none()
                    ) =>
            {
                self.class_member_overload_signature(
                    declaration.file,
                    member,
                    self.store.builtins.void,
                )
            }
            _ => Completion::Deferred,
        }
    }
    fn strict_null_checks(&self) -> bool {
        self.options.effective_strict_null_checks()
    }
    fn canonical_union(&mut self, members: &[TypeId]) -> TypeId {
        self.store
            .union(members.iter().copied(), UnionPolicy::Canonical)
    }
}
fn expression_is_recovered_number(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Literal(Literal::Number(crate::syntax::NumberLiteral::Recovery(_))) => true,
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Parenthesized(operand)
        | ExpressionKind::As {
            expression: operand,
            ..
        } => expression_is_recovered_number(operand),
        _ => false,
    }
}
fn relation_property_span(failure: &RelationFailure, expression: &Expression) -> Option<Span> {
    let ExpressionKind::Object(properties) = &expression.kind else {
        return None;
    };
    std::iter::successors(Some(failure), |reason| reason.child.as_deref()).find_map(|reason| {
        match &reason.kind {
            RelationFailureKind::Property(name) => properties
                .iter()
                .find(|property| &property.name == name)
                .map(|property| property.name_span),
            _ => None,
        }
    })
}
