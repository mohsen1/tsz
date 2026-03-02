//! Conditional type subtype checking.
//!
//! This module handles subtyping for TypeScript's conditional types:
//! - `T extends U ? X : Y`
//! - Distributive conditional types
//! - Branch compatibility checking

use crate::types::{ConditionalType, TypeData, TypeId};
use crate::visitor::type_param_info;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check conditional type to conditional type subtyping.
    ///
    /// For two conditional types `S extends U ? X : Y` <: `T extends V ? X' : Y'`:
    ///
    /// ## Subtyping Rules (matches tsc):
    /// 1. **Distributive flags must match**
    /// 2. **Extends types must be equivalent** (bidirectional subtype)
    /// 3. **Check types must be related** in either direction
    ///    (not strict equivalence — this handles generic interface variance)
    /// 4. **Branch compatibility**: both true and false branches must be compatible
    ///
    /// The relaxed check-type rule (step 3) is critical for variance to work
    /// through conditional types. When comparing properties of `Covariant<A>`
    /// vs `Covariant<B>` (where B extends A), the expanded conditional types
    /// `A extends string ? A : number` vs `B extends string ? B : number`
    /// have check types that are related but not equivalent.
    pub(crate) fn check_conditional_subtype(
        &mut self,
        source: &ConditionalType,
        target: &ConditionalType,
    ) -> SubtypeResult {
        if source.is_distributive != target.is_distributive {
            return SubtypeResult::False;
        }

        // Extends types must be structurally identical (equivalent).
        if !self.types_equivalent(source.extends_type, target.extends_type) {
            return SubtypeResult::False;
        }

        // Check types must be related in either direction.
        // tsc: isRelatedTo(source.checkType, target.checkType) ||
        //      isRelatedTo(target.checkType, source.checkType)
        if !self
            .check_subtype(source.check_type, target.check_type)
            .is_true()
            && !self
                .check_subtype(target.check_type, source.check_type)
                .is_true()
        {
            return SubtypeResult::False;
        }

        if self
            .check_subtype(source.true_type, target.true_type)
            .is_true()
            && self
                .check_subtype(source.false_type, target.false_type)
                .is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if a conditional type source is assignable to a concrete target.
    ///
    /// When checking `T extends U ? X : Y <: target`, we use two strategies:
    ///
    /// 1. **Default constraint** (tsc's `getConstraintOfConditionalType`):
    ///    Compute the "inferred true type" by replacing the check type in the
    ///    true branch with `check_type & extends_type`, then union with the
    ///    false branch. If this constraint is a subtype of target, succeed.
    ///    For `Extract<T, Function>` (= `T extends Function ? T : never`),
    ///    the constraint is `T & Function`, which is assignable to `Function`.
    ///
    /// 2. **Both branches** (fallback): Check that both the true and false
    ///    branches are individually subtypes of target.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Constraint approach: Extract<T, Function> <: Function
    /// // Constraint = T & Function | never = T & Function
    /// // T & Function <: Function ✅
    ///
    /// // Both branches approach:
    /// // type T = boolean extends true ? "yes" : "no";
    /// // "yes" <: string and "no" <: string ✅
    /// ```
    pub(crate) fn conditional_branches_subtype(
        &mut self,
        cond: &ConditionalType,
        target: TypeId,
    ) -> SubtypeResult {
        // Strategy 1: Try default constraint of the conditional type.
        // This matches tsc's getConstraintOfConditionalType / getDefaultConstraintOfConditionalType.
        if let Some(constraint) = self.get_conditional_constraint(cond)
            && self.check_subtype(constraint, target).is_true()
        {
            return SubtypeResult::True;
        }

        // Strategy 1.5: Distributive constraint evaluation.
        //
        // When the check_type is a distributive type parameter with a constraint,
        // instantiate the conditional with T→constraint and evaluate. This distributes
        // the conditional over the constraint union, producing a concrete type.
        //
        // Example: ZeroOf<T> where T extends number | string
        //   ZeroOf<T> = T extends number ? 0 : T extends string ? "" : false
        //   Instantiate T → number | string:
        //   (number | string) extends number ? 0 : ...
        //   Distribute: ZeroOf<number> | ZeroOf<string> = 0 | ""
        //   0 | "" <: number | string ✓
        //
        // This matches tsc's getConstraintOfDistributiveConditionalType().
        if cond.is_distributive
            && let Some(param_info) = type_param_info(self.interner, cond.check_type)
            && let Some(constraint) = param_info.constraint
        {
            use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
            let mut sub = TypeSubstitution::new();
            sub.insert(param_info.name, constraint);
            let cond_type_id = self.interner.conditional(cond.clone());
            let instantiated = instantiate_type(self.interner, cond_type_id, &sub);
            if instantiated != cond_type_id {
                let evaluated = self.evaluate_type(instantiated);
                if evaluated != cond_type_id && self.check_subtype(evaluated, target).is_true() {
                    return SubtypeResult::True;
                }
            }
        }

        // Strategy 2: Both branches must be subtypes of target.
        if self.check_subtype(cond.true_type, target).is_true()
            && self.check_subtype(cond.false_type, target).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Compute the default constraint of a conditional type.
    ///
    /// For `T extends U ? X : Y`, the default constraint is:
    ///   `X[T := T & U] | Y`
    ///
    /// where `X[T := T & U]` means the true branch with the check type
    /// replaced by the intersection of `check_type` and `extends_type`.
    /// This is tsc's "inferred true type" computation.
    ///
    /// Currently handles these patterns:
    /// - `T extends U ? T : Y` → `(T & U) | Y` (Extract pattern)
    /// - Other patterns: returns the general `X | Y` union
    fn get_conditional_constraint(&self, cond: &ConditionalType) -> Option<TypeId> {
        // Compute the default constraint for deferred conditional types.
        //
        // Deferred conditionals arise when:
        // - The check_type contains type parameters (T extends U ? X : Y)
        // - The extends_type contains type parameters (X extends T ? X : Y)
        // In either case the evaluator cannot pick a branch and the conditional
        // remains deferred, so we need a constraint for assignability checks.
        let is_check_type_param = matches!(
            self.interner.lookup(cond.check_type),
            Some(TypeData::TypeParameter(_))
        );

        let check_has_params = is_check_type_param
            || crate::visitor::contains_type_parameters(self.interner, cond.check_type);
        let extends_has_params =
            crate::visitor::contains_type_parameters(self.interner, cond.extends_type);

        // If neither check_type nor extends_type contains type parameters,
        // the evaluator would have already picked a branch — no constraint needed.
        if !check_has_params && !extends_has_params {
            return None;
        }

        // Compute the "inferred true type": the true branch with the check type
        // replaced by check_type & extends_type.
        //
        // tsc uses full instantiation (replaceTypes) to substitute check_type
        // with check_type & extends_type throughout the true branch. We can't do
        // full instantiation in the subtype checker, but we handle common patterns:
        //
        // 1. Extract-like: `T extends U ? T : Y` → inferred true = T & U
        // 2. Nested Extract: `T extends U ? (T extends V ? T : never) : never`
        //    → recursively compute inner constraint (T & V), then intersect with U
        //    → result: T & U & V
        let inferred_true = if cond.true_type == cond.check_type {
            // Extract-like pattern: X extends U ? X : Y
            // Inferred true = X & U
            // This covers both:
            // - Distributive: T extends U ? T : never (check_type is type param)
            // - Non-distributive: string[] extends T ? string[] : never (extends_type is type param)
            self.interner
                .intersection2(cond.check_type, cond.extends_type)
        } else if is_check_type_param {
            // Check_type is a bare type parameter. The true branch might be:
            // (a) A nested conditional with the same check_type (Extract2 pattern)
            // (b) Some other type referencing check_type
            if let Some(inner_constraint) =
                self.get_nested_conditional_constraint(cond.true_type, cond.check_type)
            {
                // Nested conditional with same check_type.
                // Inner constraint represents the constraint from inner conditionals
                // (e.g., T & Bar for `T extends Bar ? T : never`).
                // Intersect with outer extends_type to combine all constraints.
                // For `T extends Foo ? (T extends Bar ? T : never) : never`:
                //   inner_constraint = T & Bar, result = T & Foo & Bar
                self.interner
                    .intersection2(inner_constraint, cond.extends_type)
            } else if self.type_references_check_type(cond.true_type, cond.check_type) {
                // True branch references check_type but isn't identical to it
                // and isn't a nested conditional we can handle.
                // We can't do full instantiation in the subtype checker, so
                // fall back to using the true type as-is (less precise but safe).
                cond.true_type
            } else {
                cond.true_type
            }
        } else {
            // True branch doesn't reference check_type at all, or check_type
            // is a complex generic type (not a type parameter).
            // Inferred true = X (unchanged).
            cond.true_type
        };

        // Default constraint = inferred_true | false_type
        let constraint = self.interner.union2(inferred_true, cond.false_type);
        Some(constraint)
    }

    /// Check if a type references the given `check_type` (a type parameter).
    /// Used to determine if the true branch needs substitution.
    fn type_references_check_type(&self, ty: TypeId, check_type: TypeId) -> bool {
        if ty == check_type {
            return true;
        }
        // Check common wrapper types that might contain the check type
        match self.interner.lookup(ty) {
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                let member_list = self.interner.type_list(members);
                member_list.contains(&check_type)
            }
            _ => false,
        }
    }

    /// Try to compute a constraint for a nested conditional with the same `check_type`.
    ///
    /// If `ty` is a `Conditional` whose `check_type` equals `outer_check_type`,
    /// recursively compute its default constraint (which may itself recurse for
    /// deeper nesting). Returns `None` if `ty` is not a matching conditional.
    ///
    /// This handles the Extract2 pattern and similar nested Extract chains:
    /// ```text
    /// type Extract2<T, U, V> = T extends U ? T extends V ? T : never : never;
    /// // Outer: T extends U ? <inner> : never
    /// // Inner: T extends V ? T : never
    /// // Inner constraint: T & V
    /// // Outer constraint: (T & V) & U = T & U & V
    /// ```
    fn get_nested_conditional_constraint(
        &self,
        ty: TypeId,
        outer_check_type: TypeId,
    ) -> Option<TypeId> {
        if let Some(TypeData::Conditional(inner_cond_id)) = self.interner.lookup(ty) {
            let inner = self.interner.conditional_type(inner_cond_id);
            if inner.check_type == outer_check_type {
                // Same check_type — compute the inner conditional's constraint.
                // This recurses for arbitrary depth of nesting.
                return self.get_conditional_constraint(&inner);
            }
        }
        None
    }

    /// Check if source is a subtype of both branches of a conditional type.
    ///
    /// When checking `source <: (T extends U ? X : Y)`, we need to verify that:
    /// - Source is a subtype of both the true branch (X) and false branch (Y)
    ///
    /// This is used when the target is a conditional type and we need to check
    /// if the source can be assigned to it regardless of which branch is selected.
    /// tsc is very strict here — even `X | Y` (union of both branches) cannot be
    /// assigned to a deferred conditional. Only identical conditional types or
    /// `never` can satisfy this check.
    ///
    /// ## Logic:
    /// - `source <: X` AND `source <: Y` => True
    /// - Otherwise => False
    pub(crate) fn subtype_of_conditional_target(
        &mut self,
        source: TypeId,
        target: &ConditionalType,
    ) -> SubtypeResult {
        if self.check_subtype(source, target.true_type).is_true()
            && self.check_subtype(source, target.false_type).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }
}
