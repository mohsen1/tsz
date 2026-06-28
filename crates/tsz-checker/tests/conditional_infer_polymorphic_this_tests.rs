//! Tests for `infer` extraction from a member whose declared type is the
//! polymorphic `this` type (issue #14785).
//!
//! The structural rule:
//!   When a conditional `T extends { m(): infer S } ? S : F` is instantiated
//!   with a receiver `R` whose `m()` returns `this`, tsc rebinds the
//!   polymorphic `this` to the matched receiver (`getTypeWithThisArgument`)
//!   before collecting the `infer` candidate, so `S = R` and the true branch is
//!   taken. tsz previously left the method return as an unsubstituted
//!   `ThisType`, collected no candidate, and fell to the false branch — a
//!   false-positive when the (correct) value was assigned to the result.
//!
//! The fix binds `this` to the source receiver at every infer-candidate
//! extraction site. Covered here: methods returning `this`, bare `this`-typed
//! properties, and intersection receivers, across class and interface
//! receivers and renamed binders.
//!
//! (A method returning an object type that nests `this`, e.g. `m(): { v: this }`,
//! is not exercised here because such a declaration independently trips a
//! separate, pre-existing TS2526 defect in tsz's `this`-in-member-type
//! validation — orthogonal to this infer-candidate binding.)

use tsz_checker::test_utils::check_source_strict_codes;

/// Returns true when the source has no diagnostics other than TS2318
/// (missing global types, expected in the no-stdlib unit test harness).
fn has_no_errors(source: &str) -> bool {
    errors(source).is_empty()
}

fn errors(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|&code| code != 2318)
        .collect()
}

// ── Primary repro: method returning `this` ────────────────────────────────

#[test]
fn class_method_returning_this_binds_infer_to_receiver() {
    assert!(
        has_no_errors(
            r#"
class Node { self(): this { return this; } }
type GetThis<T> = T extends { self(): infer S } ? S : never;
type GN = GetThis<Node>;
const g: GN = new Node();
"#
        ),
        "GetThis<Node> should infer S = Node (true branch), not never"
    );
}

#[test]
fn interface_method_returning_this_binds_infer_to_receiver() {
    assert!(
        has_no_errors(
            r#"
interface I { build(): this; }
type GetThis<T> = T extends { build(): infer S } ? S : never;
type R = GetThis<I>;
const r: R = {} as I;
"#
        ),
        "GetThis<I> should infer S = I for an interface receiver"
    );
}

/// Renamed binders must behave identically — no name-keyed logic.
#[test]
fn renamed_binders_method_returning_this() {
    assert!(
        has_no_errors(
            r#"
class Widget { clone(): this { return this; } }
type Extract<Recv> = Recv extends { clone(): infer Out } ? Out : never;
type W = Extract<Widget>;
const w: W = new Widget();
"#
        ),
        "renamed type params should infer the receiver identically"
    );
}

// ── False-branch witness: the true branch really is taken ─────────────────

#[test]
fn this_returning_method_takes_true_branch_not_false() {
    // If S were never bound (false branch), S = "FAIL" and assigning a Node
    // would be accepted silently. tsc binds S = Node (true branch), so the
    // assignment of a Node to `"FAIL"` must be REJECTED.
    let codes = errors(
        r#"
class Node { self(): this { return this; } }
type G1<T> = T extends { self(): infer S } ? S : "FAIL";
type R1 = G1<Node>;
const bad: R1 = "FAIL";
"#,
    );
    assert!(
        codes.contains(&2322),
        "true branch (S = Node) must reject assigning the false-branch literal; got {codes:?}"
    );
}

// ── Adjacent: bare `this`-typed property (direct infer prop) ───────────────

#[test]
fn this_typed_property_binds_infer_to_receiver() {
    assert!(
        has_no_errors(
            r#"
class Cursor { node: this = this; }
type PropThis<T> = T extends { node: infer S } ? S : never;
type P = PropThis<Cursor>;
const p: P = new Cursor();
"#
        ),
        "a bare `this`-typed property should bind the infer var to the receiver"
    );
}

// ── Adjacent: intersection receiver ───────────────────────────────────────

#[test]
fn intersection_receiver_method_returning_this() {
    // The receiver is an intersection `Base & Tag`; the `this`-returning method
    // must rebind `this` to the whole intersection so `S` is assignable from a
    // value of that intersection. (The infer-candidate `this`-binding runs on
    // the intersection branch of the object-pattern matcher.)
    assert!(
        has_no_errors(
            r#"
class Base { id(): this { return this; } }
interface Tag { tag: string; }
type GetId<T> = T extends { id(): infer S } ? S : never;
type R = GetId<Base & Tag>;
declare const both: Base & Tag;
const r: R = both;
"#
        ),
        "intersection receiver should rebind `this` to the whole intersection"
    );
}

// ── Control: concrete (non-`this`) return must still be sound ──────────────

#[test]
fn concrete_return_still_infers_concrete_type() {
    let codes = errors(
        r#"
class A { x = 1; }
class Maker { make(): A { return new A(); } }
type GetMake<T> = T extends { make(): infer S } ? S : never;
type M = GetMake<Maker>;
const ok: M = new A();
const bad: M = 123;
"#,
    );
    // The good assignment is fine; the bad one (number to A) must error.
    assert!(
        codes.contains(&2322),
        "concrete return should infer A and reject a number; got {codes:?}"
    );
}
