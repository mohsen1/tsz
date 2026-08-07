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
