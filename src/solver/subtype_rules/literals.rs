//! Literal type subtype checking.
//!
//! This module handles subtyping for TypeScript's literal types:
//! - String literals ("hello", "world")
//! - Number literals (42, 3.14)
//! - Boolean literals (true, false)
//! - BigInt literals (42n)
//! - Template literal type matching (using backtracking)

use crate::interner::Atom;
use crate::solver::types::*;
use crate::solver::visitor::{intrinsic_kind, literal_value, union_list_id};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

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
                        LiteralValue::Boolean(b) => {
                            let bool_str = if b { "true" } else { "false" };
                            if remaining.starts_with(bool_str) {
                                self.match_template_literal_recursive(
                                    &remaining[bool_str.len()..],
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
        if let Some(next_text_pos) = self.find_next_text_span(spans, span_idx + 1) {
            if let TemplateSpan::Text(text) = &spans[next_text_pos] {
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
            if is_valid_number(&remaining[..len]) {
                if self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1) {
                    return true;
                }
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
        if remaining.starts_with("true") {
            if self.match_template_literal_recursive(&remaining[4..], spans, span_idx + 1) {
                return true;
            }
        }
        // Try "false"
        if remaining.starts_with("false") {
            if self.match_template_literal_recursive(&remaining[5..], spans, span_idx + 1) {
                return true;
            }
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
                    LiteralValue::Boolean(b) => {
                        let bool_str = if b { "true" } else { "false" };
                        if remaining.starts_with(bool_str) {
                            self.match_template_literal_recursive(
                                &remaining[bool_str.len()..],
                                spans,
                                span_idx + 1,
                            )
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

            match self.apparent_primitive_kind_for_type(member) {
                Some(IntrinsicKind::String) => {
                    if self.match_string_wildcard(remaining, spans, span_idx) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
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
