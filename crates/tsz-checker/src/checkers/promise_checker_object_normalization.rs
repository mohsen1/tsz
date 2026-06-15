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

impl<'a> CheckerState<'a> {
    pub(super) fn evaluate_awaited_object_properties_for_assignability(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        let shape_id = crate::query_boundaries::common::object_shape_id(self.ctx.types, type_id)?;
        let shape = self.ctx.types.object_shape(shape_id);
        let mut changed = false;
        let evaluated_properties: Vec<_> = shape
            .properties
            .iter()
            .map(|prop| {
                let evaluated_type = self
                    .evaluate_awaited_application_for_assignability_inner(prop.type_id, depth + 1);
                let evaluated_write = self.evaluate_awaited_application_for_assignability_inner(
                    prop.write_type,
                    depth + 1,
                );
                changed |= evaluated_type != prop.type_id || evaluated_write != prop.write_type;
                tsz_solver::PropertyInfo {
                    type_id: evaluated_type,
                    write_type: evaluated_write,
                    ..*prop
                }
            })
            .collect();
        let evaluated_string_index = shape.string_index.map(|mut index| {
            let evaluated = self
                .evaluate_awaited_application_for_assignability_inner(index.value_type, depth + 1);
            changed |= evaluated != index.value_type;
            index.value_type = evaluated;
            index
        });
        let evaluated_number_index = shape.number_index.map(|mut index| {
            let evaluated = self
                .evaluate_awaited_application_for_assignability_inner(index.value_type, depth + 1);
            changed |= evaluated != index.value_type;
            index.value_type = evaluated;
            index
        });

        changed.then(|| {
            self.ctx
                .types
                .factory()
                .object_with_index(tsz_solver::ObjectShape {
                    properties: evaluated_properties,
                    string_index: evaluated_string_index,
                    number_index: evaluated_number_index,
                    ..(*shape).clone()
                })
        })
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
            if let Some(elem) =
                crate::query_boundaries::common::array_element_type(self.ctx.types, type_id)
            {
                let evaluated_elem =
                    self.evaluate_awaited_application_for_assignability_inner(elem, depth + 1);
                if evaluated_elem != elem {
                    return self.ctx.types.factory().array(evaluated_elem);
                }
            }
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            {
                let raw_awaited_distribution = members
                    .iter()
                    .copied()
                    .any(|member| self.is_raw_awaited_conditional_for_assignability(member));
                let mut changed = false;
                let evaluated_members: Vec<_> = members
                    .into_iter()
                    .map(|member| {
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
                        changed |= evaluated != member;
                        evaluated
                    })
                    .collect();
                if changed {
                    return self.ctx.types.factory().union(evaluated_members);
                }
            }
            if let Some(elems) =
                crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
            {
                let mut changed = false;
                let evaluated_elems: Vec<_> = elems
                    .into_iter()
                    .map(|mut elem| {
                        let evaluated = self.evaluate_awaited_application_for_assignability_inner(
                            elem.type_id,
                            depth + 1,
                        );
                        changed |= evaluated != elem.type_id;
                        elem.type_id = evaluated;
                        elem
                    })
                    .collect();
                if changed {
                    return self.ctx.types.factory().tuple(evaluated_elems);
                }
            }
            if let Some((base, args)) =
                crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            {
                let mut changed = false;
                let evaluated_args: Vec<_> = args
                    .iter()
                    .copied()
                    .map(|arg| {
                        let evaluated = self
                            .evaluate_awaited_application_for_assignability_inner(arg, depth + 1);
                        changed |= evaluated != arg;
                        evaluated
                    })
                    .collect();
                if changed {
                    return self.ctx.types.factory().application(base, evaluated_args);
                }
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

        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, arg) {
            let awaited_members = members
                .into_iter()
                .map(|member| {
                    if let Some(awaited) = self
                        .unwrap_promise_type(member)
                        .or_else(|| self.extract_awaited_type_from_thenable(member))
                    {
                        self.evaluate_awaited_application_for_assignability_inner(
                            awaited,
                            depth + 1,
                        )
                    } else {
                        member
                    }
                })
                .collect();
            return self.ctx.types.factory().union(awaited_members);
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
        let cond_id =
            crate::query_boundaries::common::get_conditional_type_id(self.ctx.types, type_id)?;
        let cond = self.ctx.types.conditional_type(cond_id);
        // Awaited<T> expands to `T extends thenable ? ... : T`. After
        // distribution over a union, assignability can see the raw conditional
        // branches instead of the `Awaited<T>` application. Only fold that
        // canonical false-branch shape; other conditional aliases must stay
        // deferred.
        if !self.is_raw_awaited_conditional_for_assignability(type_id) {
            return None;
        }

        let check_type = self.evaluate_type_for_assignability(cond.check_type);
        if let Some(awaited) = self
            .unwrap_promise_type(check_type)
            .or_else(|| self.extract_awaited_type_from_thenable(check_type))
        {
            return Some(
                self.evaluate_awaited_application_for_assignability_inner(awaited, depth + 1),
            );
        }

        if !crate::query_boundaries::common::has_property_by_str(self.ctx.types, check_type, "then")
        {
            return Some(
                self.evaluate_awaited_application_for_assignability_inner(
                    cond.false_type,
                    depth + 1,
                ),
            );
        }

        None
    }

    fn is_raw_awaited_conditional_for_assignability(&mut self, type_id: TypeId) -> bool {
        let Some(cond_id) =
            crate::query_boundaries::common::get_conditional_type_id(self.ctx.types, type_id)
        else {
            return false;
        };
        let cond = self.ctx.types.conditional_type(cond_id);
        if cond.false_type != cond.check_type {
            return false;
        }

        let extends_type = self.evaluate_type_for_assignability(cond.extends_type);
        crate::query_boundaries::common::has_property_by_str(self.ctx.types, extends_type, "then")
    }
}
