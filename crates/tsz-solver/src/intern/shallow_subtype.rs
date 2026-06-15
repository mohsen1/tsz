//! Shallow, allocation-free subtype checks used during union/intersection
//! normalization.
//!
//! Every method here reads types through `lookup()` / `type_list()` style
//! accessors only -- never `intern()` or `evaluate()` -- so union and
//! intersection reduction can compare members without re-entering the interner
//! or triggering project-scale evaluation. Extracted from `normalize.rs` to keep
//! both files under the source-size cap; `normalize.rs` retains the canonical
//! reduction passes (`reduce_union_subtypes`, `reduce_intersection_subtypes`)
//! that call into this engine.

use super::normalize::{LiteralDomain, PrimitiveClass};
use super::{TypeInterner, TypeListBuffer};
use crate::types::{
    FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShapeId, ParamInfo, TemplateLiteralId,
    TemplateSpan, TypeData, TypeId,
};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use std::sync::Arc;
use tsz_common::interner::Atom;

impl TypeInterner {
    /// Shallow subtype check that avoids infinite recursion.
    /// Uses `TypeId` identity for nested components instead of recursive checking.
    /// This is safe for use during normalization because it only uses `lookup()` and
    /// never calls `intern()` or `evaluate()`.
    #[inline]
    pub(super) fn is_subtype_shallow(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_shallow_depth(source, target, 3)
    }

    /// Depth-limited shallow subtype check. Handles primitives, literals, objects,
    /// and function types. The depth parameter limits recursion through object
    /// properties (each level allows one more structural comparison).
    fn is_subtype_shallow_depth(&self, source: TypeId, target: TypeId, depth: u32) -> bool {
        if source == target {
            return true;
        }
        if depth == 0 {
            return false;
        }

        // Handle Top/Bottom types (no lookup needed)
        if target.is_any_or_unknown() {
            return true;
        }
        if source.is_never() {
            return true;
        }

        // Single lookup per type — reuse throughout the function
        let s_data = self.lookup(source);
        let t_data = self.lookup(target);

        // Skip reduction for type parameters and lazy types
        if matches!(
            (&s_data, &t_data),
            (
                Some(TypeData::TypeParameter(_)) | _,
                Some(TypeData::TypeParameter(_))
            ) | (Some(TypeData::Lazy(_)) | _, Some(TypeData::Lazy(_)))
        ) {
            return false;
        }

        // Handle Literal to Primitive (including unions containing primitives)
        if matches!(s_data, Some(TypeData::Literal(_))) {
            if matches!(t_data, Some(TypeData::Literal(_))) {
                // Both are literals - only subtype if identical (handled above)
                return false;
            }

            if let (
                Some(TypeData::Literal(LiteralValue::String(literal))),
                Some(TypeData::TemplateLiteral(template_id)),
            ) = (&s_data, &t_data)
            {
                // Union normalization must stay shallow. Full template-literal
                // matching may evaluate and intern large intermediate unions,
                // turning normalization into a project-scale hotspot.
                return self
                    .literal_string_matches_template_literal_shallow(*literal, *template_id);
            }

            // Check if target is a union containing a compatible primitive
            if let Some(TypeData::Union(members)) = t_data {
                let members = self.type_list(members);
                for &member in members.iter() {
                    if self.is_subtype_shallow_depth(source, member, depth) {
                        return true;
                    }
                }
                return false;
            }

            // Otherwise, check literal-to-primitive compatibility
            if let Some(domain) = self.literal_domain_from_type(source)
                && let Some(target_class) = self.primitive_class_for(target)
                && self.literal_domain_matches_primitive(domain, target_class)
            {
                return true;
            }
        }

        // Handle source as member of target union (for built-in/primitive types only).
        if self.is_builtin_type(source)
            && let Some(TypeData::Union(members)) = t_data
        {
            let members = self.type_list(members);
            return members.contains(&source);
        }

        // Handle source union: every member must be a subtype of the target.
        // This enables reduction of unions like `('hello' | undefined) <: (string | undefined)`
        // which arise from optional parameter types in function subtype checks.
        if let Some(TypeData::Union(s_members)) = s_data {
            let s_members = self.type_list(s_members);
            // Guard: only handle small unions to avoid O(N*M) blowup
            if s_members.len() <= 8 {
                return s_members
                    .iter()
                    .all(|&m| self.is_subtype_shallow_depth(m, target, depth - 1));
            }
            return false;
        }

        // Handle non-literal, non-builtin source against target union.
        // Generalizes the existing literal and builtin checks above to cover
        // cases like Function <: (Function | undefined).
        if let Some(TypeData::Union(t_members)) = t_data {
            let t_members = self.type_list(t_members);
            if t_members.len() <= 8 {
                return t_members
                    .iter()
                    .any(|&m| self.is_subtype_shallow_depth(source, m, depth - 1));
            }
            return false;
        }

        // Handle structural type comparisons
        match (s_data, t_data) {
            (
                Some(TypeData::Object(s_id) | TypeData::ObjectWithIndex(s_id)),
                Some(TypeData::Object(t_id) | TypeData::ObjectWithIndex(t_id)),
            ) => self.is_object_shape_subtype_shallow_depth(s_id, t_id, 0),
            (Some(TypeData::Function(s_id)), Some(TypeData::Function(t_id))) => {
                self.is_function_subtype_shallow(s_id, t_id, depth)
            }
            _ => false,
        }
    }

    fn literal_string_matches_template_literal_shallow(
        &self,
        literal: Atom,
        template_id: TemplateLiteralId,
    ) -> bool {
        let literal = self.resolve_atom(literal);
        if literal.len() > 128 {
            return false;
        }

        let spans = self.template_list(template_id);
        if spans.len() > 8 {
            return false;
        }

        self.match_template_literal_shallow(literal.as_str(), &spans, 0)
    }

    fn match_template_literal_shallow(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let Some(span) = spans.get(span_idx) else {
            return remaining.is_empty();
        };

        match span {
            TemplateSpan::Text(text) => {
                let text = self.resolve_atom(*text);
                remaining
                    .strip_prefix(text.as_str())
                    .is_some_and(|remaining| {
                        self.match_template_literal_shallow(remaining, spans, span_idx + 1)
                    })
            }
            TemplateSpan::Type(type_id) => {
                self.match_template_type_span_shallow(remaining, spans, span_idx, *type_id)
            }
        }
    }

    fn match_template_type_span_shallow(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
        type_id: TypeId,
    ) -> bool {
        match self.lookup(type_id) {
            Some(TypeData::Intrinsic(IntrinsicKind::Number)) => {
                let num_len =
                    crate::relations::subtype::rules::literals::find_number_length(remaining);
                if num_len == 0 {
                    return false;
                }
                for len in (1..=num_len).rev() {
                    if crate::relations::subtype::rules::literals::is_valid_number(
                        &remaining[..len],
                    ) && self.match_template_literal_shallow(
                        &remaining[len..],
                        spans,
                        span_idx + 1,
                    ) {
                        return true;
                    }
                }
                false
            }
            Some(TypeData::Intrinsic(IntrinsicKind::Bigint)) => {
                let len =
                    crate::relations::subtype::rules::literals::find_integer_length(remaining);
                len > 0
                    && self.match_template_literal_shallow(&remaining[len..], spans, span_idx + 1)
            }
            Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => {
                ["true", "false"].into_iter().any(|prefix| {
                    remaining.strip_prefix(prefix).is_some_and(|remaining| {
                        self.match_template_literal_shallow(remaining, spans, span_idx + 1)
                    })
                })
            }
            Some(TypeData::Literal(literal)) => {
                let literal_text = match literal {
                    LiteralValue::String(atom) | LiteralValue::BigInt(atom) => {
                        self.resolve_atom(atom)
                    }
                    LiteralValue::Number(num) => {
                        crate::relations::subtype::rules::literals::format_number_for_template(
                            num.0,
                        )
                    }
                    LiteralValue::Boolean(value) => {
                        if value {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                };
                remaining
                    .strip_prefix(literal_text.as_str())
                    .is_some_and(|remaining| {
                        self.match_template_literal_shallow(remaining, spans, span_idx + 1)
                    })
            }
            Some(TypeData::Union(list_id)) => {
                let members = self.type_list(list_id);
                members.len() <= 8
                    && members.iter().any(|member| {
                        self.match_template_type_span_shallow(remaining, spans, span_idx, *member)
                    })
            }
            _ => false,
        }
    }

    /// Remove string-literal union members that are matched by a sibling
    /// template-literal member: `"foo-x" | foo-${string}` reduces to the
    /// template member `foo-${string}`.
    ///
    /// This is the targeted equivalent of the generic pairwise loop in
    /// `reduce_union_subtypes` for unions whose members are all unit/inert types
    /// plus template literals: template literals are never reduced themselves and
    /// unit members never reduce each other after dedup, so string-literal-vs-
    /// template is the only pair shape that can reduce. Cost is O(L×T) cheap
    /// string checks instead of O(N²) type lookups.
    ///
    /// Template spans are resolved once per template (not once per pair), and a
    /// template whose first span is literal text only matches literals that start
    /// with that text — checked first as an O(prefix) prefilter.
    pub(super) fn remove_string_literals_matched_by_templates(
        &self,
        flat: &mut TypeListBuffer,
        string_literals: &[(usize, Atom)],
        templates: &[TemplateLiteralId],
    ) {
        // Resolve each template's span list once; precompute the leading-text
        // prefix prefilter. Mirrors the per-pair guards of
        // `literal_string_matches_template_literal_shallow`.
        type TemplateCandidate = (Option<Arc<str>>, Arc<[TemplateSpan]>);
        let mut candidates: SmallVec<[TemplateCandidate; 8]> = SmallVec::new();
        for &template_id in templates {
            let spans = self.template_list(template_id);
            if spans.len() > 8 {
                continue;
            }
            let prefix = match spans.first() {
                Some(TemplateSpan::Text(atom)) => Some(self.resolve_atom_ref(*atom)),
                _ => None,
            };
            candidates.push((prefix, spans));
        }
        if candidates.is_empty() {
            return;
        }

        let mut removed: FxHashSet<usize> = FxHashSet::default();
        for &(idx, atom) in string_literals {
            let literal = self.resolve_atom_ref(atom);
            if literal.len() > 128 {
                continue;
            }
            for (prefix, spans) in &candidates {
                if let Some(prefix) = prefix
                    && !literal.starts_with(prefix.as_ref())
                {
                    continue;
                }
                if self.match_template_literal_shallow(&literal, spans, 0) {
                    removed.insert(idx);
                    break;
                }
            }
        }

        if !removed.is_empty() {
            let mut i = 0;
            flat.retain(|_| {
                let keep = !removed.contains(&i);
                i += 1;
                keep
            });
        }
    }

    /// Shallow object shape subtype check with depth-limited property comparison.
    ///
    /// At depth > 0, property types are compared using `is_subtype_shallow_depth`,
    /// enabling reduction of objects whose properties differ structurally
    /// (e.g., `{ f(): void } | { f(x?: string): void }`).
    /// At depth 0, falls back to `TypeId` equality for properties.
    ///
    /// ## Subtyping Rules:
    /// - **Width subtyping**: Source can have extra properties
    /// - **Type comparison**: TypeId equality first, then depth-limited structural check
    /// - **Optional**: Required <: Optional is true, Optional <: Required is false
    /// - **Readonly**: Mutable <: Readonly is true, Readonly <: Mutable is false
    /// - **Nominal**: If target has a symbol, source must have the same symbol
    /// - **Index Signatures**: Skipped (too complex for shallow check)
    ///
    /// Uses O(N+M) two-pointer scan since properties are sorted by Atom.
    fn is_object_shape_subtype_shallow_depth(
        &self,
        s_id: ObjectShapeId,
        t_id: ObjectShapeId,
        depth: u32,
    ) -> bool {
        if s_id == t_id {
            return true;
        }

        let s = self.object_shape(s_id);
        let t = self.object_shape(t_id);

        // 1. Nominal check: if target is a class instance, source must match
        if t.symbol.is_some() && s.symbol != t.symbol {
            return false;
        }

        // 2. Conservative: Index signatures make subtyping complex (deferred to Solver)
        if t.string_index.is_some() || t.number_index.is_some() {
            return false;
        }

        // 3. Structural scan: Source must satisfy all Target properties.
        // Also tracks if we found ANY property overlap. If source and target have
        // completely disjoint properties, they are not in a subtype relationship.
        // Properties are sorted by Atom, so use two-pointer scan for O(N+M).
        let mut s_idx = 0;
        let s_props = &s.properties;
        let t_props = &t.properties;
        let mut has_any_overlap = false;

        for t_prop in t_props {
            // Advance source pointer to match target property name
            while s_idx < s_props.len() && s_props[s_idx].name < t_prop.name {
                s_idx += 1;
            }

            if s_idx < s_props.len() && s_props[s_idx].name == t_prop.name {
                let sp = &s_props[s_idx];
                has_any_overlap = true;

                // Type comparison: try TypeId equality first, then depth-limited structural check
                if sp.type_id != t_prop.type_id
                    && !self.is_subtype_shallow_depth(sp.type_id, t_prop.type_id, depth)
                {
                    return false;
                }

                // Rule: Required <: Optional (Optional <: Required is False)
                if !t_prop.optional && sp.optional {
                    return false;
                }

                // Rule: Mutable <: Readonly (Readonly <: Mutable is False)
                if !t_prop.readonly && sp.readonly {
                    return false;
                }

                s_idx += 1;
            } else {
                // Property missing in source: only allowed if target property is optional
                if !t_prop.optional {
                    return false;
                }
            }
        }

        // Disjoint properties check: must have at least one overlapping property
        // (matching tsc's reduction logic for unrelated object types).
        has_any_overlap
    }

    /// Shallow function subtype check for union reduction.
    ///
    /// Implements TypeScript's function subtyping rules:
    /// - Source can have fewer params than target (callback parameter compatibility)
    /// - Extra source params must be optional (otherwise source requires more args)
    /// - Parameters are checked contravariantly (target param type <: source param type)
    /// - Return type is checked covariantly (source return type <: target return type)
    /// - Handles optional vs required params with `| undefined` equivalence
    /// - Skips generic functions (too complex for shallow check)
    fn is_function_subtype_shallow(
        &self,
        s_id: FunctionShapeId,
        t_id: FunctionShapeId,
        depth: u32,
    ) -> bool {
        if s_id == t_id {
            return true;
        }

        let s = self.function_shape(s_id);
        let t = self.function_shape(t_id);

        // Skip generic functions
        if !s.type_params.is_empty() || !t.type_params.is_empty() {
            return false;
        }

        // this-type must match (different `this` types = different function types)
        if s.this_type != t.this_type {
            return false;
        }

        // Return type: covariant (source return <: target return)
        if s.return_type != t.return_type
            && !self.is_subtype_shallow_depth(s.return_type, t.return_type, depth)
        {
            return false;
        }

        // Check params in the shared range contravariantly
        let min_len = s.params.len().min(t.params.len());
        for i in 0..min_len {
            if !self.param_contravariant_shallow(&t.params[i], &s.params[i], depth) {
                return false;
            }
        }

        // Source cannot have more total params than target (even optional ones).
        // In tsc's subtype relation, `(x?: string) => void` is NOT a subtype
        // of `() => void` — having extra params (even optional) prevents subtyping.
        // But source with FEWER params IS a subtype (callback compatibility).
        if s.params.len() > t.params.len() {
            return false;
        }

        // Conservative guard for overload-like function pairs:
        // If all overlapping params have identical TypeIds, the functions look like
        // overload variants of the same method (e.g., `reduce(cb)` vs `reduce(cb, init)`).
        // Don't reduce these — even though one is technically a subtype, removing it
        // can break contextual typing and overload resolution.
        //
        // This guard allows reduction when param types actually differ, which is the
        // pattern in unionTypeReduction2: `(x: string|undefined) => void <: (x?: string) => void`
        // where the param TypeIds differ (string|undefined vs string).
        if min_len > 0 {
            let all_params_identical =
                (0..min_len).all(|i| s.params[i].type_id == t.params[i].type_id);
            if all_params_identical {
                return false;
            }
        }

        true
    }

    /// Check contravariant parameter compatibility for function subtyping.
    ///
    /// For `S <: T`, parameters are checked contravariantly: `T_param <: S_param`.
    /// Handles the optional/required distinction where `x?: T` has effective type
    /// `T | undefined` and `x: T | undefined` is equivalent.
    fn param_contravariant_shallow(
        &self,
        t_param: &ParamInfo,
        s_param: &ParamInfo,
        depth: u32,
    ) -> bool {
        let t_type = t_param.type_id;
        let s_type = s_param.type_id;

        if t_type == s_type {
            // Same base types. Check optional/required compatibility.
            if t_param.optional && !s_param.optional {
                // t_effective = type | undefined, s_effective = type
                // Need: type | undefined <: type — only if type contains undefined
                return self.type_contains_undefined(s_type);
            }
            return true;
        }

        // Types differ. Check effective type subtyping based on optionality.
        match (t_param.optional, s_param.optional) {
            (false, false) => {
                // Both required: t_type <: s_type
                self.is_subtype_shallow_depth(t_type, s_type, depth)
            }
            (true, true) => {
                // Both optional: t_type | undef <: s_type | undef
                // Reduces to: t_type <: s_type | undef, which holds if t_type <: s_type
                self.is_subtype_shallow_depth(t_type, s_type, depth) || t_type == TypeId::UNDEFINED
            }
            (false, true) => {
                // t required, s optional: t_type <: s_type | undef
                // Holds if t_type <: s_type (since s_type ⊂ s_type | undef)
                self.is_subtype_shallow_depth(t_type, s_type, depth)
            }
            (true, false) => {
                // t optional, s required: t_type | undef <: s_type
                // Need both: t_type <: s_type AND undefined <: s_type
                self.type_contains_undefined(s_type)
                    && self.is_subtype_shallow_depth(t_type, s_type, depth)
            }
        }
    }

    /// Check if a type is a built-in primitive (string, number, boolean, etc.).
    /// These are safe to check against union targets without risk of cascading
    /// reductions that affect complex type inference.
    const fn is_builtin_type(&self, id: TypeId) -> bool {
        matches!(
            id,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::VOID
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        )
    }

    /// Check if a type contains `undefined` (either is `undefined` or is a
    /// union containing `undefined`). Uses only `lookup()`, safe during normalization.
    fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED {
            return true;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(TypeData::Union(members)) = self.lookup(type_id) {
            let members = self.type_list(members);
            return members.contains(&TypeId::UNDEFINED);
        }
        false
    }

    /// Check if a literal domain matches a primitive class.
    const fn literal_domain_matches_primitive(
        &self,
        domain: LiteralDomain,
        class: PrimitiveClass,
    ) -> bool {
        matches!(
            (domain, class),
            (LiteralDomain::String, PrimitiveClass::String)
                | (LiteralDomain::Number, PrimitiveClass::Number)
                | (LiteralDomain::Boolean, PrimitiveClass::Boolean)
                | (LiteralDomain::Bigint, PrimitiveClass::Bigint)
        )
    }
}
