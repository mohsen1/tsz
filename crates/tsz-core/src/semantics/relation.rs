use std::collections::{HashMap, HashSet};

use super::checker::recursion::{ReferenceDemand, ReferenceExpansionStack};
use super::types::{Completion, DeferredType, Property, ShapeSignature, TypeId, TypeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationMode {
    Subtype,
    Assignment,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationFailureKind {
    Incompatible,
    MissingProperty(String),
    MissingProperties(Vec<String>),
    Property(String),
    Object,
    ArrayElement,
    TupleElement(usize),
    ArrayToTupleLength { required: usize },
    UnionMember,
    AliasExpansion,
    Parameter(usize),
    Return,
    InvalidProjection,
    Cycle,
    ComplexityLimit,
    Deferred,
}

impl RelationFailureKind {
    const fn propagates_unchanged(&self) -> bool {
        matches!(
            self,
            Self::InvalidProjection | Self::Cycle | Self::ComplexityLimit | Self::Deferred
        )
    }

    /// Priority for incomplete failures observed while trying alternatives.
    /// An invalid projection already owns a concrete diagnostic elsewhere;
    /// it must not hide a semantic nonclaim. The completion verdict follows
    /// the public deterministic dominance `Deferred < Cycle < Limit`.
    const fn propagation_priority(&self) -> u8 {
        match self {
            Self::Deferred => 1,
            Self::Cycle => 2,
            Self::ComplexityLimit => 3,
            Self::InvalidProjection
            | Self::Incompatible
            | Self::MissingProperty(_)
            | Self::MissingProperties(_)
            | Self::Property(_)
            | Self::Object
            | Self::ArrayElement
            | Self::TupleElement(_)
            | Self::ArrayToTupleLength { .. }
            | Self::UnionMember
            | Self::AliasExpansion
            | Self::Parameter(_)
            | Self::Return => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelationFailure {
    pub source: TypeId,
    pub target: TypeId,
    pub kind: RelationFailureKind,
    pub child: Option<Box<RelationFailure>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RelationPropertyOrder {
    orders: HashMap<TypeId, Vec<String>>,
    union_orders: HashMap<TypeId, Vec<crate::source::DeclId>>,
}

impl RelationPropertyOrder {
    pub(crate) fn insert(&mut self, ty: TypeId, names: Vec<String>) {
        self.orders.insert(ty, names);
    }

    fn get(&self, ty: TypeId) -> Option<&[String]> {
        self.orders.get(&ty).map(Vec::as_slice)
    }

    pub(crate) fn insert_union(&mut self, ty: TypeId, declarations: Vec<crate::source::DeclId>) {
        self.union_orders.insert(ty, declarations);
    }

    fn union_members<C: RelationContext>(
        &self,
        ty: TypeId,
        members: &[TypeId],
        context: &C,
    ) -> Vec<TypeId> {
        let Some(declarations) = self.union_orders.get(&ty) else {
            return members.to_vec();
        };
        let mut ordered = Vec::with_capacity(members.len());
        for declaration in declarations {
            if let Some(member) = members.iter().find(|member| {
                matches!(
                    context.type_kind(**member),
                    TypeKind::Deferred(super::types::DeferredType::Reference {
                        declaration: candidate,
                        ..
                    }) if candidate == *declaration
                )
            }) {
                ordered.push(*member);
            }
        }
        for member in members {
            if !ordered.contains(member) {
                ordered.push(*member);
            }
        }
        ordered
    }
}

pub(crate) trait RelationContext {
    fn force_type(&mut self, ty: TypeId, depth: usize) -> Completion<TypeId>;
    fn type_kind(&self, ty: TypeId) -> TypeKind;
    fn generative_reference_supported(
        &self,
        declaration: crate::source::DeclId,
        arguments: &[TypeId],
    ) -> bool;
    fn generative_relation_frame_supported(
        &self,
        declaration: crate::source::DeclId,
        arguments: &[TypeId],
    ) -> bool;
    fn strict_null_checks(&self) -> bool;
    fn canonical_union(&mut self, members: &[TypeId]) -> TypeId;
}

/// The active deferred-evaluator depth at a relation entry.
///
/// Relation recursion has its own structural depth; forcing must preserve this
/// seed instead of converting structural nesting into evaluator fuel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EvaluationDepth(usize);

impl EvaluationDepth {
    pub(crate) const ROOT: Self = Self(0);

    pub(crate) const fn from_active_depth(depth: usize) -> Self {
        Self(depth)
    }
}

pub(crate) fn relate_with_property_order<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
    property_order: RelationPropertyOrder,
) -> Result<(), RelationFailure> {
    relate_with_property_order_at_evaluation_depth(
        context,
        source,
        target,
        mode,
        property_order,
        EvaluationDepth::ROOT,
    )
}

pub(crate) fn relate_with_property_order_at_evaluation_depth<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
    property_order: RelationPropertyOrder,
    evaluation_depth: EvaluationDepth,
) -> Result<(), RelationFailure> {
    Relation {
        context,
        active: HashSet::new(),
        source_references: ReferenceExpansionStack::new(ReferenceDemand::RelationSource),
        target_references: ReferenceExpansionStack::new(ReferenceDemand::RelationTarget),
        property_order,
        evaluation_depth,
    }
    .relate_inner(source, target, mode, 0)
}

struct Relation<'a, C> {
    context: &'a mut C,
    active: HashSet<RelationQueryKey>,
    source_references: ReferenceExpansionStack,
    target_references: ReferenceExpansionStack,
    property_order: RelationPropertyOrder,
    evaluation_depth: EvaluationDepth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RelationQueryKey {
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
}

impl<C: RelationContext> Relation<'_, C> {
    fn relate_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        if source == target {
            return Ok(());
        }
        // Relation nesting is a query-local structural budget, distinct from
        // the evaluator-expansion budget owned by deferred forcing.
        if depth > 100 {
            return Err(failure(
                source,
                target,
                RelationFailureKind::ComplexityLimit,
            ));
        }

        let key = RelationQueryKey {
            source,
            target,
            mode,
        };
        if !self.active.insert(key) {
            return Ok(());
        }
        let result = self.relate_with_reference_stacks(source, target, mode, depth);
        self.active.remove(&key);
        result
    }

    fn relate_with_reference_stacks(
        &mut self,
        source: TypeId,
        target: TypeId,
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        let source_reference = deferred_reference(self.context.type_kind(source));
        let target_reference = deferred_reference(self.context.type_kind(target));
        let source_expansion = source_reference
            .as_ref()
            .and_then(|(declaration, arguments)| {
                self.source_references.generative_expansion(
                    source,
                    *declaration,
                    arguments,
                    &|ty| self.context.type_kind(ty),
                )
            });
        let target_expansion = target_reference
            .as_ref()
            .and_then(|(declaration, arguments)| {
                self.target_references.generative_expansion(
                    target,
                    *declaration,
                    arguments,
                    &|ty| self.context.type_kind(ty),
                )
            });
        if source_expansion.is_some() && target_expansion.is_some() {
            let supported = match (
                &source_reference,
                &target_reference,
                &source_expansion,
                &target_expansion,
            ) {
                (
                    Some((source_declaration, source_arguments)),
                    Some((target_declaration, target_arguments)),
                    Some(source_expansion),
                    Some(target_expansion),
                ) => {
                    source_declaration == target_declaration
                        && self
                            .context
                            .generative_reference_supported(*source_declaration, source_arguments)
                        && self
                            .context
                            .generative_reference_supported(*target_declaration, target_arguments)
                        && self.source_references.expansion_segment_supports(
                            source_expansion,
                            |frame_declaration, frame_arguments| {
                                frame_declaration == *source_declaration
                                    && self.context.generative_relation_frame_supported(
                                        frame_declaration,
                                        frame_arguments,
                                    )
                            },
                        )
                        && self.target_references.expansion_segment_supports(
                            target_expansion,
                            |frame_declaration, frame_arguments| {
                                frame_declaration == *target_declaration
                                    && self.context.generative_relation_frame_supported(
                                        frame_declaration,
                                        frame_arguments,
                                    )
                            },
                        )
                        && source_expansion.same_supported_transform(target_expansion)
                }
                _ => false,
            };
            if !supported {
                return Err(failure(source, target, RelationFailureKind::Deferred));
            }
            // Pinned TS7's `recursiveTypeRelatedTo` returns a provisional
            // `Maybe` only when both sides expand through the same supported
            // origin and positional transform. This query has no persistent
            // result cache, so the assumption cannot escape the root.
            return Ok(());
        }

        let source_checkpoint = self.source_references.checkpoint();
        let target_checkpoint = self.target_references.checkpoint();
        if let Some((declaration, arguments)) = &source_reference {
            self.source_references.push(source, *declaration, arguments);
        }
        if let Some((declaration, arguments)) = &target_reference {
            self.target_references.push(target, *declaration, arguments);
        }
        let result = self.relate_active(source, target, mode, depth);
        self.source_references.restore(source_checkpoint);
        self.target_references.restore(target_checkpoint);
        result
    }

    fn relate_active(
        &mut self,
        source: TypeId,
        target: TypeId,
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        let forced_source = self.force(source, source, target)?;
        let forced_target = self.force(target, source, target)?;
        if forced_source != source || forced_target != target {
            return self.relate_inner(forced_source, forced_target, mode, depth + 1);
        }

        let source_kind = self.context.type_kind(source);
        let target_kind = self.context.type_kind(target);
        if matches!(source_kind, TypeKind::Invalid(_))
            || matches!(target_kind, TypeKind::Invalid(_))
        {
            return Err(failure(
                source,
                target,
                RelationFailureKind::InvalidProjection,
            ));
        }
        if matches!(source_kind, TypeKind::Error) || matches!(target_kind, TypeKind::Error) {
            return Ok(());
        }
        if matches!(source_kind, TypeKind::Never) || matches!(target_kind, TypeKind::Unknown) {
            return Ok(());
        }
        if mode == RelationMode::Assignment
            && (matches!(source_kind, TypeKind::Any) || matches!(target_kind, TypeKind::Any))
        {
            return Ok(());
        }
        if !self.context.strict_null_checks()
            && matches!(source_kind, TypeKind::Null | TypeKind::Undefined)
        {
            return Ok(());
        }
        if let (Some(source_shape), Some(target_shape)) =
            (object_shape(&source_kind), object_shape(&target_kind))
        {
            return self.relate_object_shapes(
                source,
                target,
                source_shape,
                target_shape,
                mode,
                depth,
            );
        }

        match (&source_kind, &target_kind) {
            (TypeKind::LiteralString(_, _), TypeKind::String)
            | (TypeKind::LiteralNumber(_, _), TypeKind::Number)
            | (TypeKind::LiteralBoolean(_, _), TypeKind::Boolean)
            | (
                TypeKind::Object(_)
                | TypeKind::ClassInstance { .. }
                | TypeKind::ClassConstructor { .. }
                | TypeKind::Array(_)
                | TypeKind::Tuple(_)
                | TypeKind::Function(_)
                | TypeKind::ShapeFunction(_),
                TypeKind::ObjectKeyword,
            ) => Ok(()),
            (TypeKind::LiteralString(left, _), TypeKind::LiteralString(right, _))
                if left == right =>
            {
                Ok(())
            }
            (TypeKind::LiteralNumber(left, _), TypeKind::LiteralNumber(right, _))
                if left == right =>
            {
                Ok(())
            }
            (TypeKind::LiteralBoolean(left, _), TypeKind::LiteralBoolean(right, _))
                if left == right =>
            {
                Ok(())
            }
            (TypeKind::Union(members), _) => {
                let mut relation_members = members.clone();
                relation_members.sort_by_key(|member| match self.context.type_kind(*member) {
                    TypeKind::Undefined => 0,
                    TypeKind::Null => 1,
                    _ => 2,
                });
                for member in &relation_members {
                    if let Err(error) = self.relate_inner(*member, target, mode, depth + 1) {
                        return Err(wrap_failure(
                            source,
                            target,
                            RelationFailureKind::UnionMember,
                            error,
                        ));
                    }
                }
                Ok(())
            }
            (_, TypeKind::Union(members)) => {
                let members = self
                    .property_order
                    .union_members(target, members, self.context);
                self.relate_to_alternative(source, target, &members, mode, depth)
            }
            (_, TypeKind::Intersection(members)) => {
                for member in members {
                    self.relate_inner(source, *member, mode, depth + 1)?;
                }
                Ok(())
            }
            (TypeKind::Intersection(members), _) => {
                self.relate_from_alternative(source, target, members, mode, depth)
            }
            (TypeKind::Array(left), TypeKind::Array(right)) => self
                .relate_inner(*left, *right, mode, depth + 1)
                .map_err(|error| {
                    wrap_failure(source, target, RelationFailureKind::ArrayElement, error)
                }),
            (TypeKind::Array(_), TypeKind::Object(shape))
                if shape.properties.is_empty()
                    && shape.call_signatures.is_empty()
                    && shape.construct_signatures.is_empty()
                    && shape.index_signatures.is_empty() =>
            {
                Ok(())
            }
            (TypeKind::Tuple(left), TypeKind::Tuple(right)) if left.len() == right.len() => {
                for (index, (left_element, right_element)) in left.iter().zip(right).enumerate() {
                    if let Err(error) =
                        self.relate_inner(*left_element, *right_element, mode, depth + 1)
                    {
                        return Err(wrap_failure(
                            source,
                            target,
                            RelationFailureKind::TupleElement(index),
                            error,
                        ));
                    }
                }
                Ok(())
            }
            (TypeKind::Tuple(elements), TypeKind::Array(element)) => {
                let source_element = self.context.canonical_union(elements);
                self.relate_inner(source_element, *element, mode, depth + 1)
                    .map_err(|error| {
                        wrap_failure(source, target, RelationFailureKind::ArrayElement, error)
                    })
            }
            (TypeKind::Array(_), TypeKind::Tuple(required)) => Err(failure(
                source,
                target,
                RelationFailureKind::ArrayToTupleLength {
                    required: required.len(),
                },
            )),
            (
                _,
                TypeKind::ClassInstance {
                    properties: target_shape,
                    ..
                },
            ) => {
                if shape_has_unsupported_callable_members(self.context, target_shape)
                    || !target_shape.index_signatures.is_empty()
                {
                    Err(failure(source, target, RelationFailureKind::Deferred))
                } else {
                    self.relate_properties(
                        source,
                        target,
                        None,
                        &target_shape.properties,
                        mode,
                        depth,
                    )
                }
            }
            (TypeKind::Function(source_signature), TypeKind::Function(target_signature)) => self
                .relate_signatures(
                    source,
                    target,
                    &ShapeSignature::from(source_signature),
                    &ShapeSignature::from(target_signature),
                    mode,
                    depth,
                ),
            (
                TypeKind::ShapeFunction(source_signature),
                TypeKind::ShapeFunction(target_signature),
            ) => self
                .relate_signatures(
                    source,
                    target,
                    source_signature,
                    target_signature,
                    mode,
                    depth,
                )
                .map_err(|_| failure(source, target, RelationFailureKind::Deferred)),
            (TypeKind::Function(source_signature), TypeKind::ShapeFunction(target_signature)) => {
                self.relate_signatures(
                    source,
                    target,
                    &ShapeSignature::from(source_signature),
                    target_signature,
                    mode,
                    depth,
                )
                .map_err(|_| failure(source, target, RelationFailureKind::Deferred))
            }
            (TypeKind::ShapeFunction(source_signature), TypeKind::Function(target_signature)) => {
                self.relate_signatures(
                    source,
                    target,
                    source_signature,
                    &ShapeSignature::from(target_signature),
                    mode,
                    depth,
                )
                .map_err(|_| failure(source, target, RelationFailureKind::Deferred))
            }
            (TypeKind::Function(_) | TypeKind::ShapeFunction(_), TypeKind::Object(_))
            | (TypeKind::Object(_), TypeKind::Function(_) | TypeKind::ShapeFunction(_))
            | (TypeKind::Array(_), TypeKind::Object(_)) => {
                Err(failure(source, target, RelationFailureKind::Deferred))
            }
            _ => Err(failure(source, target, RelationFailureKind::Incompatible)),
        }
    }

    fn relate_object_shapes(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &super::types::ObjectShape,
        target_shape: &super::types::ObjectShape,
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        if shape_has_unsupported_callable_members(self.context, source_shape)
            || shape_has_unsupported_callable_members(self.context, target_shape)
            || !source_shape.index_signatures.is_empty()
        {
            return Err(failure(source, target, RelationFailureKind::Deferred));
        }
        self.relate_properties(
            source,
            target,
            Some(&source_shape.properties),
            &target_shape.properties,
            mode,
            depth,
        )?;
        let [] = target_shape.index_signatures.as_slice() else {
            let [index] = target_shape.index_signatures.as_slice() else {
                return Err(failure(source, target, RelationFailureKind::Deferred));
            };
            if index.key != super::types::IndexKeyKind::String {
                return Err(failure(source, target, RelationFailureKind::Deferred));
            }
            for property in &source_shape.properties {
                if let Err(error) = self.relate_inner(property.ty, index.value, mode, depth + 1) {
                    return Err(wrap_failure(
                        source,
                        target,
                        RelationFailureKind::Property(property.name.clone()),
                        error,
                    ));
                }
            }
            return Ok(());
        };
        Ok(())
    }

    fn relate_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_properties: Option<&[Property]>,
        target_properties: &[Property],
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        let authored_order = self.property_order.get(target).map(<[_]>::to_vec);
        let mut ordered_properties = Vec::with_capacity(target_properties.len());
        if let Some(names) = &authored_order {
            for name in names {
                if let Some(property) = target_properties
                    .iter()
                    .find(|property| &property.name == name)
                {
                    ordered_properties.push(property);
                }
            }
            for property in target_properties {
                if !names.iter().any(|name| name == &property.name) {
                    ordered_properties.push(property);
                }
            }
        } else {
            ordered_properties.extend(target_properties);
        }

        let missing = ordered_properties
            .iter()
            .filter(|target_property| {
                !target_property.optional
                    && source_properties.is_none_or(|properties| {
                        !properties
                            .iter()
                            .any(|property| property.name == target_property.name)
                    })
            })
            .map(|property| property.name.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            if authored_order.is_none() && missing.len() > 1 {
                return Err(failure(source, target, RelationFailureKind::Deferred));
            }
            let missing = failure(
                source,
                target,
                if missing.len() == 1 {
                    RelationFailureKind::MissingProperty(missing[0].clone())
                } else {
                    RelationFailureKind::MissingProperties(missing)
                },
            );
            return Err(wrap_failure(
                source,
                target,
                RelationFailureKind::Object,
                missing,
            ));
        }

        let mut definitive_failure = None;
        for target_property in ordered_properties {
            let source_property = source_properties.and_then(|properties| {
                properties
                    .iter()
                    .find(|property| property.name == target_property.name)
            });
            let Some(source_property) = source_property else {
                continue;
            };
            if let Err(error) =
                self.relate_inner(source_property.ty, target_property.ty, mode, depth + 1)
            {
                if error.kind.propagates_unchanged() {
                    return Err(error);
                }
                let property = wrap_failure(
                    source_property.ty,
                    target_property.ty,
                    RelationFailureKind::Property(target_property.name.clone()),
                    error,
                );
                let property = wrap_failure(source, target, RelationFailureKind::Object, property);
                if authored_order.is_some() {
                    return Err(property);
                }
                if definitive_failure.is_some() {
                    return Err(failure(source, target, RelationFailureKind::Deferred));
                }
                definitive_failure = Some(property);
            }
        }
        definitive_failure.map_or(Ok(()), Err)
    }

    fn relate_signatures(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_signature: &ShapeSignature,
        target_signature: &ShapeSignature,
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        let source_required = source_signature
            .parameters
            .iter()
            .filter(|parameter| {
                !source_signature.untyped_javascript
                    && !parameter.optional
                    && !parameter.rest
                    && !matches!(self.context.type_kind(parameter.ty), TypeKind::Void)
            })
            .count();
        let target_has_rest = target_signature
            .parameters
            .iter()
            .any(|parameter| parameter.rest);
        if !target_has_rest && source_required > target_signature.parameters.len() {
            return Err(failure(source, target, RelationFailureKind::Incompatible));
        }
        for (index, (source_parameter, target_parameter)) in source_signature
            .parameters
            .iter()
            .zip(&target_signature.parameters)
            .enumerate()
        {
            self.relate_inner(
                target_parameter.ty,
                source_parameter.ty,
                RelationMode::Subtype,
                depth + 1,
            )
            .map_err(|error| {
                wrap_failure(source, target, RelationFailureKind::Parameter(index), error)
            })?;
        }
        if matches!(
            self.context.type_kind(target_signature.return_type),
            TypeKind::Void
        ) {
            return Ok(());
        }
        self.relate_inner(
            source_signature.return_type,
            target_signature.return_type,
            mode,
            depth + 1,
        )
        .map_err(|error| wrap_failure(source, target, RelationFailureKind::Return, error))
    }

    fn relate_to_alternative(
        &mut self,
        source: TypeId,
        target: TypeId,
        members: &[TypeId],
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        let mut first_failure = None;
        let mut propagated = None;
        for member in members {
            match self.relate_inner(source, *member, mode, depth + 1) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind.propagates_unchanged() => {
                    if propagated.as_ref().is_none_or(|current: &RelationFailure| {
                        current.kind.propagation_priority() < error.kind.propagation_priority()
                    }) {
                        propagated = Some(error);
                    }
                }
                Err(error) => {
                    if first_failure.is_none() {
                        first_failure = Some((*member, error));
                    }
                }
            }
        }
        if let Some(propagated) = propagated {
            return Err(propagated);
        }
        let (member, selected) = first_failure.unwrap_or_else(|| {
            (
                target,
                failure(source, target, RelationFailureKind::Incompatible),
            )
        });
        let selected = if selected.target == member {
            selected
        } else {
            wrap_failure(
                source,
                member,
                RelationFailureKind::AliasExpansion,
                selected,
            )
        };
        Err(wrap_failure(
            source,
            target,
            RelationFailureKind::UnionMember,
            selected,
        ))
    }

    fn relate_from_alternative(
        &mut self,
        source: TypeId,
        target: TypeId,
        members: &[TypeId],
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        let mut last_failure = None;
        let mut propagated = None;
        for member in members {
            match self.relate_inner(*member, target, mode, depth + 1) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind.propagates_unchanged() => {
                    if propagated.as_ref().is_none_or(|current: &RelationFailure| {
                        current.kind.propagation_priority() < error.kind.propagation_priority()
                    }) {
                        propagated = Some(error);
                    }
                }
                Err(error) => last_failure = Some(error),
            }
        }
        Err(propagated
            .or(last_failure)
            .unwrap_or_else(|| failure(source, target, RelationFailureKind::Incompatible)))
    }

    fn force(
        &mut self,
        ty: TypeId,
        source: TypeId,
        target: TypeId,
    ) -> Result<TypeId, RelationFailure> {
        match self.context.force_type(ty, self.evaluation_depth.0) {
            Completion::Complete(value) => Ok(value),
            Completion::Cycle => Err(failure(source, target, RelationFailureKind::Cycle)),
            Completion::Limit => Err(failure(
                source,
                target,
                RelationFailureKind::ComplexityLimit,
            )),
            Completion::Deferred => Err(failure(source, target, RelationFailureKind::Deferred)),
        }
    }
}

fn deferred_reference(kind: TypeKind) -> Option<(crate::source::DeclId, Vec<TypeId>)> {
    let TypeKind::Deferred(DeferredType::Reference {
        declaration,
        arguments,
    }) = kind
    else {
        return None;
    };
    Some((declaration, arguments))
}

const fn object_shape(kind: &TypeKind) -> Option<&super::types::ObjectShape> {
    match kind {
        TypeKind::Object(shape)
        | TypeKind::ClassInstance {
            properties: shape, ..
        } => Some(shape),
        _ => None,
    }
}

fn shape_has_unsupported_callable_members<C: RelationContext>(
    context: &C,
    shape: &super::types::ObjectShape,
) -> bool {
    !shape.call_signatures.is_empty()
        || !shape.construct_signatures.is_empty()
        || shape
            .properties
            .iter()
            .any(|property| matches!(context.type_kind(property.ty), TypeKind::ShapeFunction(_)))
}

fn wrap_failure(
    source: TypeId,
    target: TypeId,
    path: RelationFailureKind,
    child: RelationFailure,
) -> RelationFailure {
    if child.kind.propagates_unchanged() {
        child
    } else {
        RelationFailure {
            source,
            target,
            kind: path,
            child: Some(Box::new(child)),
        }
    }
}

const fn failure(source: TypeId, target: TypeId, kind: RelationFailureKind) -> RelationFailure {
    RelationFailure {
        source,
        target,
        kind,
        child: None,
    }
}

#[cfg(test)]
#[path = "../../rewrite-tests/relation_unit.rs"]
mod tests;
