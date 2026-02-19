//! Template literal type interning and normalization.
//!
//! This module handles:
//! - Template literal expansion to union types
//! - Template span cardinality computation
//! - Template literal normalization (merging adjacent text spans)
//! - Template literal introspection (interpolation positions, span access)

use crate::intern::{TEMPLATE_LITERAL_EXPANSION_LIMIT, TypeInterner};
use crate::types::{LiteralValue, TemplateSpan, TypeData, TypeId};

impl TypeInterner {
    fn template_span_cardinality(&self, type_id: TypeId) -> Option<usize> {
        // Handle BOOLEAN intrinsic (expands to 2 values: true | false)
        if type_id == TypeId::BOOLEAN {
            return Some(2);
        }

        // Handle intrinsic types that expand to string literals
        if type_id == TypeId::BOOLEAN_TRUE
            || type_id == TypeId::BOOLEAN_FALSE
            || type_id == TypeId::NULL
            || type_id == TypeId::UNDEFINED
            || type_id == TypeId::VOID
        {
            return Some(1);
        }

        match self.lookup(type_id) {
            // Accept all literal types (String, Number, Boolean, BigInt) - they all stringify
            Some(TypeData::Literal(_)) => Some(1),
            Some(TypeData::Union(list_id)) => {
                let members = self.type_list(list_id);
                let mut count = 0usize;
                for member in members.iter() {
                    // Recurse to handle all cases uniformly (literals, intrinsics, nested unions)
                    let member_count = self.template_span_cardinality(*member)?;
                    count = count.checked_add(member_count)?;
                }
                Some(count)
            }
            // Task #47: Handle nested template literals
            Some(TypeData::TemplateLiteral(list_id)) => {
                let spans = self.template_list(list_id);
                let mut total = 1usize;
                for span in spans.iter() {
                    let span_count = match span {
                        TemplateSpan::Text(_) => 1,
                        TemplateSpan::Type(t) => self.template_span_cardinality(*t)?,
                    };
                    total = total.saturating_mul(span_count);
                }
                Some(total)
            }
            _ => None,
        }
    }

    fn template_literal_exceeds_limit(&self, spans: &[TemplateSpan]) -> bool {
        let mut total = 1usize;
        for span in spans {
            let span_count = match span {
                TemplateSpan::Text(_) => Some(1),
                TemplateSpan::Type(type_id) => self.template_span_cardinality(*type_id),
            };
            let Some(span_count) = span_count else {
                return false;
            };
            total = total.saturating_mul(span_count);
            if total > TEMPLATE_LITERAL_EXPANSION_LIMIT {
                return true;
            }
        }
        false
    }

    /// Check if a template literal can be expanded to a union of string literals.
    /// Returns true if all type interpolations are string literals or unions of string literals.
    fn can_expand_template_literal(&self, spans: &[TemplateSpan]) -> bool {
        for span in spans {
            if let TemplateSpan::Type(type_id) = span
                && self.template_span_cardinality(*type_id).is_none()
            {
                return false;
            }
        }
        true
    }

    /// Get the string literal values from a type (single literal or union of literals).
    /// Returns None if the type is not a string literal or union of string literals.
    fn get_string_literal_values(&self, type_id: TypeId) -> Option<Vec<String>> {
        // Handle BOOLEAN intrinsic (expands to two string literals)
        if type_id == TypeId::BOOLEAN {
            return Some(vec!["true".to_string(), "false".to_string()]);
        }

        // Helper to convert a single type to a string value if possible
        let to_string_val = |id: TypeId| -> Option<String> {
            // Handle intrinsics that stringify to text
            if id == TypeId::NULL {
                return Some("null".to_string());
            }
            if id == TypeId::UNDEFINED || id == TypeId::VOID {
                return Some("undefined".to_string());
            }
            if id == TypeId::BOOLEAN_TRUE {
                return Some("true".to_string());
            }
            if id == TypeId::BOOLEAN_FALSE {
                return Some("false".to_string());
            }

            // Handle literal types
            match self.lookup(id) {
                Some(TypeData::Literal(LiteralValue::String(atom))) => {
                    Some(self.resolve_atom_ref(atom).to_string())
                }
                Some(TypeData::Literal(LiteralValue::Boolean(b))) => Some(b.to_string()),
                Some(TypeData::Literal(LiteralValue::Number(n))) => {
                    // TypeScript stringifies numbers in templates (e.g., 1 -> "1", 1.5 -> "1.5")
                    Some(format!("{}", n.0))
                }
                Some(TypeData::Literal(LiteralValue::BigInt(atom))) => {
                    // BigInts in templates are stringified (e.g., 100n -> "100")
                    Some(self.resolve_atom_ref(atom).to_string())
                }
                _ => None,
            }
        };

        // Handle the top-level type (either a single value or a union)
        if let Some(val) = to_string_val(type_id) {
            return Some(vec![val]);
        }

        match self.lookup(type_id) {
            Some(TypeData::Union(list_id)) => {
                let members = self.type_list(list_id);
                let mut values = Vec::new();
                for member in members.iter() {
                    // RECURSIVE CALL: Handle boolean-in-union and nested unions correctly
                    let member_values = self.get_string_literal_values(*member)?;
                    values.extend(member_values);
                }
                Some(values)
            }
            // Task #47: Handle nested template literals by expanding them recursively
            Some(TypeData::TemplateLiteral(list_id)) => {
                let spans = self.template_list(list_id);
                // Check if all spans are text-only (can return a single string)
                if spans.iter().all(|s| matches!(s, TemplateSpan::Text(_))) {
                    let mut combined = String::new();
                    for span in spans.iter() {
                        if let TemplateSpan::Text(atom) = span {
                            combined.push_str(&self.resolve_atom_ref(*atom));
                        }
                    }
                    return Some(vec![combined]);
                }
                // Otherwise, try to expand via Cartesian product (recursively call expand_template_literal_to_union)
                // But we need to be careful not to cause infinite recursion
                // For now, return None to indicate this template cannot be expanded as simple string literals
                None
            }
            _ => None,
        }
    }

    /// Expand a template literal with union interpolations into a union of string literals.
    /// For example: `prefix-${"a" | "b"}-suffix` -> "prefix-a-suffix" | "prefix-b-suffix"
    fn expand_template_literal_to_union(&self, spans: &[TemplateSpan]) -> TypeId {
        // Collect text parts and interpolation alternatives
        let mut parts: Vec<Vec<String>> = Vec::new();

        for span in spans {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.resolve_atom_ref(*atom).to_string();
                    parts.push(vec![text]);
                }
                TemplateSpan::Type(type_id) => {
                    if let Some(values) = self.get_string_literal_values(*type_id) {
                        parts.push(values);
                    } else {
                        // Should not happen if can_expand_template_literal returned true
                        return TypeId::STRING;
                    }
                }
            }
        }

        // Generate all combinations using Cartesian product
        let mut combinations: Vec<String> = vec![String::new()];

        for part in &parts {
            let mut new_combinations = Vec::with_capacity(combinations.len() * part.len());
            for prefix in &combinations {
                for suffix in part {
                    let mut combined = prefix.clone();
                    combined.push_str(suffix);
                    new_combinations.push(combined);
                }
            }
            combinations = new_combinations;

            // Safety check: should not exceed limit at this point, but verify
            if combinations.len() > TEMPLATE_LITERAL_EXPANSION_LIMIT {
                return TypeId::STRING;
            }
        }

        // Create union of string literals
        if combinations.is_empty() {
            return TypeId::NEVER;
        }

        if combinations.len() == 1 {
            return self.literal_string(&combinations[0]);
        }

        let members: Vec<TypeId> = combinations
            .iter()
            .map(|s| self.literal_string(s))
            .collect();

        self.union(members)
    }

    /// Normalize template literal spans by merging consecutive text spans
    fn normalize_template_spans(&self, spans: Vec<TemplateSpan>) -> Vec<TemplateSpan> {
        if spans.len() <= 1 {
            return spans;
        }

        let mut normalized = Vec::with_capacity(spans.len());
        let mut pending_text: Option<String> = None;
        let mut has_consecutive_texts = false;

        for span in &spans {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.resolve_atom_ref(*atom).to_string();
                    if let Some(ref mut pt) = pending_text {
                        pt.push_str(&text);
                        has_consecutive_texts = true;
                    } else {
                        pending_text = Some(text);
                    }
                }
                TemplateSpan::Type(type_id) => {
                    // Task #47: Flatten nested template literals
                    // If a Type(type_id) refers to another TemplateLiteral, splice its spans into the parent
                    if let Some(TypeData::TemplateLiteral(nested_list_id)) = self.lookup(*type_id) {
                        let nested_spans = self.template_list(nested_list_id);
                        // Process each nested span as if it were part of the parent template
                        for nested_span in nested_spans.iter() {
                            match nested_span {
                                TemplateSpan::Text(atom) => {
                                    let text = self.resolve_atom_ref(*atom).to_string();
                                    if let Some(ref mut pt) = pending_text {
                                        pt.push_str(&text);
                                        has_consecutive_texts = true;
                                    } else {
                                        pending_text = Some(text);
                                    }
                                }
                                TemplateSpan::Type(nested_type_id) => {
                                    // Flush pending text before adding the nested type
                                    if let Some(text) = pending_text.take()
                                        && !text.is_empty()
                                    {
                                        normalized
                                            .push(TemplateSpan::Text(self.intern_string(&text)));
                                    }
                                    normalized.push(TemplateSpan::Type(*nested_type_id));
                                }
                            }
                        }
                        // Continue to the next span in the parent template
                        continue;
                    }

                    // Task #47: Intrinsic stringification/expansion rules
                    match *type_id {
                        TypeId::NULL => {
                            // null becomes text "null"
                            let text = "null";
                            if let Some(ref mut pt) = pending_text {
                                pt.push_str(text);
                                has_consecutive_texts = true;
                            } else {
                                pending_text = Some(text.to_string());
                            }
                            continue;
                        }
                        TypeId::UNDEFINED | TypeId::VOID => {
                            // undefined/void becomes text "undefined"
                            let text = "undefined";
                            if let Some(ref mut pt) = pending_text {
                                pt.push_str(text);
                                has_consecutive_texts = true;
                            } else {
                                pending_text = Some(text.to_string());
                            }
                            continue;
                        }
                        // number, bigint, string intrinsics do NOT widen - they're kept as-is for pattern matching
                        // BOOLEAN is also kept as-is for pattern matching - the general expansion logic handles it
                        _ => {}
                    }

                    // Task #47: Remove empty string literals from interpolations
                    // An empty string literal contributes nothing to the template
                    if let Some(TypeData::Literal(LiteralValue::String(s))) = self.lookup(*type_id)
                    {
                        let s = self.resolve_atom_ref(s);
                        if s.is_empty() {
                            // Skip this empty string literal
                            // Flush pending text first
                            if let Some(text) = pending_text.take()
                                && !text.is_empty()
                            {
                                normalized.push(TemplateSpan::Text(self.intern_string(&text)));
                            }
                            // Don't add the empty type span - continue to next span
                            continue;
                        }
                    }

                    // Flush any pending text before adding a type span
                    if let Some(text) = pending_text.take()
                        && !text.is_empty()
                    {
                        normalized.push(TemplateSpan::Text(self.intern_string(&text)));
                    }
                    normalized.push(TemplateSpan::Type(*type_id));
                }
            }
        }

        // Flush any remaining pending text
        if let Some(text) = pending_text
            && !text.is_empty()
        {
            normalized.push(TemplateSpan::Text(self.intern_string(&text)));
        }

        // If no normalization occurred, return original to avoid unnecessary allocation
        if !has_consecutive_texts && normalized.len() == spans.len() {
            return spans;
        }

        normalized
    }

    /// Intern a template literal type
    pub fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        // Task #47: High-level absorption and widening (Pass 1)
        // These checks must happen BEFORE structural normalization

        // Never absorption: if any part is never, the whole type is never
        for span in &spans {
            if let TemplateSpan::Type(type_id) = span
                && *type_id == TypeId::NEVER
            {
                return TypeId::NEVER;
            }
        }

        // Unknown and Any widening: if any part is unknown or any, the whole type is string
        // Note: string intrinsic does NOT widen (it's used for pattern matching)
        for span in &spans {
            if let TemplateSpan::Type(type_id) = span
                && (*type_id == TypeId::UNKNOWN || *type_id == TypeId::ANY)
            {
                return TypeId::STRING;
            }
        }

        // Normalize spans by merging consecutive text spans (Pass 2)
        let normalized = self.normalize_template_spans(spans);

        // Check if expansion would exceed the limit
        if self.template_literal_exceeds_limit(&normalized) {
            return TypeId::STRING;
        }

        // Try to expand to union of string literals if all interpolations are expandable
        if self.can_expand_template_literal(&normalized) {
            // Check if there are any type interpolations
            let has_type_interpolations = normalized
                .iter()
                .any(|s| matches!(s, TemplateSpan::Type(_)));

            if has_type_interpolations {
                return self.expand_template_literal_to_union(&normalized);
            }

            // If only text spans, combine them into a single string literal
            if normalized
                .iter()
                .all(|s| matches!(s, TemplateSpan::Text(_)))
            {
                let mut combined = String::new();
                for span in &normalized {
                    if let TemplateSpan::Text(atom) = span {
                        combined.push_str(&self.resolve_atom_ref(*atom));
                    }
                }
                return self.literal_string(&combined);
            }
        }

        let list_id = self.intern_template_list(normalized);
        self.intern(TypeData::TemplateLiteral(list_id))
    }

    /// Get the interpolation positions from a template literal type
    /// Returns indices of type interpolation spans
    pub fn template_literal_interpolation_positions(&self, type_id: TypeId) -> Vec<usize> {
        match self.lookup(type_id) {
            Some(TypeData::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, span)| match span {
                        TemplateSpan::Type(_) => Some(idx),
                        _ => None,
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get the span at a given position from a template literal type
    pub fn template_literal_get_span(&self, type_id: TypeId, index: usize) -> Option<TemplateSpan> {
        match self.lookup(type_id) {
            Some(TypeData::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans.get(index).cloned()
            }
            _ => None,
        }
    }

    /// Get the number of spans in a template literal type
    pub fn template_literal_span_count(&self, type_id: TypeId) -> usize {
        match self.lookup(type_id) {
            Some(TypeData::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans.len()
            }
            _ => 0,
        }
    }

    /// Check if a template literal contains only text (no interpolations)
    /// Also returns true for string literals (which are the result of text-only template expansion)
    pub fn template_literal_is_text_only(&self, type_id: TypeId) -> bool {
        match self.lookup(type_id) {
            Some(TypeData::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans.iter().all(super::types::TemplateSpan::is_text)
            }
            // String literals are the result of text-only template expansion
            Some(TypeData::Literal(LiteralValue::String(_))) => true,
            _ => false,
        }
    }
}
