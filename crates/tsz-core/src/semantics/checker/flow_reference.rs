use crate::bind::{
    BoundFlowNode, FlowAssignmentSource, FlowNarrowing, FlowPathSegment, TypeofWitness,
    TypeofWitnessSet,
};
use crate::semantics::relation::{
    EvaluationDepth, RelationFailureKind, RelationMode,
    relate_with_property_order_at_evaluation_depth,
};
use crate::semantics::types::{
    Completion, DeferredType, LiteralProvenance, TypeId, TypeKind, UnionPolicy,
};
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::TypeNodeKind;

use super::{Checker, DeclarationModel};

impl Checker<'_> {
    pub(super) fn force_flow(&mut self, query: DeferredType, depth: usize) -> Completion<TypeId> {
        let DeferredType::FlowReference {
            file,
            expression,
            declaration,
            declared,
        } = query
        else {
            unreachable!("flow-reference forcing requires a flow query");
        };
        self.flow_type_of_reference(file, expression, declaration, declared, depth)
    }

    pub(super) fn flow_type_of_reference(
        &mut self,
        file: FileId,
        expression: NodeId,
        declaration: DeclId,
        declared: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let flow = &self.program.files[file.0 as usize].bindings.flow;
        let Some(mut node) = flow.reference_node(expression, declaration) else {
            return self.force_operand(declared, depth);
        };
        let mut narrowings = Vec::new();
        let mut assigned = None;
        loop {
            node = match flow.node(node).clone() {
                BoundFlowNode::Start => break,
                BoundFlowNode::Narrowing {
                    antecedent,
                    subject,
                    narrowing,
                } => {
                    if subject == declaration {
                        narrowings.push(narrowing);
                    }
                    antecedent
                }
                BoundFlowNode::Assignment {
                    antecedent,
                    subject,
                    source,
                } => {
                    if subject == declaration {
                        assigned = Some(completed!(self.flow_assignment_source(
                            file,
                            source,
                            declared,
                            depth + 1,
                        )));
                        break;
                    }
                    antecedent
                }
                BoundFlowNode::Unsupported {
                    antecedent,
                    subject,
                    kind: _,
                } => {
                    if subject == declaration {
                        return Completion::Deferred;
                    }
                    antecedent
                }
            };
        }

        let mut narrowed = match assigned {
            Some(assigned) => assigned,
            None => completed!(self.force_operand(declared, depth)),
        };
        for narrowing in narrowings.into_iter().rev() {
            narrowed = completed!(match &narrowing {
                FlowNarrowing::TruthinessCandidate(truthy) => {
                    let candidate =
                        completed!(self.narrow_truthiness_candidate(narrowed, *truthy, depth + 1,));
                    Completion::Complete(candidate.unwrap_or(narrowed))
                }
                FlowNarrowing::Typeof { include, values } => {
                    self.narrow_typeof(narrowed, *include, *values, depth + 1)
                }
                FlowNarrowing::StringLiteral {
                    property,
                    include,
                    values,
                } => self.narrow_string_literals(
                    narrowed,
                    property.as_deref(),
                    *include,
                    values,
                    depth + 1,
                ),
                FlowNarrowing::PredicateCall {
                    callee,
                    argument_index,
                    path,
                    truthy,
                } => self.narrow_predicate(
                    narrowed,
                    *callee,
                    *argument_index,
                    path,
                    *truthy,
                    depth + 1,
                ),
            });
        }
        Completion::Complete(narrowed)
    }

    /// `None` preserves the antecedent because this bounded candidate is inapplicable.
    fn narrow_truthiness_candidate(
        &mut self,
        ty: TypeId,
        truthy: bool,
        depth: usize,
    ) -> Completion<Option<TypeId>> {
        if !self.options.effective_strict_null_checks() {
            return Completion::Complete(None);
        }
        let ty = completed!(self.force_operand(ty, depth));
        let TypeKind::Union(members) = self.store.kind(ty) else {
            return Completion::Complete(None);
        };
        let [left, right] = members.as_slice() else {
            return Completion::Complete(None);
        };
        Completion::Complete(match (self.store.kind(*left), self.store.kind(*right)) {
            (TypeKind::Array(_), TypeKind::Undefined) => Some(if truthy { *left } else { *right }),
            (TypeKind::Undefined, TypeKind::Array(_)) => Some(if truthy { *right } else { *left }),
            _ => None,
        })
    }

    fn flow_assignment_source(
        &mut self,
        file: FileId,
        source: FlowAssignmentSource,
        declared: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let declared = completed!(self.force_operand(declared, depth));
        if let FlowAssignmentSource::LogicalOrConstruct(references) = &source {
            return self.flow_logical_or_construct(file, *references, declared, depth + 1);
        }
        if !matches!(self.store.kind(declared), TypeKind::Union(_)) {
            return Completion::Complete(declared);
        }
        if let FlowAssignmentSource::Join(sources) = source {
            let [left, right] = *sources;
            let left = completed!(self.flow_assignment_source(file, left, declared, depth + 1));
            let right = completed!(self.flow_assignment_source(file, right, declared, depth + 1));
            return Completion::Complete(self.store.union([left, right], UnionPolicy::Canonical));
        }
        let (source, alternate) = match source {
            FlowAssignmentSource::Reference(reference) => (
                completed!(self.flow_source(file, reference, depth + 1)),
                None,
            ),
            FlowAssignmentSource::Literal(literal) => {
                if matches!(&literal, crate::syntax::Literal::Boolean(_)) {
                    return Completion::Deferred;
                }
                let fresh = self.literal_type(&literal, LiteralProvenance::Fresh);
                (
                    self.widen(fresh),
                    Some(self.literal_type(&literal, LiteralProvenance::Regular)),
                )
            }
            FlowAssignmentSource::DirectCall(callee, argument) => {
                let callee_type = completed!(self.flow_source(file, callee, depth + 1));
                let argument = completed!(self.flow_source(file, argument, depth + 1));
                let source = completed!(self.direct_call_type(
                    Some(callee.1),
                    callee_type,
                    Some(argument),
                    1,
                ));
                (source, None)
            }
            FlowAssignmentSource::LogicalOrConstruct(_) | FlowAssignmentSource::Join(_) => {
                return Completion::Deferred;
            }
        };
        if matches!(self.store.kind(source), TypeKind::Never) {
            return Completion::Complete(source);
        }
        for source in [Some(source), alternate].into_iter().flatten() {
            let exact = source == declared
                || matches!(self.store.kind(declared), TypeKind::Union(members) if members.contains(&source));
            if matches!(self.store.kind(source), TypeKind::Union(_)) && !exact {
                return Completion::Deferred;
            }
            if completed!(self.flow_related(source, declared, RelationMode::Assignment, depth)) {
                if !exact {
                    return Completion::Deferred;
                }
                return Completion::Complete(source);
            }
        }
        Completion::Complete(declared)
    }

    fn flow_logical_or_construct(
        &mut self,
        file: FileId,
        references: [(NodeId, DeclId); 2],
        declared: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let [reference, callee] = references;
        let reference = completed!(self.flow_source(file, reference, depth + 1));
        let callee = completed!(self.flow_source(file, callee, depth + 1));
        let constructed = completed!(self.evaluate_construct(callee, &[], &[], depth + 1));
        let [member, forced] =
            completed!(self.canonical_map_fallback_member(declared, constructed, depth + 1,));
        if reference == declared || reference == member || reference == forced {
            Completion::Complete(member)
        } else {
            Completion::Deferred
        }
    }

    pub(super) fn canonical_map_fallback_member(
        &mut self,
        declared: TypeId,
        constructed: TypeId,
        depth: usize,
    ) -> Completion<[TypeId; 2]> {
        let declared = completed!(self.force_operand(declared, depth));
        let member = completed!(self.store.non_nullable(declared));
        let forced = completed!(self.force_operand(member, depth + 1));
        let TypeKind::LibraryReference { declaration, .. } = self.store.kind(forced) else {
            return Completion::Deferred;
        };
        let declaration = *declaration;
        if !self.program.standard_library.is_map_type(declaration)
            || self
                .program
                .standard_library_type_has_authored_declarations(declaration)
        {
            return Completion::Deferred;
        }
        let constructed = completed!(self.force_operand(constructed, depth + 1));
        let any = self.store.builtins.any;
        match self.store.kind(constructed) {
            TypeKind::LibraryReference {
                declaration: constructed,
                arguments,
                ..
            } if *constructed == declaration && arguments.as_slice() == [any, any] => {
                Completion::Complete([member, forced])
            }
            _ => Completion::Deferred,
        }
    }

    fn flow_source(
        &mut self,
        file: FileId,
        (expression, declaration): (NodeId, DeclId),
        depth: usize,
    ) -> Completion<TypeId> {
        let declared = self
            .store
            .intern(TypeKind::Deferred(DeferredType::Value(declaration)));
        self.flow_type_of_reference(file, expression, declaration, declared, depth)
    }

    fn narrow_predicate(
        &mut self,
        narrowed: TypeId,
        callee: DeclId,
        argument_index: usize,
        path: &[FlowPathSegment],
        truthy: Option<bool>,
        depth: usize,
    ) -> Completion<TypeId> {
        if !self.semantic_declaration_is_claimed(callee) {
            return Completion::Deferred;
        }
        let Some(DeclarationModel::Function { declaration, scope }) =
            self.models.get(&callee).copied()
        else {
            return Completion::Deferred;
        };
        let Some(mut return_type) = declaration.return_type.as_ref() else {
            return Completion::Complete(narrowed);
        };
        while let TypeNodeKind::Parenthesized(inner) = &return_type.kind {
            return_type = inner;
        }
        let TypeNodeKind::Predicate {
            parameter,
            asserts,
            ty,
            ..
        } = &return_type.kind
        else {
            return Completion::Complete(narrowed);
        };
        if declaration
            .parameters
            .iter()
            .position(|candidate| candidate.name == *parameter)
            != Some(argument_index)
        {
            return Completion::Complete(narrowed);
        }
        let Some(asserted) = ty.as_deref() else {
            return Completion::Deferred;
        };
        if *asserts
            || !declaration.type_parameters.is_empty()
            || self.function_value_requires_overload_resolution(callee)
            || truthy.is_none()
        {
            return Completion::Deferred;
        }
        let asserted = self.resolve_type_node(callee.file, scope, asserted, &Default::default());
        self.narrow_predicate_path(narrowed, path, asserted, truthy == Some(true), depth)
    }

    fn narrow_predicate_path(
        &mut self,
        narrowed: TypeId,
        path: &[FlowPathSegment],
        asserted: TypeId,
        truthy: bool,
        depth: usize,
    ) -> Completion<TypeId> {
        let [narrowed, asserted] = self.force_operands([narrowed, asserted], depth);
        let narrowed = completed!(narrowed);
        let asserted = completed!(asserted);
        let Some((segment, rest)) = path.split_first() else {
            return self.predicate_leaf(narrowed, asserted, truthy, depth);
        };
        self.narrow_predicate_object(narrowed, segment, rest, asserted, truthy, depth)
    }

    fn narrow_predicate_object(
        &mut self,
        narrowed: TypeId,
        segment: &FlowPathSegment,
        rest: &[FlowPathSegment],
        asserted: TypeId,
        truthy: bool,
        depth: usize,
    ) -> Completion<TypeId> {
        let kind = self.store.kind(narrowed).clone();
        let mut shape = match &kind {
            TypeKind::Object(shape) => shape.clone(),
            TypeKind::ClassInstance { properties, .. } => properties.clone(),
            _ => return Completion::Deferred,
        };
        let FlowPathSegment(name) = segment;
        let Some(index) = shape
            .properties
            .iter()
            .position(|property| property.name == *name && !property.optional)
        else {
            return Completion::Deferred;
        };
        shape.properties[index].ty = completed!(self.narrow_predicate_path(
            shape.properties[index].ty,
            rest,
            asserted,
            truthy,
            depth + 1,
        ));
        Completion::Complete(match kind {
            TypeKind::Object(_) => self.store.object_shape(shape),
            TypeKind::ClassInstance {
                declaration,
                name,
                arguments,
                ..
            } => self.store.intern(TypeKind::ClassInstance {
                declaration,
                name,
                arguments,
                properties: shape,
            }),
            _ => unreachable!(),
        })
    }

    fn predicate_leaf(
        &mut self,
        ty: TypeId,
        asserted: TypeId,
        truthy: bool,
        depth: usize,
    ) -> Completion<TypeId> {
        let never = self.store.builtins.never;
        let incomplete = |this: &Self, ty| matches!(this.store.kind(ty), TypeKind::Intersection(_));
        let select = |when_true, when_false| {
            Completion::Complete(if truthy { when_true } else { when_false })
        };
        if ty == asserted {
            return select(ty, never);
        }
        use TypeKind::*;
        match (self.store.kind(ty), self.store.kind(asserted)) {
            (_, Error | Invalid(_)) | (Never, _) => return Completion::Complete(ty),
            (Unknown, Any) => return select(asserted, never),
            (Error | Invalid(_), _) | (Any, _) | (Unknown, _) => return select(asserted, ty),
            (_, Any | Unknown) => return select(ty, never),
            (_, Never) => return select(never, ty),
            _ => {}
        }
        let source = self.predicate_constituents(ty);
        let candidates = self.predicate_constituents(asserted);
        let mut all_disjoint = true;
        let mut overlap = Vec::new();
        for member in source.iter().copied() {
            for candidate in candidates.iter().copied() {
                if completed!(self.flow_related(member, candidate, RelationMode::Subtype, depth)) {
                    overlap.push(member);
                } else if completed!(self.flow_related(
                    candidate,
                    member,
                    RelationMode::Subtype,
                    depth,
                )) {
                    overlap.push(candidate);
                } else if incomplete(self, member) || incomplete(self, candidate) {
                    return Completion::Deferred;
                } else if !self.predicate_types_are_disjoint(member, candidate) {
                    all_disjoint = false;
                }
            }
        }
        let true_part = if !overlap.is_empty() {
            self.store
                .union(overlap, UnionPolicy::PreserveAuthoredStructuralOrder)
        } else if all_disjoint {
            never
        } else if source.len() == 1
            && candidates.len() == 1
            && (Self::flow_type_domain(self.store.kind(source[0])).1
                || Self::flow_type_domain(self.store.kind(candidates[0])).1)
        {
            self.store.intersection([ty, asserted])
        } else {
            return Completion::Deferred;
        };
        if truthy {
            return Completion::Complete(true_part);
        }
        let true_part_is_incomplete = incomplete(self, true_part);
        let mut retained = Vec::new();
        for member in source {
            if completed!(self.flow_related(member, true_part, RelationMode::Subtype, depth)) {
                continue;
            }
            if true_part_is_incomplete || incomplete(self, member) {
                return Completion::Deferred;
            }
            retained.push(member);
        }
        Completion::Complete(
            self.store
                .union(retained, UnionPolicy::PreserveAuthoredStructuralOrder),
        )
    }

    fn predicate_constituents(&mut self, ty: TypeId) -> Vec<TypeId> {
        match self.store.kind(ty).clone() {
            TypeKind::Union(members) => members
                .into_iter()
                .flat_map(|member| self.predicate_constituents(member))
                .collect(),
            TypeKind::Boolean => [false, true]
                .map(|value| {
                    self.store
                        .intern(TypeKind::LiteralBoolean(value, LiteralProvenance::Regular))
                })
                .to_vec(),
            _ => vec![ty],
        }
    }

    fn predicate_types_are_disjoint(&self, left: TypeId, right: TypeId) -> bool {
        use TypeKind::*;
        match (self.store.kind(left), self.store.kind(right)) {
            (LiteralBoolean(left, _), LiteralBoolean(right, _)) => left != right,
            (LiteralNumber(left, _), LiteralNumber(right, _)) => left != right,
            (LiteralString(left, _), LiteralString(right, _)) => left != right,
            (left, right) => {
                let scalar = |kind| match Self::flow_type_domain(kind) {
                    (witness, false) => witness,
                    _ => None,
                };
                matches!((scalar(left), scalar(right)), (Some(left), Some(right)) if left != right)
            }
        }
    }

    pub(super) const fn flow_type_domain(kind: &TypeKind) -> (Option<TypeofWitness>, bool) {
        use TypeKind::*;
        match kind {
            String | LiteralString(_, _) => (Some(TypeofWitness::String), false),
            Number | LiteralNumber(_, _) => (Some(TypeofWitness::Number), false),
            BigInt => (Some(TypeofWitness::BigInt), false),
            Boolean | LiteralBoolean(_, _) => (Some(TypeofWitness::Boolean), false),
            Symbol => (Some(TypeofWitness::Symbol), false),
            Null => (Some(TypeofWitness::Object), false),
            Undefined => (Some(TypeofWitness::Undefined), false),
            ObjectKeyword => (None, true),
            Array(_) | Tuple(_) | ClassInstance { .. } => (Some(TypeofWitness::Object), true),
            Object(shape) => {
                let witness =
                    if shape.call_signatures.is_empty() && shape.construct_signatures.is_empty() {
                        TypeofWitness::Object
                    } else {
                        TypeofWitness::Function
                    };
                (Some(witness), true)
            }
            ClassConstructor { .. } | Function(_) | ShapeFunction(_) => {
                (Some(TypeofWitness::Function), true)
            }
            _ => (None, false),
        }
    }

    fn flow_related(
        &mut self,
        source: TypeId,
        target: TypeId,
        mode: RelationMode,
        depth: usize,
    ) -> Completion<bool> {
        let relation = relate_with_property_order_at_evaluation_depth(
            self,
            source,
            target,
            mode,
            Default::default(),
            EvaluationDepth::from_active_depth(depth),
        );
        let Err(failure) = relation else {
            return Completion::Complete(true);
        };
        use RelationFailureKind::*;
        match failure.kind {
            InvalidProjection | Deferred => Completion::Deferred,
            Cycle => Completion::Cycle,
            ComplexityLimit => Completion::Limit,
            _ => Completion::Complete(false),
        }
    }

    fn narrow_typeof(
        &mut self,
        ty: TypeId,
        include: bool,
        values: TypeofWitnessSet,
        depth: usize,
    ) -> Completion<TypeId> {
        self.map_flow_type(ty, depth, |this, member, depth| {
            let forced = completed!(this.force_operand(member, depth));
            let witness = Self::flow_type_domain(this.store.kind(forced)).0;
            witness.map_or(Completion::Deferred, |witness| {
                Completion::Complete((values.contains(witness) == include).then_some(member))
            })
        })
    }

    fn narrow_string_literals(
        &mut self,
        ty: TypeId,
        property: Option<&str>,
        include: bool,
        values: &[String],
        depth: usize,
    ) -> Completion<TypeId> {
        if let Some(name) = property {
            return self.map_flow_type(ty, depth, |this, member, depth| {
                let forced = completed!(this.force_operand(member, depth));
                let shape = match this.store.kind(forced) {
                    TypeKind::Object(shape)
                    | TypeKind::ClassInstance {
                        properties: shape, ..
                    } => shape,
                    _ => return Completion::Deferred,
                };
                let properties = &shape.properties;
                let Some(property) = properties.iter().find(|p| p.name == name && !p.optional)
                else {
                    return Completion::Deferred;
                };
                let value = completed!(this.force_operand(property.ty, depth + 1));
                let TypeKind::LiteralString(value, _) = this.store.kind(value) else {
                    return Completion::Deferred;
                };
                Completion::Complete((values.contains(value) == include).then_some(member))
            });
        }
        self.map_flow_type(ty, depth, |this, ty, depth| {
            let ty = completed!(this.force_operand(ty, depth));
            let top = matches!(this.store.kind(ty), TypeKind::Unknown | TypeKind::String)
                || matches!(this.store.kind(ty), TypeKind::Object(shape) if shape == &Default::default());
            if top {
                if !include {
                    return Completion::Complete(Some(ty));
                }
                let literals = values
                    .iter()
                    .cloned()
                    .map(|value| {
                        this.store
                            .intern(TypeKind::LiteralString(value, LiteralProvenance::Regular))
                    })
                    .collect::<Vec<_>>();
                let narrowed = this.store.union(literals, UnionPolicy::Canonical);
                return Completion::Complete(Some(narrowed));
            }
            let select = |matched| Completion::Complete((matched == include).then_some(ty));
            match this.store.kind(ty).clone() {
                TypeKind::LiteralString(value, _) => select(values.contains(&value)),
                TypeKind::Any | TypeKind::Never | TypeKind::Error | TypeKind::Invalid(_) => {
                    Completion::Complete(Some(ty))
                }
                TypeKind::Void
                | TypeKind::Undefined
                | TypeKind::Null
                | TypeKind::Boolean
                | TypeKind::Number
                | TypeKind::BigInt
                | TypeKind::Symbol
                | TypeKind::LiteralBoolean(_, _)
                | TypeKind::LiteralNumber(_, _) => select(false),
                _ => Completion::Deferred,
            }
        })
    }

    fn map_flow_type(
        &mut self,
        ty: TypeId,
        depth: usize,
        mut map: impl FnMut(&mut Self, TypeId, usize) -> Completion<Option<TypeId>>,
    ) -> Completion<TypeId> {
        let ty = completed!(self.force_operand(ty, depth));
        let members = match self.store.kind(ty).clone() {
            TypeKind::Union(members) => members,
            TypeKind::Never | TypeKind::Error | TypeKind::Invalid(_) => {
                return Completion::Complete(ty);
            }
            _ => vec![ty],
        };
        let mut mapped = Vec::new();
        for member in members {
            if let Some(member) = completed!(map(self, member, depth + 1)) {
                mapped.push(member);
            }
        }
        Completion::Complete(self.store.union(mapped, UnionPolicy::Canonical))
    }
}
