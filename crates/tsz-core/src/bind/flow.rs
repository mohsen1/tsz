use rustc_hash::{FxHashMap, FxHashSet};

use crate::source::{DeclId, NodeId, Span};
use crate::syntax::{
    BinaryOperator, Expression, ExpressionKind, ExpressionRoot, ExpressionTraversal, IfStatement,
    Literal, SourceUnit, Statement, StatementKind, StringLiteral, SwitchClauseKind, UnaryOperator,
    contains_matching_expression, for_each_statement_in, parse_number_literal,
};

use super::{BoundFile, Meaning, ScopeId, flow_assignment_root, simple_assignment_target};

#[derive(Debug, Clone)]
pub(super) struct PendingFlowReference {
    pub(super) expression: NodeId,
    pub(super) span: Span,
    pub(super) scope: ScopeId,
    pub(super) name: String,
    pub(super) demand: FlowDemandPath,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum PendingFlowAssignmentSource {
    Reference(NodeId),
    Literal(Literal),
    DirectCall(NodeId, NodeId),
}

impl PendingFlowAssignmentSource {
    pub(super) fn from_expression(expression: &Expression) -> Option<Self> {
        let expression = expression.peel_parentheses();
        match &expression.kind {
            ExpressionKind::Identifier {
                entity_name: true, ..
            } => Some(Self::Reference(expression.id)),
            ExpressionKind::Literal(literal) => Some(Self::Literal(literal.clone())),
            ExpressionKind::Call {
                callee,
                type_arguments: None,
                arguments,
            } => {
                let [argument] = arguments.as_slice() else {
                    return None;
                };
                let reference = |expression| match Self::from_expression(expression)? {
                    Self::Reference(expression) => Some(expression),
                    _ => None,
                };
                Some(Self::DirectCall(reference(callee)?, reference(argument)?))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingFlowMutation {
    pub(super) target: NodeId,
    pub(super) source: Option<PendingFlowAssignmentSource>,
    pub(super) control: Option<NodeId>,
    pub(super) effect_span: Span,
}

#[derive(Debug, Clone, Default)]
pub(super) struct PendingFlowFacts {
    pub(super) references: Vec<PendingFlowReference>,
    pub(super) initializers: Vec<(DeclId, Literal, Span)>,
    pub(super) mutations: Vec<PendingFlowMutation>,
    pub(super) evolving_array_declarations: Vec<DeclId>,
    pub(super) evolving_array_writes: Vec<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum FlowContainerKind {
    Ordinary,
    Creation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct FlowContainer {
    pub(super) owner: NodeId,
    pub(super) kind: FlowContainerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FlowNodeId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TypeofWitness {
    Undefined,
    Object,
    Boolean,
    Number,
    BigInt,
    String,
    Symbol,
    Function,
}

impl TypeofWitness {
    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TypeofWitnessSet(u8);

impl TypeofWitnessSet {
    pub(crate) const fn contains(self, witness: TypeofWitness) -> bool {
        self.0 & witness.bit() != 0
    }

    const fn insert(&mut self, witness: TypeofWitness) -> bool {
        let duplicate = self.contains(witness);
        self.0 |= witness.bit();
        duplicate
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlowPathSegment(pub(crate) String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FlowDemandPath {
    pub(super) known_prefix: Vec<FlowPathSegment>,
    pub(super) complete: bool,
}

impl FlowDemandPath {
    pub(super) const fn root() -> Self {
        Self {
            known_prefix: Vec::new(),
            complete: true,
        }
    }

    pub(super) fn member(mut self, name: &str) -> Self {
        self.known_prefix.insert(0, FlowPathSegment(name.into()));
        self
    }

    pub(super) fn element(mut self, index: &Expression) -> Self {
        if let Some(name) = flow_path_key(index) {
            self.known_prefix.insert(0, FlowPathSegment(name));
        } else {
            self.known_prefix.clear();
            self.complete = false;
        }
        self
    }

    const fn is_root(&self) -> bool {
        self.complete && self.known_prefix.is_empty()
    }
}

#[derive(Clone, Copy)]
struct PredicateRoute<'a> {
    path: &'a FlowDemandPath,
    unknown: FlowNodeId,
}

impl PredicateRoute<'_> {
    fn node_for(self, demand: &FlowDemandPath, supported: FlowNodeId) -> Option<FlowNodeId> {
        let shared_prefix = self.path.known_prefix.starts_with(&demand.known_prefix)
            || demand.known_prefix.starts_with(&self.path.known_prefix);
        let precise = self.path.complete
            && demand.complete
            && demand.known_prefix.starts_with(&self.path.known_prefix);
        let ambiguous = shared_prefix
            && (!demand.complete || !self.path.complete && !demand.known_prefix.is_empty());
        precise
            .then_some(supported)
            .or_else(|| ambiguous.then_some(self.unknown))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlowNarrowing {
    TruthinessCandidate(bool),
    Typeof {
        include: bool,
        values: TypeofWitnessSet,
    },
    StringLiteral {
        property: Option<String>,
        include: bool,
        values: Vec<String>,
    },
    PredicateCall {
        callee: DeclId,
        argument_index: usize,
        path: Vec<FlowPathSegment>,
        truthy: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsupportedFlowKind {
    FunctionBoundaryCapture,
    DuplicateCase,
    Fallthrough,
    NonneutralClause,
    Mutation,
    SwitchExit,
    UnsupportedCase,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlowAssignmentSource {
    Reference {
        expression: NodeId,
        declaration: DeclId,
    },
    Literal(Literal),
    DirectCall((NodeId, DeclId), (NodeId, DeclId)),
    Join(Box<[FlowAssignmentSource; 2]>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BoundFlowNode {
    Start,
    Narrowing {
        antecedent: FlowNodeId,
        subject: DeclId,
        narrowing: FlowNarrowing,
    },
    Assignment {
        antecedent: FlowNodeId,
        subject: DeclId,
        source: FlowAssignmentSource,
    },
    Unsupported {
        antecedent: FlowNodeId,
        subject: DeclId,
        kind: UnsupportedFlowKind,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct BoundFlowGraph {
    nodes: Vec<BoundFlowNode>,
    references: FxHashMap<NodeId, (DeclId, FlowNodeId)>,
    incomplete_declaration_values: FxHashSet<DeclId>,
}

impl Default for BoundFlowGraph {
    fn default() -> Self {
        Self {
            nodes: vec![BoundFlowNode::Start],
            references: FxHashMap::default(),
            incomplete_declaration_values: FxHashSet::default(),
        }
    }
}

impl BoundFlowGraph {
    pub(super) fn build(
        unit: &SourceUnit,
        bindings: &BoundFile,
        pending: &PendingFlowFacts,
        resolve_global: impl Fn(&str) -> Option<DeclId>,
    ) -> Self {
        let references = ReferenceIndex::new(bindings, &pending.references, resolve_global);
        let evolving = pending
            .evolving_array_declarations
            .iter()
            .copied()
            .collect::<FxHashSet<_>>();
        let incomplete_declaration_values = pending
            .evolving_array_writes
            .iter()
            .filter_map(|target| references.expression(*target))
            .map(|target| target.declaration)
            .filter(|declaration| evolving.contains(declaration))
            .collect();
        let mutations = MutationIndex::new(&references, &pending.mutations);
        let mut builder = FlowBuilder {
            bindings,
            references: &references,
            initializers: &pending.initializers,
            mutations: &mutations,
            graph: Self::default(),
            captured: Vec::new(),
        };
        for_each_statement_in(&unit.statements, &mut |statement| {
            builder.add_switch(statement);
            builder.add_if(statement);
        });
        for (expression, declaration, node) in builder.captured.drain(..) {
            builder
                .graph
                .references
                .insert(expression, (declaration, node));
        }
        builder.graph.incomplete_declaration_values = incomplete_declaration_values;
        builder.graph
    }

    pub(crate) fn reference_node(
        &self,
        expression: NodeId,
        declaration: DeclId,
    ) -> Option<FlowNodeId> {
        let (resolved, node) = self.references.get(&expression)?;
        (*resolved == declaration && !matches!(self.node(*node), BoundFlowNode::Start))
            .then_some(*node)
    }

    pub(crate) fn node(&self, node: FlowNodeId) -> &BoundFlowNode {
        &self.nodes[node.0 as usize]
    }

    pub(crate) fn declaration_value_is_incomplete(&self, declaration: DeclId) -> bool {
        self.incomplete_declaration_values.contains(&declaration)
    }
}

#[derive(Debug, Clone)]
struct ResolvedReference {
    expression: NodeId,
    span: Span,
    scope: ScopeId,
    declaration: DeclId,
    container: Option<FlowContainer>,
    demand: FlowDemandPath,
}

struct ReferenceIndex {
    by_expression: FxHashMap<NodeId, ResolvedReference>,
    by_container: FxHashMap<Option<FlowContainer>, Vec<ResolvedReference>>,
    by_span: Vec<ResolvedReference>,
}

impl ReferenceIndex {
    fn new(
        bindings: &BoundFile,
        pending: &[PendingFlowReference],
        resolve_global: impl Fn(&str) -> Option<DeclId>,
    ) -> Self {
        let mut by_expression = FxHashMap::default();
        let mut by_container: FxHashMap<Option<FlowContainer>, Vec<ResolvedReference>> =
            FxHashMap::default();
        let mut by_span = Vec::new();
        for reference in pending {
            let Some(declaration) = bindings
                .resolve(reference.scope, &reference.name, Meaning::Value)
                .or_else(|| resolve_global(&reference.name))
            else {
                continue;
            };
            let resolved = ResolvedReference {
                expression: reference.expression,
                span: reference.span,
                scope: reference.scope,
                declaration,
                container: bindings.scopes[reference.scope.0 as usize].flow_container,
                demand: reference.demand.clone(),
            };
            by_expression.insert(resolved.expression, resolved.clone());
            by_container
                .entry(resolved.container)
                .or_default()
                .push(resolved.clone());
            by_span.push(resolved);
        }
        for references in by_container.values_mut() {
            references.sort_by_key(|reference| reference.span.start);
        }
        by_span.sort_by_key(|reference| reference.span.start);
        Self {
            by_expression,
            by_container,
            by_span,
        }
    }

    fn expression(&self, expression: NodeId) -> Option<ResolvedReference> {
        self.by_expression.get(&expression).cloned()
    }

    fn in_container_span(
        &self,
        container: Option<FlowContainer>,
        span: Span,
    ) -> &[ResolvedReference] {
        self.by_container.get(&container).map_or(&[], |references| {
            span_range(references, span, |item| item.span)
        })
    }

    fn in_span(&self, span: Span) -> &[ResolvedReference] {
        span_range(&self.by_span, span, |item| item.span)
    }

    fn after_in_container(
        &self,
        container: Option<FlowContainer>,
        start: u32,
    ) -> &[ResolvedReference] {
        let references = self.by_container.get(&container);
        let references = references.map_or(&[][..], Vec::as_slice);
        &references[references.partition_point(|reference| reference.span.start < start)..]
    }
}

fn span_range<T>(items: &[T], span: Span, item_span: impl Fn(&T) -> Span) -> &[T] {
    let start = items.partition_point(|item| item_span(item).start < span.start);
    let end = items.partition_point(|item| item_span(item).start < span.end);
    &items[start..end]
}

fn resolve_assignment_source(
    references: &ReferenceIndex,
    source: &PendingFlowAssignmentSource,
    target: DeclId,
) -> Option<Option<FlowAssignmentSource>> {
    match source {
        PendingFlowAssignmentSource::Reference(expression) => {
            let reference = references.expression(*expression)?;
            let source = FlowAssignmentSource::Reference {
                expression: *expression,
                declaration: reference.declaration,
            };
            Some((reference.declaration != target).then_some(source))
        }
        PendingFlowAssignmentSource::Literal(literal) => {
            Some(Some(FlowAssignmentSource::Literal(literal.clone())))
        }
        PendingFlowAssignmentSource::DirectCall(callee, argument) => {
            let callee = references.expression(*callee)?;
            let argument = references.expression(*argument)?;
            (callee.declaration != target && argument.declaration != target).then_some({
                Some(FlowAssignmentSource::DirectCall(
                    (callee.expression, callee.declaration),
                    (argument.expression, argument.declaration),
                ))
            })
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedMutation {
    target: NodeId,
    span: Span,
    declaration: DeclId,
    source: Option<Option<FlowAssignmentSource>>,
    control: Option<NodeId>,
    path: FlowDemandPath,
}

#[derive(Clone)]
struct Arm(FlowNodeId, FlowNodeId, Option<FlowAssignmentSource>, bool);

struct MutationIndex {
    by_container: FxHashMap<Option<FlowContainer>, Vec<ResolvedMutation>>,
}

impl MutationIndex {
    fn new(references: &ReferenceIndex, pending: &[PendingFlowMutation]) -> Self {
        let mut by_container: FxHashMap<Option<FlowContainer>, Vec<ResolvedMutation>> =
            FxHashMap::default();
        for mutation in pending {
            let Some(target) = references.expression(mutation.target) else {
                continue;
            };
            let source = mutation.source.as_ref().and_then(|source| {
                resolve_assignment_source(references, source, target.declaration)
            });
            by_container
                .entry(target.container)
                .or_default()
                .push(ResolvedMutation {
                    target: mutation.target,
                    span: mutation.effect_span,
                    declaration: target.declaration,
                    source,
                    control: mutation.control,
                    path: target.demand.clone(),
                });
        }
        for mutations in by_container.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span.start);
        }
        Self { by_container }
    }

    fn in_container_span(
        &self,
        container: Option<FlowContainer>,
        span: Span,
    ) -> &[ResolvedMutation] {
        self.by_container.get(&container).map_or(&[], |mutations| {
            span_range(mutations, span, |item| item.span)
        })
    }
}

struct FlowBuilder<'a> {
    bindings: &'a BoundFile,
    references: &'a ReferenceIndex,
    initializers: &'a [(DeclId, Literal, Span)],
    mutations: &'a MutationIndex,
    graph: BoundFlowGraph,
    captured: Vec<(NodeId, DeclId, FlowNodeId)>,
}

fn apply_rebased_node(
    graph: &mut BoundFlowGraph,
    captured: &mut Vec<(NodeId, DeclId, FlowNodeId)>,
    reference: &ResolvedReference,
    mut node: FlowNodeId,
    capture: bool,
) {
    let (expression, declaration) = (reference.expression, reference.declaration);
    let staged = captured.iter().rposition(|entry| entry.0 == expression);
    let current = staged
        .map(|index| captured[index].2)
        .or_else(|| graph.references.get(&expression).map(|entry| entry.1))
        .unwrap_or(FlowNodeId(0));
    let mut template = graph.node(node).clone();
    match &mut template {
        BoundFlowNode::Start => node = current,
        BoundFlowNode::Narrowing { antecedent, .. }
        | BoundFlowNode::Assignment { antecedent, .. }
        | BoundFlowNode::Unsupported { antecedent, .. } => {
            *antecedent = current;
            node = FlowNodeId(graph.nodes.len() as u32);
            graph.nodes.push(template);
        }
    }
    if capture {
        let antecedent = node;
        node = FlowNodeId(graph.nodes.len() as u32);
        graph.nodes.push(BoundFlowNode::Unsupported {
            antecedent,
            subject: declaration,
            kind: UnsupportedFlowKind::FunctionBoundaryCapture,
        });
    }
    if let Some(index) = staged {
        captured[index].2 = node;
    } else if capture {
        captured.push((expression, declaration, node));
    } else {
        graph.references.insert(expression, (declaration, node));
    }
}

struct LiteralSubject {
    expression: NodeId,
    declaration: DeclId,
    property: Option<String>,
    supported: bool,
}

enum SwitchMode {
    Typeof,
    Literal(Option<String>),
    UnsupportedLiteral,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IfFlowEffects {
    SubjectOnly,
    AllMutations,
}

impl FlowBuilder<'_> {
    fn add_switch(&mut self, statement: &Statement) {
        let StatementKind::Switch(switch) = &statement.kind else {
            return;
        };
        let (subject_expression, subject, mode) =
            if let Some((expression, declaration)) = self.typeof_subject(&switch.expression) {
                (expression, declaration, SwitchMode::Typeof)
            } else {
                let Some(subject) = self.literal_subject(&switch.expression) else {
                    return;
                };
                let mode = if subject.supported {
                    SwitchMode::Literal(subject.property)
                } else {
                    SwitchMode::UnsupportedLiteral
                };
                (subject.expression, subject.declaration, mode)
            };
        let mut labels = Vec::new();
        let mut labels_supported = true;
        for clause in &switch.clauses {
            if let SwitchClauseKind::Case(expression) = &clause.kind {
                if let Some(label) = switch_label(expression, &mode) {
                    labels.push(label);
                } else {
                    labels_supported = false;
                }
            }
        }
        let container = self.container(statement.id);
        let antecedent = self.antecedent(subject_expression);
        if labels.is_empty() && labels_supported {
            let mut mutated = FxHashMap::default();
            self.assign_mutations(
                statement.span,
                container,
                statement.id,
                antecedent,
                &mut mutated,
            );
            for (declaration, flow) in mutated {
                self.assign_after(statement.span.end, container, declaration, flow.1, None);
            }
            return;
        }
        let mut seen = Vec::new();
        let mut pending = Vec::new();
        let mut pending_duplicate = false;
        let mut pending_default = false;
        let mut seen_default = false;
        let mut previous_falls_through = false;
        let mut has_return = false;
        let mut has_semantic_exit = false;
        let mut join_supported = labels_supported;
        let mut mutated = FxHashMap::default();

        for clause in &switch.clauses {
            let flow_neutral = clause
                .statements
                .iter()
                .all(|statement| statement_is_flow_neutral(statement, false));
            has_semantic_exit |= contains_matching_expression(
                ExpressionRoot::Statements(&clause.statements),
                ExpressionTraversal::Executed,
                |expression| {
                    matches!(
                        expression.kind,
                        ExpressionKind::Call { .. } | ExpressionKind::New { .. }
                    )
                },
            );
            let direct = match &clause.kind {
                SwitchClauseKind::Case(expression) => switch_label(expression, &mode),
                SwitchClauseKind::Default => None,
            };
            let duplicate = direct
                .as_ref()
                .is_some_and(|witness| seen.contains(witness))
                || matches!(clause.kind, SwitchClauseKind::Default) && seen_default;
            direct.inspect(|witness| {
                seen.push(witness.clone());
                pending.push(witness.clone());
            });
            pending_duplicate |= duplicate;
            if clause.statements.is_empty() {
                if matches!(clause.kind, SwitchClauseKind::Default) {
                    pending_default = true;
                    seen_default = true;
                }
                continue;
            }
            let narrowing = match &clause.kind {
                SwitchClauseKind::Case(_) if !pending.is_empty() && !pending_default => {
                    flow_narrowing(&mode, true, &pending)
                }
                SwitchClauseKind::Default if pending.is_empty() && !pending_default => {
                    flow_narrowing(&mode, false, &labels)
                }
                SwitchClauseKind::Case(_) | SwitchClauseKind::Default => None,
            };
            let unsupported = if !labels_supported {
                Some(UnsupportedFlowKind::UnsupportedCase)
            } else if pending_duplicate {
                Some(UnsupportedFlowKind::DuplicateCase)
            } else if !flow_neutral {
                Some(UnsupportedFlowKind::NonneutralClause)
            } else if previous_falls_through || narrowing.is_none() {
                Some(UnsupportedFlowKind::Fallthrough)
            } else {
                None
            };
            let region_supported = unsupported.is_none();
            let node = match unsupported {
                Some(kind) => self.unsupported(antecedent, subject, kind),
                None => self.narrowing(antecedent, subject, narrowing.expect("checked narrowing")),
            };
            join_supported &= region_supported;
            self.assign_region(clause.span, container, subject, node, None);
            self.assign_mutations(clause.span, container, statement.id, node, &mut mutated);
            pending.clear();
            pending_duplicate = false;
            pending_default = false;
            seen_default |= matches!(clause.kind, SwitchClauseKind::Default);
            match clause.statements.last().map(clause_exit) {
                Some(ClauseExit::Break) => previous_falls_through = false,
                Some(ClauseExit::Return | ClauseExit::Unsupported) => {
                    previous_falls_through = false;
                    has_return = true;
                }
                Some(ClauseExit::Fallthrough) | None => previous_falls_through = true,
            }
        }

        if has_return || has_semantic_exit || !join_supported {
            let exit = self.unsupported(antecedent, subject, UnsupportedFlowKind::SwitchExit);
            self.assign_after(statement.span.end, container, subject, exit, None);
        }
        for (declaration, flow) in mutated {
            self.assign_after(statement.span.end, container, declaration, flow.1, None);
        }
    }

    fn add_if(&mut self, statement: &Statement) {
        let StatementKind::If(branch) = &statement.kind else {
            return;
        };
        if self.add_predicate_if(statement, branch, &branch.condition, [true, true]) {
            return;
        }
        if let Some(subject) = self
            .literal_subject(&branch.condition)
            .filter(|subject| subject.supported && subject.property.is_none())
            .filter(|subject| !self.if_root_mutates(statement, subject.declaration))
        {
            let antecedent = self.antecedent(subject.expression);
            let nodes = [true, false].map(|truthy| {
                self.narrowing(
                    antecedent,
                    subject.declaration,
                    FlowNarrowing::TruthinessCandidate(truthy),
                )
            });
            self.assign_if_nodes(
                statement,
                subject.declaration,
                nodes,
                None,
                [true, true],
                IfFlowEffects::SubjectOnly,
            );
            return;
        }
        let ExpressionKind::Binary {
            left,
            operator,
            right,
            ..
        } = &branch.condition.peel_parentheses().kind
        else {
            return;
        };
        if *operator == BinaryOperator::LogicalAnd
            && self
                .predicate_subject(left)
                .is_some_and(|(_, path)| path.complete)
            && statement_is_flow_neutral(&branch.then_statement, true)
            && self.add_predicate_if(statement, branch, right, [true, false])
        {
            return;
        }
        let equality = match operator {
            BinaryOperator::Equals | BinaryOperator::StrictEquals => true,
            BinaryOperator::NotEquals | BinaryOperator::StrictNotEquals => false,
            _ => return,
        };
        let Some((subject, value)) = self
            .literal_subject(left)
            .map(|subject| (subject, literal_value(right)))
            .or_else(|| {
                self.literal_subject(right)
                    .map(|subject| (subject, literal_value(left)))
            })
        else {
            return;
        };
        let antecedent = self.antecedent(subject.expression);
        let node = |include| match value.clone().filter(|_| subject.supported) {
            Some(value) => self.narrowing(
                antecedent,
                subject.declaration,
                FlowNarrowing::StringLiteral {
                    property: subject.property.clone(),
                    include,
                    values: vec![value],
                },
            ),
            None => self.unsupported(
                antecedent,
                subject.declaration,
                UnsupportedFlowKind::UnsupportedCase,
            ),
        };
        let nodes = [equality, !equality].map(node);
        self.assign_if_nodes(
            statement,
            subject.declaration,
            nodes,
            None,
            [true, true],
            IfFlowEffects::AllMutations,
        );
    }

    fn assign_if_nodes(
        &mut self,
        statement: &Statement,
        subject: DeclId,
        mut nodes: [FlowNodeId; 2],
        routes: Option<[PredicateRoute<'_>; 2]>,
        active: [bool; 2],
        effects: IfFlowEffects,
    ) {
        let StatementKind::If(branch) = &statement.kind else {
            return;
        };
        let container = self.container(statement.id);
        let mut mutated = [FxHashMap::default(), FxHashMap::default()];
        for (arm, (branch, node)) in [
            Some((&*branch.then_statement, nodes[0])),
            branch
                .else_statement
                .as_deref()
                .map(|branch| (branch, nodes[1])),
        ]
        .into_iter()
        .flatten()
        .enumerate()
        {
            let route = routes.map(|routes| routes[arm]);
            if active[arm] {
                self.assign_region(branch.span, container, subject, node, route);
            }
            if effects == IfFlowEffects::AllMutations {
                self.assign_mutations(
                    branch.span,
                    container,
                    statement.id,
                    node,
                    &mut mutated[arm],
                );
                if active[arm]
                    && let Some(route) = route
                    && self.assign_path_mutations(branch.span, container, subject, route)
                {
                    nodes[arm] = route.unknown;
                }
            }
        }
        let exits = [
            clause_exit(&branch.then_statement),
            branch
                .else_statement
                .as_deref()
                .map(clause_exit)
                .unwrap_or(ClauseExit::Fallthrough),
        ];
        let survivor = match exits {
            [ClauseExit::Fallthrough, ClauseExit::Return] => Some(0),
            [ClauseExit::Return, ClauseExit::Fallthrough] => Some(1),
            _ => None,
        };
        if let Some(survivor) = survivor.filter(|arm| active[*arm]) {
            self.assign_after(
                statement.span.end,
                container,
                subject,
                nodes[survivor],
                routes.map(|routes| routes[survivor]),
            );
        }
        if effects == IfFlowEffects::SubjectOnly {
            return;
        }
        let mut seen = FxHashSet::default();
        let declarations = self
            .mutations
            .in_container_span(container, statement.span)
            .iter()
            .map(|mutation| mutation.declaration)
            .filter(|declaration| {
                seen.insert(*declaration) && mutated.iter().any(|arm| arm.contains_key(declaration))
            })
            .collect::<Vec<_>>();
        for declaration in declarations {
            let flows = std::array::from_fn(|arm| mutated[arm].get(&declaration));
            let initializer = self.initializer_source(statement, container, declaration);
            let node = match (exits, flows) {
                ([ClauseExit::Return, ClauseExit::Return], _) | (_, [None, None]) => continue,
                ([ClauseExit::Return, ClauseExit::Fallthrough], [_, Some(flow)])
                | ([ClauseExit::Fallthrough, ClauseExit::Return], [Some(flow), _]) => flow.0,
                ([ClauseExit::Return, ClauseExit::Fallthrough], [Some(_), None])
                | ([ClauseExit::Fallthrough, ClauseExit::Return], [None, Some(_)]) => initializer
                    .filter(|_| flows.into_iter().flatten().all(|flow| flow.3))
                    .map(|source| self.assignment(declaration, source))
                    .unwrap_or_else(|| {
                        self.unsupported(FlowNodeId(0), declaration, UnsupportedFlowKind::Mutation)
                    }),
                (
                    [ClauseExit::Fallthrough, ClauseExit::Fallthrough],
                    [Some(flow), None] | [None, Some(flow)],
                ) if declaration != subject => initializer
                    .filter(|_| flow.3)
                    .zip(flow.2.clone())
                    .map(|(initializer, written)| {
                        self.assignment(
                            declaration,
                            FlowAssignmentSource::Join(Box::new([initializer, written])),
                        )
                    })
                    .unwrap_or(flow.1),
                (_, [Some(flow), _] | [None, Some(flow)]) => flow.1,
            };
            self.assign_after(statement.span.end, container, declaration, node, None);
        }
    }

    fn initializer_source(
        &self,
        statement: &Statement,
        container: Option<FlowContainer>,
        declaration: DeclId,
    ) -> Option<FlowAssignmentSource> {
        let initializer = self
            .initializers
            .iter()
            .find(|initializer| initializer.0 == declaration)?;
        let span = Span {
            file: statement.span.file,
            start: initializer.2.end,
            end: statement.span.start,
        };
        let scope = self.bindings.declaration(declaration)?.scope;
        if self.bindings.scopes[scope.0 as usize].flow_container != container
            || initializer.2.end > statement.span.start
            || self
                .mutations
                .in_container_span(container, span)
                .iter()
                .any(|mutation| mutation.declaration == declaration && mutation.path.is_root())
        {
            return None;
        }
        Some(FlowAssignmentSource::Literal(initializer.1.clone()))
    }

    fn assignment(&mut self, subject: DeclId, source: FlowAssignmentSource) -> FlowNodeId {
        self.push(BoundFlowNode::Assignment {
            antecedent: FlowNodeId(0),
            subject,
            source,
        })
    }

    fn narrowing(
        &mut self,
        antecedent: FlowNodeId,
        subject: DeclId,
        narrowing: FlowNarrowing,
    ) -> FlowNodeId {
        self.push(BoundFlowNode::Narrowing {
            antecedent,
            subject,
            narrowing,
        })
    }

    fn add_predicate_if(
        &mut self,
        statement: &Statement,
        branch: &IfStatement,
        condition: &Expression,
        active: [bool; 2],
    ) -> bool {
        let ExpressionKind::Call {
            callee,
            type_arguments,
            arguments,
        } = &condition.peel_parentheses().kind
        else {
            return false;
        };
        let callee = callee.peel_parentheses();
        let Some(callee) = self
            .references
            .expression(callee.id)
            .filter(|_| matches!(callee.kind, ExpressionKind::Identifier { .. }))
        else {
            return false;
        };
        for (argument_index, argument) in arguments.iter().enumerate() {
            let argument = argument.peel_parentheses();
            let Some((subject, path)) = self.predicate_subject(argument) else {
                continue;
            };
            let antecedent = self.antecedent(subject.expression);
            let mut predicate_node = |arm: usize, truthy: Option<bool>| {
                if !active[arm] {
                    return antecedent;
                }
                self.narrowing(
                    antecedent,
                    subject.declaration,
                    FlowNarrowing::PredicateCall {
                        callee: callee.declaration,
                        argument_index,
                        path: path.known_prefix.clone(),
                        truthy,
                    },
                )
            };
            let unknown: [FlowNodeId; 2] = std::array::from_fn(|arm| predicate_node(arm, None));
            let root = path.complete && path.known_prefix.is_empty();
            let supported = [
                statement_is_flow_neutral(&branch.then_statement, true),
                branch
                    .else_statement
                    .as_deref()
                    .is_none_or(|statement| statement_is_flow_neutral(statement, true)),
            ]
            .map(|neutral| type_arguments.is_none() && path.complete && (root || neutral));
            let nodes = std::array::from_fn(|arm| {
                if supported[arm] {
                    predicate_node(arm, Some(arm == 0))
                } else {
                    unknown[arm]
                }
            });
            let routes = std::array::from_fn(|arm| PredicateRoute {
                path: &path,
                unknown: unknown[arm],
            });
            self.assign_if_nodes(
                statement,
                subject.declaration,
                nodes,
                Some(routes),
                active,
                IfFlowEffects::AllMutations,
            );
        }
        true
    }

    fn assign_after(
        &mut self,
        start: u32,
        container: Option<FlowContainer>,
        declaration: DeclId,
        node: FlowNodeId,
        route: Option<PredicateRoute<'_>>,
    ) {
        for reference in self.references.after_in_container(container, start) {
            if reference.declaration == declaration {
                let Some(node) =
                    route.map_or(Some(node), |route| route.node_for(&reference.demand, node))
                else {
                    continue;
                };
                apply_rebased_node(&mut self.graph, &mut self.captured, reference, node, false);
            }
        }
    }

    fn typeof_subject(&self, expression: &Expression) -> Option<(NodeId, DeclId)> {
        let expression = expression.peel_parentheses();
        let ExpressionKind::Unary {
            operator: UnaryOperator::TypeOf,
            operand,
        } = &expression.kind
        else {
            return None;
        };
        let expression = operand.peel_parentheses();
        let reference = self.references.expression(expression.id)?;
        Some((expression.id, reference.declaration))
    }

    fn literal_subject(&self, expression: &Expression) -> Option<LiteralSubject> {
        let expression = expression.peel_parentheses();
        let (object, property, supported) = match &expression.kind {
            ExpressionKind::Identifier { .. } => (expression, None, true),
            ExpressionKind::Member { object, name, .. } => (
                object.peel_parentheses(),
                Some(name.clone()),
                matches!(
                    object.peel_parentheses().kind,
                    ExpressionKind::Identifier { .. }
                ),
            ),
            ExpressionKind::ElementAccess { object, index } => (
                object.peel_parentheses(),
                literal_value(index),
                matches!(
                    object.peel_parentheses().kind,
                    ExpressionKind::Identifier { .. }
                ) && literal_value(index).is_some(),
            ),
            _ => return None,
        };
        let reference = self.root_reference(object)?;
        Some(LiteralSubject {
            expression: reference.expression,
            declaration: reference.declaration,
            property,
            supported,
        })
    }

    fn root_reference(&self, expression: &Expression) -> Option<ResolvedReference> {
        let expression = expression.peel_parentheses();
        match &expression.kind {
            ExpressionKind::Identifier { .. } => self.references.expression(expression.id),
            ExpressionKind::Member { object, .. }
            | ExpressionKind::ElementAccess { object, .. } => self.root_reference(object),
            _ => None,
        }
    }

    fn predicate_subject(
        &self,
        expression: &Expression,
    ) -> Option<(ResolvedReference, FlowDemandPath)> {
        let expression = expression.peel_parentheses();
        match &expression.kind {
            ExpressionKind::Identifier { .. } => Some((
                self.references.expression(expression.id)?,
                FlowDemandPath::root(),
            )),
            ExpressionKind::Member { object, name, .. } => {
                let (subject, mut path) = self.predicate_subject(object)?;
                if path.complete {
                    path.known_prefix.push(FlowPathSegment(name.clone()));
                }
                Some((subject, path))
            }
            ExpressionKind::ElementAccess { object, index } => {
                let (subject, mut path) = self.predicate_subject(object)?;
                if path.complete {
                    match flow_path_key(index) {
                        Some(name) => path.known_prefix.push(FlowPathSegment(name)),
                        None => path.complete = false,
                    }
                }
                Some((subject, path))
            }
            _ => None,
        }
    }

    fn container(&self, node: NodeId) -> Option<FlowContainer> {
        let scope = self.bindings.scope_for_node.get(&node)?;
        self.bindings.scopes[scope.0 as usize].flow_container
    }

    fn antecedent(&self, expression: NodeId) -> FlowNodeId {
        let reference = self.graph.references.get(&expression);
        reference.map_or(FlowNodeId(0), |entry| entry.1)
    }

    fn if_root_mutates(&self, statement: &Statement, declaration: DeclId) -> bool {
        let container = self.container(statement.id);
        self.mutations
            .in_container_span(container, statement.span)
            .iter()
            .any(|mutation| mutation.declaration == declaration && mutation.path.is_root())
    }

    fn assign_region(
        &mut self,
        span: Span,
        container: Option<FlowContainer>,
        subject: DeclId,
        node: FlowNodeId,
        route: Option<PredicateRoute<'_>>,
    ) {
        for reference in self.references.in_container_span(container, span) {
            if reference.span.end <= span.end && reference.declaration == subject {
                let Some(node) =
                    route.map_or(Some(node), |route| route.node_for(&reference.demand, node))
                else {
                    continue;
                };
                apply_rebased_node(&mut self.graph, &mut self.captured, reference, node, false);
            }
        }
        for reference in self.references.in_span(span) {
            if reference.span.end <= span.end
                && reference.declaration == subject
                && self.creation_capture_reaches(reference.scope, container)
            {
                let Some(node) =
                    route.map_or(Some(node), |route| route.node_for(&reference.demand, node))
                else {
                    continue;
                };
                apply_rebased_node(&mut self.graph, &mut self.captured, reference, node, true);
            }
        }
    }

    fn assign_path_mutations(
        &mut self,
        clause: Span,
        container: Option<FlowContainer>,
        subject: DeclId,
        route: PredicateRoute<'_>,
    ) -> bool {
        let mutations = self.mutations.in_container_span(container, clause).to_vec();
        let mut invalidated = false;
        for mutation in mutations {
            if mutation.declaration != subject
                || mutation.path.is_root()
                || mutation.span.end > clause.end
            {
                continue;
            }
            let path = &mutation.path.known_prefix;
            let overlaps = route.path.known_prefix.starts_with(path);
            if (!mutation.path.complete || overlaps)
                && self.graph.references.contains_key(&mutation.target)
            {
                let node = self.graph.references[&mutation.target].1;
                let prior = route.node_for(&mutation.path, node).map_or(node, |_| {
                    match self.graph.node(node) {
                        BoundFlowNode::Narrowing { antecedent, .. } => *antecedent,
                        _ => node,
                    }
                });
                let node = [prior, route.unknown][usize::from(prior != FlowNodeId(0))];
                self.graph
                    .references
                    .insert(mutation.target, (subject, node));
            }
            if mutation.path.complete && !overlaps {
                continue;
            }
            invalidated = true;
            for reference in self.references.in_container_span(
                container,
                Span {
                    file: clause.file,
                    start: mutation.span.end,
                    end: clause.end,
                },
            ) {
                if reference.declaration == subject
                    && let Some(node) = route.node_for(&reference.demand, route.unknown)
                {
                    apply_rebased_node(&mut self.graph, &mut self.captured, reference, node, false);
                }
            }
        }
        invalidated
    }

    fn assign_mutations(
        &mut self,
        clause: Span,
        container: Option<FlowContainer>,
        control: NodeId,
        antecedent: FlowNodeId,
        mutated: &mut FxHashMap<DeclId, Arm>,
    ) {
        let mut local = FxHashMap::default();
        for mutation in self
            .mutations
            .in_container_span(container, clause)
            .iter()
            .filter(|mutation| mutation.path.is_root())
            .filter(|&mutation| mutation.span.end <= clause.end)
            .cloned()
        {
            let declaration = mutation.declaration;
            let antecedent = local.get(&declaration).copied().unwrap_or(antecedent);
            let outgoing = self.unsupported(antecedent, declaration, UnsupportedFlowKind::Mutation);
            let joinable = !local.contains_key(&declaration)
                && self
                    .graph
                    .reference_node(mutation.target, declaration)
                    .is_none();
            let preserve = mutation.source.as_ref().is_some_and(Option::is_none);
            let source = mutation
                .source
                .flatten()
                .filter(|_| mutation.control == Some(control));
            let fallback = if preserve { antecedent } else { outgoing };
            let local_node = source.clone().map_or(fallback, |source| {
                self.push(BoundFlowNode::Assignment {
                    antecedent,
                    subject: declaration,
                    source,
                })
            });
            for reference in self.references.in_container_span(
                container,
                Span {
                    file: clause.file,
                    start: mutation.span.start,
                    end: clause.end,
                },
            ) {
                if reference.declaration == declaration {
                    let node = if reference.expression == mutation.target {
                        self.graph.references.remove(&mutation.target);
                        continue;
                    } else if reference.span.start >= mutation.span.end {
                        local_node
                    } else {
                        continue;
                    };
                    self.graph
                        .references
                        .insert(reference.expression, (reference.declaration, node));
                }
            }
            local.insert(declaration, local_node);
            mutated.insert(declaration, Arm(local_node, outgoing, source, joinable));
        }
    }

    fn creation_capture_reaches(&self, mut scope: ScopeId, outer: Option<FlowContainer>) -> bool {
        let mut previous = outer;
        loop {
            let bound = &self.bindings.scopes[scope.0 as usize];
            if bound.flow_container == outer {
                return previous != outer;
            }
            if bound.flow_container != previous {
                let Some(container) = bound.flow_container else {
                    return false;
                };
                if container.kind != FlowContainerKind::Creation {
                    return false;
                }
                previous = Some(container);
            }
            let Some(parent) = bound.parent else {
                return false;
            };
            scope = parent;
        }
    }

    fn push(&mut self, node: BoundFlowNode) -> FlowNodeId {
        let id = FlowNodeId(self.graph.nodes.len() as u32);
        self.graph.nodes.push(node);
        id
    }

    fn unsupported(
        &mut self,
        antecedent: FlowNodeId,
        subject: DeclId,
        kind: UnsupportedFlowKind,
    ) -> FlowNodeId {
        self.push(BoundFlowNode::Unsupported {
            antecedent,
            subject,
            kind,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClauseExit {
    Break,
    Return,
    Fallthrough,
    Unsupported,
}

fn clause_exit(statement: &Statement) -> ClauseExit {
    match &statement.kind {
        StatementKind::Break(_) => ClauseExit::Break,
        StatementKind::Return(_) => ClauseExit::Return,
        StatementKind::Block(statements) => match statements.split_last() {
            None => ClauseExit::Fallthrough,
            Some((last, prefix))
                if prefix
                    .iter()
                    .all(|statement| clause_exit(statement) == ClauseExit::Fallthrough) =>
            {
                clause_exit(last)
            }
            Some(_) => ClauseExit::Unsupported,
        },
        StatementKind::If(_)
        | StatementKind::Switch(_)
        | StatementKind::Continue(_)
        | StatementKind::Unknown => ClauseExit::Unsupported,
        _ => ClauseExit::Fallthrough,
    }
}

fn typeof_witness(value: &str) -> Option<TypeofWitness> {
    match value {
        "undefined" => Some(TypeofWitness::Undefined),
        "object" => Some(TypeofWitness::Object),
        "boolean" => Some(TypeofWitness::Boolean),
        "number" => Some(TypeofWitness::Number),
        "bigint" => Some(TypeofWitness::BigInt),
        "string" => Some(TypeofWitness::String),
        "symbol" => Some(TypeofWitness::Symbol),
        "function" => Some(TypeofWitness::Function),
        _ => None,
    }
}

fn literal_value(expression: &Expression) -> Option<String> {
    match &expression.peel_parentheses().kind {
        ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value))) => {
            Some(value.clone())
        }
        ExpressionKind::Literal(Literal::NoSubstitutionTemplate(value)) => {
            Some(value.cooked.clone())
        }
        _ => None,
    }
}

fn flow_path_key(expression: &Expression) -> Option<String> {
    match &expression.peel_parentheses().kind {
        ExpressionKind::Literal(Literal::Number(value)) if value.validation_supported() => {
            parse_number_literal(value.semantic_text()).map(|value| value.display)
        }
        _ => literal_value(expression),
    }
}

fn switch_label(expression: &Expression, mode: &SwitchMode) -> Option<String> {
    let value = literal_value(expression)?;
    (!matches!(mode, SwitchMode::Typeof) || typeof_witness(&value).is_some()).then_some(value)
}

fn flow_narrowing(mode: &SwitchMode, include: bool, labels: &[String]) -> Option<FlowNarrowing> {
    match mode {
        SwitchMode::Typeof => {
            let mut values = TypeofWitnessSet::default();
            for label in labels {
                values.insert(typeof_witness(label)?);
            }
            Some(FlowNarrowing::Typeof { include, values })
        }
        SwitchMode::Literal(property) => Some(FlowNarrowing::StringLiteral {
            property: property.clone(),
            include,
            values: labels.to_vec(),
        }),
        SwitchMode::UnsupportedLiteral => None,
    }
}

fn statement_is_flow_neutral(statement: &Statement, path_writes: bool) -> bool {
    match &statement.kind {
        StatementKind::Import(_)
        | StatementKind::Function(_)
        | StatementKind::Class(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Break(_)
        | StatementKind::Empty => true,
        StatementKind::Export(declaration) => declaration
            .assignment
            .as_ref()
            .is_none_or(|value| expression_is_flow_neutral(value, path_writes)),
        StatementKind::Variable(declaration) => declaration
            .initializer
            .as_ref()
            .is_none_or(|value| expression_is_flow_neutral(value, path_writes)),
        StatementKind::Return(expression) => expression
            .as_ref()
            .is_none_or(|value| expression_is_flow_neutral(value, path_writes)),
        StatementKind::Expression(expression) => {
            expression_is_flow_neutral(expression, path_writes)
        }
        StatementKind::Block(statements) => statements
            .iter()
            .all(|statement| statement_is_flow_neutral(statement, path_writes)),
        StatementKind::If(statement) if path_writes => {
            expression_is_flow_neutral(&statement.condition, path_writes)
                && statement_is_flow_neutral(&statement.then_statement, path_writes)
                && statement
                    .else_statement
                    .as_deref()
                    .is_none_or(|arm| statement_is_flow_neutral(arm, path_writes))
        }
        StatementKind::If(_)
        | StatementKind::Switch(_)
        | StatementKind::Continue(_)
        | StatementKind::Unknown => false,
    }
}

fn expression_is_flow_neutral(expression: &Expression, path_writes: bool) -> bool {
    match &expression.kind {
        ExpressionKind::Identifier { .. }
        | ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::FunctionLike(_) => true,
        ExpressionKind::Object(properties) => properties
            .iter()
            .all(|property| expression_is_flow_neutral(&property.value, path_writes)),
        ExpressionKind::Array(elements) => elements
            .iter()
            .all(|element| expression_is_flow_neutral(element, path_writes)),
        ExpressionKind::Call {
            callee, arguments, ..
        }
        | ExpressionKind::New {
            callee, arguments, ..
        } => {
            !matches!(
                &callee.peel_parentheses_and_assertions().kind,
                ExpressionKind::FunctionLike(_)
            ) && expression_is_flow_neutral(callee, path_writes)
                && arguments
                    .iter()
                    .all(|argument| expression_is_flow_neutral(argument, path_writes))
        }
        ExpressionKind::Member { object, .. }
        | ExpressionKind::Unary {
            operand: object, ..
        }
        | ExpressionKind::As {
            expression: object, ..
        }
        | ExpressionKind::Parenthesized(object) => expression_is_flow_neutral(object, path_writes),
        ExpressionKind::ElementAccess { object, index }
        | ExpressionKind::Binary {
            left: object,
            right: index,
            ..
        } => {
            expression_is_flow_neutral(object, path_writes)
                && expression_is_flow_neutral(index, path_writes)
        }
        ExpressionKind::Assignment { left, right, .. } => {
            (simple_assignment_target(left).is_some()
                || path_writes && flow_assignment_root(left).is_some())
                && expression_is_flow_neutral(right, path_writes)
        }
        ExpressionKind::Missing => false,
    }
}
