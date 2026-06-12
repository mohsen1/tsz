use super::super::flow_dp::FlowConditionDpMemos;
use super::{FlowAnalyzer, defer_to_antecedent, flow_boundary, flow_step_budget, query};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use tsz_binder::{FlowNodeId, SymbolId, flow_flags};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    /// Iterative flow graph traversal using a worklist algorithm.
    ///
    /// This replaces the recursive implementation to prevent stack overflow
    /// on deeply nested control flow structures. Uses a `VecDeque` worklist with
    /// cycle detection to process flow nodes iteratively.
    pub(crate) fn check_flow(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_id: FlowNodeId,
        _visited: &mut Vec<FlowNodeId>,
        symbol_id: Option<SymbolId>,
    ) -> TypeId {
        // Reusable buffers to avoid heap allocations in hot path.
        // Use try_borrow_mut to handle re-entrancy safely (e.g. during bidirectional narrowing).
        // PERF: Only allocate local fallback buffers when shared buffers are unavailable.
        let mut worklist_borrow = self.flow_worklist().and_then(|b| b.try_borrow_mut().ok());
        let mut in_worklist_borrow = self
            .flow_in_worklist()
            .and_then(|b| b.try_borrow_mut().ok());
        let mut visited_borrow = self.flow_visited().and_then(|b| b.try_borrow_mut().ok());
        let mut results_borrow = self.flow_results().and_then(|b| b.try_borrow_mut().ok());

        let mut local_worklist;
        let mut local_in_worklist;
        let mut local_visited;
        let mut local_results;

        let worklist = if let Some(ref mut b) = worklist_borrow {
            &mut **b
        } else {
            local_worklist = VecDeque::new();
            &mut local_worklist
        };
        let in_worklist = if let Some(ref mut b) = in_worklist_borrow {
            &mut **b
        } else {
            local_in_worklist = FxHashSet::default();
            &mut local_in_worklist
        };
        let visited = if let Some(ref mut b) = visited_borrow {
            &mut **b
        } else {
            local_visited = FxHashSet::default();
            &mut local_visited
        };
        let results = if let Some(ref mut b) = results_borrow {
            &mut **b
        } else {
            local_results = FxHashMap::default();
            &mut local_results
        };

        // Clear buffers for reuse
        worklist.clear();
        in_worklist.clear();
        visited.clear();
        results.clear();

        // CRITICAL: Check if initial type contains type parameters ONCE, outside the loop.
        // This prevents caching generic types across different instantiations.
        // See: https://github.com/microsoft/TypeScript/issues/9998
        let initial_has_type_params = self.contains_type_parameters_cached(initial_type);
        let control_flow_typed_any_symbol = symbol_id
            .or_else(|| self.reference_symbol(reference))
            .is_some_and(|sid| self.is_control_flow_typed_any_symbol(sid));
        let skip_cache_for_control_flow_typed_any = control_flow_typed_any_symbol;

        // Select the cache symbol (disjoint spaces — see `core.rs`): real binder
        // symbol → structural path key (shared across occurrences) → per-node
        // fallback for anything non-pathy (e.g. `f().x`).
        let cache_symbol = symbol_id
            .or_else(|| self.flow_reference_path_symbol(reference))
            .unwrap_or_else(|| super::per_node_flow_cache_symbol(reference));

        // Initialize worklist with the entry point
        worklist.push_back((flow_id, initial_type));
        in_worklist.insert(flow_id);
        let step_budget = flow_step_budget(self.binder.flow_nodes.len());
        let mut steps = 0usize;
        let mut cacheable_walk = true;
        let mut pending_cache_writes: Vec<((FlowNodeId, SymbolId, TypeId), TypeId)> = Vec::new();
        let mut condition_dp_memos = FlowConditionDpMemos::default();
        condition_dp_memos.clear();

        // Process worklist until empty
        while let Some((current_flow, current_type)) = worklist.pop_front() {
            steps += 1;
            if steps > step_budget {
                // Bail out conservatively to avoid unbounded traversal in pathological CFGs.
                return results.get(&flow_id).copied().unwrap_or(initial_type);
            }
            in_worklist.remove(&current_flow);

            // Check global cache first to avoid redundant traversals.
            // Skip cache for SWITCH_CLAUSE nodes — they must be processed to
            // schedule antecedents and apply narrowing.
            let (is_switch_clause, is_loop_label_node) =
                if let Some(flow) = self.binder.flow_nodes.get(current_flow) {
                    (
                        flow.has_any_flags(flow_flags::SWITCH_CLAUSE),
                        flow.has_any_flags(flow_flags::LOOP_LABEL),
                    )
                } else {
                    (false, false)
                };
            let skip_cache_for_explicit_unknown_switch = initial_type == TypeId::UNKNOWN
                && self.flow_chain_contains_switch_clause_with_memo(
                    current_flow,
                    &mut condition_dp_memos.switch_chains,
                );
            let skip_cache_for_exhaustive_unknown_typeof = initial_type == TypeId::UNKNOWN
                && self.flow_has_exhaustive_typeof_exclusions_with_memo(
                    current_flow,
                    reference,
                    &mut condition_dp_memos.typeof_exclusions,
                );

            // Use cache if: 1) not a switch clause, AND
            // 2) either initial type is concrete OR this is a loop label.
            // Loop labels MUST always check cache because analyze_loop_fixed_point
            // injects entries as a recursion guard — skipping the check causes
            // stack overflow when types contain type parameters.
            if !is_switch_clause
                && (!skip_cache_for_control_flow_typed_any || is_loop_label_node)
                && !skip_cache_for_explicit_unknown_switch
                && !skip_cache_for_exhaustive_unknown_typeof
                && (!initial_has_type_params || is_loop_label_node)
                && let Some(cache) = self.flow_cache()
            {
                let key = (current_flow, cache_symbol, initial_type);
                if let Some(&cached_type) = cache.borrow().get(&key) {
                    // Use cached result and skip processing this node
                    results.insert(current_flow, cached_type);
                    visited.insert(current_flow);
                    continue;
                }
            }

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
            // Check if this is a merge point that needs all antecedents processed first
            let is_switch_fallthrough =
                flow.has_any_flags(flow_flags::SWITCH_CLAUSE) && flow.antecedent.len() > 1;
            let is_loop_header = flow.has_any_flags(flow_flags::LOOP_LABEL);
            let is_call = flow.has_any_flags(flow_flags::CALL);
            // Note: ARRAY_MUTATION merge point check is handled below since we need to check
            // if the mutation actually affects the reference we're analyzing
            let is_merge_point = flow
                .has_any_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL)
                || is_switch_fallthrough
                || is_call; // CRITICAL: CALL nodes need antecedent for assertion functions

            if is_merge_point && !flow.antecedent.is_empty() {
                // Some flow graphs can contain self-antecedent edges on merge nodes.
                // Treat self-edges as already satisfied to avoid requeueing the same
                // node forever before it can be finalized.
                let mut all_ready = true;
                let mut check_antecedent_ready = |ant: FlowNodeId| {
                    if ant != current_flow && !visited.contains(&ant) && !results.contains_key(&ant)
                    {
                        all_ready = false;
                    }
                };
                if is_loop_header {
                    if let Some(&ant) = flow.antecedent.first() {
                        check_antecedent_ready(ant);
                    }
                } else {
                    // BRANCH/SWITCH/CALL merge points check all antecedents.
                    for &ant in &flow.antecedent {
                        check_antecedent_ready(ant);
                    }
                }

                if !all_ready {
                    // Schedule unprocessed antecedents to be processed FIRST (push_front).
                    let mut schedule_antecedent = |ant: FlowNodeId| {
                        if ant == current_flow {
                            return;
                        }
                        if !visited.contains(&ant)
                            && !results.contains_key(&ant)
                            && !in_worklist.contains(&ant)
                        {
                            worklist.push_front((ant, current_type));
                            in_worklist.insert(ant);
                        }
                    };
                    if is_loop_header {
                        if let Some(&ant) = flow.antecedent.first() {
                            schedule_antecedent(ant);
                        }
                    } else {
                        for &ant in &flow.antecedent {
                            schedule_antecedent(ant);
                        }
                    }
                    // Re-add self to the END of worklist to process after antecedents
                    if !in_worklist.contains(&current_flow) {
                        worklist.push_back((current_flow, current_type));
                        in_worklist.insert(current_flow);
                    }
                    continue;
                }
            }

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
                // CRITICAL FIX: Implement proper fixed-point iteration for loops
                //
                // Previous implementation: Simple mutation check (unreliable)
                // New implementation: Fixed-point iteration that unions entry type with back-edge types
                //
                // Fixed-Point Algorithm:
                // 1. Start with entry type (antecedent[0] - before the loop)
                // 2. Get types at all back-edges (antecedents[1+] - continue/end of body)
                // 3. Union entry type with all back-edge types
                // 4. Repeat until type stabilizes (max 5 iterations)
                // 5. If not stabilized, widen to union(entry, initial)
                //
                // This matches TypeScript's behavior where variables in loops have
                // types that depend on both the entry condition and assignments within the loop.

                let entry_type = if let Some(&ant) = flow.antecedent.first() {
                    // Ensure entry is processed (is_merge_point logic guarantees this)
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                };

                // Use fixed-point iteration to determine stable loop type
                self.analyze_loop_fixed_point(
                    current_flow,
                    flow,
                    reference,
                    entry_type,
                    initial_type,
                    symbol_id,
                )
            } else if flow.has_any_flags(flow_flags::CONDITION) {
                // Condition node - apply narrowing
                // CRITICAL: For else-if chains, the antecedent is a CONDITION node
                // from the outer if's false branch. We must wait for it to be computed
                // so we narrow from the already-narrowed type, not the original type.
                let (pre_type, antecedent_id) = if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_type) = results.get(&ant) {
                        // Antecedent already computed — use its narrowed type
                        (ant_type, ant)
                    } else if !visited.contains(&ant) {
                        // Antecedent not yet computed — defer if it could carry
                        // narrowing info we need:
                        //   CONDITION: else-if chains (nested type guards)
                        //   CALL: assertion functions
                        //   LOOP_LABEL: loop fixed-point analysis (incomplete types)
                        //   BRANCH_LABEL: merges after if-return that carry narrowed types
                        //   ASSIGNMENT (targeting our ref): killing definitions that
                        //     narrow the type (e.g. `s = new Set<number>();
                        //     if (s instanceof Set)` — without deferring, we'd narrow
                        //     the declared type instead of the assignment-narrowed type)
                        let ant_flow = self.binder.flow_nodes.get(ant);
                        let ant_flags = ant_flow.map(|f| f.flags).unwrap_or(0);
                        // Check if the antecedent ASSIGNMENT targets our reference.
                        let ant_is_targeting_assignment = (ant_flags & flow_flags::ASSIGNMENT) != 0
                            && ant_flow.is_some_and(|f| {
                                // Quick symbol check: does this assignment target our ref?
                                let assignment_sym = self.reference_symbol(f.node);
                                assignment_sym.is_some()
                                    && symbol_id.is_some()
                                    && assignment_sym == symbol_id
                            });
                        // Also defer to non-targeting ASSIGNMENT antecedents when
                        // their own antecedent chain contains a deferrable node.
                        // This covers the pattern: `x = 10; var b = x; typeof x`
                        // where the non-targeting ASSIGNMENT (var b = x) passes
                        // through to the targeting ASSIGNMENT (x = 10). Without
                        // deferring, the CONDITION uses the stale initial_type.
                        let ant_is_passthrough_assignment = !ant_is_targeting_assignment
                            && (ant_flags & flow_flags::ASSIGNMENT) != 0
                            && ant_flow.is_some_and(|f| {
                                f.antecedent.first().is_some_and(|&grandparent| {
                                    self.binder.flow_nodes.get(grandparent).is_some_and(|gp| {
                                        gp.has_any_flags(
                                            flow_flags::CONDITION
                                                | flow_flags::CALL
                                                | flow_flags::ASSIGNMENT
                                                | flow_flags::LOOP_LABEL,
                                        )
                                    })
                                })
                            });
                        let ant_needs_defer = (ant_flags & flow_flags::CONDITION) != 0
                            // Closure START nodes may carry the enclosing flow
                            // that preserves narrowing for effectively-const captures.
                            || (ant_flags & flow_flags::START) != 0
                            || (ant_flags & flow_flags::CALL) != 0
                            || (ant_flags & flow_flags::LOOP_LABEL) != 0
                            || (ant_flags & flow_flags::BRANCH_LABEL) != 0
                            || (ant_flags & flow_flags::SWITCH_CLAUSE) != 0
                            || ant_is_targeting_assignment
                            || ant_is_passthrough_assignment;
                        if ant_needs_defer {
                            defer_to_antecedent(
                                worklist,
                                in_worklist,
                                ant,
                                current_flow,
                                current_type,
                            );
                            continue;
                        }
                        (current_type, ant)
                    } else {
                        // Antecedent visited but no result — use current_type
                        (current_type, ant)
                    }
                } else {
                    (current_type, FlowNodeId::NONE)
                };

                if initial_type == TypeId::UNKNOWN
                    && self.flow_has_exhaustive_typeof_exclusions_with_memo(
                        current_flow,
                        reference,
                        &mut condition_dp_memos.typeof_exclusions,
                    )
                {
                    query::empty_object_type(self.interner)
                } else {
                    let is_true_branch = flow.has_any_flags(flow_flags::TRUE_CONDITION);
                    self.narrow_type_by_condition_with_dp_memos(
                        pre_type,
                        flow.node,
                        reference,
                        is_true_branch,
                        antecedent_id,
                        &mut condition_dp_memos,
                    )
                }
            } else if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                // Defer if the pre-switch antecedent hasn't been computed yet.
                // Without this, switch clause narrowing uses the stale current_type
                // instead of the narrowed type from prior control flow (e.g., after
                // `if (x !== undefined) { switch(x.kind) { ... } }`).
                if let Some(&ant) = flow.antecedent.first()
                    && !visited.contains(&ant)
                    && !results.contains_key(&ant)
                {
                    defer_to_antecedent(worklist, in_worklist, ant, current_flow, current_type);
                    continue;
                }

                // Switch clause - apply switch-specific narrowing
                self.handle_switch_clause_iterative(reference, current_type, flow, results)
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                // OPTIMIZATION: Quick symbol-based filtering before expensive AST comparison.
                // If we have a resolved symbol and the assignment's target has a different symbol,
                // we can skip this assignment entirely. This turns O(N²) into O(N) for cases like
                // many independent variable assignments.
                let targets_reference = if let Some(target_sym) = symbol_id {
                    // Get the assignment target's symbol (O(1) lookup)
                    let assignment_sym = self.reference_symbol(flow.node);
                    if assignment_sym.is_some() && assignment_sym != Some(target_sym) {
                        // Symbols differ - this assignment cannot target our reference
                        false
                    } else {
                        // Same symbol or couldn't determine - do full check
                        self.assignment_targets_reference_node(flow.node, reference)
                    }
                } else {
                    // No symbol ID - must do full check
                    self.assignment_targets_reference_node(flow.node, reference)
                };
                tracing::trace!(
                    flow_node = ?flow.node,
                    ?reference,
                    targets_reference,
                    "flow ASSIGNMENT considered"
                );
                if targets_reference {
                    // For const symbols declared via destructuring, the declared type
                    // already correctly accounts for all possible values including
                    // default-initializer widening. Recomputing the assigned type from
                    // the destructuring source would use flow-narrowed types of other
                    // variables (e.g. computed property key expressions) and incorrectly
                    // discard union members — the const binding's type must remain the
                    // full union computed at declaration time.
                    let is_const = symbol_id.is_some_and(|sid| self.is_const_symbol(sid));
                    let is_destructuring = self.is_destructuring_assignment(flow.node);
                    let is_const_destructuring = is_const && is_destructuring;
                    if is_const_destructuring {
                        if let Some(&ant) = flow.antecedent.first() {
                            if let Some(&ant_type) = results.get(&ant) {
                                ant_type
                            } else if !visited.contains(&ant) {
                                defer_to_antecedent(
                                    worklist,
                                    in_worklist,
                                    ant,
                                    current_flow,
                                    current_type,
                                );
                                continue;
                            } else {
                                current_type
                            }
                        } else {
                            current_type
                        }
                    } else {
                        let is_control_flow_typed_any = control_flow_typed_any_symbol;
                        let preserve_unknown_catch_type = initial_type == TypeId::UNKNOWN
                            && symbol_id
                                .or_else(|| self.reference_symbol(reference))
                                .is_some_and(|sid| self.is_unknown_catch_variable_symbol(sid));
                        let self_referential_assignment =
                            self.assignment_reads_reference_before_write(flow.node, reference);
                        if self_referential_assignment {
                            // `x = len(x)` still writes the RHS result to `x`.
                            // When the RHS can be resolved, let loop back-edges
                            // contribute that result; otherwise fall back to the
                            // antecedent type until expression checking catches up.
                            let is_destructuring = self.is_destructuring_assignment(flow.node);
                            let raw_assigned =
                                self.get_assigned_type(flow.node, reference, is_destructuring);
                            if let Some(assigned_type) =
                                raw_assigned.filter(|&t| t != TypeId::ERROR)
                            {
                                cacheable_walk = false;
                                assigned_type
                            } else if let Some(&ant) = flow.antecedent.first() {
                                if let Some(&ant_type) = results.get(&ant) {
                                    ant_type
                                } else if !visited.contains(&ant) {
                                    defer_to_antecedent(
                                        worklist,
                                        in_worklist,
                                        ant,
                                        current_flow,
                                        current_type,
                                    );
                                    continue;
                                } else {
                                    current_type
                                }
                            } else {
                                current_type
                            }
                        } else
                        // CRITICAL FIX: Skip "killing definition" narrowing for ANY and ERROR types only
                        // These types should preserve their identity across assignments to match tsc behavior
                        //
                        // IMPORTANT: unknown is NOT included here because it SHOULD be narrowed by assignments
                        // Example: let x: unknown; x = 123; should narrow x to number
                        //
                        // Catch variables with declared/implicit unknown are special:
                        // plain assignments do not change their flow type.
                        //
                        // any absorbs assignments (stays any)
                        // error persists to prevent cascading errors
                        if (initial_type != TypeId::ANY || is_control_flow_typed_any)
                            && initial_type != TypeId::ERROR
                            && !preserve_unknown_catch_type
                        {
                            // Check if this is a destructuring assignment (widens literals to primitives)
                            let is_destructuring = self.is_destructuring_assignment(flow.node);

                            // CRITICAL FIX: Try to get assigned type for ALL assignments, including destructuring
                            // Previously: Only direct assignments (x = ...) worked
                            // Now: Destructuring ([x] = ...) also works because get_assigned_type handles it
                            //
                            // Filter out ERROR types: during loop fixed-point iteration,
                            // node_types may contain ERROR for expressions not yet type-checked
                            // (chicken-and-egg: we need x's type to check `len(x)`, but we need
                            // `len(x)`'s result to determine x's loop type). ERROR is "subtype of
                            // everything" so narrow_assignment would keep all union members,
                            // incorrectly returning the full declared type.
                            let raw_assigned =
                                self.get_assigned_type(flow.node, reference, is_destructuring);
                            if let Some(assigned_type) =
                                raw_assigned.filter(|&t| t != TypeId::ERROR)
                            {
                                let assigned_type = if is_control_flow_typed_any {
                                    query::widen_literal_to_primitive(self.interner, assigned_type)
                                } else {
                                    assigned_type
                                };
                                // For logical assignments (??=, ||=, &&=), the binder creates
                                // a two-branch flow graph: one branch for the short-circuit
                                // (original value, with condition narrowing) and one branch for
                                // the assignment (RHS value). On the assignment branch, the
                                // variable holds exactly the RHS value — skip narrow_assignment
                                // which uses mutual-subtype filtering and can fail when the RHS
                                // type is structurally different from declared union members
                                // (e.g., arrow with different return type).
                                if self.is_logical_assignment(flow.node) {
                                    assigned_type
                                } else if self.is_access_reference(reference) {
                                    if self.arena.get(reference).is_some_and(|node| {
                                        node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                                    }) && initial_has_type_params
                                    {
                                        initial_type
                                    } else {
                                        // Property/element access reads should keep their declared read
                                        // type for constructor-valued and generic callable members.
                                        // A prior write can be assignment-compatible without changing the
                                        // member's declared read surface, especially for interface/class
                                        // members with generic call/construct signatures.
                                        let widened = query::widen_literal_to_primitive(
                                            self.interner,
                                            assigned_type,
                                        );
                                        let callable_read_preserves_declared_type = |type_id| {
                                            let function_shape = query::function_shape_for_type(
                                                self.interner,
                                                type_id,
                                            );
                                            let has_generic_call_signatures =
                                                query::call_signatures_for_type(
                                                    self.interner,
                                                    type_id,
                                                )
                                                .is_some_and(|sigs| {
                                                    sigs.iter()
                                                        .any(|sig| !sig.type_params.is_empty())
                                                });
                                            let construct_signatures =
                                                query::construct_signatures_for_type(
                                                    self.interner,
                                                    type_id,
                                                );
                                            function_shape.as_ref().is_some_and(|shape| {
                                                shape.is_constructor
                                                    || !shape.type_params.is_empty()
                                            }) || has_generic_call_signatures
                                                || construct_signatures
                                                    .as_ref()
                                                    .is_some_and(|sigs| !sigs.is_empty())
                                        };
                                        let preserves_declared_callable_read_type =
                                            callable_read_preserves_declared_type(initial_type)
                                                || callable_read_preserves_declared_type(widened);
                                        if preserves_declared_callable_read_type {
                                            initial_type
                                        } else if self
                                            .flow_assignability_related(widened, initial_type)
                                        {
                                            widened
                                        } else {
                                            initial_type
                                        }
                                    }
                                } else if is_control_flow_typed_any {
                                    // Unannotated mutable locals such as `let x;` evolve from
                                    // their writes rather than staying explicit `any`.
                                    assigned_type
                                } else {
                                    // Killing definition: replace type with RHS type and stop traversal.
                                    // Use the DECLARED type for narrowing (matching tsc's getAssignmentReducedType),
                                    // not initial_type which may be an already-narrowed type from loop analysis.
                                    // This is critical for loops like `let code: 0|1 = 0; while(true) { code = code === 1 ? 0 : 1; }`
                                    // where initial_type is `0` (narrowed) but declared type is `0|1`.
                                    let declared_type = symbol_id
                                        .and_then(|sid| self.binder.get_symbol(sid))
                                        .filter(|sym| sym.value_declaration.is_some())
                                        .and_then(|sym| {
                                            self.node_types.and_then(|nt| {
                                                self.annotation_type_from_var_decl_node(
                                                    sym.value_declaration,
                                                )
                                                .or_else(|| {
                                                    nt.get(&sym.value_declaration.0).copied()
                                                })
                                            })
                                        });
                                    let narrowing_base = declared_type.unwrap_or(initial_type);
                                    // For const declarations with enum types: if the assigned
                                    // type is a member of the enum, narrow directly to the
                                    // member type. This enables flow narrowing for patterns like
                                    // `const e: E = E.ONE` where e should have type E.ONE.
                                    // Only applies to const (not var/let) to avoid changing
                                    // mutable variable semantics.
                                    //
                                    // The assigned value must preserve nominal enum identity:
                                    // bare literals (e.g. `const a: E = 1`) collapse back to
                                    // the declared enum so cross-enum assignments still report
                                    // TS2322.
                                    if self.is_const_variable_declaration(flow.node)
                                        && query::has_enum_components(
                                            self.interner.as_type_database(),
                                            narrowing_base,
                                        )
                                        && self.flow_assignability_related(
                                            assigned_type,
                                            narrowing_base,
                                        )
                                    {
                                        return self.narrow_enum_assignment_target(
                                            narrowing_base,
                                            assigned_type,
                                            narrowing_base,
                                        );
                                    }
                                    if self
                                        .is_unannotated_conditional_variable_initializer(flow.node)
                                    {
                                        narrowing_base
                                    } else {
                                        self.narrow_assignment(narrowing_base, assigned_type)
                                    }
                                }
                            } else {
                                // This walk is provisional: assignment typing has not been computed
                                // for the RHS yet. Do not publish the declared-type result into the
                                // shared flow cache or later reads will reuse a stale answer.
                                cacheable_walk = false;
                                // If we can't resolve the RHS type, conservatively return declared type
                                // The value HAS changed, so we can't continue to antecedent
                                if self.is_await_assignment_for_reference(flow.node, reference) {
                                    // `x = await expr` assigns a realized value. When RHS typing
                                    // isn't available yet, keep this sound by at least excluding
                                    // `undefined` from the assignment base.
                                    let declared_type = symbol_id
                                        .and_then(|sid| self.binder.get_symbol(sid))
                                        .filter(|sym| sym.value_declaration.is_some())
                                        .and_then(|sym| {
                                            self.node_types.and_then(|nt| {
                                                self.annotation_type_from_var_decl_node(
                                                    sym.value_declaration,
                                                )
                                                .or_else(|| {
                                                    nt.get(&sym.value_declaration.0).copied()
                                                })
                                            })
                                        })
                                        .unwrap_or(initial_type);
                                    flow_boundary::narrow_destructuring_default(
                                        self.interner.as_type_database(),
                                        declared_type,
                                        true,
                                    )
                                } else {
                                    current_type
                                }
                            }
                        } else {
                            // For any/error/unknown-catch types: Don't apply narrowing - continue to antecedent
                            // This allows condition narrowing (typeof guards) to still work
                            if let Some(&ant) = flow.antecedent.first() {
                                if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                                    worklist.push_back((ant, current_type));
                                    in_worklist.insert(ant);
                                }
                                *results.get(&ant).unwrap_or(&current_type)
                            } else {
                                current_type
                            }
                        }
                    }
                } else if self.assignment_affects_reference_node(flow.node, reference) {
                    // Two sub-cases of "affects reference":
                    // 1. Base reassignment (obj = ... affects obj.prop): clears narrowing
                    // 2. Property mutation (obj.prop.x = ... affects obj.prop): preserves narrowing
                    //
                    // Check if the assignment targets a BASE of the reference. If so,
                    // the reference value may have changed entirely and narrowing is invalid.
                    let is_base_reassignment =
                        self.assignment_targets_base_of_reference(flow.node, reference);

                    if is_base_reassignment {
                        // Base was reassigned — narrowing is invalidated.
                        // Return initial (declared) type.
                        if let Some(&ant) = flow.antecedent.first()
                            && !in_worklist.contains(&ant)
                            && !visited.contains(&ant)
                        {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                        current_type
                    } else {
                        // Property mutation — preserve narrowing from antecedent.
                        // Must defer when antecedent carries narrowing (CONDITION/CALL/LOOP_LABEL)
                        // and hasn't been computed yet, otherwise we lose facts flowing through
                        // loop headers before entering the mutation site.
                        if let Some(&ant) = flow.antecedent.first() {
                            if let Some(&ant_type) = results.get(&ant) {
                                ant_type
                            } else if !visited.contains(&ant) {
                                let ant_needs_defer =
                                    self.binder.flow_nodes.get(ant).is_some_and(|f| {
                                        f.has_any_flags(
                                            flow_flags::CONDITION
                                                | flow_flags::CALL
                                                | flow_flags::LOOP_LABEL
                                                | flow_flags::ASSIGNMENT,
                                        )
                                    });
                                if ant_needs_defer {
                                    defer_to_antecedent(
                                        worklist,
                                        in_worklist,
                                        ant,
                                        current_flow,
                                        current_type,
                                    );
                                    continue;
                                }
                                if !in_worklist.contains(&ant) {
                                    worklist.push_back((ant, current_type));
                                    in_worklist.insert(ant);
                                }
                                *results.get(&ant).unwrap_or(&current_type)
                            } else {
                                current_type
                            }
                        } else {
                            current_type
                        }
                    }
                } else {
                    // This assignment doesn't affect our reference — pass through to antecedent.
                    // CRITICAL: If the antecedent hasn't been processed yet, we must defer to
                    // avoid losing narrowing. Without this, the worklist may process this
                    // ASSIGNMENT before its antecedent chain is resolved, using the un-narrowed
                    // type. This applies to CONDITION nodes (which directly narrow), CALL nodes
                    // (assertion functions), BRANCH_LABEL (merges), and also ASSIGNMENT chains
                    // that may themselves lead to conditions (e.g. `let v1 = x; let v2 = x;`
                    // inside an `if (x instanceof C)` block).
                    if let Some(&ant) = flow.antecedent.first() {
                        if let Some(&ant_type) = results.get(&ant) {
                            // Antecedent already computed — use its result
                            ant_type
                        } else if !visited.contains(&ant) {
                            let ant_needs_defer =
                                self.binder.flow_nodes.get(ant).is_some_and(|f| {
                                    f.has_any_flags(
                                        flow_flags::CONDITION
                                            | flow_flags::CALL
                                            | flow_flags::BRANCH_LABEL
                                            | flow_flags::LOOP_LABEL
                                            | flow_flags::ASSIGNMENT
                                            | flow_flags::SWITCH_CLAUSE,
                                    )
                                });
                            if ant_needs_defer {
                                defer_to_antecedent(
                                    worklist,
                                    in_worklist,
                                    ant,
                                    current_flow,
                                    current_type,
                                );
                                continue;
                            }
                            if !in_worklist.contains(&ant) {
                                worklist.push_back((ant, current_type));
                                in_worklist.insert(ant);
                            }
                            *results.get(&ant).unwrap_or(&current_type)
                        } else {
                            current_type
                        }
                    } else {
                        current_type
                    }
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

                let affects_ref = self.array_mutation_affects_reference(call, reference);
                let needs_antecedent = affects_ref && !flow.antecedent.is_empty();

                if needs_antecedent {
                    if let Some(&ant) = flow.antecedent.first() {
                        if !visited.contains(&ant) && !results.contains_key(&ant) {
                            defer_to_antecedent(
                                worklist,
                                in_worklist,
                                ant,
                                current_flow,
                                current_type,
                            );
                            continue;
                        }
                        let antecedent_type = *results.get(&ant).unwrap_or(&current_type);
                        let (evolved_type, complete) =
                            self.array_mutation_evolved_type(antecedent_type, call, reference);
                        if !complete {
                            cacheable_walk = false;
                        }
                        evolved_type
                    } else {
                        current_type
                    }
                } else if affects_ref {
                    current_type
                } else if let Some(&ant) = flow.antecedent.first() {
                    if self.antecedent_requires_defer(ant, reference, symbol_id)
                        && !visited.contains(&ant)
                        && !results.contains_key(&ant)
                    {
                        defer_to_antecedent(worklist, in_worklist, ant, current_flow, current_type);
                        continue;
                    }
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            } else if flow.has_any_flags(flow_flags::CALL) {
                if let Some(&ant) = flow.antecedent.first()
                    && self.antecedent_requires_defer(ant, reference, symbol_id)
                    && !visited.contains(&ant)
                    && !results.contains_key(&ant)
                {
                    defer_to_antecedent(worklist, in_worklist, ant, current_flow, current_type);
                    continue;
                }
                self.handle_call_iterative(reference, current_type, flow, results)
            } else if flow.has_any_flags(flow_flags::START) {
                // Start node - check if we're crossing a closure boundary.
                //
                // For "effectively mutable" captured variables (let/var that are
                // actually reassigned), we cannot trust narrowing from outer scope
                // because the closure may execute after the variable is mutated.
                //
                // For "effectively const" variables (const, or parameters/let/var
                // that are never reassigned), narrowing is preserved. This implements
                // tsc's "implicit const parameter" feature.
                let outer_flow_id = flow.antecedent.first().copied().or_else(|| {
                    // START with no antecedents - try to find outer flow via node_flow map
                    if flow.node.is_some() {
                        self.binder.node_flow.get(&flow.node.0).copied()
                    } else {
                        None
                    }
                });

                if let Some(outer_flow) = outer_flow_id {
                    if self.reference_uses_outer_class_property_initializer_capture(reference) {
                        // Class property initializers run outside the surrounding
                        // function's flow point, so outer bindings do not inherit its narrowing.
                        initial_type
                    } else if self.is_member_like_reference(reference) {
                        // Property/element-access (and qualified-name) references do not
                        // inherit control-flow narrowing across a function/closure boundary:
                        // the closure may run after the property has been reassigned, so tsc
                        // resets such references to their declared type at the function start
                        // (mirrors the `PropertyAccessExpression`/`ElementAccessExpression`
                        // exclusion in `getTypeAtFlowNode`'s `FlowStart` handling). Only the
                        // base of an immediately-invoked function expression keeps narrowing,
                        // and those are bound inline without a `START` boundary, so they never
                        // reach this branch.
                        initial_type
                    } else if self.is_captured_variable(reference)
                        && !self.is_effectively_const_for_narrowing(reference)
                    {
                        // Captured mutable variable that IS reassigned -
                        // cannot use narrowing from outer scope
                        initial_type
                    } else {
                        // Const or local variable - preserve narrowing from outer scope.
                        // Recursively resolve the outer flow to get the narrowed type.
                        // This is needed because the iterative worklist processes START
                        // before its outer antecedent, so the result wouldn't propagate back.
                        self.check_flow(reference, initial_type, outer_flow, _visited, symbol_id)
                    }
                } else {
                    current_type
                }
            } else {
                // Default: continue to antecedent
                if let Some(&ant) = flow.antecedent.first() {
                    if self.antecedent_requires_defer(ant, reference, symbol_id) {
                        self.get_flow_type(reference, current_type, ant)
                    } else {
                        if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                        *results.get(&ant).unwrap_or(&current_type)
                    }
                } else {
                    current_type
                }
            };

            // For merge points (BRANCH_LABEL, LOOP_LABEL, SWITCH with fallthrough),
            // we union with antecedent types. For SWITCH_CLAUSE, union clause_type with fallthrough.
            let final_type = if is_switch_fallthrough {
                // Union clause_type (result_type) with fallthrough types (antecedent index 1+)
                let mut types = vec![result_type];
                for &ant in flow.antecedent.iter().skip(1) {
                    if let Some(&t) = results.get(&ant) {
                        types.push(t);
                    }
                }
                let types = self.simplify_flow_merge_types(types);
                let merged_type = if types.len() == 1 {
                    types[0]
                } else {
                    query::union_types(self.interner, types)
                };

                // Preserve pre-switch identity (e.g. named alias display like `MyType`)
                // when the merged type semantically equals that original union.
                if let Some(&pre_switch_ant) = flow.antecedent.first() {
                    let pre_switch_type = *results.get(&pre_switch_ant).unwrap_or(&current_type);
                    if self.same_union_member_set(merged_type, pre_switch_type) {
                        pre_switch_type
                    } else {
                        merged_type
                    }
                } else {
                    merged_type
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL) {
                // LOOP_LABEL: use result_type directly from analyze_loop_fixed_point.
                // The fixed-point iteration already computes the correct union of entry
                // type and back-edge types. Re-unioning antecedent results here would
                // give the wrong answer because back-edge results are computed inside
                // analyze_loop_fixed_point's internal get_flow_type calls (which have
                // their own check_flow invocations with separate `results` maps) and
                // are NOT present in our local `results` map.
                result_type
            } else if flow.has_any_flags(flow_flags::BRANCH_LABEL) && !flow.antecedent.is_empty() {
                // Union all antecedent types for branch merge points.
                // Filter out UNREACHABLE_NEVER from dead branches (e.g., branches that
                // terminate via a never-returning function call like `fail()`).
                // Regular NEVER (from exhaustive narrowing) is NOT filtered.
                let is_unreachable = |t: &TypeId| *t == Self::UNREACHABLE_NEVER;

                let all_ant_types: Vec<TypeId> = flow
                    .antecedent
                    .iter()
                    .filter_map(|&ant| results.get(&ant).copied())
                    .collect();

                // Only filter unreachable branches if there are live branches
                let ant_types: Vec<TypeId> = if all_ant_types.iter().any(|t| !is_unreachable(t)) {
                    all_ant_types
                        .into_iter()
                        .filter(|t| !is_unreachable(t))
                        .collect()
                } else {
                    all_ant_types
                };
                let ant_types = self.simplify_flow_merge_types(ant_types);

                if initial_type == TypeId::UNKNOWN
                    && self.flow_has_exhaustive_typeof_exclusions_with_memo(
                        current_flow,
                        reference,
                        &mut condition_dp_memos.typeof_exclusions,
                    )
                {
                    query::empty_object_type(self.interner)
                } else {
                    match ant_types.len() {
                        0 => result_type,
                        1 => ant_types[0],
                        _ if initial_type == TypeId::ANY
                            && !control_flow_typed_any_symbol
                            && ant_types.contains(&TypeId::ANY) =>
                        {
                            TypeId::ANY
                        }
                        _ => self.interner.union_preserve_members(ant_types),
                    }
                }
            } else {
                result_type
            };

            results.insert(current_flow, final_type);
            visited.insert(current_flow);

            // Store result in global cache for future calls
            // CRITICAL: Only cache if BOTH initial and final types are concrete (no type parameters).
            // This prevents the "Generic Result" bug where narrowing introduces type parameters.
            // Also skip caching UNREACHABLE_NEVER as it's an internal sentinel.
            if final_type != Self::UNREACHABLE_NEVER
                && cacheable_walk
                && (!skip_cache_for_control_flow_typed_any
                    || flow.has_any_flags(flow_flags::LOOP_LABEL))
                && !(initial_type == TypeId::UNKNOWN
                    && self.flow_chain_contains_switch_clause_with_memo(
                        current_flow,
                        &mut condition_dp_memos.switch_chains,
                    ))
                && !(initial_type == TypeId::UNKNOWN
                    && self.flow_has_exhaustive_typeof_exclusions_with_memo(
                        current_flow,
                        reference,
                        &mut condition_dp_memos.typeof_exclusions,
                    ))
            {
                let final_has_type_params = self.contains_type_parameters_cached(final_type);

                // Only cache if neither initial nor final types contain type parameters
                if !initial_has_type_params && !final_has_type_params {
                    let key = (current_flow, cache_symbol, initial_type);
                    pending_cache_writes.push((key, final_type));
                }
            }
        }

        if cacheable_walk && let Some(cache) = self.flow_cache() {
            let mut cache = cache.borrow_mut();
            for (key, value) in pending_cache_writes {
                cache.insert(key, value);
            }
        }

        // Return the result for the initial flow_id.
        // When flow analysis returns UNREACHABLE_NEVER (from a never-returning call
        // like `fail()`), replace it with the declared type. This matches tsc's behavior
        // where getFlowTypeOfReference returns declaredType when the result is
        // unreachableNeverType. Unreachable code preserves the declared type so that
        // property accesses don't produce false TS2339 errors.
        // Regular TypeId::NEVER (from exhaustive narrowing) is NOT affected.
        let result = results.get(&flow_id).copied().unwrap_or(initial_type);
        if result == Self::UNREACHABLE_NEVER {
            initial_type
        } else {
            result
        }
    }
}
