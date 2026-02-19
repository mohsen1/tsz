//! Type overlap detection for subtype checking.
//!
//! This module implements overlap detection between types, used for TS2367
//! ("This condition will always return 'false' since the types 'X' and 'Y' have no overlap").

use crate::subtype::SubtypeChecker;
use crate::type_resolver::TypeResolver;
use crate::types::{IntrinsicKind, LiteralValue, TemplateLiteralId, TemplateSpan, TypeId};
use crate::visitor::{intrinsic_kind, literal_value, template_literal_id};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if two types have any overlap (non-empty intersection).
    ///
    /// This is used for TS2367: "This condition will always return 'false' since the types 'X' and 'Y' have no overlap."
    ///
    /// Returns true if there exists at least one type that is a subtype of both a and b.
    /// Returns false if a & b would be the `never` type (zero overlap).
    ///
    /// # MVP Implementation (Phase 1)
    ///
    /// This catches OBVIOUS non-overlaps:
    /// - Different primitives (string vs number, boolean vs bigint, etc.)
    /// - Different literals of same primitive ("a" vs "b", 1 vs 2)
    /// - Object property type mismatches ({ a: string } vs { a: number })
    ///
    /// For complex types (unions, intersections, generics), we conservatively return true
    /// to avoid false positives. Phase 2 will add more sophisticated overlap detection.
    ///
    /// # Examples
    /// - `are_types_overlapping(string, number)` -> false (different primitives)
    /// - `are_types_overlapping(1, 2)` -> false (different number literals)
    /// - `are_types_overlapping({ a: string }, { a: number })` -> false (property type mismatch)
    /// - `are_types_overlapping({ a: 1 }, { b: 2 })` -> true (can have { a: 1, b: 2 })
    /// - `are_types_overlapping(string, "hello")` -> true (literal is subtype of primitive)
    pub fn are_types_overlapping(&self, a: TypeId, b: TypeId) -> bool {
        // Fast path: identical types overlap (unless never)
        if a == b {
            return a != TypeId::NEVER;
        }

        // Top types: any/unknown overlap with everything except never
        if a.is_any_or_unknown() {
            return !b.is_never();
        }
        if b.is_any_or_unknown() {
            return !a.is_never();
        }

        // Bottom type: never overlaps with nothing
        if a == TypeId::NEVER || b == TypeId::NEVER {
            return false;
        }

        // Resolve Lazy/Ref types before checking
        let a_resolved = self.resolve_ref_type(a);
        let b_resolved = self.resolve_ref_type(b);

        // Check if either is subtype of the other (sufficient condition, not necessary)
        // This catches: literal <: primitive, object <: interface, etc.
        // Note: check_subtype returns SubtypeResult, but we need &mut self for it
        // For now, we'll use a simpler approach that doesn't require mutation
        if self.are_types_in_subtype_relation(a_resolved, b_resolved) {
            return true;
        }

        // Check for different primitive types
        if let (Some(a_kind), Some(b_kind)) = (
            intrinsic_kind(self.interner, a_resolved),
            intrinsic_kind(self.interner, b_resolved),
        ) {
            // 1. Handle strictNullChecks
            if !self.strict_null_checks {
                // If strict null checks is OFF, null/undefined overlap with everything
                if matches!(a_kind, IntrinsicKind::Null | IntrinsicKind::Undefined)
                    || matches!(b_kind, IntrinsicKind::Null | IntrinsicKind::Undefined)
                {
                    return true;
                }
            }

            // 2. Handle Void vs Undefined (always overlap)
            if (a_kind == IntrinsicKind::Void && b_kind == IntrinsicKind::Undefined)
                || (a_kind == IntrinsicKind::Undefined && b_kind == IntrinsicKind::Void)
            {
                return true;
            }

            // 3. Handle Null/Undefined comparisons (always allowed for TS2367 purposes)
            // TypeScript allows null/undefined to be compared with ANY type without TS2367.
            // This is true even with strict null checks enabled.
            // Examples that should NOT emit TS2367:
            //   - null !== undefined
            //   - null == 5
            //   - "hello" === undefined
            // TS2367 is only for truly incompatible types like "hello" === 5 or 1 === "2".
            if matches!(a_kind, IntrinsicKind::Null | IntrinsicKind::Undefined)
                || matches!(b_kind, IntrinsicKind::Null | IntrinsicKind::Undefined)
            {
                return true;
            }

            // 4. Compare primitives
            match (a_kind, b_kind) {
                (IntrinsicKind::String, IntrinsicKind::String)
                | (IntrinsicKind::Number, IntrinsicKind::Number)
                | (IntrinsicKind::Boolean, IntrinsicKind::Boolean)
                | (IntrinsicKind::Bigint, IntrinsicKind::Bigint)
                | (IntrinsicKind::Symbol, IntrinsicKind::Symbol) => {
                    // Same primitive type - check if they're different literals
                    return self.are_literals_overlapping(a_resolved, b_resolved);
                }
                // Distinct primitives do not overlap
                (
                    IntrinsicKind::String
                    | IntrinsicKind::Number
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol
                    | IntrinsicKind::Null
                    | IntrinsicKind::Undefined
                    | IntrinsicKind::Void
                    | IntrinsicKind::Object,
                    _,
                ) => {
                    return false;
                }
                // Handle Object keyword vs Primitives (Disjoint)
                // Note: It DOES overlap with Object (interface), but that is handled
                // by object_shape_id, not intrinsic_kind.
                // Fallback for any new intrinsics added later
                _ => return true,
            }
        }

        // Check for different literal values of the same primitive type
        if let (Some(a_lit), Some(b_lit)) = (
            literal_value(self.interner, a_resolved),
            literal_value(self.interner, b_resolved),
        ) {
            // Different literal values never overlap
            return a_lit == b_lit;
        }

        // For object-like types, use refined overlap detection with PropertyCollector
        // This handles: objects, objects with index signatures, and intersections
        // This replaces the simplified check that only handled direct object-to-object
        let is_a_obj = self.is_object_like(a_resolved);
        let is_b_obj = self.is_object_like(b_resolved);

        if is_a_obj && is_b_obj {
            return self.do_refined_object_overlap_check(a_resolved, b_resolved);
        }

        // Template literal disjointness detection
        // Two template literals with different starting/ending text are disjoint
        if let (Some(a_spans), Some(b_spans)) = (
            template_literal_id(self.interner, a_resolved),
            template_literal_id(self.interner, b_resolved),
        ) {
            return self.are_template_literals_overlapping(a_spans, b_spans);
        }

        // Conservative: assume overlap for complex types we haven't fully handled yet
        // (unions, intersections, generics, etc.)
        // Better to miss some TS2367 errors than to emit them incorrectly
        true
    }

    /// Check if one type is a subtype of the other without mutation.
    ///
    /// This is a simplified version that checks obvious subtype relationships
    /// without needing to call the full `check_subtype` which requires &mut self.
    fn are_types_in_subtype_relation(&self, a: TypeId, b: TypeId) -> bool {
        // Check identity first
        if a == b {
            return true;
        }

        // Check for literal-to-primitive relationships
        if let (Some(a_lit), Some(b_kind)) = (
            literal_value(self.interner, a),
            intrinsic_kind(self.interner, b),
        ) {
            return matches!(
                (a_lit, b_kind),
                (LiteralValue::String(_), IntrinsicKind::String)
                    | (LiteralValue::Number(_), IntrinsicKind::Number)
                    | (LiteralValue::BigInt(_), IntrinsicKind::Bigint)
                    | (LiteralValue::Boolean(_), IntrinsicKind::Boolean)
            );
        }

        if let (Some(a_kind), Some(b_lit)) = (
            intrinsic_kind(self.interner, a),
            literal_value(self.interner, b),
        ) {
            return matches!(
                (a_kind, b_lit),
                (IntrinsicKind::String, LiteralValue::String(_))
                    | (IntrinsicKind::Number, LiteralValue::Number(_))
                    | (IntrinsicKind::Bigint, LiteralValue::BigInt(_))
                    | (IntrinsicKind::Boolean, LiteralValue::Boolean(_))
            );
        }

        false
    }

    /// Check if two literal types have overlapping values.
    ///
    /// Returns false if they're different literals of the same primitive type.
    /// Returns true if they're the same literal or if we can't determine.
    fn are_literals_overlapping(&self, a: TypeId, b: TypeId) -> bool {
        if let (Some(a_lit), Some(b_lit)) = (
            literal_value(self.interner, a),
            literal_value(self.interner, b),
        ) {
            // Different literal values of the same primitive type never overlap
            a_lit == b_lit
        } else {
            // At least one isn't a literal, so they overlap
            true
        }
    }

    /// Check if two template literal types have any overlap.
    ///
    /// Template literals are disjoint if they have incompatible fixed text spans.
    /// For example:
    /// - `foo${string}` and `bar${string}` are disjoint (different prefixes)
    /// - `foo${string}` and `foo${number}` may overlap (same prefix, compatible types)
    /// - `a${string}b` and `a${string}c` are disjoint (different suffixes)
    ///
    /// Returns false if types are guaranteed disjoint, true otherwise.
    fn are_template_literals_overlapping(
        &self,
        a: TemplateLiteralId,
        b: TemplateLiteralId,
    ) -> bool {
        // Fast path: same template literal definitely overlaps
        if a == b {
            return true;
        }

        let a_spans = self.interner.template_list(a);
        let b_spans = self.interner.template_list(b);

        // Templates with different numbers of spans might still overlap
        // if the type holes are wide enough (e.g., string)
        // We need to check if there's any possible string that matches both patterns

        // For simplicity, we check if there are incompatible fixed text spans
        let a_len = a_spans.len();
        let b_len = b_spans.len();

        // Collect fixed text patterns from both templates
        // Two templates are disjoint if they have incompatible fixed text at any position
        let mut a_idx = 0;
        let mut b_idx = 0;

        loop {
            // Skip type holes in both templates
            while a_idx < a_len && matches!(a_spans[a_idx], TemplateSpan::Type(_)) {
                a_idx += 1;
            }
            while b_idx < b_len && matches!(b_spans[b_idx], TemplateSpan::Type(_)) {
                b_idx += 1;
            }

            // If both reached the end, they overlap (both can match empty string after all type holes)
            if a_idx >= a_len && b_idx >= b_len {
                return true;
            }

            // If only one reached the end, check if the remaining can be empty
            if a_idx >= a_len {
                // A exhausted, B has more content
                // They overlap only if B's remaining content is all type holes
                return b_spans[b_idx..]
                    .iter()
                    .all(|s| matches!(s, TemplateSpan::Type(_)));
            }
            if b_idx >= b_len {
                // B exhausted, A has more content
                return a_spans[a_idx..]
                    .iter()
                    .all(|s| matches!(s, TemplateSpan::Type(_)));
            }

            // Both have text spans - check if they match
            match (&a_spans[a_idx], &b_spans[b_idx]) {
                (TemplateSpan::Text(a_text), TemplateSpan::Text(b_text)) => {
                    let a_str = self.interner.resolve_atom(*a_text);
                    let b_str = self.interner.resolve_atom(*b_text);

                    // Check if the text spans can match
                    // They must have at least one common prefix
                    let min_len = a_str.len().min(b_str.len());
                    if a_str[..min_len] != b_str[..min_len] {
                        // Incompatible prefixes - templates are disjoint
                        return false;
                    }

                    // Advance past the common prefix
                    let advance = min_len;
                    a_idx += 1;
                    b_idx += 1;

                    // If one text span is exhausted, the other must have type holes to continue
                    if a_str.len() > advance {
                        // A's text is longer - B needs a type hole to consume the rest
                        if b_idx >= b_len || !matches!(b_spans[b_idx], TemplateSpan::Type(_)) {
                            // B can't consume the rest of A's text - disjoint unless A's extra text is a prefix
                            // that B's type hole can match
                            return a_str[advance..].is_empty();
                        }
                    }
                    if b_str.len() > advance {
                        // B's text is longer - A needs a type hole to consume the rest
                        if a_idx >= a_len || !matches!(a_spans[a_idx], TemplateSpan::Type(_)) {
                            return b_str[advance..].is_empty();
                        }
                    }
                }
                _ => {
                    // One is text, one is type - they're compatible
                    // The type can match any string, so we advance both
                    a_idx += 1;
                    b_idx += 1;
                }
            }
        }
    }

    /// Check if two types are "object-like" (should use `PropertyCollector` for overlap detection).
    ///
    /// Object-like types include:
    /// - Plain objects with properties
    /// - Objects with index signatures
    /// - Intersections (which may contain objects)
    fn is_object_like(&self, type_id: TypeId) -> bool {
        use crate::visitor::{intersection_list_id, object_shape_id, object_with_index_shape_id};

        object_shape_id(self.interner, type_id).is_some()
            || object_with_index_shape_id(self.interner, type_id).is_some()
            || intersection_list_id(self.interner, type_id).is_some()
    }

    /// Check if two object-like types have overlapping properties and index signatures.
    ///
    /// This is the refined implementation using `PropertyCollector` to handle:
    /// - Intersections (flattened property collection)
    /// - Index signatures (both string and number)
    /// - Optional properties (correct undefined handling via `optional_property_type`)
    /// - Discriminant detection (common property with disjoint literal types)
    ///
    /// Returns false if types have zero overlap, true otherwise.
    fn do_refined_object_overlap_check(&self, a: TypeId, b: TypeId) -> bool {
        use crate::objects::{PropertyCollectionResult, collect_properties};

        // Collect properties and index signatures from both types
        let res_a = collect_properties(a, self.interner, self.resolver);
        let res_b = collect_properties(b, self.interner, self.resolver);

        // Extract properties and index signatures from results
        let (props_a, s_idx_a, _n_idx_a) = match res_a {
            PropertyCollectionResult::Any | PropertyCollectionResult::NonObject => return true, // Any overlaps with everything
            // Conservatively overlap
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
            } => (properties, string_index, number_index),
        };

        let (props_b, s_idx_b, _n_idx_b) = match res_b {
            PropertyCollectionResult::Any | PropertyCollectionResult::NonObject => return true,
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
            } => (properties, string_index, number_index),
        };

        // 1. Check Common Properties for overlap
        // If a property exists in both objects, their types must overlap
        for p_a in &props_a {
            if let Some(p_b) = props_b.iter().find(|p| p.name == p_a.name) {
                // Use optional_property_type for correct undefined handling
                let type_a = self.optional_property_type(p_a);
                let type_b = self.optional_property_type(p_b);

                if !self.are_types_overlapping(type_a, type_b) {
                    return false; // Hard conflict - no overlap
                }
            }
        }

        // 2. Check Required Properties A against Index Signatures B
        // Only REQUIRED properties must be compatible with B's string index.
        // Optional properties can be missing (undefined) so they don't conflict with index signatures.
        // Example: { a?: string } and { [k: string]: number } DO overlap because {} satisfies both.
        if let Some(ref idx_b) = s_idx_b {
            for p_a in &props_a {
                if !p_a.optional {
                    // Only check required properties
                    if !self.are_types_overlapping(p_a.type_id, idx_b.value_type) {
                        return false;
                    }
                }
            }
        }

        // 3. Check Required Properties B against Index Signatures A
        // Only REQUIRED properties must be compatible with A's string index.
        if let Some(ref idx_a) = s_idx_a {
            for p_b in &props_b {
                if !p_b.optional {
                    // Only check required properties
                    if !self.are_types_overlapping(p_b.type_id, idx_a.value_type) {
                        return false;
                    }
                }
            }
        }

        // 4. Index Signature Compatibility Check
        // NOTE: Index signatures do NOT prevent overlap even if their value types are disjoint
        // because the empty object {} satisfies both index signatures.
        // Example: { [k: string]: string } and { [k: string]: number } DO overlap.
        // So NO CHECK needed here - index signatures never cause disjointness.

        // All checks passed - types overlap
        true
    }
}
