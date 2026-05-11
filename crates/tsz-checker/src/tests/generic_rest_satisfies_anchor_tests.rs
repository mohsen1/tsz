//! Regression coverage for generic rest argument diagnostics involving
//! `satisfies` expressions.

use crate::test_utils::check_source_diagnostics;

/// A rest parameter whose type is an outer type parameter must be checked as
/// the whole argument tuple against that type parameter, not just per element
/// against the constraint. This is visible for `satisfies` arguments because
/// `satisfies unknown` blocks the contextual literal from becoming `{ a: true }`.
#[test]
fn generic_rest_type_parameter_arguments_emit_tuple_ts2345_at_argument() {
    let source = r#"
function fn<T extends { a: true }[]>(f: (...args: T) => void) {
  f({ a: true } satisfies unknown);
  const o = { a: true as const };
  f(o satisfies unknown);
  f(o);
}
"#;
    let diags = check_source_diagnostics(source);

    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        3,
        "Expected one TS2345 for each generic rest call, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );

    let expected = [
        (
            "{ a: true } satisfies unknown",
            "Argument of type '[{ a: boolean; }]' is not assignable to parameter of type 'T'.",
        ),
        (
            "o satisfies unknown",
            "Argument of type '[{ a: true; }]' is not assignable to parameter of type 'T'.",
        ),
        (
            "o);",
            "Argument of type '[{ a: true; }]' is not assignable to parameter of type 'T'.",
        ),
    ];

    for (diag, (anchor, message)) in ts2345.iter().zip(expected) {
        assert_eq!(
            diag.start as usize,
            source.find(anchor).expect("anchor text should exist"),
            "TS2345 should anchor at the argument expression for `{anchor}`, got {diag:?}"
        );
        assert_eq!(
            diag.message_text, message,
            "Unexpected TS2345 message for `{anchor}`"
        );
    }
}
