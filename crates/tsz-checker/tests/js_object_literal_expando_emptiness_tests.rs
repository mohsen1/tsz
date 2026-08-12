//! A JS variable initialized with a NON-EMPTY object literal is a closed
//! shape, not an expando host: a later write to an undeclared member is an
//! ordinary property assignment, reported `TS2339` under `noImplicitAny`.
//!
//! Closes #17226. The structural rule (tsc `getExpandoInitializer`):
//!
//! > A `var X = {…}` object-literal initializer hosts expando members only when
//! > the literal is EMPTY (`properties.length === 0`); a prototype assignment
//! > (`X.prototype = {…}`) relaxes this. A non-empty literal keeps its closed
//! > inferred shape, so `X.newMember = …` / `X.newMember` is `TS2339` under
//! > `noImplicitAny`. Function and class expression initializers remain expando
//! > hosts regardless of content.
//!
//! `tsz` accepted ANY object-literal initializer as an expando host at three
//! sites — the checker read/write predicates
//! (`root_symbol_supports_js_expando_read`,
//! `root_symbol_supports_js_direct_expando_write`) and the binder registration
//! (`is_expando_init`) — so the write was silently accepted even under
//! `noImplicitAny`, a false negative. Each site now gates the object-literal
//! branch on `arena.is_empty_object_literal(...)`.
//!
//! The `noImplicitAny`-OFF open-container leniency
//! (`js_open_object_receiver_under_implicit_any`) is unchanged: it lives at the
//! `TS2339` emission site and keeps the same write silent when `noImplicitAny`
//! is off, matching tsc. The matrix below flips only `no_implicit_any` between
//! otherwise-identical runs (the tsconfig-sentinel oracle method).

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
// The bug: non-empty object-literal host, undeclared-member WRITE.
// ===========================================================================

/// noImplicitAny ON: `var o = { zig: 1 }; o.zag = 2;` — the write to the
/// undeclared `zag` is a real property assignment on the closed shape
/// `{ zig: number }`, so tsc reports TS2339. Previously silent (the false
/// negative this issue tracks).
#[test]
fn non_empty_literal_write_reports_ts2339_under_no_implicit_any() {
    let codes = codes_no_implicit_any_on("var o = { zig: 1 };\no.zag = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "o.zag = 2 on a closed `{{ zig: number }}` shape must be TS2339, got {codes:?}"
    );
}

/// noImplicitAny OFF: the SAME write stays silent — the open-container
/// leniency types the access as implicit `any`. This must not regress.
#[test]
fn non_empty_literal_write_silent_without_no_implicit_any() {
    let codes = codes_no_implicit_any_off("var o = { zig: 1 };\no.zag = 2;\n");
    assert!(
        codes.is_empty(),
        "o.zag = 2 without noImplicitAny is an open-container implicit any, got {codes:?}"
    );
}

/// The READ side mirrors the write side: `var o = { zig: 1 }; o.zag;` is
/// TS2339 under noImplicitAny, silent without it.
#[test]
fn non_empty_literal_read_reports_ts2339_under_no_implicit_any() {
    let codes = codes_no_implicit_any_on("var o = { zig: 1 };\no.zag;\n");
    assert_eq!(
        codes,
        vec![2339],
        "reading undeclared o.zag on a closed shape must be TS2339, got {codes:?}"
    );
}

#[test]
fn non_empty_literal_read_silent_without_no_implicit_any() {
    let codes = codes_no_implicit_any_off("var o = { zig: 1 };\no.zag;\n");
    assert!(
        codes.is_empty(),
        "reading o.zag without noImplicitAny is an open-container implicit any, got {codes:?}"
    );
}

/// A declared member of the non-empty literal reads cleanly in both configs —
/// the closed shape still carries its own members.
#[test]
fn non_empty_literal_declared_member_reads_clean() {
    let src = "var o = { zig: 1 };\nlet n = o.zig;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "o.zig is a declared member, must be clean under noImplicitAny"
    );
    assert!(
        codes_no_implicit_any_off(src).is_empty(),
        "o.zig is a declared member, must be clean without noImplicitAny"
    );
}

// ===========================================================================
// Control cases that must keep working — EMPTY-literal / function / class hosts.
// ===========================================================================

/// Empty literal host: `var p = {}; p.zag = 2;` is a genuine expando
/// declaration — silent in both configs.
#[test]
fn empty_literal_write_is_expando_declaration_silent_both_configs() {
    let src = "var p = {};\np.zag = 2;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "empty-literal expando write must be silent under noImplicitAny"
    );
    assert!(
        codes_no_implicit_any_off(src).is_empty(),
        "empty-literal expando write must be silent without noImplicitAny"
    );
}

/// An empty-literal host reads its declared expando member back cleanly.
#[test]
fn empty_literal_write_then_read_resolves_member() {
    let src = "var p = {};\np.zag = 2;\nlet n = p.zag;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "p.zag was declared by the expando write, must read clean, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

/// Nested empty-literal expando writes stay silent (`var N = {}; N.commands =
/// {}; N.commands.a = 1;`).
#[test]
fn nested_empty_literal_expando_writes_silent() {
    let src = "var N = {};\nN.commands = {};\nN.commands.a = 1;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "nested empty-literal expando writes must stay silent, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

/// Function host: `function f() {} f.x = 1;` remains an expando declaration
/// regardless of noImplicitAny — a function initializer is always a host.
#[test]
fn function_host_expando_write_silent() {
    let src = "function f() {}\nf.x = 1;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "function-host expando write must stay silent, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

/// A variable initialized with a function expression stays an expando host
/// regardless of its (irrelevant) content — the emptiness rule is
/// object-literal-only.
#[test]
fn function_expression_host_expando_write_silent() {
    let src = "var g = function () {};\ng.x = 1;\n";
    assert!(
        codes_no_implicit_any_on(src).is_empty(),
        "function-expression-host expando write must stay silent, got {:?}",
        codes_no_implicit_any_on(src)
    );
}

/// An empty-literal host with a NEVER-declared member still reports TS2339 on
/// read under noImplicitAny: the host is open, but only assignment-declared
/// members exist on it.
#[test]
fn empty_literal_undeclared_read_reports_ts2339_under_no_implicit_any() {
    let codes = codes_no_implicit_any_on("var q = {};\nq.nope;\n");
    assert_eq!(
        codes,
        vec![2339],
        "q.nope was never declared, must be TS2339 under noImplicitAny, got {codes:?}"
    );
}

// ===========================================================================
// Anti-hardcoding: the rule is structural, not keyed to identifiers.
// ===========================================================================

/// Renamed binders behave identically — no name-based fast path. A non-empty
/// literal under a different variable/property name still reports TS2339 on the
/// undeclared write.
#[test]
fn non_empty_literal_write_ts2339_renamed_binders() {
    let codes = codes_no_implicit_any_on("var zqz = { qbq: 1 };\nzqz.wxw = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "renamed non-empty-literal host must still report TS2339, got {codes:?}"
    );
}

/// A non-empty literal with a computed/string-keyed member is still a closed
/// shape — the undeclared write is TS2339 under noImplicitAny.
#[test]
fn non_empty_string_keyed_literal_write_ts2339() {
    let codes = codes_no_implicit_any_on("var o = { \"zig\": 1 };\no.zag = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "string-keyed non-empty literal is still closed, got {codes:?}"
    );
}

/// A spread-only literal (`{ ...src }`) has `properties.length === 1`, so it is
/// NOT empty and NOT an expando host: the undeclared write is TS2339 under
/// noImplicitAny.
#[test]
fn spread_only_literal_is_not_empty_host() {
    let codes = codes_no_implicit_any_on("var src = { a: 1 };\nvar o = { ...src };\no.zag = 2;\n");
    assert_eq!(
        codes,
        vec![2339],
        "spread-only literal has one element, so it is not an empty host, got {codes:?}"
    );
}
