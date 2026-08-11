//! Locks tsc-parity for the missing-property head promotion (TS2739 / TS2740 /
//! TS2741) when a value is passed as a CALL ARGUMENT to a parameter whose type
//! is a concrete object/interface that declares BOTH an index signature AND a
//! method with its own (bound) generic type parameter — e.g.
//!
//! ```ts
//! interface Big { m<S>(x: S): S; readonly [n: number]: string }
//! function f(x: Big) {}
//! f({}); // tsc: TS2741 "Property 'm' is missing ... but required in type 'Big'"
//! ```
//!
//! Regression for issue #17145. The call-argument mismatch renderer decides
//! whether to take the "preserve the parameter type's verbatim display" path
//! (which skips missing-property elaboration and emits a bare TS2345) by asking
//! whether the parameter type contains a type parameter. It must ask for a
//! *free* type parameter: a method's own `<S>` is BOUND by that method's
//! signature, so a concrete target like `Big` (or the real shape of
//! `Array<T>` / `ReadonlyArray<T>`, whose `every<S extends T>` etc. carry bound
//! method type parameters) is fully resolved for the argument and must promote
//! a sole missing property to TS2741 — exactly as the direct-assignment path
//! already does.
//!
//! The binder names are varied deliberately (`Big`, `Enormous`, `StrIdx`) so no
//! fix can key on a specific identifier.

use tsz_checker::test_utils::check_source_code_messages as check;
use tsz_common::diagnostics::diagnostic_codes;

const TS2345: u32 = diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
const TS2739: u32 = diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE;
const TS2740: u32 = diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE;
const TS2741: u32 = diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE;

fn assert_has(code: u32, source: &str) {
    let diags = check(source);
    assert!(
        diags.iter().any(|(c, _)| *c == code),
        "Expected TS{code}. Got: {diags:?}"
    );
}

fn assert_none(code: u32, source: &str) {
    let diags = check(source);
    assert!(
        diags.iter().all(|(c, _)| *c != code),
        "Expected no TS{code}. Got: {diags:?}"
    );
}

// The core repro: number index signature + generic method, passed as a call
// argument. Before the fix this fell back to a bare TS2345 with no elaboration;
// now it promotes to exactly one TS2741 and emits no bare TS2345 head.
#[test]
fn number_index_plus_generic_method_call_arg_promotes_to_ts2741() {
    let diags = check(
        r#"
interface Big {
    m<S>(x: S): S;
    readonly [n: number]: string;
}
function f(x: Big) {}
f({});
"#,
    );
    assert!(
        diags.iter().all(|(c, _)| *c != TS2345),
        "call argument should promote to TS2741, not fall back to bare TS2345. Got: {diags:?}"
    );
    assert_eq!(
        diags.iter().filter(|(c, _)| *c == TS2741).count(),
        1,
        "exactly one TS2741 expected. Got: {diags:?}"
    );
}

// Renamed binders: the fix must not depend on any identifier.
#[test]
fn renamed_binders_still_promote() {
    assert_has(
        TS2741,
        r#"
interface Enormous {
    handle<Q>(v: Q): Q;
    readonly [k: number]: string;
}
function g(y: Enormous) {}
g({});
"#,
    );
}

// String index signature variant (either index-signature kind triggers it).
#[test]
fn string_index_plus_generic_method_call_arg_promotes() {
    let src = r#"
interface StrIdx {
    pick<T>(k: T): T;
    [s: string]: unknown;
}
function h(z: StrIdx) {}
h({});
"#;
    assert_has(TS2741, src);
    assert_none(TS2345, src);
}

// The real-world shape that surfaced this: an interface extending a generic
// base that itself carries bound method type parameters. Passing an
// incompatible value must promote to the missing-property family (TS2739 /
// TS2740 / TS2741), never fall back to a bare TS2345. (The unit harness does
// not load `lib.es5`, so `Array<T>` resolves to only the interface's own
// `clear` here; the full lib shape's many-missing TS2740 head is exercised by
// the conformance corpus — `templateStringsArrayType*`, `noParameterReassignmentJSIIFE`.)
#[test]
fn interface_extends_generic_base_promotes_missing_property() {
    let src = r#"
interface Base<T> {
    every<S extends T>(pred: (v: T) => v is S): boolean;
    readonly [n: number]: T;
}
interface ObsArr<T> extends Base<T> {
    clear(): T[];
}
function k(a: ObsArr<number>) {}
k({});
"#;
    let diags = check(src);
    assert!(
        diags.iter().all(|(c, _)| *c != TS2345),
        "extends-generic-base argument must promote, not fall back to bare TS2345. Got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(c, _)| *c == TS2739 || *c == TS2740 || *c == TS2741),
        "expected a missing-property head (TS2739/2740/2741). Got: {diags:?}"
    );
}

// The direct-assignment path already promoted correctly; guard it so a future
// change cannot regress the reference behavior the call path now matches.
#[test]
fn direct_assignment_still_promotes_to_ts2741() {
    assert_has(
        TS2741,
        r#"
interface Big {
    m<S>(x: S): S;
    readonly [n: number]: string;
}
const b: Big = {};
"#,
    );
}

// Negative: a value that genuinely satisfies the parameter type must NOT error.
#[test]
fn satisfying_argument_is_accepted() {
    assert_none(
        TS2741,
        r#"
interface Big {
    m<S>(x: S): S;
    readonly [n: number]: string;
}
function f(x: Big) {}
declare const good: Big;
f(good);
"#,
    );
}

// Negative: a genuinely missing property on a plain (non-generic) target still
// promotes — the fix narrows the preserve-display gate, it does not disable
// promotion.
#[test]
fn plain_target_missing_property_still_promotes() {
    assert_has(
        TS2741,
        r#"
interface Plain { a: number; }
function p(q: Plain) {}
p({});
"#,
    );
}
