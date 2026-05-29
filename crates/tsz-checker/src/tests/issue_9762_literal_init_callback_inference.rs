//! Regression coverage for issue #9762.
//!
//! Structural rule: when a generic type parameter is inferred from a direct
//! non-fresh literal initializer and the same parameter appears in a callback
//! return position, the direct initializer candidate widens before the
//! callback target is fixed. The callback should be checked against the
//! widened primitive, not the literal initializer.

use crate::test_utils::{check_source_diagnostics, diagnostic_count};

fn diagnostics(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source_diagnostics(source)
}

fn diagnostic_count_for(source: &str, code: u32) -> usize {
    diagnostic_count(&diagnostics(source), code)
}

#[test]
fn literal_init_widens_before_callback_return_check() {
    let source = r#"
declare function reduce<Item, Acc>(
  arr: Item[],
  fn: (acc: Acc, item: Item) => Acc,
  init: Acc
): Acc;
reduce(["a"], (acc, item) => { acc.toFixed(); return acc; }, 0);
"#;

    assert_eq!(
        diagnostic_count_for(source, 2345),
        0,
        "direct literal init should widen to number before checking callback return"
    );
}

#[test]
fn literal_init_widened_accumulator_still_checks_callback_return() {
    let source = r#"
declare function reduce<Element, Accumulator>(
  arr: Element[],
  fn: (acc: Accumulator, item: Element) => Accumulator,
  init: Accumulator
): Accumulator;
reduce(["a", "b"], (total, item) => total + item, 0);
"#;

    assert_eq!(
        diagnostic_count_for(source, 2322),
        1,
        "callback returning string should be checked against widened number accumulator"
    );
}

#[test]
fn string_literal_init_widens_before_callback_return_check() {
    let source = r#"
declare function fold<Item, State>(
  arr: Item[],
  fn: (state: State, item: Item) => State,
  init: State
): State;
fold([1], (state, item) => { state.toUpperCase(); return state; }, "");
"#;

    assert_eq!(
        diagnostic_count_for(source, 2345),
        0,
        "direct string literal init should widen to string before checking callback return"
    );
}

#[test]
fn direct_literal_inference_still_widens_common_primitive() {
    let source = r#"
declare function choose<Choice>(left: Choice, right: Choice): Choice;
let value = choose(0, 5);
value = 99;
"#;

    assert_eq!(
        diagnostic_count_for(source, 2322),
        0,
        "plain direct literal inference should keep widening compatible numeric literals"
    );
}
