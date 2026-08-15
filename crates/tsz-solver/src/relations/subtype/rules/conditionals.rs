//! Conditional type subtype checking.
//!
//! This module handles subtyping for TypeScript's conditional types:
//! - `T extends U ? X : Y`
//! - Distributive conditional types
//! - Branch compatibility checking

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::type_queries::type_includes_undefined;
use crate::types::{ConditionalType, IntrinsicKind, TypeData, TypeId};
use crate::visitor::{
    conditional_type_id, contains_type_parameter_named, intrinsic_kind, type_param_info,
};

use super::super::{AnyPropagationMode, SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    fn conditional_identity_fallback_checker(&self) -> SubtypeChecker<'a, R> {
        let mut fallback = SubtypeChecker::with_resolver(self.interner, self.resolver)
            .with_any_propagation_mode(AnyPropagationMode::IdenticalOnly)
            .with_assume_related_on_cycle(self.assume_related_on_cycle)
            .with_assume_related_on_depth(self.assume_related_on_depth);
        if let Some(db) = self.query_db {
            fallback = fallback.with_query_db(db);
        }
        fallback
    }

    /// Conditional extends-types use a stricter equivalence than ordinary
    /// assignability. Two extends-types are equivalent only when their full
    /// per-property modifier shape matches, even when individual differences
    /// are invisible to ordinary subtype rules:
    ///
    /// - `{ a?: T }` is NOT collapsed with `{ a?: T | undefined }`, even
    ///   when `exactOptionalPropertyTypes` is otherwise disabled.
    /// - `{ readonly x: T }` is NOT collapsed with `{ x: T }`, even though
    ///   ordinary assignability is permissive about readonly. This is what
    ///   makes the higher-order `IfEquals` pattern (and `ReadonlyKeys` /
    ///   `MutableKeys` built on top of it) able to distinguish properties
    ///   by mutability.
    fn conditional_extends_types_equivalent(&mut self, left: TypeId, right: TypeId) -> bool {
        // For extends-clause identity (tsc's `isTypeIdenticalTo`) `any` is
        // identical only to `any`. The bidirectional check below runs with
        // `TopLevelOnly` any-propagation, so a top-level `any` would otherwise
        // relate to every type and wrongly equate `T extends any` with
        // `T extends string` (breaking the higher-order `Equal<X, Y>` trick).
        // Resolving one `Lazy(DefId)` level keeps `type A = any` recognised.
        if (self.resolve_lazy_type(left) == TypeId::ANY)
            != (self.resolve_lazy_type(right) == TypeId::ANY)
        {
            return false;
        }

        if !self.identity_fallback_property_modifiers_match(left, right) {
            return false;
        }

        if self.intersection_identity_compatible(left, right)
            && self.with_extends_clause_identity_mode(|sub| {
                sub.check_subtype(left, right).is_true() && sub.check_subtype(right, left).is_true()
            })
        {
            return true;
        }

        let left_eval = self.evaluate_type(left);
        let right_eval = self.evaluate_type(right);
        if (left_eval != left || right_eval != right)
            && self.intersection_identity_compatible(left_eval, right_eval)
            && self.with_extends_clause_identity_mode(|sub| {
                sub.check_subtype(left_eval, right_eval).is_true()
                    && sub.check_subtype(right_eval, left_eval).is_true()
            })
        {
            return true;
        }

        if !self.identity_fallback_property_modifiers_match(left_eval, right_eval) {
            return false;
        }

        if !self.intersection_identity_compatible(left_eval, right_eval) {
            return false;
        }

        let mut fallback = self.conditional_identity_fallback_checker();
        let fallback_events_at_entry = fallback.unresolved_lazy_relation_event_count();
        let fallback_incomplete_at_entry = fallback.incomplete_evaluation_relation_event_count();
        let fallback_limits_at_entry = fallback.relation_limit_event_count();
        let equivalent = fallback.check_subtype(left_eval, right_eval).is_true()
            && fallback.check_subtype(right_eval, left_eval).is_true();
        self.absorb_unresolved_lazy_relation_events_from(&fallback, fallback_events_at_entry);
        self.absorb_incomplete_evaluation_relation_events_from(
            &fallback,
            fallback_incomplete_at_entry,
        );
        self.absorb_relation_limit_events_from(&fallback, fallback_limits_at_entry);
        equivalent
    }

    /// Bidirectional subtyping approximates tsc's `isTypeIdenticalTo` for most
    /// extends-type shapes, but it is unsound whenever one side is an
    /// `Intersection` containing a member that already subsumes the whole
    /// comparison: `A & M` is mutually assignable with `A` alone whenever
    /// `A <: M` (the intersection contributes no extra constraint), even
    /// though `A & M` and `A` are different type nodes and tsc's real
    /// `isTypeIdenticalTo` does not conflate a redundant intersection with
    /// one of its own members. Gate every bidirectional-subtype acceptance in
    /// `conditional_extends_types_equivalent` on this: when either side is an
    /// `Intersection`, require an exact (order-independent) member-set match
    /// — matching tsc, which does not sort intersection members but does
    /// treat differently-ordered intersections of the same members as
    /// identical — rather than falling back to mutual assignability.
    fn intersection_identity_compatible(&self, left: TypeId, right: TypeId) -> bool {
        match (
            self.intersection_member_set(left),
            self.intersection_member_set(right),
        ) {
            (None, None) => true,
            (Some(left_members), Some(right_members)) => left_members == right_members,
            _ => false,
        }
    }

    /// Member set of an intersection extends-type, seen through object merging.
    ///
    /// `normalize_intersection` merges the object members of an intersection
    /// into a single synthesized `Object` (`{ a: 1 } & { a: 1 | number }`
    /// becomes one object shape), so by the time the relation layer sees the
    /// extends-type there is no `TypeData::Intersection` left to recognise and
    /// the guard above would read it as an ordinary object. The interner
    /// records the pre-merge intersection for exactly this reason
    /// (`store_merged_intersection_origin`: "stable structural provenance for
    /// semantic pruning and diagnostics", written once at merge time and never
    /// repainted), so recover the original members from it.
    ///
    /// Comparing member *sets* keeps this order-independent, so two merged
    /// intersections built from the same members in different orders stay
    /// identical while a merged intersection and a plain object never are —
    /// matching tsc, where `isTypeIdenticalTo` sees an `IntersectionType` on
    /// one side and an anonymous object on the other.
    fn intersection_member_set(&self, id: TypeId) -> Option<FxHashSet<TypeId>> {
        let intersection = match self.interner.lookup(id) {
            Some(TypeData::Intersection(list_id)) => {
                return Some(self.interner.type_list(list_id).iter().copied().collect());
            }
            _ => self.interner.get_merged_intersection_origin(id)?,
        };
        match self.interner.lookup(intersection) {
            Some(TypeData::Intersection(list_id)) => {
                Some(self.interner.type_list(list_id).iter().copied().collect())
            }
            _ => None,
        }
    }

    fn identity_fallback_property_modifiers_match(&self, left: TypeId, right: TypeId) -> bool {
        let left_members = match self.interner.lookup(left) {
            Some(TypeData::Union(list_id)) => self.interner.type_list(list_id).to_vec(),
            _ => vec![left],
        };
        let right_members = match self.interner.lookup(right) {
            Some(TypeData::Union(list_id)) => self.interner.type_list(list_id).to_vec(),
            _ => vec![right],
        };

        for left_member in &left_members {
            for right_member in &right_members {
                let left_shape = match self.interner.lookup(*left_member) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                        Some(self.interner.object_shape(shape_id))
                    }
                    _ => None,
                };
                let right_shape = match self.interner.lookup(*right_member) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                        Some(self.interner.object_shape(shape_id))
                    }
                    _ => None,
                };
                let (Some(left_shape), Some(right_shape)) = (left_shape, right_shape) else {
                    continue;
                };
                for left_prop in &left_shape.properties {
                    if let Some(right_prop) = right_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == left_prop.name)
                        && !self.property_identity_shapes_match(left_prop, right_prop)
                    {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn property_identity_shapes_match(
        &self,
        left: &crate::types::PropertyInfo,
        right: &crate::types::PropertyInfo,
    ) -> bool {
        left.optional == right.optional
            && left.readonly == right.readonly
            && left.is_symbol_named == right.is_symbol_named
            && type_includes_undefined(self.interner, left.type_id)
                == type_includes_undefined(self.interner, right.type_id)
    }

    /// Run `f` with the stricter property-modifier identity rules used by
    /// conditional `extends` equivalence (`exact_optional_property_types`
    /// and `strict_readonly_identity`). Both flags are restored on normal
    /// return.
    fn with_extends_clause_identity_mode<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved_exact_optional = self.exact_optional_property_types;
        let saved_strict_readonly = self.strict_readonly_identity;
        self.exact_optional_property_types = true;
        self.strict_readonly_identity = true;
        let result = self.with_identity_check_mode(|sub| {
            // `with_identity_check_mode` sets `TopLevelOnly`, which still lets
            // a nested `any` collapse into other types. tsc's
            // `isTypeIdenticalTo` treats `any` as identical only to `any` at
            // every depth, so override to `IdenticalOnly` for the extends-
            // clause equivalence check.
            let saved_any = sub.any_propagation;
            sub.any_propagation = AnyPropagationMode::IdenticalOnly;
            let inner = f(sub);
            sub.any_propagation = saved_any;
            inner
        });
        self.exact_optional_property_types = saved_exact_optional;
        self.strict_readonly_identity = saved_strict_readonly;
        result
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

        // Extends types must usually be structurally identical (equivalent).
        // A narrow non-distributive exception is needed for branch-refined
        // comparisons such as:
        //
        //   [T] extends [string] ? Y : AB
        //     <: [T] extends [number]
        //          ? ([T] extends [string] ? Y : A)
        //          : ([T] extends [string] ? Y : B)
        //
        // Under the target true branch (`T extends number`), the source must
        // be in its false branch because `[string]` and `[number]` are disjoint.
        // Under the target false branch, the original source conditional is
        // compared recursively against that branch.
        if !self.conditional_extends_types_equivalent(source.extends_type, target.extends_type) {
            if let Some(result) =
                self.check_disjoint_non_distributive_conditional_subtype(source, target)
            {
                return result;
            }
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

    fn check_disjoint_non_distributive_conditional_subtype(
        &mut self,
        source: &ConditionalType,
        target: &ConditionalType,
    ) -> Option<SubtypeResult> {
        if source.is_distributive || target.is_distributive {
            return None;
        }
        if !self
            .conditional_extends_types_definitely_disjoint(source.extends_type, target.extends_type)
        {
            return None;
        }

        let target_true = self.conditional_branch_under_extends_assumption(
            target.true_type,
            target.check_type,
            target.extends_type,
        );
        if !self.check_subtype(source.false_type, target_true).is_true() {
            return Some(SubtypeResult::False);
        }

        let source_id = self.interner.conditional(*source);
        Some(self.check_subtype(source_id, target.false_type))
    }

    fn conditional_branch_under_extends_assumption(
        &mut self,
        branch: TypeId,
        assumed_check_type: TypeId,
        assumed_extends_type: TypeId,
    ) -> TypeId {
        let Some(branch_cond_id) = conditional_type_id(self.interner, branch) else {
            return branch;
        };
        let branch_cond = self.interner.get_conditional(branch_cond_id);
        if branch_cond.is_distributive || branch_cond.check_type != assumed_check_type {
            return branch;
        }
        if self.conditional_extends_types_equivalent(branch_cond.extends_type, assumed_extends_type)
            || self
                .check_subtype(assumed_extends_type, branch_cond.extends_type)
                .is_true()
        {
            branch_cond.true_type
        } else if self.conditional_extends_types_definitely_disjoint(
            branch_cond.extends_type,
            assumed_extends_type,
        ) {
            branch_cond.false_type
        } else {
            branch
        }
    }

    fn conditional_extends_types_definitely_disjoint(&self, left: TypeId, right: TypeId) -> bool {
        if self.simple_types_definitely_disjoint(left, right) {
            return true;
        }

        match (self.interner.lookup(left), self.interner.lookup(right)) {
            (Some(TypeData::Tuple(left_elements)), Some(TypeData::Tuple(right_elements))) => {
                let left_elements = self.interner.tuple_list(left_elements);
                let right_elements = self.interner.tuple_list(right_elements);
                left_elements.len() == right_elements.len()
                    && left_elements
                        .iter()
                        .zip(right_elements.iter())
                        .any(|(left, right)| {
                            !left.rest
                                && !right.rest
                                && self
                                    .simple_types_definitely_disjoint(left.type_id, right.type_id)
                        })
            }
            _ => false,
        }
    }

    fn simple_types_definitely_disjoint(&self, left: TypeId, right: TypeId) -> bool {
        matches!(
            (
                intrinsic_kind(self.interner, left),
                intrinsic_kind(self.interner, right)
            ),
            (Some(IntrinsicKind::String), Some(IntrinsicKind::Number))
                | (Some(IntrinsicKind::Number), Some(IntrinsicKind::String))
                | (Some(IntrinsicKind::String), Some(IntrinsicKind::Boolean))
                | (Some(IntrinsicKind::Boolean), Some(IntrinsicKind::String))
                | (Some(IntrinsicKind::Number), Some(IntrinsicKind::Boolean))
                | (Some(IntrinsicKind::Boolean), Some(IntrinsicKind::Number))
                | (Some(IntrinsicKind::Bigint), Some(IntrinsicKind::String))
                | (Some(IntrinsicKind::String), Some(IntrinsicKind::Bigint))
                | (Some(IntrinsicKind::Bigint), Some(IntrinsicKind::Number))
                | (Some(IntrinsicKind::Number), Some(IntrinsicKind::Bigint))
                | (Some(IntrinsicKind::Bigint), Some(IntrinsicKind::Boolean))
                | (Some(IntrinsicKind::Boolean), Some(IntrinsicKind::Bigint))
                | (Some(IntrinsicKind::Symbol), Some(IntrinsicKind::String))
                | (Some(IntrinsicKind::String), Some(IntrinsicKind::Symbol))
                | (Some(IntrinsicKind::Symbol), Some(IntrinsicKind::Number))
                | (Some(IntrinsicKind::Number), Some(IntrinsicKind::Symbol))
                | (Some(IntrinsicKind::Symbol), Some(IntrinsicKind::Boolean))
                | (Some(IntrinsicKind::Boolean), Some(IntrinsicKind::Symbol))
                | (Some(IntrinsicKind::Symbol), Some(IntrinsicKind::Bigint))
                | (Some(IntrinsicKind::Bigint), Some(IntrinsicKind::Symbol))
        )
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

        // Strategy 1.25: getConstraintFromConditionalType for an infer-extraction
        // conditional whose check type is a constrained type parameter.
        //
        // For `Parameters<F>` (= `F extends (...args: infer P) => any ? P :
        // never`), the default constraint above is the union of branch results,
        // which leaves the rest-position `infer P` unresolved — so it never
        // recognizes the array base `never[]`. tsc instead substitutes the check
        // type `F` with its constraint and re-instantiates: `AnyFunction extends
        // (...args: infer P) => any ? P : never` evaluates to `never[]`, the
        // concrete apparent base.
        //
        // This is gated on an `infer` in the extends type: a predicate
        // conditional without `infer` (e.g. `T extends unknown[] ? true :
        // false`) keeps the determinism guard in Strategy 1.5 below, which this
        // must not bypass — substituting its check type and picking a single
        // branch would unsoundly narrow a non-deterministic result. An
        // infer-extraction conditional has no such hazard: the true branch is
        // the matched-and-extracted shape, so the instantiated extraction is a
        // sound upper bound.
        if let Some(evaluated) =
            self.infer_extraction_conditional_constraint(self.interner.conditional(*cond))
            && self.check_subtype(evaluated, target).is_true()
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
            // (c) the extends type is an `infer` pattern (`F extends (...a: infer P)
            //     => any ? P : never`) — an *extraction* conditional rather than a
            //     discriminating test. Instantiating the check type with its
            //     constraint and evaluating yields the genuine extracted type, which
            //     is exactly the constraint tsc reads via
            //     `getConstraintOfDistributiveConditionalType`. The
            //     subtype-of-extends heuristic below is meaningless for a pattern
            //     (the infer placeholder always "matches"), so it must not gate
            //     these out.
            // (d) otherwise → non-deterministic, skip Strategy 1.5
            let extends_is_inference_pattern =
                crate::type_queries::contains_infer_types_db(self.interner, cond.extends_type);
            let is_non_deterministic = if extends_is_inference_pattern
                || self.check_subtype(constraint, cond.extends_type).is_true()
            {
                // extraction pattern, or constraint <: extends (always-true
                // branch) — deterministic. The `||` short-circuits, so the
                // subtype check is skipped for an inference pattern (where it
                // would be meaningless).
                false
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
                    // A `never` infer-extraction result means there is no
                    // distributive constraint, so that path falls back to the
                    // default constraint. For non-infer conditionals such as
                    // `ZeroOf<T extends {}>`, `never` remains a legitimate
                    // subtype witness.
                    if evaluated != cond_type_id
                        && (!extends_is_inference_pattern || evaluated != TypeId::NEVER)
                        && self.check_subtype(evaluated, target).is_true()
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

    /// tsc's `getConstraintFromConditionalType` for an infer-extraction
    /// conditional whose check type is a constrained type parameter.
    ///
    /// Substitutes the check-type parameter with its own constraint and
    /// evaluates the result, so a deferred extraction utility such as
    /// `Parameters<F>` (`F extends (...args: infer P) => any ? P : never`)
    /// resolves to its concrete apparent base (`never[]`). Returns `None` when
    /// `type_id` is not a deferred conditional, when its extends type has no
    /// `infer` (a predicate conditional — substituting and picking a branch
    /// would unsoundly narrow a non-deterministic result, so those keep the
    /// determinism-guarded distributive path), when the check type is not a
    /// type parameter, or when the instantiation collapses to `never`.
    pub(crate) fn infer_extraction_conditional_constraint(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let cond_id = crate::type_queries::get_conditional_type_id(self.interner, type_id)?;
        let cond = self.interner.conditional_type(cond_id);
        // Cheap structural gate first: a non-type-parameter check type can never
        // profit, so short-circuit before the `contains_infer_types_db` tree walk.
        if type_param_info(self.interner, cond.check_type).is_none()
            || !crate::type_queries::contains_infer_types_db(self.interner, cond.extends_type)
        {
            return None;
        }
        // The shallow base query intentionally preserves resolver-owned alias
        // applications. Expose that application before substitution so a
        // distributive extraction sees each constraint member rather than an
        // opaque alias that later collapses to `unknown` as one whole check.
        let raw_constraint =
            crate::type_queries::get_base_constraint_of_type(self.interner, cond.check_type);
        if raw_constraint == cond.check_type {
            return None;
        }
        let exposed_constraint = self.evaluate_type(raw_constraint);
        // Checker-owned relations use the query-backed exact rewriter so a
        // same-named sibling binder is not replaced. Standalone
        // TypeDatabase-only relations retain the legacy name-based fallback.
        let substituted = if let Some(query_db) = self.query_db {
            crate::type_queries::conditional_check_type_substituted_with_constraint_exact(
                query_db,
                type_id,
                exposed_constraint,
            )
        } else {
            crate::type_queries::conditional_check_type_substituted_with_constraint(
                self.interner,
                type_id,
                exposed_constraint,
            )
        }?;
        let evaluated = self.evaluate_type(substituted);
        (evaluated != TypeId::NEVER).then_some(evaluated)
    }

    /// Compute the default constraint of a deferred conditional type.
    ///
    /// Delegates to the shared [`crate::type_queries::conditional_default_constraint_from_data`]
    /// query (tsc's `getDefaultConstraintOfConditionalType`): for
    /// `T extends U ? X : Y` it yields `X[T := T & U] | Y`, handling the
    /// Extract-style patterns (`T extends U ? T : Y` and nested Extract chains)
    /// without full instantiation. Returns `None` when the conditional is not
    /// deferred (neither operand contains type parameters).
    fn get_conditional_constraint(&self, cond: &ConditionalType) -> Option<TypeId> {
        crate::type_queries::conditional_default_constraint_from_data(self.interner, cond)
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
        let target_id = self.interner.conditional(*target);
        if source == target_id {
            return SubtypeResult::True;
        }
        if let Some(source_cond_id) = conditional_type_id(self.interner, source) {
            let source_cond = self.interner.get_conditional(source_cond_id);
            if source_cond.check_type == target.check_type
                && source_cond.extends_type == target.extends_type
                && source_cond.true_type == target.true_type
                && source_cond.false_type == target.false_type
                && source_cond.is_distributive == target.is_distributive
            {
                return SubtypeResult::True;
            }
        }

        // `T extends T ? T : never` is exactly `T`, even for `T = never`
        // where distributivity also produces `never`. This target-position
        // identity shows up through Extract<T, T>-style aliases.
        if source == target.check_type
            && target.extends_type == target.check_type
            && target.true_type == target.check_type
            && target.false_type == TypeId::NEVER
        {
            return SubtypeResult::True;
        }

        // `T extends unknown ? T : never` and `T extends any ? T : never`
        // are transparent identity conditionals in target position. Keep this
        // narrower than the general Extract-like path: `T extends object ? T :
        // never` is not transparent for unconstrained T.
        if source == target.check_type
            && target.true_type == target.check_type
            && target.false_type == TypeId::NEVER
            && (target.extends_type == TypeId::UNKNOWN || target.extends_type == TypeId::ANY)
        {
            return SubtypeResult::True;
        }

        let target_contains_unbound_infer =
            crate::type_queries::contains_infer_types_db(self.interner, target.extends_type)
                || crate::type_queries::contains_infer_types_db(self.interner, target.true_type)
                || crate::type_queries::contains_infer_types_db(self.interner, target.false_type);
        // `contains_type_parameters` matches both `TypeParameter` and `Infer`, so a
        // `false` here means `check_type` is fully concrete — no free type parameters
        // and no `infer` variables.
        let check_type_is_generic =
            crate::visitor::contains_type_parameters(self.interner, target.check_type);
        let target_has_generic_extends = check_type_is_generic
            && crate::visitor::contains_type_parameters(self.interner, target.extends_type);

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
        if !target_contains_unbound_infer
            && target.is_distributive
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

        if target_contains_unbound_infer || target_has_generic_extends {
            // A target-position conditional whose CHECK type is fully concrete —
            // no free type parameters and no `infer` variables — is statically
            // determinable even when its extends clause still carries an `infer`
            // pattern: the concrete check either matches the pattern (true branch,
            // binding the infer) or fails it (false branch). Such a conditional
            // only survives here in deferred form when its extends application
            // base was transiently unresolved at the time the enclosing mapped
            // type was evaluated — e.g. a homomorphic unwrapper
            // `{ [K in keyof T]: T[K] extends Promise<infer U> ? U : T[K] }`
            // stores the `string`-keyed property as
            // `string extends Promise<infer U> ? U : string` before `Promise`'s
            // lib base has materialized. The base *is* resolvable in this relation
            // context, so reduce the conditional to its real branch and relate
            // against that, rather than bailing to a spurious `False` — the
            // reflexivity-breaking `string` not assignable to `string`, i.e.
            // `M` not assignable to `M` (#17537). When evaluation cannot make
            // progress (the base is still unresolved here too) the conditional is
            // unchanged and the original deferred `False` stands.
            if !check_type_is_generic {
                let evaluated = self.evaluate_type(target_id);
                if evaluated != target_id {
                    return self.check_subtype(source, evaluated);
                }
            }
            return SubtypeResult::False;
        }

        // Strategy 1.5: Evaluate statically determinable conditionals.
        //
        // When the check type and extends type are both concrete (contain no type
        // parameters), the conditional resolves to a single branch without needing
        // any substitution context. Evaluate it and check source against the result.
        //
        // This handles generic defaults with conditional types that become fully
        // concrete after substitution, e.g.:
        //   type Wrap<K, V, M = K extends string ? Map<K, V> : Map<string, V>>
        // After Test<string, number>: M = string extends string ? Map<string,number> : ...
        // which evaluates to Map<string,number>. Strategy 2 below would also reach
        // the correct answer when both branches are identical, but this strategy
        // correctly handles the case where only one branch matches the source.
        if !crate::visitor::contains_type_parameters(self.interner, target.check_type)
            && !crate::visitor::contains_type_parameters(self.interner, target.extends_type)
        {
            let evaluated = self.evaluate_type(target_id);
            if evaluated != target_id {
                return self.check_subtype(source, evaluated);
            }
        }

        // Strategy 2: Branch compatibility — both branches must be supertypes
        // of source, and a failed true branch is decisive: a recursive
        // conditional alias whose false branch re-applies the alias
        // (`Grow<T, N> = ... ? T : Grow<[X, ...T], N>`) re-enters this rule
        // with an ever-growing target each level, burning the relation depth
        // budget (spurious TS2859, or a depth-exceeded "assume related" false
        // negative) where tsc reports a plain relation failure. tsc's
        // `structuredTypeRelatedTo` likewise consults the false branch only
        // after the true branch succeeded.
        if !self.check_subtype(source, target.true_type).is_true() {
            return SubtypeResult::False;
        }

        // A distributive `T extends any|unknown ? X : never` target is the
        // canonical shape of utility aliases that map over each constituent of
        // `T`. For any non-empty constituent the false branch is unreachable;
        // for `never`, distribution produces `never` rather than a value
        // outside `X`. When the source already fits `X`, do not require it to
        // also fit `never`.
        if target.is_distributive
            && target.false_type == TypeId::NEVER
            && (target.extends_type == TypeId::ANY || target.extends_type == TypeId::UNKNOWN)
        {
            return SubtypeResult::True;
        }

        if self.check_subtype(source, target.false_type).is_true() {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;

    #[test]
    fn conditional_identity_fallback_inherits_cycle_and_depth_policies() {
        let interner = TypeInterner::new();
        let checker = SubtypeChecker::new(&interner)
            .with_assume_related_on_cycle(false)
            .with_assume_related_on_depth(false);

        let fallback = checker.conditional_identity_fallback_checker();

        assert!(!fallback.assume_related_on_cycle);
        assert!(!fallback.assume_related_on_depth);
    }
}
