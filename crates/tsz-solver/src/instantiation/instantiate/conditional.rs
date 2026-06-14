//! `Conditional`-type instantiation: the `TypeData::Conditional` arm of
//! `instantiate_key`, including distributive-conditional expansion over
//! substituted union and `boolean` check types.

use crate::types::{ConditionalType, ConditionalTypeId, TypeData, TypeId};

use super::{TypeInstantiator, instantiate_type, instantiate_type_preserving};

impl<'a> TypeInstantiator<'a> {
    /// Instantiate a conditional type: instantiate all parts.
    pub(super) fn instantiate_conditional(
        &mut self,
        type_id: TypeId,
        cond_id: &ConditionalTypeId,
    ) -> TypeId {
        let cond = self.interner.get_conditional(*cond_id);
        if cond.is_distributive
            && let Some(TypeData::TypeParameter(info)) = self.interner.lookup(cond.check_type)
            && !self.is_shadowed(info.name)
            && let Some(substituted) = self.substitution.get(info.name)
        {
            // When substituting with `never`, the result is `never`
            if substituted == crate::types::TypeId::NEVER {
                return substituted;
            }
            // For `any`, we need to let evaluation handle it properly
            // so it can distribute to both branches
            // TypeScript treats `boolean` as `true | false` for distributive conditionals
            if substituted == TypeId::BOOLEAN {
                let cond_type = self.interner.conditional(cond);
                let mut results = Vec::with_capacity(2);
                for &member in &[TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE] {
                    if self.depth_exceeded {
                        return TypeId::ERROR;
                    }
                    let mut member_subst = self.substitution.clone();
                    member_subst.insert(info.name, member);
                    let instantiated = if self.preserve_unsubstituted_type_params {
                        instantiate_type_preserving(self.interner, cond_type, &member_subst)
                    } else {
                        instantiate_type(self.interner, cond_type, &member_subst)
                    };
                    if instantiated == TypeId::ERROR {
                        self.depth_exceeded = true;
                        return TypeId::ERROR;
                    }
                    let evaluated =
                        crate::evaluation::evaluate::evaluate_type(self.interner, instantiated);
                    if evaluated == TypeId::ERROR {
                        self.depth_exceeded = true;
                        return TypeId::ERROR;
                    }
                    results.push(evaluated);
                }
                return self.interner.union(results);
            }
            let distribution_source = match self.interner.lookup(substituted) {
                Some(TypeData::Union(_)) => substituted,
                _ => crate::evaluation::evaluate::evaluate_type(self.interner, substituted),
            };
            if let Some(TypeData::Union(members)) = self.interner.lookup(distribution_source) {
                let members = self.interner.type_list(members);
                // Limit distribution to prevent OOM with pathologically
                // large unions (e.g. string-literal unions with thousands
                // of members). Shares the evaluation-path cap so both
                // lowering routes agree on what is representable.
                if members.len()
                    > crate::evaluation::evaluate_rules::conditional::MAX_CONDITIONAL_DISTRIBUTION_SIZE
                {
                    self.depth_exceeded = true;
                    return TypeId::ERROR;
                }
                let cond_type = self.interner.conditional(cond);
                let mut results = Vec::with_capacity(members.len());
                // Reuse one substitution map across members: only the
                // distributed parameter (`info.name`) changes per step, so
                // overwrite that single key instead of cloning the whole
                // map for every member (matters now the cap allows up to
                // `MAX_CONDITIONAL_DISTRIBUTION_SIZE` members).
                let mut member_subst = self.substitution.clone();
                for &member in members.iter() {
                    // Check depth before each distribution step
                    if self.depth_exceeded {
                        return TypeId::ERROR;
                    }
                    member_subst.insert(info.name, member);
                    let instantiated = if self.preserve_unsubstituted_type_params {
                        instantiate_type_preserving(self.interner, cond_type, &member_subst)
                    } else {
                        instantiate_type(self.interner, cond_type, &member_subst)
                    };
                    // Check if instantiation hit depth limit
                    if instantiated == TypeId::ERROR {
                        self.depth_exceeded = true;
                        return TypeId::ERROR;
                    }
                    // Don't evaluate here — the instantiator lacks a TypeResolver,
                    // so evaluate_type (with NoopResolver) can't resolve Lazy types
                    // in the conditional's check/extends positions. Instead, return
                    // the unevaluated conditionals and let the caller's evaluator
                    // (which has a proper resolver) handle evaluation.
                    results.push(instantiated);
                }
                return self.interner.union(results);
            }
        }
        let instantiated = ConditionalType {
            check_type: self.instantiate(cond.check_type),
            extends_type: self.instantiate(cond.extends_type),
            true_type: self.instantiate(cond.true_type),
            false_type: self.instantiate(cond.false_type),
            is_distributive: cond.is_distributive,
        };
        if instantiated == cond {
            return type_id;
        }
        self.interner.conditional(instantiated)
    }
}
