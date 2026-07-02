//! Inference over deferred (stuck) conditional-type pairs.
//!
//! When a generic HKT-style encoding keeps both the argument and the
//! parameter as unevaluable conditionals (e.g. `URI extends URIS ?
//! URItoKind<A>[URI] : any` with an abstract `URI`), `tsc`'s
//! `inferToConditionalType` still registers candidates by walking the two
//! deferred forms pairwise instead of reducing them. These helpers implement
//! that structural rule for the constraint walker.

use crate::inference::infer::InferenceContext;
use crate::operations::constraints::walker_guard_state::with_placeholder_visited;
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{InferencePriority, TypeData, TypeId};
use rustc_hash::FxHashMap;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Constrain a source conditional against a target conditional.
    ///
    /// If either side still evaluates, restart with the evaluated forms.
    /// Otherwise both conditionals are stuck (deferred on an abstract check
    /// type): mirror tsc's `inferToConditionalType` source-conditional arm
    /// and infer pairwise over the corresponding positions, so candidates
    /// register from the deferred forms without reduction.
    pub(super) fn constrain_conditional_pair(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        s_cond_id: crate::types::ConditionalTypeId,
        target: TypeId,
        t_cond_id: crate::types::ConditionalTypeId,
        priority: InferencePriority,
    ) {
        let s_cond = self.interner.get_conditional(s_cond_id);
        let t_cond = self.interner.get_conditional(t_cond_id);
        let s_eval = self.interner.evaluate_conditional(&s_cond);
        let t_eval = self.interner.evaluate_conditional(&t_cond);
        if s_eval != source || t_eval != target {
            self.constrain_types(ctx, var_map, s_eval, t_eval, priority);
            return;
        }
        self.constrain_types(ctx, var_map, s_cond.check_type, t_cond.check_type, priority);
        self.constrain_types(
            ctx,
            var_map,
            s_cond.extends_type,
            t_cond.extends_type,
            priority,
        );
        self.constrain_types(ctx, var_map, s_cond.true_type, t_cond.true_type, priority);
        self.constrain_types(ctx, var_map, s_cond.false_type, t_cond.false_type, priority);
    }

    /// A stuck conditional source met a non-conditional target: the target
    /// may be an unexpanded alias application whose body is the matching
    /// deferred conditional (`Kind<F, A>`). Reduce the target one step and
    /// retry so the pairwise conditional arm can fire.
    pub(super) fn constrain_stuck_conditional_source(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: InferencePriority,
    ) {
        let evaluated_target = self.checker.evaluate_type(target);
        if evaluated_target != target
            && matches!(
                self.interner.lookup(evaluated_target),
                Some(TypeData::Conditional(_))
            )
        {
            self.constrain_types(ctx, var_map, source, evaluated_target, priority);
        }
    }

    /// Infer against a target conditional that cannot evaluate yet.
    ///
    /// tsc's `inferToConditionalType` checks the source side first: a source
    /// that reduces to a deferred conditional infers pairwise against the
    /// target conditional. Otherwise, infer against both branch types —
    /// direct naked type-parameter branches are fallback evidence; structured
    /// branches should win when they can infer more specific candidates.
    pub(super) fn constrain_to_stuck_conditional_target(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        cond: &crate::types::ConditionalType,
        priority: InferencePriority,
    ) {
        if let Some(evaluated_source) = self.source_as_deferred_conditional(source) {
            self.constrain_types(ctx, var_map, evaluated_source, target, priority);
            return;
        }
        let contains_placeholder = with_placeholder_visited(|visited| {
            self.type_contains_placeholder(target, var_map, visited)
        });
        if !contains_placeholder {
            return;
        }
        if var_map.contains_key(&cond.check_type)
            && cond.true_type != TypeId::NEVER
            && cond.false_type != TypeId::NEVER
        {
            return;
        }
        let true_priority = if var_map.contains_key(&cond.true_type) {
            InferencePriority::LowPriority
        } else {
            priority
        };
        let false_priority = if var_map.contains_key(&cond.false_type) {
            InferencePriority::LowPriority
        } else {
            priority
        };
        self.constrain_types(ctx, var_map, source, cond.true_type, true_priority);
        self.constrain_types(ctx, var_map, source, cond.false_type, false_priority);
    }

    /// Return the deferred conditional that `source` is one evaluation step
    /// away from (an alias application like `Kind<F, A>` whose body cannot
    /// reduce further under an abstract check type), if any. Lazy sources
    /// need no arm here: the walker's top-level Lazy resolution already
    /// redispatches any resolvable Lazy before structural dispatch.
    fn source_as_deferred_conditional(&mut self, source: TypeId) -> Option<TypeId> {
        if !matches!(self.interner.lookup(source), Some(TypeData::Application(_))) {
            return None;
        }
        let evaluated = self.checker.evaluate_type(source);
        (evaluated != source
            && matches!(
                self.interner.lookup(evaluated),
                Some(TypeData::Conditional(_))
            ))
        .then_some(evaluated)
    }
}
