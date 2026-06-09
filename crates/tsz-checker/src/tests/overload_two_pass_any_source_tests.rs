//! Two-pass overload resolution with `any` arguments (issue #13042).
//!
//! Structural rule: tsc's `chooseOverload` runs twice — first with the
//! subtype relation, where an `any` SOURCE is not related to concrete
//! targets (at every nesting level), then with the assignable relation in
//! declaration order. With an `any` argument and a mixed
//! non-generic/generic overload set, the non-generic candidate is skipped
//! in pass 1 and the generic one wins with `U = any`; when every candidate
//! fails pass 1, the first assignable candidate wins in declaration order.
//!
//! tsz routes pass 1 through the solver's `AnySourceNotRelated` relation
//! mode via `resolve_call_with_checker_adapter_subtype_pass`.
//!
//! The inferred result types are pinned through probe assignments:
//! a probe against an incompatible concrete target errors (TS2322) when the
//! call result is concrete and stays silent when the result is `any`.

use crate::test_utils::{check_source_diagnostics, diagnostic_count};

fn count(source: &str, code: u32) -> usize {
    diagnostic_count(&check_source_diagnostics(source), code)
}

/// Mixed overload set, `any` argument: the generic candidate wins pass 1
/// with `U = any`, so the call result is `any` (matrix case `e`).
#[test]
fn any_argument_prefers_generic_candidate_over_first_nongeneric() {
    let source = r#"
declare function grabItem(key: string): "s";
declare function grabItem<TPick>(key: TPick, alt?: TPick): TPick;
declare const opaque: any;
const picked = grabItem(opaque);
const probeNum: number = picked;
const probeObj: { marker: number } = picked;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "result should be `any` (generic candidate with TPick = any), not the literal \"s\""
    );
}

/// Declaration order with the generic candidate first: pass 1 selects it
/// directly, so the result is still `any`.
#[test]
fn any_argument_generic_first_declaration_order_still_any() {
    let source = r#"
declare function fetchSlot<TBox>(handle: TBox, fallback?: TBox): TBox;
declare function fetchSlot(handle: string): "s";
declare const blob: any;
const slot = fetchSlot(blob);
const probeNum: number = slot;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "generic-first declaration order should also produce `any`"
    );
}

/// All-non-generic overloads with an `any` argument: every candidate fails
/// pass 1, and pass 2 picks the FIRST candidate in declaration order
/// (matrix case `e2`).
#[test]
fn any_argument_all_nongeneric_falls_back_in_declaration_order() {
    let source = r#"
declare function stamp(value: string): "s";
declare function stamp(value: number): "n";
declare const fuzzy: any;
const tag = stamp(fuzzy);
const probeWrong: "n" = tag;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "pass 2 must select the first overload in declaration order (result \"s\", not `any`/\"n\")"
    );

    let ok_source = r#"
declare function stamp(value: string): "s";
declare function stamp(value: number): "n";
declare const fuzzy: any;
const tag = stamp(fuzzy);
const probeRight: "s" = tag;
"#;
    assert_eq!(count(ok_source, 2322), 0, "result should be exactly \"s\"");
}

/// When the generic candidate cannot match the call arity, the non-generic
/// candidate still wins through the pass-2 fallback.
#[test]
fn any_argument_generic_arity_mismatch_keeps_nongeneric_winner() {
    let source = r#"
declare function routeKey(name: string): "s";
declare function routeKey<TPair>(name: TPair, partner: TPair): TPair;
declare const loose: any;
const route = routeKey(loose);
const probeWrong: "x" = route;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "generic candidate fails arity, so the non-generic \"s\" result must win (not `any`)"
    );
}

/// Non-`any` arguments are unaffected: the first matching overload wins as
/// before, in both literal and numeric forms.
#[test]
fn concrete_arguments_keep_existing_overload_selection() {
    let source = r#"
declare function render(value: string): "s";
declare function render(value: number): "n";
const a = render("title");
const b = render(42);
const probeA: "s" = a;
const probeB: "n" = b;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "concrete arguments must keep today's overload selection"
    );
}

/// Reduce-style overload pair with an `any` seed: the union-typed callback
/// result flows from the generic candidate with `U = any` (matrix cases
/// `a`/`b`).
#[test]
fn reduce_like_any_seed_selects_generic_candidate() {
    let source = r#"
interface SegmentList {
    fold(merge: (acc: string, item: string) => string): string;
    fold(merge: (acc: string, item: string) => string, seed: string): string;
    fold<TAcc>(merge: (acc: TAcc, item: string) => TAcc, seed: TAcc): TAcc;
}
declare const segments: SegmentList;
declare const opaqueSeed: any;
const merged = segments.fold((acc, item) => acc[item], opaqueSeed);
const probeNum: number = merged;
const constant = segments.fold(() => "x", opaqueSeed);
const probeNum2: number = constant;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "any seed must select the generic fold (TAcc = any); result is `any`, not string"
    );
}

/// Reduce-style call with a concrete seed keeps the string overload
/// (matrix case `d`).
#[test]
fn reduce_like_concrete_seed_keeps_string_result() {
    let source = r#"
interface SegmentList {
    fold(merge: (acc: string, item: string) => string): string;
    fold(merge: (acc: string, item: string) => string, seed: string): string;
    fold<TAcc>(merge: (acc: TAcc, item: string) => TAcc, seed: TAcc): TAcc;
}
declare const segments: SegmentList;
const collapsed = segments.fold(() => "x", "start");
const probeNum: number = collapsed;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "concrete seed keeps the string overload; string is not assignable to number"
    );
}

/// An explicit callback parameter annotation makes the generic candidate
/// fail pass 1 too — the rejection applies inside the nested callback
/// parameter comparison, not just at the top-level argument (matrix case
/// `c`). Declaration order then selects the string overload in pass 2.
#[test]
fn annotated_callback_param_rejects_generic_candidate_at_nested_level() {
    let source = r#"
interface SegmentList {
    fold(merge: (acc: string, item: string) => string): string;
    fold(merge: (acc: string, item: string) => string, seed: string): string;
    fold<TAcc>(merge: (acc: TAcc, item: string) => TAcc, seed: TAcc): TAcc;
}
declare const segments: SegmentList;
declare const opaqueSeed: any;
const joined = segments.fold((acc: string, item) => acc + item, opaqueSeed);
const probeNum: number = joined;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "annotated callback param must reject the generic candidate in pass 1 (nested any source); result is string"
    );
}
