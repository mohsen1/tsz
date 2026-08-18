use super::super::flow_dp::FlowConditionDpMemos;
use super::flow_cache_policy::{FlowCacheBypass, FlowCachePolicy, FlowCacheRead, FlowCacheWrite};
use super::{
    FlowAnalyzer, FlowDeferMemos, defer_to_antecedent, flow_boundary, flow_step_budget, query,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use tsz_binder::{FlowNodeId, SymbolId, flow_flags};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

/// Immutable per-walk parameters for [`FlowAnalyzer::chase_linear_passthrough`],
/// bundled so the chase helper stays within the argument-count budget.
#[derive(Clone, Copy)]
struct PassthroughGate {
    reference: NodeIndex,
    symbol_id: Option<SymbolId>,
    initial_type: TypeId,
    initial_has_type_params: bool,
    skip_cache_for_control_flow_typed_any: bool,
    cache_symbol: SymbolId,
}

impl<'a> FlowAnalyzer<'a> {
    /// Fuel cap for the conditional-expression-merge arm walk
    /// ([`Self::is_conditional_expression_merge`]). Bounds the combined
    /// per-arm passthrough chase and nested-merge recursion so a pathological or
    /// cyclic flow graph cannot loop. Generous relative to real expression
    /// nesting; normal ternary/logical merges resolve in a handful of steps.
    const CONDITIONAL_MERGE_WALK_FUEL: u32 = 256;

    /// Fuel bounding `array_mutation_chain_requires_defer`'s single-antecedent
    /// walk over a straight-line run of `ARRAY_MUTATION` flow nodes, so a
    /// malformed or cyclic flow graph cannot loop. Generous relative to real
    /// interleaved-mutation runs (`a.push(); b.push(); c.push(); …`).
    const ARRAY_MUTATION_CHAIN_WALK_FUEL: u32 = 256;

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
        let mut cache_policy = FlowCachePolicy::new(
            initial_type,
            initial_has_type_params,
            skip_cache_for_control_flow_typed_any,
        );
        // A walk that runs while a loop's fixed point is still iterating reads
        // the provisional assumption `analyze_loop_fixed_point` injects to break
        // its own recursion. Those results are incomplete in `tsc`'s sense —
        // later iterations widen the loop type — so they must not be committed
        // to the shared cache. Committing them pinned the first iteration's
        // guess: in `while (cond) { if (typeof x === "string") { x = x.slice(); } }`
        // with `x: string | number = 0`, the entry guess `number` narrowed to
        // `never`, and that `never` was still cached once the loop type had
        // widened to `string | number`.
        if self.loop_fixed_point_depth.get() > 0 {
            cache_policy.mark_provisional();
        }

        // Initialize worklist with the entry point
        worklist.push_back((flow_id, initial_type));
        in_worklist.insert(flow_id);
        let step_budget = flow_step_budget(self.binder.flow_nodes.len());
        let mut steps = 0usize;
        let mut pending_cache_writes: Vec<((FlowNodeId, SymbolId, TypeId), TypeId)> = Vec::new();
        let mut condition_dp_memos = FlowConditionDpMemos::default();
        // Per-walk defer / CALL narrow-divert classification memos, shared by the
        // chase and the defer classifier so a call-dense scope classifies each node
        // (and extracts each call's predicate signature) at most once per walk.
        let mut defer_memos = FlowDeferMemos::default();
        let mut condition_antecedent_defer_memo: FxHashMap<FlowNodeId, bool> = FxHashMap::default();
        condition_dp_memos.clear();

        // Landed nodes whose spliced pass-through run CONTAINS the entry `flow_id`.
        // When the chase splices out the very node we must return a result for, the
        // landed node's finalized type IS that result (the run is pure pass-through,
        // so `flow_id`'s flow type equals the antecedent result). The landed node may
        // DEFER and re-pop any number of times before finalizing, so we cannot alias
        // eagerly; instead we record the landed node here and write `results[flow_id]`
        // at whichever finalize site (cache hit, visited skip, or final write) resolves
        // it. We deliberately alias ONLY `flow_id`, never interior spliced nodes: an
        // interior node can still be a live antecedent of a surviving merge point that
        // re-schedules and finalizes it on its own; overwriting that with the landed
        // result would corrupt the merge (the `jsxComplexSignature` family). `flow_id`
        // is the sole result read outside the walk, so aliasing it alone is sufficient
        // and never overwrites an independently-computed merge result.
        let mut flow_id_landed_on: Option<FlowNodeId> = None;

        // Process worklist until empty
        while let Some((entry_flow, current_type)) = worklist.pop_front() {
            steps += 1;
            if steps > step_budget {
                // Bail out conservatively to avoid unbounded traversal in pathological CFGs.
                return results.get(&flow_id).copied().unwrap_or(initial_type);
            }
            in_worklist.remove(&entry_flow);

            // O(N²) → O(N) linear pass-through short-circuit.
            //
            // A straight-line run of ASSIGNMENT flow nodes that neither *target*
            // nor *affect* the reference (the prior top-level `const x_j = …`
            // statements when narrowing `x_i`/`x_i.prop`) carries no narrowing:
            // each node's flow type equals its antecedent's. The plain worklist
            // would still pop, cache-probe, hashset-track, and re-push the
            // antecedent for every one of them — Σ O(i) per reference, O(N²) total.
            // The #13404 root pre-filter made each visit cheap to *reject* but the
            // worklist still ENUMERATED all N, so it stayed O(N²) with a smaller
            // constant.
            //
            // Splice the whole run out in O(1) per node by chasing the single
            // antecedent in place, landing on the first node that is NOT a pure
            // pass-through (a merge/condition/call/loop/switch, a targeting or
            // affecting assignment, a node needing defer, or one already
            // finalized/cached). Only that landed node is processed. If the entry
            // `flow_id` itself was among the spliced nodes, its result is aliased
            // from the landed node's once that resolves (`flow_id_landed_on`).
            // `flow_id` is the only result read outside this walk, so we alias it
            // alone and never touch interior spliced nodes: an interior node may
            // still be a live antecedent of a surviving merge that re-schedules and
            // finalizes it independently, and overwriting that would corrupt the
            // merge. Re-scheduling an interior node simply re-runs the cheap chase.
            // Falls back to the full per-node walk whenever any gate is uncertain,
            // so narrowing stays byte-identical.
            let mut passthrough_run_contains_flow_id = false;
            let current_flow = self.chase_linear_passthrough(
                entry_flow,
                PassthroughGate {
                    reference,
                    symbol_id,
                    initial_type,
                    initial_has_type_params,
                    skip_cache_for_control_flow_typed_any,
                    cache_symbol,
                },
                visited,
                results,
                &mut defer_memos,
                flow_id,
                &mut passthrough_run_contains_flow_id,
            );
            // If the chase spliced out the entry `flow_id` itself, remember the node
            // it landed on so the final result can be aliased to `flow_id` once that
            // node resolves (possibly after deferrals). Interior spliced nodes are
            // intentionally left untracked so a surviving merge can re-derive them.
            if current_flow != flow_id && passthrough_run_contains_flow_id {
                flow_id_landed_on = Some(current_flow);
            }

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
            let cache_bypass = FlowCacheBypass::new(
                skip_cache_for_explicit_unknown_switch,
                skip_cache_for_exhaustive_unknown_typeof,
            );

            // Use cache if: 1) not a switch clause, AND
            // 2) either initial type is concrete OR this is a loop label.
            // Loop labels MUST always check cache because analyze_loop_fixed_point
            // injects entries as a recursion guard — skipping the check causes
            // stack overflow when types contain type parameters.
            if cache_policy.allows_read(FlowCacheRead {
                is_switch_clause,
                is_loop_label_node,
                bypass: cache_bypass,
            }) && let Some(cache) = self.flow_cache()
            {
                let key = (current_flow, cache_symbol, initial_type);
                if let Some(&cached_type) = cache.borrow().get(&key) {
                    // Use cached result and skip processing this node. Alias the
                    // cached type to every pass-through node the chase spliced out:
                    // those nodes are pure pass-throughs whose flow type equals this
                    // (landed) node's, so `results[flow_id]` stays correct even when
                    // the landed node was already cached by a prior walk.
                    results.insert(current_flow, cached_type);
                    visited.insert(current_flow);
                    if flow_id_landed_on == Some(current_flow) {
                        results.insert(flow_id, cached_type);
                        visited.insert(flow_id);
                    }
                    continue;
                }
            }

            // Skip if we've already finalized this node. Propagate its result to
            // `flow_id` if the chase spliced `flow_id` onto this landed node.
            if visited.contains(&current_flow) {
                if flow_id_landed_on == Some(current_flow)
                    && let Some(&done) = results.get(&current_flow)
                {
                    results.insert(flow_id, done);
                    visited.insert(flow_id);
                }
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
                        let ant_needs_defer = self.condition_antecedent_requires_defer_cached(
                            ant,
                            reference,
                            symbol_id,
                            &mut condition_antecedent_defer_memo,
                        );
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
                let assignment_roots_may_overlap =
                    self.assignment_root_symbols_may_overlap(flow.node, reference, symbol_id);
                // OPTIMIZATION: Quick symbol-based filtering before expensive AST comparison.
                // If we have a resolved symbol and the assignment's target has a different symbol,
                // we can skip this assignment entirely. This turns O(N²) into O(N) for cases like
                // many independent variable assignments.
                let targets_reference = if !assignment_roots_may_overlap {
                    false
                } else if let Some(target_sym) = symbol_id {
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
                            let mut assignment_type_is_provisional = false;
                            let mut preserve_declared_assignment_flow = false;
                            let raw_assigned = self.get_assigned_type(
                                flow.node,
                                reference,
                                is_destructuring,
                                Some(initial_type),
                                &mut assignment_type_is_provisional,
                                &mut preserve_declared_assignment_flow,
                            );
                            if let Some(assigned_type) =
                                raw_assigned.filter(|&t| t != TypeId::ERROR)
                            {
                                cache_policy.mark_provisional();
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
                            let mut assignment_type_is_provisional = false;
                            let mut preserve_declared_assignment_flow = false;
                            let raw_assigned = self.get_assigned_type(
                                flow.node,
                                reference,
                                is_destructuring,
                                Some(initial_type),
                                &mut assignment_type_is_provisional,
                                &mut preserve_declared_assignment_flow,
                            );
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
                                                || callable_read_preserves_declared_type(
                                                    assigned_type,
                                                );
                                        if preserves_declared_callable_read_type {
                                            initial_type
                                        } else {
                                            // Match tsc's `getTypeAtFlowAssignment`: reduce a union
                                            // declared type to the members compatible with the
                                            // assigned value, but keep a non-union declared type
                                            // verbatim instead of adopting the RHS shape. This
                                            // preserves declared property modifiers — e.g. a nested
                                            // `readonly`, so `r.a.b = ...` still reports TS2540
                                            // after `r.a = { b: 2 }`. `narrow_assignment` returns
                                            // the declared type unchanged when it is not a union.
                                            self.narrow_assignment(initial_type, assigned_type)
                                        }
                                    }
                                } else if is_control_flow_typed_any {
                                    // Unannotated mutable locals such as `let x;` evolve from
                                    // their writes rather than staying explicit `any`.
                                    //
                                    // A `var x = null` / `= undefined` local is control-flow-
                                    // typed-any (unannotated, non-const, bare-nullish-literal
                                    // initializer). In non-strict mode tsc widens the nullish
                                    // initializer to `any` (getWidenedType), so a later read sees
                                    // `any`. Without this, tsz keeps the raw `null`/`undefined`
                                    // flow type and reports spurious TS2407/TS2349/TS2365/TS2403
                                    // (#94) — e.g. `var arr = null; for (i in arr)`. Scoped to a
                                    // bare `null`/`undefined` assigned type (the only initializer
                                    // shape that makes a symbol control-flow-typed-any), so
                                    // non-nullish and union writes are unchanged. Strict mode keeps
                                    // the narrowed nullish type.
                                    if matches!(assigned_type, TypeId::NULL | TypeId::UNDEFINED)
                                        && self
                                            .checker_context
                                            .is_some_and(|c| !c.strict_null_checks())
                                    {
                                        TypeId::ANY
                                    } else {
                                        assigned_type
                                    }
                                } else {
                                    // Killing definition: replace type with RHS type and stop traversal.
                                    // Use the DECLARED type for narrowing (matching tsc's getAssignmentReducedType),
                                    // not initial_type which may be an already-narrowed type from loop analysis.
                                    // This is critical for loops like `let code: 0|1 = 0; while(true) { code = code === 1 ? 0 : 1; }`
                                    // where initial_type is `0` (narrowed) but declared type is `0|1`.
                                    // `annotation_type_from_var_decl_node` only reads
                                    // `VARIABLE_DECLARATION` annotations, and a parameter's
                                    // declaration node is commonly absent from `node_types`
                                    // during loop back-edge walks, so an annotated *parameter*
                                    // binding must recover its declared union from the
                                    // annotation syntax. Without that, the reduction base
                                    // degrades to the loop-narrowed `initial_type` and a
                                    // widening back-edge write (`x = n` with `x: string |
                                    // number` narrowed to `string`) can never re-widen the
                                    // loop-head join.
                                    let declared_type = symbol_id
                                        .and_then(|sid| self.binder.get_symbol(sid))
                                        .filter(|sym| sym.value_declaration.is_some())
                                        .and_then(|sym| {
                                            self.node_types.and_then(|types| {
                                                self.annotation_type_from_var_decl_node(
                                                    sym.value_declaration,
                                                )
                                                .or_else(|| {
                                                    types.get(&sym.value_declaration.0).copied()
                                                })
                                                .or_else(|| {
                                                    self.fallback_declared_annotation_type(
                                                        sym.value_declaration,
                                                    )
                                                })
                                            })
                                        });
                                    // A bare `unique symbol` alias binding caches
                                    // its un-widened `typeof cs` here; widen it so
                                    // the killing definition reduces from `symbol`
                                    // (tsc's widened `getTypeOfVariableDeclaration`)
                                    // and a later `const a: typeof cs = p` read
                                    // sees `symbol` (the real TS2322).
                                    let narrowing_base = self.flow_widen_binding_declared_type(
                                        symbol_id,
                                        declared_type.unwrap_or(initial_type),
                                    );
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
                                cache_policy.mark_provisional();
                                // Preserve specialized sound fallbacks; otherwise keep the
                                // current branch type until canonical RHS typing catches up.
                                if self.is_await_assignment_for_reference(flow.node, reference) {
                                    // `x = await expr` assigns a realized value. When RHS typing
                                    // isn't available yet, keep this sound by at least excluding
                                    // `undefined` from the assignment base.
                                    let declared_type = symbol_id
                                        .and_then(|sid| self.binder.get_symbol(sid))
                                        .filter(|sym| sym.value_declaration.is_some())
                                        .and_then(|sym| {
                                            self.node_types.and_then(|types| {
                                                self.annotation_type_from_var_decl_node(
                                                    sym.value_declaration,
                                                )
                                                .or_else(|| {
                                                    types.get(&sym.value_declaration.0).copied()
                                                })
                                            })
                                        })
                                        .unwrap_or(initial_type);
                                    flow_boundary::narrow_destructuring_default(
                                        self.interner.as_type_database(),
                                        declared_type,
                                        true,
                                    )
                                } else if self.is_killing_definition_with_non_nullish_rhs(
                                    flow.node, reference,
                                ) {
                                    // `target = <object/array literal | new | fn>` always
                                    // writes a definitely non-nullish value, even when the
                                    // RHS type could not be resolved (deferred closure typing
                                    // plus an unreconstructable structural fallback). Drop
                                    // `null`/`undefined` from the declared union so the killing
                                    // definition matches tsc's `getAssignmentReducedType`
                                    // instead of leaving a false TS18048 on a later read.
                                    let declared_type =
                                        symbol_id
                                            .and_then(|sid| self.binder.get_symbol(sid))
                                            .filter(|sym| sym.value_declaration.is_some())
                                            .and_then(|sym| {
                                                self.annotation_type_from_var_decl_node(
                                                    sym.value_declaration,
                                                )
                                                .or_else(|| {
                                                    self.node_types.and_then(|types| {
                                                        types.get(&sym.value_declaration.0).copied()
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
                } else if assignment_roots_may_overlap
                    && self.assignment_affects_reference_node(flow.node, reference)
                {
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
                        // Must defer when the antecedent can carry or merge narrowing and
                        // hasn't been computed yet, otherwise we lose facts flowing into the
                        // mutation site. This mirrors the defer set of the sibling
                        // pass-through path below: a property/element mutation (`m.p = …`)
                        // does not redefine `m`, so when a control-flow join (BRANCH_LABEL)
                        // or switch clause sits between a narrowing guard and the mutation,
                        // failing to defer would re-read the declared (un-narrowed) type and
                        // drop the guard's narrowing of the mutated object.
                        if let Some(&ant) = flow.antecedent.first() {
                            if let Some(&ant_type) = results.get(&ant) {
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
                                                | flow_flags::SWITCH_CLAUSE
                                                | flow_flags::AWAIT_POINT
                                                | flow_flags::YIELD_POINT
                                                | flow_flags::START,
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
                                            | flow_flags::SWITCH_CLAUSE
                                            | flow_flags::AWAIT_POINT
                                            | flow_flags::YIELD_POINT
                                            | flow_flags::START,
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
                            cache_policy.mark_provisional();
                        }
                        evolved_type
                    } else {
                        current_type
                    }
                } else if affects_ref {
                    current_type
                } else if let Some(&ant) = flow.antecedent.first() {
                    // This mutation targets a DIFFERENT array than `reference`, so it
                    // is a pure value pass-through for `reference` and must NOT
                    // re-derive its narrowing. Mirror the non-targeting ASSIGNMENT
                    // pass-through: if the antecedent carries narrowing for
                    // `reference` and has not been resolved yet, defer so the
                    // narrowed type reaches the dependent reader instead of the
                    // declared type. A bare `antecedent_requires_defer` is too
                    // narrow here — it does not recognize a prior `ARRAY_MUTATION`
                    // node (the mutation/evolution of `reference` itself) or a
                    // non-targeting ASSIGNMENT chain that still carries the
                    // assignment-narrowing of `reference` (e.g.
                    // `a = a || []; a.push(1); b.push(1); a.pop()`), so the
                    // unrelated mutation re-widens `a` to `T | undefined` and emits
                    // a false `TS18048`.
                    if let Some(&ant_type) = results.get(&ant) {
                        ant_type
                    } else if !visited.contains(&ant) {
                        let ant_needs_defer = self.binder.flow_nodes.get(ant).is_some_and(|f| {
                            f.has_any_flags(
                                flow_flags::CONDITION
                                    | flow_flags::CALL
                                    | flow_flags::BRANCH_LABEL
                                    | flow_flags::LOOP_LABEL
                                    | flow_flags::ASSIGNMENT
                                    | flow_flags::ARRAY_MUTATION
                                    | flow_flags::SWITCH_CLAUSE
                                    | flow_flags::AWAIT_POINT
                                    | flow_flags::YIELD_POINT
                                    | flow_flags::START,
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
            } else if flow.has_any_flags(flow_flags::CALL) {
                if let Some(&ant) = flow.antecedent.first()
                    && self.antecedent_requires_defer_cached(
                        ant,
                        reference,
                        symbol_id,
                        &mut defer_memos,
                    )
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
            } else if flow.has_any_flags(flow_flags::AWAIT_POINT | flow_flags::YIELD_POINT) {
                // `await`/`yield` suspension points carry no narrowing of their own
                // and tsc does not model them as flow nodes. They must resolve to
                // their single antecedent's (possibly narrowed) type. The generic
                // default handler below would finalize this node with the antecedent's
                // result ONLY when it is already in `results`; on the common first
                // visit (antecedent not yet processed) it falls back to the
                // un-narrowed `current_type` and then marks this node finalized,
                // permanently dropping any guard-applied narrowing that lives on the
                // antecedent. Resolve the antecedent directly so the narrowed type is
                // always carried through the suspension point. (The pass-through
                // splice in `chase_linear_passthrough` handles the concrete, cacheable
                // walks; this branch covers the generic / non-spliced walks.)
                if let Some(&ant) = flow.antecedent.first() {
                    self.get_flow_type(reference, current_type, ant)
                } else {
                    current_type
                }
            } else {
                // Default: continue to antecedent
                if let Some(&ant) = flow.antecedent.first() {
                    if self.antecedent_requires_defer_cached(
                        ant,
                        reference,
                        symbol_id,
                        &mut defer_memos,
                    ) {
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

            // If the chase spliced the entry `flow_id` onto this landed node, write
            // its result now: the spliced run is pure pass-through, so `flow_id`'s
            // flow type equals this antecedent result. Only `flow_id` is aliased
            // (never interior spliced nodes — see `flow_id_landed_on`), so a surviving
            // merge that re-derives an interior node is never overwritten. This is not
            // written to the global flow cache: `flow_id`'s cached entry would be keyed
            // by this reference's `cache_symbol`, which no other reference shares.
            if flow_id_landed_on == Some(current_flow) {
                results.insert(flow_id, final_type);
                visited.insert(flow_id);
            }

            // Store result in global cache for future calls
            // CRITICAL: Only cache if BOTH initial and final types are concrete (no type parameters).
            // This prevents the "Generic Result" bug where narrowing introduces type parameters.
            // Also skip caching UNREACHABLE_NEVER as it's an internal sentinel.
            let final_has_type_params = self.contains_type_parameters_cached(final_type);
            if cache_policy.allows_write(FlowCacheWrite {
                is_loop_label_node: flow.has_any_flags(flow_flags::LOOP_LABEL),
                bypass: FlowCacheBypass::new(
                    initial_type == TypeId::UNKNOWN
                        && self.flow_chain_contains_switch_clause_with_memo(
                            current_flow,
                            &mut condition_dp_memos.switch_chains,
                        ),
                    initial_type == TypeId::UNKNOWN
                        && self.flow_has_exhaustive_typeof_exclusions_with_memo(
                            current_flow,
                            reference,
                            &mut condition_dp_memos.typeof_exclusions,
                        ),
                ),
                final_type,
                final_has_type_params,
                unreachable_never: Self::UNREACHABLE_NEVER,
            }) {
                let key = (current_flow, cache_symbol, initial_type);
                pending_cache_writes.push((key, final_type));
            }
        }

        if cache_policy.allows_pending_writes()
            && let Some(cache) = self.flow_cache()
        {
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

    /// Linear pass-through short-circuit for the `check_flow` worklist.
    ///
    /// Starting at `entry`, chases the single antecedent in place across a run of
    /// *pure pass-through* ASSIGNMENT flow nodes — nodes that neither target nor
    /// affect `reference` — collapsing a straight-line `const`/assignment segment
    /// (the `Σ O(i)` independent-assignment narrowing hotspot) into one landed
    /// node. If the original queried flow node is skipped this way, the chase
    /// records that fact so the caller can alias its result from the landed node
    /// once the landed node resolves.
    ///
    /// A node is only spliced when ALL of these hold, so narrowing is byte
    /// identical to the full walk:
    /// - it is an ASSIGNMENT with no other flow flags (not a merge label, switch
    ///   clause, loop label, call, condition, array mutation, or start);
    /// - it has exactly one antecedent (no join to union);
    /// - it neither targets nor affects `reference`, checked with the same
    ///   `assignment_root_symbols_may_overlap` pre-filter (#13404) plus the symbol
    ///   and `assignment_targets_reference_node` / `assignment_affects_reference_node`
    ///   AST predicates the worklist's own ASSIGNMENT branch uses, so it is neither
    ///   a killing definition, a base reassignment, nor a property mutation of the
    ///   reference;
    /// - its antecedent does not `antecedent_requires_defer` (so we never skip past
    ///   a node carrying pending narrowing such as a condition, call, loop, branch,
    ///   or a targeting assignment); and
    /// - the antecedent is not already finalized/cached for this walk (so we never
    ///   bypass an existing `results`/`visited`/flow-cache answer).
    ///
    /// The chase is disabled for type-parameter-bearing or control-flow-`any`
    /// initial types, mirroring the worklist's own cache-eligibility gate, so it
    /// cannot perturb loop fixed-point or generic-result caching. On any node that
    /// fails a gate the chase stops and returns that node for normal processing.
    fn chase_linear_passthrough(
        &self,
        entry: FlowNodeId,
        gate: PassthroughGate,
        visited: &FxHashSet<FlowNodeId>,
        results: &FxHashMap<FlowNodeId, TypeId>,
        memos: &mut FlowDeferMemos,
        alias_flow_id: FlowNodeId,
        run_contains_alias_flow_id: &mut bool,
    ) -> FlowNodeId {
        let PassthroughGate {
            reference,
            symbol_id,
            initial_type,
            initial_has_type_params,
            skip_cache_for_control_flow_typed_any,
            cache_symbol,
        } = gate;
        // Gate: only collapse when the worklist treats this walk as
        // cacheable/concrete. Generic or control-flow-`any` walks keep the full
        // per-node path so loop fixed-point and generic-result invariants are
        // untouched. `UNKNOWN` is excluded too: the worklist gives UNKNOWN
        // references dedicated switch-clause and exhaustive-typeof handling
        // (`skip_cache_for_explicit_unknown_switch` /
        // `skip_cache_for_exhaustive_unknown_typeof`) whose narrowing flows
        // through the very pass-through `const` nodes the chase would splice, so
        // collapsing them would drop switch/typeof narrowing (e.g. `unknownType2`).
        // UNKNOWN references are catch-clause / explicit-`unknown` shaped, not the
        // concrete member-typed `const` hotspot, so this costs no perf.
        let cache_policy = FlowCachePolicy::new(
            initial_type,
            initial_has_type_params,
            skip_cache_for_control_flow_typed_any,
        );
        if !cache_policy.allows_passthrough_chase() {
            return entry;
        }
        let flow_cache = self.flow_cache();
        let flow_cache = flow_cache.as_ref().map(|cache| cache.borrow());

        // Only pure non-merge ASSIGNMENT flow flags are skippable. Any other flag
        // means the node can apply or merge narrowing and must be processed.
        let pure_assignment_only = |flags: u32| -> bool {
            flags & flow_flags::ASSIGNMENT != 0
                && flags
                    & (flow_flags::BRANCH_LABEL
                        | flow_flags::LOOP_LABEL
                        | flow_flags::SWITCH_CLAUSE
                        | flow_flags::CONDITION
                        | flow_flags::CALL
                        | flow_flags::ARRAY_MUTATION
                        | flow_flags::START)
                    == 0
        };

        // A CALL flow node is splice-eligible only when it is pure value
        // pass-through (neither a never-returning divert nor an `asserts`
        // predicate). The interleaved dispatch-table shape
        // (`const x = f(); const y = x.p; const z = g(); ...`) puts such a CALL
        // between every pair of `const` assignments, so without splicing CALL
        // nodes the chase collapses at most one assignment before stalling and
        // the worklist re-walks the prior chain per reference read (O(N^2)).
        let pure_passthrough_call = |flags: u32| -> bool {
            flags & flow_flags::CALL != 0
                && flags
                    & (flow_flags::BRANCH_LABEL
                        | flow_flags::LOOP_LABEL
                        | flow_flags::SWITCH_CLAUSE
                        | flow_flags::CONDITION
                        | flow_flags::ASSIGNMENT
                        | flow_flags::ARRAY_MUTATION
                        | flow_flags::START)
                    == 0
        };

        // An `await`/`yield` suspension point carries no narrowing of its own and
        // tsc does not model it as a flow node at all: it is a pure value
        // pass-through whose flow type equals its single antecedent's. Without
        // splicing it the worklist's "default" handler returns the antecedent's
        // result only when that antecedent is already finalized, otherwise it falls
        // back to the un-narrowed `current_type`, so an `await` (or `yield`) placed
        // after a narrowing guard and before a later read of the narrowed variable
        // silently drops the narrowing. Splice it just like a pure pass-through
        // CALL. A suspension node is splice-eligible only when it carries no other
        // flow-relevant flag.
        let pure_passthrough_suspension = |flags: u32| -> bool {
            flags & (flow_flags::AWAIT_POINT | flow_flags::YIELD_POINT) != 0
                && flags
                    & (flow_flags::BRANCH_LABEL
                        | flow_flags::LOOP_LABEL
                        | flow_flags::SWITCH_CLAUSE
                        | flow_flags::CONDITION
                        | flow_flags::CALL
                        | flow_flags::ASSIGNMENT
                        | flow_flags::ARRAY_MUTATION
                        | flow_flags::START)
                    == 0
        };

        let mut current = entry;
        loop {
            let Some(flow) = self.binder.flow_nodes.get(current) else {
                return current;
            };
            // Splice a pure pass-through CALL node: it carries no narrowing for
            // any reference, so it can be skipped in O(1) just like a
            // non-targeting assignment. Re-derive the divert/assertion gate via
            // `call_node_may_narrow_or_divert` (the same positive predicate the
            // worklist uses in `antecedent_requires_defer`).
            if pure_passthrough_call(flow.flags) {
                if self.call_node_may_narrow_or_divert_cached(current, flow, &mut memos.call_divert)
                {
                    return current;
                }
                let [ant] = flow.antecedent.as_slice() else {
                    return current;
                };
                let ant = *ant;
                if self.antecedent_requires_defer_cached(ant, reference, symbol_id, memos)
                    || visited.contains(&ant)
                    || results.contains_key(&ant)
                {
                    return current;
                }
                if let Some(cache) = self.flow_cache()
                    && cache
                        .borrow()
                        .contains_key(&(ant, cache_symbol, initial_type))
                {
                    return current;
                }
                if current == alias_flow_id {
                    *run_contains_alias_flow_id = true;
                }
                current = ant;
                continue;
            }
            // Splice a pure pass-through `await`/`yield` suspension point. Mirrors
            // the pure-pass-through CALL handling above: a suspension point with a
            // single antecedent that neither carries pending narrowing nor is
            // already finalized/cached is transparent and can be skipped in O(1),
            // so the chase lands directly on the narrowing-bearing antecedent.
            if pure_passthrough_suspension(flow.flags) {
                let [ant] = flow.antecedent.as_slice() else {
                    return current;
                };
                let ant = *ant;
                if self.antecedent_requires_defer_cached(ant, reference, symbol_id, memos)
                    || visited.contains(&ant)
                    || results.contains_key(&ant)
                {
                    return current;
                }
                if let Some(cache) = self.flow_cache()
                    && cache
                        .borrow()
                        .contains_key(&(ant, cache_symbol, initial_type))
                {
                    return current;
                }
                if current == alias_flow_id {
                    *run_contains_alias_flow_id = true;
                }
                current = ant;
                continue;
            }
            if !pure_assignment_only(flow.flags) {
                return current;
            }
            // A destructuring assignment is never spliced: it has dedicated
            // worklist handling (`is_const_destructuring` defers to the antecedent
            // and re-derives the binding from the narrowed source). The targeting
            // predicates below compare against the binding *pattern* node, not the
            // bound element, so a destructuring node that DOES define the reference
            // (e.g. `const { nested: { b: text } } = aFoo` defining `text` after a
            // guard, `destructuringTypeGuardFlow`) would read as non-targeting and
            // be wrongly skipped, dropping the narrowing. Let the worklist own it.
            if self.is_destructuring_assignment(flow.node) {
                return current;
            }
            // Exactly one antecedent: a join/merge must be processed so its
            // antecedents union correctly.
            let [ant] = flow.antecedent.as_slice() else {
                return current;
            };
            let ant = *ant;

            // The assignment must neither target nor affect the reference. Reuse
            // the #13404 O(1) root pre-filter (`assignment_root_symbols_may_overlap`):
            // when the assignment's root symbol is provably disjoint from the
            // reference's, it is irrelevant and skippable. Only fall back to the
            // deep AST predicates (which the worklist's ASSIGNMENT branch also runs)
            // when the roots may overlap. The classification is a per-walk-pure
            // function of the node, re-derived on every overlapping chase re-scan,
            // so it is memoized by flow-node id alongside the defer / call-divert
            // memos — one classification per node per walk, byte-identical in value.
            if self.assignment_relevant_to_reference_cached(
                current,
                flow.node,
                reference,
                symbol_id,
                &mut memos.assignment_relevant,
            ) {
                return current;
            }

            // Never skip past an antecedent that carries pending narrowing, and
            // never skip one already finalized/cached (let the worklist reuse the
            // existing answer for it).
            if self.antecedent_requires_defer_cached(ant, reference, symbol_id, memos)
                || visited.contains(&ant)
                || results.contains_key(&ant)
            {
                return current;
            }
            if flow_cache
                .as_ref()
                .is_some_and(|cache| cache.contains_key(&(ant, cache_symbol, initial_type)))
            {
                return current;
            }

            // `current` is a pure pass-through: record it and advance.
            if current == alias_flow_id {
                *run_contains_alias_flow_id = true;
            }
            current = ant;
        }
    }

    pub(super) fn antecedent_requires_defer_cached(
        &self,
        antecedent: FlowNodeId,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
        memos: &mut FlowDeferMemos,
    ) -> bool {
        if let Some(&cached) = memos.defer.get(&antecedent) {
            return cached;
        }
        let result = self.antecedent_requires_defer(antecedent, reference, symbol_id, memos);
        memos.defer.insert(antecedent, result);
        result
    }

    /// Whether `label` is the merge `BRANCH_LABEL` of a *conditional/short-circuit
    /// expression* (`a ? b : c`, `a && b`, `a || b`, `a ?? b`) rather than a
    /// statement-level control-flow merge (the join after an `if`, `switch`,
    /// `try`/`catch`, or loop).
    ///
    /// Both kinds merge two CONDITION antecedents, so the flow flags alone do not
    /// distinguish them. The discriminator is the AST parent of the condition
    /// expression each arm records: a ternary's condition parents to a
    /// `ConditionalExpression`, and a logical operator's left operand parents to a
    /// logical `BinaryExpression`. A statement `if`/`while` condition parents to
    /// the statement node instead.
    ///
    /// This matters for narrowing: a non-targeting `const t = <ternary>`
    /// initializer produces an ASSIGNMENT whose antecedent is this merge, and that
    /// merge carries the narrowing established before the conditional. A following
    /// CONDITION node (the next `if`) must defer through the assignment to the
    /// merge so it narrows from the merged type rather than the declared type.
    /// Statement merges are deliberately excluded: deferring through them
    /// re-routes resolution order for targeting-assignment chains inside
    /// `try`/`catch` and over-narrows (e.g. `controlFlowForCatchAndFinally`).
    ///
    /// An arm is not always a *bare* CONDITION: any expression evaluated inside
    /// an arm adds its own flow nodes, so the merge's antecedent can be a `CALL`
    /// (`a ? f() : g()`), an `ASSIGNMENT` (`a ? (x = 1) : 0`), an array mutation,
    /// an `await`/`yield`, or a nested conditional-expression merge
    /// (`a ? (b ? x : y) : z`) instead. Each arm is therefore walked backward
    /// through such pure value-passthrough flow nodes to the CONDITION that
    /// controls it before checking that the condition is an expression operand.
    /// Requiring a bare CONDITION antecedent (the previous behavior) silently
    /// failed for every arm that called a function or otherwise produced a flow
    /// node, dropping unrelated narrowing across the following guard (e.g.
    /// `const t = c ? x : getKeys(x); if (!t) return;`).
    fn is_conditional_expression_merge(&self, label: FlowNodeId) -> bool {
        self.is_conditional_expression_merge_fueled(label, Self::CONDITIONAL_MERGE_WALK_FUEL)
    }

    /// Fuel-bounded core of [`Self::is_conditional_expression_merge`]. The fuel
    /// bounds the combined arm walk plus nested-merge recursion so a pathological
    /// or cyclic flow graph cannot loop; it is decremented on every step and
    /// every recursion.
    fn is_conditional_expression_merge_fueled(&self, label: FlowNodeId, fuel: u32) -> bool {
        if fuel == 0 {
            return false;
        }
        let Some(flow) = self.binder.flow_nodes.get(label) else {
            return false;
        };
        if !flow.has_any_flags(flow_flags::BRANCH_LABEL) || flow.antecedent.is_empty() {
            return false;
        }
        flow.antecedent
            .iter()
            .all(|&ant| self.arm_reaches_expression_condition(ant, fuel - 1))
    }

    /// Walk a conditional/short-circuit merge arm backward through pure value-
    /// passthrough flow nodes (`CALL`, `ASSIGNMENT`, `ARRAY_MUTATION`,
    /// `AWAIT`/`YIELD`, each with a single antecedent) to the CONDITION node that
    /// controls it, returning whether that condition is a conditional or
    /// logical-binary expression operand. A nested `BRANCH_LABEL` arm recurses
    /// into [`Self::is_conditional_expression_merge_fueled`]. Statement-level
    /// structures (`LOOP_LABEL`, `SWITCH_CLAUSE`, `START`), joins with multiple
    /// antecedents, and dead ends are rejected, which is what keeps statement
    /// merges (`if`/`switch`/`try`/loops) excluded.
    fn arm_reaches_expression_condition(&self, arm: FlowNodeId, fuel: u32) -> bool {
        let mut current = arm;
        let mut fuel = fuel;
        loop {
            if fuel == 0 {
                return false;
            }
            fuel -= 1;
            let Some(flow) = self.binder.flow_nodes.get(current) else {
                return false;
            };
            if flow.has_any_flags(flow_flags::CONDITION) {
                return self.condition_node_is_expression_operand(flow.node);
            }
            if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                return self.is_conditional_expression_merge_fueled(current, fuel);
            }
            if flow.has_any_flags(
                flow_flags::LOOP_LABEL | flow_flags::SWITCH_CLAUSE | flow_flags::START,
            ) {
                return false;
            }
            // Pure value-passthrough node: chase its single antecedent.
            let [ant] = flow.antecedent.as_slice() else {
                return false;
            };
            current = *ant;
        }
    }

    /// Whether `condition` (a CONDITION flow node's recorded AST node) is the
    /// condition of a `ConditionalExpression` or an operand of a logical
    /// `&&`/`||`/`??` `BinaryExpression`, as opposed to a statement condition.
    fn condition_node_is_expression_operand(&self, condition: NodeIndex) -> bool {
        if condition.is_none() {
            return false;
        }
        let Some(ext) = self.arena.get_extended(condition) else {
            return false;
        };
        if ext.parent.is_none() {
            return false;
        }
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return true;
        }
        if parent.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(parent)
        {
            return binary.operator_token
                == tsz_scanner::SyntaxKind::AmpersandAmpersandToken as u16
                || binary.operator_token == tsz_scanner::SyntaxKind::BarBarToken as u16
                || binary.operator_token == tsz_scanner::SyntaxKind::QuestionQuestionToken as u16;
        }
        false
    }

    fn condition_antecedent_requires_defer_cached(
        &self,
        antecedent: FlowNodeId,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
        memo: &mut FxHashMap<FlowNodeId, bool>,
    ) -> bool {
        if let Some(&cached) = memo.get(&antecedent) {
            return cached;
        }
        let result = self.condition_antecedent_requires_defer(antecedent, reference, symbol_id);
        memo.insert(antecedent, result);
        result
    }

    fn condition_antecedent_requires_defer(
        &self,
        antecedent: FlowNodeId,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
    ) -> bool {
        let Some(ant_flow) = self.binder.flow_nodes.get(antecedent) else {
            return false;
        };
        let ant_flags = ant_flow.flags;
        let ant_is_assignment = (ant_flags & flow_flags::ASSIGNMENT) != 0;
        // Check if the antecedent ASSIGNMENT targets our reference.
        //
        // The symbol-equality shortcut only works for plain-identifier references,
        // whose `symbol_id` is the binder symbol the assignment also resolves to.
        // Member-access references (`c.r`, `o[k]`) carry no `symbol_id`, so a
        // symbol-only test silently misses an assignment that DOES target them
        // (`c.r = …` before an `if`), and the CONDITION join then drops the
        // assignment's narrowing. Fall back to the same structural targeting
        // predicate the worklist's ASSIGNMENT branch and the sibling
        // `antecedent_requires_defer` classifier use, gated by the O(1) root
        // pre-filter so unrelated assignments stay cheap to reject.
        let ant_is_targeting_assignment = ant_is_assignment
            && ant_flow.node.is_some()
            && self.assignment_root_symbols_may_overlap(ant_flow.node, reference, symbol_id)
            && (symbol_id
                .zip(self.reference_symbol(ant_flow.node))
                .is_some_and(|(target, assignment)| target == assignment)
                || self.assignment_targets_reference_node(ant_flow.node, reference)
                || self.assignment_affects_reference_node(ant_flow.node, reference));
        // Also defer to non-targeting ASSIGNMENT antecedents when
        // their own antecedent chain contains a deferrable node.
        // This covers the pattern: `x = 10; var b = x; typeof x`
        // where the non-targeting ASSIGNMENT (var b = x) passes
        // through to the targeting ASSIGNMENT (x = 10). Without
        // deferring, the CONDITION uses the stale initial_type.
        //
        // A conditional/short-circuit-expression initializer
        // (`const t = a ? b : c`, `const t = a && b`) produces a non-targeting
        // ASSIGNMENT whose antecedent is the BRANCH_LABEL merging the
        // expression's arms. That merge carries the narrowing established before
        // the conditional (e.g. inside `if (x.kind === "min")`), so the next
        // CONDITION must defer through the assignment to the merge. Only
        // conditional-EXPRESSION merges qualify — statement merges (`if`/`switch`/
        // `try`) are excluded to avoid re-ordering targeting-assignment chains.
        // A non-targeting initializer whose value is produced *after* a
        // suspension point (`const data = await c.r.text();`) parents to an
        // `AWAIT_POINT`/`YIELD_POINT`, which in turn sits behind the targeting
        // assignment that carries the narrowing (`c.r = f(); await …; const
        // data = …; if (cond) { c.r }`). Including the suspension flags here
        // (alongside `CALL`, which an `await`ed call already contributes) lets
        // the following CONDITION defer through the initializer to the
        // suspension point, whose own handler resolves the narrowed antecedent.
        let ant_is_passthrough_assignment = !ant_is_targeting_assignment
            && ant_is_assignment
            && ant_flow.antecedent.first().is_some_and(|&grandparent| {
                self.binder.flow_nodes.get(grandparent).is_some_and(|gp| {
                    gp.has_any_flags(
                        flow_flags::CONDITION
                            | flow_flags::CALL
                            | flow_flags::ASSIGNMENT
                            | flow_flags::LOOP_LABEL
                            | flow_flags::AWAIT_POINT
                            | flow_flags::YIELD_POINT,
                    )
                }) || self.is_conditional_expression_merge(grandparent)
            });
        // An `ARRAY_MUTATION` antecedent forces a defer when a
        // `reference`-affecting mutation lies on its straight-line antecedent
        // chain (the node itself, or behind a run of sibling-array pass-through
        // mutations). See `array_mutation_chain_requires_defer`.
        let ant_is_deferring_array_mutation =
            self.array_mutation_chain_requires_defer(antecedent, reference, symbol_id);
        // An `await`/`yield` suspension point is a pure value pass-through (it
        // carries no narrowing of its own) but, exactly like the pass-through
        // ASSIGNMENT case above, it must force a defer when its OWN antecedent
        // carries narrowing that must reach this CONDITION merge — e.g.
        // `c.r = f(); await p; if (cond) { c.r._data }`, where the targeting
        // assignment sits behind the suspension point. Without this the
        // CONDITION finalizes with the un-narrowed pre-type before the
        // suspension point's antecedent is resolved and the narrowing is
        // dropped. Mirrors the sibling `antecedent_requires_defer`'s
        // `ant_is_deferring_suspension` handling for the worklist/chase path.
        let ant_is_deferring_suspension =
            (ant_flags & (flow_flags::AWAIT_POINT | flow_flags::YIELD_POINT)) != 0
                && ant_flow.antecedent.first().is_some_and(|&grandparent| {
                    self.condition_antecedent_requires_defer(grandparent, reference, symbol_id)
                });
        (ant_flags & flow_flags::CONDITION) != 0
            // Closure START nodes may carry the enclosing flow
            // that preserves narrowing for effectively-const captures.
            || (ant_flags & flow_flags::START) != 0
            || (ant_flags & flow_flags::CALL) != 0
            || (ant_flags & flow_flags::LOOP_LABEL) != 0
            || (ant_flags & flow_flags::BRANCH_LABEL) != 0
            || (ant_flags & flow_flags::SWITCH_CLAUSE) != 0
            || ant_is_targeting_assignment
            || ant_is_passthrough_assignment
            || ant_is_deferring_array_mutation
            || ant_is_deferring_suspension
    }

    /// Whether an `ARRAY_MUTATION` flow node `antecedent` carries narrowing of
    /// `reference` that a following CONDITION, CALL, or merge node must defer to.
    ///
    /// An `ARRAY_MUTATION` is, for `reference`, either:
    ///   - a *mutation of `reference` itself* (`reference.push(x)`), which carries
    ///     `reference`'s own assignment-narrowing forward (it must defer); or
    ///   - a *pure value pass-through* (it mutates a different array), which
    ///     carries whatever narrowing its single antecedent carries — so it must
    ///     defer exactly when that antecedent requires a defer, mirroring the
    ///     pass-through CALL and pass-through ASSIGNMENT handling.
    ///
    /// This walks the straight-line single-antecedent run of pass-through array
    /// mutations: it returns `true` as soon as one affects `reference`, and at the
    /// first non-`ARRAY_MUTATION` antecedent it delegates to
    /// `condition_antecedent_requires_defer` (so an upstream narrowing assignment
    /// such as `b = b || []` behind interleaved `a.push(x); b.push(y)` reaches the
    /// reader). Delegating to the CONDITION classifier — not the CALL/chase
    /// `antecedent_requires_defer` — confines this array-mutation deferral to the
    /// branch/join path and keeps it out of the linear-passthrough chase, so
    /// straight-line loop entries (`a = a || []; a.push(x); while (…) {} a.pop()`)
    /// are untouched. `fuel` bounds the array-mutation walk so a malformed or
    /// cyclic flow graph cannot loop.
    pub(super) fn array_mutation_chain_requires_defer(
        &self,
        antecedent: FlowNodeId,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
    ) -> bool {
        let mut current = antecedent;
        let mut fuel = Self::ARRAY_MUTATION_CHAIN_WALK_FUEL;
        loop {
            if fuel == 0 {
                return false;
            }
            fuel -= 1;
            let Some(flow) = self.binder.flow_nodes.get(current) else {
                return false;
            };
            if !flow.has_any_flags(flow_flags::ARRAY_MUTATION) {
                return false;
            }
            if self.array_mutation_flow_affects_reference(current, reference) {
                return true;
            }
            // Pure pass-through mutation of a different array: it carries its
            // antecedent's narrowing. Keep walking while the run stays array
            // mutations; at the first non-mutation antecedent, defer exactly when
            // the CONDITION classifier would defer to it (so an upstream narrowing
            // assignment such as `b = b || []` behind interleaved
            // `a.push(x); b.push(y)` reaches a post-join read). Delegating to the
            // CONDITION classifier (not the CALL/chase classifier) keeps this
            // array-mutation deferral confined to the branch/join path and out of
            // the linear-passthrough chase, so straight-line loop entries are
            // untouched.
            let [ant] = flow.antecedent.as_slice() else {
                return false;
            };
            let ant = *ant;
            let ant_is_array_mutation = self
                .binder
                .flow_nodes
                .get(ant)
                .is_some_and(|f| f.has_any_flags(flow_flags::ARRAY_MUTATION));
            if !ant_is_array_mutation {
                return self.condition_antecedent_requires_defer(ant, reference, symbol_id);
            }
            current = ant;
        }
    }
}
