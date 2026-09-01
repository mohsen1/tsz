use super::checker::recursion::{ReferenceDemand, ReferenceExpansionStack};
use super::types::{Completion, DeferredType, Property, Signature, TypeId, TypeKind};
use std::collections::HashSet;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationMode {
    Subtype,
    Assignment,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationFailureKind {
    Incompatible,
    SignatureArityMismatch {
        source_minimum: usize,
        target_parameter_count: usize,
    },
    MissingProperty(String),
    MissingProperties(Vec<String>),
    Property(String),
    Object,
    ArrayElement,
    TupleElement(usize),
    TypeArgument(usize),
    ArrayToTupleLength {
        required: usize,
    },
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
    /// Priority for incomplete failures observed while trying alternatives.
    /// An invalid projection already owns a concrete diagnostic elsewhere;
    /// it must not hide a semantic nonclaim. The completion verdict follows
    /// the public deterministic dominance `Deferred < Cycle < Limit`.
    const fn propagation_rank(&self) -> u8 {
        match self {
            Self::InvalidProjection => 1,
            Self::Deferred => 2,
            Self::Cycle => 3,
            Self::ComplexityLimit => 4,
            _ => 0,
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
impl RelationFailure {
    fn wrapped(self, source: TypeId, target: TypeId, kind: RelationFailureKind) -> Self {
        if self.kind.propagation_rank() > 0 {
            self
        } else {
            Self {
                source,
                target,
                kind,
                child: Some(Box::new(self)),
            }
        }
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
    fn library_reference_arguments_are_covariant(&self, declaration: crate::source::DeclId)
    -> bool;
    fn class_constructor_signature(
        &mut self,
        _declaration: crate::source::DeclId,
    ) -> Completion<Signature> {
        Completion::Deferred
    }
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
pub(crate) fn relate_types<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
) -> Result<(), RelationFailure> {
    relate_types_at_evaluation_depth(context, source, target, mode, EvaluationDepth::ROOT)
}
pub(crate) fn relate_types_at_evaluation_depth<C: RelationContext>(
    context: &mut C,
    source: TypeId,
    target: TypeId,
    mode: RelationMode,
    evaluation_depth: EvaluationDepth,
) -> Result<(), RelationFailure> {
    Relation {
        context,
        active: HashSet::new(),
        source_references: ReferenceExpansionStack::new(ReferenceDemand::RelationSource),
        target_references: ReferenceExpansionStack::new(ReferenceDemand::RelationTarget),
        evaluation_depth,
    }
    .relate_inner(source, target, mode, 0)
}
struct Relation<'a, C> {
    context: &'a mut C,
    active: HashSet<RelationQueryKey>,
    source_references: ReferenceExpansionStack,
    target_references: ReferenceExpansionStack,
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
        let source = self.assignment_source(source, target, mode);
        if source == target {
            return Ok(());
        }
        if mode == RelationMode::Assignment
            && matches!(self.context.type_kind(source), TypeKind::Any)
            && matches!(
                self.context.type_kind(target),
                TypeKind::Deferred(DeferredType::KeyOf(operand))
                    if matches!(self.context.type_kind(operand), TypeKind::TypeParameter { .. })
            )
        {
            return Ok(());
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
    fn assignment_source(&self, mut source: TypeId, target: TypeId, mode: RelationMode) -> TypeId {
        let target_kind = self.context.type_kind(target);
        while mode == RelationMode::Assignment
            && let TypeKind::Deferred(DeferredType::Logical {
                operator: super::types::DeferredLogicalOperator::Or,
                left,
                right,
            }) = self.context.type_kind(source)
        {
            let same_target = left == target
                && matches!(target_kind, TypeKind::Deferred(DeferredType::Value(_)))
                || matches!(
                    (&target_kind, self.context.type_kind(left)),
                    (
                        TypeKind::Deferred(DeferredType::Value(target_declaration)),
                        TypeKind::Deferred(DeferredType::FlowReference {
                            declaration,
                            declared,
                            ..
                        })
                    ) if declaration == *target_declaration && declared == target
                );
            if !same_target {
                break;
            }
            source = right;
        }
        source
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
        if matches!(target_kind, TypeKind::Never) {
            return Err(failure(source, target, RelationFailureKind::Incompatible));
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
                        return Err(error.wrapped(
                            source,
                            target,
                            RelationFailureKind::UnionMember,
                        ));
                    }
                }
                Ok(())
            }
            (_, TypeKind::Union(members)) => {
                self.relate_alternatives(source, target, members, mode, depth, false)
            }
            (_, TypeKind::Intersection(members)) => {
                for member in members {
                    self.relate_inner(source, *member, mode, depth + 1)?;
                }
                Ok(())
            }
            (TypeKind::Intersection(members), _) => {
                self.relate_alternatives(source, target, members, mode, depth, true)
            }
            (TypeKind::Array(left), TypeKind::Array(right)) => self
                .relate_inner(*left, *right, mode, depth + 1)
                .map_err(|error| error.wrapped(source, target, RelationFailureKind::ArrayElement)),
            (TypeKind::Array(_), TypeKind::Object(shape))
                if shape.properties.is_empty()
                    && shape.call_signatures.is_empty()
                    && shape.construct_signatures.is_empty()
                    && shape.index_signatures.is_empty() =>
            {
                Ok(())
            }
            (TypeKind::Tuple(left), TypeKind::Tuple(right)) if left.len() == right.len() => self
                .relate_covariant_types(
                    source,
                    target,
                    left,
                    right,
                    mode,
                    depth,
                    RelationFailureKind::TupleElement,
                ),
            (TypeKind::Tuple(elements), TypeKind::Array(element)) => {
                let source_element = self.context.canonical_union(elements);
                self.relate_inner(source_element, *element, mode, depth + 1)
                    .map_err(|error| {
                        error.wrapped(source, target, RelationFailureKind::ArrayElement)
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
                if shape_has_unsupported_callable_members(self.context, target_shape, None)
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
                    source_signature,
                    target_signature,
                    mode,
                    depth,
                ),
            (
                TypeKind::ShapeFunction(source_signature),
                TypeKind::Function(target_signature) | TypeKind::ShapeFunction(target_signature),
            )
            | (TypeKind::Function(source_signature), TypeKind::ShapeFunction(target_signature)) => {
                self.relate_signatures(
                    source,
                    target,
                    source_signature,
                    target_signature,
                    mode,
                    depth,
                )
                .map_err(|_| failure(source, target, RelationFailureKind::Deferred))
            }
            (
                TypeKind::ClassConstructor {
                    declaration: source_declaration,
                    ..
                },
                TypeKind::ClassConstructor {
                    declaration: target_declaration,
                    ..
                },
            ) => {
                let source_signature = self
                    .context
                    .class_constructor_signature(*source_declaration);
                let source_signature = Self::relation_input(source_signature, source, target)?;
                let target_signature = self
                    .context
                    .class_constructor_signature(*target_declaration);
                let target_signature = Self::relation_input(target_signature, source, target)?;
                Err(self
                    .signature_arity_failure(source, target, &source_signature, &target_signature)
                    .unwrap_or_else(|| failure(source, target, RelationFailureKind::Deferred)))
            }
            (
                TypeKind::LibraryReference {
                    declaration: source_declaration,
                    arguments: source_arguments,
                    ..
                },
                TypeKind::LibraryReference {
                    declaration: target_declaration,
                    arguments: target_arguments,
                    ..
                },
            ) if source_declaration == target_declaration
                && self
                    .context
                    .library_reference_arguments_are_covariant(*source_declaration) =>
            {
                if source_arguments.len() != target_arguments.len() {
                    Err(failure(source, target, RelationFailureKind::Deferred))
                } else {
                    self.relate_covariant_types(
                        source,
                        target,
                        source_arguments,
                        target_arguments,
                        mode,
                        depth,
                        RelationFailureKind::TypeArgument,
                    )
                }
            }
            (TypeKind::Function(_) | TypeKind::ShapeFunction(_), TypeKind::Object(_))
            | (TypeKind::Object(_), TypeKind::Function(_) | TypeKind::ShapeFunction(_))
            | (TypeKind::Array(_), TypeKind::Object(_))
            | (TypeKind::LibraryReference { .. }, _)
            | (_, TypeKind::LibraryReference { .. }) => {
                Err(failure(source, target, RelationFailureKind::Deferred))
            }
            _ => Err(failure(source, target, RelationFailureKind::Incompatible)),
        }
    }
    fn relate_covariant_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_types: &[TypeId],
        target_types: &[TypeId],
        mode: RelationMode,
        depth: usize,
        element_failure: fn(usize) -> RelationFailureKind,
    ) -> Result<(), RelationFailure> {
        source_types
            .iter()
            .zip(target_types)
            .enumerate()
            .try_for_each(|(index, (source_type, target_type))| {
                self.relate_inner(*source_type, *target_type, mode, depth + 1)
                    .map_err(|error| error.wrapped(source, target, element_failure(index)))
            })
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
        if shape_has_unsupported_callable_members(self.context, target_shape, Some(source_shape))
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
                    return Err(error.wrapped(
                        source,
                        target,
                        RelationFailureKind::Property(property.name.clone()),
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
        let missing = target_properties
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
            let missing = failure(
                source,
                target,
                if missing.len() == 1 {
                    RelationFailureKind::MissingProperty(missing[0].clone())
                } else {
                    RelationFailureKind::MissingProperties(missing)
                },
            );
            return Err(missing.wrapped(source, target, RelationFailureKind::Object));
        }
        for target_property in target_properties {
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
                let property = error.wrapped(
                    source_property.ty,
                    target_property.ty,
                    RelationFailureKind::Property(target_property.name.clone()),
                );
                let property = property.wrapped(source, target, RelationFailureKind::Object);
                return Err(property);
            }
        }
        Ok(())
    }
    fn relate_signatures(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_signature: &Signature,
        target_signature: &Signature,
        mode: RelationMode,
        depth: usize,
    ) -> Result<(), RelationFailure> {
        if let Some(failure) =
            self.signature_arity_failure(source, target, source_signature, target_signature)
        {
            return Err(failure);
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
                error.wrapped(source, target, RelationFailureKind::Parameter(index))
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
        .map_err(|error| error.wrapped(source, target, RelationFailureKind::Return))
    }
    fn signature_arity_failure(
        &self,
        source: TypeId,
        target: TypeId,
        source_signature: &Signature,
        target_signature: &Signature,
    ) -> Option<RelationFailure> {
        let source_minimum = source_signature
            .parameters
            .iter()
            .filter(|parameter| {
                !source_signature.untyped_javascript
                    && !parameter.optional
                    && !parameter.rest
                    && !matches!(self.context.type_kind(parameter.ty), TypeKind::Void)
            })
            .count();
        let target_parameter_count = target_signature.parameters.len();
        (!target_signature
            .parameters
            .iter()
            .any(|parameter| parameter.rest)
            && source_minimum > target_parameter_count)
            .then(|| RelationFailure {
                source,
                target,
                kind: RelationFailureKind::Incompatible,
                child: Some(Box::new(failure(
                    source,
                    target,
                    RelationFailureKind::SignatureArityMismatch {
                        source_minimum,
                        target_parameter_count,
                    },
                ))),
            })
    }
    fn relation_input<T>(
        input: Completion<T>,
        source: TypeId,
        target: TypeId,
    ) -> Result<T, RelationFailure> {
        let kind = match input {
            Completion::Complete(value) => return Ok(value),
            Completion::Deferred => RelationFailureKind::Deferred,
            Completion::Cycle => RelationFailureKind::Cycle,
            Completion::Limit => RelationFailureKind::ComplexityLimit,
        };
        Err(failure(source, target, kind))
    }
    fn relate_alternatives(
        &mut self,
        source: TypeId,
        target: TypeId,
        members: &[TypeId],
        mode: RelationMode,
        depth: usize,
        source_members: bool,
    ) -> Result<(), RelationFailure> {
        let mut selected = None;
        for member in members {
            let relation = if source_members {
                self.relate_inner(*member, target, mode, depth + 1)
            } else {
                self.relate_inner(source, *member, mode, depth + 1)
            };
            match relation {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let rank = error.kind.propagation_rank();
                    let replace =
                        selected
                            .as_ref()
                            .is_none_or(|(_, current): &(TypeId, RelationFailure)| {
                                rank > current.kind.propagation_rank()
                                    || source_members
                                        && rank == 0
                                        && current.kind.propagation_rank() == 0
                            });
                    if replace {
                        selected = Some((*member, error));
                    }
                }
            }
        }
        let (member, selected) = selected.unwrap_or_else(|| {
            (
                if source_members { source } else { target },
                failure(source, target, RelationFailureKind::Incompatible),
            )
        });
        if source_members || selected.kind.propagation_rank() > 0 {
            return Err(selected);
        }
        let selected = if selected.target == member {
            selected
        } else {
            selected.wrapped(source, member, RelationFailureKind::AliasExpansion)
        };
        Err(selected.wrapped(source, target, RelationFailureKind::UnionMember))
    }
    fn force(
        &mut self,
        ty: TypeId,
        source: TypeId,
        target: TypeId,
    ) -> Result<TypeId, RelationFailure> {
        let input = self.context.force_type(ty, self.evaluation_depth.0);
        Self::relation_input(input, source, target)
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
    target: &super::types::ObjectShape,
    source: Option<&super::types::ObjectShape>,
) -> bool {
    if !target.call_signatures.is_empty()
        || !target.construct_signatures.is_empty()
        || source.is_some_and(|source| {
            !source.call_signatures.is_empty() || !source.construct_signatures.is_empty()
        })
    {
        return true;
    }
    target.properties.iter().any(|target_property| {
        let target_kind = context.type_kind(target_property.ty);
        let Some(source_property) = source.and_then(|source| {
            source
                .properties
                .iter()
                .find(|property| property.name == target_property.name)
        }) else {
            return matches!(target_kind, TypeKind::ShapeFunction(_));
        };
        let source_kind = context.type_kind(source_property.ty);
        match (&source_kind, &target_kind) {
            (TypeKind::ShapeFunction(source), TypeKind::Function(target))
            | (TypeKind::ShapeFunction(source), TypeKind::ShapeFunction(target))
            | (TypeKind::Function(source), TypeKind::ShapeFunction(target)) => {
                source.generic_declaration.is_some() || target.generic_declaration.is_some()
            }
            (TypeKind::ShapeFunction(_), _) | (_, TypeKind::ShapeFunction(_)) => true,
            _ => false,
        }
    })
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
#[path = "../../rewrite-tests/relation_arity_unit.rs"]
mod arity_tests;
#[cfg(test)]
#[path = "../../rewrite-tests/relation_unit.rs"]
mod tests;
