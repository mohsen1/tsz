//! An object literal's `get`/`set` accessor pair shares one declared type.
//!
//! `tsc` types an unannotated `set` accessor parameter from the paired `get`
//! accessor (`getAnnotatedAccessorType`, falling back to the getter's inferred
//! return type). The class-member path already did this via
//! `contextual_setter_parameter_types_for_class_accessor`; the object-literal
//! path did not, so the setter's parameter was implicitly `any` inside the
//! body while the *property* type was correctly the getter's type.
//!
//! The witnesses use `var y = <param>; var y: T;` because `TS2403` compares
//! the two declarations with type identity, which makes the parameter's type
//! observable in a diagnostic without depending on any rendered type text.
//!
//! Negative controls pin the two ways the pair must NOT be joined: an
//! annotated setter parameter keeps its annotation, and a setter with no
//! paired getter stays implicitly `any`.

use tsz_checker::test_utils::check_source_non_strict_codes;

/// `CheckerOptions::default()` is a strict run, and these shapes are all
/// `@strict: false` corpus rows, so every case goes through the non-strict
/// entry point.
fn assert_clean(src: &str) {
    let codes = check_source_non_strict_codes(src);
    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}

fn assert_codes(src: &str, expected: &[u32]) {
    let codes = check_source_non_strict_codes(src);
    assert_eq!(codes, expected, "unexpected diagnostics");
}

// ── setter parameter takes the paired getter's type ─────────────────────────

#[test]
fn object_literal_setter_param_takes_inferred_getter_return_type() {
    assert_clean("var o = { get n() { return ''; }, set n(v) { var y = v; var y: string; } };");
}

#[test]
fn object_literal_setter_param_takes_annotated_getter_return_type() {
    assert_clean(
        "var o = { get n(): string { return undefined; }, \
         set n(v) { var y = v; var y: string; } };",
    );
}

/// Declaration order must not matter: the getter is found by scanning the
/// literal's elements, not by what has been recorded so far.
#[test]
fn object_literal_setter_before_getter_still_pairs() {
    assert_clean("var o = { set n(v) { var y = v; var y: string; }, get n() { return ''; } };");
}

/// Binder-name independence: the pairing is by property name, and the same
/// property name spelled differently (quoted vs. bare, and two numeric
/// spellings of 32) must still pair, exactly as `tsc` pairs them.
#[test]
fn object_literal_accessor_pairing_is_independent_of_name_spelling() {
    assert_clean("var o = { get 'zz'() { return ''; }, set zz(q) { var w = q; var w: string; } };");
    assert_clean(
        "var o = { get 0x20() { return ''; }, set 3.2e1(q) { var w = q; var w: string; } };",
    );
}

/// The paired type is not restricted to primitives — an object getter type
/// reaches the setter body the same way.
#[test]
fn object_literal_setter_param_takes_object_getter_type() {
    assert_clean(
        "interface Pt { x: number; }\n\
         declare var pt: Pt;\n\
         var o = { get n() { return pt; }, set n(v) { var y = v; var y: Pt; } };",
    );
}

/// A renamed binder must behave identically — the rule is structural, not
/// keyed on any particular parameter or property identifier.
#[test]
fn object_literal_accessor_pair_survives_renamed_binders() {
    assert_clean(
        "var renamedOuter = { get renamedProp() { return ''; }, \
         set renamedProp(renamedParam) { var renamedLocal = renamedParam; \
         var renamedLocal: string; } };",
    );
}

/// Nesting: an inner object literal's pair must resolve against its own
/// elements, not the outer literal's.
#[test]
fn nested_object_literal_accessor_pairs_resolve_per_literal() {
    assert_clean(
        "var outer = { get n() { return 0; }, set n(v) { var y = v; var y: number; }, \
         inner: { get n() { return ''; }, set n(v) { var y = v; var y: string; } } };",
    );
}

// ── class parity (already correct — pinned so it cannot regress) ────────────

#[test]
fn class_setter_param_takes_paired_getter_type() {
    assert_clean("class C { get n() { return ''; } set n(v) { var y = v; var y: string; } }");
}

// ── negative controls ──────────────────────────────────────────────────────

/// No paired getter: the parameter stays implicitly `any`, so the redeclaration
/// really does conflict and `TS2403` is correct. `tsc` reports it here too.
#[test]
fn object_literal_setter_without_paired_getter_stays_any() {
    assert_codes(
        "var o = { set n(v) { var y = v; var y: string; } };",
        &[2403],
    );
}

/// An annotated setter parameter wins over the getter's type — the pair is
/// allowed to be split (TS 4.3+), and the write type must stay the annotation.
///
/// Both `TS2322`s are correct and oracle-pinned against `tsc` 7.0.2: one for
/// the getter's `string` body against the pair's `number` write type, one for
/// the `number` parameter read into a `string` local.
#[test]
fn annotated_setter_param_is_not_overridden_by_getter_type() {
    assert_codes(
        "var o = { get n() { return ''; }, set n(v: number) { var y: string = v; } };",
        &[2322, 2322],
    );
}

/// A getter with neither an annotation nor a body yields nothing to pair with,
/// so the setter parameter must not be silently typed from it.
#[test]
fn getter_only_accessor_does_not_type_an_unrelated_setter() {
    assert_codes(
        "var o = { get m() { return ''; }, set n(v) { var y = v; var y: string; } };",
        &[2403],
    );
}
