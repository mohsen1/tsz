//! TypeScript 7 union-member ordering for reconstructed declaration type text.
//!
//! The declaration emitter reconstructs some inferred types from source text
//! when the solver type is unavailable or less precise than what `tsc` prints
//! (e.g. a variadic-tuple spread whose checker type collapses to `unknown[]`,
//! or a numeric-literal return union that the solver widens to `number`). Such
//! reconstruction preserves the as-written member order, but TypeScript 7's
//! declaration printer orders union members by its `TypeFlags` rank
//! (`any < unknown < void < string < number < bigint < boolean < symbol`,
//! literals above the primitives, object/named members above literals, with
//! `null`/`undefined` pushed to the tail).
//!
//! This module mirrors `TypePrinter::print_union`'s rank model at the text
//! layer so those reconstruction paths match `tsc`. It only reorders the
//! members of a `|`-separated union; every other token is preserved verbatim,
//! and the transform is idempotent on text that is already in rank order, so
//! applying it to an already-correct union is a no-op.

use super::super::DeclarationEmitter;

// Rank buckets mirroring `TypePrinter::print_union` / its `primitive_rank`.
const RANK_ANY: u32 = 1;
const RANK_UNKNOWN: u32 = 2;
const RANK_VOID: u32 = 16;
const RANK_STRING: u32 = 32;
const RANK_NUMBER: u32 = 64;
const RANK_BIGINT: u32 = 128;
const RANK_BOOLEAN: u32 = 256;
const RANK_SYMBOL: u32 = 512;
const RANK_STRING_LIT: u32 = 1 << 10;
const RANK_NUMBER_LIT: u32 = 1 << 11;
const RANK_BIGINT_LIT: u32 = 1 << 12;
const RANK_BOOLEAN_LIT: u32 = 1 << 13;
const RANK_OBJECT: u32 = 1 << 20;
const RANK_TEMPLATE: u32 = 1 << 22;
// `null`/`undefined` form the nullable tail, appended after every real member.
const RANK_NULL: u32 = u32::MAX - 1;
const RANK_UNDEFINED: u32 = u32::MAX;

/// A total-order tiebreak within a single rank bucket, mirroring the literal
/// comparisons in `print_union` (string literals lexicographically, numeric
/// literals by value, booleans `false < true`).
#[derive(PartialEq)]
enum MemberKey {
    Number(f64),
    Text(String),
    None,
}

impl DeclarationEmitter<'_> {
    /// Order already-separated rendered union member texts by the TypeScript 7
    /// rank. Members whose shape does not resolve to a ranked literal/primitive
    /// keep their relative order (stable sort within the object bucket).
    pub(in crate::declaration_emitter) fn order_ts7_union_member_texts<S: AsRef<str>>(
        members: &[S],
    ) -> Vec<String> {
        Self::ts7_union_member_order(members)
            .into_iter()
            .map(|idx| members[idx].as_ref().trim().to_string())
            .collect()
    }

    /// Return the permutation that orders `members` by the TypeScript 7 union
    /// rank, letting callers reorder data held in parallel with the member
    /// texts (e.g. a raw/rendered text pair). Stable within an equal rank+key.
    pub(in crate::declaration_emitter) fn ts7_union_member_order<S: AsRef<str>>(
        members: &[S],
    ) -> Vec<usize> {
        let ranked: Vec<(u32, MemberKey)> = members
            .iter()
            .map(|member| {
                let text = member.as_ref().trim();
                let rank = Self::ts7_member_rank(text);
                let key = Self::ts7_member_key(text, rank);
                (rank, key)
            })
            .collect();
        let mut order: Vec<usize> = (0..members.len()).collect();
        order.sort_by(|&a, &b| {
            ranked[a]
                .0
                .cmp(&ranked[b].0)
                .then_with(|| Self::compare_member_keys(&ranked[a].1, &ranked[b].1))
                .then_with(|| a.cmp(&b))
        });
        order
    }

    /// Reorder every `|`-separated union in `type_text` by the TypeScript 7
    /// rank, recursing into a leading parenthesized group so a nested element
    /// union such as `(2 | 1)[]` is reordered too. Non-union structure is
    /// preserved.
    pub(in crate::declaration_emitter) fn reorder_ts7_unions_in_text(type_text: &str) -> String {
        let members = Self::split_top_level_union_members(type_text);
        if members.len() <= 1 {
            return Self::reorder_ts7_unions_in_parenthesized_group(type_text);
        }
        let recursed: Vec<String> = members
            .iter()
            .map(|member| Self::reorder_ts7_unions_in_parenthesized_group(member.trim()))
            .collect();
        Self::order_ts7_union_member_texts(&recursed).join(" | ")
    }

    /// Recurse into the interior of a leading `(...)` group of a non-union
    /// segment (the shape `(<union>)[]`, `(<union>)`), reordering a union nested
    /// there. Only a leading parenthesized group is descended into — that is the
    /// one place a reconstructed element union appears — so the pass never
    /// rewrites object bodies, generic arguments, or arrow returns.
    fn reorder_ts7_unions_in_parenthesized_group(segment: &str) -> String {
        let trimmed = segment.trim();
        if !trimmed.starts_with('(') {
            return trimmed.to_string();
        }
        let Some(close) = Self::matching_paren(trimmed) else {
            return trimmed.to_string();
        };
        let inner = &trimmed[1..close];
        let suffix = &trimmed[close + 1..];
        // Only descend when the parenthesized interior is itself a union; a
        // parenthesized function type `(a: number) => void` must stay intact.
        if Self::split_top_level_union_members(inner).len() <= 1 {
            return trimmed.to_string();
        }
        format!("({}){}", Self::reorder_ts7_unions_in_text(inner), suffix)
    }

    /// Split `type_text` into its top-level `|`-separated union members,
    /// honoring nested brackets, string literals, template literals, and the
    /// `=>` arrow (whose `>` is not a bracket). Returns a single element when
    /// there is no top-level union.
    pub(in crate::declaration_emitter) fn split_top_level_union_members(
        type_text: &str,
    ) -> Vec<String> {
        let bytes = type_text.as_bytes();
        let mut members = Vec::new();
        let mut depth = 0i32;
        let mut start = 0usize;
        let mut scanner = UnionScanner::default();
        // A depth-0 `=>` starts a function-type return that greedily extends to
        // the end of this type, so a `|` after it belongs to the return union,
        // not to this level's union: `(x: T) => A | B` is one function type.
        let mut seen_top_arrow = false;
        let mut idx = 0usize;
        while idx < bytes.len() {
            let byte = bytes[idx];
            if scanner.consume(byte) {
                idx += 1;
                continue;
            }
            match byte {
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b'=' if bytes.get(idx + 1) == Some(&b'>') => {
                    if depth == 0 {
                        seen_top_arrow = true;
                    }
                    // `=>` arrow: skip the `>` so it does not close a group.
                    idx += 2;
                    continue;
                }
                b')' | b']' | b'}' | b'>' => depth -= 1,
                b'|' if depth == 0 && !seen_top_arrow => {
                    members.push(type_text[start..idx].trim().to_string());
                    start = idx + 1;
                }
                _ => {}
            }
            idx += 1;
        }
        members.push(type_text[start..].trim().to_string());
        members.retain(|member| !member.is_empty());
        if members.is_empty() {
            members.push(type_text.trim().to_string());
        }
        members
    }

    /// Index of the `)` that closes the `(` at byte 0 of `text`, honoring
    /// nested brackets, the `=>` arrow, and string/template literals. `None` if
    /// unbalanced.
    fn matching_paren(text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        if bytes.first() != Some(&b'(') {
            return None;
        }
        let mut depth = 0u32;
        let mut scanner = UnionScanner::default();
        let mut idx = 0usize;
        while idx < bytes.len() {
            let byte = bytes[idx];
            if scanner.consume(byte) {
                idx += 1;
                continue;
            }
            match byte {
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b'=' if bytes.get(idx + 1) == Some(&b'>') => {
                    // `=>` arrow: its `>` is not a bracket.
                    idx += 2;
                    continue;
                }
                b')' | b']' | b'}' | b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        None
    }

    fn ts7_member_rank(member: &str) -> u32 {
        match member {
            "any" => RANK_ANY,
            "unknown" => RANK_UNKNOWN,
            "void" => RANK_VOID,
            "string" => RANK_STRING,
            "number" => RANK_NUMBER,
            "bigint" => RANK_BIGINT,
            "boolean" => RANK_BOOLEAN,
            "symbol" => RANK_SYMBOL,
            "null" => RANK_NULL,
            "undefined" => RANK_UNDEFINED,
            "true" | "false" => RANK_BOOLEAN_LIT,
            _ => {
                if Self::is_numeric_literal_text(member) {
                    RANK_NUMBER_LIT
                } else if Self::is_bigint_literal_text(member) {
                    RANK_BIGINT_LIT
                } else if Self::is_string_literal_text(member) {
                    RANK_STRING_LIT
                } else if member.starts_with('`') {
                    RANK_TEMPLATE
                } else {
                    RANK_OBJECT
                }
            }
        }
    }

    fn ts7_member_key(member: &str, rank: u32) -> MemberKey {
        match rank {
            RANK_NUMBER_LIT => member
                .parse::<f64>()
                .map_or(MemberKey::None, MemberKey::Number),
            RANK_STRING_LIT => MemberKey::Text(member[1..member.len() - 1].to_string()),
            RANK_BOOLEAN_LIT => MemberKey::Number(if member == "true" { 1.0 } else { 0.0 }),
            RANK_BIGINT_LIT => member
                .trim_end_matches('n')
                .parse::<f64>()
                .map_or(MemberKey::None, MemberKey::Number),
            _ => MemberKey::None,
        }
    }

    fn compare_member_keys(a: &MemberKey, b: &MemberKey) -> std::cmp::Ordering {
        match (a, b) {
            (MemberKey::Number(l), MemberKey::Number(r)) => l.total_cmp(r),
            (MemberKey::Text(l), MemberKey::Text(r)) => l.cmp(r),
            _ => std::cmp::Ordering::Equal,
        }
    }

    fn is_numeric_literal_text(member: &str) -> bool {
        // Reject bigint (`10n`) here so it ranks in its own bucket. A leading
        // `-` is the only sign a normalized numeric-literal type carries.
        if member.is_empty() || member.ends_with('n') {
            return false;
        }
        member.parse::<f64>().is_ok()
    }

    fn is_bigint_literal_text(member: &str) -> bool {
        let Some(digits) = member.strip_suffix('n') else {
            return false;
        };
        let digits = digits.strip_prefix('-').unwrap_or(digits);
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    }

    fn is_string_literal_text(member: &str) -> bool {
        let bytes = member.as_bytes();
        bytes.len() >= 2
            && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    }
}

/// Tracks whether the scanner is inside a string or template literal so union
/// splitting and bracket matching ignore delimiters that appear as literal
/// content (`"a|b"`, `` `${T}` ``).
#[derive(Default)]
struct UnionScanner {
    string: Option<u8>,
    escaped: bool,
}

impl UnionScanner {
    /// Advance one byte. Returns `true` when `byte` is literal content that the
    /// caller must not treat as structural (a bracket, `|`, or `=>`).
    const fn consume(&mut self, byte: u8) -> bool {
        if let Some(quote) = self.string {
            if self.escaped {
                self.escaped = false;
            } else if byte == b'\\' {
                self.escaped = true;
            } else if byte == quote {
                self.string = None;
            }
            return true;
        }
        if byte == b'"' || byte == b'\'' || byte == b'`' {
            self.string = Some(byte);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::DeclarationEmitter;

    fn order(members: &[&str]) -> Vec<String> {
        DeclarationEmitter::order_ts7_union_member_texts(members)
    }

    #[test]
    fn numeric_literals_order_by_value_including_negatives() {
        assert_eq!(order(&["5", "-1", "3", "-2"]), ["-2", "-1", "3", "5"]);
    }

    #[test]
    fn string_literals_order_lexicographically() {
        assert_eq!(order(&["\"foo\"", "\"bar\""]), ["\"bar\"", "\"foo\""]);
    }

    #[test]
    fn literals_precede_object_and_typeof_members() {
        // string-literal < number-literal < object bucket (`typeof`), and
        // `undefined` is pushed to the tail.
        assert_eq!(
            order(&["typeof a", "\"ok\"", "1", "undefined"]),
            ["\"ok\"", "1", "typeof a", "undefined"]
        );
    }

    #[test]
    fn keyword_primitives_precede_literals() {
        assert_eq!(order(&["1", "string", "number"]), ["string", "number", "1"]);
    }

    #[test]
    fn object_bucket_members_keep_relative_order() {
        assert_eq!(order(&["B", "A", "C"]), ["B", "A", "C"]);
    }

    #[test]
    fn reorders_a_parenthesized_element_union() {
        assert_eq!(
            DeclarationEmitter::reorder_ts7_unions_in_text("(2 | 4 | 1 | 3)[]"),
            "(1 | 2 | 3 | 4)[]"
        );
    }

    #[test]
    fn reorders_a_top_level_union() {
        assert_eq!(
            DeclarationEmitter::reorder_ts7_unions_in_text("1 | -1"),
            "-1 | 1"
        );
    }

    #[test]
    fn arrow_return_union_is_not_split_as_a_top_level_union() {
        // A bare function type is one member: the `|` binds into the arrow
        // return, so the whole type is preserved rather than being reordered as
        // if it were `((x: T) => "b") | "a"`.
        let text = "(x: T) => \"b\" | \"a\"";
        assert_eq!(
            DeclarationEmitter::split_top_level_union_members(text).len(),
            1
        );
        assert_eq!(DeclarationEmitter::reorder_ts7_unions_in_text(text), text);
    }

    #[test]
    fn a_union_pipe_inside_a_string_literal_is_not_a_separator() {
        assert_eq!(
            DeclarationEmitter::split_top_level_union_members("\"a|b\" | \"c\""),
            ["\"a|b\"", "\"c\""]
        );
    }
}
