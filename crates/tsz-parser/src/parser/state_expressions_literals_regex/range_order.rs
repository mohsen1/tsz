//! Regex character-class range-order (TS1517) analysis, extracted from
//! `state_expressions_literals_regex.rs`.
//!
//! Pure file-organization move; no logic changes. Keeps the parent under the
//! parser LOC ceiling. `regex_range_order_errors` was the largest
//! self-contained unit in `parse_regex_literal` — a standalone pass over the
//! raw literal text that borrows nothing from the parser, so it lifts out
//! whole, carrying its private `parse_hex_u32` helper and borrowing the
//! shared `decode_surrogate_pair`/`split_non_unicode_atom_offsets` from the
//! parent module.
use super::{decode_surrogate_pair, split_non_unicode_atom_offsets};

fn parse_hex_u32(raw_text: &str, start: usize, len: usize) -> Option<u32> {
    raw_text
        .get(start..start + len)
        .and_then(|slice| u32::from_str_radix(slice, 16).ok())
}

pub(super) fn regex_range_order_errors(raw_text: &str, body_end: usize) -> Vec<(u32, u32)> {
    #[derive(Clone, Copy)]
    enum ClassToken {
        Atom { value: u32, start: u32 },
        OpaqueAtom,
        Hyphen,
    }

    type ClassAtomParse = (Vec<(u32, u32)>, usize);

    fn parse_class_atom(
        raw_text: &str,
        start: usize,
        class_end: usize,
        unicode_mode: bool,
    ) -> Option<ClassAtomParse> {
        let rest = raw_text.get(start..class_end)?;
        let mut chars = rest.chars();
        let ch = chars.next()?;
        if ch == '\\' {
            let next_start = start + ch.len_utf8();
            let next = raw_text.get(next_start..class_end)?.chars().next()?;
            if next == 'x' {
                let hex_start = next_start + next.len_utf8();
                let next_index = hex_start.saturating_add(2).min(class_end);
                if let Some(value) = parse_hex_u32(raw_text, hex_start, 2) {
                    return Some((vec![(value, u32::try_from(start).ok()?)], next_index));
                }
                return Some((Vec::new(), next_index));
            }
            if next == 'u' {
                let brace_start = next_start + next.len_utf8();
                if raw_text.as_bytes().get(brace_start).copied() == Some(b'{') {
                    let hex_start = brace_start + 1;
                    let mut hex_end = hex_start;
                    while hex_end < class_end
                        && raw_text.as_bytes().get(hex_end).copied() != Some(b'}')
                    {
                        hex_end += 1;
                    }
                    if hex_end < class_end
                        && let Some(value) = parse_hex_u32(raw_text, hex_start, hex_end - hex_start)
                    {
                        return Some((vec![(value, u32::try_from(start).ok()?)], hex_end + 1));
                    }
                } else if let Some(value) = parse_hex_u32(raw_text, brace_start, 4) {
                    let next_index = brace_start + 4;
                    if unicode_mode
                        && let Some(after_first) = raw_text.get(next_index..class_end)
                        && after_first.starts_with("\\u")
                        && let Some(low) = parse_hex_u32(raw_text, next_index + 2, 4)
                        && let Some(code_point) = decode_surrogate_pair(value, low)
                    {
                        return Some((
                            vec![(code_point, u32::try_from(start).ok()?)],
                            next_index + 6,
                        ));
                    }
                    return Some((vec![(value, u32::try_from(start).ok()?)], next_index));
                }
            }

            let escaped_start = next_start;
            let escaped = raw_text.get(escaped_start..class_end)?.chars().next()?;
            if matches!(escaped, 'd' | 'D' | 's' | 'S' | 'w' | 'W' | 'p' | 'P') {
                return Some((Vec::new(), escaped_start + escaped.len_utf8()));
            }
            if unicode_mode {
                Some((
                    vec![(escaped as u32, u32::try_from(start).ok()?)],
                    escaped_start + escaped.len_utf8(),
                ))
            } else {
                Some((
                    escaped
                        .encode_utf16(&mut [0; 2])
                        .iter()
                        .zip(split_non_unicode_atom_offsets(start, escaped))
                        .map(|(u, offset)| (*u as u32, offset))
                        .collect(),
                    escaped_start + escaped.len_utf8(),
                ))
            }
        } else if unicode_mode {
            Some((
                vec![(ch as u32, u32::try_from(start).ok()?)],
                start + ch.len_utf8(),
            ))
        } else {
            Some((
                ch.encode_utf16(&mut [0; 2])
                    .iter()
                    .zip(split_non_unicode_atom_offsets(start, ch))
                    .map(|(u, offset)| (*u as u32, offset))
                    .collect(),
                start + ch.len_utf8(),
            ))
        }
    }

    /// Analyse one character class, starting just past its `[`, and
    /// push range-order (TS1517) errors for the ranges it contains.
    /// Under the `v` flag a class may contain further classes, and a
    /// class that has committed to a `--`/`&&` class-set operator
    /// contains no ranges at all — but its nested classes still do, so
    /// the walk recurses instead of skipping.
    fn analyze_class_ranges(
        raw_text: &str,
        i: &mut usize,
        body_end: usize,
        unicode_mode: bool,
        unicode_sets_mode: bool,
        errors: &mut Vec<(u32, u32)>,
    ) {
        let bytes = raw_text.as_bytes();
        let mut tokens: Vec<ClassToken> = Vec::new();
        let mut in_class_set_expression = false;

        while *i < body_end {
            if bytes[*i] == b']' {
                *i += 1;
                break;
            }
            // `--` and `&&` commit the class to a `ClassSetExpression`,
            // which admits no ranges.
            if unicode_sets_mode
                && *i + 1 < body_end
                && ((bytes[*i] == b'-' && bytes[*i + 1] == b'-')
                    || (bytes[*i] == b'&' && bytes[*i + 1] == b'&'))
            {
                in_class_set_expression = true;
                *i += 2;
                continue;
            }
            if bytes[*i] == b'-' {
                tokens.push(ClassToken::Hyphen);
                *i += 1;
                continue;
            }
            // A nested class is never a range bound, so it contributes
            // no code point to compare — but it is a class in its own
            // right and carries its own ranges.
            if unicode_sets_mode && bytes[*i] == b'[' {
                tokens.push(ClassToken::OpaqueAtom);
                *i += 1;
                analyze_class_ranges(
                    raw_text,
                    i,
                    body_end,
                    unicode_mode,
                    unicode_sets_mode,
                    errors,
                );
                continue;
            }
            // `\q{ ... }` is a class-string-disjunction operand, not a
            // code point: it spans through its closing brace and, like
            // a nested class, can never bound a range. Letting the
            // generic atom walk see it splits it into `q`, `{`, the
            // alternatives and `}`, and the brace then compares as a
            // range bound.
            if unicode_sets_mode
                && bytes[*i] == b'\\'
                && bytes.get(*i + 1).copied() == Some(b'q')
                && bytes.get(*i + 2).copied() == Some(b'{')
            {
                tokens.push(ClassToken::OpaqueAtom);
                *i += 3;
                while *i < body_end && bytes[*i] != b'}' {
                    *i += 1;
                }
                if *i < body_end {
                    *i += 1;
                }
                continue;
            }
            let Some((atoms, next_i)) = parse_class_atom(raw_text, *i, body_end, unicode_mode)
            else {
                break;
            };
            if atoms.is_empty() {
                tokens.push(ClassToken::OpaqueAtom);
            } else {
                tokens.extend(
                    atoms
                        .into_iter()
                        .map(|(value, start)| ClassToken::Atom { value, start }),
                );
            }
            *i = next_i;
        }

        if in_class_set_expression {
            return;
        }

        let mut token_index = 0usize;
        while token_index + 2 < tokens.len() {
            match &tokens[token_index..token_index + 3] {
                [
                    ClassToken::Atom { value: left, start },
                    ClassToken::Hyphen,
                    ClassToken::Atom { value: right, .. },
                ] => {
                    if left > right {
                        errors.push((*start, 1));
                    }
                    token_index += 3;
                }
                _ => token_index += 1,
            }
        }
    }

    let flags = &raw_text[body_end + 1..];
    let unicode_mode = flags.contains('u') || flags.contains('v');
    let unicode_sets_mode = flags.contains('v');
    let bytes = raw_text.as_bytes();
    let mut errors = Vec::new();
    let mut i = 1usize;

    while i < body_end {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i < body_end {
                    i += 1;
                }
            }
            b'[' => {
                i += 1;
                analyze_class_ranges(
                    raw_text,
                    &mut i,
                    body_end,
                    unicode_mode,
                    unicode_sets_mode,
                    &mut errors,
                );
            }
            _ => {
                if let Some(ch) = raw_text.get(i..body_end).and_then(|s| s.chars().next()) {
                    i += ch.len_utf8();
                } else {
                    break;
                }
            }
        }
    }

    errors
}
