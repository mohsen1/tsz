//! Conditional type subtype checking.
//!
//! This module handles subtyping for TypeScript's conditional types:
//! - `T extends U ? X : Y`
//! - Distributive conditional types
//! - Branch compatibility checking

use std::sync::Arc;

use crate::types::{ConditionalType, TypeData, TypeId};
use crate::visitor::{contains_type_parameter_named, type_param_info};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Conditional extends-types use a stricter equivalence than ordinary
    /// assignability. In particular, tsc does not collapse `{ a?: T }` and
    /// `{ a?: T | undefined }` to the same type here, even when
    /// `exactOptionalPropertyTypes` is otherwise disabled.
    fn conditional_extends_types_equivalent(&mut self, left: TypeId, right: TypeId) -> bool {
        let prev = self.exact_optional_property_types;
        self.exact_optional_property_types = true;
        let equivalent =
            self.check_subtype(left, right).is_true() && self.check_subtype(right, left).is_true();
        self.exact_optional_property_types = prev;
        equivalent
    }

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
        if !self.conditional_extends_types_equivalent(source.extends_type, target.extends_type) {
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
        let constraint = self.get_conditional_constraint(cond);
        if let Some(constraint) = constraint
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
        //
        // IMPORTANT: Skip when the conditional is "non-deterministic" given the
        // constraint. A conditional T extends E ? X : Y is non-deterministic if:
        //   - constraint is NOT a subtype of E (not the "always-true" case), AND
        //   - some member of E IS a subtype of constraint
        //     (meaning some subtypes of constraint can satisfy E and others can't).
        //
        // Example: IsArray<T extends object> = T extends unknown[] ? true : false
        //   - `object` is not a subtype of `unknown[]` (not always-true)
        //   - `unknown[]` IS a subtype of `object` (arrays are objects)
        //   → non-deterministic: IsArray<string[]> = true, IsArray<object> = false
        //   → Strategy 1.5 would give IsArray<object> = false, incorrectly passing
        //     `false <: false` when T could be string[] giving true.
        if cond.is_distributive
            && let Some(param_info) = type_param_info(self.interner, cond.check_type)
            && let Some(constraint) = param_info.constraint
            && !contains_type_parameter_named(self.interner, constraint, param_info.name)
        {
            // Check if the conditional is deterministic for this constraint:
            // (a) constraint <: extends_type → always the true branch, deterministic
            // (b) no member of extends_type <: constraint → extends_type can never be
            //     satisfied by any subtype of constraint, always the false branch, deterministic
            // (c) otherwise → non-deterministic, skip Strategy 1.5
            let constraint_subtype_of_extends =
                self.check_subtype(constraint, cond.extends_type).is_true();
            let is_non_deterministic = if constraint_subtype_of_extends {
                false // always-true branch — deterministic
            } else if matches!(self.interner.lookup(constraint), Some(TypeData::Union(_))) {
                // Union constraint: distribution over each member is always deterministic.
                // e.g. ZeroOf<T extends number | string>: instantiate T→(number|string),
                // distribute → ZeroOf<number>|ZeroOf<string> = 0|"". Always correct.
                false
            } else {
                // Non-union constraint: check if some member of extends_type is a subtype
                // of constraint. If so, some subtypes of constraint could satisfy the
                // extends check while others can't (non-deterministic).
                let extends_type = cond.extends_type;
                match self.interner.lookup(extends_type) {
                    Some(TypeData::Union(union_id)) => {
                        let members: Arc<[TypeId]> = self.interner.type_list(union_id);
                        members
                            .iter()
                            .any(|&m| self.check_subtype(m, constraint).is_true())
                    }
                    _ => self.check_subtype(extends_type, constraint).is_true(),
                }
            };

            if !is_non_deterministic {
                use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
                let sub = TypeSubstitution::single(param_info.name, constraint);
                let cond_type_id = self.interner.conditional(*cond);
                let instantiated = instantiate_type(self.interner, cond_type_id, &sub);
                if instantiated != cond_type_id {
                    let evaluated = self.evaluate_type(instantiated);
                    if evaluated != cond_type_id && self.check_subtype(evaluated, target).is_true()
                    {
                        return SubtypeResult::True;
                    }
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

        // Default constraint: matches tsc's getDefaultConstraintOfConditionalType.
        // When either branch is `any`, tsc returns just the inferred true type
        // to avoid collapsing the constraint to `any` (since `X | any = any`).
        // This preserves the type information needed for proper assignability checks.
        let constraint = if inferred_true == TypeId::ANY || cond.false_type == TypeId::ANY {
            inferred_true
        } else {
            self.interner.union2(inferred_true, cond.false_type)
        };
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

    /// Check if source is a subtype of a conditional type target.
    ///
    /// When checking `source <: (T extends U ? X : Y)`, we use multiple strategies:
    ///
    /// 1. **Distributive constraint evaluation**: When the check type is a distributive
    ///    type parameter with a constraint, instantiate the conditional with T→constraint
    ///    and evaluate. If the conditional resolves to a concrete type that is a supertype
    ///    of source, succeed. This handles cases like `S <: UnrollOnHover<S>` where
    ///    S extends Schema extends object, and the conditional resolves to an identity
    ///    mapped type.
    ///
    /// 2. **Both branches**: Check that source is a subtype of both the true
    ///    branch (X) and false branch (Y).
    ///
    /// This handles cases where a concrete type needs to be assigned to a
    /// deferred conditional — e.g., `{ a: number } <: Foo<K>` where
    /// `type Foo<K> = K extends unknown ? { a: number } : unknown`.
    pub(crate) fn subtype_of_conditional_target(
        &mut self,
        source: TypeId,
        target: &ConditionalType,
    ) -> SubtypeResult {
        // Strategy 1: Distributive constraint evaluation for target-position conditionals.
        //
        // When the target conditional has a distributive check type parameter with a constraint,
        // instantiate the conditional with T→constraint and evaluate. This resolves the
        // conditional into a concrete type. Then check if source is assignable to that type.
        //
        // This matches tsc's getConstraintOfDistributiveConditionalType() behavior
        // for target-position conditionals.
        //
        // Example: `S <: UnrollOnHover<S>` where UnrollOnHover<O> = O extends object ? {[K in keyof O]: O[K]} : never
        // S extends Schema extends Record<string, unknown> extends object
        // Instantiate O → Schema (constraint of the check type parameter in context):
        //   Schema extends object ? {[K in keyof Schema]: Schema[K]} : never
        //   → {[K in keyof Schema]: Schema[K]} (resolves to true branch)
        // But we need to check S <: this result, which requires the original type parameter.
        // Instead, we instantiate with T→constraint where T is the conditional's own check type:
        //   constraint(O) = object → object extends object ? {[K in keyof object]: object[K]} : never → {}
        // This doesn't help. The correct approach is: try evaluating the conditional with
        // the source as the check type. If the source satisfies the extends clause,
        // check source against the true branch (with source substituted for check_type).
        if target.is_distributive
            && let Some(param_info) = type_param_info(self.interner, target.check_type)
        {
            // If the source is itself a type parameter with a constraint that satisfies
            // the extends clause, try resolving the conditional.
            if let Some(source_param) = type_param_info(self.interner, source)
                && let Some(source_constraint) = source_param.constraint
            {
                // Check if the source's constraint satisfies the extends clause.
                if self
                    .check_subtype(source_constraint, target.extends_type)
                    .is_true()
                {
                    // The conditional would resolve to the true branch when instantiated
                    // with a type that satisfies the extends clause.
                    // Substitute source for check_type in the true branch.
                    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
                    let sub = TypeSubstitution::single(param_info.name, source);
                    let instantiated_true = instantiate_type(self.interner, target.true_type, &sub);
                    let evaluated = self.evaluate_type(instantiated_true);
                    if self.check_subtype(source, evaluated).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }
        }

        // Strategy 2: Both branches must be supertypes of source.
        if self.check_subtype(source, target.true_type).is_true()
            && self.check_subtype(source, target.false_type).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }
}
