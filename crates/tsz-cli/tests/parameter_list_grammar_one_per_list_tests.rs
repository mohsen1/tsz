//! tsc reports **at most one** parameter-list ordering-grammar diagnostic per
//! parameter list: `checkGrammarParameterList` walks the list once and every
//! arm is `return grammarErrorOnNode(...)`, so the first offending parameter
//! wins and the walk stops (#16644).
//!
//! tsz split this family across layers — TS1015/TS1016 are checker-owned
//! (`check_parameter_ordering`), while the rest-parameter grammar (TS1014
//! rest-not-last, TS1047 rest-optional, TS1048 rest-initializer) is
//! parser-emitted — so before the fix the declaration path both (a) kept
//! reporting TS1016 for every required parameter after the optional run
//! instead of only the first, and (b) let a later parser-emitted TS1014 ride
//! along behind a checker TS1016 that tsc's early return had already made the
//! sole winner.
//!
//! The checker's `check_parameter_ordering` now early-returns at the first
//! violation and records the spans of any rest parameters that follow a
//! checker-owned winner; the driver drops the parser's rest-grammar
//! diagnostics anchored there (`suppress_parameter_grammar_losers`).
//!
//! Expectations are oracle-pinned against `typescript@7.0.2`
//! (`--noEmit --strict --pretty false --target es2022 --lib es2022`).

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile a single-file `source` with `--strict` and return its diagnostics.
fn compile_source(source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("main.ts"), source).expect("write repro file");

    let argv = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        "main.ts",
    ];
    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn count_code(diagnostics: &[Diagnostic], code: u32) -> usize {
    diagnostics.iter().filter(|d| d.code == code).count()
}

/// d1 witness: three parameters, one optional followed by two required. tsc
/// reports a single TS1016 on the first required parameter, not one per
/// required parameter.
#[test]
fn required_after_optional_reports_single_ts1016() {
    let diags = compile_source("function d1(a?: number, b: string, c: string) {}");
    assert_eq!(
        count_code(&diags, 1016),
        1,
        "expected exactly one TS1016 for `(a?, b, c)`, got: {diags:?}"
    );
}

/// d4 witness: a checker-owned TS1016 wins over a later parser-emitted TS1014
/// (rest-not-last). tsc returns at the TS1016 and never reaches the rest
/// parameter, so the TS1014 must not surface.
#[test]
fn ts1016_winner_suppresses_later_rest_not_last_ts1014() {
    let diags = compile_source("function d4(a?: number, b: string, ...c: any[], d: any) {}");
    assert_eq!(
        count_code(&diags, 1016),
        1,
        "expected exactly one TS1016 for `(a?, b, ...c, d)`, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 1014),
        0,
        "TS1014 must be suppressed behind the earlier TS1016, got: {diags:?}"
    );
}

/// The one-per-list rule holds across every signature form, not just function
/// declarations.
#[test]
fn required_after_optional_single_ts1016_across_signature_forms() {
    let cases = [
        (
            "function expression",
            "const f = function (a?: number, b: string, c: string) {};",
        ),
        (
            "arrow function",
            "const f = (a?: number, b: string, c: string) => {};",
        ),
        (
            "method",
            "class K { m(a?: number, b: string, c: string) {} }",
        ),
        (
            "constructor",
            "class K { constructor(a?: number, b: string, c: string) {} }",
        ),
        (
            "object-literal method",
            "const o = { m(a?: number, b: string, c: string) {} };",
        ),
        (
            "interface method signature",
            "interface I { m(a?: number, b: string, c: string): void; }",
        ),
        (
            "interface call signature",
            "interface I { (a?: number, b: string, c: string): void; }",
        ),
        (
            "interface construct signature",
            "interface I { new (a?: number, b: string, c: string): void; }",
        ),
    ];
    for (label, source) in cases {
        let diags = compile_source(source);
        assert_eq!(
            count_code(&diags, 1016),
            1,
            "expected exactly one TS1016 for {label} `{source}`, got: {diags:?}"
        );
    }
}

/// Negative control: rest-not-last with no earlier ordering violation must
/// still report TS1014 — nothing preceded it, so it is the list's own winner.
#[test]
fn lone_rest_not_last_still_reports_ts1014() {
    let diags = compile_source("function f(...a: any[], b: number) {}");
    assert_eq!(
        count_code(&diags, 1014),
        1,
        "a lone rest-not-last must still report TS1014, got: {diags:?}"
    );
}

/// Negative control: the suppression must be scoped to the parameter that
/// actually follows the winner. A nested function inside a *default value* of
/// a parameter that comes *before* the outer winner runs its own
/// `checkGrammarParameterList`, so its rest-not-last TS1014 must survive
/// alongside the outer TS1016.
#[test]
fn nested_function_rest_grammar_survives_outer_ts1016() {
    let diags = compile_source(
        "function outer(a?: number, b: () => void = function (...x: any[], y: number) {}, c: number) {}",
    );
    assert_eq!(
        count_code(&diags, 1016),
        1,
        "outer list should report exactly one TS1016, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 1014),
        1,
        "the nested function's rest-not-last TS1014 must survive, got: {diags:?}"
    );
}

/// Negative control: a required-after-optional violation is unrelated to a
/// separate valid signature — each list is judged independently, so a clean
/// sibling list stays clean.
#[test]
fn clean_sibling_list_stays_clean() {
    let diags = compile_source(
        "function bad(a?: number, b: string) {}\nfunction good(a: number, b?: string) {}",
    );
    assert_eq!(
        count_code(&diags, 1016),
        1,
        "only the `bad` list should report TS1016, got: {diags:?}"
    );
}
