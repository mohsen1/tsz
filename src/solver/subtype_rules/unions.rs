//! Union and intersection type subtype checking.
//!
//! This module handles subtyping for TypeScript's composite types:
//! - Union types (A | B | C) - source must be subtype of at least one member
//! - Intersection types (A & B & C) - source must be subtype of all members
//! - Distributivity rules between unions and intersections
//! - Type parameter compatibility in union/intersection contexts

use crate::solver::types::*;

use super::super::{SubtypeChecker, SubtypeFailureReason, SubtypeResult, TypeResolver};

#[allow(dead_code)]
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
    ///
    /// ## Special Case - Object with All Optional Properties:
    /// When target is an object with only optional properties, we use a relaxed rule:
    /// `{a: A1} | {b: B1} <: {a?: A2, b?: B2}` if each union member satisfies the properties it has.
    /// This allows union literal widening where different union members contribute different properties.
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

        // Special handling for object targets with all optional properties
        // This enables union literal widening: {a: 'x'} | {b: 'y'} <: {a?: string, b?: string}
        if let TypeKey::Object(t_shape_id) | TypeKey::ObjectWithIndex(t_shape_id) = target_key {
            let t_shape = self.interner.object_shape(*t_shape_id);

            // Check if all target properties are optional
            let all_optional = t_shape.properties.iter().all(|p| p.optional);
            let has_index = t_shape.string_index.is_some() || t_shape.number_index.is_some();

            if all_optional && !has_index && !t_shape.properties.is_empty() {
                // Use relaxed union-to-object checking for optional property targets
                return self.check_union_to_all_optional_object(members, &t_shape.properties);
            }
        }

        let members = self.interner.type_list(members);
        for &member in members.iter() {
            // Any poisoning: if any member is `any`, the whole union acts as `any`
            // and is assignable to everything (TypeScript unsoundness rule)
            if member == TypeId::ANY {
                return SubtypeResult::True;
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
            // Optimization: For literal sources, check if the primitive type is in the union
            // This helps reduce false positives when a literal should match a union containing its primitive
            if let TypeKey::Literal(literal) = source_key {
                let primitive_type = match literal {
                    LiteralValue::String(_) => TypeId::STRING,
                    LiteralValue::Number(_) => TypeId::NUMBER,
                    LiteralValue::BigInt(_) => TypeId::BIGINT,
                    LiteralValue::Boolean(_) => TypeId::BOOLEAN,
                };
                // Fast path: exact primitive match
                if member == primitive_type {
                    return SubtypeResult::True;
                }
                // For literal-to-literal unions (e.g., "a" <: "a" | "b"), check if the literal
                // is directly in the union. This is more precise than the full subtype check.
                if matches!(self.interner.lookup(member), Some(TypeKey::Literal(_))) {
                    if self.check_subtype(source, member).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }
            // Optimization: For union sources, check if any member matches directly
            // This can help reduce false positives when checking (A | B) <: (A | B | C)
            if let TypeKey::Union(source_members) = source_key {
                let source_members_list = self.interner.type_list(*source_members);
                if source_members_list.contains(&member) {
                    return SubtypeResult::True;
                }
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

    /// Check if a union type is assignable to an object type with all optional properties.
    ///
    /// This implements the relaxed rule for union literal widening:
    /// `{a: 'x'} | {b: 'y'} <: {a?: string, b?: string}`
    ///
    /// Each union member must satisfy the properties it has (if any).
    /// Properties not present in a union member are satisfied by the target's optional nature.
    pub(crate) fn check_union_to_all_optional_object(
        &mut self,
        union_members: TypeListId,
        target_props: &[crate::solver::types::PropertyInfo],
    ) -> SubtypeResult {
        use crate::solver::types::TypeKey;

        let union_members = self.interner.type_list(union_members);

        // Each union member must satisfy the properties it has
        for &union_member in union_members.iter() {
            let union_key = match self.interner.lookup(union_member) {
                Some(key) => key,
                None => return SubtypeResult::False,
            };

            // Get properties from the union member
            let union_props = match union_key {
                TypeKey::Object(shape_id) => {
                    let shape = self.interner.object_shape(shape_id);
                    shape.properties.clone()
                }
                TypeKey::ObjectWithIndex(shape_id) => {
                    let shape = self.interner.object_shape(shape_id);
                    shape.properties.clone()
                }
                // For non-object types, use the normal check
                _ => {
                    let target_type = self.interner.object(target_props.to_vec());
                    return self.check_subtype(union_member, target_type);
                }
            };

            // Check each property in the union member
            for union_prop in &union_props {
                // Find if there's a corresponding target property
                let t_prop = match target_props.iter().find(|p| p.name == union_prop.name) {
                    Some(p) => p,
                    None => continue, // Union member has extra property - that's OK
                };

                // NOTE: TypeScript allows readonly source to satisfy mutable target
                // (readonly is a constraint on the reference, not structural compatibility)

                // Check if the union member's property is compatible with the target's property
                // Get the effective type (adding undefined for optional properties if needed)
                let source_type = if union_prop.optional && !self.exact_optional_property_types {
                    self.interner.union2(union_prop.type_id, TypeId::UNDEFINED)
                } else {
                    union_prop.type_id
                };

                let target_type = if t_prop.optional && !self.exact_optional_property_types {
                    self.interner.union2(t_prop.type_id, TypeId::UNDEFINED)
                } else {
                    t_prop.type_id
                };

                let allow_bivariant = union_prop.is_method || t_prop.is_method;

                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    return SubtypeResult::False;
                }

                // Check write type compatibility
                if !t_prop.readonly
                    && (union_prop.write_type != union_prop.type_id
                        || t_prop.write_type != t_prop.type_id)
                {
                    let source_write = if union_prop.optional && !self.exact_optional_property_types
                    {
                        self.interner
                            .union2(union_prop.write_type, TypeId::UNDEFINED)
                    } else {
                        union_prop.write_type
                    };

                    let target_write = if t_prop.optional && !self.exact_optional_property_types {
                        self.interner.union2(t_prop.write_type, TypeId::UNDEFINED)
                    } else {
                        t_prop.write_type
                    };

                    if !self
                        .check_subtype_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
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
                // (handles transitive constraints like T extends U, U extends V, checking T <: V)
                if self.check_subtype(s_constraint, target).is_true() {
                    return SubtypeResult::True;
                }
            }
            // Two different type parameters are NOT interchangeable even if they have
            // the same or compatible constraints. T extends string and U extends string
            // are distinct types - only identity or explicit constraint relationships
            // can make them subtypes.
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

        // If we're already in bivariant mode, don't nest - just check normally
        // This prevents infinite recursion when methods contain other methods
        if !self.strict_function_types && self.allow_bivariant_param_count {
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

        // If we're already in bivariant mode, don't nest - just check normally
        // This prevents infinite recursion when methods contain other methods
        if !self.strict_function_types && self.allow_bivariant_param_count {
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
