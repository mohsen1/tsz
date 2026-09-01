//! Unit tests for the regular-expression grammar band of
//! `is_non_suppressing_parse_error`, plus a tripwire that keeps the band from
//! silently reopening.
//!
//! tsc never puts a regex grammar diagnostic in `parseDiagnostics`: its regex
//! validation runs from the checker, which re-scans the literal through
//! `scanner.scanRange`, so a malformed pattern cannot participate in
//! `hasParseDiagnostics()` suppression. tsz validates the pattern in
//! `crates/tsz-parser/src/parser/state_expressions_literals_regex.rs` during
//! parsing instead, which puts the same diagnostics in `parse_diagnostics` —
//! where they set `has_syntax_parse_errors` and delete unrelated real
//! diagnostics from the whole file.
//!
//! Every code asserted below was pinned against `typescript@7.0.2` with a
//! fixture pairing the regex literal with TS1039, TS2304, TS2322 and TS2339.
//! tsc reports all four companions in every case; the same fixture with a
//! genuine structural error (`const broken = ;`, TS1109) drops all four.

use super::*;

/// Codes emitted by `state_expressions_literals_regex.rs` that are unique to
/// the regex grammar walk, each with the oracle witness it was pinned on.
///
/// Codes that walk shares with non-regex contexts (TS1005, TS1125, TS1161,
/// TS1198) are excluded on purpose: this predicate is keyed on the code, not
/// on the emitting site, and each of those is a real parse failure elsewhere.
const REGEX_GRAMMAR_CODES: &[(u32, &str)] = &[
    (1487, r"/[\0]/u"),
    (1499, "/a/q"),
    (1500, "/a/gg"),
    (1501, "/(?s:x)/"), // parser-side only for a subpattern flag; see below
    (1502, "/a/uv"),
    (1504, "/(?-:x)/"),
    (1509, "/(?g:x)/"),
    (1511, r"/\q{a}/v"),
    (1505, "/a{1,/u"),
    (1506, "/a{2,1}/"),
    (1507, "/{1}/u"),
    (1508, "/[a[b]]/u"),
    (1510, r"/\k/u"),
    (1512, r"/\c1/u"),
    (1516, r"/[a-\d]/u"),
    (1517, "/[b-a]/"),
    (1519, r"/[a&&\d--\w]/v"),
    (1520, "/[a--]/v"),
    (1522, "/[a!!b]/v"),
    (1523, r"/\p{=x}/u"),
    (1524, r"/\p{Foo=Bar}/u"),
    (1525, r"/\p{Script=}/u"),
    (1526, r"/\p{Script=NotAScript}/u"),
    (1527, r"/\p{}/u"),
    (1528, r"/\p{RGI_Emoji}/u"),
    (1529, r"/\p{NotAThing}/u"),
    (1530, r"/\p{L}/"),
    (1531, r"/\p/u"),
    (1533, r"/(a)\2/"),
    (1534, r"/\1/"),
    (1535, r"/\y/u"),
    (1536, r"/[\1]/u"),
    (1537, r"/[\8]/u"),
    (1538, r"/\u{61}/"),
];

/// The contiguous regular-expression grammar band in upstream's diagnostics
/// table, from `Unknown regular expression flag.` to `Unicode escape sequences
/// are only available when the Unicode (u) flag …`.
///
/// `is_non_suppressing_parse_error` matches this as a range. The witness table
/// above is the subset someone has already wired and oracle-pinned; the band is
/// what actually has to hold, including for codes not yet emitted anywhere
/// (`1503`, `1513`, `1518`, `1521`).
const REGEX_GRAMMAR_BAND: std::ops::RangeInclusive<u32> = 1499..=1538;

/// The message upstream assigns to `code`, or `None` for an unallocated code.
///
/// Reads the generated table rather than a copy of it, so these tests observe
/// exactly what `sync-typescript-diagnostics` produced.
fn diagnostic_message_for_code(code: u32) -> Option<String> {
    tsz_common::diagnostics::data::iter_diagnostic_messages()
        .find(|message| message.code == code)
        .map(|message| message.message.to_owned())
}

/// Every `NAME -> code` row in the generated diagnostics table.
///
/// The audit below scrapes `diagnostic_codes::NAME` references out of the
/// parser's regex modules as text, so it needs a way to turn those names back
/// into codes before it can ask the predicate about them. Parsing the generated
/// table's own single-declaration rows (`(NAME, code, Category, "message")`) is
/// that way — it is the same source the `codes` and `templates` modules expand
/// from, so it cannot drift from them.
fn diagnostic_codes_by_name() -> std::collections::HashMap<String, u32> {
    const TABLE_PARTS: &[&str] = &[
        include_str!("../../../../tsz-common/src/diagnostics/data/parts/part_000.rs"),
        include_str!("../../../../tsz-common/src/diagnostics/data/parts/part_001.rs"),
        include_str!("../../../../tsz-common/src/diagnostics/data/parts/part_002.rs"),
        include_str!("../../../../tsz-common/src/diagnostics/data/parts/part_003.rs"),
    ];

    let mut by_name = std::collections::HashMap::new();
    for part in TABLE_PARTS {
        for line in part.lines() {
            let row = line.trim_start();
            let Some(row) = row.strip_prefix('(') else {
                continue;
            };
            let Some((name, rest)) = row.split_once(',') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            {
                continue;
            }
            let code = rest.trim_start();
            let end = code
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(code.len());
            if let Ok(code) = code[..end].parse::<u32>() {
                by_name.insert(name.to_owned(), code);
            }
        }
    }

    assert!(
        by_name.len() > 1000,
        "parsed only {} rows out of the generated diagnostics table; the row \
         format changed and this extraction is broken",
        by_name.len()
    );
    by_name
}

#[test]
fn every_regex_grammar_code_is_non_suppressing() {
    for &(code, witness) in REGEX_GRAMMAR_CODES {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} ({witness}) is emitted by tsz's regex grammar walk into \
             parse_diagnostics, but tsc reports it from the checker and keeps \
             every companion diagnostic in the file. Without an entry in \
             is_non_suppressing_parse_error it sets has_syntax_parse_errors and \
             deletes unrelated TS1039/TS2304/TS2322/TS2339 from the whole file."
        );
    }
}

/// The predicate must cover the regex band as a *range*, including the codes no
/// witness above exercises yet.
///
/// The witness table is necessarily a list of what someone has already wired.
/// Every whole-file suppression bug in this family so far landed on a code that
/// was not in such a list at the time — TS1511, then TS1501/1504/1509, then
/// TS1514/1515 — so a test that only walks the witnesses cannot catch the next
/// one. This walks the band instead.
#[test]
fn the_whole_regex_grammar_band_is_non_suppressing() {
    for code in REGEX_GRAMMAR_BAND {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} is inside the TS{}..=TS{} regular-expression grammar band, \
             where tsc reports every code from the checker rather than the parser, \
             so it must never set has_syntax_parse_errors. Match the band as a \
             range; do not re-enumerate it.",
            REGEX_GRAMMAR_BAND.start(),
            REGEX_GRAMMAR_BAND.end(),
        );
    }
}

/// The band's edges, so widening it is a deliberate act rather than a drift.
///
/// These two codes are the non-regex neighbours in upstream's own allocation.
/// If a future `sync-typescript-diagnostics` run moves the boundary, this fails
/// instead of the predicate silently swallowing a real parse failure.
#[test]
fn the_codes_bounding_the_regex_band_are_not_regex_grammar() {
    for (code, message) in [
        (1498u32, "Invalid syntax in decorator."),
        (
            1539u32,
            "A 'bigint' literal cannot be used as a property name.",
        ),
    ] {
        assert_eq!(
            diagnostic_message_for_code(code).as_deref(),
            Some(message),
            "TS{code} bounds the regular-expression grammar band and is expected \
             to carry a non-regex message. Upstream reallocated it, so re-derive \
             the band in is_non_suppressing_parse_error before trusting the range."
        );
    }

    assert!(
        !is_non_suppressing_parse_error(1498),
        "TS1498 sits just below the regex band and is a real parse failure; \
         widening the range to include it would suppress real diagnostics."
    );
    assert!(
        !is_non_suppressing_parse_error(1539),
        "TS1539 sits just above the regex band and is a real parse failure; \
         widening the range to include it would suppress real diagnostics."
    );
}

/// Every message inside the band really is a regular-expression grammar message.
///
/// This is what licenses matching the band as a range at all. The diagnostics
/// table is generated verbatim from TypeScript's `diagnosticMessages.json`, so
/// the band is upstream's allocation, not a shape tsz imposes — but an upstream
/// sync could in principle drop a non-regex message into a gap. Each row is
/// identified by its own message text, so a reallocation fails here.
#[test]
fn every_message_in_the_regex_band_is_regex_grammar() {
    // Wording that only a regular-expression grammar message uses. Kept as
    // whole phrases rather than loose substrings so an unrelated message cannot
    // pass by accident.
    const REGEX_MARKERS: &[&str] = &[
        "regular expression",
        "character class",
        "capturing group",
        "Unicode property",
        "quantifier",
        "backreference",
        "class set operand",
        "Unicode (u) flag",
        "repetition",
        "character escape",
        "string alternatives",
        "negated",
        // Regex messages whose wording names no regex noun at all. Each is
        // distinctive enough that no non-regex diagnostic shares it.
        "Subpattern flags",         // TS1504
        "escape it with backslash", // TS1508
        "ASCII letter",             // TS1512
    ];

    for code in REGEX_GRAMMAR_BAND {
        let Some(message) = diagnostic_message_for_code(code) else {
            continue; // A gap in upstream's allocation is fine; nothing emits it.
        };
        assert!(
            REGEX_MARKERS.iter().any(|marker| message.contains(marker)),
            "TS{code} is inside the regular-expression grammar band that \
             is_non_suppressing_parse_error matches as a range, but its message \
             does not read like a regex grammar message:\n  {message}\nEither \
             upstream reallocated this code — in which case narrow the band and \
             re-pin it — or add the new wording to REGEX_MARKERS."
        );
    }
}

/// Tripwire. Every diagnostic the regex validator can emit must be
/// non-suppressing, whoever wired it and whenever.
///
/// This scrapes the `diagnostic_codes::` constants the validator references,
/// resolves each one to its actual code through the generated diagnostics
/// table, and asks `is_non_suppressing_parse_error` about it — so it is keyed
/// on the same thing the predicate is keyed on. With the TS1499..=TS1538 band
/// matched as a range, a newly wired regex diagnostic passes here by
/// construction; what remains is a real check for anything emitted from the
/// regex walk that falls *outside* the band.
#[test]
fn regex_validator_diagnostic_surface_is_audited() {
    // Every parser module that emits into the regex-literal walk has to be
    // listed here, not just the main one. Splitting a sub-grammar out into its
    // own module is normal (`state_expressions_literals_regex.rs` is past the
    // 2000-line shard limit, so it will keep happening) and must not carry the
    // emit sites out of this tripwire's sight along with them.
    const VALIDATOR_SOURCES: &[(&str, &str)] = &[
        (
            "state_expressions_literals_regex.rs",
            include_str!("../../../../tsz-parser/src/parser/state_expressions_literals_regex.rs"),
        ),
        (
            "regex_modifier_groups.rs",
            include_str!("../../../../tsz-parser/src/parser/regex_modifier_groups.rs"),
        ),
    ];

    /// Constants the validator shares with non-regex parse failures. These stay
    /// out of `is_non_suppressing_parse_error` because the code, not the site,
    /// is what the predicate keys on.
    const SHARED_WITH_REAL_PARSE_FAILURES: &[&str] = &[
        "EXPECTED",
        "HEXADECIMAL_DIGIT_EXPECTED",
        "UNTERMINATED_REGULAR_EXPRESSION_LITERAL",
        "AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE",
    ];

    let mut referenced: Vec<(&str, &str)> = VALIDATOR_SOURCES
        .iter()
        .flat_map(|&(module, source)| {
            source
                .match_indices("diagnostic_codes::")
                .map(move |(at, marker)| {
                    let rest = &source[at + marker.len()..];
                    let end = rest
                        .find(|c: char| !c.is_ascii_uppercase() && !c.is_ascii_digit() && c != '_')
                        .unwrap_or(rest.len());
                    (module, &rest[..end])
                })
        })
        .collect();
    referenced.sort_unstable();
    referenced.dedup();

    let codes_by_name = diagnostic_codes_by_name();

    // Resolve each referenced constant to its actual code and ask the predicate
    // about it. This is the whole point of the rewrite: the previous version
    // compared constant NAMES against a hand-maintained allowlist, which could
    // only tell you whether a human had typed the name into a list — never
    // whether the code actually suppresses. It went red twice on main for pure
    // bookkeeping drift while the one real defect it existed to catch (TS1501,
    // whose name WAS present) sailed past it.
    let unaudited: Vec<String> = referenced
        .iter()
        .filter(|(_, name)| !SHARED_WITH_REAL_PARSE_FAILURES.contains(name))
        .filter_map(|&(module, name)| {
            let code = codes_by_name.get(name).copied()?;
            (!is_non_suppressing_parse_error(code)).then(|| format!("{module}: {name} (TS{code})"))
        })
        .collect();

    assert!(
        unaudited.is_empty(),
        "the regex validator emits diagnostic code(s) {unaudited:?} that still \
         suppress. tsz emits these at PARSE time but tsc emits the whole regex \
         grammar family at CHECK time, so a suppressing code silently deletes \
         every other diagnostic in any file containing the offending literal. \
         If the code is inside the TS1499..=TS1538 band it is covered \
         automatically and this failure means the band regressed; otherwise \
         probe it against typescript@7.0.2 with a companion fixture (TS1039 + \
         TS2304 + TS2322 + TS2339), then add it to is_non_suppressing_parse_error, \
         or to SHARED_WITH_REAL_PARSE_FAILURES with the reason."
    );

    // Every name the scan pulled out must exist in the diagnostics table. A
    // typo'd or renamed constant would otherwise resolve to nothing and be
    // silently skipped by the filter above.
    let unresolved: Vec<(&str, &str)> = referenced
        .iter()
        .copied()
        .filter(|(_, name)| !codes_by_name.contains_key(*name))
        .collect();
    assert!(
        unresolved.is_empty(),
        "constant(s) {unresolved:?} are referenced by the regex validator but do \
         not appear in the generated diagnostics table, so this audit cannot \
         classify them. Either the extraction above is broken or the constants \
         come from an aliased re-export that needs handling here."
    );

    // Non-vacuity: the scan must actually find the band, not silently match zero.
    assert!(
        referenced.len() >= REGEX_GRAMMAR_CODES.len(),
        "scan found only {} constants; the extraction is broken",
        referenced.len()
    );
}

/// A regex grammar diagnostic must not set `has_syntax_parse_errors`, while a
/// real structural error still must. This is the behaviour the band protects.
#[test]
fn regex_grammar_diagnostic_does_not_flag_syntax_parse_errors() {
    for &(code, witness) in REGEX_GRAMMAR_CODES {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} ({witness}) must not flag has_syntax_parse_errors"
        );
    }
    // Discriminating control: the codes tsc really does report from its parser
    // stay suppressing, so the band above is not just "everything passes".
    for code in [1005u32, 1109, 1125, 1128, 1161, 1198] {
        assert!(
            !is_non_suppressing_parse_error(code),
            "TS{code} is a real parse failure in tsc and must keep suppressing"
        );
    }
}
