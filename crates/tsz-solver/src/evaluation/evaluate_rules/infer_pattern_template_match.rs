//! Template-literal infer-pattern matching helpers.

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{IntrinsicKind, LiteralValue, TemplateSpan, TypeData, TypeId, TypeParamInfo};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn parse_template_number_capture(&self, captured: &str) -> Option<TypeId> {
        let value = if let Some(digits) = captured.strip_prefix("0x") {
            u64::from_str_radix(digits, 16).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0X") {
            u64::from_str_radix(digits, 16).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0o") {
            u64::from_str_radix(digits, 8).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0O") {
            u64::from_str_radix(digits, 8).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0b") {
            u64::from_str_radix(digits, 2).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0B") {
            u64::from_str_radix(digits, 2).ok().map(|n| n as f64)?
        } else {
            captured.parse::<f64>().ok()?
        };

        if !value.is_finite() {
            return None;
        }

        let literal = self.interner().literal_number(value);
        let round_trips = match value {
            v if v.fract() == 0.0 && v.abs() < 1e15 => (v as i64).to_string() == captured,
            v => format!("{v}") == captured,
        };
        Some(if round_trips { literal } else { TypeId::NUMBER })
    }

    fn parse_template_bigint_capture(&self, captured: &str) -> Option<TypeId> {
        let (negative, digits) = captured
            .strip_prefix('-')
            .map_or((false, captured), |rest| (true, rest));
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        Some(self.interner().literal_bigint_with_sign(negative, digits))
    }

    fn template_capture_for_constraint(
        &self,
        captured: &str,
        captured_type: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        if checker.is_subtype_of(captured_type, constraint) {
            return Some(captured_type);
        }

        match self.interner().lookup(constraint) {
            Some(TypeData::Intrinsic(IntrinsicKind::Number)) => self
                .parse_template_number_capture(captured)
                .filter(|&ty| checker.is_subtype_of(ty, constraint)),
            Some(TypeData::Intrinsic(IntrinsicKind::Bigint)) => self
                .parse_template_bigint_capture(captured)
                .filter(|&ty| checker.is_subtype_of(ty, constraint)),
            Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => match captured {
                "true" => Some(self.interner().literal_boolean(true)),
                "false" => Some(self.interner().literal_boolean(false)),
                _ => None,
            },
            Some(TypeData::Intrinsic(IntrinsicKind::Null)) if captured == "null" => {
                Some(TypeId::NULL)
            }
            Some(TypeData::Intrinsic(IntrinsicKind::Undefined)) if captured == "undefined" => {
                Some(TypeId::UNDEFINED)
            }
            Some(TypeData::Union(members_id)) => {
                let members = self.interner().type_list(members_id);
                members.iter().find_map(|&member| {
                    self.template_capture_for_constraint(captured, captured_type, member, checker)
                        .filter(|&ty| checker.is_subtype_of(ty, constraint))
                })
            }
            _ => None,
        }
    }

    fn bind_template_infer_capture(
        &self,
        info: &TypeParamInfo,
        captured: &str,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let captured_type = self.interner().literal_string(captured);
        let inferred = if let Some(constraint) = info.constraint {
            let Some(converted) =
                self.template_capture_for_constraint(captured, captured_type, constraint, checker)
            else {
                return false;
            };
            converted
        } else {
            captured_type
        };

        self.bind_infer(info, inferred, bindings, checker)
    }

    /// Match a template literal string against a pattern.
    pub(crate) fn match_template_literal_string(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        self.match_template_literal_string_from(source, pattern, 0, 0, bindings, checker)
    }

    fn match_template_segment_prefix(
        &self,
        source: &str,
        pos: usize,
        type_id: TypeId,
    ) -> Option<usize> {
        match self.interner().lookup(type_id)? {
            TypeData::Literal(LiteralValue::String(atom)) => {
                let text = self.interner().resolve_atom(atom);
                source
                    .get(pos..)?
                    .starts_with(&text)
                    .then_some(pos + text.len())
            }
            TypeData::Union(list_id) => self
                .interner()
                .type_list(list_id)
                .iter()
                .find_map(|member| self.match_template_segment_prefix(source, pos, *member)),
            TypeData::TemplateLiteral(template_id) => {
                let spans = self.interner().template_list(template_id);
                let mut text = String::new();
                for span in spans.iter() {
                    let TemplateSpan::Text(atom) = span else {
                        return None;
                    };
                    text.push_str(&self.interner().resolve_atom(*atom));
                }
                source
                    .get(pos..)?
                    .starts_with(&text)
                    .then_some(pos + text.len())
            }
            _ => None,
        }
    }

    fn is_template_infer_span(&self, span: Option<&TemplateSpan>) -> bool {
        span.is_some_and(|span| {
            matches!(span, TemplateSpan::Type(type_id) if matches!(self.interner().lookup(*type_id), Some(TypeData::Infer(_))))
        })
    }

    fn next_char_end(source: &str, pos: usize) -> Option<usize> {
        if pos >= source.len() {
            return None;
        }
        Some(
            source[pos..]
                .char_indices()
                .nth(1)
                .map_or(source.len(), |(idx, _)| pos + idx),
        )
    }

    fn candidate_template_capture_ends(
        &self,
        source: &str,
        pos: usize,
        pattern: &[TemplateSpan],
        index: usize,
    ) -> Vec<usize> {
        if index + 1 >= pattern.len() {
            return vec![source.len()];
        }

        if self.is_template_infer_span(pattern.get(index))
            && matches!(
                pattern.get(index + 1),
                Some(TemplateSpan::Type(
                    TypeId::STRING | TypeId::ANY | TypeId::UNKNOWN
                ))
            )
        {
            if self.is_template_infer_span(pattern.get(index + 2)) {
                return Self::next_char_end(source, pos).into_iter().collect();
            }

            return Self::next_char_end(source, pos)
                .or(Some(pos))
                .into_iter()
                .collect();
        }

        if pattern
            .get(index + 1)
            .is_some_and(|s| matches!(s, TemplateSpan::Type(type_id) if matches!(self.interner().lookup(*type_id), Some(TypeData::Infer(_)))))
        {
            return Self::next_char_end(source, pos).into_iter().collect();
        }

        if let Some(next_text) = pattern[index + 1..].iter().find_map(|span| match span {
            TemplateSpan::Text(text) => Some(*text),
            TemplateSpan::Type(_) => None,
        }) {
            let next_value = self.interner().resolve_atom_ref(next_text);
            let remaining = &source[pos..];
            return remaining
                .match_indices(next_value.as_ref())
                .map(|(offset, _)| pos + offset)
                .collect();
        }

        source[pos..]
            .char_indices()
            .map(|(offset, _)| pos + offset)
            .chain(std::iter::once(source.len()))
            .collect()
    }

    /// Match an intrinsic-typed span at position `pos` in the infer-pattern path.
    ///
    /// Returns `Some(true/false)` when the span is a recognized intrinsic kind
    /// (number, bigint, boolean, null, undefined) and dispatches length-aware
    /// matching for it.  Returns `None` for wildcard intrinsics (string/any/
    /// unknown) so the caller falls through to generic handling.
    fn match_intrinsic_span_from(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        pos: usize,
        index: usize,
        type_id: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<bool> {
        use crate::relations::subtype::rules::literals::{
            find_integer_length, find_number_length, is_valid_number,
        };

        let remaining = &source[pos..];

        match self.interner().lookup(type_id)? {
            TypeData::Intrinsic(kind) => match kind {
                IntrinsicKind::Number => {
                    let num_len = find_number_length(remaining);
                    if num_len == 0 {
                        return Some(false);
                    }
                    // Try shortest valid number first — matches tsc's non-greedy
                    // behaviour for ambiguous infer+number patterns.
                    for len in 1..=num_len {
                        if is_valid_number(&remaining[..len])
                            && self.match_template_literal_string_from(
                                source,
                                pattern,
                                pos + len,
                                index + 1,
                                bindings,
                                checker,
                            )
                        {
                            return Some(true);
                        }
                    }
                    Some(false)
                }
                IntrinsicKind::Bigint => {
                    let int_len = find_integer_length(remaining);
                    if int_len == 0 {
                        return Some(false);
                    }
                    // Try shortest valid integer first — consistent with tsc.
                    for len in 1..=int_len {
                        if self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + len,
                            index + 1,
                            bindings,
                            checker,
                        ) {
                            return Some(true);
                        }
                    }
                    Some(false)
                }
                IntrinsicKind::Boolean => {
                    if remaining.starts_with("true")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 4,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return Some(true);
                    }
                    if remaining.starts_with("false")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 5,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return Some(true);
                    }
                    Some(false)
                }
                IntrinsicKind::Null => {
                    if remaining.starts_with("null")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 4,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        Some(true)
                    } else {
                        Some(false)
                    }
                }
                IntrinsicKind::Undefined => {
                    if remaining.starts_with("undefined")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 9,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        Some(true)
                    } else {
                        Some(false)
                    }
                }
                // Wildcards and other intrinsics fall through to generic handling.
                _ => None,
            },
            _ => None,
        }
    }

    fn match_template_literal_string_from(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        pos: usize,
        index: usize,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if index == pattern.len() {
            return pos == source.len();
        }

        match pattern[index] {
            TemplateSpan::Text(text) => {
                let text_value = self.interner().resolve_atom_ref(text);
                let text_value = text_value.as_ref();
                if !source[pos..].starts_with(text_value) {
                    return false;
                }
                self.match_template_literal_string_from(
                    source,
                    pattern,
                    pos + text_value.len(),
                    index + 1,
                    bindings,
                    checker,
                )
            }
            TemplateSpan::Type(type_id) => {
                if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                    for end in self.candidate_template_capture_ends(source, pos, pattern, index) {
                        let mut next_bindings = bindings.clone();
                        let captured = &source[pos..end];
                        if !self.bind_template_infer_capture(
                            &info,
                            captured,
                            &mut next_bindings,
                            checker,
                        ) {
                            continue;
                        }
                        if self.match_template_literal_string_from(
                            source,
                            pattern,
                            end,
                            index + 1,
                            &mut next_bindings,
                            checker,
                        ) {
                            *bindings = next_bindings;
                            return true;
                        }
                    }
                    return false;
                }

                if let Some(next_pos) = self.match_template_segment_prefix(source, pos, type_id) {
                    return self.match_template_literal_string_from(
                        source,
                        pattern,
                        next_pos,
                        index + 1,
                        bindings,
                        checker,
                    );
                }

                if let Some(result) = self.match_intrinsic_span_from(
                    source, pattern, pos, index, type_id, bindings, checker,
                ) {
                    return result;
                }

                for end in self.candidate_template_capture_ends(source, pos, pattern, index) {
                    let captured = &source[pos..end];
                    let captured_type = self.interner().literal_string(captured);
                    if self
                        .template_capture_for_constraint(captured, captured_type, type_id, checker)
                        .is_some()
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            end,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Capture value for a bare single-placeholder `` `${infer V}` `` pattern
    /// matched against a template-literal `source`.
    ///
    /// tsc captures the whole source type (`getStringLikeTypeForType` of the
    /// placeholder) rather than widening to `string`: inferring `` `${infer V}` ``
    /// from `` `${number}` `` yields `` `${number}` ``, not `string`.
    ///
    /// When the infer variable carries an `extends` constraint, this mirrors
    /// tsc's `getInferredType` fallback: if the captured template type isn't
    /// assignable to the constraint, fall back to the constraint itself, but
    /// only when the source is assignable to the constraint's string form
    /// (`` `${C}` ``) — i.e. when the conditional's post-inference `extends`
    /// re-check would still succeed. Otherwise the match fails and the
    /// conditional takes its false branch.
    fn single_placeholder_template_capture(
        &self,
        source: TypeId,
        info: &TypeParamInfo,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        let Some(constraint) = info.constraint else {
            return Some(source);
        };
        if checker.is_subtype_of(source, constraint) {
            return Some(source);
        }
        let constraint_string_form = self
            .interner()
            .template_literal(vec![TemplateSpan::Type(constraint)]);
        checker
            .is_subtype_of(source, constraint_string_form)
            .then_some(constraint)
    }

    /// Match template literal spans against a pattern.
    ///
    /// A captured `infer` slot always lands in the string domain: when the
    /// source segment is a non-string-domain type (e.g. `number`, `bigint`),
    /// it is wrapped as a single-placeholder template `` `${T}` `` before
    /// being bound. This mirrors tsc's `getStringLikeTypeForType`.
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
                let Some(inferred) =
                    self.single_placeholder_template_capture(source, &info, checker)
                else {
                    return false;
                };
                return self.bind_infer(&info, inferred, bindings, checker);
            }
            return checker.is_subtype_of(source, type_id);
        }

        if source_spans.len() == pattern_spans.len()
            && source_spans
                .iter()
                .zip(pattern_spans.iter())
                .all(|(s, p)| s.is_text() == p.is_text())
        {
            return self.match_template_literal_spans_aligned(
                source_spans,
                pattern_spans,
                bindings,
                checker,
            );
        }

        // Fall back to general cursor-based matching that allows source text
        // spans to be split to align with pattern texts and infer slots.
        self.match_template_literal_spans_general(source_spans, pattern_spans, bindings, checker)
    }

    /// Structural match: pattern and source share the same span shape
    /// (text-vs-type alignment), so each pattern span pairs with the
    /// corresponding source span. Text spans must match exactly; type spans
    /// bind via `bind_infer` with the source segment promoted to a
    /// string-domain type (see `string_like_type_for_type`).
    fn match_template_literal_spans_aligned(
        &self,
        source_spans: &[TemplateSpan],
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
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
                        TemplateSpan::Type(source_type) => {
                            crate::type_queries::extended::string_like_type_for_type(
                                self.interner(),
                                *source_type,
                            )
                        }
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

    /// General template-literal pattern matcher: walks both source and
    /// pattern with a cursor that can split source text spans to align with
    /// the pattern's text and infer spans. Mirrors tsc's
    /// `inferFromLiteralPartsToTemplateLiteralType` for the `TemplateLiteral`
    /// source case where structural span alignment does not hold.
    ///
    /// Captures are always promoted to string-domain types: a source `Type`
    /// span captured into an `infer` slot is wrapped as `` `${T}` `` unless
    /// the source type is already a string subtype.
    fn match_template_literal_spans_general(
        &self,
        source_spans: &[TemplateSpan],
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        // Pre-resolve source text atoms once: `resolve_atom_ref` acquires a
        // RwLock and bumps an Arc refcount, and the cursor revisits the same
        // source text span across consume/find/capture operations.
        let source_texts: Vec<Option<std::sync::Arc<str>>> = source_spans
            .iter()
            .map(|s| match s {
                TemplateSpan::Text(atom) => Some(self.interner().resolve_atom_ref(*atom)),
                TemplateSpan::Type(_) => None,
            })
            .collect();

        // Cursor: `(s_idx, s_offset)`. `s_offset` is only meaningful when
        // `source_spans[s_idx]` is a `Text` span — `Type` spans are consumed
        // atomically with `s_offset == 0`.
        let mut s_idx: usize = 0;
        let mut s_offset: usize = 0;

        for (p_idx, pattern_span) in pattern_spans.iter().enumerate() {
            match *pattern_span {
                TemplateSpan::Text(text_atom) => {
                    let text_arc = self.interner().resolve_atom_ref(text_atom);
                    if !consume_source_text(
                        text_arc.as_ref(),
                        &mut s_idx,
                        &mut s_offset,
                        source_spans,
                        &source_texts,
                    ) {
                        return false;
                    }
                }
                TemplateSpan::Type(pattern_type) => {
                    let info = match self.interner().lookup(pattern_type) {
                        Some(TypeData::Infer(info)) => Some(info),
                        _ => None,
                    };

                    let next_anchor = pattern_spans.get(p_idx + 1).and_then(|s| match s {
                        TemplateSpan::Text(atom) => Some(*atom),
                        TemplateSpan::Type(_) => None,
                    });
                    let is_last_span = p_idx + 1 == pattern_spans.len();

                    let captured = if is_last_span {
                        let result = self.capture_source_between(
                            s_idx,
                            s_offset,
                            source_spans.len(),
                            0,
                            source_spans,
                            &source_texts,
                        );
                        s_idx = source_spans.len();
                        s_offset = 0;
                        result
                    } else if let Some(anchor_atom) = next_anchor {
                        let anchor_arc = self.interner().resolve_atom_ref(anchor_atom);
                        let Some((end_idx, end_offset)) = find_anchor_in_source(
                            anchor_arc.as_ref(),
                            s_idx,
                            s_offset,
                            source_spans,
                            &source_texts,
                        ) else {
                            return false;
                        };
                        let result = self.capture_source_between(
                            s_idx,
                            s_offset,
                            end_idx,
                            end_offset,
                            source_spans,
                            &source_texts,
                        );
                        s_idx = end_idx;
                        s_offset = end_offset;
                        result
                    } else {
                        // Two adjacent pattern `Type` spans
                        // (e.g. `${infer A}${infer B}`): give the first infer
                        // a single source `Type` segment when one is at the
                        // cursor, otherwise an empty string.
                        self.capture_one_source_type(
                            &mut s_idx,
                            &mut s_offset,
                            source_spans,
                            &source_texts,
                        )
                    };

                    if let Some(info) = info {
                        if !self.bind_infer(&info, captured, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(captured, pattern_type) {
                        return false;
                    }
                }
            }
        }

        source_fully_consumed(s_idx, s_offset, source_spans, &source_texts)
    }

    fn capture_source_between(
        &self,
        start_idx: usize,
        start_offset: usize,
        end_idx: usize,
        end_offset: usize,
        source_spans: &[TemplateSpan],
        source_texts: &[Option<std::sync::Arc<str>>],
    ) -> TypeId {
        let stop = end_idx.min(source_spans.len());
        let mut captured: Vec<TemplateSpan> =
            Vec::with_capacity(stop.saturating_sub(start_idx) + 1);
        let mut i = start_idx;
        let mut offset = start_offset;
        while i < stop {
            match source_spans[i] {
                TemplateSpan::Text(_) => {
                    let src = source_texts[i].as_ref().expect("text span").as_ref();
                    let slice = &src[offset..src.len()];
                    if !slice.is_empty() {
                        captured.push(TemplateSpan::Text(self.interner().intern_string(slice)));
                    }
                }
                TemplateSpan::Type(t) => {
                    captured.push(TemplateSpan::Type(t));
                }
            }
            i += 1;
            offset = 0;
        }
        if i == end_idx
            && end_idx < source_spans.len()
            && let TemplateSpan::Text(_) = source_spans[end_idx]
        {
            let src = source_texts[end_idx].as_ref().expect("text span").as_ref();
            if end_offset > offset {
                let slice = &src[offset..end_offset];
                if !slice.is_empty() {
                    captured.push(TemplateSpan::Text(self.interner().intern_string(slice)));
                }
            }
        }

        if captured.is_empty() {
            return self.interner().literal_string("");
        }
        // The template_literal builder collapses single-Text spans to a
        // string literal; it does NOT collapse single `${T}` for non-string
        // intrinsics, so the wrap-as-string-domain invariant is preserved.
        self.interner().template_literal(captured)
    }

    fn capture_one_source_type(
        &self,
        s_idx: &mut usize,
        s_offset: &mut usize,
        source_spans: &[TemplateSpan],
        source_texts: &[Option<std::sync::Arc<str>>],
    ) -> TypeId {
        while *s_idx < source_spans.len() {
            match source_spans[*s_idx] {
                TemplateSpan::Text(_) => {
                    let src = source_texts[*s_idx].as_ref().expect("text span").as_ref();
                    if *s_offset < src.len() {
                        return self.interner().literal_string("");
                    }
                    *s_idx += 1;
                    *s_offset = 0;
                }
                TemplateSpan::Type(t) => {
                    *s_idx += 1;
                    *s_offset = 0;
                    return crate::type_queries::extended::string_like_type_for_type(
                        self.interner(),
                        t,
                    );
                }
            }
        }
        self.interner().literal_string("")
    }
}

fn consume_source_text(
    text: &str,
    s_idx: &mut usize,
    s_offset: &mut usize,
    source_spans: &[TemplateSpan],
    source_texts: &[Option<std::sync::Arc<str>>],
) -> bool {
    if text.is_empty() {
        return true;
    }
    let Some(TemplateSpan::Text(_)) = source_spans.get(*s_idx) else {
        return false;
    };
    let src = source_texts[*s_idx].as_ref().expect("text span").as_ref();
    if !src[*s_offset..].starts_with(text) {
        return false;
    }
    *s_offset += text.len();
    true
}

fn find_anchor_in_source(
    anchor: &str,
    start_idx: usize,
    start_offset: usize,
    source_spans: &[TemplateSpan],
    source_texts: &[Option<std::sync::Arc<str>>],
) -> Option<(usize, usize)> {
    if anchor.is_empty() {
        return Some((start_idx, start_offset));
    }
    let mut i = start_idx;
    let mut offset = start_offset;
    while i < source_spans.len() {
        if let TemplateSpan::Text(_) = source_spans[i] {
            let src = source_texts[i].as_ref().expect("text span").as_ref();
            if let Some(pos) = src[offset..].find(anchor) {
                return Some((i, offset + pos));
            }
        }
        // Anchors are literal text — Type spans contain no characters, and
        // unmatched text spans must be walked past to keep searching.
        i += 1;
        offset = 0;
    }
    None
}

fn source_fully_consumed(
    s_idx: usize,
    s_offset: usize,
    source_spans: &[TemplateSpan],
    source_texts: &[Option<std::sync::Arc<str>>],
) -> bool {
    if s_idx >= source_spans.len() {
        return true;
    }
    let mut i = s_idx;
    let mut offset = s_offset;
    while i < source_spans.len() {
        match source_spans[i] {
            TemplateSpan::Text(_) => {
                let src = source_texts[i].as_ref().expect("text span").as_ref();
                if offset != src.len() {
                    return false;
                }
            }
            TemplateSpan::Type(_) => return false,
        }
        i += 1;
        offset = 0;
    }
    true
}
