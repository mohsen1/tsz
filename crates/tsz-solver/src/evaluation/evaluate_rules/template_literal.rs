//! Template literal type evaluation.
//!
//! Handles TypeScript template literal types like "hello ${T}".

use crate::relations::subtype::TypeResolver;
use crate::types::{LiteralValue, TemplateLiteralId, TemplateSpan, TypeData, TypeId};

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a template literal type: `hello${T}world`
    ///
    /// Template literals evaluate to a union of all possible literal string combinations.
    /// For example: `get${K}` where K = "a" | "b" evaluates to "geta" | "getb"
    /// Multiple unions compute a Cartesian product: `${"a"|"b"}-${"x"|"y"}` => "a-x"|"a-y"|"b-x"|"b-y"
    pub fn evaluate_template_literal(&mut self, spans: TemplateLiteralId) -> TypeId {
        use crate::intern::TEMPLATE_LITERAL_EXPANSION_LIMIT;

        let span_list = self.interner().template_list(spans);

        tracing::trace!(
            span_count = span_list.len(),
            "evaluate_template_literal: called with {} spans",
            span_list.len()
        );

        // Check if all spans are just text (no interpolation)
        let all_text = span_list
            .iter()
            .all(|span| matches!(span, TemplateSpan::Text(_)));

        if all_text {
            tracing::trace!("evaluate_template_literal: all text - concatenating");
            // Concatenate all text spans into a single string literal
            let mut result = String::new();
            for span in span_list.iter() {
                if let TemplateSpan::Text(atom) = span {
                    result.push_str(self.interner().resolve_atom_ref(*atom).as_ref());
                }
            }
            return self.interner().literal_string(&result);
        }

        // PERF: Pre-evaluate all type spans once and cache results.
        // This avoids double evaluation in the size-check loop and expansion loop.
        let mut evaluated_strings = Vec::with_capacity(span_list.len());
        let mut normalized_spans = Vec::with_capacity(span_list.len());
        let mut total_combinations: usize = 1;

        let mut can_fully_expand = true;
        for span in span_list.iter() {
            match span {
                TemplateSpan::Text(atom) => {
                    evaluated_strings.push(None); // Marker for text span
                    normalized_spans.push(TemplateSpan::Text(*atom));
                }
                TemplateSpan::Type(type_id) => {
                    let evaluated = self.evaluate(*type_id);
                    normalized_spans.push(TemplateSpan::Type(evaluated));
                    let strings = self.extract_literal_strings(evaluated);
                    let span_count = self
                        .template_span_complexity_cardinality(evaluated)
                        .or_else(|| (!strings.is_empty()).then_some(strings.len()));

                    if let Some(span_count) = span_count {
                        total_combinations = total_combinations.saturating_mul(span_count);
                        if total_combinations >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                            self.interner().mark_union_too_complex();
                            return TypeId::STRING;
                        }
                    }

                    if strings.is_empty() {
                        // Contains non-literal types. Keep scanning the remaining
                        // spans first so mixed template unions can still trip TS2590.
                        can_fully_expand = false;
                        evaluated_strings.push(None);
                    } else {
                        evaluated_strings.push(Some(strings));
                    }
                }
            }
        }

        if !can_fully_expand {
            return self.interner().template_literal(normalized_spans);
        }

        // Check if we can fully evaluate to a union of literals
        let mut combinations = vec![String::new()];

        for (i, span) in span_list.iter().enumerate() {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.interner().resolve_atom_ref(*atom);
                    for combo in &mut combinations {
                        combo.push_str(text.as_ref());
                    }
                }
                TemplateSpan::Type(_) => {
                    let string_values = evaluated_strings[i]
                        .as_ref()
                        .expect("Type spans always have evaluated values at matching index");
                    let new_size = combinations.len() * string_values.len();

                    // Pre-allocate to minimize reallocations during Cartesian product
                    let mut new_combinations = Vec::with_capacity(new_size);
                    for combo in &combinations {
                        for value in string_values {
                            // OPTIMIZATION: Reserve exact capacity for the new string
                            let mut new_combo = String::with_capacity(combo.len() + value.len());
                            new_combo.push_str(combo);
                            new_combo.push_str(value);
                            new_combinations.push(new_combo);
                        }
                    }
                    combinations = new_combinations;
                }
            }
        }

        // Convert combinations to union of literal strings
        if combinations.is_empty() {
            return TypeId::NEVER;
        }

        let literal_types: Vec<TypeId> = combinations
            .iter()
            .map(|s| self.interner().literal_string(s))
            .collect();

        if literal_types.len() == 1 {
            literal_types[0]
        } else {
            self.interner().union(literal_types)
        }
    }

    /// Extract string representations from a type.
    /// Handles string, number, boolean, and bigint literals, converting them to their string form.
    /// For unions, extracts all members recursively.
    /// Maximum recursion depth for template literal evaluation to prevent stack overflow.
    const MAX_LITERAL_COUNT_DEPTH: u32 = 50;

    fn extract_literal_strings(&self, type_id: TypeId) -> Vec<String> {
        self.extract_literal_strings_impl(type_id, 0)
    }

    fn template_span_complexity_cardinality(&self, type_id: TypeId) -> Option<usize> {
        self.template_span_complexity_cardinality_impl(type_id, 0)
    }

    fn template_span_complexity_cardinality_impl(
        &self,
        type_id: TypeId,
        depth: u32,
    ) -> Option<usize> {
        if depth > Self::MAX_LITERAL_COUNT_DEPTH {
            return None;
        }

        if type_id == TypeId::BOOLEAN {
            return Some(2);
        }
        if type_id == TypeId::BOOLEAN_TRUE
            || type_id == TypeId::BOOLEAN_FALSE
            || type_id == TypeId::NULL
            || type_id == TypeId::UNDEFINED
            || type_id == TypeId::VOID
        {
            return Some(1);
        }
        if type_id.is_intrinsic() {
            return None;
        }

        match self.interner().lookup(type_id) {
            Some(TypeData::Literal(_)) | Some(TypeData::StringIntrinsic { .. }) => Some(1),
            Some(TypeData::Enum(_, structural_type)) => {
                self.template_span_complexity_cardinality_impl(structural_type, depth + 1)
            }
            Some(TypeData::Union(members_id)) => {
                let members = self.interner().type_list(members_id);
                let mut count = 0usize;
                for &member in members.iter() {
                    count = count.checked_add(
                        self.template_span_complexity_cardinality_impl(member, depth + 1)?,
                    )?;
                }
                Some(count)
            }
            Some(TypeData::TemplateLiteral(spans_id)) => {
                let spans = self.interner().template_list(spans_id);
                let mut total = 1usize;
                for span in spans.iter() {
                    let span_count = match span {
                        TemplateSpan::Text(_) => 1,
                        TemplateSpan::Type(type_id) => self
                            .template_span_complexity_cardinality_impl(*type_id, depth + 1)
                            .unwrap_or(1),
                    };
                    total = total.saturating_mul(span_count);
                }
                Some(total)
            }
            _ => None,
        }
    }

    /// Internal implementation with depth tracking.
    fn extract_literal_strings_impl(&self, type_id: TypeId, depth: u32) -> Vec<String> {
        // Prevent infinite recursion in deeply nested union types
        if depth > Self::MAX_LITERAL_COUNT_DEPTH {
            return Vec::new(); // Abort - too deep
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(type_id) {
            let members = self.interner().type_list(members);
            let mut result = Vec::new();
            for &member in members.iter() {
                let strings = self.extract_literal_strings_impl(member, depth + 1);
                if strings.is_empty() {
                    // Union contains a non-stringifiable type
                    return Vec::new();
                }
                result.extend(strings);
            }
            result
        } else if let Some(TypeData::Literal(lit)) = self.interner().lookup(type_id) {
            match lit {
                LiteralValue::String(atom) => {
                    vec![self.interner().resolve_atom_ref(atom).to_string()]
                }
                LiteralValue::Number(n) => {
                    // Convert number to string matching JavaScript's Number::toString(10)
                    // ECMAScript spec: use scientific notation if |x| < 10^-6 or |x| >= 10^21
                    let n_val = n.0;
                    let abs_val = n_val.abs();

                    tracing::trace!(
                        number = n_val,
                        abs_val = abs_val,
                        "extract_literal_strings: converting number to string"
                    );

                    if n_val == 0.0 {
                        vec!["0".to_string()]
                    } else if !(1e-6..1e21).contains(&abs_val) {
                        // Use scientific notation (Rust adds sign for negative exponents, but not positive)
                        let mut s = format!("{n_val:e}");
                        // Rust outputs "1e-7" for 1e-7 (good) but "1e21" instead of "1e+21" for 1e21
                        // We need to add "+" to positive exponents
                        if s.contains("e") && !s.contains("e-") && !s.contains("e+") {
                            let parts: Vec<&str> = s.split('e').collect();
                            if parts.len() == 2 {
                                s = format!("{}e+{}", parts[0], parts[1]);
                            }
                        }
                        tracing::trace!(result = %s, "extract_literal_strings: scientific notation");
                        vec![s]
                    } else if n_val.fract() == 0.0 && abs_val < 1e15 {
                        // Integer-like number - avoid scientific notation
                        let s = (n_val as i64).to_string();
                        tracing::trace!(result = %s, "extract_literal_strings: integer-like");
                        vec![s]
                    } else {
                        // Fixed-point notation
                        let s = format!("{n_val}");
                        tracing::trace!(result = %s, "extract_literal_strings: fixed-point");
                        vec![s]
                    }
                }
                LiteralValue::Boolean(b) => {
                    vec![if b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }]
                }
                LiteralValue::BigInt(atom) => {
                    // BigInt literals are stored without the 'n' suffix
                    vec![self.interner().resolve_atom_ref(atom).to_string()]
                }
            }
        } else if let Some(TypeData::Enum(_, structural_type)) = self.interner().lookup(type_id) {
            // Enum member types wrap a literal (e.g., AnimalType.cat wraps "cat").
            // Delegate to the structural type to extract the underlying literal string.
            self.extract_literal_strings_impl(structural_type, depth + 1)
        } else {
            // Not a literal type - can't extract string
            Vec::new()
        }
    }
}
