//! TS1182 ("A destructuring declaration must have an initializer") is a grammar
//! check tsc skips in **every** ambient declaration, not just `declare`-flagged
//! ones. A destructuring `var`/`let`/`const` at the top level of a `.d.ts` file
//! is ambient by virtue of the file and never carries an initializer, so tsc
//! reports nothing there.
//!
//! tsz previously gated the suppression on `in_ambient_context()`, which only
//! tracks the `CONTEXT_FLAG_AMBIENT` set inside a `declare` — so a top-level
//! `.d.ts` destructuring declaration wrongly drew TS1182. The fix consults
//! `in_ambient_declaration()`, which also covers whole `.d.ts` files.
//!
//! Binder names are varied so the behavior is structural, not identifier-keyed.

use crate::parser::test_fixture::parse_source_named;
use tsz_common::diagnostics::diagnostic_codes;

fn codes(file_name: &str, source: &str) -> Vec<u32> {
    let (parser, _) = parse_source_named(file_name, source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn has_ts1182(file_name: &str, source: &str) -> bool {
    codes(file_name, source)
        .contains(&diagnostic_codes::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER)
}

// ---------------------------------------------------------------------------
// Ambient by file (`.d.ts`): no initializer required, so no TS1182.
// ---------------------------------------------------------------------------

#[test]
fn declaration_file_object_destructuring_no_ts1182() {
    for (name, source) in [
        ("a.d.ts", "export var { a, b }: Foo;"),
        ("lib.d.ts", "export let { first, second }: Pair;"),
        ("types.d.ts", "export const { x, y }: Point;"),
        ("m.d.mts", "export var { one }: Wrap;"),
    ] {
        assert!(
            !has_ts1182(name, source),
            "no TS1182 expected in ambient .d.ts for {source:?}, got {:?}",
            codes(name, source)
        );
    }
}

#[test]
fn declaration_file_object_rest_and_array_patterns_no_ts1182() {
    assert!(!has_ts1182("a.d.ts", "export var { a, ...rest }: Foo;"));
    assert!(!has_ts1182("a.d.ts", "export var [head, tail]: Tuple;"));
}

// ---------------------------------------------------------------------------
// Ambient by `declare` keyword: already correct, kept as a coupled guard.
// ---------------------------------------------------------------------------

#[test]
fn declare_keyword_destructuring_no_ts1182() {
    assert!(!has_ts1182("m.ts", "declare var { a, b }: Foo;"));
    assert!(!has_ts1182(
        "m.ts",
        "declare namespace N { export var { value }: Foo; }"
    ));
}

// ---------------------------------------------------------------------------
// Non-ambient: the grammar rule still applies — TS1182 must fire.
// ---------------------------------------------------------------------------

#[test]
fn non_ambient_destructuring_without_initializer_reports_ts1182() {
    for (name, source) in [
        ("m.ts", "var { a, b }: Foo;"),
        ("m.ts", "let { first }: Pair;"),
        ("app.ts", "const { x, y }: Point;"),
    ] {
        assert!(
            has_ts1182(name, source),
            "TS1182 expected for non-ambient {source:?}, got {:?}",
            codes(name, source)
        );
    }
}

#[test]
fn non_ambient_destructuring_with_initializer_no_ts1182() {
    // Sanity: a plain declaration with an initializer never trips the rule.
    assert!(!has_ts1182("m.ts", "var { a, b } = obj;"));
}
