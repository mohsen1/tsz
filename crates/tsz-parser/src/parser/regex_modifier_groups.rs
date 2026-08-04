//! Subpattern modifier groups inside regular expression literals: `(?ims-ims: … )`.
//!
//! A group that opens `(?` and is not followed by `=`, `!` or `<` is a
//! *modifier group* in tsc's grammar (`scanRegularExpressionWorker`'s `default`
//! arm in `scanner.ts`). Its prelude is an optional run of flags, an optional
//! `-` plus a second run, and then a mandatory `:` — so `(?:` is the degenerate
//! modifier group with two empty runs rather than a separate syntactic form.
//!
//! The rules the runs carry are the same ones tsc's `scanPatternModifiers`
//! applies, in this order per character: an identifier character that is not a
//! regular expression flag is TS1499, a flag already present in the accumulated
//! set is TS1500, a flag outside the toggleable set (`i`, `m`, `s`) is TS1509,
//! and anything else joins the set. The second run starts from the first run's
//! flags, which is why `(?i-i:x)` is a duplicate rather than a toggle.
//!
//! Kept out of `state_expressions_literals_regex.rs` deliberately: that file is
//! already past the 2000-line shard limit, so regex sub-grammars with a
//! self-contained owner live beside it instead of inside it.

use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

use super::state::ParserState;

/// Flag bits, mirroring tsc's `RegularExpressionFlags` so the toggleable mask
/// below can be read against `scanner.ts` directly.
const FLAG_HAS_INDICES: u32 = 1 << 0;
const FLAG_GLOBAL: u32 = 1 << 1;
const FLAG_IGNORE_CASE: u32 = 1 << 2;
const FLAG_MULTILINE: u32 = 1 << 3;
const FLAG_DOT_ALL: u32 = 1 << 4;
const FLAG_UNICODE: u32 = 1 << 5;
const FLAG_UNICODE_SETS: u32 = 1 << 6;
const FLAG_STICKY: u32 = 1 << 7;

/// The only flags a subpattern may toggle (tsc's `RegularExpressionFlags.Modifiers`).
const TOGGLEABLE_IN_SUBPATTERN: u32 = FLAG_IGNORE_CASE | FLAG_MULTILINE | FLAG_DOT_ALL;

/// Map a regular expression flag character to its bit, or `None` when the
/// character is not a flag at all (tsc's `characterCodeToRegularExpressionFlag`).
const fn regular_expression_flag(ch: char) -> Option<u32> {
    match ch {
        'd' => Some(FLAG_HAS_INDICES),
        'g' => Some(FLAG_GLOBAL),
        'i' => Some(FLAG_IGNORE_CASE),
        'm' => Some(FLAG_MULTILINE),
        's' => Some(FLAG_DOT_ALL),
        'u' => Some(FLAG_UNICODE),
        'v' => Some(FLAG_UNICODE_SETS),
        'y' => Some(FLAG_STICKY),
        _ => None,
    }
}

/// Decode the character at `pos`, bounded by `end`.
pub(crate) fn next_utf8_char(bytes: &[u8], end: usize, pos: usize) -> Option<(char, usize)> {
    bytes
        .get(pos..end)
        .and_then(|slice| std::str::from_utf8(slice).ok())
        .and_then(|slice| slice.chars().next())
        .map(|ch| (ch, ch.len_utf8()))
}

/// Scan one run of subpattern modifier flags, seeded with `current_flags`, and
/// return the accumulated set. Mirrors tsc's `scanPatternModifiers`: the run
/// ends at the first character that is not an identifier part, so a bad flag is
/// reported and consumed rather than terminating the run.
pub(crate) fn scan_pattern_modifiers(
    parser: &mut ParserState,
    emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
    body: &[u8],
    end: usize,
    pos: &mut usize,
    current_flags: u32,
) -> u32 {
    let mut flags = current_flags;

    while *pos < end {
        let Some((ch, char_len)) = next_utf8_char(body, end, *pos) else {
            break;
        };

        // tsc terminates flag scanning with `isIdentifierPart`; route through
        // the scanner predicate so termination matches identifier scanning.
        if !tsz_scanner::is_ecmascript_identifier_part(ch) {
            break;
        }

        let char_size = u32::try_from(char_len).unwrap_or(1);
        match regular_expression_flag(ch) {
            None => emit(
                parser,
                *pos,
                char_size,
                diagnostic_messages::UNKNOWN_REGULAR_EXPRESSION_FLAG,
                diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG,
            ),
            Some(flag) if flags & flag != 0 => emit(
                parser,
                *pos,
                char_size,
                diagnostic_messages::DUPLICATE_REGULAR_EXPRESSION_FLAG,
                diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG,
            ),
            Some(flag) if flag & TOGGLEABLE_IN_SUBPATTERN == 0 => emit(
                parser,
                *pos,
                char_size,
                diagnostic_messages::THIS_REGULAR_EXPRESSION_FLAG_CANNOT_BE_TOGGLED_WITHIN_A_SUBPATTERN,
                diagnostic_codes::THIS_REGULAR_EXPRESSION_FLAG_CANNOT_BE_TOGGLED_WITHIN_A_SUBPATTERN,
            ),
            Some(flag) => {
                // tsc calls `checkRegularExpressionFlagAvailability` only on
                // the accepted branch, i.e. after TS1509 has already rejected
                // every flag outside `i`/`m`/`s`. Of those three only `s` is
                // version-gated, so the subpattern path can reach exactly one
                // availability answer — the wider flag/version table stays
                // owned by the checker's trailing-flag pass.
                if flag == FLAG_DOT_ALL && !parser.language_version.supports_es2018() {
                    let message = format_message(
                        diagnostic_messages::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER,
                        &["es2018"],
                    );
                    emit(
                        parser,
                        *pos,
                        char_size,
                        &message,
                        diagnostic_codes::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER,
                    );
                }
                flags |= flag;
            }
        }

        *pos += char_len;
    }

    flags
}

/// Scan the prelude of a modifier group — the flag runs, the optional `-`, and
/// the mandatory `:` — leaving `pos` just past the `:` when one is present.
///
/// `(?-:x)` is the shape TS1504 exists for: tsc detects it by position, not by
/// flag set, reporting when the whole prelude consumed exactly the minus sign.
/// A prelude with flags on either side of the minus (`(?i-:x)`, `(?-i:x)`) is
/// legal, so a flag-set test would report where tsc does not.
pub(crate) fn scan_modifier_group_prelude(
    parser: &mut ParserState,
    emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
    body: &[u8],
    end: usize,
    pos: &mut usize,
) {
    let prelude_start = *pos;
    let set_flags = scan_pattern_modifiers(parser, emit, body, end, pos, 0);

    if *pos < end && body[*pos] == b'-' {
        *pos += 1;
        scan_pattern_modifiers(parser, emit, body, end, pos, set_flags);

        if *pos == prelude_start + 1 {
            emit(
                parser,
                prelude_start,
                u32::try_from(*pos - prelude_start).unwrap_or(1),
                diagnostic_messages::SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN,
                diagnostic_codes::SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN,
            );
        }
    }

    // tsc's `scanExpectedChar(colon)`: consume the `:` or report a zero-width
    // TS1005 where it should have been, and keep scanning the disjunction
    // either way. There is deliberately no backtracking here — a group that
    // opened `(?` is a modifier group even when it is malformed, so it must
    // not be re-scanned as pattern characters.
    if *pos < end && body[*pos] == b':' {
        *pos += 1;
    } else {
        emit(parser, *pos, 0, "':' expected.", diagnostic_codes::EXPECTED);
    }
}
