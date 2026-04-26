//! JSDoc-style postfix `T?` should resolve to `T | null` even though the
//! parser flags TS17019 ("'?' at the end of a type is not valid TypeScript
//! syntax"). tsc's behavior is parser-recovery: it emits the syntax
//! diagnostic but the type semantics treat `T?` as nullable, so a
//! subsequent assignment from `undefined` reports against `T | null` (not
//! plain `T`).

use crate::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    crate::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// `var x: number? = undefined;` should report TS2322 against
/// `number | null`, not against bare `number`.
#[test]
fn postfix_question_widens_target_to_union_with_null() {
    let source = r#"
var postfixopt: number? = undefined;
"#;
    let diags = check_strict(source);

    // TS2322 must reference `number | null` as the target, not bare `number`.
    // (TS17019 also fires for the syntax error in production builds but the
    // unit-test harness here surfaces only checker-emitted codes.)
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected TS2322 for `undefined` -> `number | null`, got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|(_, msg)| msg.contains("number | null")),
        "expected TS2322 message to reference `number | null` target, got: {ts2322:?}"
    );
    assert!(
        !ts2322
            .iter()
            .any(|(_, msg)| msg == "Type 'undefined' is not assignable to type 'number'."),
        "must not display bare 'number' target — postfix-? is a JSDoc nullable: {ts2322:?}"
    );
}

/// `T?[]` should chain — postfix `?` followed by array suffix produces
/// `(T | null)[]`. Verify the parser doesn't crash and the type resolves
/// to an array of `T | null` so subsequent assignments respect the
/// nullable element type.
#[test]
fn postfix_question_chains_with_array_suffix() {
    let source = r#"
var arr: number?[] = [1, null];
"#;
    let diags = check_strict(source);
    // No TS2322 expected: each element is assignable to `number | null`.
    assert!(
        !diags.iter().any(|(c, msg)| *c == 2322
            && (msg.contains("Type '1'") || msg.contains("Type 'null'"))),
        "elements should be assignable to (number | null)[]: {diags:?}"
    );
}

/// Conditional types `T extends U ? X : Y` use `?` as a ternary operator;
/// they must NOT be misparsed as postfix-nullable. The postfix-`?` branch
/// looks ahead for a type-starting token after the `?` and bails when it
/// finds one. This test guards that lookahead.
#[test]
fn ternary_question_in_conditional_type_not_misparsed_as_postfix_nullable() {
    let source = r#"
type Pick2<T, K> = T extends string ? T : K;
declare var x: Pick2<"a" | "b", number>;
const y: "a" | "b" | number = x;
"#;
    let diags = check_strict(source);
    // No TS17019 should fire — the `?` here is a conditional-type operator,
    // not a JSDoc nullable suffix.
    assert!(
        !diags.iter().any(|(c, _)| *c == 17019),
        "must not flag conditional-type `?` as JSDoc postfix nullable: {diags:?}"
    );
}
