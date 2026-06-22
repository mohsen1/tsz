//! Regression tests for issue #14530.
//!
//! tsc infers an unannotated function's return type as
//! `getWidenedType(getUnionType(unwidenedReturnTypes))`: the *unwidened* return
//! contributions are unioned first, and a fresh primitive literal widens **only**
//! when the union reduces to exactly that single literal. Two distinct literals
//! (or a literal alongside the implicit `undefined` of a fall-through) therefore
//! stay a precise union rather than collapsing to the base primitive.
//!
//! tsz previously widened each branch's literal *before* the union, so a function
//! returning two distinct literals inferred the base primitive (`string` /
//! `number`) and a literal-typed assignment of its result drew a spurious
//! TS2322. These tests pin the corrected behavior; binder names are varied so the
//! rule is structural, not name-keyed.

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn ts2322(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

#[test]
fn distinct_string_literals_preserve_union() {
    // The reported witness.
    let source = r#"
function classify(n: number) {
  if (n > 0) return "positive";
  return "zero";
}
const c: "positive" | "zero" = classify(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "distinct string-literal returns must infer the literal union, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn distinct_numeric_literals_preserve_union() {
    let source = r#"
function step(input: number) {
  if (input > 0) return 1;
  return 2;
}
const v: 1 | 2 = step(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "distinct numeric-literal returns must infer `1 | 2`, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn literal_plus_fallthrough_undefined_preserved() {
    let source = r#"
function maybe(flag: number) {
  if (flag > 0) return "a";
}
const m: "a" | undefined = maybe(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "literal + implicit undefined must infer `\"a\" | undefined`, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn single_literal_return_still_widens() {
    // A lone fresh literal still widens to its base primitive.
    let source = r#"
function only() {
  return "x";
}
const wide: string = only();
"#;
    assert!(
        ts2322(source).is_empty(),
        "a single literal return must widen to `string`, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn single_literal_return_not_assignable_to_narrower_literal() {
    // The negative control for the widening above: the widened `string` must NOT
    // be assignable to a narrower literal type.
    let source = r#"
function only() {
  return "x";
}
const narrow: "x" = only();
"#;
    assert!(
        !ts2322(source).is_empty(),
        "widened `string` return must not be assignable to `\"x\"` (expected TS2322)"
    );
}

#[test]
fn repeated_identical_literal_widens() {
    // Two *identical* literals dedup to a single fresh literal, which still widens.
    let source = r#"
function same(flag: number) {
  if (flag > 0) return "a";
  return "a";
}
const widened: string = same(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "two identical literal returns must widen to `string`, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn const_assertion_return_is_pinned_not_widened() {
    let source = r#"
function pinned() {
  return "a" as const;
}
const exact: "a" = pinned();
"#;
    assert!(
        ts2322(source).is_empty(),
        "a `return x as const` must keep the literal type, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn mixed_literal_and_primitive_widens() {
    // A literal unioned with a base primitive reduces to the primitive.
    let source = r#"
function mixed(text: string, n: number) {
  if (n > 0) return "a";
  return text;
}
const out: string = mixed("", 0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "literal + `string` must infer `string`, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn distinct_literal_union_not_assignable_to_single_member() {
    // Negative control: the preserved union is genuinely the inferred type, so it
    // is not assignable to a single one of its members.
    let source = r#"
function classify(n: number) {
  if (n > 0) return "positive";
  return "zero";
}
const bad: "positive" = classify(0);
"#;
    assert!(
        !ts2322(source).is_empty(),
        "`\"positive\" | \"zero\"` must not be assignable to `\"positive\"` (expected TS2322)"
    );
}

#[test]
fn renamed_binders_distinct_literals_preserve_union() {
    // Same structural rule, different names — guards against name-keyed shortcuts.
    let source = r#"
function pick(amount: number) {
  if (amount > 0) return "lo";
  if (amount < 0) return "hi";
  return "mid";
}
const choice: "lo" | "hi" | "mid" = pick(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "three distinct literals must infer their union, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn boolean_literals_collapse_to_boolean() {
    let source = r#"
function toggle(n: number) {
  if (n > 0) return true;
  return false;
}
const b: boolean = toggle(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "`true | false` must reduce to `boolean`, got: {:?}",
        ts2322(source)
    );
}

#[test]
fn switch_distinct_literals_preserve_union() {
    let source = r#"
function route(kind: number) {
  switch (kind) {
    case 0: return "start";
    case 1: return "stop";
    default: return "idle";
  }
}
const state: "start" | "stop" | "idle" = route(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "switch-arm distinct literals must infer their union, got: {:?}",
        ts2322(source)
    );
}
