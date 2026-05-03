//! Exact-type substitution used by distributive conditional evaluation.
//!
//! The key invariant: substitution must rewrite every occurrence of the
//! source type, even when the same hash-consed node is reachable through
//! multiple paths in the type tree. A simple visit-once `seen` set
//! conflates "currently being processed" with "already substituted",
//! which causes the second occurrence of a shared node to be returned
//! unchanged. We therefore memoize per-node substitutions and use a
//! self-mapping placeholder to handle true cycles.

use crate::relations::subtype::TypeResolver;
use crate::types::{ConditionalType, FunctionShape, TypeData, TypeId};
use rustc_hash::FxHashMap;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Substitute every occurrence of `from` with `to` inside `type_id`.
    ///
    /// `memo` maps each visited node to its substituted result. On entry we
    /// insert `type_id -> type_id` as a cycle guard; if a recursive call
    /// re-enters the same node before we finish, the cached self-mapping is
    /// returned (matching the previous `seen`-set behavior). Once processing
    /// completes we overwrite the placeholder with the real substituted
    /// result, so later non-reentrant visits to the same hash-consed node
    /// reuse the substituted value instead of seeing the original.
    pub(crate) fn substitute_exact_type(
        &mut self,
        type_id: TypeId,
        from: TypeId,
        to: TypeId,
        memo: &mut FxHashMap<TypeId, TypeId>,
    ) -> TypeId {
        if type_id == from {
            return to;
        }
        if type_id.is_intrinsic() {
            return type_id;
        }
        // Insert cycle guard. If the slot was already occupied (revisit), the
        // previous value is the substitution result we should return: either
        // the final substituted node (shared-node case) or the placeholder
        // pointing back at `type_id` (true cycle).
        if let Some(cached) = memo.insert(type_id, type_id) {
            return cached;
        }

        let result = match self.interner().lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                let app = self.interner().type_application(app_id);
                let base = self.substitute_exact_type(app.base, from, to, memo);
                let mut changed = base != app.base;
                let args: Vec<_> = app
                    .args
                    .iter()
                    .map(|&arg| {
                        let substituted = self.substitute_exact_type(arg, from, to, memo);
                        changed |= substituted != arg;
                        substituted
                    })
                    .collect();
                if changed {
                    self.interner().application(base, args)
                } else {
                    type_id
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner().function_shape(shape_id);
                let mut changed = false;
                let params = shape
                    .params
                    .iter()
                    .map(|param| {
                        let type_id = self.substitute_exact_type(param.type_id, from, to, memo);
                        changed |= type_id != param.type_id;
                        crate::types::ParamInfo { type_id, ..*param }
                    })
                    .collect();
                let this_type = shape.this_type.map(|this_type| {
                    let substituted = self.substitute_exact_type(this_type, from, to, memo);
                    changed |= substituted != this_type;
                    substituted
                });
                let return_type = self.substitute_exact_type(shape.return_type, from, to, memo);
                changed |= return_type != shape.return_type;
                let type_predicate = shape.type_predicate.map(|mut predicate| {
                    if let Some(predicate_type) = predicate.type_id {
                        let substituted =
                            self.substitute_exact_type(predicate_type, from, to, memo);
                        changed |= substituted != predicate_type;
                        predicate.type_id = Some(substituted);
                    }
                    predicate
                });
                if changed {
                    self.interner().function(FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    type_id
                }
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner().get_conditional(cond_id);
                let check_type = self.substitute_exact_type(cond.check_type, from, to, memo);
                let extends_type = self.substitute_exact_type(cond.extends_type, from, to, memo);
                let true_type = self.substitute_exact_type(cond.true_type, from, to, memo);
                let false_type = self.substitute_exact_type(cond.false_type, from, to, memo);
                if check_type != cond.check_type
                    || extends_type != cond.extends_type
                    || true_type != cond.true_type
                    || false_type != cond.false_type
                {
                    self.interner().conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                } else {
                    type_id
                }
            }
            _ => type_id,
        };

        // Overwrite the cycle-guard placeholder with the real result so later
        // visits to this shared hash-consed node return the substituted form.
        memo.insert(type_id, result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefId;
    use crate::intern::TypeInterner;
    use crate::types::TypeParamInfo;

    /// Regression: `substitute_exact_type` must substitute every occurrence
    /// of `from`, including hash-consed nodes that appear via multiple paths.
    /// Previously the visit-once `seen` set caused later occurrences of a
    /// shared node to be returned unchanged.
    #[test]
    fn test_substitute_exact_type_handles_shared_hash_consed_nodes() {
        let interner = TypeInterner::new();

        // Type parameter `T`.
        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });

        // Two named base types `Bar` and `Foo` (modeled as `Lazy(DefId)`).
        let bar = interner.lazy(DefId(101));
        let foo = interner.lazy(DefId(102));

        // Inner `Bar<T>` — the interner is hash-consed, so referencing this
        // structure twice yields the *same* TypeId.
        let bar_of_t = interner.application(bar, vec![t_param]);
        let bar_of_t_again = interner.application(bar, vec![t_param]);
        assert_eq!(
            bar_of_t, bar_of_t_again,
            "interner should return the same TypeId for structurally identical Application types"
        );

        // Outer `Foo<Bar<T>, Bar<T>>` — both args are the same shared node.
        let outer = interner.application(foo, vec![bar_of_t, bar_of_t]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(outer, t_param, TypeId::STRING, &mut memo);

        // Expected: `Foo<Bar<string>, Bar<string>>`.
        let expected_inner = interner.application(bar, vec![TypeId::STRING]);
        let expected = interner.application(foo, vec![expected_inner, expected_inner]);
        assert_eq!(
            result, expected,
            "shared hash-consed node should be substituted on every occurrence"
        );

        // Sanity: pre-fix output would have been `Foo<Bar<string>, Bar<T>>`.
        let buggy_outer = interner.application(foo, vec![expected_inner, bar_of_t]);
        assert_ne!(
            result, buggy_outer,
            "second occurrence of shared node was left unsubstituted (pre-fix bug)"
        );
    }
}
