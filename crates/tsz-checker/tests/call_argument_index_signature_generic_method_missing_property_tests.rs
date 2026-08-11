//! Call-argument missing-property promotion for a target that carries BOTH an
//! index signature AND a method with its own (bound) type parameter (#17145).
//!
//! Structural rule: when an incompatible value is passed as a call argument to a
//! parameter whose object/interface type declares both (a) a number/string index
//! signature and (b) a method member whose signature has its own type
//! parameter(s) (`m<S>(x: S): S`), tsc promotes the sole/grouped
//! missing-property failure to the primary diagnostic (TS2741/TS2739/TS2740),
//! exactly as it already does for a direct assignment. tsz previously dropped
//! the elaboration and emitted a bare TS2345 for this shape only.
//!
//! Root cause: the call-argument display gate
//! (`preserve_type_parameter_expected_display`) tested
//! `contains_type_parameters`, which counts a member method's *bound* signature
//! parameter as a type parameter (and, when the object also carries an index
//! signature, surfaces it as a raw `TypeParameter`). That routed the argument to
//! the bare "preserve the generic parameter display" head, which never runs the
//! missing-property elaboration. The gate now uses `contains_free_type_parameters`
//! so only a genuinely FREE type parameter (from an enclosing generic signature)
//! preserves its display; a member-bound parameter does not.
//!
//! These tests vary the interface name, the type-parameter name, the property
//! names, and the index-signature key/value types so a fix keyed to a particular
//! spelling would not satisfy them. They assert on the diagnostic code and the
//! named missing member(s), not on the exact rendered type text.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when a sole-missing-property TS2741 names `prop` as missing and required.
fn has_single_missing_property(diags: &[Diagnostic], prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing");
    diags.iter().any(|d| {
        d.code == 2741 && d.message_text.contains(&needle) && d.message_text.contains("required in")
    })
}

/// True when a grouped missing-property diagnostic (TS2739/TS2740) lists every
/// name in `props`.
fn has_grouped_missing_properties(diags: &[Diagnostic], props: &[&str]) -> bool {
    diags.iter().any(|d| {
        (d.code == 2739 || d.code == 2740)
            && d.message_text
                .contains("is missing the following properties from type")
            && props.iter().all(|p| d.message_text.contains(p))
    })
}

/// True when a bare TS2345 argument error was emitted with no missing-property
/// promotion — the pre-fix behavior this issue is about.
fn has_bare_argument_ts2345(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.code == 2345 && d.message_text.contains("is not assignable to parameter"))
}

/// The reported repro: index signature + generic method, passed as a call
/// argument. tsc promotes to TS2741; tsz used to emit a bare TS2345.
#[test]
fn call_argument_index_and_generic_method_promotes_single_missing_property() {
    let diags = diagnostics(
        "interface Big {\n\
         \x20   m<S>(x: S): S;\n\
         \x20   readonly [n: number]: string;\n\
         }\n\
         declare function f(x: Big): void;\n\
         f({});",
    );
    assert!(
        has_single_missing_property(&diags, "m"),
        "call argument must promote to TS2741 naming the missing method `m`: {diags:?}"
    );
    assert!(
        !has_bare_argument_ts2345(&diags),
        "the bare TS2345 head must no longer be emitted for this shape: {diags:?}"
    );
}

/// Anti-hardcoding: rename the interface, the bound type parameter, the missing
/// method, and the index-signature key/value types. The promotion must still
/// fire — the fix is structural, not keyed to any spelling.
#[test]
fn call_argument_promotion_survives_renamed_binders() {
    let diags = diagnostics(
        "interface Container {\n\
         \x20   transform<Elem>(value: Elem): Elem;\n\
         \x20   readonly [key: number]: boolean;\n\
         }\n\
         declare function accept(c: Container): void;\n\
         accept({});",
    );
    assert!(
        has_single_missing_property(&diags, "transform"),
        "renamed shape must still promote to TS2741 naming `transform`: {diags:?}"
    );
    assert!(
        !has_bare_argument_ts2345(&diags),
        "no bare TS2345 for the renamed shape: {diags:?}"
    );
}

/// A string index signature (not just numeric) alongside the generic method is
/// the same shape and must promote identically.
#[test]
fn call_argument_promotion_with_string_index_signature() {
    let diags = diagnostics(
        "interface Bag {\n\
         \x20   pick<K>(k: K): K;\n\
         \x20   readonly [name: string]: unknown;\n\
         }\n\
         declare function use(b: Bag): void;\n\
         use({});",
    );
    assert!(
        has_single_missing_property(&diags, "pick"),
        "string-index shape must promote to TS2741 naming `pick`: {diags:?}"
    );
}

/// Several required members missing at once (a generic method plus two plain
/// properties) alongside the index signature: tsc groups them into TS2739/TS2740.
#[test]
fn call_argument_groups_multiple_missing_properties() {
    let diags = diagnostics(
        "interface Multi {\n\
         \x20   alpha<A>(x: A): A;\n\
         \x20   beta: number;\n\
         \x20   gamma: string;\n\
         \x20   readonly [i: number]: symbol;\n\
         }\n\
         declare function want(m: Multi): void;\n\
         want({});",
    );
    assert!(
        has_grouped_missing_properties(&diags, &["alpha", "beta", "gamma"]),
        "multiple missing members must group into TS2739/TS2740 listing all names: {diags:?}"
    );
    assert!(
        !has_bare_argument_ts2345(&diags),
        "no bare TS2345 when several properties are missing: {diags:?}"
    );
}

/// Parity guard: the identical target as a DIRECT ASSIGNMENT was already correct
/// (TS2741). Keep it pinned so the two paths cannot drift apart again.
#[test]
fn direct_assignment_same_shape_still_promotes() {
    let diags = diagnostics(
        "interface Big {\n\
         \x20   m<S>(x: S): S;\n\
         \x20   readonly [n: number]: string;\n\
         }\n\
         const b: Big = {};",
    );
    assert!(
        has_single_missing_property(&diags, "m"),
        "direct assignment must keep promoting to TS2741: {diags:?}"
    );
}

/// Control: the generic method WITHOUT an index signature already promoted; keep
/// it green so the fix does not regress the single-condition case.
#[test]
fn control_generic_method_only_still_promotes() {
    let diags = diagnostics(
        "interface OnlyGeneric {\n\
         \x20   m<S>(x: S): S;\n\
         }\n\
         declare function f(x: OnlyGeneric): void;\n\
         f({});",
    );
    assert!(
        has_single_missing_property(&diags, "m"),
        "generic-method-only target must still promote to TS2741: {diags:?}"
    );
}

/// Control: the index signature WITHOUT a generic method already promoted; keep
/// it green so the fix does not regress the single-condition case.
#[test]
fn control_index_signature_only_still_promotes() {
    let diags = diagnostics(
        "interface OnlyIndex {\n\
         \x20   m(x: number): number;\n\
         \x20   readonly [n: number]: string;\n\
         }\n\
         declare function f(x: OnlyIndex): void;\n\
         f({});",
    );
    assert!(
        has_single_missing_property(&diags, "m"),
        "index-signature-only target must still promote to TS2741: {diags:?}"
    );
}
