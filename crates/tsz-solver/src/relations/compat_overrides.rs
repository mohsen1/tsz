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
    /// If both source and target types have private brands, they must match exactly.
    /// This implements nominal typing for classes with private fields.
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

        // 1. Handle Target Union (OR logic)
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

        // 2. Handle Source Union (AND logic)
        // (A | B) -> T : Valid if A -> T AND B -> T
        if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                if let Some(false) = self.private_brand_assignability_override(member, target) {
                    return Some(false); // Fail if any member fails
                }
            }
            return None; // All passed or fell back
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

        let mut has_private_brands = false;

        // Check Target requirements (Nominality)
        // If Target has a private/protected property, Source MUST match its origin exactly.
        for target_prop in &target_shape.properties {
            if target_prop.visibility == Visibility::Private
                || target_prop.visibility == Visibility::Protected
            {
                has_private_brands = true;
                let source_prop = source_shape
                    .properties
                    .binary_search_by_key(&target_prop.name, |p| p.name)
                    .ok()
                    .map(|idx| &source_shape.properties[idx]);

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
            if source_prop.visibility == Visibility::Private
                || source_prop.visibility == Visibility::Protected
            {
                has_private_brands = true;
                if let Some(target_prop) = target_shape
                    .properties
                    .binary_search_by_key(&source_prop.name, |p| p.name)
                    .ok()
                    .map(|idx| &target_shape.properties[idx])
                    && target_prop.visibility == Visibility::Public
                {
                    return Some(false);
                }
            }
        }

        has_private_brands.then_some(true)
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
            if self.is_literal_enum_member(source) && self.is_literal_enum_member(target) {
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
                            return Some(true);
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

    /// Get the `DefId` of an enum type, handling both direct Enum members and Union-based Enums.
    /// Check whether `type_id` is an enum whose underlying member is a string or number literal.
    fn is_literal_enum_member(&self, type_id: TypeId) -> bool {
        matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Enum(_, member_type))
                if matches!(
                    self.interner.lookup(member_type),
                    Some(TypeData::Literal(LiteralValue::Number(_) | LiteralValue::String(_)))
                )
        )
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
        let a = a_norm;
        let b = b_norm;

        // 5. Structural Identity
        // Delegate to the Judge to check bidirectional subtyping
        let fwd = self.subtype.is_subtype_of(a, b);
        let bwd = self.subtype.is_subtype_of(b, a);
        tracing::trace!(
            a = a.0,
            b = b.0,
            fwd,
            bwd,
            "are_types_identical_for_redeclaration: result"
        );
        fwd && bwd
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
