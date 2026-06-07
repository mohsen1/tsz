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

/// Backtracking budget for matching a string literal against a pattern template.
/// When an input exceeds these bounds the reduction *bails* (returns `None`)
/// rather than risk an unsound decision or pathological matching cost.
const MAX_PATTERN_TEMPLATE_SPANS: usize = 8;
/// Total `${...}` placeholders (each branches the backtracking search).
const MAX_PATTERN_PLACEHOLDERS: usize = 3;
const MAX_PATTERN_LITERAL_LEN: usize = 128;

/// How a non-template member of a `… & pattern-template` intersection constrains
/// the (string-domain) value set of the result.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TemplateMemberKind {
    /// A finite set of string literals (a literal, a union of literals, or a
    /// string-disjoint primitive whose string set is empty). Bounds the result.
    Finite,
    /// An infinite string set (`string` or a pattern template literal). Filters
    /// candidates but does not bound enumeration.
    Infinite,
    /// A shape this reduction cannot decide (object, type parameter, lazy/indexed/
    /// conditional type, non-reducible template, …). Forces the caller to bail.
    Undecidable,
}

impl TemplateMemberKind {
    /// Combine constituent kinds when classifying a union member: `Undecidable`
    /// dominates (any undecidable constituent makes the union undecidable), then
    /// `Infinite` (a single infinite constituent widens the union), otherwise
    /// `Finite`.
    const fn combine(self, other: TemplateMemberKind) -> TemplateMemberKind {
        match (self, other) {
            (TemplateMemberKind::Undecidable, _) | (_, TemplateMemberKind::Undecidable) => {
                TemplateMemberKind::Undecidable
            }
            (TemplateMemberKind::Infinite, _) | (_, TemplateMemberKind::Infinite) => {
                TemplateMemberKind::Infinite
            }
            (TemplateMemberKind::Finite, TemplateMemberKind::Finite) => TemplateMemberKind::Finite,
        }
    }
}

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
        // Need at least one pattern template and at least one non-template
        // member. A pure `pattern & pattern` intersection (e.g. `` `${number}` ``
        // `& ` `` `${string}` ``) is not modeled here.
        if templates.is_empty() || others.is_empty() {
            return None;
        }

        // Classify every non-template member and locate a *finite* string-literal
        // member to bound the result. The intersection is the set of string values
        // that inhabit every template **and** belong to every `others` member; it is
        // finitely enumerable exactly when at least one member is a finite set of
        // string literals (a literal, or a union of literals / string-disjoint
        // primitives). That member's literals are a superset of the result, so we
        // enumerate them and filter against the remaining members and the patterns.
        //
        // This generalizes the single-member case to the distributive-conditional
        // family where several large `keyof`-on-tuple key unions are intersected
        // together (`keyof A & keyof B & ` `` `${number}` ``): each key union is too
        // large for the size-gated union distribution to expand, so without this the
        // non-matching keys leak through and the result is over-broad.
        let mut bound: Option<TypeId> = None;
        for &member in &others {
            match self.classify_template_member(member) {
                // A member this reduction cannot decide (object, type parameter,
                // lazy/indexed/conditional type, non-reducible template, …): bail
                // rather than risk an incorrect collapse.
                TemplateMemberKind::Undecidable => return None,
                TemplateMemberKind::Finite => {
                    if bound.is_none() {
                        bound = Some(member);
                    }
                }
                TemplateMemberKind::Infinite => {}
            }
        }
        // No finite member bounds the result (e.g. `string & ` `` `${number}` ``,
        // whose value set is the infinite `` `${number}` ``): leave it for the other
        // normalization passes, which model the `string`/pattern interaction.
        let bound = bound?;

        // Candidate literals (in source order) come from the bounding member; the
        // result is a subset of them.
        let mut candidates: Vec<TypeId> = Vec::new();
        self.collect_string_literals_ordered(bound, &mut candidates);

        let mut kept: Vec<TypeId> = Vec::with_capacity(candidates.len());
        'next: for literal_id in candidates {
            let Some(TypeData::Literal(LiteralValue::String(atom))) = self.lookup(literal_id)
            else {
                continue;
            };
            let literal = self.resolve_atom(atom);
            if literal.len() > MAX_PATTERN_LITERAL_LEN {
                return None;
            }
            // Must inhabit every pattern template.
            if !templates
                .iter()
                .all(|spans| self.match_pattern_template(literal.as_str(), spans, 0))
            {
                continue;
            }
            // Must belong to every other non-template member.
            for &member in &others {
                if member == bound {
                    continue;
                }
                match self.string_literal_in_member(literal.as_str(), member) {
                    Some(true) => {}
                    Some(false) => continue 'next,
                    // Unreachable after `classify_template_member` accepted every
                    // member, but stay safe rather than guess.
                    None => return None,
                }
            }
            kept.push(literal_id);
        }
        Some(self.union(kept))
    }

    /// Whether the value set of `member` (restricted to the string domain) is a
    /// *finite* set of string literals, an *infinite* string set (`string` or a
    /// pattern template), or a shape this reduction cannot decide.
    fn classify_template_member(&self, member: TypeId) -> TemplateMemberKind {
        match self.lookup(member) {
            Some(TypeData::Literal(LiteralValue::String(_))) => TemplateMemberKind::Finite,
            Some(TypeData::Intrinsic(IntrinsicKind::String)) => TemplateMemberKind::Infinite,
            Some(TypeData::TemplateLiteral(_)) => {
                // A pattern template spans an infinite string set; a non-reducible
                // template (over budget / generic spans) cannot be decided.
                if self.reducible_template_spans(member).is_some() {
                    TemplateMemberKind::Infinite
                } else {
                    TemplateMemberKind::Undecidable
                }
            }
            Some(TypeData::Union(list_id)) => {
                let mut kind = TemplateMemberKind::Finite;
                for &constituent in self.type_list(list_id).iter() {
                    kind = kind.combine(self.classify_template_member(constituent));
                    if matches!(kind, TemplateMemberKind::Undecidable) {
                        break;
                    }
                }
                kind
            }
            _ => {
                // Primitives disjoint from the string domain (number/boolean/bigint/
                // symbol/null/undefined and their literals) contribute no string and
                // are a finite (empty) string set; everything else is undecidable.
                if self
                    .primitive_class_for(member)
                    .is_some_and(|class| class != PrimitiveClass::String)
                {
                    TemplateMemberKind::Finite
                } else {
                    TemplateMemberKind::Undecidable
                }
            }
        }
    }

    /// Collect the string-literal `TypeId`s reachable from `member` in source
    /// order (deduplicated). Only meaningful for a `Finite` member (see
    /// [`Self::classify_template_member`]); non-literal constituents contribute
    /// nothing.
    fn collect_string_literals_ordered(&self, member: TypeId, out: &mut Vec<TypeId>) {
        match self.lookup(member) {
            Some(TypeData::Literal(LiteralValue::String(_))) if !out.contains(&member) => {
                out.push(member);
            }
            Some(TypeData::Union(list_id)) => {
                for &constituent in self.type_list(list_id).iter() {
                    self.collect_string_literals_ordered(constituent, out);
                }
            }
            _ => {}
        }
    }

    /// Whether the concrete string `literal` is a member of `member`'s value set.
    /// Returns `None` for shapes this reduction cannot decide (caller bails).
    fn string_literal_in_member(&self, literal: &str, member: TypeId) -> Option<bool> {
        match self.lookup(member)? {
            TypeData::Literal(LiteralValue::String(atom)) => {
                Some(self.resolve_atom(atom).as_str() == literal)
            }
            TypeData::Intrinsic(IntrinsicKind::String) => Some(true),
            TypeData::TemplateLiteral(_) => {
                let spans = self.reducible_template_spans(member)?;
                Some(self.match_pattern_template(literal, &spans, 0))
            }
            TypeData::Union(list_id) => {
                let mut found = false;
                for &constituent in self.type_list(list_id).iter() {
                    if self.string_literal_in_member(literal, constituent)? {
                        found = true;
                    }
                }
                Some(found)
            }
            _ => match self.primitive_class_for(member) {
                // A string literal cannot inhabit a string-disjoint primitive.
                Some(class) if class != PrimitiveClass::String => Some(false),
                _ => None,
            },
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

    /// Complete (backtracking) match of a concrete string literal against a
    /// reducible pattern template literal. Unlike the deliberately shallow
    /// matcher used by union normalization, this resolves `${string}` wildcards
    /// so the `never`-collapse decision is sound (a `false` result means the
    /// literal genuinely cannot inhabit the pattern). Gated on
    /// [`Self::reducible_template_spans`].
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

    #[test]
    fn template_filters_intersection_of_two_key_unions() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let two = interner.literal_string("2");
        let length = interner.literal_string("length");
        // Mirrors `keyof [a,b,c] & keyof [a,b] & `${number}`` — two large key
        // unions intersected with the numeric pattern. The size-gated union
        // distribution skips unions this wide, so the reduction must filter the
        // multi-member intersection directly. Only the numeric keys common to both
        // unions survive: `"0" | "1"`.
        let keys_abc = interner.union(vec![zero, one, two, length, TypeId::NUMBER]);
        let keys_ab = interner.union(vec![zero, one, length, TypeId::NUMBER]);
        let result = interner.intersection(vec![keys_abc, keys_ab, numeric_template(&interner)]);
        assert_eq!(result, interner.union(vec![zero, one]));
    }

    #[test]
    fn template_filters_three_way_key_union_intersection() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let two = interner.literal_string("2");
        let three = interner.literal_string("3");
        let length = interner.literal_string("length");
        let a = interner.union(vec![zero, one, two, three, length, TypeId::NUMBER]);
        let b = interner.union(vec![zero, one, two, length, TypeId::NUMBER]);
        let c = interner.union(vec![zero, one, length, TypeId::NUMBER]);
        let result = interner.intersection(vec![a, b, c, numeric_template(&interner)]);
        assert_eq!(result, interner.union(vec![zero, one]));
    }

    #[test]
    fn multi_member_intersection_with_no_common_numeric_key_is_never() {
        let interner = TypeInterner::new();
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let two = interner.literal_string("2");
        // `("0" | "1") & ("2") & `${number}`` — the literal sets are disjoint, so
        // the numeric filter leaves nothing.
        let result = interner.intersection(vec![
            interner.union(vec![zero, one]),
            two,
            numeric_template(&interner),
        ]);
        assert_eq!(result, TypeId::NEVER);
    }

    #[test]
    fn prefix_template_filters_multi_member_intersection() {
        let interner = TypeInterner::new();
        let a1 = interner.literal_string("a-1");
        let a2 = interner.literal_string("a-2");
        let b1 = interner.literal_string("b-1");
        // `("a-1" | "a-2" | "b-1") & ("a-1" | "b-1" | "a-2") & `a-${number}``
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("a-")),
            TemplateSpan::Type(TypeId::NUMBER),
        ]);
        let result = interner.intersection(vec![
            interner.union(vec![a1, a2, b1]),
            interner.union(vec![a1, b1, a2]),
            template,
        ]);
        // `b-1` fails the `a-${number}` pattern; `a-1` and `a-2` survive in both.
        assert_eq!(result, interner.union(vec![a1, a2]));
    }

    #[test]
    fn infinite_member_without_finite_bound_is_left_for_other_passes() {
        let interner = TypeInterner::new();
        // `(string | "0") & `${number}`` has an infinite value set (`${number}`),
        // so there is no finite member to bound enumeration: the reduction must
        // bail rather than collapse to the enumerated literal `"0"`.
        let zero = interner.literal_string("0");
        let su = interner.union(vec![TypeId::STRING, zero]);
        let result = interner.intersection(vec![su, numeric_template(&interner)]);
        // Whatever the other passes choose, it must not be the over-narrow `"0"`.
        assert_ne!(
            result, zero,
            "must not collapse `(string | \"0\") & `${{number}}`` to \"0\""
        );
    }

    #[test]
    fn finite_bound_lets_string_member_act_as_universal_filter() {
        let interner = TypeInterner::new();
        // `("0" | "1") & string & `${number}`` — the finite `"0" | "1"` bounds the
        // result and the universal `string` member keeps every candidate; the
        // numeric pattern is already satisfied. Result: `"0" | "1"`.
        let zero = interner.literal_string("0");
        let one = interner.literal_string("1");
        let result = interner.intersection(vec![
            interner.union(vec![zero, one]),
            TypeId::STRING,
            numeric_template(&interner),
        ]);
        assert_eq!(result, interner.union(vec![zero, one]));
    }

    #[test]
    fn undecidable_object_member_bails_without_collapsing() {
        let interner = TypeInterner::new();
        // An object member is not a string filter; the reduction must bail (return
        // `None`) and leave the intersection for the structural passes rather than
        // dropping the object or the literals.
        let zero = interner.literal_string("0");
        let reduced = interner.reduce_pattern_template_intersection(&[
            zero,
            TypeId::OBJECT,
            numeric_template(&interner),
        ]);
        assert!(
            reduced.is_none(),
            "object member must bail, leaving the intersection to other passes"
        );
    }
}
