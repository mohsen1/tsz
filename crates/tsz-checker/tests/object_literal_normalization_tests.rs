use tsz_checker::context::CheckerOptions;

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
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
    .map(|diag| (diag.code, diag.message_text))
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
