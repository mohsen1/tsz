//! TS2345 callback-parameter head display for generic calls whose type
//! parameters are fixed before inference.
//!
//! Structural rule: the TS2345 head renders the callback parameter with the
//! call's actual final type arguments. A type argument fixed before inference
//! — by the receiver's instantiation (`declare var c3: C3` with concrete type
//! arguments) or by an explicit type-argument list — renders the fixed type; a
//! type argument the call genuinely infers renders the inference result, so a
//! naked later-literal candidate stays a literal (`(a: number) => 1`). `tsc`
//! shows the same type in head and elaboration in every fixed case; tsz does
//! this through the checker's call-error display gateway by never restoring a
//! later argument's literal over a type parameter the call does not infer,
//! and by never re-resolving a declaration annotation when the invoked
//! signature owns no type parameters (a method of a generic class rendered
//! `(a: T) => U` through that path resolves `T`/`U` out of scope to `any`).
//!
//! Every expectation is oracle-pinned against the pinned conformance
//! typescript@7.0.2 via `scripts/conformance/oracle.sh`, strict and
//! non-strict, 2026-08-19.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check_with(source: &str, strict: bool) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions {
            strict,
            strict_null_checks: strict,
            strict_function_types: strict,
            no_implicit_any: strict,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn messages(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Asserts exactly one TS2345 whose combined head + elaboration text contains
/// every `needles` entry and none of the `forbidden` entries, in both modes.
fn assert_single_ts2345(source: &str, needles: &[&str], forbidden: &[&str], context: &str) {
    for strict in [true, false] {
        let diags = check_with(source, strict);
        let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
        assert_eq!(
            ts2345.len(),
            1,
            "{context} (strict: {strict}): expected exactly one TS2345, got: {:#?}",
            messages(&diags)
        );
        let mut chain = vec![ts2345[0].message_text.clone()];
        chain.extend(
            ts2345[0]
                .related_information
                .iter()
                .map(|related| related.message_text.clone()),
        );
        let combined = chain.join("\n");
        for needle in needles {
            assert!(
                combined.contains(needle),
                "{context} (strict: {strict}): missing `{needle}` in:\n{combined}"
            );
        }
        for bad in forbidden {
            assert!(
                !combined.contains(bad),
                "{context} (strict: {strict}): unexpected `{bad}` in:\n{combined}"
            );
        }
    }
}

/// Receiver instantiation fixes the callback's return type parameter: the head
/// renders the instantiated pair, never `any` from an out-of-scope annotation
/// re-resolution and never the later argument's literal.
#[test]
fn class_fixed_receiver_renders_instantiated_callback_pair() {
    assert_single_ts2345(
        r#"
class C3<T, U> {
    foo3(x: T, cb: (a: T) => U, y: U) {
        return cb(x);
    }
}
declare var c3: C3<number, number>;
var r12 = c3.foo3(1, function (a) { return '' }, 1);
"#,
        &[
            "parameter of type '(a: number) => number'",
            "Type 'string' is not assignable to type 'number'.",
        ],
        &["(a: any) => any", "=> 1'"],
        "class-fixed receiver",
    );
}

/// Renamed binders and a string instantiation: same fixed-receiver rule.
#[test]
fn class_fixed_receiver_renamed_binders_string_instantiation() {
    assert_single_ts2345(
        r#"
class Grid<P, Q> {
    put(x: P, cb: (v: P) => Q, y: Q) {
        return cb(x);
    }
}
declare var g: Grid<string, string>;
var out = g.put('a', function (v) { return 1; }, 'z');
"#,
        &[
            "parameter of type '(v: string) => string'",
            "Type 'number' is not assignable to type 'string'.",
        ],
        &["(v: any) => any", "=> 1'"],
        "class-fixed receiver, renamed binders",
    );
}

/// An explicit type-argument list fixes every type parameter before inference:
/// the head renders the explicit instantiation, not the later literal.
#[test]
fn explicit_type_arguments_render_fixed_instantiation() {
    assert_single_ts2345(
        r#"
function foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
    return cb(x);
}
var r8 = foo3<number, number>(1, function (a) { return ''; }, 1);
"#,
        &["parameter of type '(a: number) => number'"],
        &["=> 1'"],
        "explicit type arguments",
    );
}

/// Negative control: a call that genuinely infers the type parameter from a
/// later naked literal keeps the literal in the head (`tsc` renders the
/// unwidened inference result in both positions).
#[test]
fn inferred_later_literal_keeps_literal_head() {
    assert_single_ts2345(
        r#"
function foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
    return cb(x);
}
var r8 = foo3(1, function (a) { return ''; }, 1);
"#,
        &["parameter of type '(a: number) => 1'"],
        &["parameter of type '(a: number) => number'"],
        "inferred later literal",
    );
}

/// Negative control, renamed binders and a string literal.
#[test]
fn inferred_later_string_literal_keeps_literal_head() {
    assert_single_ts2345(
        r#"
function blend<A, B>(p: A, fn: (q: A) => B, r: B) {
    return fn(p);
}
var out = blend('s', function (q) { return 2; }, 'x');
"#,
        &["parameter of type '(q: string) => \"x\"'"],
        &["parameter of type '(q: string) => string'"],
        "inferred later string literal, renamed binders",
    );
}

/// A generic method on a generic class infers its own type parameters even
/// though the receiver is instantiated: the literal head stays.
#[test]
fn generic_method_on_generic_class_keeps_inferred_literal_head() {
    assert_single_ts2345(
        r#"
class D<T, U> {
    foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
        return cb(x);
    }
}
declare var d: D<number, string>;
var r = d.foo3(1, function (a) { return ''; }, 1);
"#,
        &["parameter of type '(a: number) => 1'"],
        &["parameter of type '(a: number) => number'"],
        "generic method on generic class",
    );
}

/// An interface call member with its own type parameters infers at the call.
#[test]
fn interface_member_call_keeps_inferred_literal_head() {
    assert_single_ts2345(
        r#"
interface HasM {
    m<T, U>(x: T, cb: (a: T) => U, y: U): U;
}
declare var h: HasM;
var r = h.m(1, function (a) { return ''; }, 1);
"#,
        &["parameter of type '(a: number) => 1'"],
        &["parameter of type '(a: number) => number'"],
        "interface member call",
    );
}

/// An alias-typed generic function value infers at the call.
#[test]
fn alias_typed_function_keeps_inferred_literal_head() {
    assert_single_ts2345(
        r#"
type Fn = <T, U>(x: T, cb: (a: T) => U, y: U) => U;
declare var fw: Fn;
var r = fw(1, function (a) { return ''; }, 1);
"#,
        &["parameter of type '(a: number) => 1'"],
        &["parameter of type '(a: number) => number'"],
        "alias-typed generic function",
    );
}

/// A non-generic method whose parameter annotation is a named alias keeps the
/// alias spelling: bailing out of the annotation re-resolution must fall back
/// to the checked parameter type's own alias-aware display, not lose the name.
#[test]
fn nongeneric_method_alias_annotation_keeps_alias_name() {
    assert_single_ts2345(
        r#"
type Cb = (a: string) => string;
interface HasM2 {
    m(x: number, cb: Cb): void;
}
declare var h2: HasM2;
h2.m(1, function (a: string) { return 2; });
"#,
        &[
            "parameter of type 'Cb'",
            "Type 'number' is not assignable to type 'string'.",
        ],
        &["(a: any) => any"],
        "non-generic method, alias annotation",
    );
}

/// A non-generic method with a structural annotation renders the checked pair.
#[test]
fn nongeneric_method_concrete_annotation_renders_checked_pair() {
    assert_single_ts2345(
        r#"
class Plain {
    m(x: number, cb: (a: string) => string): void { }
}
declare var p: Plain;
p.m(1, function (a: string) { return 2; });
"#,
        &[
            "parameter of type '(a: string) => string'",
            "Type 'number' is not assignable to type 'string'.",
        ],
        &["(a: any) => any"],
        "non-generic method, concrete annotation",
    );
}

/// Living TODO (`#[ignore]`d, red on main): when the call infers the type
/// parameter from a later naked literal, the 7.0.2 oracle keeps the literal in
/// the ELABORATION too (`Type 'string' is not assignable to type '1'.`); tsz
/// widens the elaboration pair to the base primitive because inference stores
/// the widened candidate. Aligning that is the coupled inference-widening
/// family PR #17693/#17709 both regressed (see the fences in
/// `generic_signature_dual_callback_inference_tests.rs`); do not fix it
/// display-side.
#[test]
#[ignore = "pre-existing divergence: elaboration widens the inferred literal; oracle 7.0.2 keeps '1'"]
fn inferred_later_literal_elaboration_keeps_literal() {
    assert_single_ts2345(
        r#"
function foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
    return cb(x);
}
var r8 = foo3(1, function (a) { return ''; }, 1);
"#,
        &["Type 'string' is not assignable to type '1'."],
        &["Type 'string' is not assignable to type 'number'."],
        "inferred later literal, elaboration",
    );
}
