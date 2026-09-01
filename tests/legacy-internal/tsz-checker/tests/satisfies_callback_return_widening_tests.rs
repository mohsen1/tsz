//! Regression coverage for return-type widening of `satisfies`-typed callbacks.
//!
//! Structural rule: when a function-body return is a fresh literal and the
//! contextual return type does not pin that literal (`isLiteralOfContextualType`
//! is false — e.g. `unknown`, `any`, a base primitive, or an object/function
//! type), tsc widens the literal to its base; tsz must too. A contextual return
//! type only suppresses widening when it actually pins the literal (a literal or
//! literal-containing union/type-parameter context).
//!
//! The `Ret<T>` helper extracts a function's return type via `infer` so these
//! tests need no lib (`ReturnType` is a lib type and `check_source_diagnostics`
//! does not load lib).

use crate::test_utils::{check_source_diagnostics, diagnostic_count};

fn ts2322_count(source: &str) -> usize {
    diagnostic_count(&check_source_diagnostics(source), 2322)
}

/// Reported repro: `(() => 1) satisfies () => unknown` widens the body return
/// to `number`, so assigning `99` to the extracted return type is allowed.
#[test]
fn satisfies_unknown_return_widens_number_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const f = (() => 1) satisfies () => unknown;
const p: Ret<typeof f> = 99;
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "return literal should widen to number under non-pinning `() => unknown`"
    );
}

/// Same rule for a string-literal body, proving the fix is not number-specific.
#[test]
fn satisfies_unknown_return_widens_string_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const g = (() => "x") satisfies () => unknown;
const p: Ret<typeof g> = "other";
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "return literal should widen to string under non-pinning `() => unknown`"
    );
}

/// Object-of-handlers form (still plain `satisfies`, inline object type).
#[test]
fn satisfies_object_handler_return_widens_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const handlers = {
  click: () => 1,
} satisfies { click: () => unknown };
const p: Ret<typeof handlers.click> = 99;
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "handler return literal should widen to number under non-pinning context"
    );
}

/// A non-literal but non-pinning base-primitive contextual return also widens
/// (`() => number` does not pin the literal `1`).
#[test]
fn satisfies_primitive_return_widens_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const f = (() => 1) satisfies () => number;
const p: Ret<typeof f> = 99;
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "return literal should widen to number under non-pinning `() => number`"
    );
}

/// Boundary: a *literal* contextual return pins the literal, so widening is
/// suppressed and the extracted return type stays `1`; assigning `99` errors.
/// This must not regress when the non-pinning case is fixed.
#[test]
fn satisfies_literal_return_preserves_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const h = (() => 1) satisfies () => 1;
const p: Ret<typeof h> = 99;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "literal contextual return `() => 1` must keep the body return as `1` (TS2322 on `99`)"
    );
}

/// Boundary: a literal-containing union contextual return pins the literal too.
#[test]
fn satisfies_literal_union_return_preserves_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const h = (() => 1) satisfies () => 1 | 2;
const p: Ret<typeof h> = 99;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "literal-union contextual return `() => 1 | 2` must keep the body return as `1`"
    );
}

/// Control: with no `satisfies`, the bare arrow already widens its body return
/// to `number`. Locks that the fix matches the no-context baseline.
#[test]
fn no_satisfies_control_widens_literal() {
    let source = r#"
type Ret<T> = T extends () => infer R ? R : never;
const c = () => 1;
const p: Ret<typeof c> = 99;
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "bare arrow body return should widen to number (control)"
    );
}
