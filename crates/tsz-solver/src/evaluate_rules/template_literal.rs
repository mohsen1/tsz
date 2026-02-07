//! Template literal type evaluation.
//!
//! Handles TypeScript's template literal types: `\`hello ${T}\``

use crate::subtype::TypeResolver;
use crate::types::*;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a template literal type: `hello${T}world`
    ///
    /// Template literals evaluate to a union of all possible literal string combinations.
    /// For example: `get${K}` where K = "a" | "b" evaluates to "geta" | "getb"
    /// Multiple unions compute a Cartesian product: `${"a"|"b"}-${"x"|"y"}` => "a-x"|"a-y"|"b-x"|"b-y"
    pub fn evaluate_template_literal(&mut self, spans: TemplateLiteralId) -> TypeId {
        use crate::TEMPLATE_LITERAL_EXPANSION_LIMIT;

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

        // Pre-compute the total number of combinations to check against the limit
        // This avoids doing expensive work if we're going to exceed the limit anyway
        let mut total_combinations: usize = 1;
        for span in span_list.iter() {
            if let TemplateSpan::Type(type_id) = span {
                let evaluated = self.evaluate(*type_id);
                let span_count = self.count_literal_members(evaluated);
                if span_count == 0 {
                    // Contains non-literal types, can't fully evaluate
                    return self.interner().template_literal(span_list.to_vec());
                }
                total_combinations = total_combinations.saturating_mul(span_count);
                if total_combinations > TEMPLATE_LITERAL_EXPANSION_LIMIT {
                    // Would exceed limit - keep the unexpanded template literal type
                    // instead of widening to string. This is sound: subtyping uses
                    // the backtracking matcher to check Literal <: TemplateLiteral
                    // without expansion, so "z" won't incorrectly satisfy the template.
                    return self.interner().template_literal(span_list.to_vec());
                }
            }
        }

        // Check if we can fully evaluate to a union of literals
        let mut combinations = vec![String::new()];

        for span in span_list.iter() {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.interner().resolve_atom_ref(*atom).to_string();
                    for combo in &mut combinations {
                        combo.push_str(&text);
                    }
                }
                TemplateSpan::Type(type_id) => {
                    let evaluated = self.evaluate(*type_id);

                    tracing::trace!(
                        type_id = type_id.0,
                        "evaluate_template_literal: evaluating type span"
                    );

                    // Try to extract string representations from the type
                    let string_values = self.extract_literal_strings(evaluated);

                    tracing::trace!(
                        string_count = string_values.len(),
                        strings = ?string_values,
                        "evaluate_template_literal: extracted strings"
                    );

                    if string_values.is_empty() {
                        // Can't evaluate this type - return template literal as-is
                        tracing::trace!(
                            "evaluate_template_literal: no strings extracted, returning template literal"
                        );
                        return self.interner().template_literal(span_list.to_vec());
                    }

                    // Check if expansion would exceed limit
                    let new_size = combinations.len().saturating_mul(string_values.len());
                    if new_size > TEMPLATE_LITERAL_EXPANSION_LIMIT {
                        // Would exceed limit - keep the unexpanded template literal type
                        // for sound subtyping (backtracking matcher handles this)
                        return self.interner().template_literal(span_list.to_vec());
                    }

                    // Compute Cartesian product
                    let mut new_combinations = Vec::with_capacity(new_size);
                    for combo in &combinations {
                        for value in &string_values {
                            new_combinations.push(format!("{}{}", combo, value));
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

    /// Maximum recursion depth for counting literal members to prevent stack overflow
    const MAX_LITERAL_COUNT_DEPTH: u32 = 50;

    /// Count the number of literal members that can be converted to strings.
    /// Returns 0 if the type contains non-literal types that cannot be stringified.
    pub fn count_literal_members(&self, type_id: TypeId) -> usize {
        self.count_literal_members_impl(type_id, 0)
    }

    /// Internal implementation with depth tracking.
    fn count_literal_members_impl(&self, type_id: TypeId, depth: u32) -> usize {
        // Prevent infinite recursion in deeply nested union types
        if depth > Self::MAX_LITERAL_COUNT_DEPTH {
            return 0; // Abort - too deep
        }

        if let Some(TypeKey::Union(members)) = self.interner().lookup(type_id) {
            let members = self.interner().type_list(members);
            let mut count = 0;
            for &member in members.iter() {
                let member_count = self.count_literal_members_impl(member, depth + 1);
                if member_count == 0 {
                    return 0;
                }
                count += member_count;
            }
            count
        } else if let Some(TypeKey::Literal(_)) = self.interner().lookup(type_id) {
            1
        } else if type_id == TypeId::STRING
            || type_id == TypeId::NUMBER
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::BIGINT
        {
            // Primitive types can't be fully enumerated
            0
        } else {
            0
        }
    }

    /// Extract string representations from a type.
    /// Handles string, number, boolean, and bigint literals, converting them to their string form.
    /// For unions, extracts all members recursively.
    pub fn extract_literal_strings(&self, type_id: TypeId) -> Vec<String> {
        self.extract_literal_strings_impl(type_id, 0)
    }

    /// Internal implementation with depth tracking.
    fn extract_literal_strings_impl(&self, type_id: TypeId, depth: u32) -> Vec<String> {
        // Prevent infinite recursion in deeply nested union types
        if depth > Self::MAX_LITERAL_COUNT_DEPTH {
            return Vec::new(); // Abort - too deep
        }

        if let Some(TypeKey::Union(members)) = self.interner().lookup(type_id) {
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
        } else if let Some(TypeKey::Literal(lit)) = self.interner().lookup(type_id) {
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

                    if abs_val < 1e-6 || abs_val >= 1e21 {
                        // Use scientific notation (Rust adds sign for negative exponents, but not positive)
                        let mut s = format!("{:e}", n_val);
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
                        let s = format!("{}", n_val as i64);
                        tracing::trace!(result = %s, "extract_literal_strings: integer-like");
                        vec![s]
                    } else {
                        // Fixed-point notation
                        let s = format!("{}", n_val);
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
        } else {
            // Not a literal type - can't extract string
            Vec::new()
        }
    }
}
