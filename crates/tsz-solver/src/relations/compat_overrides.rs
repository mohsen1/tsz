//! Nominal typing overrides for the compatibility checker.
//!
//! This module contains assignability override methods that implement
//! TypeScript's nominal typing rules on top of the structural compatibility
//! engine. These overrides handle:
//!
//! - **Enum nominality**: Different enums are not assignable even if structurally
//!   identical. String enums are strictly nominal; numeric enums allow number
//!   assignability.
//! - **Private brand checking**: Classes with private/protected fields use nominal
//!   typing — the `parent_id` on each property must match exactly.
//! - **Redeclaration compatibility**: Variable redeclarations (`var x: T; var x: U`)
//!   require nominal identity for enums and bidirectional structural subtyping for
//!   other types.

use crate::relations::compat::{AssignabilityOverrideProvider, ShapeExtractor, StringLikeVisitor};
use crate::relations::subtype::TypeResolver;
use crate::types::{LiteralValue, TypeData, TypeId};
use crate::visitor::TypeVisitor;

use super::compat::CompatChecker;

// =============================================================================
// Assignability Override Functions
// =============================================================================

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    /// Check if `source` is assignable to `target` using TS compatibility rules,
    /// with checker-provided overrides for enums, abstract constructors, and accessibility.
    ///
    /// This is the main entry point for assignability checking when checker context is available.
    pub fn is_assignable_with_overrides<P: AssignabilityOverrideProvider + ?Sized>(
        &mut self,
        source: TypeId,
        target: TypeId,
        overrides: &P,
    ) -> bool {
        // Check override provider for enum assignability
        if let Some(result) = overrides.enum_assignability_override(source, target) {
            return result;
        }

        // Check override provider for abstract constructor assignability
        if let Some(result) = overrides.abstract_constructor_assignability_override(source, target)
        {
            return result;
        }

        // Check override provider for constructor accessibility
        if let Some(result) = overrides.constructor_accessibility_override(source, target) {
            return result;
        }

        // Check private brand assignability (can be done with TypeDatabase alone)
        if let Some(result) = self.private_brand_assignability_override(source, target) {
            return result;
        }

        // Fall through to regular assignability check
        self.is_assignable(source, target)
    }

    /// Private brand assignability override.
    /// Returns `Some(false)` when nominal private/protected compatibility fails,
    /// otherwise falls through to the structural checker.
    ///
    /// This implements TypeScript's nominal guard for classes with private fields
    /// without bypassing ordinary member-type comparisons after the brands match.
    ///
    /// Uses recursive structure to preserve Union/Intersection semantics:
    /// - Union (A | B): OR logic - must satisfy at least one branch
    /// - Intersection (A & B): AND logic - must satisfy all branches
    pub fn private_brand_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        use crate::types::Visibility;

        // Fast path: identical types don't need nominal brand override logic.
        // Let the regular assignability path decide.
        if source == target {
            return None;
        }

        // 1. Handle Source Union (AND logic) — MUST run before target union.
        // (A | B) -> T : Valid if A -> T AND B -> T
        // When both source and target are unions, decomposing the source first
        // ensures each source member is checked against the ENTIRE target union.
        // If target union were decomposed first, the check would incorrectly
        // require the entire source union to match a single target member.
        if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                if let Some(false) = self.private_brand_assignability_override(member, target) {
                    return Some(false); // Fail if any member fails
                }
            }
            return None; // All passed or fell back
        }

        // 2. Handle Target Union (OR logic)
        // S -> (A | B) : Valid if S -> A OR S -> B
        if let Some(TypeData::Union(members)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members);
            // If source matches ANY target member, it's valid
            for &member in members.iter() {
                match self.private_brand_assignability_override(source, member) {
                    Some(true) | None => return None, // Pass (or structural fallback)
                    Some(false) => {}                 // Keep checking other members
                }
            }
            return Some(false); // Failed against all members
        }

        // 3. Handle Target Intersection (AND logic)
        // S -> (A & B) : Valid if S -> A AND S -> B
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(target) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                if let Some(false) = self.private_brand_assignability_override(source, member) {
                    return Some(false); // Fail if any member fails
                }
            }
            return None; // All passed or fell back
        }

        // 4. Handle Source Intersection (OR logic)
        // (A & B) -> T : Valid if A -> T OR B -> T
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                match self.private_brand_assignability_override(member, target) {
                    Some(true) | None => return None, // Pass (or structural fallback)
                    Some(false) => {}                 // Keep checking other members
                }
            }
            return Some(false); // Failed against all members
        }

        // 5. Handle Lazy types (recursive resolution)
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(source)
            && let Some(resolved) = self.subtype.resolver.resolve_lazy(def_id, self.interner)
        {
            // Guard against non-progressing lazy resolution (e.g. DefId -> same Lazy type),
            // which would otherwise recurse forever.
            if resolved == source {
                return None;
            }
            return self.private_brand_assignability_override(resolved, target);
        }

        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(target)
            && let Some(resolved) = self.subtype.resolver.resolve_lazy(def_id, self.interner)
        {
            // Same non-progress guard for target-side lazy resolution.
            if resolved == target {
                return None;
            }
            return self.private_brand_assignability_override(source, resolved);
        }

        // 6. Base case: Extract and compare object shapes
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);

        // Get source shape
        let source_shape_id = extractor.extract(source)?;
        let source_shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(source_shape_id));

        // Get target shape
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let target_shape_id = extractor.extract(target)?;
        let target_shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(target_shape_id));

        // Check Target requirements (Nominality)
        // If Target has a private/protected property, Source MUST match its origin exactly.
        for target_prop in &target_shape.properties {
            if target_prop.visibility == Visibility::Private
                || target_prop.visibility == Visibility::Protected
            {
                let source_prop = crate::utils::lookup_property(
                    self.interner,
                    &source_shape.properties,
                    Some(crate::types::ObjectShapeId(source_shape_id)),
                    target_prop.name,
                );

                match source_prop {
                    Some(sp) => {
                        // CRITICAL: The parent_id must match exactly.
                        if sp.parent_id != target_prop.parent_id {
                            return Some(false);
                        }
                    }
                    None => {
                        return Some(false);
                    }
                }
            }
        }

        // Check Source restrictions (Visibility leakage)
        // If Source has a private/protected property, it cannot be assigned to a Target
        // that expects it to be Public.
        for source_prop in &source_shape.properties {
            if (source_prop.visibility == Visibility::Private
                || source_prop.visibility == Visibility::Protected)
                && let Some(target_prop) = crate::utils::lookup_property(
                    self.interner,
                    &target_shape.properties,
                    Some(crate::types::ObjectShapeId(target_shape_id)),
                    source_prop.name,
                )
                && target_prop.visibility == Visibility::Public
            {
                return Some(false);
            }
        }

        None
    }

    /// Enum member assignability override.
    /// Implements nominal typing for enum members: EnumA.X is NOT assignable to `EnumB` even if values match.
    ///
    /// TypeScript enum rules:
    /// 1. Different enums with different `DefIds` are NOT assignable (nominal typing)
    /// 2. Numeric enums are bidirectionally assignable to number (Rule #7 - Open Numeric Enums)
    /// 3. String enums are strictly nominal (string literals NOT assignable to string enums)
    /// 4. Same enum members with different values are NOT assignable (EnumA.X != EnumA.Y)
    /// 5. Unions containing enums: Source union assigned to target enum checks all members
    pub fn enum_assignability_override(&self, source: TypeId, target: TypeId) -> Option<bool> {
        use crate::type_queries;
        use crate::visitor;

        // Special case: Source union -> Target enum
        // When assigning a union to an enum, ALL enum members in the union must match the target enum.
        // This handles cases like: (EnumA | EnumB) assigned to EnumC
        // And allows: (Choice.Yes | Choice.No) assigned to Choice (subset of same enum)
        if let Some((t_def, _)) = visitor::enum_components(self.interner, target)
            && type_queries::is_union_type(self.interner, source)
        {
            let union_members = type_queries::get_union_members(self.interner, source)?;

            let mut all_same_enum = true;
            let mut has_non_enum = false;
            for &member in &union_members {
                if let Some((member_def, _)) = visitor::enum_components(self.interner, member) {
                    // Check if this member belongs to the target enum.
                    // Members have their own DefIds (different from parent enum's DefId),
                    // so we must also check the parent relationship.
                    let member_parent = self.subtype.resolver.get_enum_parent_def_id(member_def);
                    if member_def != t_def && member_parent != Some(t_def) {
                        // Found an enum member from a different enum than target
                        return Some(false);
                    }
                } else {
                    all_same_enum = false;
                    has_non_enum = true;
                }
            }

            // If ALL union members are enum members from the same enum as the target,
            // the union is a subset of the enum and therefore assignable.
            // This handles: `type YesNo = Choice.Yes | Choice.No` assignable to `Choice`.
            if all_same_enum && !has_non_enum && !union_members.is_empty() {
                return Some(true);
            }
            // Otherwise fall through to structural check for non-enum union members.
        }

        // String enums are assignable to string (like numeric enums are to number).
        // Fall through to structural checking for this case.

        // Fast path: Check if both are enum types with same DefId but different TypeIds
        // This handles the test case where enum members aren't in the resolver
        if let (Some((s_def, _)), Some((t_def, _))) = (
            visitor::enum_components(self.interner, source),
            visitor::enum_components(self.interner, target),
        ) && s_def == t_def
            && source != target
        {
            // Same enum DefId but different TypeIds
            // Check if both are literal enum members (not union-based enums)
            if crate::type_queries::is_literal_enum_member(self.interner, source)
                && crate::type_queries::is_literal_enum_member(self.interner, target)
            {
                // Both are enum literals with same DefId but different values
                // Nominal rule: E.A is NOT assignable to E.B
                return Some(false);
            }
        }

        let source_def = self.get_enum_def_id(source);
        let target_def = self.get_enum_def_id(target);

        match (source_def, target_def) {
            // Case 1: Both are enums (or enum members or Union-based enums)
            // Note: Same-DefId, different-TypeId case is now handled above before get_enum_def_id
            (Some(s_def), Some(t_def)) => {
                if s_def == t_def {
                    // Same DefId: Same type (E.A -> E.A or E -> E)
                    return Some(true);
                }

                // Gap A: Different DefIds, but might be member -> parent relationship
                // Check if they share a parent enum (e.g., E.A -> E)
                let s_parent = self.subtype.resolver.get_enum_parent_def_id(s_def);
                let t_parent = self.subtype.resolver.get_enum_parent_def_id(t_def);

                match (s_parent, t_parent) {
                    (Some(sp), Some(tp)) if sp == tp => {
                        // Same parent enum
                        // If target is the Enum Type (e.g., 'E'), allow structural check
                        if self.subtype.resolver.is_enum_type(target, self.interner) {
                            return None;
                        }
                        // If target is a different specific member (e.g., 'E.B'), reject nominally
                        // E.A -> E.B should fail even if they have the same value
                        Some(false)
                    }
                    (Some(sp), None) => {
                        // Source is a member, target doesn't have a parent (target is not a member)
                        // Check if target is the parent enum type
                        if t_def == sp {
                            // Target is the parent enum of source member
                            // Allow member to parent enum assignment (E.A -> E)
                            return Some(true);
                        }
                        // Target is an enum type but not the parent
                        Some(false)
                    }
                    _ => {
                        // Different parents (or one/both are types, not members)
                        // Nominal mismatch: EnumA.X is not assignable to EnumB
                        Some(false)
                    }
                }
            }

            // Case 2: Target is an enum, source is a primitive
            (None, Some(t_def)) => {
                // Check if target is a numeric enum
                if self.subtype.resolver.is_numeric_enum(t_def) {
                    // Rule #7: Numeric enums allow number assignability
                    // BUT we need to distinguish between:
                    // - `let x: E = 1` (enum TYPE - allowed)
                    // - `let x: E.A = 1` (enum MEMBER - rejected)

                    // Check if source is number-like (number or number literal)
                    let is_source_number = source == TypeId::NUMBER
                        || matches!(
                            self.interner.lookup(source),
                            Some(TypeData::Literal(LiteralValue::Number(_)))
                        );

                    if is_source_number {
                        // If target is the full Enum Type (e.g., `let x: E = 1`), allow it.
                        if self.subtype.resolver.is_enum_type(target, self.interner) {
                            // Allow bare `number` type but not arbitrary literals
                            if source == TypeId::NUMBER {
                                return Some(true);
                            }
                            // For number literals, fall through to structural check
                            return None;
                        }

                        // If target is a specific member (e.g., `let x: E.A = 1`),
                        // fall through to structural check.
                        // - `1 -> E.A(0)` will fail structural check (Correct)
                        // - `0 -> E.A(0)` will pass structural check (Correct)
                        return None;
                    }

                    None
                } else {
                    // String enums do NOT allow raw string assignability
                    // If source is string or string literal, reject
                    if self.is_string_like(source) {
                        return Some(false);
                    }
                    None
                }
            }

            // Case 3: Source is an enum, target is a primitive
            // String enums (both types and members) are assignable to string via structural checking
            (Some(s_def), None) => {
                // Check if source is a string enum
                if !self.subtype.resolver.is_numeric_enum(s_def) {
                    // Source is a string enum
                    if target == TypeId::STRING {
                        // Both enum types (Union of members) and enum members (string literals)
                        // are assignable to string. Fall through to structural checking.
                        return None;
                    }
                }
                // Numeric enums and non-string targets: fall through to structural check
                None
            }

            // Case 4: Neither is an enum
            (None, None) => None,
        }
    }

    /// Check if a type is string-like (string, string literal, or template literal).
    /// Used to reject primitive-to-string-enum assignments.
    fn is_string_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING {
            return true;
        }
        // Use visitor to check for string literals, template literals, etc.
        let mut visitor = StringLikeVisitor { db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Returns `Some(def_id)` if the type is an Enum or a Union of Enum members from the same enum.
    /// Returns None if the type is not an enum or contains mixed enums.
    fn get_enum_def_id(&self, type_id: TypeId) -> Option<crate::def::DefId> {
        use crate::{type_queries, visitor};

        // Resolve Lazy types first (handles imported/forward-declared enums)
        let resolved =
            if let Some(lazy_def_id) = type_queries::get_lazy_def_id(self.interner, type_id) {
                // Try to resolve the Lazy type
                if let Some(resolved_type) = self
                    .subtype
                    .resolver
                    .resolve_lazy(lazy_def_id, self.interner)
                {
                    // Guard against self-referential lazy types
                    if resolved_type == type_id {
                        return None;
                    }
                    // Recursively check the resolved type
                    return self.get_enum_def_id(resolved_type);
                }
                // Lazy type couldn't be resolved yet, return None
                return None;
            } else {
                type_id
            };

        // 1. Check for Intrinsic Primitives first (using visitor, not TypeId constants)
        // This filters out intrinsic types like string, number, boolean which are stored
        // as TypeData::Enum for definition store purposes but are NOT user enums
        if visitor::intrinsic_kind(self.interner, resolved).is_some() {
            return None;
        }

        // 2. Check direct Enum member
        if let Some((def_id, _inner)) = visitor::enum_components(self.interner, resolved) {
            // Use the new is_user_enum_def method to check if this is a user-defined enum
            // This properly filters out intrinsic types from lib.d.ts
            if self.subtype.resolver.is_user_enum_def(def_id) {
                return Some(def_id);
            }
            // Not a user-defined enum (intrinsic type or type alias)
            return None;
        }

        // 3. Check Union of Enum members (handles Enum types represented as Unions)
        if let Some(members) = visitor::union_list_id(self.interner, resolved) {
            let members = self.interner.type_list(members);
            if members.is_empty() {
                return None;
            }

            let first_def = self.get_enum_def_id(members[0])?;
            for &member in members.iter().skip(1) {
                if self.get_enum_def_id(member) != Some(first_def) {
                    return None; // Mixed union or non-enum members
                }
            }
            return Some(first_def);
        }

        None
    }

    /// Checks if two types are compatible for variable redeclaration (TS2403).
    ///
    /// This applies TypeScript's nominal identity rules for enums and
    /// respects 'any' propagation. Used for checking if multiple variable
    /// declarations have compatible types.
    ///
    /// # Examples
    /// - `var x: number; var x: number` → true
    /// - `var x: E.A; var x: E.A` → true
    /// - `var x: E.A; var x: E.B` → false
    /// - `var x: E; var x: F` → false (different enums)
    /// - `var x: E; var x: number` → false
    pub fn are_types_identical_for_redeclaration(&mut self, a: TypeId, b: TypeId) -> bool {
        // 1. Fast path: physical identity
        if a == b {
            return true;
        }

        // 2. Error propagation — suppress cascading errors from ERROR types.
        if a == TypeId::ERROR || b == TypeId::ERROR {
            return true;
        }

        // For redeclaration, `any` is only identical to `any`.
        // `a == b` already caught the `any == any` case above.
        if a == TypeId::ANY || b == TypeId::ANY {
            return false;
        }

        // 4. Enum Nominality Check
        // If one is an enum and the other isn't, or they are different enums,
        // they are not identical for redeclaration, even if structurally compatible.
        if let Some(res) = self.enum_redeclaration_check(a, b) {
            return res;
        }

        // 5. Normalize Application/Mapped/Lazy types before structural comparison.
        // Required<{a?: string}> must evaluate to {a: string} before bidirectional
        // subtype checking, just as is_assignable_impl() does via normalize_assignability_operands.
        let (a_norm, b_norm) = self.normalize_assignability_operands(a, b);
        tracing::trace!(
            a = a.0,
            b = b.0,
            a_norm = a_norm.0,
            b_norm = b_norm.0,
            a_changed = a != a_norm,
            b_changed = b != b_norm,
            "are_types_identical_for_redeclaration: normalized"
        );

        // Re-check `any` identity after normalization. Homomorphic mapped types
        // applied to `any` evaluate to `any` (e.g., `FindConditions<any>` → `any`).
        // The pre-normalization check at line 507 only catches literal `any` inputs;
        // if normalization produces `any`, the bidirectional subtype check below
        // would treat it as top/bottom at depth 0 (TopLevelOnly mode), causing a
        // false positive. Re-applying the rule here ensures that a post-evaluation
        // `any` is only identical to `any`.
        if a_norm == b_norm {
            return true;
        }
        if a_norm == TypeId::ANY || b_norm == TypeId::ANY {
            return false;
        }

        // 5 pre-check: Callable signature type parameter identity.
        //
        // The bidirectional subtype check below uses coinductive cycle detection,
        // which assumes recursive type pairs are related. This causes false positives
        // for generic interfaces with different type parameter constraints:
        //   IPromise<string, number> vs Promise<string, boolean>
        // After constraint erasure + cycle detection, the subtype checker concludes
        // these are mutual subtypes even though the constraints differ.
        //
        // tsc's isTypeIdenticalTo requires exact type parameter constraint matching
        // for signature identity. Pre-check this before the bidirectional subtype
        // to catch constraint mismatches that the subtype checker misses.
        if !self.callable_signatures_have_identical_type_params(a_norm, b_norm) {
            return false;
        }
        let a = a_norm;
        let b = b_norm;

        // 5a. DNF normalization for intersection-of-unions.
        //
        // TypeScript's `isTypeIdenticalTo` normalizes intersection types to their
        // Disjunctive Normal Form (DNF) before comparison. This means that:
        //   type X1 = (A | B) & (C | D)   -- all-union intersection
        //   type X2 = A & C | A & D | B & C | B & D  -- DNF union
        // are considered identical for TS2403 purposes, even though they have
        // different TypeIds in our interner (we preserve the written form).
        //
        // To match tsc, we distribute any all-union intersection to its DNF form
        // before the structural identity check. Non-all-union intersections
        // (e.g., `A & (B | C)`) are already normalized during interning via
        // `distribute_intersection_over_unions` with the `has_non_union` guard.
        let a = self.normalize_all_union_intersection_to_dnf(a);
        let b = self.normalize_all_union_intersection_to_dnf(b);

        // 5b. Structural Identity
        // tsc uses isTypeIdenticalTo for redeclaration checking, which is stricter
        // than bidirectional subtyping. In particular, a non-union type is NEVER
        // identical to a union type, even if they are bidirectionally related.
        // E.g., `C` is NOT identical to `C | D` even when `D extends C`.
        // PERF: 2 lookups instead of 4
        let a_key = self.interner.lookup(a);
        let b_key = self.interner.lookup(b);
        let a_is_union = matches!(a_key, Some(TypeData::Union(_)));
        let b_is_union = matches!(b_key, Some(TypeData::Union(_)));
        let a_is_intersection = matches!(a_key, Some(TypeData::Intersection(_)));
        let b_is_intersection = matches!(b_key, Some(TypeData::Intersection(_)));
        // When one side is an intersection, skip the union/non-union identity mismatch
        // because intersections of unions can distribute to unions of intersections
        // (e.g., `(A|B) & (C|D)` ≡ `A&C | A&D | B&C | B&D`)
        if a_is_union != b_is_union && !a_is_intersection && !b_is_intersection {
            return false;
        }

        // For two union types, first try fast TypeId-level identity, then fall through
        // to bidirectional subtype for cases like intersection distribution where
        // `(A | B) & (C | D)` and `A & C | A & D | B & C | B & D` are structurally
        // equivalent but have different member TypeIds.
        if a_is_union
            && b_is_union
            && let (Some(TypeData::Union(a_list)), Some(TypeData::Union(b_list))) =
                (self.interner.lookup(a), self.interner.lookup(b))
        {
            let a_members = self.interner.type_list(a_list);
            let b_members = self.interner.type_list(b_list);
            if a_members.len() == b_members.len()
                && a_members
                    .iter()
                    .zip(b_members.iter())
                    .all(|(a_m, b_m)| a_m == b_m)
            {
                return true;
            }
            // Fast identity failed — fall through to bidirectional subtype
        }

        // For both union and non-union types, delegate to bidirectional subtyping.
        // This handles intersection distribution, typeof resolution, and other
        // structural equivalences that TypeId-level identity misses.
        //
        // CRITICAL: Use TopLevelOnly any propagation for identity checking.
        // tsc's isTypeIdenticalTo treats `any` as only identical to `any` — it does
        // NOT use any-propagation rules. Without this, types like `Promise<any, any>`
        // appear "identical" to `IPromise<U, W>` because nested `any` matches
        // everything in bidirectional subtype mode, producing false negatives for TS2403.
        //
        // Also enable identity_cycle_check: when DefId-level cycle detection fires
        // for recursive generic interfaces, compare Application type arguments before
        // assuming related. Without this, `IPromise2<W, U>` vs `Promise2<any, W>` at
        // a cycle point would be assumed related (because same DefId pair), even though
        // the type arguments [W, U] vs [any, W] are NOT identical.
        let saved_any_mode = self.subtype.any_propagation;
        let saved_identity_cycle = self.subtype.identity_cycle_check;
        let saved_method_bivariance = self.subtype.disable_method_bivariance;
        let saved_strict_fn = self.subtype.strict_function_types;
        self.subtype.any_propagation =
            crate::relations::subtype::core::AnyPropagationMode::TopLevelOnly;
        self.subtype.identity_cycle_check = true;
        // TS2403 identity checking mirrors tsc's `isTypeIdenticalTo` which uses
        // the `identity` relation — strictly bidirectional structural equality.
        // Unlike the subtype relation, identity does NOT have bivariance at all:
        // - No method bivariance (methods must be strictly compatible)
        // - No function bivariance (strictFunctionTypes behavior regardless of flag)
        // Without this, recursive method types can appear identical through a
        // bivariant path that hits a cycle (CycleDetected = True) even when the
        // forward structural check correctly rejects the types.
        self.subtype.disable_method_bivariance = true;
        self.subtype.strict_function_types = true;
        let fwd = self.subtype.is_subtype_of(a, b);
        let bwd = self.subtype.is_subtype_of(b, a);
        self.subtype.any_propagation = saved_any_mode;
        self.subtype.identity_cycle_check = saved_identity_cycle;
        self.subtype.disable_method_bivariance = saved_method_bivariance;
        self.subtype.strict_function_types = saved_strict_fn;
        tracing::trace!(
            a = a.0,
            b = b.0,
            fwd,
            bwd,
            "are_types_identical_for_redeclaration: result"
        );
        fwd && bwd
    }

    /// Check that callable signatures in corresponding properties of two object types
    /// have identical type parameter structures (arity and constraints).
    ///
    /// tsc's `isTypeIdenticalTo` requires exact type parameter constraint matching
    /// for callable/function identity. Our bidirectional subtype check can miss this
    /// because the subtype checker's generic signature comparison uses constraint
    /// erasure fallbacks and coinductive cycle detection, which together can make
    /// `IPromise<string, number>` appear identical to `Promise<string, boolean>`
    /// even though the `then` method's type parameter constraints differ.
    ///
    /// Returns `true` if all corresponding callable signatures have identical type
    /// parameter structures, or if the types are not Object types (in which case
    /// we defer to the bidirectional subtype check).
    fn callable_signatures_have_identical_type_params(&mut self, a: TypeId, b: TypeId) -> bool {
        // Collect constraint pairs from type parameter lists.
        // Applies to Object types (properties with callable signatures),
        // Callable types (direct call signatures), and Function types.
        let constraint_pairs = match (self.interner.lookup(a), self.interner.lookup(b)) {
            (
                Some(TypeData::Object(a_id) | TypeData::ObjectWithIndex(a_id)),
                Some(TypeData::Object(b_id) | TypeData::ObjectWithIndex(b_id)),
            ) => Self::collect_constraint_pairs(self.interner, a_id, b_id),
            (Some(TypeData::Callable(a_cid)), Some(TypeData::Callable(b_cid))) => {
                Self::collect_callable_constraint_pairs(self.interner, a_cid, b_cid)
            }
            (Some(TypeData::Function(a_fid)), Some(TypeData::Function(b_fid))) => {
                Self::collect_function_constraint_pairs(self.interner, a_fid, b_fid)
            }
            _ => return true,
        };

        // Check collected constraints via subtype relation.
        for (a_constraint, b_constraint) in constraint_pairs {
            if a_constraint != b_constraint {
                let fwd = self.subtype.is_subtype_of(a_constraint, b_constraint);
                let bwd = self.subtype.is_subtype_of(b_constraint, a_constraint);
                if !(fwd && bwd) {
                    return false;
                }
            }
        }

        true
    }

    /// Collect type parameter constraint pairs from matching callable signatures
    /// on two object shapes' properties. Returns `Err(())` on arity mismatch,
    /// or `Ok(pairs)` with constraint pairs to check.
    ///
    /// Separated from `callable_signatures_have_identical_type_params` to avoid
    /// borrow conflicts: this borrows `interner` (immutable) while the caller
    /// needs `self.subtype` (mutable) for constraint comparison.
    fn collect_constraint_pairs(
        interner: &dyn crate::TypeDatabase,
        a_shape_id: crate::types::ObjectShapeId,
        b_shape_id: crate::types::ObjectShapeId,
    ) -> Vec<(TypeId, TypeId)> {
        let a_shape = interner.object_shape(a_shape_id);
        let b_shape = interner.object_shape(b_shape_id);
        let mut pairs = Vec::new();

        for a_prop in &a_shape.properties {
            let b_prop = match b_shape.properties.iter().find(|p| p.name == a_prop.name) {
                Some(p) => p,
                None => continue,
            };

            let (a_callable, b_callable) = match (
                interner.lookup(a_prop.type_id),
                interner.lookup(b_prop.type_id),
            ) {
                (Some(TypeData::Callable(a_cid)), Some(TypeData::Callable(b_cid))) => (
                    interner.callable_shape(a_cid),
                    interner.callable_shape(b_cid),
                ),
                _ => continue,
            };

            // Check both call and construct signatures, using chained iterators
            // to avoid duplicating the loop body.
            let all_sig_pairs = a_callable
                .call_signatures
                .iter()
                .zip(b_callable.call_signatures.iter())
                .chain(
                    a_callable
                        .construct_signatures
                        .iter()
                        .zip(b_callable.construct_signatures.iter()),
                );

            for (a_sig, b_sig) in all_sig_pairs {
                if a_sig.type_params.len() != b_sig.type_params.len() {
                    // Arity mismatch — signal via a sentinel pair that will always fail.
                    // TypeId::NEVER is not a subtype of TypeId::STRING and vice versa.
                    pairs.push((TypeId::NEVER, TypeId::STRING));
                    continue;
                }
                // Unconstrained type params default to UNKNOWN so two unconstrained
                // params compare as identical (UNKNOWN == UNKNOWN → skip).
                for (a_tp, b_tp) in a_sig.type_params.iter().zip(b_sig.type_params.iter()) {
                    pairs.push((
                        a_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                        b_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                    ));
                }
            }

            // Different signature counts mean the callable shapes differ structurally.
            if a_callable.call_signatures.len() != b_callable.call_signatures.len()
                || a_callable.construct_signatures.len() != b_callable.construct_signatures.len()
            {
                pairs.push((TypeId::NEVER, TypeId::STRING));
            }
        }

        pairs
    }

    /// Collect type parameter constraint pairs from two Callable types' signatures.
    fn collect_callable_constraint_pairs(
        interner: &dyn crate::TypeDatabase,
        a_cid: crate::types::CallableShapeId,
        b_cid: crate::types::CallableShapeId,
    ) -> Vec<(TypeId, TypeId)> {
        let a_callable = interner.callable_shape(a_cid);
        let b_callable = interner.callable_shape(b_cid);
        let mut pairs = Vec::new();

        let all_sig_pairs = a_callable
            .call_signatures
            .iter()
            .zip(b_callable.call_signatures.iter())
            .chain(
                a_callable
                    .construct_signatures
                    .iter()
                    .zip(b_callable.construct_signatures.iter()),
            );

        for (a_sig, b_sig) in all_sig_pairs {
            if a_sig.type_params.len() != b_sig.type_params.len() {
                pairs.push((TypeId::NEVER, TypeId::STRING));
                continue;
            }
            for (a_tp, b_tp) in a_sig.type_params.iter().zip(b_sig.type_params.iter()) {
                pairs.push((
                    a_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                    b_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                ));
            }
        }

        if a_callable.call_signatures.len() != b_callable.call_signatures.len()
            || a_callable.construct_signatures.len() != b_callable.construct_signatures.len()
        {
            pairs.push((TypeId::NEVER, TypeId::STRING));
        }

        pairs
    }

    /// Collect type parameter constraint pairs from two Function types.
    fn collect_function_constraint_pairs(
        interner: &dyn crate::TypeDatabase,
        a_fid: crate::types::FunctionShapeId,
        b_fid: crate::types::FunctionShapeId,
    ) -> Vec<(TypeId, TypeId)> {
        let a_fn = interner.function_shape(a_fid);
        let b_fn = interner.function_shape(b_fid);
        let mut pairs = Vec::new();

        if a_fn.type_params.len() != b_fn.type_params.len() {
            pairs.push((TypeId::NEVER, TypeId::STRING));
        } else {
            for (a_tp, b_tp) in a_fn.type_params.iter().zip(b_fn.type_params.iter()) {
                pairs.push((
                    a_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                    b_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                ));
            }
        }

        pairs
    }

    /// Normalize an intersection-of-all-unions to its Disjunctive Normal Form (DNF).
    ///
    /// TypeScript's `isTypeIdenticalTo` normalizes `(A|B)&(C|D)` to
    /// `(A&C)|(A&D)|(B&C)|(B&D)` before comparing types for TS2403 identity.
    /// Our interner preserves the written form (all-union intersections are NOT
    /// distributed during interning, to avoid changing diagnostic messages).
    /// This function distributes on-demand for identity comparison only.
    ///
    /// Returns the original type unchanged if it is not an all-union intersection
    /// or if distribution would produce too many combinations (>25).
    fn normalize_all_union_intersection_to_dnf(&self, ty: TypeId) -> TypeId {
        // Only applies to intersection types
        let Some(TypeData::Intersection(members_id)) = self.interner.lookup(ty) else {
            return ty;
        };
        let members = self.interner.type_list(members_id);
        // Only applies when ALL members are union types (all-union intersection)
        let all_are_unions = members
            .iter()
            .all(|&id| matches!(self.interner.lookup(id), Some(TypeData::Union(_))));
        if !all_are_unions {
            return ty;
        }
        // Collect member TypeIds (each is a union type)
        let union_types: Vec<TypeId> = members.iter().copied().collect();
        // Guard: abort if total combinations would exceed 25
        let mut total_combinations = 1usize;
        for &ut in &union_types {
            if let Some(TypeData::Union(ul)) = self.interner.lookup(ut) {
                total_combinations =
                    total_combinations.saturating_mul(self.interner.type_list(ul).len());
                if total_combinations > 25 {
                    return ty;
                }
            }
        }
        // Build combinations: cartesian product of all union members
        let mut combinations: Vec<Vec<TypeId>> = vec![vec![]];
        for &ut in &union_types {
            let Some(TypeData::Union(ul)) = self.interner.lookup(ut) else {
                return ty;
            };
            let union_members = self.interner.type_list(ul);
            let mut new_combinations = Vec::with_capacity(combinations.len() * union_members.len());
            for combination in &combinations {
                for &um in union_members.iter() {
                    let mut new_combo = combination.clone();
                    new_combo.push(um);
                    new_combinations.push(new_combo);
                }
            }
            combinations = new_combinations;
        }
        // Convert each combination to an intersection TypeId and union them (DNF)
        let dnf_members: Vec<TypeId> = combinations
            .into_iter()
            .map(|combo| self.interner.intersection(combo))
            .collect();
        self.interner.union(dnf_members)
    }

    /// Check if two types involving enums are compatible for redeclaration.
    ///
    /// Returns Some(bool) if either type is an enum:
    /// - Some(false) if different enums or enum vs primitive
    /// - None if neither is an enum (delegate to structural check)
    fn enum_redeclaration_check(&self, a: TypeId, b: TypeId) -> Option<bool> {
        let a_def = self.get_enum_def_id(a);
        let b_def = self.get_enum_def_id(b);

        match (a_def, b_def) {
            (Some(def_a), Some(def_b)) => {
                // Both are enums: must be the same enum definition
                if def_a != def_b {
                    Some(false)
                } else {
                    // Same enum DefId: compatible for redeclaration
                    // This allows: var x: MyEnum; var x = MyEnum.Member;
                    // where MyEnum.Member (enum member) is compatible with MyEnum (enum type)
                    Some(true)
                }
            }
            (Some(_), None) | (None, Some(_)) => {
                // One is an enum, the other is a primitive (e.g., number)
                // In TS, Enum E and 'number' are NOT identical for redeclaration
                Some(false)
            }
            (None, None) => None,
        }
    }
}
