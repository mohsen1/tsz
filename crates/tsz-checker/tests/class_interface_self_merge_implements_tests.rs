//! Regression tests for declaration self-merge in an `implements` clause.
//!
//! When a class and a same-named interface merge into one symbol, the class may
//! legally `implements` that name (`class Foo implements Foo`). The implements
//! target then resolves to the merged symbol whose declarations include this very
//! class, so the class trivially implements its own (reflexive) type. tsc emits
//! no diagnostics for this pattern, even when the class has private/protected
//! members. Previously tsz reported a spurious TS2720 ("incorrectly implements
//! class … Did you mean to extend") because the merged symbol's own class
//! declaration carries private members.
//!
//! See <https://github.com/tsz-org/tsz/issues/14114> (xstate `SimulatedClock`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diag_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.d.ts"]);
    let diagnostics = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn self_merge_with_private_member_no_ts2720() {
    // The minimal witness reduced from xstate `SimulatedClock`.
    let codes = diag_codes(
        r#"
interface Base { a(): void }
interface Foo extends Base { b(): void }
class Foo implements Foo {
    private x: number = 0;
    a() {}
    b() {}
}
"#,
    );
    assert!(
        codes.is_empty(),
        "class/interface self-merge with a private member should be clean, got: {codes:?}"
    );
}

#[test]
fn self_merge_with_protected_member_no_error() {
    let codes = diag_codes(
        r#"
interface Foo { a(): void }
class Foo implements Foo {
    protected y: number = 1;
    a() {}
}
"#,
    );
    assert!(
        codes.is_empty(),
        "class/interface self-merge with a protected member should be clean, got: {codes:?}"
    );
}

#[test]
fn generic_self_merge_identity_with_private_no_error() {
    let codes = diag_codes(
        r#"
interface Box<T> { v: T }
class Box<T> implements Box<T> {
    private secret: number = 0;
    v!: T;
}
"#,
    );
    assert!(
        codes.is_empty(),
        "generic identity self-merge with a private member should be clean, got: {codes:?}"
    );
}

#[test]
fn self_merge_xstate_simulated_clock_shape_no_ts2720() {
    // Mirrors the real witness: a private-heavy class implementing a same-named
    // interface that extends another interface.
    let codes = diag_codes(
        r#"
interface Clock {
    setTimeout(fn: (...args: any[]) => void, timeout: number): any;
    clearTimeout(id: any): void;
}
interface SimulatedClock extends Clock {
    start(speed: number): void;
    increment(ms: number): void;
    set(ms: number): void;
}
class SimulatedClock implements SimulatedClock {
    private timeouts: Record<number, any> = {};
    private _now: number = 0;
    private getId(): number { return 0; }
    setTimeout(fn: (...args: any[]) => void, timeout: number): any { return 0; }
    clearTimeout(id: any): void {}
    start(speed: number): void {}
    increment(ms: number): void {}
    set(ms: number): void {}
}
"#,
    );
    assert!(
        !codes.contains(&2720),
        "self-merge SimulatedClock shape must not report TS2720, got: {codes:?}"
    );
    assert!(
        codes.is_empty(),
        "self-merge SimulatedClock shape should be clean, got: {codes:?}"
    );
}

#[test]
fn implements_distinct_class_with_private_still_ts2720() {
    // Regression guard: implementing a *different* class that has private members
    // remains a TS2720 — the fix must not silence the genuine diagnostic.
    let codes = diag_codes(
        r#"
class A { private x: number = 0; m(): void {} }
class B implements A { m(): void {} }
"#,
    );
    assert!(
        codes.contains(&2720),
        "implementing a distinct class with private members must still be TS2720, got: {codes:?}"
    );
}

#[test]
fn generic_self_merge_nonidentity_still_reports_member_mismatch() {
    // A non-identity generic self-reference is not reflexive; the public member
    // value check still fires (TS2416), matching tsc.
    let codes = diag_codes(
        r#"
interface Box<T> { v: T }
class Box<T> implements Box<string> {
    v!: T;
}
"#,
    );
    assert!(
        codes.contains(&2416),
        "non-identity generic self-merge must still report the member mismatch, got: {codes:?}"
    );
}
