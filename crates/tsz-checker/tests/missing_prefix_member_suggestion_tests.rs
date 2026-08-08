//! `TS2662`/`TS2663` — the "did you mean the static/instance member" suggestion
//! for an unresolved identifier inside a class, tsc's
//! `checkAndReportErrorForMissingPrefix`. Issue #16815.
//!
//! Structural rule, one sentence: when an unresolved value identifier sits in a
//! class member's body/initializer and names a member reachable on the enclosing
//! class's **constructor type** (`TS2662`, `Class.name`) or — in a non-static
//! context — its **instance type** (`TS2663`, `this.name`), tsc suggests that
//! member instead of the bare `TS2304`. A constructor type is a subtype of
//! `Function` (which extends `Object`), so `arguments`, `caller`, `length`,
//! `prototype`, `apply`, … resolve on it for *any* class — `arguments` in a
//! class field initializer at module scope is the #16815 witness: post-#16812
//! (which stopped `TS2815` firing when no function encloses the reference) it
//! fell through to a bare `TS2304`.
//!
//! The suggestion is decided from the AST (the enclosing `class` ancestor), not
//! the ambient enclosing-class state, so it is identical in the statement-walk
//! pass and the on-demand class-type-computation pass — the two both resolve the
//! same identifier and diagnostics dedupe by `(start, code)`, so a suggestion in
//! one pass and a bare `TS2304` in the other would otherwise both surface.
//!
//! These tests load the real default lib (via [`load_default_lib_files`]) so the
//! global `Function` interface resolves — the no-lib `check_source_strict`
//! harness cannot see `Function`'s members. Expectations were recorded from
//! `typescript@7.0.2` (`scripts/conformance/oracle.sh`) under `--strict --lib
//! es2022 --target es2022`.
//!
//! Out of scope (a known limitation, not asserted here): a member inherited from
//! a *base class* (`class Sub extends Base { p = baseStatic; }`) still gets a
//! bare `TS2304` rather than the suggestion — resolving it needs the class's own
//! (mid-computation) constructor/instance type, whose lookup is not stable across
//! the two resolution passes and would reintroduce a duplicate diagnostic.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{
    check_source_with_libs, load_default_lib_files, strict_checker_options,
};

/// The member-prefix *suggestion* diagnostics (`TS2662`/`TS2663`) for `source`,
/// as `"TS<code>: <message>"`, sorted.
///
/// Only the suggestion codes are asserted here: this in-process harness does not
/// exercise the bare-`TS2304` fallback arm (a name reachable on neither side
/// leaves this view empty rather than showing `TS2304`), so a fallback row reads
/// as "no suggestion". The bare `TS2304`, and the exact column anchors, are
/// verified end-to-end against the pinned oracle (see the module doc).
fn suggestions(source: &str, libs: &[Arc<LibFile>]) -> Vec<String> {
    let mut out: Vec<String> =
        check_source_with_libs(source, "test.ts", strict_checker_options(), libs)
            .iter()
            .filter(|d| matches!(d.code, 2662 | 2663))
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect();
    out.sort();
    out
}

fn assert_suggestions(source: &str, libs: &[Arc<LibFile>], expected: &[&str]) {
    let actual = suggestions(source, libs);
    let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(actual, expected, "source: {source}");
}

// ---------------------------------------------------------------------------
// TS2662 — the constructor-type (static) suggestion, including the `Function`
// members every constructor carries. Exactly one diagnostic (no duplicate).
// ---------------------------------------------------------------------------

#[test]
fn arguments_in_field_initializer_suggests_the_static_member() {
    let libs = load_default_lib_files();
    // The #16815 witness: `arguments` is a `Function` member, so it is a member
    // of `typeof C`. Exactly one diagnostic — the two resolution passes agree.
    assert_suggestions(
        "class C { p = arguments; }",
        &libs,
        &["TS2662: Cannot find name 'arguments'. Did you mean the static member 'C.arguments'?"],
    );
}

#[test]
fn caller_in_field_initializer_suggests_the_static_member() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class F { p = caller; }",
        &libs,
        &["TS2662: Cannot find name 'caller'. Did you mean the static member 'F.caller'?"],
    );
}

#[test]
fn prototype_is_a_function_member_too() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class C3 { p = prototype; }",
        &libs,
        &["TS2662: Cannot find name 'prototype'. Did you mean the static member 'C3.prototype'?"],
    );
}

#[test]
fn own_static_member_suggests_the_static_member() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class D { static s = 1; p = s; }",
        &libs,
        &["TS2662: Cannot find name 's'. Did you mean the static member 'D.s'?"],
    );
}

#[test]
fn static_context_prefers_the_static_suggestion() {
    let libs = load_default_lib_files();
    // In a static field initializer `this` is the constructor, so a static
    // member (here `prototype`, a `Function` member) is still suggested.
    assert_suggestions(
        "class J { static p = prototype; }",
        &libs,
        &["TS2662: Cannot find name 'prototype'. Did you mean the static member 'J.prototype'?"],
    );
}

// ---------------------------------------------------------------------------
// TS2663 — the instance-type suggestion, only in a non-static context.
// ---------------------------------------------------------------------------

#[test]
fn own_instance_member_suggests_the_instance_member() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class E { m() {} p = m; }",
        &libs,
        &["TS2663: Cannot find name 'm'. Did you mean the instance member 'this.m'?"],
    );
}

// ---------------------------------------------------------------------------
// Fallback rows: bare TS2304 (no suggestion). A name reachable on neither side,
// and an instance member referenced from a static context.
// ---------------------------------------------------------------------------

#[test]
fn a_name_on_neither_side_produces_no_suggestion() {
    let libs = load_default_lib_files();
    // Reachable on neither side → tsc's bare `TS2304` (verified via the oracle);
    // here the point is that no member-prefix suggestion is offered.
    assert_suggestions("class G { p = nonexistent; }", &libs, &[]);
}

#[test]
fn instance_member_from_a_static_initializer_produces_no_suggestion() {
    let libs = load_default_lib_files();
    // `m` is an instance member, but in `static p = m` `this` is the constructor,
    // so tsc reports a bare `TS2304` — no `this.m` suggestion (and `m` is not on
    // the constructor type, so the static arm does not fire either).
    assert_suggestions("class H { m() {} static p = m; }", &libs, &[]);
}

#[test]
fn arguments_inside_a_method_body_is_resolved_not_reported() {
    let libs = load_default_lib_files();
    // With an enclosing function, `arguments` resolves to `IArguments` — no
    // cannot-find-name diagnostic at all.
    assert_suggestions("class C { m() { return arguments; } }", &libs, &[]);
}

// ---------------------------------------------------------------------------
// Type-position regression (#16840, fixed on main via #16844's `is_in_type_query`).
// The property/parameter/return-type-annotation rows and their value-position
// controls live in `property_type_annotation_produces_no_suggestion` /
// `parameter_type_annotation_produces_no_suggestion` /
// `return_type_annotation_produces_no_suggestion` below. This section adds the
// coverage #16844 didn't: the exact codes on the pinned corpus fixture (not just
// the absence of TS2662/TS2663), and a static-member-in-a-parameter-annotation
// variant of the still-open static arm (see `a_static_member_in_a_type_position_produces_no_suggestion`
// below, no longer `#[ignore]`d).
// ---------------------------------------------------------------------------

/// All diagnostic codes for `source`, sorted — used below to assert the bare
/// `TS2304` fires (not just that no suggestion is offered).
fn diagnostic_codes(source: &str, libs: &[Arc<LibFile>]) -> Vec<u32> {
    let mut out: Vec<u32> =
        check_source_with_libs(source, "test.ts", strict_checker_options(), libs)
            .iter()
            .map(|d| d.code)
            .collect();
    out.sort_unstable();
    out
}

#[test]
fn typeof_of_a_static_member_in_a_parameter_annotation_produces_no_suggestion() {
    let libs = load_default_lib_files();
    // The static (TS2662) arm's type-position leak (fixed in resolved.rs, the
    // second emission site #16844 left open — see the un-ignored test below)
    // in a *parameter* annotation rather than a property annotation. `s` is
    // initialized and the operand lives in a parameter, not a bare
    // uninitialized instance property, so no unrelated
    // strictPropertyInitialization diagnostic is in play.
    assert_suggestions(
        "class C { static s: number = 0; m(x: typeof s) {} }",
        &libs,
        &[],
    );
    assert_eq!(
        diagnostic_codes("class C { static s: number = 0; m(x: typeof s) {} }", &libs),
        vec![2304],
        "must be the bare TS2304, not TS2662/TS2663"
    );
}

#[test]
fn typeof_in_a_value_expression_still_suggests_the_member() {
    let libs = load_default_lib_files();
    // Control: `typeof a` used as a value-position operand of an *expression*
    // (not a type annotation) is a value reference, so the suggestion still
    // applies there — the regression is specifically about type positions.
    assert_suggestions(
        "class C { a: number; m() { return typeof a; } }",
        &libs,
        &["TS2663: Cannot find name 'a'. Did you mean the instance member 'this.a'?"],
    );
}

#[test]
fn conformance_fixture_typeof_property_no_member_prefix_suggestion() {
    let libs = load_default_lib_files();
    // The exact regression witness from #16840: TypeScript's own
    // `tests/cases/compiler/typeofProperty.ts` corpus fixture (pinned at
    // typescript@7.0.2's `4d4f005c8541e0255a9d8791205fdce326e462bc` submodule
    // commit — `scripts/conformance/tsc-cache-full.json`'s
    // `compiler/typeofProperty.ts` entry records the oracle's 9-diagnostic set:
    // six bare TS2304 (interfaces I1-I3 and classes C1-C3, none suggested —
    // I1-I3 have no class to suggest from, and C1/C3's `typeof a`/`typeof e`
    // are type-position operands per this fix) plus three TS2564 (properties
    // `a`, `d`, `x` themselves lack initializers under strict mode — unrelated
    // to the `typeof` operand and independently oracle-confirmed).
    let source = r#"
interface I1 {
    a: number;
    b: typeof a; // Should yield error (a is not a value)
}

interface I2 {
    c: typeof d; // Should yield error (d is not a value)
    d: string;
}

interface I3 {
    e: typeof e; // Should yield error (e is not a value)
}

class C1 {
    a: number;
    b: typeof a; // Should yield error (a is not a value)
}


class C2 {
    c: typeof d; // Should yield error (d is not a value)
    d: string;
}

class C3 {
    e: typeof e; // Should yield error (e is not a value)
}



interface ValidInterface {
    x: string;
}

class ValidClass implements ValidInterface {
    x: string;
}
"#;
    assert_suggestions(source, &libs, &[]);
    assert_eq!(
        diagnostic_codes(source, &libs),
        vec![2304, 2304, 2304, 2304, 2304, 2304, 2564, 2564, 2564],
        "must match the pinned typescript@7.0.2 oracle set exactly — no TS2662/TS2663"
    );
}

// ---------------------------------------------------------------------------
// Binder-name invariance: the rule reads the class shape, never a name string.
// ---------------------------------------------------------------------------

#[test]
fn suggestion_is_binder_name_invariant() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class Zqx { static wob = 1; p = wob; }",
        &libs,
        &["TS2662: Cannot find name 'wob'. Did you mean the static member 'Zqx.wob'?"],
    );
    assert_suggestions(
        "class Wibble { frob() {} p = frob; }",
        &libs,
        &["TS2663: Cannot find name 'frob'. Did you mean the instance member 'this.frob'?"],
    );
}

// ---------------------------------------------------------------------------
// Type positions take the bare `TS2304` (#16840). `typeof a` in a type is the
// one place a *value* name appears inside type syntax, and `this.a` is not
// writable there, so tsc offers no suggestion — `checkAndReportErrorForMissing
// Prefix` is reached from `checkIdentifier`, never from the type-reference path.
//
// Each row pairs a type position with the *same* class and member name in a
// value position. Without the pairing these would only show that some sources
// produce no suggestion; paired, they isolate the position as the variable.
// Oracle-verified against `typescript@7.0.2` (`--singleThreaded
// --stableTypeOrdering true`): the type rows report `TS2304`, the value rows
// report `TS2663`.
// ---------------------------------------------------------------------------

#[test]
fn property_type_annotation_produces_no_suggestion() {
    let libs = load_default_lib_files();
    // `compiler/typeofProperty.ts` — the conformance row this regressed.
    assert_suggestions(
        "class C { a: number = 1; b: typeof a = 1 as any; }",
        &libs,
        &[],
    );
    // Same class, same member, value position → suggestion.
    assert_suggestions(
        "class C { a: number = 1; p = a; }",
        &libs,
        &["TS2663: Cannot find name 'a'. Did you mean the instance member 'this.a'?"],
    );
}

#[test]
fn parameter_type_annotation_produces_no_suggestion() {
    let libs = load_default_lib_files();
    assert_suggestions("class C { a: number = 1; m(x: typeof a) {} }", &libs, &[]);
    assert_suggestions(
        "class C { a: number = 1; m() { return a; } }",
        &libs,
        &["TS2663: Cannot find name 'a'. Did you mean the instance member 'this.a'?"],
    );
}

#[test]
fn return_type_annotation_produces_no_suggestion() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class C { a: number = 1; m(): typeof a { return 1 as any; } }",
        &libs,
        &[],
    );
    assert_suggestions(
        "class C { a: number = 1; constructor() { a; } }",
        &libs,
        &["TS2663: Cannot find name 'a'. Did you mean the instance member 'this.a'?"],
    );
}

#[test]
// Was a known, deliberately `#[ignore]`d divergence (older than #16834): a
// *declared static* named in a type position binds to a real symbol, so it
// never reaches `resolve_truly_unknown_identifier` — it was emitted from the
// resolved-symbol path in `identifier/resolved.rs` (`type_of_resolved_value_symbol`'s
// STATIC-flag arm), gated here the same way via `is_in_type_query`.
// `identifier/helpers.rs`'s `get_type_of_assignment_target` looked like a second
// site but every caller is a write/destructuring target, never a `typeof`
// operand, so it was never reachable from a type position and needs no change.
fn a_static_member_in_a_type_position_produces_no_suggestion() {
    let libs = load_default_lib_files();
    assert_suggestions(
        "class C { static s: number = 1; b: typeof s = 1 as any; }",
        &libs,
        &[],
    );
}

#[test]
fn a_static_member_in_a_value_position_still_suggests() {
    let libs = load_default_lib_files();
    // The control for the ignored row above: the same class and member in a
    // value position must keep its suggestion, so whoever fixes the static
    // arm's type-position leak has a live guard against over-correcting.
    assert_suggestions(
        "class C { static s: number = 1; m() { return s; } }",
        &libs,
        &["TS2662: Cannot find name 's'. Did you mean the static member 'C.s'?"],
    );
}

#[test]
fn arguments_in_a_type_position_produces_no_suggestion() {
    let libs = load_default_lib_files();
    // The `Function`-member path (#16815's original witness) is the widest arm —
    // it matches any name on `Function`/`Object` — so it is the most likely to
    // leak into a type position.
    assert_suggestions("class C { b: typeof arguments = 1 as any; }", &libs, &[]);
}
