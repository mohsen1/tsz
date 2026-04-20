use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(crate) fn match_template_literal_string(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut pos = 0;
        let mut index = 0;

        while index < pattern.len() {
            match pattern[index] {
                TemplateSpan::Text(text) => {
                    let text_value = self.interner().resolve_atom_ref(text);
                    let text_value = text_value.as_ref();
                    if !source[pos..].starts_with(text_value) {
                        return false;
                    }
                    pos += text_value.len();
                    index += 1;
                }
                TemplateSpan::Type(type_id) => {
                    let next_text = pattern[index + 1..].iter().find_map(|span| match span {
                        TemplateSpan::Text(text) => Some(*text),
                        TemplateSpan::Type(_) => None,
                    });
                    // Check if the next span is another Type (no text separator).
                    // In tsc, consecutive infer types like `${infer C}${infer R}`
                    // require the first infer to capture exactly 1 character.
                    // Without this, `""` would match both infers with empty strings,
                    // causing infinite recursion in tail-recursive conditional types
                    // like `type GetChars<S> = S extends `${infer C}${infer R}` ? ... : ...`.
                    let next_is_type = pattern
                        .get(index + 1)
                        .is_some_and(|s| matches!(s, TemplateSpan::Type(_)));
                    let end = if let Some(next_text) = next_text {
                        let next_value = self.interner().resolve_atom_ref(next_text);
                        // When there are no more Type (infer) spans after the next text
                        // separator, the text must match at the END of the remaining string.
                        // Use rfind (last occurrence) so the infer captures greedily.
                        // Example: `${infer R} ` matching "hello  " → R = "hello " (rfind)
                        //
                        // When more Type spans follow, use find (first occurrence) so each
                        // infer captures minimally, leaving content for later infers.
                        // Example: `${infer A}.${infer B}` matching "a.b.c" → A = "a" (find)
                        let has_more_types_after_separator = pattern[index + 1..]
                            .iter()
                            .skip_while(|s| !matches!(s, TemplateSpan::Text(_)))
                            .skip(1) // skip the text separator itself
                            .any(|s| matches!(s, TemplateSpan::Type(_)));
                        let search_fn = if has_more_types_after_separator {
                            str::find
                        } else {
                            str::rfind
                        };
                        match search_fn(&source[pos..], next_value.as_ref()) {
                            Some(offset) => pos + offset,
                            None => return false,
                        }
                    } else if next_is_type {
                        // Consecutive infer types: capture exactly 1 character.
                        // This matches tsc behavior where `${infer C}${infer R}`
                        // splits "AB" as C="A", R="B" and fails on "".
                        if pos >= source.len() {
                            return false; // Not enough characters for this infer
                        }
                        // Find the next char boundary (for UTF-8 safety)

                        source[pos..]
                            .char_indices()
                            .nth(1)
                            .map_or(source.len(), |(idx, _)| pos + idx)
                    } else {
                        source.len()
                    };

                    let captured = &source[pos..end];
                    pos = end;
                    let captured_type = self.interner().literal_string(captured);

                    if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                        if !self.bind_infer(&info, captured_type, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(captured_type, type_id) {
                        return false;
                    }
                    index += 1;
                }
            }
        }

        pos == source.len()
    }

    /// Match template literal spans against a pattern.
    pub(crate) fn match_template_literal_spans(
        &self,
        source: TypeId,
        source_spans: &[TemplateSpan],
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans.len() == 1
            && let TemplateSpan::Type(type_id) = pattern_spans[0]
        {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                let inferred = if source_spans
                    .iter()
                    .all(|span| matches!(span, TemplateSpan::Type(_)))
                {
                    TypeId::STRING
                } else {
                    source
                };
                return self.bind_infer(&info, inferred, bindings, checker);
            }
            return checker.is_subtype_of(source, type_id);
        }

        if source_spans.len() != pattern_spans.len() {
            return false;
        }

        for (source_span, pattern_span) in source_spans.iter().zip(pattern_spans.iter()) {
            match pattern_span {
                TemplateSpan::Text(text) => match source_span {
                    TemplateSpan::Text(source_text) if source_text == text => {}
                    _ => return false,
                },
                TemplateSpan::Type(type_id) => {
                    let inferred = match source_span {
                        TemplateSpan::Text(text) => {
                            let text_value = self.interner().resolve_atom_ref(*text);
                            self.interner().literal_string(text_value.as_ref())
                        }
                        TemplateSpan::Type(source_type) => *source_type,
                    };
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(*type_id) {
                        if !self.bind_infer(&info, inferred, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(inferred, *type_id) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Match a string type against a template literal pattern.
    pub(crate) fn match_template_literal_string_type(
        &self,
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans
            .iter()
            .any(|span| matches!(span, TemplateSpan::Text(_)))
        {
            return false;
        }

        for span in pattern_spans {
            if let TemplateSpan::Type(type_id) = span {
                if let Some(TypeData::Infer(info)) = self.interner().lookup(*type_id) {
                    if !self.bind_infer(&info, TypeId::STRING, bindings, checker) {
                        return false;
                    }
                } else if !checker.is_subtype_of(TypeId::STRING, *type_id) {
                    return false;
                }
            }
        }

        true
    }
}
