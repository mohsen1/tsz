//! TS2322 source-side display preserves the `readonly` modifier on tuple
//! sources whose readonliness is itself the relevant property of the
//! assignment.
//!
//! tsc renders `Type 'readonly [1]' is not assignable to type 'readonly []'`
//! when both sides are readonly tuples; without the source-side `readonly`
//! prefix the message reads `Type '[1]' is not assignable to type
//! 'readonly []'`, which leaks the source's readonliness from the
//! diagnostic the user is trying to read.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Direct case from `readonlyTupleAndArrayElaboration`.
#[test]
fn readonly_tuple_source_shows_readonly_prefix_in_ts2322() {
    let source = r#"
const t1: readonly [1] = [1];
const t2: readonly [] = t1;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for `t1` to `readonly []` assignment, got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'readonly [1]'"),
        "Source side must render with `readonly` prefix; got: {msg}"
    );
    assert!(
        !msg.contains("Type '[1]'"),
        "Source side must NOT drop the `readonly` modifier; got: {msg}"
    );
}

/// Anti-regression: a NON-readonly tuple source must NOT pick up a
/// spurious `readonly` prefix.
#[test]
fn mutable_tuple_source_does_not_gain_readonly_prefix() {
    let source = r#"
const t5: [1] = [1];
const t6: readonly [] = t5;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for mutable `t5` to `readonly []`, got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'[1]'"),
        "Mutable tuple source must render without `readonly` prefix; got: {msg}"
    );
    assert!(
        !msg.contains("'readonly [1]'"),
        "Source side must NOT add `readonly` prefix to a mutable tuple; got: {msg}"
    );
}

/// Sibling case with multiple elements — the `readonly` prefix applies
/// to the whole tuple, not per-element.
#[test]
fn readonly_two_element_tuple_source_renders_with_single_readonly_prefix() {
    let source = r#"
const t1: readonly [1, 2] = [1, 2];
const t2: readonly [number] = t1;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for narrowing `readonly [1, 2]` to `readonly [number]`, got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'readonly [1, 2]'"),
        "Source side must render `readonly [1, 2]` exactly once; got: {msg}"
    );
}
