use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    relevant_checker_diagnostics(source)
        .into_iter()
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

fn relevant_checker_diagnostics(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .collect()
}

#[test]
fn normalized_union_assignment_preserves_fresh_literal_source_display() {
    let source = r#"
let target = [
    { a: 0, flag: true },
    { a: 1, flag: false, text: "x" },
    { a: 2, flag: true, count: 1 },
][0];

target = { flag: true };
target = { flag: false };

let stringLiteralTarget: { kind: "a" } | { kind: "b", extra?: undefined };
let numberLiteralTarget: { code: 1 } | { code: 2, extra?: undefined };
stringLiteralTarget = { kind: "oops" };
numberLiteralTarget = { code: 3 };
"#;

    let diagnostics = relevant_diagnostics(source);
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    for expected in [
        "Type '{ flag: true; }' is not assignable",
        "Type '{ flag: false; }' is not assignable",
        "Type '\"oops\"' is not assignable to type '\"a\" | \"b\"'",
        "Type '3' is not assignable to type '1 | 2'",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "expected TS2322 containing {expected:?}, got {diagnostics:#?}"
        );
    }

    {
        let widened = "Type '{ flag: boolean; }'";
        assert!(
            !messages.iter().any(|message| message.contains(widened)),
            "normalized-union assignment should preserve fresh source display, got {diagnostics:#?}"
        );
    }
}

#[test]
fn generic_rest_rechecks_fresh_object_literals_against_non_fresh_candidate() {
    let source = r#"
declare function f<T>(...items: T[]): T;
declare let data: { a: 1, b: "abc", c: true };

let e3 = f(data, { a: 2 });
let e4 = f({ a: 2 }, data);
"#;

    let diagnostics = relevant_diagnostics(source);
    let literal_mismatches: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("Type '2' is not assignable to type '1'")
        })
        .collect();

    assert_eq!(
        literal_mismatches.len(),
        2,
        "expected both generic rest object literals to be checked against the non-fresh candidate, got {diagnostics:#?}"
    );
}

#[test]
fn generic_capture_still_skips_true_excess_property_checks() {
    let source = r#"
declare function capture<T extends { a: number }>(value: T): T;
declare function captureRest<T extends { a: number }>(...items: T[]): T;

capture({ a: 1, extra: 2 });
captureRest({ a: 1, extra: 2 }, { a: 2, extra: 3 });
"#;

    let diagnostics = relevant_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            matches!(*code, 2345 | 2353)
                || message.contains("Object literal may only specify known properties")
        }),
        "generic capture should not emit true excess-property diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn conformance_object_literal_normalization_keeps_ts2322_count_and_anchors() {
    let source = r#"
let a1 = [{ a: 0 }, { a: 1, b: "x" }, { a: 2, b: "y", c: true }][0];
a1.a;
a1.b;
a1.c;
a1 = { a: 1 };
a1 = { a: 0, b: 0 };
a1 = { b: "y" };
a1 = { c: true };

let a2 = [{ a: 1, b: 2 }, { a: "abc" }, {}][0];
a2.a;
a2.b;
a2 = { a: 10, b: 20 };
a2 = { a: "def" };
a2 = {};
a2 = { a: "def", b: 20 };
a2 = { a: 1 };

let d1 = [{ kind: 'a', pos: { x: 0, y: 0 } }, { kind: 'b', pos: !true ? { a: "x" } : { b: 0 } }][0];
d1.kind;
d1.pos;
d1.pos.x;
d1.pos.y;
d1.pos.a;
d1.pos.b;

declare function f<T>(...items: T[]): T;
declare let data: { a: 1, b: "abc", c: true };

let e1 = f({ a: 1, b: 2 }, { a: "abc" }, {});
let e2 = f({}, { a: "abc" }, { a: 1, b: 2 });
let e3 = f(data, { a: 2 });
let e4 = f({ a: 2 }, data);
"#;

    let diagnostics = relevant_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| !matches!(*code, 2339 | 2353)),
        "normalization should not drift into TS2339/TS2353, got {diagnostics:#?}"
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        7,
        "expected the conformance TS2322 count, got {diagnostics:#?}"
    );

    for expected in [
        "Type 'number' is not assignable to type 'string'.",
        "Type '{ b: string; }' is not assignable",
        "Type '{ c: true; }' is not assignable",
        "Type '{ a: string; b: number; }' is not assignable",
        "Type '{ a: number; }' is not assignable",
    ] {
        assert!(
            ts2322.iter().any(|(_, message)| message.contains(expected)),
            "expected TS2322 containing {expected:?}, got {diagnostics:#?}"
        );
    }

    let literal_mismatch_count = ts2322
        .iter()
        .filter(|(_, message)| message.contains("Type '2' is not assignable to type '1'"))
        .count();
    assert_eq!(
        literal_mismatch_count, 2,
        "expected both generic-rest literal mismatches, got {diagnostics:#?}"
    );

    assert!(
        !ts2322
            .iter()
            .any(|(_, message)| message.contains("Type '{ c: boolean; }'")),
        "`{{ c: true }}` source display must not be widened, got {diagnostics:#?}"
    );
}

#[test]
fn nested_object_literal_normalization_preserves_deep_optional_properties() {
    let source = r#"
let nested = [
    { kind: "cartesian", box: { pos: { x: 0, y: 0 } } },
    { kind: "named", box: { pos: !true ? { name: "origin" } : { active: true } } },
][0];

nested.box.pos.x;
nested.box.pos.y;
nested.box.pos.name;
nested.box.pos.active;

let pos = nested.box.pos;
pos = { active: false };
pos = { x: 1 };
"#;

    let diagnostics = relevant_diagnostics(source);
    assert!(
        diagnostics.iter().all(|(code, message)| {
            *code != 2339
                || !(message.contains("'x'")
                    || message.contains("'y'")
                    || message.contains("'name'")
                    || message.contains("'active'"))
        }),
        "nested normalized properties should be readable as optional members, got {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '{ x: number; }' is not assignable")
        }),
        "nested normalized assignment should still reject incomplete objects, got {diagnostics:#?}"
    );
}
