//! Template literal type evaluation.
//!
//! Handles TypeScript template literal types like "hello ${T}".

use crate::relations::subtype::TypeResolver;
use crate::types::{LiteralValue, TemplateLiteralId, TemplateSpan, TypeData, TypeId};
use rustc_hash::FxHashMap;

use super::super::evaluate::TypeEvaluator;

#[derive(Clone)]
struct TemplateSpanExpansion {
    cardinality: Option<usize>,
    strings: Vec<String>,
}

type TemplateSpanExpansionCache = FxHashMap<(TypeId, u32), TemplateSpanExpansion>;

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
        let mut span_expansion_cache = TemplateSpanExpansionCache::default();

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
                    let expansion =
                        self.template_span_expansion(evaluated, &mut span_expansion_cache);

                    if let Some(span_count) = expansion.cardinality {
                        total_combinations = total_combinations.saturating_mul(span_count);
                        if total_combinations >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                            self.interner().mark_union_too_complex();
                            return TypeId::STRING;
                        }
                    }

                    if expansion.cardinality.is_none() && !expansion.strings.is_empty() {
                        total_combinations =
                            total_combinations.saturating_mul(expansion.strings.len());
                        if total_combinations >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                            self.interner().mark_union_too_complex();
                            return TypeId::STRING;
                        }
                    }

                    if expansion.strings.is_empty() {
                        // Contains non-literal types. Keep scanning the remaining
                        // spans first so mixed template unions can still trip TS2590.
                        can_fully_expand = false;
                        evaluated_strings.push(None);
                    } else {
                        evaluated_strings.push(Some(expansion.strings));
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
            .into_iter()
            .map(|s| self.interner().literal_string(&s))
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

    fn template_span_expansion(
        &self,
        type_id: TypeId,
        cache: &mut TemplateSpanExpansionCache,
    ) -> TemplateSpanExpansion {
        self.template_span_expansion_impl(type_id, 0, cache)
    }

    fn template_span_expansion_impl(
        &self,
        type_id: TypeId,
        depth: u32,
        cache: &mut TemplateSpanExpansionCache,
    ) -> TemplateSpanExpansion {
        if depth > Self::MAX_LITERAL_COUNT_DEPTH {
            return TemplateSpanExpansion {
                cardinality: None,
                strings: Vec::new(),
            };
        }

        let cache_key = (type_id, depth);
        if let Some(expansion) = cache.get(&cache_key) {
            return expansion.clone();
        }

        let expansion = if type_id == TypeId::BOOLEAN {
            TemplateSpanExpansion {
                cardinality: Some(2),
                strings: Vec::new(),
            }
        } else if type_id == TypeId::BOOLEAN_TRUE {
            TemplateSpanExpansion {
                cardinality: Some(1),
                strings: vec!["true".to_string()],
            }
        } else if type_id == TypeId::BOOLEAN_FALSE {
            TemplateSpanExpansion {
                cardinality: Some(1),
                strings: vec!["false".to_string()],
            }
        } else if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED || type_id == TypeId::VOID
        {
            TemplateSpanExpansion {
                cardinality: Some(1),
                strings: Vec::new(),
            }
        } else if type_id.is_intrinsic() {
            TemplateSpanExpansion {
                cardinality: None,
                strings: Vec::new(),
            }
        } else {
            match self.interner().lookup(type_id) {
                Some(TypeData::Literal(lit)) => TemplateSpanExpansion {
                    cardinality: Some(1),
                    strings: Self::literal_template_string(self, lit)
                        .into_iter()
                        .collect(),
                },
                Some(TypeData::StringIntrinsic { .. }) => TemplateSpanExpansion {
                    cardinality: Some(1),
                    strings: Vec::new(),
                },
                Some(TypeData::Enum(_, structural_type)) => {
                    self.template_span_expansion_impl(structural_type, depth + 1, cache)
                }
                Some(TypeData::Union(members_id)) => {
                    let members = self.interner().type_list(members_id);
                    let mut count = 0usize;
                    let mut count_known = true;
                    let mut strings = Vec::with_capacity(members.len());
                    let mut all_stringifiable = true;

                    for &member in members.iter() {
                        let expansion = self.template_span_expansion_impl(member, depth + 1, cache);
                        if let Some(cardinality) = expansion.cardinality {
                            if let Some(next_count) = count.checked_add(cardinality) {
                                count = next_count;
                            } else {
                                count_known = false;
                            }
                        } else {
                            count_known = false;
                        }

                        if expansion.strings.is_empty() {
                            all_stringifiable = false;
                        } else if all_stringifiable {
                            strings.extend(expansion.strings);
                        }
                    }

                    TemplateSpanExpansion {
                        cardinality: count_known.then_some(count),
                        strings: if all_stringifiable {
                            strings
                        } else {
                            Vec::new()
                        },
                    }
                }
                Some(TypeData::TemplateLiteral(spans_id)) => {
                    let spans = self.interner().template_list(spans_id);
                    let mut total = 1usize;
                    for span in spans.iter() {
                        let span_count = match span {
                            TemplateSpan::Text(_) => 1,
                            TemplateSpan::Type(type_id) => self
                                .template_span_expansion_impl(*type_id, depth + 1, cache)
                                .cardinality
                                .unwrap_or(1),
                        };
                        total = total.saturating_mul(span_count);
                    }
                    TemplateSpanExpansion {
                        cardinality: Some(total),
                        strings: Vec::new(),
                    }
                }
                _ => TemplateSpanExpansion {
                    cardinality: None,
                    strings: Vec::new(),
                },
            }
        };

        cache.insert(cache_key, expansion.clone());
        expansion
    }

    fn literal_template_string(&self, lit: LiteralValue) -> Option<String> {
        match lit {
            LiteralValue::String(atom) => Some(self.interner().resolve_atom_ref(atom).to_string()),
            LiteralValue::Number(n) => Some(crate::utils::js_number_to_string(n.0).into_owned()),
            LiteralValue::Boolean(b) => Some(if b { "true" } else { "false" }.to_string()),
            LiteralValue::BigInt(atom) => {
                // BigInt literals are stored without the 'n' suffix.
                Some(self.interner().resolve_atom_ref(atom).to_string())
            }
        }
    }
}
