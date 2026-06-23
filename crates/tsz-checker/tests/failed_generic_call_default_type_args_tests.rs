//! When a generic call (function or constructor) fails the argument-count
//! check (TS2554/TS2555), tsc still produces a best-effort result type by
//! substituting each of the signature's own type parameters with its
//! `default → constraint → unknown` fallback (`getInferredTypes` →
//! `getDefaultTypeArgumentType`). tsz previously left the bare type parameter
//! (`T`) in the recovered result, leaking it into the use-site value type and
//! drawing a spurious `TS2322`/`TS2339` that tsc never reports.
//!
//! These tests pin the parity. Binder names vary across cases so the result is
//! structural, not keyed to a particular identifier.

use tsz_checker::test_utils::{check_source_code_messages, check_source_codes};

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

/// `new Box()` where `Box<T = string>` requires a constructor argument: TS2554
/// fires, but `b.value` is `string` (the default), so the `string` assignment
/// is clean — no spurious TS2322.
#[test]
fn constructor_single_default_no_spurious_ts2322() {
    let c = codes(
        r#"
class Box<T = string> { constructor(public value: T) {} }
const b = new Box();
const v: string = b.value;
"#,
    );
    assert!(c.contains(&2554), "expected TS2554, got {c:?}");
    assert!(!c.contains(&2322), "no spurious TS2322 expected, got {c:?}");
}

/// Multiple type parameters, each with a default, all resolve.
#[test]
fn constructor_multiple_defaults_no_spurious_ts2322() {
    let c = codes(
        r#"
class Pair<A = string, B = number> { constructor(public a: A, public b: B) {} }
const p = new Pair();
const a: string = p.a;
const b: number = p.b;
"#,
    );
    assert!(c.contains(&2554), "expected TS2554, got {c:?}");
    assert!(!c.contains(&2322), "no spurious TS2322 expected, got {c:?}");
}

/// A default that references an earlier type parameter (`Two = One[]`) must
/// resolve through it (`One = string` ⇒ `Two = string[]`).
#[test]
fn constructor_default_referencing_earlier_param_resolves() {
    let c = codes(
        r#"
class Holder<One = string, Two = One[]> { constructor(public head: One, public tail: Two) {} }
const h = new Holder();
const head: string = h.head;
const tail: string[] = h.tail;
"#,
    );
    assert!(c.contains(&2554), "expected TS2554, got {c:?}");
    assert!(!c.contains(&2322), "no spurious TS2322 expected, got {c:?}");
}

/// A constrained default (`Elem extends object = { x: number }`) resolves to the
/// default object type, so `.x` exists — no spurious TS2339.
#[test]
fn constructor_constrained_default_no_spurious_ts2339() {
    let c = codes(
        r#"
class Wrap<Elem extends object = { x: number }> { constructor(public value: Elem) {} }
const w = new Wrap();
const n: number = w.value.x;
"#,
    );
    assert!(c.contains(&2554), "expected TS2554, got {c:?}");
    assert!(!c.contains(&2339), "no spurious TS2339 expected, got {c:?}");
}

/// The same bug surfaces for a generic *function* call, not only `new`.
#[test]
fn generic_function_call_default_no_spurious_ts2322() {
    let c = codes(
        r#"
declare function build<Out = string>(seed: Out): { value: Out };
const made = build();
const v: string = made.value;
"#,
    );
    assert!(c.contains(&2554), "expected TS2554, got {c:?}");
    assert!(!c.contains(&2322), "no spurious TS2322 expected, got {c:?}");
}

/// No default and no constraint: tsc renders the un-inferred parameter as
/// `unknown` in the residual TS2322 (not the bare parameter name).
#[test]
fn no_default_renders_unknown_not_bare_param() {
    let msgs = check_source_code_messages(
        r#"
class Cell<Item> { constructor(public value: Item) {} }
const c = new Cell();
const v: string = c.value;
"#,
    );
    let ts2322: Vec<&String> = msgs
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, m)| m)
        .collect();
    assert!(
        ts2322.iter().any(|m| m.contains("unknown")),
        "expected a TS2322 mentioning 'unknown', got {ts2322:?}"
    );
    assert!(
        !ts2322.iter().any(|m| m.contains("'Item'")),
        "TS2322 must not leak the bare type parameter name, got {ts2322:?}"
    );
}

/// A constraint but no default: the un-inferred parameter renders as its
/// constraint (`number`), not the bare parameter name.
#[test]
fn constrained_no_default_renders_constraint_not_bare_param() {
    let msgs = check_source_code_messages(
        r#"
class Cell<Item extends number> { constructor(public value: Item) {} }
const c = new Cell();
const v: string = c.value;
"#,
    );
    let ts2322: Vec<&String> = msgs
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, m)| m)
        .collect();
    assert!(
        ts2322.iter().any(|m| m.contains("number")),
        "expected a TS2322 mentioning the constraint 'number', got {ts2322:?}"
    );
    assert!(
        !ts2322.iter().any(|m| m.contains("'Item'")),
        "TS2322 must not leak the bare type parameter name, got {ts2322:?}"
    );
}

/// Negative control: a *successful* `new` with a default still types the
/// instance through the default — no arity error, no spurious assignment error.
#[test]
fn successful_new_with_default_is_clean() {
    let c = codes(
        r#"
class Box<T = string> { value!: T; }
const b = new Box();
const v: string = b.value;
"#,
    );
    assert!(
        !c.contains(&2554),
        "no TS2554 expected for a valid new, got {c:?}"
    );
    assert!(!c.contains(&2322), "no TS2322 expected, got {c:?}");
}

/// Negative control: a genuinely wrong assignment off the defaulted instance
/// still errors (the fix must not blanket-suppress real mismatches).
#[test]
fn defaulted_instance_still_reports_real_mismatch() {
    let c = codes(
        r#"
class Box<T = string> { constructor(public value: T) {} }
const b = new Box();
const v: number = b.value;
"#,
    );
    assert!(c.contains(&2554), "expected TS2554, got {c:?}");
    assert!(
        c.contains(&2322),
        "string default is not assignable to number — TS2322 expected, got {c:?}"
    );
}
