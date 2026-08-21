use std::collections::HashSet;

use crate::bind::Meaning;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};
use crate::source::DeclId;
use crate::syntax::{
    Parameter, TypeAliasDeclaration, TypeMember, TypeMemberKind, TypeNode, TypeNodeKind,
    TypeParameterDeclaration,
};

use super::object_shape::{plain_required_property, plain_type_parameters};
use super::{Checker, DeclarationModel};

/// The semantic owner whose query is following symbolic references.
///
/// Demand is part of the key so an assumption made while assembling a shape
/// cannot become an answer for a required-type, display, or relation query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReferenceDemand {
    ShapeSupport,
    RequiredType,
    AuthoredDisplay,
    RelationSource,
    RelationTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceExpansionKey {
    demand: ReferenceDemand,
    reference: TypeId,
    declaration: DeclId,
    arguments: Vec<TypeId>,
}

impl ReferenceExpansionKey {
    fn new(
        demand: ReferenceDemand,
        reference: TypeId,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> Self {
        Self {
            demand,
            reference,
            declaration,
            arguments: arguments.to_vec(),
        }
    }

    fn recursion_from<F>(&self, ancestor: &Self, kind: &F) -> ReferenceRecursion
    where
        F: Fn(TypeId) -> TypeKind,
    {
        if self.demand != ancestor.demand || self.declaration != ancestor.declaration {
            return ReferenceRecursion::Distinct;
        }
        if self.reference == ancestor.reference || self.arguments == ancestor.arguments {
            return ReferenceRecursion::Exact;
        }
        if arguments_generatively_expand(&ancestor.arguments, &self.arguments, kind) {
            ReferenceRecursion::Generative
        } else {
            ReferenceRecursion::Distinct
        }
    }
}

/// Query-local classification of a repeated declaration reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceRecursion {
    Distinct,
    Exact,
    Generative,
    UnsupportedGenerative,
}

/// Active reference frames for one semantic demand.
///
/// The stack has no session residency and is never a definitive cache. A
/// generative edge is only a provisional result for its owning operation;
/// callers must still validate arguments and every sibling before the root
/// query can complete.
#[derive(Debug)]
pub(crate) struct ReferenceExpansionStack {
    demand: ReferenceDemand,
    frames: Vec<ReferenceExpansionKey>,
}

/// A raw generative revisit plus its conservative positional wrapper shape.
///
/// `transform == None` means growth was structural but outside the relation
/// cutoff's currently owned transform family. Callers must fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenerativeExpansion {
    ancestor_index: usize,
    transform: Option<Vec<ExpansionTransform>>,
}

impl GenerativeExpansion {
    pub(crate) fn same_supported_transform(&self, other: &Self) -> bool {
        self.transform.is_some() && self.transform == other.transform
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExpansionTransform {
    Argument(usize),
    Array(Box<Self>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AliasRecursionProductivity {
    Acyclic,
    Productive,
    Unproductive,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AliasPathKind {
    Neutral,
    Productive,
    Unsupported,
}

impl AliasPathKind {
    const fn through_productive_boundary(self) -> Self {
        match self {
            Self::Unsupported => Self::Unsupported,
            Self::Neutral | Self::Productive => Self::Productive,
        }
    }

    const fn through_unsupported_boundary(self) -> Self {
        Self::Unsupported
    }
}

#[derive(Debug, Clone, Copy)]
struct AliasEdge {
    source: DeclId,
    target: DeclId,
    path: AliasPathKind,
}

const MAX_ALIAS_RECURSION_GRAPH: usize = 512;

impl ReferenceExpansionStack {
    pub(crate) const fn new(demand: ReferenceDemand) -> Self {
        Self {
            demand,
            frames: Vec::new(),
        }
    }

    pub(crate) fn classify<F>(
        &self,
        reference: TypeId,
        declaration: DeclId,
        arguments: &[TypeId],
        kind: &F,
    ) -> ReferenceRecursion
    where
        F: Fn(TypeId) -> TypeKind,
    {
        let current = ReferenceExpansionKey::new(self.demand, reference, declaration, arguments);
        let mut exact = false;
        for ancestor in self.frames.iter().rev() {
            match current.recursion_from(ancestor, kind) {
                ReferenceRecursion::Generative => return ReferenceRecursion::Generative,
                ReferenceRecursion::UnsupportedGenerative => unreachable!(
                    "structural recursion classification has no declaration capability"
                ),
                ReferenceRecursion::Exact => exact = true,
                ReferenceRecursion::Distinct => {}
            }
        }
        if exact {
            ReferenceRecursion::Exact
        } else {
            ReferenceRecursion::Distinct
        }
    }

    pub(crate) fn generative_expansion<F>(
        &self,
        reference: TypeId,
        declaration: DeclId,
        arguments: &[TypeId],
        kind: &F,
    ) -> Option<GenerativeExpansion>
    where
        F: Fn(TypeId) -> TypeKind,
    {
        let current = ReferenceExpansionKey::new(self.demand, reference, declaration, arguments);
        self.frames
            .iter()
            .enumerate()
            .rev()
            .find_map(|(ancestor_index, ancestor)| {
                matches!(
                    current.recursion_from(ancestor, kind),
                    ReferenceRecursion::Generative
                )
                .then(|| GenerativeExpansion {
                    ancestor_index,
                    transform: positional_array_transform(
                        &ancestor.arguments,
                        &current.arguments,
                        kind,
                    ),
                })
            })
    }

    pub(crate) const fn checkpoint(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn expansion_segment_supports<F>(
        &self,
        expansion: &GenerativeExpansion,
        supported: F,
    ) -> bool
    where
        F: Fn(DeclId, &[TypeId]) -> bool,
    {
        self.frames[expansion.ancestor_index..]
            .iter()
            .all(|frame| supported(frame.declaration, &frame.arguments))
    }

    pub(crate) fn push(&mut self, reference: TypeId, declaration: DeclId, arguments: &[TypeId]) {
        self.frames.push(ReferenceExpansionKey::new(
            self.demand,
            reference,
            declaration,
            arguments,
        ));
    }

    pub(crate) fn restore(&mut self, checkpoint: usize) {
        debug_assert!(checkpoint <= self.frames.len());
        self.frames.truncate(checkpoint);
    }
}

impl Checker<'_> {
    pub(super) fn evaluate_type_alias_reference(
        &mut self,
        declaration: DeclId,
        alias: &TypeAliasDeclaration,
        scope: crate::bind::ScopeId,
        arguments: &[TypeId],
    ) -> Completion<TypeId> {
        match self.alias_recursion_productivity(declaration) {
            AliasRecursionProductivity::Unproductive => {
                self.report_deferred_cycle(&DeferredType::Reference {
                    declaration,
                    arguments: arguments.to_vec(),
                });
                return Completion::Cycle;
            }
            AliasRecursionProductivity::Unsupported => return Completion::Deferred,
            AliasRecursionProductivity::Acyclic | AliasRecursionProductivity::Productive => {}
        }
        let parameters = self.substitution(declaration, &alias.type_parameters, arguments);
        Completion::Complete(self.resolve_type_node(
            declaration.file,
            scope,
            &alias.ty,
            &parameters,
        ))
    }

    pub(super) fn alias_recursion_productivity(&self, root: DeclId) -> AliasRecursionProductivity {
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        let mut declarations = Vec::new();
        let mut edges = Vec::new();

        while let Some(declaration) = pending.pop() {
            if !seen.insert(declaration) {
                continue;
            }
            if declarations.len() == MAX_ALIAS_RECURSION_GRAPH {
                return AliasRecursionProductivity::Unsupported;
            }
            let Some(DeclarationModel::TypeAlias {
                declaration: alias,
                scope,
            }) = self.models.get(&declaration).copied()
            else {
                continue;
            };
            declarations.push(declaration);
            if !self.collect_alias_edges(
                declaration,
                declaration.file,
                scope,
                &alias.type_parameters,
                &alias.ty,
                &mut edges,
            ) {
                return AliasRecursionProductivity::Unsupported;
            }
            for edge in edges.iter().filter(|edge| edge.source == declaration) {
                if !seen.contains(&edge.target) {
                    pending.push(edge.target);
                }
            }
        }

        let component = declarations
            .iter()
            .copied()
            .filter(|declaration| graph_path_exists(*declaration, root, &edges))
            .collect::<Vec<_>>();
        let component_set = component.iter().copied().collect::<HashSet<_>>();
        if graph_root_has_cycle(root, &edges, |edge| edge.path == AliasPathKind::Neutral) {
            return AliasRecursionProductivity::Unproductive;
        }
        if edges.iter().any(|unsupported| {
            unsupported.path == AliasPathKind::Unsupported
                && component_set.contains(&unsupported.source)
                && component_set.contains(&unsupported.target)
        }) {
            return AliasRecursionProductivity::Unsupported;
        }
        if graph_root_has_cycle(root, &edges, |_| true) {
            AliasRecursionProductivity::Productive
        } else {
            AliasRecursionProductivity::Acyclic
        }
    }

    /// A transparent alias reached after an authored productive boundary may
    /// return the productive root while that root is still active. Re-forcing
    /// it here would turn a legal cycle into TS2456 before the shape owner can
    /// apply its typed recursion rule. Keep that edge symbolic and uncached.
    pub(super) fn productive_alias_edge_is_provisional(
        &self,
        source: &DeferredType,
        reference: TypeId,
    ) -> bool {
        let TypeKind::Deferred(DeferredType::Reference {
            declaration,
            arguments,
        }) = self.store.kind(reference)
        else {
            return false;
        };
        let target_is_productive_alias = matches!(
            self.models.get(declaration),
            Some(DeclarationModel::TypeAlias { .. })
        ) && self.alias_recursion_productivity(*declaration)
            == AliasRecursionProductivity::Productive;
        let source_is_productive_alias = matches!(
            source,
            DeferredType::Reference {
                declaration: source,
                ..
            } if matches!(
                self.models.get(source),
                Some(DeclarationModel::TypeAlias { .. })
            ) && self.alias_recursion_productivity(*source)
                == AliasRecursionProductivity::Productive
        );
        let target_is_productive_wrapper = matches!(
            self.models.get(declaration),
            Some(DeclarationModel::Interface { .. } | DeclarationModel::Class { .. })
        ) || self
            .program
            .standard_library
            .is_rest_array_type(*declaration);
        (target_is_productive_alias || source_is_productive_alias && target_is_productive_wrapper)
            && matches!(
                self.force_reference_stack
                    .classify(reference, *declaration, arguments, &|ty| self
                        .store
                        .kind(ty)
                        .clone(),),
                ReferenceRecursion::Exact
            )
    }

    fn collect_alias_edges<'a>(
        &self,
        source: DeclId,
        file: crate::source::FileId,
        scope: crate::bind::ScopeId,
        type_parameters: &'a [TypeParameterDeclaration],
        root: &'a TypeNode,
        edges: &mut Vec<AliasEdge>,
    ) -> bool {
        let shadows = type_parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>();
        let mut pending = vec![(root, AliasPathKind::Neutral, shadows)];
        while let Some((node, path, shadows)) = pending.pop() {
            if edges.len() >= MAX_ALIAS_RECURSION_GRAPH {
                return false;
            }
            match &node.kind {
                TypeNodeKind::Array(inner) => {
                    pending.push((inner, path.through_productive_boundary(), shadows));
                }
                TypeNodeKind::Tuple(elements) => {
                    pending.extend(elements.iter().map(|element| {
                        (element, path.through_productive_boundary(), shadows.clone())
                    }));
                }
                TypeNodeKind::Union(elements) | TypeNodeKind::Intersection(elements) => {
                    pending.extend(
                        elements
                            .iter()
                            .map(|element| (element, path, shadows.clone())),
                    );
                }
                TypeNodeKind::Object(members) => push_alias_member_types(
                    members,
                    path.through_productive_boundary(),
                    &shadows,
                    &mut pending,
                ),
                TypeNodeKind::Function {
                    type_parameters,
                    parameters,
                    return_type,
                    ..
                }
                | TypeNodeKind::Constructor {
                    type_parameters,
                    parameters,
                    return_type,
                    ..
                } => {
                    let productive = path.through_productive_boundary();
                    let signature_shadows = with_alias_type_parameters(&shadows, type_parameters);
                    push_alias_type_parameter_types(
                        type_parameters,
                        productive,
                        &signature_shadows,
                        &mut pending,
                    );
                    push_alias_parameter_types(
                        parameters,
                        productive,
                        &signature_shadows,
                        &mut pending,
                    );
                    pending.push((return_type, productive, signature_shadows));
                }
                TypeNodeKind::Reference {
                    name, arguments, ..
                } => {
                    if shadows.iter().any(|shadow| *shadow == name) {
                        pending.extend(arguments.iter().map(|argument| {
                            (argument, AliasPathKind::Unsupported, shadows.clone())
                        }));
                        continue;
                    }
                    let target = self.resolve_name(file, scope, name, Meaning::Type);
                    let argument_path = match target.and_then(|target| {
                        self.models
                            .get(&target)
                            .copied()
                            .map(|model| (target, model))
                    }) {
                        Some((target, DeclarationModel::TypeAlias { .. })) => {
                            let edge_path = if self.is_single_type_symbol_declaration(target) {
                                path
                            } else {
                                AliasPathKind::Unsupported
                            };
                            edges.push(AliasEdge {
                                source,
                                target,
                                path: edge_path,
                            });
                            edge_path
                        }
                        Some((
                            _,
                            DeclarationModel::Interface { .. } | DeclarationModel::Class { .. },
                        )) => path.through_productive_boundary(),
                        None if target.is_some_and(|target| {
                            self.program.standard_library.is_rest_array_type(target)
                        }) =>
                        {
                            path.through_productive_boundary()
                        }
                        Some((_, _)) | None => path.through_unsupported_boundary(),
                    };
                    pending.extend(
                        arguments
                            .iter()
                            .map(|argument| (argument, argument_path, shadows.clone())),
                    );
                }
                TypeNodeKind::Readonly(inner) | TypeNodeKind::Parenthesized(inner) => {
                    pending.push((inner, path, shadows));
                }
                TypeNodeKind::Infer {
                    name, constraint, ..
                } => {
                    let mut infer_shadows = shadows;
                    infer_shadows.push(name);
                    pending.extend(constraint.iter().map(|constraint| {
                        (
                            constraint.as_ref(),
                            AliasPathKind::Unsupported,
                            infer_shadows.clone(),
                        )
                    }));
                }
                TypeNodeKind::Predicate { ty, .. } => pending.extend(
                    ty.iter()
                        .map(|ty| (ty.as_ref(), AliasPathKind::Unsupported, shadows.clone())),
                ),
                TypeNodeKind::KeyOf(inner) => {
                    pending.push((inner, AliasPathKind::Unsupported, shadows));
                }
                TypeNodeKind::Conditional {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                } => {
                    pending.push((check_type, AliasPathKind::Unsupported, shadows.clone()));
                    pending.push((extends_type, AliasPathKind::Unsupported, shadows.clone()));
                    let mut true_shadows = shadows.clone();
                    collect_conditional_infer_names(extends_type, &mut true_shadows);
                    pending.push((true_type, AliasPathKind::Unsupported, true_shadows));
                    pending.push((false_type, AliasPathKind::Unsupported, shadows));
                }
                TypeNodeKind::Mapped {
                    parameter,
                    constraint,
                    name_type,
                    value_type,
                    members,
                    ..
                } => {
                    pending.push((constraint, AliasPathKind::Unsupported, shadows.clone()));
                    let mut mapped_shadows = shadows;
                    mapped_shadows.push(parameter);
                    pending.extend(name_type.iter().map(|ty| {
                        (
                            ty.as_ref(),
                            AliasPathKind::Unsupported,
                            mapped_shadows.clone(),
                        )
                    }));
                    pending.push((
                        value_type,
                        AliasPathKind::Unsupported,
                        mapped_shadows.clone(),
                    ));
                    push_alias_member_types(
                        members,
                        AliasPathKind::Unsupported,
                        &mapped_shadows,
                        &mut pending,
                    );
                }
                TypeNodeKind::IndexedAccess { object, index } => {
                    pending.extend(
                        [object, index]
                            .into_iter()
                            .map(|ty| (ty.as_ref(), AliasPathKind::Unsupported, shadows.clone())),
                    );
                }
                TypeNodeKind::Keyword(_)
                | TypeNodeKind::Literal(_)
                | TypeNodeKind::TypeQuery { .. }
                | TypeNodeKind::Missing => {}
            }
        }
        true
    }

    /// Whether a generative assumption is admissible for this declaration.
    ///
    /// This capability is intentionally stricter than structural containment:
    /// only a sole, non-heritage interface declaration with exact plain type
    /// parameters and supported required properties can provisionally close
    /// a growing edge. Unsupported or merged origins stay Deferred and never
    /// reach the definitive force cache.
    pub(super) fn generative_reference_supported(
        &self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> bool {
        if !self.is_single_interface_declaration(declaration) {
            return false;
        }
        let Some(DeclarationModel::Interface {
            declaration: interface,
            ..
        }) = self.models.get(&declaration).copied()
        else {
            return false;
        };
        if !interface.extends.is_empty()
            || interface.type_parameters.len() != arguments.len()
            || !plain_type_parameters(&interface.type_parameters)
            || interface.members.is_empty()
        {
            return false;
        }
        let mut property_names = HashSet::new();
        interface.members.iter().all(|member| {
            plain_required_property(member)
                && matches!(
                    &member.kind,
                    TypeMemberKind::Property { name, .. }
                        if name
                            .semantic_name()
                            .is_some_and(|name| property_names.insert(name.to_string()))
                )
        })
    }

    pub(super) fn reference_expansion_frame_supported(
        &self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> bool {
        self.generative_reference_supported(declaration, arguments)
            || self.narrow_interface_heritage_reference_supported(declaration, arguments)
    }

    /// Relation cutoffs need a stronger proof than shape admission. A finite
    /// sibling can become incompatible only after another generic expansion,
    /// so this checkpoint admits only the single recursive-property shape
    /// whose authored edge exactly matches the positional array transform.
    pub(super) fn generative_relation_frame_supported(
        &self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> bool {
        if !self.generative_reference_supported(declaration, arguments) {
            return false;
        }
        let Some(DeclarationModel::Interface {
            declaration: interface,
            scope,
        }) = self.models.get(&declaration).copied()
        else {
            return false;
        };
        let ([parameter], [member]) = (
            interface.type_parameters.as_slice(),
            interface.members.as_slice(),
        ) else {
            return false;
        };
        let TypeMemberKind::Property { ty: Some(ty), .. } = &member.kind else {
            return false;
        };
        let TypeNodeKind::Reference {
            name,
            arguments: next_arguments,
            ..
        } = &ty.kind
        else {
            return false;
        };
        let [next_argument] = next_arguments.as_slice() else {
            return false;
        };
        self.resolve_name(declaration.file, scope, name, Meaning::Type) == Some(declaration)
            && matches!(
                &next_argument.kind,
                TypeNodeKind::Array(element)
                    if matches!(
                        &element.kind,
                        TypeNodeKind::Reference { name, arguments, .. }
                            if name == &parameter.name && arguments.is_empty()
                    )
            )
    }

    /// Classify the reference currently being admitted into an object shape.
    ///
    /// Only active `Computing` force frames participate. `Ready` entries are
    /// definitive cache results and cannot supply a provisional assumption.
    pub(super) fn shape_reference_recursion(
        &self,
        reference: TypeId,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> ReferenceRecursion {
        let kind = |ty| self.store.kind(ty).clone();
        if let Some(expansion) = self.force_reference_stack.generative_expansion(
            reference,
            declaration,
            arguments,
            &kind,
        ) {
            return if self.generative_reference_supported(declaration, arguments)
                && self.force_reference_stack.expansion_segment_supports(
                    &expansion,
                    |frame_declaration, frame_arguments| {
                        self.reference_expansion_frame_supported(frame_declaration, frame_arguments)
                    },
                ) {
                ReferenceRecursion::Generative
            } else {
                ReferenceRecursion::UnsupportedGenerative
            };
        }
        self.force_reference_stack
            .classify(reference, declaration, arguments, &kind)
    }
}

type AliasPending<'a> = (&'a TypeNode, AliasPathKind, Vec<&'a str>);

fn with_alias_type_parameters<'a>(
    outer: &[&'a str],
    parameters: &'a [TypeParameterDeclaration],
) -> Vec<&'a str> {
    outer
        .iter()
        .copied()
        .chain(parameters.iter().map(|parameter| parameter.name.as_str()))
        .collect()
}

fn push_alias_type_parameter_types<'a>(
    parameters: &'a [TypeParameterDeclaration],
    _path: AliasPathKind,
    shadows: &[&'a str],
    pending: &mut Vec<AliasPending<'a>>,
) {
    for parameter in parameters {
        pending.extend(
            parameter
                .constraint
                .iter()
                .chain(&parameter.default)
                .map(|ty| (ty, AliasPathKind::Unsupported, shadows.to_vec())),
        );
    }
}

fn push_alias_parameter_types<'a>(
    parameters: &'a [Parameter],
    path: AliasPathKind,
    shadows: &[&'a str],
    pending: &mut Vec<AliasPending<'a>>,
) {
    pending.extend(
        parameters
            .iter()
            .filter_map(|parameter| parameter.annotation.as_ref())
            .map(|annotation| (annotation, path, shadows.to_vec())),
    );
}

fn push_alias_member_types<'a>(
    members: &'a [TypeMember],
    path: AliasPathKind,
    shadows: &[&'a str],
    pending: &mut Vec<AliasPending<'a>>,
) {
    for member in members {
        match &member.kind {
            TypeMemberKind::Property { ty, .. } => {
                pending.extend(ty.iter().map(|ty| (ty, path, shadows.to_vec())));
            }
            TypeMemberKind::Method {
                type_parameters,
                parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Call {
                type_parameters,
                parameters,
                return_type,
            }
            | TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
            } => {
                let member_shadows = with_alias_type_parameters(shadows, type_parameters);
                push_alias_type_parameter_types(type_parameters, path, &member_shadows, pending);
                push_alias_parameter_types(parameters, path, &member_shadows, pending);
                pending.extend(
                    return_type
                        .iter()
                        .map(|ty| (ty, path, member_shadows.clone())),
                );
            }
            TypeMemberKind::Accessor {
                parameters,
                return_type,
                ..
            } => {
                push_alias_parameter_types(parameters, path, shadows, pending);
                pending.extend(return_type.iter().map(|ty| (ty, path, shadows.to_vec())));
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                push_alias_parameter_types(parameters, path, shadows, pending);
                pending.extend(value_type.iter().map(|ty| (ty, path, shadows.to_vec())));
            }
        }
    }
}

fn collect_conditional_infer_names<'a>(root: &'a TypeNode, names: &mut Vec<&'a str>) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match &node.kind {
            TypeNodeKind::Infer { name, .. } => {
                if !names.iter().any(|candidate| *candidate == name) {
                    names.push(name);
                }
            }
            TypeNodeKind::Array(child)
            | TypeNodeKind::KeyOf(child)
            | TypeNodeKind::Readonly(child)
            | TypeNodeKind::Parenthesized(child) => pending.push(child),
            TypeNodeKind::Tuple(children)
            | TypeNodeKind::Union(children)
            | TypeNodeKind::Intersection(children) => pending.extend(children),
            TypeNodeKind::Object(members) => push_infer_member_nodes(members, &mut pending),
            TypeNodeKind::Function {
                parameters,
                return_type,
                ..
            }
            | TypeNodeKind::Constructor {
                parameters,
                return_type,
                ..
            } => {
                pending.extend(
                    parameters
                        .iter()
                        .filter_map(|parameter| parameter.annotation.as_ref()),
                );
                pending.push(return_type);
            }
            TypeNodeKind::Reference { arguments, .. } => pending.extend(arguments),
            TypeNodeKind::Predicate { ty, .. } => {
                pending.extend(ty.iter().map(Box::as_ref));
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                pending.push(constraint);
                pending.extend(name_type.iter().map(Box::as_ref));
                pending.push(value_type);
                push_infer_member_nodes(members, &mut pending);
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                pending.extend([object.as_ref(), index.as_ref()]);
            }
            // A nested conditional owns its own inferred parameters.
            TypeNodeKind::Conditional { .. }
            | TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Missing => {}
        }
    }
}

fn push_infer_member_nodes<'a>(members: &'a [TypeMember], pending: &mut Vec<&'a TypeNode>) {
    for member in members {
        match &member.kind {
            TypeMemberKind::Property { ty, .. } => pending.extend(ty),
            TypeMemberKind::Method {
                parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Accessor {
                parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Call {
                parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Construct {
                parameters,
                return_type,
                ..
            } => {
                pending.extend(
                    parameters
                        .iter()
                        .filter_map(|parameter| parameter.annotation.as_ref()),
                );
                pending.extend(return_type);
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                pending.extend(
                    parameters
                        .iter()
                        .filter_map(|parameter| parameter.annotation.as_ref()),
                );
                pending.extend(value_type);
            }
        }
    }
}

fn graph_root_has_cycle(
    root: DeclId,
    edges: &[AliasEdge],
    include: impl Fn(&AliasEdge) -> bool,
) -> bool {
    let mut pending = edges
        .iter()
        .filter(|edge| edge.source == root && include(edge))
        .map(|edge| edge.target)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == root {
            return true;
        }
        if seen.insert(current) {
            pending.extend(
                edges
                    .iter()
                    .filter(|edge| edge.source == current && include(edge))
                    .map(|edge| edge.target),
            );
        }
    }
    false
}

fn graph_path_exists(start: DeclId, target: DeclId, edges: &[AliasEdge]) -> bool {
    let mut pending = vec![start];
    let mut seen = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        if seen.insert(current) {
            pending.extend(
                edges
                    .iter()
                    .filter(|edge| edge.source == current)
                    .map(|edge| edge.target),
            );
        }
    }
    false
}

fn positional_array_transform<F>(
    ancestor: &[TypeId],
    current: &[TypeId],
    kind: &F,
) -> Option<Vec<ExpansionTransform>>
where
    F: Fn(TypeId) -> TypeKind,
{
    if ancestor.len() != current.len() {
        return None;
    }
    current
        .iter()
        .enumerate()
        .map(|(index, current)| expansion_transform(*current, ancestor, index, kind))
        .collect()
}

fn expansion_transform<F>(
    current: TypeId,
    ancestor: &[TypeId],
    position: usize,
    kind: &F,
) -> Option<ExpansionTransform>
where
    F: Fn(TypeId) -> TypeKind,
{
    if current == ancestor[position] {
        return Some(ExpansionTransform::Argument(position));
    }
    match kind(current) {
        TypeKind::Array(element) => expansion_transform(element, ancestor, position, kind)
            .map(|child| ExpansionTransform::Array(Box::new(child))),
        _ => None,
    }
}

/// Whether every prior argument survives inside the new argument graph and at
/// least one survives below a newly introduced structural wrapper.
///
/// This direction distinguishes `T -> T[]` from finite authored shrinking
/// such as `Box<Box<Fn>> -> Box<Fn> -> Fn`. It also rejects mere argument
/// permutation as growth. The walk is iterative and keyed only by session-
/// local type identities and declaration structure.
fn arguments_generatively_expand<F>(ancestor: &[TypeId], current: &[TypeId], kind: &F) -> bool
where
    F: Fn(TypeId) -> TypeKind,
{
    if ancestor.is_empty() || current.is_empty() {
        return false;
    }
    let mut introduced_wrapper = false;
    for prior in ancestor {
        let mut found = false;
        for next in current {
            if next == prior {
                found = true;
            } else if type_contains_nested(*next, *prior, kind) {
                found = true;
                introduced_wrapper = true;
            }
        }
        if !found {
            return false;
        }
    }
    introduced_wrapper
}

fn type_contains_nested<F>(root: TypeId, needle: TypeId, kind: &F) -> bool
where
    F: Fn(TypeId) -> TypeKind,
{
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        if ty == needle && ty != root {
            return true;
        }
        push_type_children(kind(ty), &mut pending);
    }
    false
}

fn push_type_children(kind: TypeKind, pending: &mut Vec<TypeId>) {
    match kind {
        TypeKind::Invalid(crate::semantics::types::InvalidType::MissingProperty {
            object, ..
        })
        | TypeKind::Invalid(crate::semantics::types::InvalidType::MissingProperties {
            object,
            ..
        })
        | TypeKind::Array(object) => pending.push(object),
        TypeKind::Tuple(children)
        | TypeKind::Union(children)
        | TypeKind::Intersection(children) => pending.extend(children),
        TypeKind::Object(shape)
        | TypeKind::ClassInstance {
            properties: shape, ..
        } => {
            pending.extend(shape.properties.into_iter().map(|property| property.ty));
            for signature in shape
                .call_signatures
                .into_iter()
                .chain(shape.construct_signatures)
            {
                pending.extend(
                    signature
                        .parameters
                        .into_iter()
                        .map(|parameter| parameter.ty),
                );
                pending.push(signature.return_type);
            }
            pending.extend(shape.index_signatures.into_iter().map(|index| index.value));
        }
        TypeKind::Function(signature) => {
            pending.extend(
                signature
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty),
            );
            pending.push(signature.return_type);
        }
        TypeKind::ShapeFunction(signature) => {
            pending.extend(
                signature
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty),
            );
            pending.push(signature.return_type);
        }
        TypeKind::Deferred(deferred) => push_deferred_children(deferred, pending),
        TypeKind::Error
        | TypeKind::Any
        | TypeKind::Unknown
        | TypeKind::Never
        | TypeKind::Void
        | TypeKind::Undefined
        | TypeKind::Null
        | TypeKind::Boolean
        | TypeKind::Number
        | TypeKind::String
        | TypeKind::BigInt
        | TypeKind::ObjectKeyword
        | TypeKind::Symbol
        | TypeKind::LiteralBoolean(_, _)
        | TypeKind::LiteralNumber(_, _)
        | TypeKind::LiteralString(_, _)
        | TypeKind::TypeParameter { .. }
        | TypeKind::ClassConstructor { .. } => {}
    }
}

fn push_deferred_children(deferred: DeferredType, pending: &mut Vec<TypeId>) {
    match deferred {
        DeferredType::Reference { arguments, .. } => pending.extend(arguments),
        DeferredType::Call { callee, .. }
        | DeferredType::Property { object: callee, .. }
        | DeferredType::Unary {
            operand: callee, ..
        }
        | DeferredType::KeyOf(callee) => pending.push(callee),
        DeferredType::Construct {
            callee,
            type_arguments,
            ..
        } => {
            pending.push(callee);
            pending.extend(type_arguments);
        }
        DeferredType::Predicate { asserted, .. } => pending.extend(asserted),
        DeferredType::Logical { left, right, .. } => pending.extend([left, right]),
        DeferredType::Conditional {
            check,
            extends,
            when_true,
            when_false,
        } => pending.extend([check, extends, when_true, when_false]),
        DeferredType::Mapped {
            constraint,
            name_type,
            value,
            ..
        } => {
            pending.extend([constraint, value]);
            pending.extend(name_type);
        }
        DeferredType::IndexedAccess { object, index } => pending.extend([object, index]),
        DeferredType::Value(_)
        | DeferredType::GenericCall
        | DeferredType::BigIntLiteral
        | DeferredType::UniqueSymbol
        | DeferredType::GenericFunction
        | DeferredType::ObjectShape => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::semantics::types::TypeKind;
    use crate::source::{DeclId, FileId};

    use super::*;

    fn parameter(declaration: DeclId, index: u32) -> TypeKind {
        TypeKind::TypeParameter {
            declaration,
            index,
            name: "ignored".to_string(),
        }
    }

    #[test]
    fn typed_growth_distinguishes_expansion_shrinking_and_permutation() {
        let declaration = DeclId {
            file: FileId(0),
            local: 0,
        };
        let other = DeclId {
            file: FileId(0),
            local: 1,
        };
        let kinds = vec![
            parameter(declaration, 0),
            TypeKind::Array(TypeId(0)),
            TypeKind::Array(TypeId(1)),
            parameter(declaration, 1),
        ];
        let kind = |ty: TypeId| kinds[ty.0 as usize].clone();
        let mut stack = ReferenceExpansionStack::new(ReferenceDemand::RequiredType);
        stack.push(TypeId(10), declaration, &[TypeId(0), TypeId(3)]);

        assert_eq!(
            stack.classify(TypeId(11), declaration, &[TypeId(1), TypeId(3)], &kind,),
            ReferenceRecursion::Generative
        );
        assert_eq!(
            stack.classify(TypeId(12), declaration, &[TypeId(3), TypeId(0)], &kind,),
            ReferenceRecursion::Distinct
        );
        assert_eq!(
            stack.classify(TypeId(13), declaration, &[TypeId(0), TypeId(3)], &kind,),
            ReferenceRecursion::Exact
        );
        assert_eq!(
            stack.classify(TypeId(14), other, &[TypeId(1), TypeId(3)], &kind,),
            ReferenceRecursion::Distinct
        );

        let mut shrinking = ReferenceExpansionStack::new(ReferenceDemand::RequiredType);
        shrinking.push(TypeId(20), declaration, &[TypeId(2)]);
        assert_eq!(
            shrinking.classify(TypeId(21), declaration, &[TypeId(1)], &kind),
            ReferenceRecursion::Distinct
        );
    }

    #[test]
    fn structural_containment_terminates_on_a_cyclic_type_graph() {
        let kinds = [TypeKind::Array(TypeId(0)), TypeKind::String];
        assert!(!type_contains_nested(TypeId(0), TypeId(1), &|ty| {
            kinds[ty.0 as usize].clone()
        }));
    }
}
