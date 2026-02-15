//! Literal type subtype checking.
//!
//! This module handles subtyping for TypeScript's literal types:
//! - String literals ("hello", "world")
//! - Number literals (42, 3.14)
//! - Boolean literals (true, false)
//! - BigInt literals (42n)
//! - Template literal type matching (using backtracking)
//! - Template-to-template literal subtype matching (generalized pattern matching)

use crate::types::*;
use crate::visitor::{intrinsic_kind, literal_value, template_literal_id, union_list_id};
use tsz_common::interner::Atom;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

/// Pre-resolved template span for template-to-template matching.
/// Text atoms are resolved into owned Strings so we don't hold borrows on the interner
/// during mutable subtype checks.
#[derive(Clone)]
enum ResolvedSpan {
    Text(String),
    Type(TypeId),
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if a literal value is compatible with an intrinsic type kind.
    ///
    /// Literal types are subtypes of their corresponding intrinsic types:
    /// - String literals (e.g., "hello") are subtypes of `string`
    /// - Number literals (e.g., 42, 3.14) are subtypes of `number`
    /// - BigInt literals (e.g., 42n) are subtypes of `bigint`
    /// - Boolean literals (true/false) are subtypes of `boolean`
    ///
    /// ## TypeScript Soundness:
    /// Literal types are more specific than their base intrinsic types, so:
    /// - `"hello"` <: `string` ✅
    /// - `42` <: `number` ✅
    /// - `42` <: `string` ❌
    ///
    /// ## Examples:
    /// ```typescript
    /// let x: "hello" = "hello";  // ✅
    /// let y: string = "hello";    // ✅ (literal to intrinsic)
    /// let z: number = "hello";    // ❌ (wrong intrinsic)
    /// ```
    pub(crate) fn check_literal_to_intrinsic(
        &self,
        literal: &LiteralValue,
        target: IntrinsicKind,
    ) -> SubtypeResult {
        let matches = match literal {
            LiteralValue::String(_) => target == IntrinsicKind::String,
            LiteralValue::Number(_) => target == IntrinsicKind::Number,
            LiteralValue::BigInt(_) => target == IntrinsicKind::Bigint,
            LiteralValue::Boolean(_) => target == IntrinsicKind::Boolean,
        };

        if matches {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if a literal string matches a template literal pattern.
    ///
    /// Uses backtracking to match a literal against a template literal pattern that may
    /// contain type holes (represented by string type placeholders).
    ///
    /// ## Template Literal Pattern:
    /// Template literals are represented as spans of literal text and type holes:
    /// - Literal spans: must match exactly
    /// - Type holes: match any string (wildcards)
    ///
    /// ## Examples:
    /// ```typescript
    /// // Pattern: `foo${string}bar`
    /// // "foobazbar" ✅ matches (baz fills the type hole)
    /// // "foobar" ❌ doesn't match (missing content for type hole)
    ///
    /// // Pattern: `prefix-${string}-suffix`
    /// // "prefix-value-suffix" ✅ matches
    /// ```
    ///
    /// ## Backtracking:
    /// The algorithm tries different ways to match literal spans against type holes,
    /// ensuring that all literal constraints are satisfied.
    pub(crate) fn check_literal_matches_template_literal(
        &self,
        literal: Atom,
        template_spans: TemplateLiteralId,
    ) -> SubtypeResult {
        // Get the literal string value
        let literal_str = self.interner.resolve_atom(literal);

        // Get the template literal spans
        let spans = self.interner.template_list(template_spans);

        // Use backtracking to match the literal against the pattern
        if self.match_template_literal_recursive(literal_str.as_str(), &spans, 0) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Recursively match a string against template literal spans using backtracking.
    pub(crate) fn match_template_literal_recursive(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        // Base case: if we've processed all spans, check if we've consumed the entire string
        if span_idx >= spans.len() {
            return remaining.is_empty();
        }

        match &spans[span_idx] {
            TemplateSpan::Text(text) => {
                let text_str = self.interner.resolve_atom(*text);
                // Check if the remaining string starts with this text
                if remaining.starts_with(text_str.as_str()) {
                    // Continue matching with the rest of the string and spans
                    self.match_template_literal_recursive(
                        &remaining[text_str.len()..],
                        spans,
                        span_idx + 1,
                    )
                } else {
                    false
                }
            }
            TemplateSpan::Type(type_id) => {
                let type_id = *type_id;
                if let Some(kind) = intrinsic_kind(self.interner, type_id) {
                    return match kind {
                        IntrinsicKind::String => {
                            self.match_string_wildcard(remaining, spans, span_idx)
                        }
                        IntrinsicKind::Number => {
                            self.match_number_pattern(remaining, spans, span_idx)
                        }
                        IntrinsicKind::Boolean => {
                            self.match_boolean_pattern(remaining, spans, span_idx)
                        }
                        IntrinsicKind::Bigint => {
                            self.match_bigint_pattern(remaining, spans, span_idx)
                        }
                        _ => false,
                    };
                }

                if let Some(literal) = literal_value(self.interner, type_id) {
                    return match literal {
                        LiteralValue::String(pattern) => {
                            let pattern_str = self.interner.resolve_atom(pattern);
                            if let Some(remaining) = remaining.strip_prefix(pattern_str.as_str()) {
                                self.match_template_literal_recursive(
                                    remaining,
                                    spans,
                                    span_idx + 1,
                                )
                            } else {
                                false
                            }
                        }
                        LiteralValue::Number(num) => {
                            let num_str = format_number_for_template(num.0);
                            if let Some(remaining) = remaining.strip_prefix(&num_str) {
                                self.match_template_literal_recursive(
                                    remaining,
                                    spans,
                                    span_idx + 1,
                                )
                            } else {
                                false
                            }
                        }
                        LiteralValue::Boolean(b) => {
                            let bool_str = if b { "true" } else { "false" };
                            if let Some(remaining) = remaining.strip_prefix(bool_str) {
                                self.match_template_literal_recursive(
                                    remaining,
                                    spans,
                                    span_idx + 1,
                                )
                            } else {
                                false
                            }
                        }
                        LiteralValue::BigInt(n) => {
                            let bigint_str = self.interner.resolve_atom(n);
                            if remaining.starts_with(bigint_str.as_str()) {
                                self.match_template_literal_recursive(
                                    &remaining[bigint_str.len()..],
                                    spans,
                                    span_idx + 1,
                                )
                            } else {
                                false
                            }
                        }
                    };
                }

                if let Some(members) = union_list_id(self.interner, type_id) {
                    return self.match_union_pattern(remaining, spans, span_idx, members);
                }

                match self.apparent_primitive_kind_for_type(type_id) {
                    Some(IntrinsicKind::String) => {
                        self.match_string_wildcard(remaining, spans, span_idx)
                    }
                    Some(IntrinsicKind::Number) => {
                        self.match_number_pattern(remaining, spans, span_idx)
                    }
                    Some(IntrinsicKind::Boolean) => {
                        self.match_boolean_pattern(remaining, spans, span_idx)
                    }
                    _ => false,
                }
            }
        }
    }

    /// Match a string wildcard using backtracking.
    /// Tries all possible lengths from 0 to remaining.len()
    pub(crate) fn match_string_wildcard(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let is_last_span = span_idx == spans.len() - 1;

        // If this is the last span, any remaining string is valid
        if is_last_span {
            return true;
        }

        // Find the next text span to use as an anchor for optimization
        if let Some(next_text_pos) = self.find_next_text_span(spans, span_idx + 1)
            && let TemplateSpan::Text(text) = &spans[next_text_pos]
        {
            let text_str = self.interner.resolve_atom(*text);
            // Optimization: only try positions where the next text could match
            for match_pos in remaining.match_indices(text_str.as_str()) {
                // Try matching from this position
                if self.match_template_literal_recursive(
                    &remaining[match_pos.0..],
                    spans,
                    span_idx + 1,
                ) {
                    return true;
                }
            }
            // Also try if the pattern can match with empty wildcard
            // (in case the next span is also a type that could consume the text)
            if self.match_template_literal_recursive(remaining, spans, span_idx + 1) {
                return true;
            }
            return false;
        }

        // No optimization available, try all possible lengths
        for len in 0..=remaining.len() {
            if self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1) {
                return true;
            }
        }
        false
    }

    /// Find the next text span after the given index.
    pub(crate) fn find_next_text_span(
        &self,
        spans: &[TemplateSpan],
        start_idx: usize,
    ) -> Option<usize> {
        for i in start_idx..spans.len() {
            if matches!(spans[i], TemplateSpan::Text(_)) {
                return Some(i);
            }
        }
        None
    }

    /// Match a number pattern - matches valid numeric strings.
    pub(crate) fn match_number_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let is_last_span = span_idx == spans.len() - 1;

        // Find the longest valid number at the start of remaining
        let num_len = find_number_length(remaining);

        if num_len == 0 {
            // No valid number found, but empty match might be valid for last span
            if is_last_span {
                return remaining.is_empty();
            }
            return false;
        }

        // Try all valid number lengths from longest to shortest
        for len in (1..=num_len).rev() {
            // Verify this is a valid number
            if is_valid_number(&remaining[..len])
                && self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1)
            {
                return true;
            }
        }

        false
    }

    /// Match a boolean pattern - matches "true" or "false".
    pub(crate) fn match_boolean_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        // Try "true"
        if remaining.starts_with("true")
            && self.match_template_literal_recursive(&remaining[4..], spans, span_idx + 1)
        {
            return true;
        }
        // Try "false"
        if remaining.starts_with("false")
            && self.match_template_literal_recursive(&remaining[5..], spans, span_idx + 1)
        {
            return true;
        }
        false
    }

    /// Match a bigint pattern - matches integer strings.
    pub(crate) fn match_bigint_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let is_last_span = span_idx == spans.len() - 1;

        // Find the longest valid bigint at the start of remaining
        let int_len = find_integer_length(remaining);

        if int_len == 0 {
            if is_last_span {
                return remaining.is_empty();
            }
            return false;
        }

        // Try all valid integer lengths from longest to shortest
        for len in (1..=int_len).rev() {
            if self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1) {
                return true;
            }
        }

        false
    }

    /// Match a union pattern - try each member of the union.
    pub(crate) fn match_union_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
        members: TypeListId,
    ) -> bool {
        let members = self.interner.type_list(members);

        for &member in members.iter() {
            if let Some(literal) = literal_value(self.interner, member) {
                let matched = match literal {
                    LiteralValue::String(pattern) => {
                        let pattern_str = self.interner.resolve_atom(pattern);
                        if remaining.starts_with(pattern_str.as_str()) {
                            self.match_template_literal_recursive(
                                &remaining[pattern_str.len()..],
                                spans,
                                span_idx + 1,
                            )
                        } else {
                            false
                        }
                    }
                    LiteralValue::Number(num) => {
                        let num_str = format_number_for_template(num.0);
                        if remaining.starts_with(&num_str) {
                            self.match_template_literal_recursive(
                                &remaining[num_str.len()..],
                                spans,
                                span_idx + 1,
                            )
                        } else {
                            false
                        }
                    }
                    LiteralValue::BigInt(n) => {
                        let bigint_str = self.interner.resolve_atom(n);
                        if let Some(remaining) = remaining.strip_prefix(bigint_str.as_str()) {
                            self.match_template_literal_recursive(remaining, spans, span_idx + 1)
                        } else {
                            false
                        }
                    }
                    LiteralValue::Boolean(b) => {
                        let bool_str = if b { "true" } else { "false" };
                        if let Some(remaining) = remaining.strip_prefix(bool_str) {
                            self.match_template_literal_recursive(remaining, spans, span_idx + 1)
                        } else {
                            false
                        }
                    }
                };
                if matched {
                    return true;
                }
                continue;
            }

            if let Some(kind) = intrinsic_kind(self.interner, member) {
                let matched = match kind {
                    IntrinsicKind::String => self.match_string_wildcard(remaining, spans, span_idx),
                    IntrinsicKind::Number => self.match_number_pattern(remaining, spans, span_idx),
                    IntrinsicKind::Boolean => {
                        self.match_boolean_pattern(remaining, spans, span_idx)
                    }
                    _ => false,
                };
                if matched {
                    return true;
                }
                continue;
            }

            if let Some(IntrinsicKind::String) = self.apparent_primitive_kind_for_type(member) {
                if self.match_string_wildcard(remaining, spans, span_idx) {
                    return true;
                }
            }
        }
        false
    }

    // =========================================================================
    // Template-to-template literal subtype matching
    // =========================================================================

    /// Check if a source template literal is a subtype of a target template literal.
    ///
    /// This handles template literals with different span structures by using
    /// generalized pattern matching with backtracking. For example:
    /// - `` `1.1.${number}` `` is assignable to `` `${number}.${number}.${number}` ``
    /// - `` `${string} abc` `` is assignable to `` `${string} ${string}` ``
    ///
    /// The algorithm flattens the source into text segments and type wildcards,
    /// then matches against the target pattern using backtracking.
    pub(crate) fn check_template_assignable_to_template(
        &mut self,
        source_id: TemplateLiteralId,
        target_id: TemplateLiteralId,
    ) -> SubtypeResult {
        if source_id == target_id {
            return SubtypeResult::True;
        }

        // Pre-resolve spans into owned data to avoid borrow conflicts
        let source = self.resolve_template_spans(source_id);
        let target = self.resolve_template_spans(target_id);

        if self.match_tt_recursive(&source, 0, 0, &target, 0) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Resolve template spans into owned ResolvedSpan values.
    fn resolve_template_spans(&self, id: TemplateLiteralId) -> Vec<ResolvedSpan> {
        let spans = self.interner.template_list(id);
        spans
            .iter()
            .map(|span| match span {
                TemplateSpan::Text(atom) => {
                    ResolvedSpan::Text(self.interner.resolve_atom(*atom).to_string())
                }
                TemplateSpan::Type(type_id) => ResolvedSpan::Type(*type_id),
            })
            .collect()
    }

    /// Recursive template-to-template pattern matcher.
    ///
    /// Walks source spans (text + type) matching against the target pattern.
    /// - `si` = source span index
    /// - `s_off` = byte offset within current source Text span
    /// - `ti` = target span index
    fn match_tt_recursive(
        &mut self,
        source: &[ResolvedSpan],
        si: usize,
        s_off: usize,
        target: &[ResolvedSpan],
        ti: usize,
    ) -> bool {
        // Skip past exhausted source text spans
        if si < source.len()
            && let ResolvedSpan::Text(ref text) = source[si]
            && s_off >= text.len()
        {
            return self.match_tt_recursive(source, si + 1, 0, target, ti);
        }

        let src_done = si >= source.len();
        let tgt_done = ti >= target.len();

        if src_done && tgt_done {
            return true;
        }

        if src_done {
            return self.tt_remaining_target_accepts_empty(target, ti);
        }

        if tgt_done {
            return false;
        }

        match &source[si] {
            ResolvedSpan::Text(s_text) => {
                let remaining = &s_text[s_off..];
                match &target[ti] {
                    ResolvedSpan::Text(t_text) => {
                        // Source text must start with target text
                        if remaining.starts_with(t_text.as_str()) {
                            self.match_tt_recursive(
                                source,
                                si,
                                s_off + t_text.len(),
                                target,
                                ti + 1,
                            )
                        } else {
                            false
                        }
                    }
                    ResolvedSpan::Type(t_type) => {
                        // Target type hole consuming source text characters
                        self.match_tt_target_type_consumes_text(
                            *t_type, remaining, source, si, s_off, target, ti,
                        )
                    }
                }
            }
            ResolvedSpan::Type(s_type) => {
                let s_type = *s_type;
                match &target[ti] {
                    ResolvedSpan::Type(t_type) => {
                        let t_type = *t_type;
                        // Both are type holes — check compatibility
                        if self.is_template_type_assignable(s_type, t_type) {
                            // Option 1: match this pair and advance both
                            if self.match_tt_recursive(source, si + 1, 0, target, ti + 1) {
                                return true;
                            }
                        }
                        // Option 2: if target is string, it can absorb this source type
                        // AND continue consuming more source spans
                        if intrinsic_kind(self.interner, t_type) == Some(IntrinsicKind::String)
                            && self.match_tt_string_consume_more(source, si + 1, 0, target, ti)
                        {
                            return true;
                        }
                        false
                    }
                    ResolvedSpan::Text(_) => {
                        // Source type against target text — source type could produce
                        // strings not matching the target text, so this generally fails.
                        // Exception: source type is a literal that produces a known string.
                        if let Some(lit) = literal_value(self.interner, s_type) {
                            let lit_str = self.literal_to_template_string(&lit);
                            // Treat the literal as virtual text and match against target
                            self.match_tt_virtual_text(&lit_str, source, si + 1, target, ti)
                        } else {
                            false
                        }
                    }
                }
            }
        }
    }

    /// Check if a source type is assignable to a target type in template literal context.
    ///
    /// In template literals, all type holes produce strings, so:
    /// - `${number}` <: `${string}` (every number string is a string)
    /// - `${boolean}` <: `${string}` (every boolean string is a string)
    fn is_template_type_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        // Regular subtype check first
        if self.check_subtype(source, target).is_true() {
            return true;
        }

        // In template literal context, string on target accepts all stringifiable types
        if intrinsic_kind(self.interner, target) == Some(IntrinsicKind::String) {
            if let Some(kind) = intrinsic_kind(self.interner, source) {
                return matches!(
                    kind,
                    IntrinsicKind::String
                        | IntrinsicKind::Number
                        | IntrinsicKind::Boolean
                        | IntrinsicKind::Bigint
                        | IntrinsicKind::Null
                        | IntrinsicKind::Undefined
                );
            }
            // Template literal types are always subtypes of string
            if template_literal_id(self.interner, source).is_some() {
                return true;
            }
        }

        false
    }

    /// Target type hole consuming source text characters.
    fn match_tt_target_type_consumes_text(
        &mut self,
        t_type: TypeId,
        remaining: &str,
        source: &[ResolvedSpan],
        si: usize,
        s_off: usize,
        target: &[ResolvedSpan],
        ti: usize,
    ) -> bool {
        if let Some(kind) = intrinsic_kind(self.interner, t_type) {
            match kind {
                IntrinsicKind::String => {
                    let is_last_target = ti == target.len() - 1;
                    if is_last_target {
                        // Last string hole can consume everything remaining
                        return true;
                    }
                    // String wildcard: try consuming 0..=remaining.len() characters,
                    // then continue matching with target[ti+1].
                    // Also try consuming past the current source text span.
                    for len in 0..=remaining.len() {
                        if self.match_tt_recursive(source, si, s_off + len, target, ti + 1) {
                            return true;
                        }
                    }
                    // Also try consuming all text and continuing to absorb more source spans
                    self.match_tt_string_consume_more(source, si + 1, 0, target, ti)
                }
                IntrinsicKind::Number => {
                    let num_len = find_number_length(remaining);
                    for len in (1..=num_len).rev() {
                        if is_valid_number(&remaining[..len])
                            && self.match_tt_recursive(source, si, s_off + len, target, ti + 1)
                        {
                            return true;
                        }
                    }
                    false
                }
                IntrinsicKind::Boolean => {
                    if remaining.starts_with("true")
                        && self.match_tt_recursive(source, si, s_off + 4, target, ti + 1)
                    {
                        return true;
                    }
                    if remaining.starts_with("false")
                        && self.match_tt_recursive(source, si, s_off + 5, target, ti + 1)
                    {
                        return true;
                    }
                    false
                }
                IntrinsicKind::Bigint => {
                    let int_len = find_integer_length(remaining);
                    for len in (1..=int_len).rev() {
                        if self.match_tt_recursive(source, si, s_off + len, target, ti + 1) {
                            return true;
                        }
                    }
                    false
                }
                _ => false,
            }
        } else {
            false
        }
    }

    /// Continue consuming source spans for a target string wildcard.
    ///
    /// When a target Type(string) hole is consuming content, it can span
    /// across multiple source spans (both text and type) since all template
    /// literal components produce strings.
    fn match_tt_string_consume_more(
        &mut self,
        source: &[ResolvedSpan],
        si: usize,
        s_off: usize,
        target: &[ResolvedSpan],
        ti: usize,
    ) -> bool {
        // Try stopping here (advance target past the string)
        if self.match_tt_recursive(source, si, s_off, target, ti + 1) {
            return true;
        }

        if si >= source.len() {
            return false;
        }

        match &source[si] {
            ResolvedSpan::Text(s_text) => {
                let remaining = &s_text[s_off..];
                if remaining.is_empty() {
                    return self.match_tt_string_consume_more(source, si + 1, 0, target, ti);
                }
                // Try consuming characters one position at a time
                for len in 1..=remaining.len() {
                    if self.match_tt_recursive(source, si, s_off + len, target, ti + 1) {
                        return true;
                    }
                }
                // Also try consuming all text and continuing with next span
                self.match_tt_string_consume_more(source, si + 1, 0, target, ti)
            }
            ResolvedSpan::Type(_) => {
                // The string absorbs this type span (all types produce strings)
                // Try stopping after this span
                if self.match_tt_recursive(source, si + 1, 0, target, ti + 1) {
                    return true;
                }
                // Or continue consuming more
                self.match_tt_string_consume_more(source, si + 1, 0, target, ti)
            }
        }
    }

    /// Match virtual text (from a literal type value) against the target pattern.
    fn match_tt_virtual_text(
        &mut self,
        text: &str,
        source: &[ResolvedSpan],
        next_si: usize,
        target: &[ResolvedSpan],
        ti: usize,
    ) -> bool {
        if text.is_empty() {
            return self.match_tt_recursive(source, next_si, 0, target, ti);
        }

        if ti >= target.len() {
            return false;
        }

        match &target[ti] {
            ResolvedSpan::Text(t_text) => {
                if text.starts_with(t_text.as_str()) {
                    self.match_tt_virtual_text(
                        &text[t_text.len()..],
                        source,
                        next_si,
                        target,
                        ti + 1,
                    )
                } else {
                    false
                }
            }
            ResolvedSpan::Type(t_type) => {
                let t_type = *t_type;
                if let Some(kind) = intrinsic_kind(self.interner, t_type) {
                    match kind {
                        IntrinsicKind::String => {
                            let is_last_target = ti == target.len() - 1;
                            if is_last_target {
                                return self.match_tt_recursive(source, next_si, 0, target, ti + 1);
                            }
                            for len in 0..=text.len() {
                                if self.match_tt_virtual_text(
                                    &text[len..],
                                    source,
                                    next_si,
                                    target,
                                    ti + 1,
                                ) {
                                    return true;
                                }
                            }
                            false
                        }
                        IntrinsicKind::Number => {
                            let num_len = find_number_length(text);
                            for len in (1..=num_len).rev() {
                                if is_valid_number(&text[..len])
                                    && self.match_tt_virtual_text(
                                        &text[len..],
                                        source,
                                        next_si,
                                        target,
                                        ti + 1,
                                    )
                                {
                                    return true;
                                }
                            }
                            false
                        }
                        IntrinsicKind::Boolean => {
                            if text.starts_with("true")
                                && self.match_tt_virtual_text(
                                    &text[4..],
                                    source,
                                    next_si,
                                    target,
                                    ti + 1,
                                )
                            {
                                return true;
                            }
                            if text.starts_with("false")
                                && self.match_tt_virtual_text(
                                    &text[5..],
                                    source,
                                    next_si,
                                    target,
                                    ti + 1,
                                )
                            {
                                return true;
                            }
                            false
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Check if remaining target spans can match empty input.
    fn tt_remaining_target_accepts_empty(&self, target: &[ResolvedSpan], ti: usize) -> bool {
        for i in ti..target.len() {
            match &target[i] {
                ResolvedSpan::Text(text) => {
                    if !text.is_empty() {
                        return false;
                    }
                }
                ResolvedSpan::Type(type_id) => {
                    // Only string type can match empty string
                    if intrinsic_kind(self.interner, *type_id) != Some(IntrinsicKind::String) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Convert a literal value to its string representation for template matching.
    fn literal_to_template_string(&self, lit: &LiteralValue) -> String {
        match lit {
            LiteralValue::String(atom) => self.interner.resolve_atom(*atom).to_string(),
            LiteralValue::Number(n) => format_number_for_template(n.0),
            LiteralValue::Boolean(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            LiteralValue::BigInt(atom) => self.interner.resolve_atom(*atom).to_string(),
        }
    }
}

// =============================================================================
// Helper functions for template literal matching
// =============================================================================

/// Format a number for template literal string coercion.
/// Follows JavaScript's number-to-string conversion rules.
pub(crate) fn format_number_for_template(num: f64) -> String {
    if num.is_nan() {
        return "NaN".to_string();
    }
    if num.is_infinite() {
        return if num.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    // Use JavaScript-like formatting (no trailing .0 for integers)
    if num.fract() == 0.0 && num.abs() < 1e15 {
        format!("{:.0}", num)
    } else {
        // Use default Rust formatting which is close enough for most cases
        let s = format!("{}", num);
        // Remove unnecessary trailing zeros after decimal point
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Find the length of a valid number at the start of a string.
pub(crate) fn find_number_length(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    // Handle optional sign
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        i += 1;
    }

    // Check for special values
    if s.len() > i && s[i..].starts_with("Infinity") {
        return i + 8;
    }
    if s.len() > i && s[i..].starts_with("NaN") {
        return i + 3;
    }

    let start = i;
    let mut has_digits = false;
    let mut has_dot = false;
    let mut has_exponent = false;

    // Integer or decimal part
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            has_digits = true;
            i += 1;
        } else if chars[i] == '.' && !has_dot && !has_exponent {
            has_dot = true;
            i += 1;
        } else if (chars[i] == 'e' || chars[i] == 'E') && has_digits && !has_exponent {
            has_exponent = true;
            i += 1;
            // Optional sign after exponent
            if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                i += 1;
            }
            // Must have at least one digit after exponent
            if i >= chars.len() || !chars[i].is_ascii_digit() {
                // Invalid exponent, backtrack
                i = if has_dot { i - 2 } else { i - 1 };
                if i > 0 && (chars[i - 1] == '+' || chars[i - 1] == '-') {
                    i -= 1;
                }
                break;
            }
        } else {
            break;
        }
    }

    if !has_digits {
        return 0;
    }

    // Don't count trailing dot without digits
    if i > start && chars[i - 1] == '.' && (i == start + 1 || !chars[i - 2].is_ascii_digit()) {
        i -= 1;
    }

    i
}

/// Check if a string is a valid number representation.
pub(crate) fn is_valid_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Handle special values
    if s == "NaN" || s == "Infinity" || s == "-Infinity" || s == "+Infinity" {
        return true;
    }
    // Try parsing as f64
    s.parse::<f64>().is_ok()
}

/// Find the length of a valid integer at the start of a string.
pub(crate) fn find_integer_length(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    // Handle optional sign
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        i += 1;
    }

    let start = i;

    // Must have at least one digit
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }

    if i == start {
        return 0;
    }

    i
}
