//! The non-primitive `object` intrinsic as the SOURCE of a failed
//! assignability relation surfaces the generic TS2322 (assignment) / TS2345
//! (argument) head naming `object` verbatim — never a `{}`-rendered
//! TS2741/TS2739/TS2740 missing-property head.
//!
//! `object` carries `TypeFlags.NonPrimitive`, not `TypeFlags.StructuredType`,
//! so tsc never routes it through `propertiesRelatedTo` (the owner of
//! TS2741/TS2739/TS2740). The empty object `{}` and a members-less interface,
//! by contrast, are genuine structured object sources and keep their TS2741
//! head. See issue #17103.
//!
//! Every row is pinned against `typescript@7.0.2`
//! (`--strict --target es2015 --lib es2015 --pretty false`). The oracle keeps
//! the missing-property line only as *nested* related information under the
//! generic head; these tests assert the top-level diagnostic code, which is
//! where tsz diverged (an extra `{}`-rendered TS2741/TS2739/TS2740 head).
use tsz_checker::test_utils::check_source_diagnostics;

fn top_level_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// `object` intrinsic source → generic TS2322 head, never TS2741/TS2739/TS2740
// ---------------------------------------------------------------------------

#[test]
fn object_source_to_single_property_target_is_ts2322_not_ts2741() {
    let codes = top_level_codes(
        "declare let a: object;\n\
         let y: { foo: string } = a;\n",
    );
    assert!(
        codes.contains(&2322),
        "expected top-level TS2322 for `object` source, got {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "the `{{}}`-rendered TS2741 missing-property head must not fire for an \
         `object` source, got {codes:?}"
    );
}

#[test]
fn object_source_to_multi_property_target_is_ts2322_not_ts2739_or_ts2740() {
    let codes = top_level_codes(
        "declare let a: object;\n\
         let y: { foo: string; bar: number } = a;\n",
    );
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
    assert!(
        !codes.contains(&2739) && !codes.contains(&2740),
        "the multi-property missing head (TS2739/TS2740) must not fire for an \
         `object` source, got {codes:?}"
    );
}

#[test]
fn object_source_to_interface_target_is_ts2322() {
    let codes = top_level_codes(
        "interface J { foo: string; bar: number; }\n\
         declare let a: object;\n\
         let j: J = a;\n",
    );
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
    assert!(
        !codes.contains(&2739) && !codes.contains(&2740) && !codes.contains(&2741),
        "no missing-property head for an `object` source, got {codes:?}"
    );
}

#[test]
fn object_source_to_index_signature_target_is_ts2322() {
    let codes = top_level_codes(
        "declare let a: object;\n\
         let idx: { [k: string]: number } = a;\n",
    );
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
    assert!(
        !codes.contains(&2741) && !codes.contains(&2739) && !codes.contains(&2740),
        "an index-signature target must not draw a missing-property head for an \
         `object` source, got {codes:?}"
    );
}

#[test]
fn object_source_to_class_target_is_ts2322() {
    let codes = top_level_codes(
        "class C { m() {} }\n\
         declare let a: object;\n\
         let c: C = a;\n",
    );
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
    assert!(
        !codes.contains(&2741),
        "a class target must not draw TS2741 for an `object` source, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Argument position: the same source surfaces TS2345, never a missing-property
// head
// ---------------------------------------------------------------------------

#[test]
fn object_source_in_argument_position_is_ts2345_not_ts2741() {
    let codes = top_level_codes(
        "declare let a: object;\n\
         function need(p: { foo: string }): void {}\n\
         need(a);\n",
    );
    assert!(
        codes.contains(&2345),
        "expected top-level TS2345 for an `object` argument, got {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "no TS2741 head for an `object` argument, got {codes:?}"
    );
}

#[test]
fn object_source_argument_to_index_signature_is_ts2345() {
    let codes = top_level_codes(
        "declare let a: object;\n\
         function need(p: { [k: string]: number }): void {}\n\
         need(a);\n",
    );
    assert!(codes.contains(&2345), "expected TS2345, got {codes:?}");
    assert!(
        !codes.contains(&2741) && !codes.contains(&2739) && !codes.contains(&2740),
        "no missing-property head for an `object` argument, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Anti-hardcoding: the rule is about the `object` intrinsic, not any name
// ---------------------------------------------------------------------------

#[test]
fn the_rule_holds_regardless_of_binder_names() {
    let codes = top_level_codes(
        "declare let somethingElse: object;\n\
         let target: { alpha: string; beta: number } = somethingElse;\n",
    );
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
    assert!(
        !codes.contains(&2739) && !codes.contains(&2740) && !codes.contains(&2741),
        "renaming the binders must not change the head, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Controls: genuine structured object sources keep their TS2741 head, and
// `unknown` keeps its TS2322 — the fix must not disturb either
// ---------------------------------------------------------------------------

#[test]
fn empty_object_literal_source_keeps_ts2741() {
    let codes = top_level_codes(
        "declare let e: {};\n\
         let y: { foo: string } = e;\n",
    );
    assert!(
        codes.contains(&2741),
        "the empty object `{{}}` is a structured source and keeps TS2741, got {codes:?}"
    );
}

#[test]
fn members_less_interface_source_keeps_ts2741() {
    let codes = top_level_codes(
        "interface I {}\n\
         declare let i: I;\n\
         let y: { foo: string } = i;\n",
    );
    assert!(
        codes.contains(&2741),
        "a members-less interface is a structured source and keeps TS2741, got {codes:?}"
    );
}

#[test]
fn unknown_source_stays_ts2322() {
    let codes = top_level_codes(
        "declare let u: unknown;\n\
         let y: { foo: string } = u;\n",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 for `unknown`, got {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "`unknown` never draws a missing-property head, got {codes:?}"
    );
}
