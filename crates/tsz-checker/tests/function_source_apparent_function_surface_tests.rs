//! A function source is compared against the whole global `Function` surface.
//!
//! `tsc` models a function value's apparent type as its call and construct
//! signatures plus every member of the global `Function` interface — `length`,
//! `name`, `bind`, `call`, `apply`, `toString`, `arguments`, `caller`,
//! `prototype`. A target requiring any of those is satisfied by any function.
//!
//! tsz synthesized a two-name stub instead (`call`/`apply` for a callable,
//! `prototype` for a constructor), which is the property set the weak-type rule
//! needs, not the apparent surface. Every other `Function` member therefore read
//! as absent and the target's required property failed with a spurious
//! `TS2322`.
//!
//! The member *types* have to come from the interface too, not from a widened
//! stub of `any`s: `{ length: string }` must still be rejected, because
//! `Function` declares `length: number`.
//!
//! Rows below were pinned against `typescript@7.0.2`
//! (`--noEmit --strict --lib es2022 --target es2022`).

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_with_libs_code_messages, has_diagnostic_code, load_default_lib_files,
};

/// The apparent surface under test *is* the global `Function` interface, so
/// every row runs against the real lib bundle — with no lib loaded there is no
/// interface to resolve and the synthesized two-name stub is the whole answer.
fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    has_diagnostic_code(&get_diagnostics(source), code)
}

fn is_clean(source: &str) -> bool {
    get_diagnostics(source).is_empty()
}

// =========================================================================
// Required `Function` members on the target: tsc accepts, tsz rejected
// =========================================================================

#[test]
fn function_satisfies_a_length_target() {
    assert!(
        is_clean("var q1: { length: number } = () => {};"),
        "`Function` declares `length: number`, so every function provides it"
    );
}

#[test]
fn function_satisfies_a_name_target() {
    assert!(is_clean("var q2: { name: string } = () => {};"));
}

#[test]
fn function_satisfies_a_bind_target() {
    assert!(is_clean("var q3: { bind(...a: any[]): any } = () => {};"));
}

#[test]
fn function_satisfies_an_arguments_target() {
    assert!(is_clean("var q7: { arguments: any } = () => {};"));
}

#[test]
fn function_satisfies_a_caller_target() {
    assert!(is_clean("var q8: { caller: Function } = () => {};"));
}

#[test]
fn function_satisfies_a_prototype_target() {
    // `Function` declares `prototype: any`, so a plain arrow satisfies this too —
    // not only a constructor.
    assert!(is_clean("var q9: { prototype: any } = () => {};"));
}

#[test]
fn function_satisfies_a_multi_member_function_surface_target() {
    assert!(is_clean(
        "var q10: { length: number; name: string } = () => {};"
    ));
}

#[test]
fn a_declared_function_satisfies_a_length_target() {
    // Binder-name variation: a `function` declaration, not an arrow.
    assert!(is_clean(
        r#"
function namedFn(alpha: number, beta: number) {}
var d1: { length: number } = namedFn;
"#
    ));
}

#[test]
fn an_aliased_target_type_satisfies_the_same_surface() {
    // Alias/wrapper form of the same target.
    assert!(is_clean(
        r#"
type Sized = { length: number };
type Named = { name: string };
var a1: Sized = () => {};
var a2: Named = function renamedBinder() {};
"#
    ));
}

#[test]
fn a_nested_target_position_satisfies_the_same_surface() {
    // The relation is reached as a nested property value, not a direct assignment.
    assert!(is_clean(
        r#"
var nested: { outer: { length: number } } = { outer: () => {} };
"#
    ));
}

#[test]
fn a_generic_parameter_constrained_to_the_function_surface_accepts_a_function() {
    assert!(is_clean(
        r#"
declare function takesSized<T extends { length: number }>(value: T): T;
takesSized(() => {});
"#
    ));
}

// =========================================================================
// Negative rows: the surface is the real interface, not a widened stub
// =========================================================================

#[test]
fn function_does_not_satisfy_a_wrongly_typed_length_target() {
    assert!(
        has_error_with_code("var q11: { length: string } = () => {};", 2322),
        "`Function.length` is `number`; a `string` target must still fail"
    );
}

#[test]
fn function_does_not_satisfy_a_target_requiring_a_non_function_member() {
    assert!(has_error_with_code(
        "var q12: { zzz: number } = () => {};",
        2322
    ));
}

#[test]
fn function_does_not_satisfy_a_mixed_target_with_one_foreign_member() {
    assert!(
        has_error_with_code("var q13: { length: number; zzz: number } = () => {};", 2322),
        "one unsatisfiable member is enough, even beside a real `Function` member"
    );
}

// =========================================================================
// Weak targets: owned by the weak-type rule, deliberately untouched
// =========================================================================

#[test]
fn function_does_not_satisfy_a_weak_target_naming_a_function_member() {
    // tsc rejects with TS2559 ("no properties in common"): its weak-type rule
    // scans the source's *declared* properties, which a bare function has none
    // of — the apparent `Function` surface never enters that scan.
    assert!(
        !is_clean("var w1: { length?: number } = () => {};"),
        "widening the apparent surface must not silence the weak-type rejection"
    );
}

#[test]
fn function_does_not_satisfy_a_weak_target_naming_no_function_member() {
    assert!(!is_clean("var w3: { zzz?: string } = () => {};"));
}

#[test]
fn function_stays_a_member_of_an_intersection_with_a_weak_object_part() {
    // The intersection-member arm suppresses the weak rule; tsc accepts.
    assert!(is_clean(
        r#"
declare var value: (() => void) & { brand?: number };
var i1: (() => void) & { brand?: number } = value;
"#
    ));
}

// =========================================================================
// Constructor sources: same surface
// =========================================================================

#[test]
fn a_class_constructor_satisfies_the_function_surface() {
    assert!(is_clean(
        r#"
class Renamed {}
var c1: { prototype: any } = Renamed;
var c2: { length: number } = Renamed;
var c3: { name: string } = Renamed;
var c4: { bind(...a: any[]): any } = Renamed;
"#
    ));
}

#[test]
fn a_class_constructor_with_statics_still_satisfies_the_function_surface() {
    // The callable-shape arm (source has its own declared properties) takes the
    // same second opinion as the bare-function arm.
    assert!(is_clean(
        r#"
class WithStatics {
    static tag = "t";
}
var s1: { length: number } = WithStatics;
var s2: { tag: string } = WithStatics;
"#
    ));
}

#[test]
fn a_constructor_does_not_satisfy_a_target_requiring_a_non_function_member() {
    // The oracle reports the missing-property spelling here (`TS2741`), not the
    // generic `TS2322` it uses for the arrow-function rows above.
    assert!(has_error_with_code(
        r#"
class Renamed {}
var c9: { zzz: number } = Renamed;
"#,
        2741
    ));
}

// =========================================================================
// The sibling rules this arm shares with #16473 / #16481 stay put
// =========================================================================

#[test]
fn function_still_fails_a_numeric_index_target() {
    // #16473's rule: a function's apparent type carries no numeric index
    // signature, and the wider surface must not resurrect one.
    assert!(has_error_with_code(
        r#"
interface Bar { b: number; }
declare var target: { [n: number]: Bar };
declare var source: (x: any) => void;
target = source;
"#,
        2322
    ));
}

#[test]
fn function_still_satisfies_a_call_only_target() {
    assert!(is_clean(
        r#"
declare var target: { call(x: any): any };
declare var source: (x: any) => void;
target = source;
"#
    ));
}
