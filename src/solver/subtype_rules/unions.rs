//! Union and intersection type subtype checking.
//!
//! This module handles subtyping for TypeScript's composite types:
//! - Union types (A | B | C) - source must be subtype of at least one member
//! - Intersection types (A & B & C) - source must be subtype of all members
//! - Distributivity rules between unions and intersections
//! - Type parameter compatibility in union/intersection contexts

use crate::solver::types::*;

use super::super::{SubtypeChecker, SubtypeFailureReason, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if a union type is a subtype of a target type.
    ///
    /// Union source: all members must be subtypes of target.
    /// When target is an intersection, applies distributivity rules.
    ///
    /// ## Union Source Rule:
    /// `(A | B | C) <: T` if `A <: T` AND `B <: T` AND `C <: T`
    ///
    /// ## Distributivity:
    /// `(A | B) <: (C & D)` requires each union member to satisfy ALL intersection members
    pub(crate) fn check_union_source_subtype(
        &mut self,
        members: TypeListId,
        target: TypeId,
        target_key: &TypeKey,
    ) -> SubtypeResult {
        // Distributivity: (A | B) & C distributes to (A & C) | (B & C)
        if let TypeKey::Intersection(inter_members) = target_key {
            let inter_members = self.interner.type_list(*inter_members);
            let union_members = self.interner.type_list(members);

            // Check: (A | B) <: (C & D)
            for &union_member in union_members.iter() {
                let mut satisfies_all = true;
                for &inter_member in inter_members.iter() {
                    if !self.check_subtype(union_member, inter_member).is_true() {
                        satisfies_all = false;
                        break;
                    }
                }
                if !satisfies_all {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        let members = self.interner.type_list(members);
        for &member in members.iter() {
            // Don't accept `any` as universal subtype in union checks
            if member == TypeId::ANY && target != TypeId::ANY {
                return SubtypeResult::False;
            }
            if !self.check_subtype(member, target).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    /// Check if a source type is a subtype of a union type.
    ///
    /// When the target is a union, the source must be assignable to AT LEAST ONE
    /// union member. This is the "exists" quantifier - there exists some union member
    /// that the source is compatible with.
    ///
    /// ## Union Target Rule:
    /// `S <: (A | B | C)` if `S <: A` OR `S <: B` OR `S <: C`
    ///
    /// ## Keyof Special Case:
    /// If source is keyof T and the union includes all primitive types (string | number | symbol),
    /// then it's compatible (keyof can match any property key type).
    ///
    /// ## Examples:
    /// ```typescript
    /// // string <: (string | number) ✅
    /// // string <: (number | boolean) ❌
    /// // never <: (string | number) ✅ (never is subtype of everything)
    /// // keyof T <: (string | number | symbol) ✅ (if union is all primitives)
    /// ```
    pub(crate) fn check_union_target_subtype(
        &mut self,
        source: TypeId,
        source_key: &TypeKey,
        members: TypeListId,
    ) -> SubtypeResult {
        if matches!(source_key, TypeKey::KeyOf(_)) && self.union_includes_keyof_primitives(members)
        {
            return SubtypeResult::True;
        }
        let members = self.interner.type_list(members);
        for &member in members.iter() {
            if member == TypeId::ANY && source != TypeId::ANY {
                continue;
            }
            if self.check_subtype(source, member).is_true() {
                return SubtypeResult::True;
            }
        }
        SubtypeResult::False
    }

    /// Check if an intersection type is a subtype of a target type.
    ///
    /// When the source is an intersection, it's a subtype if ANY constituent is a subtype
    /// of the target. This is the "exists" quantifier for intersections.
    ///
    /// ## Intersection Source Rule:
    /// `(A & B & C) <: T` if `A <: T` OR `B <: T` OR `C <: T`
    ///
    /// ## Type Parameter Constraint Narrowing:
    /// For intersections containing type parameters, we attempt to narrow the parameter's
    /// constraint by intersecting it with the other members. This handles cases like:
    /// ```typescript
    /// function foo<T extends string | number>(x: T & { foo: string }): void
    /// // Here T & { foo: string } is assignable to T because we can narrow T's constraint
    /// ```
    ///
    /// ## Examples:
    /// ```typescript
    /// // (string & number) <: string ✅ (impossible, but if it were possible)
    /// // (string & { foo: number }) <: string ❌ (neither constituent alone satisfies)
    /// // never <: (string | number) ✅ (never is subtype of everything)
    /// ```
    pub(crate) fn check_intersection_source_subtype(
        &mut self,
        members: TypeListId,
        target: TypeId,
    ) -> SubtypeResult {
        let members = self.interner.type_list(members);

        // First, check if any member is directly a subtype
        for &member in members.iter() {
            if self.check_subtype(member, target).is_true() {
                return SubtypeResult::True;
            }
        }

        // For type parameters in intersections, try narrowing the constraint
        for &member in members.iter() {
            if let Some(TypeKey::TypeParameter(param_info)) | Some(TypeKey::Infer(param_info)) =
                self.interner.lookup(member)
                && let Some(constraint) = param_info.constraint
            {
                let other_members: Vec<TypeId> =
                    members.iter().filter(|&&m| m != member).copied().collect();

                if !other_members.is_empty() {
                    let mut all_members = vec![constraint];
                    all_members.extend(other_members);
                    let narrowed_constraint = self.interner.intersection(all_members);

                    if self.check_subtype(narrowed_constraint, target).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }
        }

        SubtypeResult::False
    }

    /// Check if a source type is a subtype of an intersection type.
    ///
    /// When the target is an intersection, the source must be a subtype of ALL
    /// intersection members. This is the "forall" quantifier - for every member in
    /// the intersection, the source must satisfy it.
    ///
    /// ## Intersection Target Rule:
    /// `S <: (A & B & C)` if `S <: A` AND `S <: B` AND `S <: C`
    ///
    /// ## Dual to Union Source:
    /// This is the dual of union source checking:
    /// - Union source (S1 | S2) <: T requires BOTH S1 <: T AND S2 <: T
    /// - Intersection target: S <: (T1 & T2) requires S <: T1 AND S <: T2
    ///
    /// ## Examples:
    /// ```typescript
    /// // { name: string; age: number } <: ({ name: string } & { age: number }) ✅
    /// // { name: string } <: ({ name: string } & { age: number }) ❌
    /// // never <: (string & number) ✅ (never is subtype of everything)
    /// ```
    pub(crate) fn check_intersection_target_subtype(
        &mut self,
        source: TypeId,
        members: TypeListId,
    ) -> SubtypeResult {
        let members = self.interner.type_list(members);
        for &member in members.iter() {
            if !self.check_subtype(source, member).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    /// Check if two types are equivalent (mutually subtypes).
    ///
    /// Type equivalence means bidirectional subtyping:
    /// `A ≡ B` iff `A <: B` AND `B <: A`
    ///
    /// ## Examples:
    /// - `string` ≡ `string` ✅ (reflexive)
    /// - `A | B` ≡ `B | A` ✅ (union commutes)
    /// - `T & U` ≡ `U & T` ✅ (intersection commutes)
    ///
    /// Note: For most type checking, unidirectional subtyping (`<:`) is used.
    /// Equivalence (`≡`) is primarily for type parameter constraints and exact matching.
    pub(crate) fn types_equivalent(&mut self, left: TypeId, right: TypeId) -> bool {
        self.check_subtype(left, right).is_true() && self.check_subtype(right, left).is_true()
    }

    /// Check if a union includes all primitive types (string, number, symbol).
    ///
    /// This is used to optimize `keyof` type checking. When a union contains
    /// all three primitives, `keyof` returns the union of all their keys.
    ///
    /// ## Example:
    /// ```typescript
    /// type T = string | number | symbol;
    /// type Keys = keyof T; // Returns string | number | symbol
    /// ```
    ///
    /// Returns true if all three primitives are present in the union.
    pub(crate) fn union_includes_keyof_primitives(&self, members: TypeListId) -> bool {
        let members = self.interner.type_list(members);
        let mut has_string = false;
        let mut has_number = false;
        let mut has_symbol = false;

        for &member in members.iter() {
            match member {
                TypeId::STRING => has_string = true,
                TypeId::NUMBER => has_number = true,
                TypeId::SYMBOL => has_symbol = true,
                _ => {}
            }
            if has_string && has_number && has_symbol {
                return true;
            }
        }

        false
    }

    /// Check if a type parameter is a subtype of a target type.
    ///
    /// Handles both type parameter vs type parameter and type parameter vs concrete type.
    /// Implements TypeScript's soundness rules for type parameter compatibility.
    ///
    /// ## TypeScript Soundness Rules:
    /// - Same type parameter by name → reflexive (always compatible)
    /// - Different type parameters → check constraint transitivity
    /// - Type parameter vs concrete → constraint must be subtype of concrete
    /// - Unconstrained type parameter → acts like `unknown` (top type)
    pub(crate) fn check_type_parameter_subtype(
        &mut self,
        s_info: &TypeParamInfo,
        target: TypeId,
        target_key: &TypeKey,
    ) -> SubtypeResult {
        // Type parameter vs type parameter
        if let TypeKey::TypeParameter(t_info) | TypeKey::Infer(t_info) = target_key {
            // Same type parameter by name - reflexive
            if s_info.name == t_info.name {
                return SubtypeResult::True;
            }

            // Different type parameters - check if source's constraint implies compatibility
            // TypeScript soundness: T <: U only if:
            // 1. Constraint(T) is exactly U (e.g., U extends T, checking U <: T)
            // 2. Constraint(T) extends U's constraint transitively
            if let Some(s_constraint) = s_info.constraint {
                // Check if source's constraint IS the target type parameter itself
                if s_constraint == target {
                    return SubtypeResult::True;
                }
                // Check if source's constraint is a subtype of the target type parameter
                if self.check_subtype(s_constraint, target).is_true() {
                    return SubtypeResult::True;
                }
            }
            // Two different type parameters with independent constraints are not interchangeable
            return SubtypeResult::False;
        }

        // Type parameter vs concrete type
        if let Some(constraint) = s_info.constraint {
            return self.check_subtype(constraint, target);
        }

        // Unconstrained type parameter acts like `unknown` (top type)
        // An unconstrained type param as source cannot be assigned to a concrete target
        SubtypeResult::False
    }

    /// Check subtype with optional method bivariance.
    ///
    /// When `allow_bivariant` is true, temporarily disables strict function types
    /// to allow bivariant parameter checking. This is used for method compatibility
    /// where TypeScript allows bivariance even in strict mode.
    ///
    /// ## Variance Modes:
    /// - **Contravariant (strict)**: `target <: source` - Function parameters in strict mode
    /// - **Bivariant (legacy)**: `target <: source OR source <: target` - Methods, legacy functions
    ///
    /// ## Example:
    /// ```typescript
    /// // Bivariant methods allow unsound but convenient assignments
    /// interface Animal { name: string; }
    /// interface Dog extends Animal { bark(): void; }
    /// class AnimalKeeper {
    ///   feed(animal: Animal) { ... }  // Contravariant parameter
    /// }
    /// class DogKeeper {
    ///   feed(dog: Dog) { ... }  // More specific
    /// }
    /// // DogKeeper.feed is assignable to AnimalKeeper.feed (bivariant)
    /// ```
    pub(crate) fn check_subtype_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> SubtypeResult {
        if !allow_bivariant {
            return self.check_subtype(source, target);
        }
        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.check_subtype(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }

    /// Explain failure with method bivariance rules.
    pub(crate) fn explain_failure_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> Option<SubtypeFailureReason> {
        if !allow_bivariant {
            return self.explain_failure(source, target);
        }
        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.explain_failure(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }
}
