//! Unit tests for the containment
//! `is_parser_grammar_code` ⊆ `is_non_suppressing_parse_error`.
//!
//! The two predicates answer different questions about the same set of codes,
//! and before this containment was stated they disagreed on ~65 of ~70 members.
//!
//! - `is_parser_grammar_code` means "tsc emits this from the checker via
//!   `grammarErrorOnNode`; tsz emits it from the parser instead". It decides
//!   whether the code is itself suppressed when a sibling parse error exists.
//! - `is_non_suppressing_parse_error` decides whether the code sets
//!   `ctx.has_syntax_parse_errors`, tsz's stand-in for tsc's
//!   `hasParseDiagnostics(sourceFile)`.
//!
//! A diagnostic tsc raises from the checker never lands in
//! `sourceFile.parseDiagnostics`, so `hasParseDiagnostics` stays `false` and
//! every other checker grammar check in the file still runs. The first
//! predicate's contract therefore *implies* the second's answer, and the two
//! cannot legally disagree on any code.
//!
//! # Oracle evidence
//!
//! Pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --module esnext --pretty false`). Each
//! witness below was compiled in a file that also contains
//!
//! ```text
//! function outer() { const g = function () { return await 1; }; return g; }
//! ```
//!
//! whose TS1308 (`'await' expressions are only allowed within async functions
//! and at the top levels of modules.`) is the companion being counted. tsc
//! reported TS1308 alongside the grammar code in **every** case below. Before
//! the containment, tsz reported the grammar code alone in every case below.
//!
//! The probe is discriminating rather than vacuous for the same reason the
//! regex-band probe is: a genuine structural error in the same slot
//! (`const broken = ;`, TS1109) *does* drop the companion, in both compilers.

use super::*;

/// Grammar codes paired with the source line that provokes them, alongside the
/// TS1308 companion described in the module docs.
///
/// Deliberately spans the whole shape range of `is_parser_grammar_code`, not
/// just the convenient members: modifier-order and modifier-duplication checks,
/// statement-level checks, the strict-mode pair, decorator placement, and — the
/// cases most likely to have justified suppression — the "list cannot be empty"
/// and "declaration must have an initializer" family, where the parser really
/// does recover a malformed node. tsc keeps the companion for all of them.
const GRAMMAR_WITNESSES: &[(u32, &str)] = &[
    (1029, "class D { static public x = 1; }"),
    (1028, "class E { public public y = 1; }"),
    (1040, "declare class I { async m(): void; }"),
    (1097, "class G implements { }"),
    (1098, "function tp<>() { return 1; }"),
    (1099, "type TA = Array<>;"),
    (
        1113,
        "function s(v: number) { switch (v) { default: break; default: break; } }",
    ),
    (1114, "function l() { a: b: a: while (true) { break a; } }"),
    (1123, "var ;"),
    (1163, "function ng() { const q = yield 1; return q; }"),
    (1182, "function dd() { let { p }; return p; }"),
    (1200, "const ar = ()\n=> 1;"),
    (1206, "@dec function bad() {}\ndeclare const dec: any;"),
    (18037, "class C { static { const c = await 4; } }"),
];

/// Every code the parser-grammar predicate claims must also be non-suppressing.
///
/// This is the invariant itself, checked over the whole `u32` diagnostic range
/// rather than over a hand-kept list, so a code added to `is_parser_grammar_code`
/// in a later audit cannot reopen the gap by omission. `#16279`'s audit added
/// three codes to that list in one PR; without this test each would have
/// silently started deleting its own file's checker diagnostics.
#[test]
fn every_parser_grammar_code_is_non_suppressing() {
    let mut offenders = Vec::new();
    for code in 0..20_000_u32 {
        if is_parser_grammar_code(code) && !is_non_suppressing_parse_error(code) {
            offenders.push(code);
        }
    }
    assert!(
        offenders.is_empty(),
        "these codes are classified as checker-side grammar checks by \
         is_parser_grammar_code, yet still set has_syntax_parse_errors: {offenders:?}. \
         A code tsc raises from the checker is never in sourceFile.parseDiagnostics, \
         so it cannot participate in hasParseDiagnostics() suppression."
    );
}

/// The disjointness invariant: no `is_parser_grammar_code` member may also be
/// an `is_real_syntax_error` or `is_structural_parse_error`, checked over the
/// whole `u32` range rather than a hand-kept list.
///
/// A parser grammar code fires on a *well-formed* AST (tsc's checker-side
/// `grammarErrorOnNode`), so it must not drive `has_real_syntax_errors` or the
/// structural-cascade heuristic — doing so both self-suppresses the code (it
/// becomes its own `filtered_parse_diagnostics` trigger) and deletes its file's
/// semantic siblings. This exact overlap has bitten twice by omission: TS1313
/// (`#16279` round 10) and TS1155 (`#17253`), each speculatively listed as a
/// structural/real parse error and only caught after it started deleting
/// TS2588/TS7005/TS1054 in the field. Round 10 and `#17253` each removed the
/// offending code and added per-code `!is_real_syntax_error`/
/// `!is_structural_parse_error` assertions; this whole-range guard generalizes
/// those so a code added to `is_parser_grammar_code` in a future audit cannot
/// reopen the class by omission, mirroring
/// [`every_parser_grammar_code_is_non_suppressing`] one function up.
#[test]
fn no_parser_grammar_code_is_a_real_or_structural_parse_error() {
    let mut real_offenders = Vec::new();
    let mut structural_offenders = Vec::new();
    for code in 0..20_000_u32 {
        if is_parser_grammar_code(code) {
            if is_real_syntax_error(code) {
                real_offenders.push(code);
            }
            if is_structural_parse_error(code) {
                structural_offenders.push(code);
            }
        }
    }
    assert!(
        real_offenders.is_empty(),
        "these codes are checker-side grammar checks (is_parser_grammar_code) yet \
         also classified as real syntax errors: {real_offenders:?}. That makes each \
         its own filtered_parse_diagnostics trigger (self-suppression) and sets \
         has_real_syntax_errors, deleting the file's semantic siblings — the TS1313 \
         (#16279 round 10) / TS1155 (#17253) regression shape."
    );
    assert!(
        structural_offenders.is_empty(),
        "these codes are checker-side grammar checks (is_parser_grammar_code) yet \
         also classified as structural parse errors: {structural_offenders:?}. A \
         grammar check runs on a well-formed AST and must not drive the \
         cascading-suppression heuristic (#17253)."
    );
}

/// The oracle-pinned witnesses, asserted individually so a failure names the
/// code and the source that produced it rather than only a set difference.
#[test]
fn oracle_pinned_grammar_witnesses_are_non_suppressing() {
    for &(code, witness) in GRAMMAR_WITNESSES {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} must not set has_syntax_parse_errors: typescript@7.0.2 \
             reports TS1308 alongside it for `{witness}`, so tsc's \
             hasParseDiagnostics() is false for this file"
        );
    }
}

/// Guards the direction that makes the probe meaningful.
///
/// A real structural parse failure must keep suppressing, otherwise the
/// containment above would be indistinguishable from deleting
/// `has_syntax_parse_errors` altogether. TS1109 (`Expression expected.`),
/// TS1005 (`'{0}' expected.`) and TS1128 (`Declaration or statement expected.`)
/// are the structural codes the oracle probe used as its negative control.
#[test]
fn structural_parse_failures_still_suppress() {
    for code in [1005_u32, 1109, 1128] {
        assert!(
            !is_non_suppressing_parse_error(code),
            "TS{code} is a structural parse failure and must keep setting \
             has_syntax_parse_errors; tsc drops the file's other checker \
             diagnostics for it"
        );
        assert!(
            !is_parser_grammar_code(code),
            "TS{code} is a structural parse failure, not a checker-side grammar \
             check; listing it in is_parser_grammar_code would also stop it from \
             counting as the trigger in filtered_parse_diagnostics"
        );
    }
}

/// The five codes that already overlapped before the containment was stated.
///
/// They stay enumerated in `is_non_suppressing_parse_error` for documentation
/// value; this pins that the containment now answers for them either way, so a
/// later cleanup of the redundant enumeration cannot change behaviour.
#[test]
fn previously_overlapping_codes_still_answer_both_ways() {
    for code in [1014_u32, 1047, 1048, 1096, 1191] {
        assert!(
            is_parser_grammar_code(code),
            "TS{code} left the grammar list"
        );
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} must remain non-suppressing"
        );
    }
}
