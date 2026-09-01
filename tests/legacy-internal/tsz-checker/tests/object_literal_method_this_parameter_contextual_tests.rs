//! Contextual typing of object-literal methods with an explicit `this`
//! parameter.
//!
//! Structural rule: when an object-literal method declares a `this`
//! parameter, `tsc` maps the contextual signature's parameter types
//! positionally over the non-`this` parameters and types the `this`
//! parameter from the contextual signature's `this` type. tsz does the
//! same through `is_this_parameter_name` (an AST-owned check); the
//! `this` detection must not depend on atom identity across interner
//! namespaces (issue #13056: an arena `AstAtom` compared against a
//! solver-interned atom never matched, shifting every contextual
//! parameter type by one).

use crate::test_utils::check_source_strict;

fn strict_codes(source: &str) -> Vec<u32> {
    check_source_strict(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn method_this_parameter_keeps_positional_contextual_parameter_types() {
    let codes = strict_codes(
        r#"
const handlers: { onEvent(this: { id: number }, name: string): void } = {
  onEvent(this, name) {
    const n: string = name;
  },
};
"#,
    );
    assert!(
        codes.is_empty(),
        "expected `name` to be contextually `string` after the `this` parameter; got {codes:?}"
    );
}

#[test]
fn method_this_parameter_receives_contextual_this_type() {
    let codes = strict_codes(
        r#"
const counter: { bump(this: { total: number }, by: number): number } = {
  bump(this, by) {
    return this.total + by;
  },
};
"#,
    );
    assert!(
        codes.is_empty(),
        "expected `this` to be contextually `{{ total: number }}`; got {codes:?}"
    );
}

#[test]
fn method_this_parameter_mismatched_use_still_errors() {
    let codes = strict_codes(
        r#"
const widget: { render(this: { depth: number }, label: string): void } = {
  render(this, label) {
    const wrong: number = label;
  },
};
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "expected exactly one TS2322 from assigning the string parameter to a number"
    );
}

#[test]
fn parameter_named_like_this_is_not_a_this_parameter() {
    let codes = strict_codes(
        r#"
const callbacks: { go(thisArg: string, extra: number): void } = {
  go(thisArg, extra) {
    const s: string = thisArg;
    const n: number = extra;
  },
};
"#,
    );
    assert!(
        codes.is_empty(),
        "expected `thisArg` to map to contextual parameter 0; got {codes:?}"
    );
}

#[test]
fn function_expression_this_parameter_keeps_positional_contextual_parameter_types() {
    let codes = strict_codes(
        r#"
const onTick: (this: { id: number }, elapsed: number) => void = function (this, elapsed) {
  const e: number = elapsed;
};
"#,
    );
    assert!(
        codes.is_empty(),
        "expected `elapsed` to be contextually `number` after the `this` parameter; got {codes:?}"
    );
}

#[test]
fn this_parameter_does_not_satisfy_contextual_required_arity() {
    // The contextual signature has zero non-`this` parameters, so the extra
    // required parameter is implicitly any (TS7006) and the method is not
    // assignable (TS2322) — matching tsc 5.9.
    let mut codes = strict_codes(
        r#"
const o: { m(this: { id: number }): void } = {
  m(this, extra) {
    const x = extra;
  },
};
"#,
    );
    codes.sort_unstable();
    assert_eq!(
        codes,
        vec![2322, 7006],
        "expected TS2322 (too few target args) plus TS7006 for the uncovered parameter"
    );
}

#[test]
fn method_this_parameter_with_rest_parameter_maps_contextually() {
    let codes = strict_codes(
        r#"
const sink: { push(this: { size: number }, ...items: string[]): void } = {
  push(this, ...items) {
    const first: string | undefined = items[0];
  },
};
"#,
    );
    assert!(
        codes.is_empty(),
        "expected the rest parameter to be contextually `string[]`; got {codes:?}"
    );
}
