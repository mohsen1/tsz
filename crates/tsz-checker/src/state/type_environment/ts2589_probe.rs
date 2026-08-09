//! Use-site TS2589 ("excessively deep") divergence probe for computed
//! recursive type aliases.
//!
//! Split out of `lazy.rs` (arch-size boundary) since this is a cohesive,
//! self-contained convergence check independent of the rest of that file's
//! lazy-resolution machinery.

use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
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
        // It is divergence evidence only when it makes no *progress*: at a use site
        // (the checked type is itself a concrete application of the alias) compare
        // the structural argument weight of the input against each residual. A
        // residual that stays the same size or shrinks is *not* proof of
        // divergence:
        //   * it may shrink along a dimension the coarse metric scores flat — a
        //     numeric depth counter (`N` -> `Exclude<N, 0>`) or a structural descent
        //     into `T[K]` — and so terminate at a base case (e.g. `DeepObject<T, N>`);
        //   * or it may tie a finite knot the way `tsc` defers recursive object and
        //     mapped-property references (`{ [K in keyof T]: Rec<T[K]> }`), which is
        //     accepted, not flagged.
        // A residual whose weight is strictly larger than the step before it is
        // *not* immediate proof of divergence either: the evaluator only expands
        // one non-tail property lookup per round (e.g. `Nest<N> = N["length"]
        // extends K ? Base : { a: Nest<[unknown, ...N]> }` grows its tuple by one
        // element per round while genuinely converging toward `K`), so one round of
        // growth is exactly what a legitimate bounded recursion looks like on its
        // way to a base case tsc would still reach. `residual_self_application_diverges`
        // keeps following growth for up to `MAX_INSTANTIATION_DEPTH` real rounds —
        // matching tsc's own `instantiationDepth` bound — and only calls it
        // divergent when growth is sustained all the way to that bound. The
        // same-identity stall (`Foo<unknown>` -> `Foo<unknown>`) that a single-round
        // check once caught here is already detected earlier as an Application
        // cycle (`eval_result.depth_exceeded`), and any residual the weight metric
        // cannot see is still bounded by the per-`DefId` instantiation-depth limit,
        // so requiring sustained growth here only removes false positives. When
        // there is no input application to compare against (the definition-site
        // pass evaluates the conditional body directly), any surviving concrete
        // self-reference stays divergent, preserving definition-site TS2589.
        let result = eval_result.result;
        if result != type_id && result != TypeId::ERROR {
            let db = self.ctx.types.as_type_database();
            let residuals = crate::query_boundaries::state::type_environment::collect_concrete_applications_with_def(
                db,
                result,
                alias_def_id,
            );
            match crate::query_boundaries::state::type_environment::self_application_arg_weight(
                db,
                &*env,
                type_id,
                alias_def_id,
            ) {
                None => return !residuals.is_empty(),
                Some(input_weight) => {
                    let diverges = residuals.iter().any(|&residual| {
                        match crate::query_boundaries::state::type_environment::self_application_arg_weight(
                            db,
                            &*env,
                            residual,
                            alias_def_id,
                        ) {
                            None => true,
                            Some(residual_weight) if residual_weight > input_weight => {
                                Self::residual_self_application_diverges(
                                    db,
                                    &*env,
                                    residual,
                                    alias_def_id,
                                    residual_weight,
                                    1,
                                )
                            }
                            Some(_) => false,
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

    /// Bounded real-depth probe for a residual self-application of a computed
    /// recursive alias that grew on the previous round of
    /// `evaluate_type_for_ts2589_check`.
    ///
    /// One round of structural growth is not proof of divergence (see the
    /// caller's doc comment) — it keeps re-evaluating the residual for up to
    /// `MAX_INSTANTIATION_DEPTH` further real rounds, matching tsc's own
    /// `instantiationDepth` bound, and treats the recursion as divergent only
    /// when growth is sustained every round all the way to that bound. Growth
    /// that stops (a round's weight no longer exceeds the round before it) or
    /// that terminates (no residual application survives a round) proves the
    /// recursion is making progress toward a base case tsc would also reach.
    fn residual_self_application_diverges<R: tsz_solver::relations::subtype::TypeResolver>(
        db: &dyn tsz_solver::construction::TypeDatabase,
        resolver: &R,
        residual: TypeId,
        alias_def_id: tsz_solver::def::DefId,
        prev_weight: u64,
        depth: u32,
    ) -> bool {
        if depth >= MAX_INSTANTIATION_DEPTH {
            return true;
        }
        let eval_result =
            crate::query_boundaries::state::type_environment::evaluate_type_for_ts2589(
                db, resolver, residual,
            );
        if eval_result.depth_exceeded {
            return true;
        }
        if eval_result.result == residual || eval_result.result == TypeId::ERROR {
            return false;
        }
        let next_residuals = crate::query_boundaries::state::type_environment::collect_concrete_applications_with_def(
            db,
            eval_result.result,
            alias_def_id,
        );
        next_residuals.iter().any(|&next| {
            match crate::query_boundaries::state::type_environment::self_application_arg_weight(
                db,
                resolver,
                next,
                alias_def_id,
            ) {
                None => true,
                Some(weight) if weight > prev_weight => Self::residual_self_application_diverges(
                    db,
                    resolver,
                    next,
                    alias_def_id,
                    weight,
                    depth + 1,
                ),
                Some(_) => false,
            }
        })
    }
}
