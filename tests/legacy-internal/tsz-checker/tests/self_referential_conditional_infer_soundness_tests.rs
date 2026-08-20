//! Regression coverage for issue #14784 — a self-referential generic-method
//! conditional return whose `extends` clause carries an `infer` variable must
//! not silence `TS2322` when the call result is assigned to a *different*
//! instantiation of the same constructor.
//!
//! Structural rule: an `infer V` declared inside a *generic* signature's body
//! (e.g. the conditional return `m<U>(): U extends … ? Box<T> : Box<T>`) is a
//! definitional binder scoped to that signature/conditional, not a live
//! transient inference placeholder. So an object type that merely *contains*
//! such a method (a self-referential class `Box<T>` whose conditional branch
//! re-includes `m`) must NOT be treated as carrying a free `infer`, which would
//! wrongly route `should_suppress_assignability_diagnostic` into suppressing a
//! real `TS2322`.
//!
//! The trigger requires the *intersection* of (a) an `infer` in the `extends`
//! clause and (b) a self-referential class branch; with either removed tsz
//! already reported correctly, so the controls below pin the boundary. The
//! tuple `[infer V]` extends form keeps these lib-free (`check_source_strict`
//! does not load lib).

use crate::test_utils::{check_source_strict, diagnostic_count};

fn ts2322_count(source: &str) -> usize {
    diagnostic_count(&check_source_strict(source), 2322)
}

/// Reported repro (method form): assigning the `Box<number>` call result to
/// `Box<string>` and to `string` must both error.
#[test]
fn self_ref_conditional_infer_method_assignment_errors() {
    let source = r#"
class Box<T> {
  m<U>(f: (t: T) => U): U extends [infer V] ? Box<T> : Box<T> { return null as any; }
}
declare const b: Box<number>;
const r = b.m(x => x);
const bad1: Box<string> = r;
const bad2: string = r;
"#;
    assert_eq!(
        ts2322_count(source),
        2,
        "self-ref conditional with infer must still report both TS2322s"
    );
}

/// Same false-negative when `m` is an arrow-function class property rather than
/// a method — the binder shape differs but the `infer` is still signature-bound.
#[test]
fn self_ref_conditional_infer_arrow_property_assignment_errors() {
    let source = r#"
class Box<T> {
  m: <U>(f: (t: T) => U) => U extends [infer V] ? Box<T> : Box<T> = null as any;
}
declare const b: Box<number>;
const r = b.m(x => x);
const bad1: Box<string> = r;
const bad2: string = r;
"#;
    assert_eq!(
        ts2322_count(source),
        2,
        "arrow-property form must report both TS2322s, like the method form"
    );
}

/// Renamed binders (class param `K`, method param `W`, infer `Q`, value `boxed`)
/// — the rule is structural, not tied to any identifier.
#[test]
fn self_ref_conditional_infer_renamed_binders_errors() {
    let source = r#"
class Container<K> {
  pick<W>(g: (k: K) => W): W extends [infer Q] ? Container<K> : Container<K> { return null as any; }
}
declare const c: Container<number>;
const boxed = c.pick(k => k);
const wrong: Container<string> = boxed;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "renamed binders must not change the structural outcome"
    );
}

/// Control: with NO `infer` in the `extends` clause, tsz already reported — the
/// fix must not change that.
#[test]
fn self_ref_conditional_without_infer_still_errors() {
    let source = r#"
class Box<T> {
  m<U>(f: (t: T) => U): U extends [number] ? Box<T> : Box<T> { return null as any; }
}
declare const b: Box<number>;
const r = b.m(x => x);
const bad1: Box<string> = r;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "no-infer control must still report TS2322"
    );
}

/// Control: a non-self-referential sibling-class branch (`Other<T>`) with the
/// same `infer` — tsz already reported, and must keep doing so.
#[test]
fn sibling_class_branch_with_infer_still_errors() {
    let source = r#"
class Other<T> { o!: T; }
class Box<T> {
  m<U>(f: (t: T) => U): U extends [infer V] ? Other<T> : Other<T> { return null as any; }
}
declare const b: Box<number>;
const r = b.m(x => x);
const bad1: Other<string> = r;
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "sibling-class branch with infer must still report TS2322"
    );
}

/// Guard: a genuinely matching same-instantiation assignment must NOT error —
/// the fix removes a false negative, it must not introduce a false positive.
#[test]
fn matching_same_instantiation_assignment_is_accepted() {
    let source = r#"
class Box<T> {
  m<U>(f: (t: T) => U): U extends [infer V] ? Box<T> : Box<T> { return null as any; }
}
declare const b: Box<number>;
const r = b.m(x => x);
const ok: Box<number> = r;
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "assigning to the same instantiation must remain accepted"
    );
}
