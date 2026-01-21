//! Control Flow Analysis for type narrowing.
//!
//! This module provides flow-sensitive type analysis that walks the control flow
//! graph backwards from identifier usages to determine narrowed types.
//!
//! Example:
//! ```typescript
//! function foo(x: string | number) {
//!     if (typeof x === "string") {
//!         // FlowAnalyzer walks back and sees TRUE_CONDITION (typeof x === "string")
//!         // Returns: string (narrowed from string | number)
//!         console.log(x.length);
//!     } else {
//!         // FlowAnalyzer sees FALSE_CONDITION
//!         // Returns: number
//!         console.log(x.toFixed(2));
//!     }
//! }
//! ```

use crate::binder::BinderState;
use crate::binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags, symbol_flags};
use crate::interner::Atom;
use crate::parser::node::{BinaryExprData, CallExprData, NodeArena};
use crate::parser::{NodeIndex, NodeList, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::solver::{
    LiteralValue, NarrowingContext, ParamInfo, TypeId, TypeInterner, TypeKey, TypePredicate,
    TypePredicateTarget,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::collections::VecDeque;

// =============================================================================
// FlowGraph
// =============================================================================

/// A control flow graph that provides query methods for flow analysis.
///
/// This wraps the `FlowNodeArena` and provides convenient methods for querying
/// flow information during type checking.
pub struct FlowGraph<'a> {
    /// Reference to the flow node arena containing all flow nodes
    arena: &'a FlowNodeArena,
}

impl<'a> FlowGraph<'a> {
    /// Create a new FlowGraph from a FlowNodeArena.
    pub fn new(arena: &'a FlowNodeArena) -> Self {
        Self { arena }
    }

    /// Get a flow node by ID.
    pub fn get(&self, id: FlowNodeId) -> Option<&FlowNode> {
        self.arena.get(id)
    }

    /// Get a mutable reference to a flow node by ID.
    pub fn get_mut(&mut self, _id: FlowNodeId) -> Option<&mut FlowNode> {
        // Note: This would require interior mutability or a different API design
        // For now, we'll return None as FlowGraph is meant for querying, not modifying
        None
    }

    /// Get the number of flow nodes in the graph.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// Check if the flow graph is empty.
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Check if a flow node has a specific flag.
    pub fn node_has_flag(&self, id: FlowNodeId, flag: u32) -> bool {
        self.get(id)
            .map(|node| node.has_any_flags(flag))
            .unwrap_or(false)
    }

    /// Get the antecedents (predecessors) of a flow node.
    pub fn antecedents(&self, id: FlowNodeId) -> Vec<FlowNodeId> {
        self.get(id)
            .map(|node| node.antecedent.clone())
            .unwrap_or_default()
    }

    /// Get the AST node associated with a flow node.
    pub fn node(&self, id: FlowNodeId) -> NodeIndex {
        self.get(id)
            .map(|node| node.node)
            .unwrap_or(NodeIndex::NONE)
    }
}

// =============================================================================
// FlowAnalyzer
// =============================================================================

/// Flow analyzer for control flow-based type narrowing.
///
/// Walks the control flow graph backwards from a reference point to determine
/// what type narrowing applies at that location.
pub struct FlowAnalyzer<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    interner: &'a TypeInterner,
    node_types: Option<&'a FxHashMap<u32, TypeId>>,
    flow_graph: Option<FlowGraph<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyPresence {
    Required,
    Optional,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
enum PropertyKey {
    Atom(Atom),
    Index(usize),
}

#[derive(Clone)]
struct PredicateSignature {
    predicate: TypePredicate,
    params: Vec<ParamInfo>,
}

impl<'a> FlowAnalyzer<'a> {
    /// Create a new FlowAnalyzer.
    pub fn new(arena: &'a NodeArena, binder: &'a BinderState, interner: &'a TypeInterner) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            node_types: None,
            flow_graph,
        }
    }

    pub fn with_node_types(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        interner: &'a TypeInterner,
        node_types: &'a FxHashMap<u32, TypeId>,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            node_types: Some(node_types),
            flow_graph,
        }
    }

    /// Get a reference to the flow graph.
    pub fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }

    /// Get the narrowed type of a symbol at a specific flow node.
    ///
    /// This walks backwards through the flow graph, applying narrowing operations
    /// when it encounters condition nodes.
    pub fn get_flow_type(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        if flow_node.is_none() {
            return initial_type;
        }

        self.check_flow(reference, initial_type, flow_node, &mut Vec::new())
    }

    /// Check if a reference is definitely assigned at a specific flow node.
    pub fn is_definitely_assigned(&self, reference: NodeIndex, flow_node: FlowNodeId) -> bool {
        if flow_node.is_none() {
            return true;
        }

        let mut visited = Vec::new();
        let mut cache = FxHashMap::default();
        self.check_definite_assignment(reference, flow_node, &mut visited, &mut cache)
    }

    /// Iterative flow graph traversal using a worklist algorithm.
    ///
    /// This replaces the recursive implementation to prevent stack overflow
    /// on deeply nested control flow structures. Uses a VecDeque worklist with
    /// cycle detection to process flow nodes iteratively.
    fn check_flow(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_id: FlowNodeId,
        _visited: &mut Vec<FlowNodeId>,
    ) -> TypeId {
        // Work item: (flow_id, type_at_this_point)
        let mut worklist: VecDeque<(FlowNodeId, TypeId)> = VecDeque::new();
        let mut in_worklist: FxHashSet<FlowNodeId> = FxHashSet::default();
        let mut visited: FxHashSet<FlowNodeId> = FxHashSet::default();

        // Result cache: flow_id -> narrowed_type
        let mut results: FxHashMap<FlowNodeId, TypeId> = FxHashMap::default();

        // Initialize worklist with the entry point
        worklist.push_back((flow_id, initial_type));
        in_worklist.insert(flow_id);

        // Process worklist until empty
        while let Some((current_flow, current_type)) = worklist.pop_front() {
            in_worklist.remove(&current_flow);

            // Skip if we've already finalized this node
            if visited.contains(&current_flow) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(current_flow) else {
                // Flow node doesn't exist - use the type we have
                results.insert(current_flow, current_type);
                visited.insert(current_flow);
                continue;
            };

            // Process this flow node based on its flags
            let result_type = if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                // Branch label - union types from all antecedents
                if flow.antecedent.is_empty() {
                    current_type
                } else {
                    // Add all antecedents to worklist
                    for &ant in &flow.antecedent {
                        if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                    }
                    current_type // Will be updated when antecedents are processed
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL) {
                // Loop label - union types from entry and back-edges
                if flow.antecedent.is_empty() {
                    current_type
                } else {
                    // Add all antecedents to worklist
                    for &ant in &flow.antecedent {
                        if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                    }
                    current_type // Will be updated when antecedents are processed
                }
            } else if flow.has_any_flags(flow_flags::CONDITION) {
                // Condition node - apply narrowing
                let pre_type = if let Some(&ant) = flow.antecedent.first() {
                    // Get the result from antecedent if available, otherwise use current_type
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                };

                let is_true_branch = flow.has_any_flags(flow_flags::TRUE_CONDITION);
                self.narrow_type_by_condition(pre_type, flow.node, reference, is_true_branch)
            } else if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                // Switch clause - apply switch-specific narrowing
                self.handle_switch_clause_iterative(
                    reference,
                    current_type,
                    flow,
                    &results,
                    &mut worklist,
                    &mut in_worklist,
                    visited.clone(),
                )
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                // Assignment - check if it targets our reference
                let targets_reference =
                    self.assignment_targets_reference_node(flow.node, reference);

                if targets_reference {
                    if self.is_direct_assignment_to_reference(flow.node, reference) {
                        if let Some(assigned_type) = self.get_assigned_type(flow.node, reference) {
                            assigned_type
                        } else {
                            current_type
                        }
                    } else {
                        current_type
                    }
                } else if self.assignment_affects_reference_node(flow.node, reference) {
                    current_type
                } else if let Some(&ant) = flow.antecedent.first() {
                    // Continue to antecedent
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            } else if flow.has_any_flags(flow_flags::ARRAY_MUTATION) {
                // Array mutation
                let node = match self.arena.get(flow.node) {
                    Some(n) => n,
                    None => {
                        results.insert(current_flow, current_type);
                        visited.insert(current_flow);
                        continue;
                    }
                };
                let call = match self.arena.get_call_expr(node) {
                    Some(c) => c,
                    None => {
                        results.insert(current_flow, current_type);
                        visited.insert(current_flow);
                        continue;
                    }
                };

                if self.array_mutation_affects_reference(call, reference) {
                    current_type
                } else if let Some(&ant) = flow.antecedent.first() {
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            } else if flow.has_any_flags(flow_flags::CALL) {
                // Call expression - check for type predicates
                self.handle_call_iterative(
                    reference,
                    current_type,
                    flow,
                    &results,
                    &mut worklist,
                    &mut in_worklist,
                    &visited,
                )
            } else if flow.has_any_flags(flow_flags::START) {
                // Start node - continue to antecedent if any
                if let Some(&ant) = flow.antecedent.first()
                    && !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                current_type
            } else {
                // Default: continue to antecedent
                if let Some(&ant) = flow.antecedent.first() {
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            };

            // Store the result
            let _changed = if let Some(&existing) = results.get(&current_flow) {
                existing != result_type
            } else {
                true
            };

            results.insert(current_flow, result_type);

            // Only mark as visited if we won't need to revisit
            // For BRANCH_LABEL and LOOP_LABEL, we may need multiple passes
            if !flow.has_any_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL) {
                visited.insert(current_flow);
            }

            // If this is a branch/loop point and we've now processed all antecedents,
            // we can finalize the result by unioning
            if flow.has_any_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL) {
                // Check if all antecedents have been processed
                let all_processed = flow
                    .antecedent
                    .iter()
                    .all(|&ant| visited.contains(&ant) || results.contains_key(&ant));
                if all_processed {
                    // Union all antecedent types
                    let ant_types: Vec<TypeId> = flow
                        .antecedent
                        .iter()
                        .filter_map(|&ant| results.get(&ant).copied())
                        .collect();

                    let unioned = if ant_types.len() == 1 {
                        ant_types[0]
                    } else if !ant_types.is_empty() {
                        self.interner.union(ant_types)
                    } else {
                        current_type
                    };

                    results.insert(current_flow, unioned);
                    visited.insert(current_flow);
                }
            }
        }

        // Return the result for the initial flow_id
        results.get(&flow_id).copied().unwrap_or(initial_type)
    }

    /// Helper function for switch clause handling in iterative mode.
    fn handle_switch_clause_iterative(
        &self,
        reference: NodeIndex,
        current_type: TypeId,
        flow: &FlowNode,
        results: &FxHashMap<FlowNodeId, TypeId>,
        worklist: &mut VecDeque<(FlowNodeId, TypeId)>,
        in_worklist: &mut FxHashSet<FlowNodeId>,
        visited: FxHashSet<FlowNodeId>,
    ) -> TypeId {
        let clause_idx = flow.node;
        let Some(switch_idx) = self.binder.get_switch_for_clause(clause_idx) else {
            return current_type;
        };
        let Some(switch_node) = self.arena.get(switch_idx) else {
            return current_type;
        };
        let Some(switch_data) = self.arena.get_switch(switch_node) else {
            return current_type;
        };
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return current_type;
        };
        let Some(clause) = self.arena.get_case_clause(clause_node) else {
            return current_type;
        };

        let pre_switch_type = if let Some(&ant) = flow.antecedent.first() {
            *results.get(&ant).unwrap_or(&current_type)
        } else {
            current_type
        };

        let narrowing = NarrowingContext::new(self.interner);
        let clause_type = if clause.expression.is_none() {
            self.narrow_by_default_switch_clause(
                pre_switch_type,
                switch_data.expression,
                switch_data.case_block,
                reference,
                &narrowing,
            )
        } else {
            self.narrow_by_switch_clause(
                pre_switch_type,
                switch_data.expression,
                clause.expression,
                reference,
                &narrowing,
            )
        };

        // Handle fallthrough
        if flow.antecedent.len() > 1 {
            // Add fallthrough antecedents to worklist
            for &ant in flow.antecedent.iter().skip(1) {
                if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                    worklist.push_back((ant, current_type));
                    in_worklist.insert(ant);
                }
            }
        }

        clause_type
    }

    /// Helper function for call handling in iterative mode.
    fn handle_call_iterative(
        &self,
        reference: NodeIndex,
        current_type: TypeId,
        flow: &FlowNode,
        results: &FxHashMap<FlowNodeId, TypeId>,
        _worklist: &mut VecDeque<(FlowNodeId, TypeId)>,
        _in_worklist: &mut FxHashSet<FlowNodeId>,
        _visited: &FxHashSet<FlowNodeId>,
    ) -> TypeId {
        // Note: worklist/in_worklist/visited parameters are reserved for future use
        // in more sophisticated iterative algorithms that need to defer processing
        let pre_type = if let Some(&ant) = flow.antecedent.first() {
            *results.get(&ant).unwrap_or(&current_type)
        } else {
            current_type
        };

        let Some(node) = self.arena.get(flow.node) else {
            return pre_type;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return pre_type;
        }
        let Some(call) = self.arena.get_call_expr(node) else {
            return pre_type;
        };

        let Some(node_types) = self.node_types else {
            return pre_type;
        };
        let Some(&callee_type) = node_types.get(&call.expression.0) else {
            return pre_type;
        };
        let Some(signature) = self.predicate_signature_for_type(callee_type) else {
            return pre_type;
        };
        if !signature.predicate.asserts {
            return pre_type;
        }

        let Some(predicate_target) =
            self.predicate_target_expression(call, &signature.predicate, &signature.params)
        else {
            return pre_type;
        };
        if !self.is_matching_reference(predicate_target, reference) {
            return pre_type;
        }

        self.apply_type_predicate_narrowing(pre_type, &signature.predicate, true)
    }

    /// Iterative flow graph traversal for definite assignment checks.
    ///
    /// This replaces the recursive implementation to prevent stack overflow
    /// on deeply nested control flow structures. Uses a worklist algorithm with
    /// fixed-point iteration to determine if a variable is definitely assigned.
    fn check_definite_assignment(
        &self,
        reference: NodeIndex,
        flow_id: FlowNodeId,
        _visited: &mut Vec<FlowNodeId>,
        cache: &mut FxHashMap<FlowNodeId, bool>,
    ) -> bool {
        // Helper: Add a node to the worklist if not already present
        let add_to_worklist = |node: FlowNodeId,
                               worklist: &mut Vec<FlowNodeId>,
                               in_worklist: &mut FxHashSet<FlowNodeId>| {
            if !in_worklist.contains(&node) {
                worklist.push(node);
                in_worklist.insert(node);
            }
        };

        // Result cache: flow_id -> is_assigned
        // We use a local cache that we'll merge into the provided cache
        let mut local_cache: FxHashMap<FlowNodeId, bool> = FxHashMap::default();

        // Worklist for processing nodes
        let mut worklist: Vec<FlowNodeId> = vec![flow_id];
        let mut in_worklist: FxHashSet<FlowNodeId> = FxHashSet::default();
        in_worklist.insert(flow_id);

        // Track nodes that are waiting for their antecedents to be computed
        // Map: node -> set of antecedents it's waiting for
        let mut waiting_for: FxHashMap<FlowNodeId, FxHashSet<FlowNodeId>> = FxHashMap::default();

        while let Some(current_flow) = worklist.pop() {
            in_worklist.remove(&current_flow);

            // Skip if we already have a result
            if local_cache.contains_key(&current_flow) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(current_flow) else {
                // Flow node doesn't exist - mark as assigned
                local_cache.insert(current_flow, true);
                // Notify any nodes waiting for this one
                let ready: Vec<_> = waiting_for
                    .iter()
                    .filter(|(_, ants)| ants.contains(&current_flow))
                    .map(|(&node, _)| node)
                    .collect();
                for node in ready {
                    waiting_for.remove(&node);
                    add_to_worklist(node, &mut worklist, &mut in_worklist);
                }
                continue;
            };

            // Compute the result based on flow node type
            let result = if flow.has_any_flags(flow_flags::UNREACHABLE) {
                false
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                if self.assignment_targets_reference(flow.node, reference) {
                    true
                } else if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        // Add antecedent to worklist and defer
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                if flow.antecedent.is_empty() {
                    false
                } else {
                    // Check if all antecedents have results
                    let mut all_ready = true;
                    let mut results = Vec::new();

                    for &ant in &flow.antecedent {
                        if let Some(ant_node) = self.binder.flow_nodes.get(ant)
                            && ant_node.has_any_flags(flow_flags::UNREACHABLE) {
                                // Unreachable branches satisfy the condition vacuously
                                results.push(true);
                                continue;
                            }

                        if let Some(&ant_result) = local_cache.get(&ant) {
                            results.push(ant_result);
                        } else {
                            all_ready = false;
                            add_to_worklist(ant, &mut worklist, &mut in_worklist);
                            waiting_for.entry(current_flow).or_default().insert(ant);
                        }
                    }

                    if !all_ready {
                        continue;
                    }

                    // All antecedents processed - compute result (all must be true)
                    results.iter().all(|&r| r)
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL) {
                if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::CONDITION) {
                if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                if flow.antecedent.is_empty() {
                    false
                } else {
                    // Similar to BRANCH_LABEL - check all antecedents
                    let mut all_ready = true;
                    let mut results = Vec::new();

                    for &ant in &flow.antecedent {
                        if let Some(ant_node) = self.binder.flow_nodes.get(ant)
                            && ant_node.has_any_flags(flow_flags::UNREACHABLE) {
                                results.push(true);
                                continue;
                            }

                        if let Some(&ant_result) = local_cache.get(&ant) {
                            results.push(ant_result);
                        } else {
                            all_ready = false;
                            add_to_worklist(ant, &mut worklist, &mut in_worklist);
                            waiting_for.entry(current_flow).or_default().insert(ant);
                        }
                    }

                    if !all_ready {
                        continue;
                    }

                    results.iter().all(|&r| r)
                }
            } else if flow.has_any_flags(flow_flags::START) {
                false
            } else if let Some(&ant) = flow.antecedent.first() {
                if let Some(&ant_result) = local_cache.get(&ant) {
                    ant_result
                } else {
                    add_to_worklist(ant, &mut worklist, &mut in_worklist);
                    waiting_for.entry(current_flow).or_default().insert(ant);
                    continue;
                }
            } else {
                false
            };

            // Store the result
            local_cache.insert(current_flow, result);

            // Notify any nodes waiting for this one
            let ready: Vec<_> = waiting_for
                .iter()
                .filter(|(_, ants)| ants.contains(&current_flow))
                .map(|(&node, _)| node)
                .collect();
            for node in ready {
                waiting_for.remove(&node);
                add_to_worklist(node, &mut worklist, &mut in_worklist);
            }
        }

        // Get the final result
        let final_result = *local_cache.get(&flow_id).unwrap_or(&false);

        // Merge local cache into the provided cache
        cache.extend(local_cache);

        final_result
    }

    /// Check if this is a direct assignment to a reference (e.g., `x = value`)
    /// as opposed to a destructuring assignment (e.g., `[x] = [value]`)
    fn is_direct_assignment_to_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        // Check if it's a binary expression (x = value)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
                && self.is_assignment_operator(bin.operator_token) {
                    // Check if the left side is directly the target (not a destructuring pattern)
                    let left = self.skip_parenthesized(bin.left);
                    let target = self.skip_parenthesized(target);
                    return self.is_matching_reference(left, target);
                }

        // Increment/decrement operators (x++, --x) are also direct assignments
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.arena.get_unary_expr(node)
                && (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                {
                    let operand = self.skip_parenthesized(unary.operand);
                    let target = self.skip_parenthesized(target);
                    return self.is_matching_reference(operand, target);
                }

        false
    }

    fn get_assigned_type(&self, assignment_node: NodeIndex, target: NodeIndex) -> Option<TypeId> {
        let Some(node) = self.arena.get(assignment_node) else {
            return None;
        };

        if let Some(rhs) = self.assignment_rhs_for_reference(assignment_node, target) {
            // For flow narrowing, prefer literal types from AST nodes over the type checker's widened types
            // This ensures that `x = 42` narrows to literal 42.0, not just NUMBER
            // This matches TypeScript's behavior where control flow analysis preserves literal types
            if let Some(literal_type) = self.literal_type_from_node(rhs) {
                return Some(literal_type);
            }
            if let Some(nullish_type) = self.nullish_literal_type(rhs) {
                return Some(nullish_type);
            }
            // Fall back to type checker's result for non-literal expressions
            if let Some(node_types) = self.node_types
                && let Some(&rhs_type) = node_types.get(&rhs.0) {
                    return Some(rhs_type);
                }
            return None;
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            let unary = self.arena.get_unary_expr(node)?;
            if (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
                && self.is_matching_reference(unary.operand, target)
            {
                return Some(TypeId::NUMBER);
            }
        }

        None
    }

    fn assignment_rhs_for_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> Option<NodeIndex> {
        let Some(node) = self.arena.get(assignment_node) else {
            return None;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                if self.is_matching_reference(bin.left, reference) {
                    return Some(bin.right);
                }
                if let Some(rhs) = self.match_destructuring_rhs(bin.left, bin.right, reference) {
                    return Some(rhs);
                }
            }
            return None;
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(node)?;
            if self.is_matching_reference(decl.name, reference) && !decl.initializer.is_none() {
                return Some(decl.initializer);
            }
            if !decl.initializer.is_none()
                && let Some(rhs) =
                    self.match_destructuring_rhs(decl.name, decl.initializer, reference)
                {
                    return Some(rhs);
                }
            return None;
        }

        if (node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node.kind == syntax_kind_ext::VARIABLE_STATEMENT)
            && let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    if self.is_matching_reference(decl.name, reference)
                        && !decl.initializer.is_none()
                    {
                        return Some(decl.initializer);
                    }
                    if !decl.initializer.is_none()
                        && let Some(rhs) =
                            self.match_destructuring_rhs(decl.name, decl.initializer, reference)
                        {
                            return Some(rhs);
                        }
                }
            }

        None
    }

    fn match_destructuring_rhs(
        &self,
        pattern: NodeIndex,
        rhs: NodeIndex,
        target: NodeIndex,
    ) -> Option<NodeIndex> {
        if pattern.is_none() {
            return None;
        }

        let pattern = self.skip_parens_and_assertions(pattern);
        let rhs = if rhs.is_none() {
            rhs
        } else {
            self.skip_parens_and_assertions(rhs)
        };

        if !rhs.is_none() && self.is_matching_reference(pattern, target) {
            return Some(rhs);
        }

        let node = self.arena.get(pattern)?;
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                    return None;
                }
                if let Some(found) = self.match_destructuring_rhs(bin.left, rhs, target) {
                    return Some(found);
                }
                if self.assignment_targets_reference_internal(bin.left, target) {
                    if let Some(found) = self.match_destructuring_rhs(bin.left, bin.right, target) {
                        return Some(found);
                    }
                    return Some(bin.right);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                let elements = if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };
                let rhs_elements = self.array_literal_elements(rhs);
                for (index, &elem) in elements.nodes.iter().enumerate() {
                    if elem.is_none() {
                        continue;
                    }
                    if !self.assignment_targets_reference_internal(elem, target) {
                        continue;
                    }
                    let rhs_elem = rhs_elements
                        .and_then(|rhs_list| rhs_list.nodes.get(index).copied())
                        .unwrap_or(NodeIndex::NONE);
                    if let Some(found) = self.match_destructuring_rhs(elem, rhs_elem, target) {
                        return Some(found);
                    }
                    if !rhs_elem.is_none() {
                        return Some(rhs_elem);
                    }
                    return None;
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN =>
            {
                let elements = if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };
                for &elem in &elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if let Some(found) = self.match_object_pattern_element(elem, rhs, target) {
                        return Some(found);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(node)?;
                if self.assignment_targets_reference_internal(binding.name, target) {
                    if !rhs.is_none() {
                        if let Some(found) = self.match_destructuring_rhs(binding.name, rhs, target)
                        {
                            return Some(found);
                        }
                        if self.is_matching_reference(binding.name, target) {
                            return Some(rhs);
                        }
                    }
                    if !binding.initializer.is_none() {
                        if let Some(found) =
                            self.match_destructuring_rhs(binding.name, binding.initializer, target)
                        {
                            return Some(found);
                        }
                        return Some(binding.initializer);
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn match_object_pattern_element(
        &self,
        elem: NodeIndex,
        rhs: NodeIndex,
        target: NodeIndex,
    ) -> Option<NodeIndex> {
        let elem_node = self.arena.get(elem)?;
        match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(elem_node)?;
                if !self.assignment_targets_reference_internal(prop.initializer, target) {
                    return None;
                }
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, prop.name) {
                    if let Some(found) =
                        self.match_destructuring_rhs(prop.initializer, rhs_value, target)
                    {
                        return Some(found);
                    }
                    return Some(rhs_value);
                }
                return self.match_destructuring_rhs(prop.initializer, NodeIndex::NONE, target);
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(elem_node)?;
                if !self.assignment_targets_reference_internal(prop.name, target) {
                    return None;
                }
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, prop.name) {
                    return Some(rhs_value);
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(elem_node)?;
                if !self.assignment_targets_reference_internal(binding.name, target) {
                    return None;
                }
                let name_idx = if binding.property_name.is_none() {
                    binding.name
                } else {
                    binding.property_name
                };
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, name_idx) {
                    if let Some(found) =
                        self.match_destructuring_rhs(binding.name, rhs_value, target)
                    {
                        return Some(found);
                    }
                    return Some(rhs_value);
                }
                if !binding.initializer.is_none() {
                    if let Some(found) =
                        self.match_destructuring_rhs(binding.name, binding.initializer, target)
                    {
                        return Some(found);
                    }
                    return Some(binding.initializer);
                }
            }
            _ => {}
        }
        None
    }

    fn array_literal_elements(&self, rhs: NodeIndex) -> Option<&NodeList> {
        if rhs.is_none() {
            return None;
        }
        let rhs = self.skip_parens_and_assertions(rhs);
        let node = self.arena.get(rhs)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        self.arena.get_literal_expr(node).map(|lit| &lit.elements)
    }

    fn lookup_property_in_rhs(&self, rhs: NodeIndex, name: NodeIndex) -> Option<NodeIndex> {
        if rhs.is_none() || name.is_none() {
            return None;
        }
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;
        let key = self.property_key_from_name(name)?;

        if rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let lit = self.arena.get_literal_expr(rhs_node)?;
            if let PropertyKey::Index(index) = key {
                return lit
                    .elements
                    .nodes
                    .get(index)
                    .copied()
                    .filter(|n| !n.is_none());
            }
            return None;
        }

        if rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let lit = self.arena.get_literal_expr(rhs_node)?;
            if let PropertyKey::Atom(atom) = key {
                return self.find_property_in_object_literal(lit, atom);
            }
        }

        None
    }

    fn find_property_in_object_literal(
        &self,
        literal: &crate::parser::node::LiteralExprData,
        target: Atom,
    ) -> Option<NodeIndex> {
        for &elem in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem) else {
                continue;
            };
            match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_property_assignment(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target {
                            return Some(prop.initializer);
                        }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_shorthand_property(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target {
                            return Some(prop.name);
                        }
                }
                _ => {}
            }
        }
        None
    }

    fn assignment_affects_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self
                .arena
                .get_binary_expr(node)
                .map(|bin| {
                    self.is_assignment_operator(bin.operator_token)
                        && self.assignment_affects_reference(bin.left, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self
                .arena
                .get_unary_expr(node)
                .map(|unary| {
                    (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && self.assignment_affects_reference(unary.operand, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .map(|decl| self.assignment_affects_reference(decl.name, target))
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_affects_reference(decl.name, target) {
                            return true;
                        }
                }
            }
            return false;
        }

        self.assignment_affects_reference(assignment_node, target)
    }

    pub fn assignment_targets_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        self.assignment_targets_reference_node(assignment_node, target)
    }

    fn assignment_targets_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self
                .arena
                .get_binary_expr(node)
                .map(|bin| {
                    self.is_assignment_operator(bin.operator_token)
                        && self.assignment_targets_reference_internal(bin.left, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self
                .arena
                .get_unary_expr(node)
                .map(|unary| {
                    (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && self.assignment_targets_reference_internal(unary.operand, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .map(|decl| self.assignment_targets_reference_internal(decl.name, target))
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_targets_reference_internal(decl.name, target) {
                            return true;
                        }
                }
            }
            return false;
        }

        self.assignment_targets_reference_internal(assignment_node, target)
    }

    fn narrow_by_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let binary = BinaryExprData {
            left: switch_expr,
            operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
            right: case_expr,
        };

        self.narrow_by_binary_expr(type_id, &binary, target, true, narrowing)
    }

    fn narrow_by_default_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_block: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let Some(case_block_node) = self.arena.get(case_block) else {
            return type_id;
        };
        let Some(case_block) = self.arena.get_block(case_block_node) else {
            return type_id;
        };

        let mut narrowed = type_id;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                continue;
            };
            if clause.expression.is_none() {
                continue;
            }

            let binary = BinaryExprData {
                left: switch_expr,
                operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
                right: clause.expression,
            };
            narrowed = self.narrow_by_binary_expr(narrowed, &binary, target, false, narrowing);
        }

        narrowed
    }

    /// Apply type narrowing based on a condition expression.
    fn narrow_type_by_condition(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        let mut visited_aliases = Vec::new();
        self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            &mut visited_aliases,
        )
    }

    fn narrow_type_by_condition_inner(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> TypeId {
        let condition_idx = self.skip_parenthesized(condition_idx);
        let Some(cond_node) = self.arena.get(condition_idx) else {
            return type_id;
        };

        let narrowing = NarrowingContext::new(self.interner);

        if cond_node.kind == SyntaxKind::Identifier as u16
            && let Some((sym_id, initializer)) = self.const_condition_initializer(condition_idx)
                && !visited_aliases.contains(&sym_id) {
                    visited_aliases.push(sym_id);
                    let narrowed = self.narrow_type_by_condition_inner(
                        type_id,
                        initializer,
                        target,
                        is_true_branch,
                        visited_aliases,
                    );
                    visited_aliases.pop();
                    return narrowed;
                }

        match cond_node.kind {
            // typeof x === "string"
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(cond_node) {
                    if let Some(narrowed) = self.narrow_by_logical_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        visited_aliases,
                    ) {
                        return narrowed;
                    }
                    return self.narrow_by_binary_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        &narrowing,
                    );
                }
            }

            // Prefix unary: !x
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(cond_node) {
                    // !x inverts the narrowing
                    if unary.operator == SyntaxKind::ExclamationToken as u16 {
                        return self.narrow_type_by_condition_inner(
                            type_id,
                            unary.operand,
                            target,
                            !is_true_branch,
                            visited_aliases,
                        );
                    }
                }
            }

            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(cond_node) {
                    if let Some(narrowed) =
                        self.narrow_by_call_predicate(type_id, call, target, is_true_branch)
                    {
                        return narrowed;
                    }
                    if is_true_branch {
                        let optional_call =
                            (cond_node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0;
                        if optional_call && self.is_matching_reference(call.expression, target) {
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                        if let Some(callee_node) = self.arena.get(call.expression)
                            && let Some(access) = self.arena.get_access_expr(callee_node)
                                && access.question_dot_token
                                    && self.is_matching_reference(access.expression, target)
                                {
                                    let narrowed =
                                        narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                                    return narrowing
                                        .narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                                }
                    }
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(cond_node) {
                    // Handle optional chaining: y?.a
                    if access.question_dot_token
                        && is_true_branch
                        && self.is_matching_reference(access.expression, target)
                    {
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                }
                // Handle discriminant narrowing (discriminated unions)
                if let Some(prop_name) = self.discriminant_property(condition_idx, target) {
                    let literal_true = self.interner.literal_boolean(true);
                    if is_true_branch {
                        return narrowing.narrow_by_discriminant(type_id, prop_name, literal_true);
                    }
                    return narrowing.narrow_by_excluding_discriminant(
                        type_id,
                        prop_name,
                        literal_true,
                    );
                }

                // Handle truthiness narrowing for property/element access: if (y.a)
                if self.is_matching_reference(condition_idx, target) {
                    if is_true_branch {
                        // Remove null/undefined (truthy narrowing)
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                    // False branch - keep only falsy types
                    return self.narrow_to_falsy(type_id);
                }
            }

            // Truthiness check: if (x)
            _ => {
                if self.is_matching_reference(condition_idx, target) {
                    if is_true_branch {
                        // Remove null/undefined (truthy narrowing)
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                    }
                    // False branch - keep only falsy types
                    return self.narrow_to_falsy(type_id);
                }
            }
        }

        type_id
    }

    fn const_condition_initializer(&self, ident_idx: NodeIndex) -> Option<(SymbolId, NodeIndex)> {
        let sym_id = self.binder.resolve_identifier(self.arena, ident_idx)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return None;
        }
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if decl.initializer.is_none() {
            return None;
        }
        Some((sym_id, decl.initializer))
    }

    fn is_const_variable_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let mut flags = decl_node.flags as u32;
        if (flags & (node_flags::LET | node_flags::CONST)) == 0 {
            let Some(ext) = self.arena.get_extended(decl_idx) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                return false;
            }
            flags |= parent_node.flags as u32;
        }
        (flags & node_flags::CONST) != 0
    }

    /// Narrow type based on a binary expression (===, !==, typeof checks, etc.)
    fn narrow_by_binary_expr(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let operator = bin.operator_token;

        if operator == SyntaxKind::InstanceOfKeyword as u16 {
            return self.narrow_by_instanceof(type_id, bin, target, is_true_branch);
        }

        if operator == SyntaxKind::InKeyword as u16 {
            return self.narrow_by_in_operator(type_id, bin, target, is_true_branch);
        }

        let (is_equals, is_strict) = match operator {
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => (true, true),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => (false, true),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => (true, false),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => (false, false),
            _ => return type_id,
        };

        let effective_truth = if is_equals {
            is_true_branch
        } else {
            !is_true_branch
        };

        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target) {
            if effective_truth {
                return narrowing.narrow_by_typeof(type_id, type_name);
            }
            return self.narrow_by_typeof_negation(type_id, type_name, narrowing);
        }

        if let Some(nullish) = self.nullish_comparison(bin.left, bin.right, target) {
            if is_strict {
                if effective_truth {
                    return nullish;
                }
                return narrowing.narrow_excluding_type(type_id, nullish);
            }

            let nullish_union = self.interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
            if effective_truth {
                return nullish_union;
            }

            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
        }

        if is_strict {
            if let Some((prop_name, literal_type, is_optional)) =
                self.discriminant_comparison(bin.left, bin.right, target)
            {
                let mut base_type = type_id;
                if is_optional && effective_truth {
                    let narrowed = narrowing.narrow_excluding_type(base_type, TypeId::NULL);
                    base_type = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                }
                return self.narrow_by_discriminant_for_type(
                    base_type,
                    prop_name,
                    literal_type,
                    effective_truth,
                    narrowing,
                );
            }

            if let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target) {
                if effective_truth {
                    let narrowed = narrowing.narrow_to_type(type_id, literal_type);
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    if self.literal_assignable_to(literal_type, type_id, narrowing) {
                        return literal_type;
                    }
                    return TypeId::NEVER;
                }
                return narrowing.narrow_excluding_type(type_id, literal_type);
            }
        }

        type_id
    }

    fn narrow_by_logical_expr(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        let operator = bin.operator_token;

        if operator == SyntaxKind::AmpersandAmpersandToken as u16 {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_true,
                    bin.right,
                    target,
                    true,
                    visited_aliases,
                );
                return Some(right_true);
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                visited_aliases,
            );
            let left_true = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                true,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_true,
                bin.right,
                target,
                false,
                visited_aliases,
            );
            return Some(self.union_types(left_false, right_false));
        }

        if operator == SyntaxKind::BarBarToken as u16 {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    visited_aliases,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    visited_aliases,
                );
                return Some(self.union_types(left_true, right_true));
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                visited_aliases,
            );
            return Some(right_false);
        }

        None
    }

    fn is_assignment_operator(&self, operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    fn assignment_affects_reference(&self, left: NodeIndex, target: NodeIndex) -> bool {
        let left = self.skip_parenthesized(left);
        let target = self.skip_parenthesized(target);
        if self.is_matching_reference(left, target) {
            return true;
        }
        if let Some(base) = self.reference_base(target)
            && self.assignment_affects_reference(left, base) {
                return true;
            }

        let Some(node) = self.arena.get(left) else {
            return false;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let Some(access) = self.arena.get_access_expr(node) else {
                return false;
            };
            if access.question_dot_token {
                return false;
            }
            return self.assignment_affects_reference(access.expression, target);
        }

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node) {
                return self.assignment_affects_reference(unary.expression, target);
            }

        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node) {
                return self.assignment_affects_reference(assertion.expression, target);
            }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
                && self.is_assignment_operator(bin.operator_token) {
                    return self.assignment_affects_reference(bin.left, target);
                }

        if (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            && let Some(lit) = self.arena.get_literal_expr(node) {
                for &elem in &lit.elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if self.assignment_affects_reference(elem, target) {
                        return true;
                    }
                }
            }

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
                && self.assignment_affects_reference(prop.initializer, target) {
                    return true;
                }

        if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_shorthand_property(node)
                && self.assignment_affects_reference(prop.name, target) {
                    return true;
                }

        if (node.kind == syntax_kind_ext::SPREAD_ELEMENT
            || node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            && let Some(spread) = self.arena.get_spread(node)
                && self.assignment_affects_reference(spread.expression, target) {
                    return true;
                }

        if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.arena.get_binding_pattern(node) {
                for &elem in &pattern.elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if self.assignment_affects_reference(elem, target) {
                        return true;
                    }
                }
            }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(binding) = self.arena.get_binding_element(node)
                && self.assignment_affects_reference(binding.name, target) {
                    return true;
                }

        false
    }

    fn assignment_targets_reference_internal(&self, left: NodeIndex, target: NodeIndex) -> bool {
        let left = self.skip_parenthesized(left);
        let target = self.skip_parenthesized(target);
        if self.is_matching_reference(left, target) {
            return true;
        }

        let Some(node) = self.arena.get(left) else {
            return false;
        };

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node) {
                return self.assignment_targets_reference_internal(unary.expression, target);
            }

        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node) {
                return self.assignment_targets_reference_internal(assertion.expression, target);
            }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
                && self.is_assignment_operator(bin.operator_token) {
                    return self.assignment_targets_reference_internal(bin.left, target);
                }

        if (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            && let Some(lit) = self.arena.get_literal_expr(node) {
                for &elem in &lit.elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if self.assignment_targets_reference_internal(elem, target) {
                        return true;
                    }
                }
            }

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
                && self.assignment_targets_reference_internal(prop.initializer, target) {
                    return true;
                }

        if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_shorthand_property(node)
                && self.assignment_targets_reference_internal(prop.name, target) {
                    return true;
                }

        if (node.kind == syntax_kind_ext::SPREAD_ELEMENT
            || node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            && let Some(spread) = self.arena.get_spread(node)
                && self.assignment_targets_reference_internal(spread.expression, target) {
                    return true;
                }

        if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.arena.get_binding_pattern(node) {
                for &elem in &pattern.elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if self.assignment_targets_reference_internal(elem, target) {
                        return true;
                    }
                }
            }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(binding) = self.arena.get_binding_element(node)
                && self.assignment_targets_reference_internal(binding.name, target) {
                    return true;
                }

        false
    }

    fn array_mutation_affects_reference(&self, call: &CallExprData, target: NodeIndex) -> bool {
        let Some(callee_node) = self.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        self.assignment_affects_reference(access.expression, target)
    }

    fn narrow_by_call_predicate(
        &self,
        type_id: TypeId,
        call: &CallExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> Option<TypeId> {
        let node_types = self.node_types?;
        let callee_type = *node_types.get(&call.expression.0)?;
        let signature = self.predicate_signature_for_type(callee_type)?;
        let predicate_target =
            self.predicate_target_expression(call, &signature.predicate, &signature.params)?;

        if !self.is_matching_reference(predicate_target, target) {
            return None;
        }

        Some(self.apply_type_predicate_narrowing(type_id, &signature.predicate, is_true_branch))
    }

    fn predicate_signature_for_type(&self, callee_type: TypeId) -> Option<PredicateSignature> {
        let key = self.interner.lookup(callee_type)?;

        match key {
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let predicate = shape.type_predicate.clone()?;
                Some(PredicateSignature {
                    predicate,
                    params: shape.params.clone(),
                })
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                if shape.call_signatures.len() != 1 {
                    return None;
                }
                let sig = &shape.call_signatures[0];
                let predicate = sig.type_predicate.clone()?;
                Some(PredicateSignature {
                    predicate,
                    params: sig.params.clone(),
                })
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    if let Some(sig) = self.predicate_signature_for_type(member) {
                        return Some(sig);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn predicate_target_expression(
        &self,
        call: &CallExprData,
        predicate: &TypePredicate,
        params: &[ParamInfo],
    ) -> Option<NodeIndex> {
        match predicate.target {
            TypePredicateTarget::Identifier(name) => {
                let param_index = params.iter().position(|param| param.name == Some(name))?;
                let args = call.arguments.as_ref()?.nodes.as_slice();
                args.get(param_index).copied()
            }
            TypePredicateTarget::This => {
                let callee_node = self.arena.get(call.expression)?;
                let access = self.arena.get_access_expr(callee_node)?;
                Some(access.expression)
            }
        }
    }

    fn apply_type_predicate_narrowing(
        &self,
        type_id: TypeId,
        predicate: &TypePredicate,
        is_true_branch: bool,
    ) -> TypeId {
        if predicate.asserts && !is_true_branch {
            return type_id;
        }

        let narrowing = NarrowingContext::new(self.interner);

        if let Some(predicate_type) = predicate.type_id {
            if is_true_branch {
                return narrowing.narrow_to_type(type_id, predicate_type);
            }
            return narrowing.narrow_excluding_type(type_id, predicate_type);
        }

        if is_true_branch {
            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
        }

        self.narrow_to_falsy(type_id)
    }

    fn narrow_by_instanceof(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        if !is_true_branch {
            return type_id;
        }

        if !self.is_matching_reference(bin.left, target) {
            return type_id;
        }

        if let Some(instance_type) = self.instance_type_from_constructor(bin.right) {
            let narrowing = NarrowingContext::new(self.interner);
            return narrowing.narrow_to_type(type_id, instance_type);
        }

        self.narrow_to_objectish(type_id)
    }

    fn instance_type_from_constructor(&self, expr: NodeIndex) -> Option<TypeId> {
        if let Some(node_types) = self.node_types
            && let Some(&type_id) = node_types.get(&expr.0)
                && let Some(instance_type) = self.instance_type_from_constructor_type(type_id) {
                    return Some(instance_type);
                }

        let expr = self.skip_parens_and_assertions(expr);
        let sym_id = self.binder.resolve_identifier(self.arena, expr)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::CLASS) != 0 {
            return Some(self.interner.reference(crate::solver::SymbolRef(sym_id.0)));
        }

        None
    }

    fn instance_type_from_constructor_type(&self, type_id: TypeId) -> Option<TypeId> {
        match self.interner.lookup(type_id)? {
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    return None;
                }
                let mut returns = Vec::new();
                for sig in &shape.construct_signatures {
                    returns.push(sig.return_type);
                }
                Some(if returns.len() == 1 {
                    returns[0]
                } else {
                    self.interner.union(returns)
                })
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let mut instance_types = Vec::new();
                for &member in members.iter() {
                    if let Some(instance_type) = self.instance_type_from_constructor_type(member) {
                        instance_types.push(instance_type);
                    }
                }
                if instance_types.is_empty() {
                    None
                } else if instance_types.len() == 1 {
                    Some(instance_types[0])
                } else {
                    Some(self.interner.union(instance_types))
                }
            }
            _ => None,
        }
    }

    fn narrow_by_in_operator(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        if !self.is_matching_reference(bin.right, target) {
            return type_id;
        }

        let Some((prop_name, prop_is_number)) = self.in_property_name(bin.left) else {
            return type_id;
        };

        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return type_id;
        }

        if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(type_id) {
            if let Some(constraint) = info.constraint
                && constraint != type_id {
                    let narrowed_constraint =
                        self.narrow_by_in_operator(constraint, bin, target, is_true_branch);
                    if narrowed_constraint != constraint {
                        return self.interner.intersection2(type_id, narrowed_constraint);
                    }
                }
            return type_id;
        }

        let Some(TypeKey::Union(members)) = self.interner.lookup(type_id) else {
            return type_id;
        };

        let members = self.interner.type_list(members);
        let mut filtered = Vec::new();
        for &member in members.iter() {
            let presence = self.property_presence(member, prop_name, prop_is_number);
            if self.keep_in_operator_member(presence, is_true_branch) {
                filtered.push(member);
            }
        }

        match filtered.len() {
            0 => TypeId::NEVER,
            1 => filtered[0],
            _ => {
                if filtered.len() == members.len() {
                    type_id
                } else {
                    self.interner.union(filtered)
                }
            }
        }
    }

    fn narrow_to_objectish(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return type_id;
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(members);
            let mut kept = Vec::new();
            for &member in members.iter() {
                if !self.is_definitely_non_object(member) {
                    kept.push(member);
                }
            }

            return match kept.len() {
                0 => TypeId::NEVER,
                1 => kept[0],
                _ => {
                    if kept.len() == members.len() {
                        type_id
                    } else {
                        self.interner.union(kept)
                    }
                }
            };
        }

        if self.is_definitely_non_object(type_id) {
            TypeId::NEVER
        } else {
            type_id
        }
    }

    fn is_definitely_non_object(&self, type_id: TypeId) -> bool {
        if matches!(
            type_id,
            TypeId::NEVER
                | TypeId::VOID
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::BOOLEAN
                | TypeId::NUMBER
                | TypeId::STRING
                | TypeId::BIGINT
                | TypeId::SYMBOL
        ) {
            return true;
        }

        match self.interner.lookup(type_id) {
            Some(TypeKey::Literal(_)) => true,
            Some(TypeKey::Intrinsic(kind)) => matches!(
                kind,
                crate::solver::IntrinsicKind::Void
                    | crate::solver::IntrinsicKind::Undefined
                    | crate::solver::IntrinsicKind::Null
                    | crate::solver::IntrinsicKind::Boolean
                    | crate::solver::IntrinsicKind::Number
                    | crate::solver::IntrinsicKind::String
                    | crate::solver::IntrinsicKind::Bigint
                    | crate::solver::IntrinsicKind::Symbol
                    | crate::solver::IntrinsicKind::Never
            ),
            _ => false,
        }
    }

    fn in_property_name(&self, idx: NodeIndex) -> Option<(Atom, bool)> {
        let idx = self.skip_parenthesized(idx);

        // Handle private identifiers (e.g., `#field in obj`)
        if let Some(node) = self.arena.get(idx)
            && node.kind == SyntaxKind::PrivateIdentifier as u16
                && let Some(ident) = self.arena.get_identifier(node) {
                    return Some((self.interner.intern_string(&ident.escaped_text), false));
                }

        self.literal_atom_and_kind_from_node_or_type(idx)
    }

    fn keep_in_operator_member(&self, presence: PropertyPresence, is_true_branch: bool) -> bool {
        match (presence, is_true_branch) {
            (PropertyPresence::Required, false) => false,
            (PropertyPresence::Absent, true) => false,
            _ => true,
        }
    }

    fn property_presence(
        &self,
        type_id: TypeId,
        prop_name: Atom,
        prop_is_number: bool,
    ) -> PropertyPresence {
        let Some(key) = self.interner.lookup(type_id) else {
            return PropertyPresence::Unknown;
        };

        match key {
            TypeKey::Intrinsic(crate::solver::IntrinsicKind::Object) => PropertyPresence::Unknown,
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                self.property_presence_in_object(shape_id, prop_name, prop_is_number)
            }
            TypeKey::Callable(callable_id) => {
                self.property_presence_in_callable(callable_id, prop_name)
            }
            TypeKey::Array(_) | TypeKey::Tuple(_) => {
                if prop_is_number {
                    PropertyPresence::Optional
                } else {
                    PropertyPresence::Unknown
                }
            }
            _ => PropertyPresence::Unknown,
        }
    }

    fn property_presence_in_object(
        &self,
        shape_id: crate::solver::ObjectShapeId,
        prop_name: Atom,
        prop_is_number: bool,
    ) -> PropertyPresence {
        let shape = self.interner.object_shape(shape_id);
        let mut found = None;

        match self.interner.object_property_index(shape_id, prop_name) {
            crate::solver::PropertyLookup::Found(idx) => {
                found = shape.properties.get(idx);
            }
            crate::solver::PropertyLookup::Uncached => {
                found = shape.properties.iter().find(|prop| prop.name == prop_name);
            }
            crate::solver::PropertyLookup::NotFound => {}
        }

        if let Some(prop) = found {
            return if prop.optional {
                PropertyPresence::Optional
            } else {
                PropertyPresence::Required
            };
        }

        if prop_is_number && shape.number_index.is_some() {
            return PropertyPresence::Optional;
        }

        if shape.string_index.is_some() {
            return PropertyPresence::Optional;
        }

        PropertyPresence::Absent
    }

    fn property_presence_in_callable(
        &self,
        callable_id: crate::solver::CallableShapeId,
        prop_name: Atom,
    ) -> PropertyPresence {
        let shape = self.interner.callable_shape(callable_id);
        if let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_name) {
            return if prop.optional {
                PropertyPresence::Optional
            } else {
                PropertyPresence::Required
            };
        }
        PropertyPresence::Absent
    }

    fn union_types(&self, left: TypeId, right: TypeId) -> TypeId {
        if left == right {
            left
        } else {
            self.interner.union(vec![left, right])
        }
    }

    fn skip_parenthesized(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node) {
                    idx = paren.expression;
                    continue;
                }
            return idx;
        }
    }

    fn skip_parens_and_assertions(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            idx = self.skip_parenthesized(idx);
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    idx = unary.expression;
                    continue;
                }
            if (node.kind == syntax_kind_ext::TYPE_ASSERTION
                || node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
                && let Some(assertion) = self.arena.get_type_assertion(node) {
                    idx = assertion.expression;
                    continue;
                }
            return idx;
        }
    }

    fn typeof_comparison_literal(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<&str> {
        if self.is_typeof_target(left, target) {
            return self.literal_string_from_node(right);
        }
        if self.is_typeof_target(right, target) {
            return self.literal_string_from_node(left);
        }
        None
    }

    fn is_typeof_target(&self, expr: NodeIndex, target: NodeIndex) -> bool {
        let expr = self.skip_parenthesized(expr);
        let node = match self.arena.get(expr) {
            Some(node) => node,
            None => return false,
        };

        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }

        let Some(unary) = self.arena.get_unary_expr(node) else {
            return false;
        };

        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return false;
        }

        self.is_matching_reference(unary.operand, target)
    }

    fn literal_string_from_node(&self, idx: NodeIndex) -> Option<&str> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.as_str());
        }

        // Handle private identifiers (e.g., #a) for `in` operator narrowing
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.as_str());
        }

        None
    }

    fn literal_type_from_node(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.arena.get_literal(node)?;
                Some(self.interner.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                Some(self.interner.literal_number(value))
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                let normalized = self.normalize_bigint_literal(text)?;
                Some(self.interner.literal_bigint(normalized.as_ref()))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.interner.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(self.interner.literal_boolean(false)),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }

                let operand = self.skip_parenthesized(unary.operand);
                let operand_node = self.arena.get(operand)?;
                match operand_node.kind {
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        let lit = self.arena.get_literal(operand_node)?;
                        let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                        let value = if op == SyntaxKind::MinusToken as u16 {
                            -value
                        } else {
                            value
                        };
                        Some(self.interner.literal_number(value))
                    }
                    k if k == SyntaxKind::BigIntLiteral as u16 => {
                        let lit = self.arena.get_literal(operand_node)?;
                        let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                        let normalized = self.normalize_bigint_literal(text)?;
                        let negative = op == SyntaxKind::MinusToken as u16;
                        Some(
                            self.interner
                                .literal_bigint_with_sign(negative, normalized.as_ref()),
                        )
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn literal_assignable_to(
        &self,
        literal: TypeId,
        target: TypeId,
        narrowing: &NarrowingContext,
    ) -> bool {
        if literal == target || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .any(|&member| self.literal_assignable_to(literal, member, narrowing));
        }

        narrowing.narrow_to_type(literal, target) != TypeId::NEVER
    }

    fn nullish_literal_type(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::NullKeyword as u16 {
            return Some(TypeId::NULL);
        }
        if node.kind == SyntaxKind::UndefinedKeyword as u16 {
            return Some(TypeId::UNDEFINED);
        }

        None
    }

    fn nullish_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.nullish_literal_type(right);
        }
        if self.is_matching_reference(right, target) {
            return self.nullish_literal_type(left);
        }
        None
    }

    fn discriminant_property(&self, expr: NodeIndex, target: NodeIndex) -> Option<Atom> {
        self.discriminant_property_info(expr, target)
            .and_then(|(prop, is_optional)| if is_optional { None } else { Some(prop) })
    }

    fn discriminant_property_info(
        &self,
        expr: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Atom, bool)> {
        let expr = self.skip_parenthesized(expr);
        let node = self.arena.get(expr)?;

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if !self.is_matching_reference(access.expression, target) {
                return None;
            }
            let name_node = self.arena.get(access.name_or_argument)?;
            let ident = self.arena.get_identifier(name_node)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((name, access.question_dot_token));
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if !self.is_matching_reference(access.expression, target) {
                return None;
            }
            let name = self.literal_atom_from_node_or_type(access.name_or_argument)?;
            return Some((name, access.question_dot_token));
        }

        None
    }

    fn discriminant_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Atom, TypeId, bool)> {
        if let Some((prop, is_optional)) = self.discriminant_property_info(left, target)
            && let Some(literal) = self.literal_type_from_node(right) {
                return Some((prop, literal, is_optional));
            }

        if let Some((prop, is_optional)) = self.discriminant_property_info(right, target)
            && let Some(literal) = self.literal_type_from_node(left) {
                return Some((prop, literal, is_optional));
            }

        None
    }

    fn narrow_by_discriminant_for_type(
        &self,
        type_id: TypeId,
        prop_name: Atom,
        literal_type: TypeId,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(type_id)
            && let Some(constraint) = info.constraint
                && constraint != type_id {
                    let narrowed_constraint = if is_true_branch {
                        narrowing.narrow_by_discriminant(constraint, prop_name, literal_type)
                    } else {
                        narrowing.narrow_by_excluding_discriminant(
                            constraint,
                            prop_name,
                            literal_type,
                        )
                    };
                    if narrowed_constraint != constraint {
                        return self.interner.intersection2(type_id, narrowed_constraint);
                    }
                }

        if is_true_branch {
            narrowing.narrow_by_discriminant(type_id, prop_name, literal_type)
        } else {
            narrowing.narrow_by_excluding_discriminant(type_id, prop_name, literal_type)
        }
    }

    fn literal_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.literal_type_from_node(right);
        }
        if self.is_matching_reference(right, target) {
            return self.literal_type_from_node(left);
        }
        None
    }

    fn narrow_by_typeof_negation(
        &self,
        type_id: TypeId,
        typeof_result: &str,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        match typeof_result {
            "string" => narrowing.narrow_excluding_type(type_id, TypeId::STRING),
            "number" => narrowing.narrow_excluding_type(type_id, TypeId::NUMBER),
            "boolean" => narrowing.narrow_excluding_type(type_id, TypeId::BOOLEAN),
            "bigint" => narrowing.narrow_excluding_type(type_id, TypeId::BIGINT),
            "symbol" => narrowing.narrow_excluding_type(type_id, TypeId::SYMBOL),
            "undefined" => narrowing.narrow_excluding_type(type_id, TypeId::UNDEFINED),
            "object" => narrowing.narrow_excluding_type(type_id, TypeId::OBJECT),
            "function" => narrowing.narrow_excluding_function(type_id),
            _ => type_id,
        }
    }

    fn narrow_to_falsy(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return type_id;
        }

        match self.falsy_component(type_id) {
            Some(falsy) => falsy,
            None => TypeId::NEVER,
        }
    }

    fn falsy_component(&self, type_id: TypeId) -> Option<TypeId> {
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return Some(type_id);
        }
        if type_id == TypeId::BOOLEAN {
            return Some(self.interner.literal_boolean(false));
        }
        if type_id == TypeId::STRING {
            return Some(self.interner.literal_string(""));
        }
        if type_id == TypeId::NUMBER {
            return Some(self.interner.literal_number(0.0));
        }
        if type_id == TypeId::BIGINT {
            return Some(self.interner.literal_bigint("0"));
        }

        let key = self.interner.lookup(type_id)?;
        match key {
            TypeKey::Literal(literal) => {
                if self.literal_is_falsy(&literal) {
                    Some(type_id)
                } else {
                    None
                }
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let mut falsy_members = Vec::new();
                for &member in members.iter() {
                    if let Some(falsy) = self.falsy_component(member) {
                        falsy_members.push(falsy);
                    }
                }
                match falsy_members.len() {
                    0 => None,
                    1 => Some(falsy_members[0]),
                    _ => Some(self.interner.union(falsy_members)),
                }
            }
            TypeKey::TypeParameter(_) | TypeKey::Infer(_) => Some(type_id),
            _ => None,
        }
    }

    fn literal_is_falsy(&self, literal: &LiteralValue) -> bool {
        match literal {
            LiteralValue::Boolean(false) => true,
            LiteralValue::Number(value) => value.0 == 0.0,
            LiteralValue::String(atom) => self.interner.resolve_atom(*atom).is_empty(),
            LiteralValue::BigInt(atom) => self.interner.resolve_atom(*atom) == "0",
            _ => false,
        }
    }

    fn strip_numeric_separators<'b>(&self, text: &'b str) -> Cow<'b, str> {
        if !text.as_bytes().contains(&b'_') {
            return Cow::Borrowed(text);
        }

        let mut out = String::with_capacity(text.len());
        for &byte in text.as_bytes() {
            if byte != b'_' {
                out.push(byte as char);
            }
        }
        Cow::Owned(out)
    }

    fn parse_numeric_literal_value(&self, value: Option<f64>, text: &str) -> Option<f64> {
        if let Some(value) = value {
            return Some(value);
        }

        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::parse_radix_digits(rest, 16);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::parse_radix_digits(rest, 2);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::parse_radix_digits(rest, 8);
        }

        if text.as_bytes().contains(&b'_') {
            let cleaned = self.strip_numeric_separators(text);
            return cleaned.as_ref().parse::<f64>().ok();
        }

        text.parse::<f64>().ok()
    }

    fn parse_radix_digits(text: &str, base: u32) -> Option<f64> {
        if text.is_empty() {
            return None;
        }

        let mut value = 0f64;
        let base_value = base as f64;
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;
            value = value * base_value + digit as f64;
        }

        if !saw_digit {
            return None;
        }

        Some(value)
    }

    fn normalize_bigint_literal<'b>(&self, text: &'b str) -> Option<Cow<'b, str>> {
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::bigint_base_to_decimal(rest, 16).map(Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::bigint_base_to_decimal(rest, 2).map(Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::bigint_base_to_decimal(rest, 8).map(Cow::Owned);
        }

        match self.strip_numeric_separators(text) {
            Cow::Borrowed(cleaned) => {
                let trimmed = cleaned.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned.len() {
                    return Some(Cow::Borrowed(cleaned));
                }
                Some(Cow::Borrowed(trimmed))
            }
            Cow::Owned(mut cleaned) => {
                let cleaned_ref = cleaned.as_str();
                let trimmed = cleaned_ref.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned_ref.len() {
                    return Some(Cow::Owned(cleaned));
                }

                let trim_len = cleaned_ref.len() - trimmed.len();
                cleaned.drain(..trim_len);
                Some(Cow::Owned(cleaned))
            }
        }
    }

    fn bigint_base_to_decimal(text: &str, base: u32) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        let mut digits: Vec<u8> = vec![0];
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;

            let mut carry = digit;
            for slot in &mut digits {
                let value = (*slot as u32) * base + carry;
                *slot = (value % 10) as u8;
                carry = value / 10;
            }
            while carry > 0 {
                digits.push((carry % 10) as u8);
                carry /= 10;
            }
        }

        if !saw_digit {
            return None;
        }

        while digits.len() > 1 {
            if let Some(&last) = digits.last() {
                if last == 0 {
                    digits.pop();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let mut out = String::with_capacity(digits.len());
        for digit in digits.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        Some(out)
    }

    /// Check if two references point to the same symbol or property access chain.
    fn is_matching_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a = self.skip_parenthesized(a);
        let b = self.skip_parenthesized(b);

        if let (Some(node_a), Some(node_b)) = (self.arena.get(a), self.arena.get(b)) {
            if node_a.kind == SyntaxKind::ThisKeyword as u16
                && node_b.kind == SyntaxKind::ThisKeyword as u16
            {
                return true;
            }
            if node_a.kind == SyntaxKind::SuperKeyword as u16
                && node_b.kind == SyntaxKind::SuperKeyword as u16
            {
                return true;
            }
        }

        let sym_a = self.reference_symbol(a);
        let sym_b = self.reference_symbol(b);
        if sym_a.is_some() && sym_a == sym_b {
            return true;
        }

        self.is_matching_property_reference(a, b)
    }

    fn is_matching_property_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let Some((a_base, a_name)) = self.property_reference(a) else {
            return false;
        };
        let Some((b_base, b_name)) = self.property_reference(b) else {
            return false;
        };
        if a_name != b_name {
            return false;
        }
        self.is_matching_reference(a_base, b_base)
    }

    fn property_reference(&self, idx: NodeIndex) -> Option<(NodeIndex, Atom)> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.property_reference(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.property_reference(assertion.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name_node = self.arena.get(access.name_or_argument)?;
            let ident = self.arena.get_identifier(name_node)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((access.expression, name));
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name = self.literal_atom_from_node_or_type(access.name_or_argument)?;
            return Some((access.expression, name));
        }

        None
    }

    fn literal_atom_from_node_or_type(&self, idx: NodeIndex) -> Option<Atom> {
        if let Some(name) = self.literal_string_from_node(idx) {
            return Some(self.interner.intern_string(name));
        }
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some(self.atom_from_numeric_value(value));
        }
        self.literal_atom_from_type(idx)
    }

    fn literal_atom_and_kind_from_node_or_type(&self, idx: NodeIndex) -> Option<(Atom, bool)> {
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some((self.atom_from_numeric_value(value), true));
        }
        if let Some(name) = self.literal_string_from_node(idx) {
            return Some((self.interner.intern_string(name), false));
        }

        // Handle private identifiers (e.g., #a in x)
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            let ident = self.arena.get_identifier(node)?;
            return Some((self.interner.intern_string(&ident.escaped_text), false));
        }

        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match self.interner.lookup(type_id)? {
            TypeKey::Literal(LiteralValue::String(atom)) => Some((atom, false)),
            TypeKey::Literal(LiteralValue::Number(num)) => {
                Some((self.atom_from_numeric_value(num.0), true))
            }
            _ => None,
        }
    }

    fn literal_number_from_node_or_type(&self, idx: NodeIndex) -> Option<f64> {
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some(value);
        }
        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match self.interner.lookup(type_id)? {
            TypeKey::Literal(LiteralValue::Number(num)) => Some(num.0),
            _ => None,
        }
    }

    fn literal_atom_from_type(&self, idx: NodeIndex) -> Option<Atom> {
        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match self.interner.lookup(type_id)? {
            TypeKey::Literal(LiteralValue::String(atom)) => Some(atom),
            TypeKey::Literal(LiteralValue::Number(num)) => {
                Some(self.atom_from_numeric_value(num.0))
            }
            _ => None,
        }
    }

    fn property_key_from_name(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        let name_idx = self.skip_parens_and_assertions(name_idx);
        let node = self.arena.get(name_idx)?;

        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            if let Some(value) = self.literal_number_from_node_or_type(computed.expression)
                && value.fract() == 0.0 && value >= 0.0 {
                    return Some(PropertyKey::Index(value as usize));
                }
            if let Some(atom) = self.literal_atom_from_node_or_type(computed.expression) {
                return Some(PropertyKey::Atom(atom));
            }
            return None;
        }

        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(PropertyKey::Atom(
                self.interner.intern_string(&ident.escaped_text),
            ));
        }

        if let Some((atom, _)) = self.literal_atom_and_kind_from_node_or_type(name_idx) {
            return Some(PropertyKey::Atom(atom));
        }

        None
    }

    fn literal_number_from_node(&self, idx: NodeIndex) -> Option<f64> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                self.parse_numeric_literal_value(lit.value, &lit.text)
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = self.skip_parenthesized(unary.operand);
                let operand_node = self.arena.get(operand)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.arena.get_literal(operand_node)?;
                let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                Some(if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                })
            }
            _ => None,
        }
    }

    fn atom_from_numeric_value(&self, value: f64) -> Atom {
        let name = if value == 0.0 && value.is_sign_negative() {
            "-0".to_string()
        } else if value.fract() == 0.0 {
            format!("{:.0}", value)
        } else {
            format!("{}", value)
        };
        self.interner.intern_string(&name)
    }

    fn reference_base(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.reference_base(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.reference_base(assertion.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return Some(access.expression);
        }

        None
    }

    fn reference_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let mut visited = Vec::new();
        self.reference_symbol_inner(idx, &mut visited)
    }

    fn reference_symbol_inner(
        &self,
        idx: NodeIndex,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let idx = self.skip_parenthesized(idx);
        if let Some(sym_id) = self
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.binder.resolve_identifier(self.arena, idx))
        {
            return self.resolve_alias_symbol(sym_id, visited);
        }

        let node = self.arena.get(idx)?;
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if self.is_assignment_operator(bin.operator_token) {
                return self.reference_symbol_inner(bin.left, visited);
            }
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            return self.resolve_namespace_member(qn.left, qn.right, visited);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return self.resolve_namespace_member(
                access.expression,
                access.name_or_argument,
                visited,
            );
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name = self.literal_string_from_node(access.name_or_argument)?;
            return self.resolve_namespace_member_by_name(access.expression, name, visited);
        }

        None
    }

    fn resolve_namespace_member(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let right_name = self
            .arena
            .get(right)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;
        self.resolve_namespace_member_by_name(left, right_name, visited)
    }

    fn resolve_namespace_member_by_name(
        &self,
        left: NodeIndex,
        right_name: &str,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let left_sym = self.reference_symbol_inner(left, visited)?;
        let left_sym = self.resolve_alias_symbol(left_sym, visited)?;
        let left_symbol = self.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        let member_sym = exports.get(right_name)?;
        self.resolve_alias_symbol(member_sym, visited)
    }

    fn resolve_alias_symbol(
        &self,
        sym_id: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        if visited.contains(&sym_id) {
            return None;
        }
        visited.push(sym_id);

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import = self.arena.get_import_decl(decl_node)?;
        self.reference_symbol_inner(import.module_specifier, visited)
    }
}

/// Check whether a function body can fall through to the end.
/// Returns true if execution can reach the end of the function body without
/// encountering a return/throw statement.
pub fn function_body_falls_through(_arena: &NodeArena, _body_idx: NodeIndex) -> bool {
    // Simplified stub: assume function bodies can fall through
    // A full implementation would analyze control flow to detect
    // if all paths have return/throw statements
    true
}

/// Check whether a statement can fall through to the next statement.
/// Returns true if execution can continue past this statement.
pub fn statement_falls_through(_arena: &NodeArena, _stmt_idx: NodeIndex) -> bool {
    // Simplified stub: assume statements can fall through
    // A full implementation would analyze control flow
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::solver::PropertyInfo;

    fn get_if_condition(arena: &NodeArena, root: NodeIndex, stmt_index: usize) -> NodeIndex {
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let if_idx = *source_file
            .statements
            .nodes
            .get(stmt_index)
            .expect("if statement");
        let if_node = arena.get(if_idx).expect("if node");
        let if_data = arena.get_if_statement(if_node).expect("if data");
        if_data.expression
    }

    #[test]
    fn test_truthiness_false_branch_narrows_to_falsy() {
        let source = r#"
let x: string | number | boolean | null | undefined;
if (x) {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let union = types.union(vec![
            TypeId::STRING,
            TypeId::NUMBER,
            TypeId::BOOLEAN,
            TypeId::NULL,
            TypeId::UNDEFINED,
        ]);
        let narrowed =
            analyzer.narrow_type_by_condition(union, condition_idx, condition_idx, false);

        let falsy_string = types.literal_string("");
        let falsy_number = types.literal_number(0.0);
        let falsy_boolean = types.literal_boolean(false);

        let key = types.lookup(narrowed).expect("narrowed type");
        match key {
            TypeKey::Union(members) => {
                let members = types.type_list(members);
                assert!(members.contains(&falsy_string));
                assert!(members.contains(&falsy_number));
                assert!(members.contains(&falsy_boolean));
                assert!(members.contains(&TypeId::NULL));
                assert!(members.contains(&TypeId::UNDEFINED));
            }
            _ => panic!("Expected falsy union, got {:?}", key),
        }
    }

    #[test]
    fn test_typeof_false_branch_excludes_type() {
        let source = r#"
let x: string | number;
if (typeof x === "string") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let typeof_node = arena.get(binary.left).expect("typeof node");
        let unary = arena.get_unary_expr(typeof_node).expect("typeof data");
        let target_idx = unary.operand;

        let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let narrowed = analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);
        assert_eq!(narrowed, TypeId::NUMBER);
    }

    #[test]
    fn test_logical_and_applies_right_guard() {
        let source = r#"
let x: string | number;
if (x && typeof x === "string") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let target_idx = binary.left;

        let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let narrowed = analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        assert_eq!(narrowed, TypeId::STRING);
    }

    #[test]
    fn test_logical_or_narrows_to_union_of_literals() {
        let source = r#"
let x: "a" | "b" | "c";
if (x === "a" || x === "b") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let left_node = arena.get(binary.left).expect("left condition");
        let left_eq = arena.get_binary_expr(left_node).expect("left equality");
        let target_idx = left_eq.left;

        let lit_a = types.literal_string("a");
        let lit_b = types.literal_string("b");
        let lit_c = types.literal_string("c");
        let union = types.union(vec![lit_a, lit_b, lit_c]);

        let narrowed_true =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        let narrowed_false =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);

        assert_eq!(narrowed_true, types.union(vec![lit_a, lit_b]));
        assert_eq!(narrowed_false, lit_c);
    }

    #[test]
    fn test_discriminant_property_access_narrows_union() {
        let source = r#"
let action: any;
if (action.type === "add") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let access_node = arena.get(binary.left).expect("property access node");
        let access = arena
            .get_access_expr(access_node)
            .expect("property access data");
        let target_idx = access.expression;

        let type_key = types.intern_string("type");
        let type_add = types.literal_string("add");
        let type_remove = types.literal_string("remove");

        let add_member = types.object(vec![PropertyInfo {
            name: type_key,
            type_id: type_add,
            write_type: type_add,
            optional: false,
            readonly: false,
            is_method: false,
        }]);
        let remove_member = types.object(vec![PropertyInfo {
            name: type_key,
            type_id: type_remove,
            write_type: type_remove,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let union = types.union(vec![add_member, remove_member]);
        let narrowed_true =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        let narrowed_false =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);

        assert_eq!(narrowed_true, add_member);
        assert_eq!(narrowed_false, remove_member);
    }

    #[test]
    fn test_literal_equality_narrows_to_literal() {
        let source = r#"
let x: string | number;
if (x === "a") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let target_idx = binary.left;

        let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let literal_a = types.literal_string("a");
        let narrowed = analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);

        assert_eq!(narrowed, literal_a);
    }

    #[test]
    fn test_loose_nullish_equality_narrows_to_nullish_union() {
        let source = r#"
let x: string | null | undefined;
if (x == null) {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let target_idx = binary.left;

        let union = types.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
        let expected_true = types.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

        let narrowed_true =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        let narrowed_false =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);

        assert_eq!(narrowed_true, expected_true);
        assert_eq!(narrowed_false, TypeId::STRING);
    }
}
