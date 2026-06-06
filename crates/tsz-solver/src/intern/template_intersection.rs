//! Pattern template-literal intersection reduction.
//!
//! Mirrors tsc's `getIntersectionType` template-literal handling
//! (`extractRedundantTemplateLiterals`): intersecting string-domain keys with a
//! pattern template literal such as `` `${number}` `` keeps only the keys that
//! actually inhabit the pattern. Kept separate from `normalize.rs` to respect the
//! per-file size ceiling.

use super::TypeInterner;
use super::normalize::PrimitiveClass;
use crate::types::{IntrinsicKind, LiteralValue, TemplateSpan, TypeData, TypeId};
use smallvec::SmallVec;
use std::sync::Arc;
use tsz_common::interner::Atom;

/// Backtracking budget for matching a string literal against a pattern template.
/// When an input exceeds these bounds the reduction *bails* (returns `None`)
/// rather than risk an unsound decision or pathological matching cost.
const MAX_PATTERN_TEMPLATE_SPANS: usize = 8;
/// Total `${...}` placeholders (each branches the backtracking search).
const MAX_PATTERN_PLACEHOLDERS: usize = 3;
const MAX_PATTERN_LITERAL_LEN: usize = 128;

impl TypeInterner {
    /// Reduce a `... & pattern-template-literal` intersection, mirroring tsc's
    /// `getIntersectionType` template-literal handling
    /// (`extractRedundantTemplateLiterals`).
    ///
    /// A pattern template literal such as `` `${number}` `` or `` `id-${string}` ``
    /// spans an (infinite) set of strings. Intersecting it with the string-domain
    /// keys produced by `keyof`/explicit unions keeps only the keys that actually
    /// inhabit the pattern:
    /// - `"0" & ` `` `${number}` `` → `"0"` (the literal is more specific; the
    ///   template is redundant);
    /// - `"length" & ` `` `${number}` `` → `never` (no value is both);
    /// - `(keyof [string, string]) & ` `` `${number}` `` → `"0" | "1"` (numeric
    ///   index keys survive, `"length"` / method names / the `number` index key
    ///   drop out).
    ///
    /// This is the reduction behind the distributive-conditional-over-tuple-union
    /// witnesses (`T extends unknown ? keyof T & ` `` `${number}` `` ` : never`):
    /// without it the non-matching keys leak through and the result is
    /// over-broad. It runs at interning time, before the size-gated
    /// union-distribution path, so the large key unions produced by `keyof` on a
    /// tuple are handled directly.
    ///
    /// Returns `Some(result)` when the template interaction fully determines the
    /// reduced type (a filtered union, a single literal, or `never`); returns
    /// `None` to leave the intersection for the other normalization passes
    /// (including non-string members, deferred/generic templates, and shapes this
    /// reduction does not model).
    pub(crate) fn reduce_pattern_template_intersection(&self, flat: &[TypeId]) -> Option<TypeId> {
        // Cheap early-out for the overwhelmingly common no-template case: skip
        // the partition + allocation unless the intersection contains a template
        // literal at all. `normalize_intersection` is on every type-construction
        // hot path, so this guard matters.
        if !flat
            .iter()
            .any(|&m| matches!(self.lookup(m), Some(TypeData::TemplateLiteral(_))))
        {
            return None;
        }

        // Partition into reducible pattern templates (spans resolved once) and
        // everything else.
        let mut templates: SmallVec<[Arc<[TemplateSpan]>; 2]> = SmallVec::new();
        let mut others: SmallVec<[TypeId; 4]> = SmallVec::new();
        for &m in flat {
            match self.reducible_template_spans(m) {
                Some(spans) => templates.push(spans),
                None => others.push(m),
            }
        }
        // Only the `<single string-domain member> & <pattern template(s)>` shape
        // is modeled here. Anything else (multiple non-template members, branded
        // intersections, …) bails — distribution and the other normalization
        // passes still handle it, recursing back here on each `member & template`
        // pair they produce.
        if templates.is_empty() || others.len() != 1 {
            return None;
        }

        match self.lookup(others[0])? {
            TypeData::Literal(LiteralValue::String(atom)) => {
                // Drop the redundant template(s) when the literal matches; the
                // value cannot be both the literal and outside the pattern.
                Some(if self.literal_inhabits_all(atom, &templates)? {
                    others[0]
                } else {
                    TypeId::NEVER
                })
            }
            TypeData::Union(list_id) => {
                let members = self.type_list(list_id);
                let mut kept: Vec<TypeId> = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    match self.lookup(member) {
                        Some(TypeData::Literal(LiteralValue::String(atom))) => {
                            if self.literal_inhabits_all(atom, &templates)? {
                                kept.push(member);
                            }
                            // Non-matching string literal → drops out (never).
                        }
                        // Members disjoint from the string domain (the numeric
                        // index key, number/boolean/bigint/symbol values and
                        // intrinsics) cannot inhabit a string template, so they
                        // drop out as `never`.
                        _ if self
                            .primitive_class_for(member)
                            .is_some_and(|class| class != PrimitiveClass::String) => {}
                        // A member this reduction cannot decide (bare `string`,
                        // nested template, object, type parameter, …): bail rather
                        // than risk an incorrect collapse.
                        _ => return None,
                    }
                }
                Some(self.union(kept))
            }
            _ => None,
        }
    }

    /// Spans of `type_id` when it is a *reducible* pattern template literal: one
    /// whose spans are only text and `string`/`number`/`bigint` placeholder
    /// intrinsics, within the backtracking budget. Finite placeholders
    /// (`boolean`, literal unions) are expanded to string-literal unions before
    /// interning, so a surviving `TemplateLiteral` with only these placeholders is
    /// always a pattern type. Returns `None` for anything outside that shape
    /// (including over-budget templates), so the caller leaves it untouched.
    fn reducible_template_spans(&self, type_id: TypeId) -> Option<Arc<[TemplateSpan]>> {
        let TypeData::TemplateLiteral(list_id) = self.lookup(type_id)? else {
            return None;
        };
        let spans = self.template_list(list_id);
        if spans.len() > MAX_PATTERN_TEMPLATE_SPANS {
            return None;
        }
        let mut placeholders = 0usize;
        for span in spans.iter() {
            if let TemplateSpan::Type(t) = span {
                if !matches!(
                    self.lookup(*t),
                    Some(TypeData::Intrinsic(
                        IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Bigint
                    ))
                ) {
                    return None;
                }
                placeholders += 1;
            }
        }
        (placeholders > 0 && placeholders <= MAX_PATTERN_PLACEHOLDERS).then_some(spans)
    }

    /// Whether `literal` inhabits *every* pattern template. Returns `None` when
    /// the literal exceeds the safe matching budget, signalling the caller to
    /// bail the whole reduction (unsound to guess either way).
    fn literal_inhabits_all(
        &self,
        literal: Atom,
        templates: &[Arc<[TemplateSpan]>],
    ) -> Option<bool> {
        let literal = self.resolve_atom(literal);
        if literal.len() > MAX_PATTERN_LITERAL_LEN {
            return None;
        }
        Some(
            templates
                .iter()
                .all(|spans| self.match_pattern_template(literal.as_str(), spans, 0)),
        )
    }

    /// Complete (backtracking) match of a concrete string literal against a
    /// reducible pattern template literal. Unlike the deliberately shallow
    /// matcher used by union normalization, this resolves `${string}` wildcards
    /// so the `never`-collapse decision is sound (a `false` result means the
    /// literal genuinely cannot inhabit the pattern). Only reachable via
    /// [`Self::literal_inhabits_all`], gated on [`Self::reducible_template_spans`].
    fn match_pattern_template(
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
                    .is_some_and(|rest| self.match_pattern_template(rest, spans, span_idx + 1))
            }
            TemplateSpan::Type(type_id) => match self.lookup(*type_id) {
                Some(TypeData::Intrinsic(IntrinsicKind::String)) => {
                    // `${string}` matches any (possibly empty) run; try every
                    // char-boundary split and recurse on the remainder.
                    let mut pos = 0;
                    loop {
                        if self.match_pattern_template(&remaining[pos..], spans, span_idx + 1) {
                            return true;
                        }
                        let Some(ch) = remaining[pos..].chars().next() else {
                            return false;
                        };
                        pos += ch.len_utf8();
                    }
                }
                Some(TypeData::Intrinsic(IntrinsicKind::Number)) => {
                    let num_len =
                        crate::relations::subtype::rules::literals::find_number_length(remaining);
                    (1..=num_len).rev().any(|len| {
                        crate::relations::subtype::rules::literals::is_valid_number(
                            &remaining[..len],
                        ) && self.match_pattern_template(&remaining[len..], spans, span_idx + 1)
                    })
                }
                Some(TypeData::Intrinsic(IntrinsicKind::Bigint)) => {
                    let len =
                        crate::relations::subtype::rules::literals::find_integer_length(remaining);
                    len > 0 && self.match_pattern_template(&remaining[len..], spans, span_idx + 1)
                }
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric_template(interner: &TypeInterner) -> TypeId {
        interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)])
    }

    #[test]
    fn matching_string_literal_drops_redundant_numeric_template() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let result = interner.intersection(vec![zero, numeric_template(&interner)]);
        assert_eq!(result, zero, "\"0\" & `${{number}}` should reduce to \"0\"");
    }

    #[test]
    fn nonmatching_string_literal_collapses_numeric_template_to_never() {
        let interner = TypeInterner::new();
        let length = interner.literal_string("length");
        let result = interner.intersection(vec![length, numeric_template(&interner)]);
        assert_eq!(
            result,
            TypeId::NEVER,
            "\"length\" & `${{number}}` should reduce to never"
        );
    }

    #[test]
    fn numeric_template_filters_string_literal_union_keys() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let length = interner.literal_string("length");
        // Mirrors `keyof [string, string] & `${number}`` once `keyof` has produced
        // the literal index keys (plus the non-numeric `"length"` and the numeric
        // index intrinsic).
        let keys = interner.union(vec![zero, one, length, TypeId::NUMBER]);
        let result = interner.intersection(vec![keys, numeric_template(&interner)]);
        let expected = interner.union(vec![zero, one]);
        assert_eq!(
            result, expected,
            "numeric index keys survive, \"length\" and the number key drop out"
        );
    }

    #[test]
    fn prefix_wildcard_template_keeps_only_matching_literals() {
        let interner = TypeInterner::new();
        let foo = interner.literal_string("foo");
        let bar = interner.literal_string("bar");
        // `f${string}`
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("f")),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let result = interner.intersection(vec![interner.union(vec![foo, bar]), template]);
        assert_eq!(
            result, foo,
            "`f${{string}}` keeps \"foo\" and drops \"bar\""
        );
    }

    #[test]
    fn suffix_wildcard_template_keeps_only_matching_literals() {
        let interner = TypeInterner::new();
        let ax = interner.literal_string("ax");
        let bx = interner.literal_string("bx");
        let ay = interner.literal_string("ay");
        // `${string}x`
        let template = interner.template_literal(vec![
            TemplateSpan::Type(TypeId::STRING),
            TemplateSpan::Text(interner.intern_string("x")),
        ]);
        let result = interner.intersection(vec![interner.union(vec![ax, bx, ay]), template]);
        assert_eq!(result, interner.union(vec![ax, bx]));
    }

    #[test]
    fn bigint_template_filters_union_literals() {
        let interner = TypeInterner::new();
        let one = interner.literal_string("1");
        let x = interner.literal_string("x");
        let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::BIGINT)]);
        let result = interner.intersection(vec![interner.union(vec![one, x]), template]);
        assert_eq!(result, one);
    }

    #[test]
    fn literal_must_satisfy_every_pattern_template() {
        let interner = TypeInterner::new();
        // `"a1"` matches `a${string}` but not `${number}` -> never.
        let a1 = interner.literal_string("a1");
        let a_prefix = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("a")),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let result = interner.intersection(vec![a1, a_prefix, numeric_template(&interner)]);
        assert_eq!(result, TypeId::NEVER);
    }

    #[test]
    fn number_literal_member_drops_out_of_template_intersection() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let two = interner.literal_number(2.0);
        // `(2 | "0") & `${number}`` -> "0" (the number literal cannot inhabit a
        // string-domain template).
        let result = interner.intersection(vec![
            interner.union(vec![two, zero]),
            numeric_template(&interner),
        ]);
        assert_eq!(result, zero);
    }

    #[test]
    fn bare_string_with_template_is_left_for_other_passes() {
        let interner = TypeInterner::new();
        // `string & `${number}`` is not modeled by this reduction (the non-literal
        // `string` member is undecidable here); it must be returned unchanged so
        // the surrounding normalization keeps both members.
        let template = numeric_template(&interner);
        let result = interner.intersection(vec![TypeId::STRING, template]);
        assert!(
            matches!(interner.lookup(result), Some(TypeData::Intersection(_))),
            "string & `${{number}}` should stay an intersection"
        );
    }

    #[test]
    fn unrelated_intersections_are_left_untouched() {
        let interner = TypeInterner::new();
        // No pattern template present: ordinary string literal intersection is
        // governed by the existing disjoint-literal logic, not this reduction.
        let a = interner.literal_string("a");
        let b = interner.literal_string("b");
        assert_eq!(interner.intersection(vec![a, b]), TypeId::NEVER);
        // A single string literal with no template is returned unchanged.
        assert_eq!(interner.intersection(vec![a]), a);
    }
}
