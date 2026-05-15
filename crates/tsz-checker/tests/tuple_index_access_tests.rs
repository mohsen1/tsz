//! Tests for tuple index access diagnostics:
//! - TS2493: Tuple out-of-bounds on single tuple types
//! - TS2339: Property does not exist on union-of-tuple types

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_codes};

#[test]
fn parameters_of_generic_function_allows_numeric_index() {
    let diagnostics = check_source_diagnostics(
        r#"
type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;
type ReturnType<T extends (...args: any) => any> = T extends (...args: any[]) => infer R ? R : any;

function apply<T extends (x: any) => any>(fn: T, arg: Parameters<T>[0]): ReturnType<T> {
  return fn(arg);
}

const result = apply((x: number) => x.toString(), 42);
"#,
    );
    let ts2536: Vec<_> = diagnostics.iter().filter(|d| d.code == 2536).collect();
    assert!(
        ts2536.is_empty(),
        "Expected Parameters<T>[0] to be accepted for callable-constrained T. Got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn readonly_variadic_tuple_to_mutable_variadic_tuple_emits_ts4104() {
    let diagnostics = check_source_diagnostics(
        r"
function f<T extends unknown[]>(m: [...T], r: readonly [...T]) {
    m = r;
}
declare let concrete: [string];
declare let readonlyConcrete: readonly [string];
concrete = readonlyConcrete;
",
    );
    let ts4104_messages = diagnostics
        .iter()
        .filter(|d| d.code == 4104)
        .map(|d| d.message_text.as_str())
        .collect::<Vec<_>>();
    assert!(
        ts4104_messages.len() >= 2
            && ts4104_messages
                .iter()
                .any(|message| message.contains("readonly [...T]")),
        "Expected TS4104 for both readonly variadic and concrete tuple assignments, got {ts4104_messages:?}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_type_level_tuple_out_of_bounds_ts2493() {
    let diagnostics = check_source_diagnostics(
        r"
type T1 = [string, number];
type T12 = T1[2];
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2493),
        "Expected TS2493 for out-of-bounds tuple index access, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_type_level_union_tuple_out_of_bounds_ts2339() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
type T22 = T2[2];
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "Expected TS2339 for out-of-bounds union tuple index access, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_runtime_union_tuple_out_of_bounds_ts2339() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
declare let t2: T2;
let t22 = t2[2];
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "Expected TS2339 for runtime union tuple out-of-bounds access, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_destructuring_union_tuple_out_of_bounds_ts2339() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
declare let t2: T2;
let [d0, d1, d2] = t2;
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "Expected TS2339 for destructuring union tuple out-of-bounds, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_union_tuple_valid_index_no_error() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
type T21 = T2[1];
",
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2339 && d.code != 2493),
        "Expected no TS2339/TS2493 for valid union tuple index, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Regression: errorForUsingPropertyOfTypeAsType03.ts
/// `type C1 = Color` is a type alias for an enum.  Accessing a non-existent
/// property on `C1` (e.g. `C1["Red"]`) should report the error against the
/// underlying enum's nominal name (`'Color'`), not the alias (`'C1'`).
/// tsc treats type aliases for enums transparently in TS2339 messages.
#[test]
fn test_ts2339_type_alias_for_enum_displays_underlying_enum_name() {
    let diagnostics = check_source_diagnostics(
        r"
namespace Test1 {
    enum Color { Red, Green, Blue }
    type C1 = Color;
    let c3: C1['Red']['toString'];
}
",
    );
    let ts2339 = diagnostics
        .iter()
        .find(|d| d.code == 2339)
        .expect("expected TS2339 for non-existent property on alias-of-enum");
    assert!(
        ts2339.message_text.contains("on type 'Color'"),
        "TS2339 should display underlying enum name `Color`, got: {}",
        ts2339.message_text
    );
    assert!(
        !ts2339.message_text.contains("on type 'C1'"),
        "TS2339 must not display alias name `C1`, got: {}",
        ts2339.message_text
    );
}

/// Regression for `castingTuple.ts`: when comparing two same-length tuples whose
/// element classes don't overlap, TS2352 must fire on the cast. Before the fix,
/// `deep_evaluate_object_properties` skipped tuple element types, leaving them
/// as `Lazy(DefId)` class refs that the solver's depth>0 Lazy heuristic
/// short-circuited to "comparable", masking the real mismatch.
#[test]
fn test_ts2352_tuple_cast_with_unrelated_element_classes_emits_error() {
    let diagnostics = check_source_diagnostics(
        r"
class A { a: number = 10; }
class C { c: number = 1; }
declare var pair: [C];
var t = <[A]>pair;
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2352),
        "Expected TS2352 for tuple cast between unrelated element classes, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Regression for the multi-element variant from `castingTuple.ts` line 32:
/// `<[A, I]>classCDTuple` where element 0 (C → A) doesn't overlap but element
/// 1 (D → I) does. Element-wise tsc semantics mean ANY non-overlapping element
/// triggers TS2352 — the matching `length: 2` literal must NOT mask the
/// element-0 mismatch.
#[test]
fn test_ts2352_tuple_cast_partial_element_overlap_still_emits_error() {
    let diagnostics = check_source_diagnostics(
        r"
interface I {}
class A { a: number = 10; }
class C { c: number = 1; }
class D implements I { d: number = 1; }
declare var classCDTuple: [C, D];
var t9 = <[A, I]>classCDTuple;
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2352),
        "Expected TS2352 for [C, D] as [A, I] (element 0 doesn't overlap), got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Regression for `emitCapturingThisInTupleDestructuring1.ts`: when an array
/// destructuring assignment has multiple targets that exceed the source tuple
/// length, tsc emits TS2493 for **each** out-of-bounds element, not just the
/// first. The previous implementation early-returned after the first
/// diagnostic, dropping subsequent out-of-bounds errors.
#[test]
fn test_ts2493_destructuring_assignment_emits_for_each_out_of_bounds_element() {
    let diagnostics = check_source_diagnostics(
        r"
declare let array: [any];
declare let a: any;
declare let b: any;
declare let c: any;
[a, b, c] = array;
",
    );
    let ts2493_count = diagnostics.iter().filter(|d| d.code == 2493).count();
    assert_eq!(
        ts2493_count,
        2,
        "Expected TS2493 once per out-of-bounds element (indexes 1 and 2), got {}: {:?}",
        ts2493_count,
        diagnostics
            .iter()
            .filter(|d| d.code == 2493)
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
    // One diagnostic per out-of-bounds index — verify both messages mention
    // the right index numbers.
    let messages: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.code == 2493)
        .map(|d| d.message_text.clone())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("at index '1'")),
        "Expected TS2493 message mentioning index '1', got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("at index '2'")),
        "Expected TS2493 message mentioning index '2', got: {messages:?}"
    );
}

/// Regression for the union-of-tuples branch of array destructuring
/// assignment bounds checking: TS2339 must fire for **each** element where
/// every union member is out of bounds, not just the first.
#[test]
fn test_ts2339_union_destructuring_assignment_emits_for_each_out_of_bounds_element() {
    let diagnostics = check_source_diagnostics(
        r"
declare let u: [boolean] | [string];
declare let a: any;
declare let b: any;
declare let c: any;
[a, b, c] = u;
",
    );
    let ts2339_count = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339_count,
        2,
        "Expected TS2339 once per out-of-bounds element (indexes 1 and 2), got {}: {:?}",
        ts2339_count,
        diagnostics
            .iter()
            .filter(|d| d.code == 2339)
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// Regression for `restParameterWithBindingPattern3.ts`: object binding
/// patterns whose property name is a numeric literal (`{0: a, 3: d}`) on
/// a tuple type must emit TS2493 for any out-of-bounds index. Equivalent
/// to element access via `tuple[3]` — both go through the destructuring
/// path for parameter destructuring, but only the array-binding path was
/// previously bounds-checked. Object-binding numeric keys had no check.
#[test]
fn ts2493_object_binding_pattern_numeric_property_on_tuple_out_of_bounds() {
    let diagnostics = check_source_diagnostics(
        r"
function c(...{0: a, length, 3: d}: [boolean, string, number]) { }
",
    );
    let ts2493_messages: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.code == 2493)
        .map(|d| d.message_text.clone())
        .collect();
    assert!(
        ts2493_messages.iter().any(|m| m.contains("at index '3'")),
        "Expected TS2493 for out-of-bounds object-binding property '3' on \
         tuple of length 3, got: {ts2493_messages:?}"
    );
}

/// Companion regression: the bounds check is structural (per-tuple), not
/// keyed off the user's identifier names. Different binding names with a
/// different out-of-bounds key still surface TS2493 (locks the rule per
/// .claude/CLAUDE.md §25 anti-hardcoding directive).
#[test]
fn ts2493_object_binding_pattern_numeric_property_on_tuple_param_name_independent() {
    let diagnostics = check_source_diagnostics(
        r"
function fn(...{2: x, 5: y}: [boolean, string]) { }
",
    );
    let ts2493_count = diagnostics.iter().filter(|d| d.code == 2493).count();
    assert!(
        ts2493_count >= 2,
        "Expected TS2493 for both out-of-bounds keys '2' and '5' on tuple \
         of length 2, got {}: {:?}",
        ts2493_count,
        diagnostics
            .iter()
            .filter(|d| d.code == 2493)
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// Inverse rule: in-bounds numeric properties (and non-numeric keys like
/// `length`) on a fixed tuple do NOT emit TS2493. This locks that the
/// new check fires ONLY on out-of-bounds numeric keys.
#[test]
fn ts2493_object_binding_pattern_numeric_property_in_bounds_does_not_fire() {
    let diagnostics = check_source_diagnostics(
        r"
function ok(...{0: a, 1: b, 2: c, length}: [boolean, string, number]) { }
",
    );
    let ts2493_count = diagnostics.iter().filter(|d| d.code == 2493).count();
    assert_eq!(
        ts2493_count,
        0,
        "Did not expect TS2493 for in-bounds numeric properties, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2493)
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}
