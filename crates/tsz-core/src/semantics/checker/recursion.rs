use std::collections::HashSet;

use crate::bind::Meaning;
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind, TypeStore};
use crate::source::DeclId;
use crate::syntax::{
    AuthoredTypeEdge, AuthoredTypeItem, TypeAliasDeclaration, TypeMember, TypeMemberKind, TypeNode,
    TypeNodeKind, TypeParameterDeclaration,
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
    pub(super) fn complete_type(&mut self, ty: TypeId) -> Option<TypeId> {
        let completion = self.force_type(ty, 0);
        match self.require_completion(completion) {
            Completion::Complete(ty) => Some(ty),
            Completion::Deferred | Completion::Cycle | Completion::Limit => None,
        }
    }

    pub(super) fn force_operand(&mut self, operand: TypeId, depth: usize) -> Completion<TypeId> {
        self.force_type(operand, depth)
    }

    pub(super) fn force_operands<const N: usize>(
        &mut self,
        operands: [TypeId; N],
        depth: usize,
    ) -> [Completion<TypeId>; N] {
        operands.map(|operand| self.force_operand(operand, depth))
    }

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
        let mut pending = vec![AliasPending::Type(root, AliasPathKind::Neutral, shadows)];
        while let Some(item) = pending.pop() {
            let (node, path, shadows) = match item {
                AliasPending::Type(node, path, shadows) => (node, path, shadows),
                AliasPending::Member(member, path, shadows) => {
                    let member_shadows = member.kind.signature().map_or_else(
                        || shadows.clone(),
                        |(_, parameters, _, _)| with_alias_type_parameters(&shadows, parameters),
                    );
                    let mut children = Vec::new();
                    member.push_authored_children(&mut children);
                    push_alias_children(
                        children,
                        path,
                        &member_shadows,
                        &member_shadows,
                        &mut pending,
                    );
                    continue;
                }
            };
            if edges.len() >= MAX_ALIAS_RECURSION_GRAPH {
                return false;
            }
            let mut child_path = path;
            let mut child_shadows = shadows.clone();
            let mut edge_shadows = shadows.clone();
            match &node.kind {
                TypeNodeKind::Array(_) | TypeNodeKind::Tuple(_) | TypeNodeKind::Object(_) => {
                    child_path = path.through_productive_boundary();
                }
                TypeNodeKind::Function {
                    type_parameters, ..
                }
                | TypeNodeKind::Constructor {
                    type_parameters, ..
                } => {
                    child_path = path.through_productive_boundary();
                    child_shadows = with_alias_type_parameters(&shadows, type_parameters);
                }
                TypeNodeKind::Reference { name, .. } => {
                    if shadows.iter().any(|shadow| *shadow == name) {
                        child_path = AliasPathKind::Unsupported;
                    } else {
                        let target = self.resolve_name(file, scope, name, Meaning::Type);
                        child_path = match target.and_then(|target| {
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
                    }
                }
                TypeNodeKind::Infer { name, .. } => {
                    child_path = AliasPathKind::Unsupported;
                    child_shadows.push(name);
                }
                TypeNodeKind::Conditional { extends_type, .. } => {
                    child_path = AliasPathKind::Unsupported;
                    extends_type.for_each_conditional_infer(&mut |name, _| {
                        if !edge_shadows.contains(&name) {
                            edge_shadows.push(name);
                        }
                    });
                }
                TypeNodeKind::Mapped { parameter, .. } => {
                    child_path = AliasPathKind::Unsupported;
                    child_shadows.push(parameter);
                }
                TypeNodeKind::Predicate { .. }
                | TypeNodeKind::KeyOf(_)
                | TypeNodeKind::IndexedAccess { .. } => child_path = AliasPathKind::Unsupported,
                TypeNodeKind::Union(_)
                | TypeNodeKind::Intersection(_)
                | TypeNodeKind::Readonly(_)
                | TypeNodeKind::Parenthesized(_) => {}
                TypeNodeKind::Keyword(_)
                | TypeNodeKind::Literal(_)
                | TypeNodeKind::This
                | TypeNodeKind::TypeQuery { .. }
                | TypeNodeKind::Missing => continue,
            }
            let mut children = Vec::new();
            node.push_authored_children(&mut children);
            push_alias_children(
                children,
                child_path,
                &child_shadows,
                &edge_shadows,
                &mut pending,
            );
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
            || self.plain_property_interface_heritage_reference_supported(declaration, arguments)
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

enum AliasPending<'a> {
    Type(&'a TypeNode, AliasPathKind, Vec<&'a str>),
    Member(&'a TypeMember, AliasPathKind, Vec<&'a str>),
}

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

fn push_alias_children<'a>(
    children: Vec<AuthoredTypeItem<'a>>,
    path: AliasPathKind,
    shadows: &[&'a str],
    edge_shadows: &[&'a str],
    pending: &mut Vec<AliasPending<'a>>,
) {
    for child in children {
        match child {
            AuthoredTypeItem::Type(node, edge) => {
                let child_path = if edge == AuthoredTypeEdge::TypeParameterDeclaration {
                    AliasPathKind::Unsupported
                } else {
                    path
                };
                let child_shadows = match edge {
                    AuthoredTypeEdge::ConditionalTrue | AuthoredTypeEdge::MappedConstraint => {
                        edge_shadows
                    }
                    AuthoredTypeEdge::Nested | AuthoredTypeEdge::TypeParameterDeclaration => {
                        shadows
                    }
                };
                pending.push(AliasPending::Type(node, child_path, child_shadows.to_vec()));
            }
            AuthoredTypeItem::Member(member) => {
                pending.push(AliasPending::Member(member, path, shadows.to_vec()));
            }
        }
    }
}

fn graph_root_has_cycle(
    root: DeclId,
    edges: &[AliasEdge],
    include: impl Fn(&AliasEdge) -> bool,
) -> bool {
    graph_edge_path_exists(root, root, edges, include)
}

fn graph_path_exists(start: DeclId, target: DeclId, edges: &[AliasEdge]) -> bool {
    start == target || graph_edge_path_exists(start, target, edges, |_| true)
}

fn graph_edge_path_exists(
    start: DeclId,
    target: DeclId,
    edges: &[AliasEdge],
    include: impl Fn(&AliasEdge) -> bool,
) -> bool {
    let mut pending = edges
        .iter()
        .filter(|edge| edge.source == start && include(edge))
        .map(|edge| edge.target)
        .collect::<Vec<_>>();
    let mut seen = HashSet::from([start]);
    while let Some(current) = pending.pop() {
        if current == target {
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
        TypeStore::push_type_children(&kind(ty), &mut pending);
    }
    false
}

#[cfg(test)]
#[path = "../../../rewrite-tests/checker_recursion_unit.rs"]
mod tests;
