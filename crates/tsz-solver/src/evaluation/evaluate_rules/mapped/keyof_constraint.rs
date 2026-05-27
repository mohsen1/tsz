//! `keyof` constraint reduction for mapped type iteration.

use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::{LiteralValue, TypeData, TypeId};

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a keyof or constraint type for mapped type iteration.
    ///
    /// Wrapped with `stacker::maybe_grow()` to handle deeply nested union/intersection
    /// constraint chains without overflowing the default thread stack.
    ///
    /// All intermediate types in the evaluation chain remain entered in the
    /// `keyof_constraint_guard` until the chain terminates. This ensures that
    /// a cycle like `Lazy(A) → Lazy(B) → Lazy(A)` is detected when `A` is
    /// re-entered while it is still in the guard's visited set. The depth cap
    /// (`TypeEvaluation` profile: depth 100) also limits the chain length.
    pub(super) fn evaluate_keyof_or_constraint(&mut self, constraint: TypeId) -> TypeId {
        let mut current = constraint;
        let mut entered: Vec<TypeId> = Vec::new();

        let result = loop {
            match self.keyof_constraint_guard.enter(current) {
                crate::recursion::RecursionResult::Entered => {
                    entered.push(current);
                }
                _ => break current,
            }

            let step = stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
                self.evaluate_keyof_or_constraint_inner(current)
            });

            if step != current
                && matches!(
                    self.interner().lookup(step),
                    Some(
                        TypeData::Union(_)
                            | TypeData::Intersection(_)
                            | TypeData::KeyOf(_)
                            | TypeData::Conditional(_)
                            | TypeData::Lazy(_)
                            | TypeData::Application(_)
                    )
                )
            {
                current = step;
                continue;
            }
            break step;
        };

        for &id in entered.iter().rev() {
            self.keyof_constraint_guard.leave(id);
        }
        result
    }

    fn evaluate_keyof_or_constraint_inner(&mut self, constraint: TypeId) -> TypeId {
        // PERF: Single lookup handles all cases instead of 4 separate DashMap lookups.
        let members = match self.interner().lookup(constraint) {
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner().get_conditional(cond_id);
                return self.evaluate_conditional(&cond);
            }
            Some(TypeData::Literal(LiteralValue::String(_))) => {
                return constraint;
            }
            Some(TypeData::KeyOf(operand)) => {
                return self.evaluate_keyof(operand);
            }
            Some(TypeData::Union(members)) => Some(members),
            _ => None,
        };

        // Union: recursively evaluate each member. This handles the distributed form
        // where `(keyof T & keyof U)` after T is inferred becomes
        // `Union(Intersection("x", keyof U), Intersection("y", keyof U))` due to
        // the interner's intersection-over-union distribution. Each Union member
        // (which may be an Intersection) gets recursively simplified.
        if let Some(members) = members {
            let member_list = self.interner().type_list(members);
            let mut evaluated_members = Vec::with_capacity(member_list.len());
            let mut any_changed = false;
            for &member in member_list.iter() {
                let evaluated = self.evaluate_keyof_or_constraint(member);
                if evaluated != member {
                    any_changed = true;
                }
                evaluated_members.push(evaluated);
            }
            if any_changed {
                return self.interner().union(evaluated_members);
            }
            return constraint;
        }

        // Intersection: evaluate each member to get its key set, then compute
        // their intersection. Handles both pre-distribution `keyof T & keyof U`
        // and post-distribution `"x" & keyof U` forms.
        if let Some(TypeData::Intersection(members)) = self.interner().lookup(constraint) {
            let member_list = self.interner().type_list(members);
            let mut key_sets = Vec::with_capacity(member_list.len());
            for &member in member_list.iter() {
                key_sets.push(self.evaluate_keyof_or_constraint(member));
            }
            if let Some(result) = self.intersect_keyof_sets(&key_sets) {
                return result;
            }
            // If intersection computation failed, fall through to general evaluation
        }

        // Evaluate the constraint to resolve type aliases (Lazy), Applications, etc.
        // For example, `type Keys = "a" | "b"; { [P in Keys]: T }` has a Lazy(DefId)
        // constraint that must be evaluated to get the concrete union `"a" | "b"`.
        self.evaluate(constraint)
    }
}
