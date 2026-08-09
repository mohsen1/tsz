//! Use-site `TS2589` ("excessively deep and possibly infinite") convergence
//! probe for computed-recursive type aliases, split out of `lazy.rs` to stay
//! under the arch-size cap.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Evaluate a type for TS2589 detection at type alias definition sites.
    ///
    /// Like `evaluate_type_with_env_uncached` but uses an evaluator that flags
    /// `depth_exceeded` when cycle detection fires on an Application type.
    /// This catches self-referential conditional types that produce the same
    /// Application TypeId on each expansion.
    ///
    /// Returns true if depth was exceeded (TS2589 should be emitted).
    pub(crate) fn evaluate_type_for_ts2589_check(
        &mut self,
        type_id: TypeId,
        alias_def_id: tsz_solver::def::DefId,
    ) -> bool {
        let env = self.ctx.type_env.borrow();
        // First try: evaluate with flag that detects Application cycles
        let eval_result =
            crate::query_boundaries::state::type_environment::evaluate_type_for_ts2589(
                self.ctx.types,
                &*env,
                type_id,
            );
        if eval_result.depth_exceeded {
            return true;
        }

        // Second check: a concrete self-application of the alias can survive the
        // first evaluation because the evaluator leaves a recursive reference in a
        // non-tail position (a function return or object/mapped property, e.g. the
        // `Curry<T, R>` inside `(h: H) => Curry<T, R>`) deferred — so a residual
        // `Application(alias, args)` is the norm, not proof of infinite expansion.
        // A residual whose argument weight stays the same size or shrinks relative
        // to the input is *not* evidence of divergence:
        //   * it may shrink along a dimension the coarse metric scores flat — a
        //     numeric depth counter (`N` -> `Exclude<N, 0>`) or a structural descent
        //     into `T[K]` — and so terminate at a base case (e.g. `DeepObject<T, N>`);
        //   * or it may tie a finite knot the way `tsc` defers recursive object and
        //     mapped-property references (`{ [K in keyof T]: Rec<T[K]> }`), which is
        //     accepted, not flagged.
        // A residual whose weight is strictly larger is not immediately divergent
        // either: an accumulator-style alias (`Nest<N> = N["length"] extends K ?
        // Base : { a: Nest<[unknown, ...N]> }`) legitimately grows a tuple by one
        // element on every step yet still terminates once the literal-number
        // condition trips, the same way `tsc` reaches it via real, concrete
        // `instantiationDepth`-bounded evaluation rather than predicting divergence
        // from one step's growth. `residual_application_diverges` re-drives such a
        // residual through bounded, real expansion instead of guessing from a
        // single step: it converges (false) the moment expansion no longer leaves a
        // concrete self-application of `alias_def_id` behind, and only concludes
        // divergence when the residual is still an unresolved self-application
        // after `TS2589_RESIDUAL_TERMINATION_BOUND` further rounds — mirroring
        // `tsc`'s own real depth cap rather than a structural-size heuristic.
        // The same-identity stall (`Foo<unknown>` -> `Foo<unknown>`) that this check
        // once caught here is already detected earlier as an Application cycle
        // (`eval_result.depth_exceeded`), and any residual the weight metric cannot
        // see is still bounded by the per-`DefId` instantiation-depth limit, so
        // requiring strict growth here only removes false positives. When there is
        // no input application to compare against (the definition-site pass evaluates
        // the conditional body directly), any surviving concrete self-reference stays
        // divergent, preserving definition-site TS2589.
        let result = eval_result.result;
        if result != type_id && result != TypeId::ERROR {
            let db = self.ctx.types.as_type_database();
            let residuals = crate::query_boundaries::state::type_environment::collect_concrete_applications_with_def(
                db,
                result,
                alias_def_id,
            );
            let input_weight =
                crate::query_boundaries::state::type_environment::self_application_arg_weight(
                    db,
                    &*env,
                    type_id,
                    alias_def_id,
                );
            drop(env);
            match input_weight {
                None => return !residuals.is_empty(),
                Some(input_weight) => {
                    let diverges = residuals.iter().any(|&residual| {
                        let env = self.ctx.type_env.borrow();
                        let residual_weight = crate::query_boundaries::state::type_environment::self_application_arg_weight(
                            self.ctx.types.as_type_database(),
                            &*env,
                            residual,
                            alias_def_id,
                        );
                        drop(env);
                        match residual_weight {
                            None => true,
                            Some(w) if w <= input_weight => false,
                            Some(_) => self.residual_application_diverges(
                                residual,
                                alias_def_id,
                                Self::TS2589_RESIDUAL_TERMINATION_BOUND,
                            ),
                        }
                    });
                    if diverges {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Rounds of further real expansion `residual_application_diverges` will
    /// drive a growing non-tail residual through before concluding it diverges.
    /// Mirrors `tsz_solver::limits::MAX_DEF_DEPTH` (the checker cannot see that
    /// `pub(crate)` solver constant directly), tsz's own analogue of `tsc`'s real
    /// `instantiationDepth` cap: an accumulator alias that needs more real steps
    /// than `tsc` itself allows before resolving would hit `TS2589` on `tsc` too,
    /// so bailing at the same bound preserves parity in both directions.
    const TS2589_RESIDUAL_TERMINATION_BOUND: u32 = 100;

    /// Re-drive a non-tail self-application residual through bounded, real
    /// expansion to tell a terminating accumulator recursion (grows every step,
    /// still resolves once a literal-number condition trips) from genuine
    /// divergence, instead of guessing from a single step's structural weight.
    ///
    /// Converges (`false`) as soon as an expansion round leaves no concrete
    /// self-application of `alias_def_id` behind — the recursion resolved to a
    /// concrete type within the bound, exactly the outcome `tsc`'s own concrete
    /// evaluation would reach. Diverges (`true`) if the per-step evaluator's own
    /// cycle/fuel detection fires (`depth_exceeded`, catching exponential blowups
    /// far sooner than the round budget) or if a self-application of the same
    /// alias is still present after `steps_remaining` rounds.
    fn residual_application_diverges(
        &mut self,
        residual: TypeId,
        alias_def_id: tsz_solver::def::DefId,
        steps_remaining: u32,
    ) -> bool {
        if steps_remaining == 0 {
            return true;
        }
        let eval_result = {
            let env = self.ctx.type_env.borrow();
            crate::query_boundaries::state::type_environment::evaluate_type_for_ts2589(
                self.ctx.types,
                &*env,
                residual,
            )
        };
        if eval_result.depth_exceeded {
            return true;
        }
        let db = self.ctx.types.as_type_database();
        let next_residuals = crate::query_boundaries::state::type_environment::collect_concrete_applications_with_def(
            db,
            eval_result.result,
            alias_def_id,
        );
        next_residuals.iter().any(|&next| {
            self.residual_application_diverges(next, alias_def_id, steps_remaining - 1)
        })
    }
}
