use crate::query_boundaries::checkers::promise as promise_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

thread_local! {
    /// `TypeId`s whose `Awaited<…>` assignability normalization is in flight on
    /// this thread. A re-entered `TypeId` returns opaque (its own input),
    /// preserving the original cycle behavior and guaranteeing the memo never
    /// records a result derived from an in-flight (not-yet-fixpointed) walk.
    ///
    /// Keys are interner-instance-local, so the set must be empty between
    /// compilations; the RAII [`AwaitedEvalVisitGuard`] removes its entry on
    /// every exit (normal, clamp-bail, or panic unwind), and
    /// `clear_all_thread_local_state` resets it at row boundaries as a backstop.
    static AWAITED_EVAL_VISITING: std::cell::RefCell<FxHashSet<TypeId>> =
        std::cell::RefCell::new(FxHashSet::default());

    /// Monotonic counter bumped every time the `depth > 8` clamp fires. A
    /// memoized normalization result is only recorded when this counter is
    /// unchanged across the call's subtree — i.e. no clamp degraded any nested
    /// result. Mirrors how `evaluate_type_for_assignability` refuses to memoize
    /// depth-clamped/fuel-exhausted evaluations.
    static AWAITED_EVAL_CLAMP_EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Reset the `Awaited<…>` assignability-normalization thread-locals.
///
/// Called from `clear_all_thread_local_state` at compilation/row boundaries:
/// the visiting set keys on arena-local `TypeId`s that are reused across
/// compilations, so a leaked entry (e.g. a mid-walk bail not caught by the RAII
/// guard) would make a fresh `TypeId` read as already-visiting and return
/// unevaluated. The clamp epoch is monotonic and only compared for equality
/// within a single top-level walk, so resetting it is optional, but it is
/// zeroed here for total isolation.
pub(crate) fn reset_awaited_eval_thread_local_state() {
    AWAITED_EVAL_VISITING.with(|visiting| visiting.borrow_mut().clear());
    AWAITED_EVAL_CLAMP_EPOCH.with(|epoch| epoch.set(0));
}

/// RAII membership guard for the `Awaited<…>` assignability-normalization walk.
///
/// [`enter`](Self::enter) returns `None` when `type_id` is already in flight on
/// this thread; the caller returns the type opaque. Otherwise it records
/// membership and clears it on drop, restoring the set even if the walk unwinds.
#[must_use]
struct AwaitedEvalVisitGuard(TypeId);

impl AwaitedEvalVisitGuard {
    fn enter(type_id: TypeId) -> Option<Self> {
        AWAITED_EVAL_VISITING.with(|visiting| {
            if visiting.borrow_mut().insert(type_id) {
                Some(Self(type_id))
            } else {
                None
            }
        })
    }
}

impl Drop for AwaitedEvalVisitGuard {
    fn drop(&mut self) {
        AWAITED_EVAL_VISITING.with(|visiting| {
            visiting.borrow_mut().remove(&self.0);
        });
    }
}

#[inline]
fn awaited_eval_clamp_epoch() -> u64 {
    AWAITED_EVAL_CLAMP_EPOCH.with(std::cell::Cell::get)
}

#[inline]
fn bump_awaited_eval_clamp_epoch() {
    AWAITED_EVAL_CLAMP_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
}

/// Dirty the `Awaited<…>` normalization thread-locals the way a mid-walk
/// bail (stack-overflow breaker, fuel exhaustion, or a caught panic) would.
#[cfg(test)]
pub(crate) fn dirty_awaited_eval_thread_local_state_for_test() {
    AWAITED_EVAL_VISITING.with(|visiting| {
        visiting.borrow_mut().insert(TypeId(123));
    });
    bump_awaited_eval_clamp_epoch();
}

/// Whether the `Awaited<…>` normalization thread-locals are at their reset
/// state (empty visiting set, zero clamp epoch).
#[cfg(test)]
pub(crate) fn awaited_eval_thread_local_state_clear_for_test() -> bool {
    AWAITED_EVAL_VISITING.with(|visiting| visiting.borrow().is_empty())
        && awaited_eval_clamp_epoch() == 0
}

impl<'a> CheckerState<'a> {
    pub(super) fn evaluate_awaited_object_properties_for_assignability(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        promise_query::awaited_assignability_object_with_mapped_slots(
            self.ctx.types,
            type_id,
            |slot| self.evaluate_awaited_application_for_assignability_inner(slot, depth + 1),
        )
    }
    pub(crate) fn evaluate_awaited_application_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        self.evaluate_awaited_application_for_assignability_inner(type_id, 0)
    }

    /// Memoizing dispatcher for the recursive `Awaited<…>` assignability
    /// normalization walk.
    ///
    /// The walk is a pure function of `type_id` for a fixed session stamp, so a
    /// stamp-guarded per-`TypeId` result memo collapses the combinatorial
    /// re-evaluation of nested `Awaited<…>` sub-applications that are reachable
    /// through many union/tuple/object-property parents (issue #13040). The
    /// `depth > 8` clamp and the per-thread cycle guard are preserved exactly:
    ///
    /// - The cycle guard ([`AwaitedEvalVisitGuard`]) precedes the memo lookup
    ///   so an in-flight `type_id` returns opaque rather than serving a result
    ///   from a now-superseded outer walk, mirroring
    ///   `evaluate_type_for_assignability`.
    /// - Only **clamp-clean** results are recorded: a `depth > 8` bail anywhere
    ///   in the subtree (tracked by the clamp epoch) marks the result as a
    ///   degraded form a shallower re-evaluation must improve on, so it is never
    ///   cached — identical to the depth/fuel gate in
    ///   `evaluate_type_for_assignability`.
    pub(super) fn evaluate_awaited_application_for_assignability_inner(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> TypeId {
        if depth > 8 {
            bump_awaited_eval_clamp_epoch();
            return type_id;
        }

        // Intrinsics (`number`, `string`, `boolean`, `any`, `never`, …) are
        // normalization fixpoints that can never recur, so the memo would only
        // ever store `type_id -> type_id`. Skip the cycle-guard/stamp/memo
        // bookkeeping for them — they are the dominant leaf of the walk, and
        // paying a thread-local set access plus a stamp recompute per leaf is
        // pure overhead. Mirrors the `is_intrinsic` fast path in
        // `evaluate_type_for_assignability`.
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Re-entrant cycle: return the type opaque. Taking membership for the
        // duration of the walk also means the memo can never observe a result
        // derived from an in-flight (not-yet-fixpointed) evaluation of the same
        // type. The guard removes the entry on every exit (normal, clamp-bail,
        // or panic unwind).
        let Some(_visit_guard) = AwaitedEvalVisitGuard::enter(type_id) else {
            return type_id;
        };

        if let Some(stamp) = self.assignability_eval_memo_stamp()
            && let Some(memoized) = self
                .ctx
                .type_reference_validation_caches
                .awaited_assignability_eval_memo
                .get(stamp, type_id)
        {
            return memoized;
        }

        let epoch_before = awaited_eval_clamp_epoch();
        let result = self.evaluate_awaited_application_for_assignability_body(type_id, depth);

        // Record only clamp-clean completions that actually normalized the type.
        //
        // - Identity results (`result == type_id`) are skipped: caching a
        //   `type_id -> type_id` entry never saves work on a later hit (the body
        //   would re-derive the same input) yet still pays an insert, and under
        //   a churning session stamp (each assignment site grows the type
        //   environments, rolling the memo) those inserts thrash the map
        //   (allocate/rehash per cleared generation). The combinatorial blowup
        //   this memo targets is the *rewritten* `Awaited<…>` sub-applications
        //   reachable through many parents, which are exactly the non-identity
        //   results, so skipping identities preserves the win while removing the
        //   overhead on awaited-free assignability paths.
        // - A `depth > 8` bail anywhere in the subtree (epoch moved) produced a
        //   degraded form a shallower re-evaluation must improve on, so it is
        //   never recorded.
        //
        // The stamp is read after the walk on purpose — the walk grows the type
        // environments and the result is valid for that post-walk state.
        if result != type_id
            && awaited_eval_clamp_epoch() == epoch_before
            && !self.ctx.depth_exceeded.get()
            && let Some(stamp) = self.assignability_eval_memo_stamp()
        {
            self.ctx
                .type_reference_validation_caches
                .awaited_assignability_eval_memo
                .insert(stamp, type_id, result);
        }

        result
    }

    fn evaluate_awaited_application_for_assignability_body(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> TypeId {
        if self.awaited_application_arg(type_id).is_none() {
            if let Some(evaluated) = promise_query::awaited_assignability_array_with_mapped_element(
                self.ctx.types,
                type_id,
                |elem| self.evaluate_awaited_application_for_assignability_inner(elem, depth + 1),
            ) {
                return evaluated;
            }
            let raw_awaited_distribution =
                promise_query::awaited_assignability_union_has_raw_awaited_distribution(
                    self.ctx.types,
                    type_id,
                    |ty| self.evaluate_type_for_assignability(ty),
                );
            if let Some(evaluated) =
                promise_query::awaited_assignability_union_with_mapped_members_if_changed(
                    self.ctx.types,
                    type_id,
                    |member| {
                        let mut evaluated = self
                            .evaluate_awaited_application_for_assignability_inner(
                                member,
                                depth + 1,
                            );
                        if raw_awaited_distribution
                            && let Some(awaited) = self
                                .unwrap_promise_type(evaluated)
                                .or_else(|| self.extract_awaited_type_from_thenable(evaluated))
                        {
                            evaluated = self.evaluate_awaited_application_for_assignability_inner(
                                awaited,
                                depth + 1,
                            );
                        }
                        evaluated
                    },
                )
            {
                return evaluated;
            }
            if let Some(evaluated) = promise_query::awaited_assignability_tuple_with_mapped_elements(
                self.ctx.types,
                type_id,
                |element| {
                    self.evaluate_awaited_application_for_assignability_inner(element, depth + 1)
                },
            ) {
                return evaluated;
            }
            if let Some(evaluated) =
                promise_query::awaited_assignability_application_with_mapped_args(
                    self.ctx.types,
                    type_id,
                    |arg| self.evaluate_awaited_application_for_assignability_inner(arg, depth + 1),
                )
            {
                return evaluated;
            }
            if let Some(evaluated) =
                self.evaluate_awaited_object_properties_for_assignability(type_id, depth)
            {
                return evaluated;
            }
            if let Some(evaluated) =
                self.evaluate_raw_awaited_conditional_for_assignability(type_id, depth)
            {
                return evaluated;
            }
            return type_id;
        }

        if self.awaited_application_arg_from_type(type_id).is_some() {
            let evaluated = self.evaluate_application_type(type_id);
            if evaluated != type_id {
                return self
                    .evaluate_awaited_application_for_assignability_inner(evaluated, depth + 1);
            }
        }

        let Some(arg) = self.awaited_application_arg(type_id) else {
            return type_id;
        };
        let arg = self.evaluate_type_for_assignability(arg);

        if let Some(evaluated) = promise_query::awaited_assignability_union_with_mapped_members(
            self.ctx.types,
            arg,
            |member| {
                if let Some(awaited) = self
                    .unwrap_promise_type(member)
                    .or_else(|| self.extract_awaited_type_from_thenable(member))
                {
                    self.evaluate_awaited_application_for_assignability_inner(awaited, depth + 1)
                } else {
                    member
                }
            },
        ) {
            return evaluated;
        }

        if let Some(awaited) = self
            .unwrap_promise_type(arg)
            .or_else(|| self.extract_awaited_type_from_thenable(arg))
        {
            return self.evaluate_awaited_application_for_assignability_inner(awaited, depth + 1);
        }

        // Awaited<T> is transparent for non-thenables. If the conditional
        // evaluator preserved the raw alias application, keep assignability in
        // step with tsc's getAwaitedType without incorrectly treating
        // Awaited<Promise<T>> as Promise<T>.
        arg
    }

    fn evaluate_raw_awaited_conditional_for_assignability(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        // Awaited<T> expands to `T extends thenable ? ... : T`. After
        // distribution over a union, assignability can see the raw conditional
        // branches instead of the `Awaited<T>` application. Only fold that
        // canonical false-branch shape; other conditional aliases must stay
        // deferred.
        let raw = promise_query::raw_awaited_conditional_for_assignability(
            self.ctx.types,
            type_id,
            |ty| self.evaluate_type_for_assignability(ty),
        )?;

        let check_type = self.evaluate_type_for_assignability(raw.check_type);
        if let Some(awaited) = self
            .unwrap_promise_type(check_type)
            .or_else(|| self.extract_awaited_type_from_thenable(check_type))
        {
            return Some(
                self.evaluate_awaited_application_for_assignability_inner(awaited, depth + 1),
            );
        }

        if !promise_query::awaited_assignability_type_has_then_property(self.ctx.types, check_type)
        {
            return Some(
                self.evaluate_awaited_application_for_assignability_inner(
                    raw.false_type,
                    depth + 1,
                ),
            );
        }

        None
    }
}

#[cfg(test)]
mod awaited_eval_guard_tests {
    use super::*;

    #[test]
    fn visit_guard_blocks_reentry_and_restores_on_drop() {
        reset_awaited_eval_thread_local_state();
        let t = TypeId(7);
        let outer = AwaitedEvalVisitGuard::enter(t).expect("first entry succeeds");
        assert!(
            AwaitedEvalVisitGuard::enter(t).is_none(),
            "re-entry while in flight must be blocked"
        );
        // A different type is independent.
        let other = AwaitedEvalVisitGuard::enter(TypeId(8)).expect("distinct type enters");
        drop(other);
        drop(outer);
        assert!(
            AwaitedEvalVisitGuard::enter(t).is_some(),
            "membership must be cleared on drop"
        );
        reset_awaited_eval_thread_local_state();
    }

    #[test]
    fn clamp_epoch_bumps_and_resets() {
        reset_awaited_eval_thread_local_state();
        let before = awaited_eval_clamp_epoch();
        bump_awaited_eval_clamp_epoch();
        assert_ne!(
            awaited_eval_clamp_epoch(),
            before,
            "clamp epoch must advance so a subtree clamp is observable"
        );
        reset_awaited_eval_thread_local_state();
        assert_eq!(
            awaited_eval_clamp_epoch(),
            0,
            "reset zeroes the clamp epoch"
        );
    }
}
