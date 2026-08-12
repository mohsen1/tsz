//! Member-level expando-host emptiness for nested JS chains (#17226 gap 1).
//!
//! The structural rule (tsc `getExpandoInitializer`, applied at MEMBER depth):
//!
//! > A member assignment `host.sub = rhs` on an expando host declares `sub`,
//! > but `sub` itself hosts further expando members only when `rhs` is an
//! > expando initializer — an EMPTY object literal, a function/arrow
//! > expression, or a class expression. A closed RHS (`host.sub = { a: 1 }`,
//! > or an identifier) keeps `sub` at its inferred shape, so a later
//! > `host.sub.b = …` / `host.sub.b` is `TS2339` under `noImplicitAny` and
//! > silent (open-container implicit any) without it.
//!
//! `tsz`'s nested-chain gates only required the base link to be a RECORDED
//! member of its parent, not a HOST-qualified one — the binder's
//! `detect_expando_assignment` nested gates and the checker's
//! `nested_expando_base_link_is_declared` fallback both accepted any recorded
//! base, so `M.sub.b` was silently declared even under `noImplicitAny`. The
//! binder now tracks host-qualified members (`expando_host_members`,
//! bind-time-only) and the checker's fallback scan validates the base link's
//! declaring RHS shape.
//!
//! The matrix below flips only `no_implicit_any` between otherwise-identical
//! runs (the tsconfig-sentinel oracle method, typescript@7.0.2).

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_js_source_codes_with_options;

fn codes_no_implicit_any_on(source: &str) -> Vec<u32> {
    check_js_source_codes_with_options(
        source,
        "test.js",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes_no_implicit_any_off(source: &str) -> Vec<u32> {
    check_js_source_codes_with_options(
        source,
        "test.js",
        CheckerOptions {
            no_implicit_any: false,
            ..CheckerOptions::default()
        },
    )
}

// ===========================================================================
// The bug: non-empty-literal MEMBER RHS, nested undeclared write/read.
// ===========================================================================

/// noImplicitAny ON: `M.sub = { a: 1 }` declares `sub` as the closed shape
/// `{ a: number }`, so the nested `M.sub.b = 2` is TS2339. Previously silent
/// (the false negative gap 1 tracks).
#[test]
fn non_empty_member_rhs_nested_write_reports_ts2339_under_no_implicit_any() {
    let codes = codes_no_implicit_any_on("var M = {};\nM.sub = { a: 1 };\nM.sub.b = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "M.sub.b = 2 through the closed `{{ a: number }}` member must be TS2339, got {codes:?}"
    );
}

/// noImplicitAny OFF: the SAME nested write stays silent — the open-container
/// leniency types the access as implicit `any`. Must not regress.
#[test]
fn non_empty_member_rhs_nested_write_silent_without_no_implicit_any() {
    let codes = codes_no_implicit_any_off("var M = {};\nM.sub = { a: 1 };\nM.sub.b = 2;\n");
    assert!(
        codes.is_empty(),
        "M.sub.b = 2 without noImplicitAny is an open-container implicit any, got {codes:?}"
    );
}

/// The READ side mirrors the write side: an undeclared nested member read
/// through a closed base is TS2339 under noImplicitAny, silent without it.
#[test]
fn non_empty_member_rhs_nested_read_reports_ts2339_under_no_implicit_any() {
    let codes = codes_no_implicit_any_on("var M = {};\nM.sub = { a: 1 };\nvar r = M.sub.b;\n");
    assert_eq!(
        codes,
        vec![2339],
        "reading undeclared M.sub.b on a closed member shape must be TS2339, got {codes:?}"
    );
}

#[test]
fn non_empty_member_rhs_nested_read_silent_without_no_implicit_any() {
    let codes = codes_no_implicit_any_off("var M = {};\nM.sub = { a: 1 };\nvar r = M.sub.b;\n");
    assert!(
        codes.is_empty(),
        "reading M.sub.b without noImplicitAny is an open-container implicit any, got {codes:?}"
    );
}

/// A DECLARED member of the closed base still reads cleanly in both configs.
#[test]
fn non_empty_member_rhs_declared_member_reads_clean() {
    let src = "var M = {};\nM.sub = { a: 1 };\nvar r = M.sub.a;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "M.sub.a is a declared member of the literal, must be clean under noImplicitAny, got {:?}",
        codes_no_implicit_any_on(src)
    );
    assert!(
        codes_no_implicit_any_off(src).is_empty(),
        "M.sub.a must be clean without noImplicitAny"
    );
}

/// An identifier RHS is not an expando initializer either, even when the
/// referenced variable is itself an empty-literal host (oracle-verified):
/// `W.sub = t` keeps `sub` closed, so `W.sub.b = 2` is TS2339 under
/// noImplicitAny.
#[test]
fn identifier_member_rhs_is_not_a_host() {
    let codes = codes_no_implicit_any_on("var t = {};\nvar W = {};\nW.sub = t;\nW.sub.b = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "an identifier RHS does not host nested expandos, got {codes:?}"
    );
}

/// Deeper chains apply the rule at every link: `D.a = {}` hosts, but
/// `D.a.b = { c: 1 }` is closed, so `D.a.b.d = 2` is TS2339 under
/// noImplicitAny and silent without it.
#[test]
fn deeper_chain_closed_link_reports_ts2339_under_no_implicit_any() {
    let src = "var D = {};\nD.a = {};\nD.a.b = { c: 1 };\nD.a.b.d = 2;\n";
    let codes = codes_no_implicit_any_on(src);
    assert_eq!(
        codes,
        vec![2339],
        "D.a.b.d = 2 through the closed `{{ c: number }}` link must be TS2339, got {codes:?}"
    );
    assert!(
        codes_no_implicit_any_off(src).is_empty(),
        "the same deep write must stay silent without noImplicitAny"
    );
}

/// The rule also applies under a FUNCTION root: `fq.ns = { a: 1 }` declares a
/// closed member on the function host, so `fq.ns.x = 1` is TS2339 under
/// noImplicitAny (oracle-verified) and silent without it.
#[test]
fn function_root_non_empty_member_rhs_nested_write_ts2339() {
    let src = "function fq() {}\nfq.ns = { a: 1 };\nfq.ns.x = 1;\n";
    let codes = codes_no_implicit_any_on(src);
    assert_eq!(
        codes,
        vec![2339],
        "fq.ns.x = 1 through the closed function-root member must be TS2339, got {codes:?}"
    );
    assert!(
        codes_no_implicit_any_off(src).is_empty(),
        "the same function-root nested write must stay silent without noImplicitAny"
    );
}

// ===========================================================================
// Controls: host-shaped member RHS keeps nesting open in both configs.
// ===========================================================================

/// Empty-literal member RHS stays a host: `N.commands = {}` then
/// `N.commands.a = 1` is a genuine nested expando declaration — silent in
/// both configs, and the member reads back.
#[test]
fn empty_literal_member_rhs_hosts_nested_writes() {
    let src = "var N = {};\nN.commands = {};\nN.commands.a = 1;\nvar r = N.commands.a;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "nested empty-literal expando writes must stay silent under noImplicitAny, got {:?}",
        codes_no_implicit_any_on(src)
    );
    assert!(
        codes_no_implicit_any_off(src).is_empty(),
        "nested empty-literal expando writes must stay silent without noImplicitAny"
    );
}

/// Function-expression member RHS stays a host regardless of noImplicitAny.
#[test]
fn function_member_rhs_hosts_nested_writes() {
    let src = "var P = {};\nP.f = function () {};\nP.f.x = 1;\nvar r = P.f.x;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "a function-valued member hosts nested expandos, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

/// Class-expression member RHS stays a host (static-side expando).
#[test]
fn class_member_rhs_hosts_nested_writes() {
    let src = "var Q = {};\nQ.k = class {};\nQ.k.x = 1;\nvar r = Q.k.x;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "a class-valued member hosts nested static expandos, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

/// Function-root sibling control: `fr.ns = {}` (empty literal) keeps the
/// member open — `fr.ns.x = 1` stays silent in both configs.
#[test]
fn function_root_empty_member_rhs_hosts_nested_writes() {
    let src = "function fr() {}\nfr.ns = {};\nfr.ns.x = 1;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "fr.ns = {{}} is a host member, nested write must be silent, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

// ===========================================================================
// Mixed declaring writes: one closed-shape write closes the member for good,
// in either order (oracle-verified against typescript@7.0.2).
// ===========================================================================

/// Non-host write then host write: still closed — `M.sub.b = 2` is TS2339
/// under noImplicitAny.
#[test]
fn mixed_writes_non_host_then_host_member_stays_closed() {
    let codes =
        codes_no_implicit_any_on("var M = {};\nM.sub = { a: 1 };\nM.sub = {};\nM.sub.b = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "a closed-shape declaring write closes the member even before a host write, got {codes:?}"
    );
}

/// Host write then non-host write: also closed.
#[test]
fn mixed_writes_host_then_non_host_member_stays_closed() {
    let codes =
        codes_no_implicit_any_on("var K = {};\nK.sub = {};\nK.sub = { a: 1 };\nK.sub.c = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "a closed-shape declaring write closes the member even after a host write, got {codes:?}"
    );
}

// ===========================================================================
// Anti-hardcoding: the rule is structural, not keyed to identifiers.
// ===========================================================================

/// Renamed binders behave identically — no name-based fast path.
#[test]
fn non_empty_member_rhs_nested_write_ts2339_renamed_binders() {
    let codes =
        codes_no_implicit_any_on("var zqz = {};\nzqz.qbq = { wxw: 1 };\nzqz.qbq.vjv = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "renamed closed member link must still report TS2339, got {codes:?}"
    );
}
