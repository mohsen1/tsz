//! Constraint-solving finalization for `InferenceContext`.
//!
//! The tail phase of inference resolution: unifying circular constraints via
//! Tarjan SCC, strengthening constraints, propagating candidates to upper bounds,
//! and fixing the current inference variables into a `TypeSubstitution`. Extracted
//! verbatim from `infer_resolve.rs` to keep that shard under the size limit.

use crate::inference::infer::{
    InferenceContext, InferenceError, InferenceInfo, InferenceVar, MAX_CONSTRAINT_ITERATIONS,
};
use crate::instantiation::instantiate::TypeSubstitution;
use crate::types::{InferencePriority, TypeData, TypeId};
use tsz_common::interner::Atom;

impl<'a> InferenceContext<'a> {
    /// Detect and unify type parameters that form circular constraints.
    /// For example, if T extends U and U extends T, they should be unified
    /// into a single equivalence class for inference purposes.
    fn unify_circular_constraints(&mut self) -> Result<(), InferenceError> {
        use rustc_hash::{FxHashMap, FxHashSet};

        let type_params: Vec<_> = self.type_params.clone();

        // Build adjacency list: var -> set of vars it extends (upper bounds)
        let mut graph: FxHashMap<InferenceVar, FxHashSet<InferenceVar>> = FxHashMap::default();
        let mut var_for_param: FxHashMap<Atom, InferenceVar> = FxHashMap::default();

        for (name, var, _) in &type_params {
            let root = self.table.find(*var);
            var_for_param.insert(*name, root);
            graph.entry(root).or_default();
        }

        // Populate edges based on upper_bounds
        for (_name, var, _) in &type_params {
            let root = self.table.find(*var);
            let info = self.table.probe_value(root);

            for &upper in &info.upper_bounds {
                // Only follow naked type parameter upper bounds (not List<T>, etc.)
                if let Some(TypeData::TypeParameter(param_info)) = self.interner.lookup(upper)
                    && let Some(&upper_var) = var_for_param.get(&param_info.name)
                {
                    let upper_root = self.table.find(upper_var);
                    // Add edge: root extends upper_root
                    graph.entry(root).or_default().insert(upper_root);
                }
            }
        }

        // Find SCCs using Tarjan's algorithm
        let mut index_counter = 0;
        let mut indices: FxHashMap<InferenceVar, usize> = FxHashMap::default();
        let mut lowlink: FxHashMap<InferenceVar, usize> = FxHashMap::default();
        let mut stack: Vec<InferenceVar> = Vec::new();
        let mut on_stack: FxHashSet<InferenceVar> = FxHashSet::default();
        let mut sccs: Vec<Vec<InferenceVar>> = Vec::new();

        struct TarjanState<'a> {
            graph: &'a FxHashMap<InferenceVar, FxHashSet<InferenceVar>>,
            index_counter: &'a mut usize,
            indices: &'a mut FxHashMap<InferenceVar, usize>,
            lowlink: &'a mut FxHashMap<InferenceVar, usize>,
            stack: &'a mut Vec<InferenceVar>,
            on_stack: &'a mut FxHashSet<InferenceVar>,
            sccs: &'a mut Vec<Vec<InferenceVar>>,
        }

        fn strongconnect(var: InferenceVar, state: &mut TarjanState) {
            state.indices.insert(var, *state.index_counter);
            state.lowlink.insert(var, *state.index_counter);
            *state.index_counter += 1;
            state.stack.push(var);
            state.on_stack.insert(var);

            if let Some(neighbors) = state.graph.get(&var) {
                for &neighbor in neighbors {
                    if !state.indices.contains_key(&neighbor) {
                        strongconnect(neighbor, state);
                        let neighbor_low = *state.lowlink.get(&neighbor).unwrap_or(&0);
                        let var_low = state
                            .lowlink
                            .get_mut(&var)
                            .expect("var was inserted into lowlink above");
                        *var_low = (*var_low).min(neighbor_low);
                    } else if state.on_stack.contains(&neighbor) {
                        let neighbor_idx = *state.indices.get(&neighbor).unwrap_or(&0);
                        let var_low = state
                            .lowlink
                            .get_mut(&var)
                            .expect("var was inserted into lowlink above");
                        *var_low = (*var_low).min(neighbor_idx);
                    }
                }
            }

            if *state.lowlink.get(&var).unwrap_or(&0) == *state.indices.get(&var).unwrap_or(&0) {
                let mut scc = Vec::new();
                loop {
                    let w = state
                        .stack
                        .pop()
                        .expect("Tarjan SCC invariant: stack non-empty while processing component");
                    state.on_stack.remove(&w);
                    scc.push(w);
                    if w == var {
                        break;
                    }
                }
                state.sccs.push(scc);
            }
        }

        // Run Tarjan's on all nodes
        for &var in graph.keys() {
            if !indices.contains_key(&var) {
                let mut state = TarjanState {
                    graph: &graph,
                    index_counter: &mut index_counter,
                    indices: &mut indices,
                    lowlink: &mut lowlink,
                    stack: &mut stack,
                    on_stack: &mut on_stack,
                    sccs: &mut sccs,
                };
                strongconnect(var, &mut state);
            }
        }

        // Unify variables within each SCC (if SCC has >1 member)
        for scc in sccs {
            if scc.len() > 1 {
                // Unify all variables in this SCC
                let first = scc[0];
                for &other in &scc[1..] {
                    self.unify_vars(first, other)?;
                }
            }
        }

        Ok(())
    }

    /// Strengthen constraints by analyzing relationships between type parameters.
    /// For example, if T <: U and we know T = string, then U must be at least string.
    pub fn strengthen_constraints(&mut self) -> Result<(), InferenceError> {
        // Detect and unify circular constraints (SCCs)
        // This ensures that type parameters in cycles (T extends U, U extends T)
        // are treated as a single equivalence class for inference.
        self.unify_circular_constraints()?;

        let type_params: Vec<_> = self.type_params.clone();
        let mut changed = true;
        let mut iterations = 0;

        // Fixed-point propagation
        // Iterate to fixed point - continue until no new candidates are added
        while changed && iterations < MAX_CONSTRAINT_ITERATIONS {
            changed = false;
            iterations += 1;

            for (name, var, _) in &type_params {
                let root = self.table.find(*var);

                // We need to clone info to avoid borrow checker issues while mutating
                // This is expensive but necessary for correctness in this design
                let info = self.table.probe_value(root).clone();

                // Propagate candidates UP the extends chain
                // If T extends U (T <: U), then candidates of T are also candidates of U
                for &upper in &info.upper_bounds {
                    if self.propagate_candidates_to_upper(root, upper, *name)? {
                        changed = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Propagates candidates from a subtype (var) to its supertype (upper).
    /// If `var extends upper` (var <: upper), then candidates of `var` are also candidates of `upper`.
    fn propagate_candidates_to_upper(
        &mut self,
        var_root: InferenceVar,
        upper: TypeId,
        exclude_param: Atom,
    ) -> Result<bool, InferenceError> {
        // Check if 'upper' is a type parameter we are inferring
        if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(upper)
            && info.name != exclude_param
            && let Some(upper_var) = self.find_type_param(info.name)
        {
            let upper_root = self.table.find(upper_var);

            // Don't propagate to self
            if var_root == upper_root {
                return Ok(false);
            }

            // Get candidates from the subtype (var)
            let var_candidates = self.table.probe_value(var_root).candidates;

            // Add them to the supertype (upper)
            let mut changed = false;
            for candidate in var_candidates {
                // Use Circular priority to indicate this came from propagation
                if self.add_candidate_if_new(
                    upper_root,
                    candidate.type_id,
                    InferencePriority::Circular,
                ) {
                    changed = true;
                }
            }
            return Ok(changed);
        }
        Ok(false)
    }

    /// Helper to track if we actually added something (for fixed-point loop)
    fn add_candidate_if_new(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);

        // Check if type already exists in candidates
        if info.candidates.iter().any(|c| c.type_id == ty) {
            return false;
        }

        self.add_candidate(var, ty, priority);
        true
    }

    /// Fix (resolve) inference variables that have candidates from Round 1.
    ///
    /// This is called after processing non-contextual arguments to "fix" type
    /// variables that have enough information, before processing contextual
    /// arguments (like lambdas) in Round 2.
    ///
    /// The fixing process:
    /// 1. Finds variables with candidates but no resolved type yet
    /// 2. Computes their best current type from candidates
    /// 3. Sets the `resolved` field to prevent Round 2 from overriding
    ///
    /// Variables without candidates are NOT fixed (they might get info from Round 2).
    ///
    /// The optional `external_is_subtype` closure provides access to the full
    /// checker-level assignability check, which is needed for Lazy (interface/class)
    /// types that the simplified BCT checker cannot resolve through extends chains.
    pub fn fix_current_variables_with<F>(
        &mut self,
        mut external_is_subtype: Option<F>,
    ) -> Result<(), InferenceError>
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        let type_params: Vec<_> = self.type_params.clone();

        for (_name, var, _is_const) in &type_params {
            let root = self.table.find(*var);
            let info = self.table.probe_value(root);

            // Skip if already resolved
            if info.resolved.is_some() {
                continue;
            }

            // Skip if no candidates yet (might get info from Round 2)
            if info.candidates.is_empty() && info.contra_candidates.is_empty() {
                continue;
            }

            // Compute the current best type from existing candidates.
            // Mirror the unknown/error candidate filtering from compute_constraint_result:
            // when informative upper bounds exist, discard unknown/error covariant candidates
            // so that contra-candidates (from contravariant positions like function params)
            // can drive inference instead.
            let is_const = self.is_var_const(root);
            let dc = self.declared_constraints.get(&root).copied();
            let dc_preserves_literals =
                self.literal_preserving_declared_constraints.contains(&root);
            let mut candidates = self.discard_self_referential_candidates(root, &info.candidates);
            if !info.upper_bounds.is_empty() {
                let has_informative_upper_bound = info
                    .upper_bounds
                    .iter()
                    .any(|&upper| !upper.is_any_unknown_or_error());
                let has_concrete_candidate = candidates
                    .iter()
                    .any(|c| !c.type_id.is_any_unknown_or_error());
                candidates.retain(|candidate| match candidate.type_id {
                    TypeId::UNKNOWN | TypeId::ERROR => false,
                    TypeId::ANY => !has_informative_upper_bound || !has_concrete_candidate,
                    _ => true,
                });
            }
            let mut concrete_contra_candidates: Vec<_> = self
                .discard_self_referential_candidates(root, &info.contra_candidates)
                .into_iter()
                .filter(|c| self.is_concrete_contra_candidate(c.type_id))
                .collect();
            // Mirror the priority filter from `compute_constraint_result`: when
            // both co- and contra-variant candidates exist, drop contra-candidates
            // with strictly worse priority than the best covariant priority.
            // Without this, low-priority `LiteralKeyof` contras can override
            // high-priority `NakedTypeVariable` covariants during round-fixing.
            if !candidates.is_empty()
                && !concrete_contra_candidates.is_empty()
                && let Some(best_cov_priority) = candidates.iter().map(|c| c.priority).min()
            {
                concrete_contra_candidates.retain(|c| c.priority <= best_cov_priority);
            }
            let skip_literal_widening = self.top_level_in_return_type_unfixed.contains(&root);
            let spread_rest_mode = self.spread_rest_var_modes.get(&root).copied();
            let result = if !candidates.is_empty() {
                let covariant_result = self.resolve_from_candidates(
                    &candidates,
                    is_const,
                    &info.upper_bounds,
                    dc,
                    dc_preserves_literals,
                    skip_literal_widening,
                    self.root_preserves_return_position_literals(root),
                    spread_rest_mode,
                );
                // (TypeParameter filtering already done above)
                if !concrete_contra_candidates.is_empty() {
                    self.resolve_covariant_against_contra(
                        covariant_result,
                        candidates.iter().any(|c| c.from_readonly_source),
                        &concrete_contra_candidates,
                        candidates.iter().any(|c| c.from_array_element),
                        dc,
                        spread_rest_mode,
                        external_is_subtype
                            .as_mut()
                            .map(|e| e as &mut dyn FnMut(TypeId, TypeId) -> bool),
                    )
                } else {
                    covariant_result
                }
            } else if !concrete_contra_candidates.is_empty() {
                self.resolve_from_contra_candidates(&concrete_contra_candidates)
            } else {
                // All covariant candidates were filtered; fall back to upper bounds
                if info.upper_bounds.len() == 1 {
                    info.upper_bounds[0]
                } else if !info.upper_bounds.is_empty() {
                    self.interner.intersection(info.upper_bounds.clone())
                } else {
                    TypeId::UNKNOWN
                }
            };
            // Check for occurs (recursive type)
            if self.occurs_in(root, result) {
                // Don't fix variables with occurs - let them be resolved later
                continue;
            }

            // Don't fix a variable to `never` when it's the only covariant
            // candidate and there are no contra-candidates. `never` is the
            // bottom type and provides no useful contextual type for Round 2.
            // If a deferred (context-sensitive) argument provides a better
            // candidate in Round 2, it should be used instead. If `never` is
            // truly the correct inference, the final resolution will still
            // produce `never` after all rounds complete.
            if result == TypeId::NEVER
                && !candidates.is_empty()
                && candidates.iter().all(|c| c.type_id == TypeId::NEVER)
                && concrete_contra_candidates.is_empty()
            {
                continue;
            }

            // Fix this variable by setting resolved field
            // This prevents Round 2 from overriding with lower-priority constraints
            self.table.union_value(
                root,
                InferenceInfo {
                    resolved: Some(result),
                    // Keep candidates and upper_bounds for later validation
                    candidates: info.candidates,
                    contra_candidates: info.contra_candidates,
                    upper_bounds: info.upper_bounds,
                },
            );
        }

        Ok(())
    }

    /// Fix (resolve) inference variables without an external checker.
    /// Convenience wrapper for `fix_current_variables_with(None)`.
    pub fn fix_current_variables(&mut self) -> Result<(), InferenceError> {
        self.fix_current_variables_with(None::<fn(TypeId, TypeId) -> bool>)
    }

    /// The `#17282` Round-1-fix snapshot for `var`: the pristine covariant-only
    /// fix when an unannotated (context-sensitive) callback parameter has
    /// contributed a contra-candidate that may have narrowed the fix away from
    /// the direct-argument inference, else `resolved` unchanged.
    pub fn round1_fix_snapshot(&mut self, var: InferenceVar, resolved: TypeId) -> TypeId {
        if self.var_has_unannotated_contra_candidate(var) {
            self.probe_covariant_only(var).unwrap_or(resolved)
        } else {
            resolved
        }
    }

    /// Resolve `var` from its **covariant** candidates only, ignoring every
    /// contra-candidate. Mirrors the covariant branch of
    /// `fix_current_variables_with` (candidate cleanup + `resolve_from_candidates`)
    /// without the `resolve_covariant_against_contra` narrowing step.
    ///
    /// Used for the `#17282` Round-1-fix snapshot: when an unannotated
    /// (context-sensitive) callback parameter has contributed a contra-candidate
    /// that narrowed the fix away from the direct-argument inference, this
    /// recovers the pristine direct-argument value so the restore targets it.
    /// Returns `None` when the variable has no usable covariant candidate.
    pub fn probe_covariant_only(&mut self, var: InferenceVar) -> Option<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        if info.candidates.is_empty() {
            return None;
        }
        let is_const = self.is_var_const(root);
        let dc = self.declared_constraints.get(&root).copied();
        let dc_preserves_literals = self.literal_preserving_declared_constraints.contains(&root);
        let mut candidates = self.discard_self_referential_candidates(root, &info.candidates);
        if !info.upper_bounds.is_empty() {
            let has_informative_upper_bound = info
                .upper_bounds
                .iter()
                .any(|&upper| !upper.is_any_unknown_or_error());
            let has_concrete_candidate = candidates
                .iter()
                .any(|c| !c.type_id.is_any_unknown_or_error());
            candidates.retain(|candidate| match candidate.type_id {
                TypeId::UNKNOWN | TypeId::ERROR => false,
                TypeId::ANY => !has_informative_upper_bound || !has_concrete_candidate,
                _ => true,
            });
        }
        if candidates.is_empty() {
            return None;
        }
        // Keep only the best-priority candidates, matching tsc clearing
        // lower-priority candidates once a better one arrives. This drops the
        // `ReturnType`-priority Round-2 callback-body candidates (e.g. the `B`
        // from `u2 => u2.b`) so the Round-1 snapshot is the direct-argument
        // inference (`A`), not the `A | B` union.
        let candidates = self.filter_candidates_by_priority(&candidates);
        if candidates.is_empty() {
            return None;
        }
        let skip_literal_widening = self.top_level_in_return_type_unfixed.contains(&root);
        let spread_rest_mode = self.spread_rest_var_modes.get(&root).copied();
        Some(self.resolve_from_candidates(
            &candidates,
            is_const,
            &info.upper_bounds,
            dc,
            dc_preserves_literals,
            skip_literal_widening,
            self.root_preserves_return_position_literals(root),
            spread_rest_mode,
        ))
    }

    /// Get the current best substitution for all type parameters.
    ///
    /// This returns a `TypeSubstitution` mapping each type parameter to its
    /// current best type (either resolved or the best candidate so far).
    /// Used in Round 2 to provide contextual types to lambda arguments.
    pub fn get_current_substitution(&mut self) -> TypeSubstitution {
        let mut subst = TypeSubstitution::new();
        let type_params: Vec<_> = self.type_params.clone();

        for (name, var, _) in &type_params {
            let ty = match self.probe(*var) {
                Some(resolved) => {
                    tracing::trace!(
                        ?name,
                        ?var,
                        ?resolved,
                        "get_current_substitution: already resolved"
                    );
                    resolved
                }
                None => {
                    // Not resolved yet, try to get best candidate
                    let root = self.table.find(*var);
                    let info = self.table.probe_value(root);
                    tracing::trace!(
                        ?name, ?var,
                        candidates_count = info.candidates.len(),
                        contra_candidates_count = info.contra_candidates.len(),
                        upper_bounds_count = info.upper_bounds.len(),
                        upper_bounds = ?info.upper_bounds,
                        "get_current_substitution: not resolved"
                    );

                    let candidates =
                        self.discard_self_referential_candidates(root, &info.candidates);
                    let contra_candidates =
                        self.discard_self_referential_candidates(root, &info.contra_candidates);

                    if !candidates.is_empty() {
                        let is_const = self.is_var_const(root);
                        let dc = self.declared_constraints.get(&root).copied();
                        let dc_preserves_literals =
                            self.literal_preserving_declared_constraints.contains(&root);
                        let skip_literal_widening =
                            self.top_level_in_return_type_unfixed.contains(&root);
                        let spread_rest_mode = self.spread_rest_var_modes.get(&root).copied();
                        let covariant_result = self.resolve_from_candidates(
                            &candidates,
                            is_const,
                            &info.upper_bounds,
                            dc,
                            dc_preserves_literals,
                            skip_literal_widening,
                            self.root_preserves_return_position_literals(root),
                            spread_rest_mode,
                        );
                        if !contra_candidates.is_empty() {
                            let covariant_is_uninformative = matches!(
                                covariant_result,
                                TypeId::NEVER | TypeId::UNKNOWN | TypeId::ANY
                            );
                            let covariant_ok = !covariant_is_uninformative
                                && contra_candidates
                                    .iter()
                                    .any(|c| self.is_subtype(covariant_result, c.type_id));
                            if covariant_ok {
                                covariant_result
                            } else {
                                self.resolve_from_contra_candidates(&contra_candidates)
                            }
                        } else {
                            covariant_result
                        }
                    } else if !contra_candidates.is_empty() {
                        self.resolve_from_contra_candidates(&contra_candidates)
                    } else if !info.upper_bounds.is_empty() {
                        // No candidates yet, but we have a constraint (upper bound).
                        // Use the constraint as contextual fallback so that mapped types
                        // like `{ [K in keyof P]: P[K] }` resolve using the constraint
                        // type. This matches tsc's behavior for contextual typing of
                        // generic call arguments when all arguments are context-sensitive.
                        if info.upper_bounds.len() == 1 {
                            info.upper_bounds[0]
                        } else {
                            self.interner.intersection(info.upper_bounds.to_vec())
                        }
                    } else {
                        // No info yet, use unknown as placeholder
                        TypeId::UNKNOWN
                    }
                }
            };

            subst.insert(*name, ty);
        }

        subst
    }
}
