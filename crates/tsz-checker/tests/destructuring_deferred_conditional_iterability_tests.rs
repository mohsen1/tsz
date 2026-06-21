//! Regression: a rest parameter with an array binding pattern typed by a
//! *deferred* conditional (its `extends` operand is a free type parameter, both
//! branches tuples) must not emit TS2488 — tsc defers iterability to
//! instantiation. The destructuring iterability check lacked the deferred-generic
//! guard that `check_spread_argument_iterability` already has.
//!
//! Owner: `crates/tsz-checker/src/checkers/iterable_checker.rs`.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn rest_binding_deferred_conditional_no_ts2488() {
    let codes = codes(
        r#"
export class C<T extends object> {
  constructor(...[opts]: {} extends T ? [a?: string] : [a: string]) {
    void opts;
  }
}
"#,
    );
    assert!(
        !codes.contains(&2488),
        "a deferred-conditional rest binding pattern must not emit TS2488; got {codes:?}"
    );
}

#[test]
fn rest_binding_deferred_conditional_function_no_ts2488() {
    // Same defect outside a constructor, with a binder-name variation.
    let codes = codes(
        r#"
declare function make<Value extends object>(
  ...[config]: {} extends Value ? [c?: number] : [c: number]
): void;
"#,
    );
    assert!(
        !codes.contains(&2488),
        "a deferred-conditional rest binding pattern in a function must not emit TS2488; got {codes:?}"
    );
}

#[test]
fn genuinely_non_iterable_destructuring_still_ts2488() {
    // Negative control: a concrete non-iterable type still emits TS2488.
    let codes = codes(
        r#"
declare const o: object;
const [a] = o;
void a;
"#,
    );
    assert!(
        codes.contains(&2488),
        "destructuring a non-iterable concrete type still emits TS2488; got {codes:?}"
    );
}
