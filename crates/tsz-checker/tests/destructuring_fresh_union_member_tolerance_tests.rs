//! Destructuring a union binding pattern whose initializer contains a FRESH
//! non-empty object literal member must tolerate a destructured property that
//! is absent from that fresh member: tsc's `getTypeOfDestructuredProperty`
//! contributes implicit `undefined` for the missing property rather than
//! failing the whole lookup with TS2339.
//!
//! Freshness (`FRESH_LITERAL`) is absent on named types, call-return types,
//! and freshness-widened const-bound values, so those still error correctly.
//! Direct `obj.prop` member access uses a different path and is unaffected.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts2339(diagnostics: &[(u32, String)]) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect()
}

// ── FP → clean: fresh non-empty member lacking the property is tolerated ──

#[test]
fn min_repro_fresh_member_missing_prop_is_clean() {
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; b?: boolean; }
declare const cond: boolean;
function f(options: Options) {
  const { a, b } = cond ? { a: "x" } : options;
  return [a, b];
}
"#,
    );
    assert!(
        ts2339(&diagnostics).is_empty(),
        "fresh object literal member lacking `b` should contribute implicit undefined, not TS2339: {diagnostics:#?}"
    );
}

#[test]
fn no_defaults_bare_destructure_fresh_member_is_clean() {
    // Renamed binder names to vary the binder identity.
    let diagnostics = check_strict(
        r#"
interface Settings { first?: string; second?: boolean; }
declare const flag: boolean;
function pick(opts: Settings) {
  const { first, second } = flag ? { first: "x" } : opts;
  return [first, second];
}
"#,
    );
    assert!(
        ts2339(&diagnostics).is_empty(),
        "bare destructure (no defaults) from a fresh member should be clean: {diagnostics:#?}"
    );
}

#[test]
fn three_member_titlecase_shape_renamed_binders_is_clean() {
    // Mirrors the change-case title-case witness shape with three optional
    // properties and renamed binders.
    let diagnostics = check_strict(
        r#"
interface CaseOptions {
  splitRegexp?: string;
  stripRegexp?: string;
  delimiter?: string;
}
declare const useFresh: boolean;
function titleCase(input: string, options: CaseOptions) {
  const { splitRegexp, stripRegexp, delimiter } =
    useFresh ? { splitRegexp: "(?!^)" } : options;
  return [input, splitRegexp, stripRegexp, delimiter];
}
"#,
    );
    assert!(
        ts2339(&diagnostics).is_empty(),
        "three-member fresh union destructure should be clean: {diagnostics:#?}"
    );
}

#[test]
fn fresh_arm_vs_named_type_optional_prop_is_clean() {
    // `cond ? {a:"x"} : Options` where the missing prop `b` is optional.
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; b?: boolean; }
declare const cond: boolean;
declare const opts: Options;
const { a, b } = cond ? { a: "x" } : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&diagnostics).is_empty(),
        "fresh arm vs named type with optional missing prop should be clean: {diagnostics:#?}"
    );
}

#[test]
fn fresh_arm_vs_named_type_required_prop_is_clean() {
    // `cond ? {a:"x"} : Options` where the missing prop `b` is required on the
    // named arm. tsc still does not emit TS2339 here: the fresh member supplies
    // implicit undefined for `b`; the union property resolution succeeds.
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; b: boolean; }
declare const cond: boolean;
declare const opts: Options;
const { a, b } = cond ? { a: "x" } : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&diagnostics).is_empty(),
        "fresh arm vs named type with required missing prop should be clean (TS2339-free): {diagnostics:#?}"
    );
}

#[test]
fn case_g_fresh_arm_includes_missing_prop_is_clean() {
    // Case G: the fresh arm already includes `b`, so no member lacks it.
    // This was already clean before the fix; assert it stays clean.
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; b?: boolean; }
declare const cond: boolean;
declare const opts: Options;
const { a, b } = cond ? { a: "x", b: true } : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&diagnostics).is_empty(),
        "fresh arm including the property should be clean: {diagnostics:#?}"
    );
}

// ── Negative controls: freshness absent → TS2339 must still fire ──

#[test]
fn named_type_arm_missing_prop_still_errors() {
    // The missing-prop arm is a NAMED type (not fresh): `b` is absent from
    // `Partial`, so destructuring `b` must still error (TS2339), matching tsc.
    let diagnostics = check_strict(
        r#"
interface OnlyA { a?: string; }
interface Options { a?: string; b?: boolean; }
declare const cond: boolean;
declare const onlyA: OnlyA;
declare const opts: Options;
const { a, b } = cond ? onlyA : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&diagnostics).iter().any(|m| m.contains("b")),
        "named (non-fresh) member missing `b` should still emit TS2339 on `b`: {diagnostics:#?}"
    );
}

#[test]
fn call_return_arm_missing_prop_still_errors() {
    // The fresh-looking arm is a CALL RETURN type — freshness is lost across
    // the call boundary, so the missing prop must still error.
    let diagnostics = check_strict(
        r#"
interface OnlyA { a?: string; }
interface Options { a?: string; b?: boolean; }
declare function makeOnlyA(): OnlyA;
declare const cond: boolean;
declare const opts: Options;
const { a, b } = cond ? makeOnlyA() : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&diagnostics).iter().any(|m| m.contains("b")),
        "call-return (non-fresh) member missing `b` should still emit TS2339 on `b`: {diagnostics:#?}"
    );
}

#[test]
fn const_bound_then_destructured_loses_freshness_still_errors() {
    // Binding the fresh literal to `const src` FIRST widens away freshness, so
    // destructuring `b` from the union must still error.
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; b?: boolean; }
declare const cond: boolean;
declare const opts: Options;
const src = { a: "x" };
const { a, b } = cond ? src : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&diagnostics).iter().any(|m| m.contains("b")),
        "const-bound (freshness-lost) member missing `b` should still emit TS2339 on `b`: {diagnostics:#?}"
    );
}

#[test]
fn prop_absent_everywhere_still_errors() {
    // A property absent from EVERY member must still error even when one
    // member is fresh, because no member supplies it.
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; }
declare const cond: boolean;
declare const opts: Options;
const { a, missing } = cond ? { a: "x" } : opts;
const sink = [a, missing];
"#,
    );
    assert!(
        ts2339(&diagnostics).iter().any(|m| m.contains("missing")),
        "a property absent from every member should still emit TS2339: {diagnostics:#?}"
    );
}

#[test]
fn direct_member_access_required_prop_still_errors() {
    // Direct `obj.b` member access uses a different code path that does NOT
    // share the destructure fresh-member tolerance. With `b` REQUIRED on the
    // named arm, the fresh `{ a: "x" }` arm genuinely lacks `b`, so direct
    // access must still emit TS2339 — matching tsc (verified: tsc reports
    // TS2339 on `obj.b` here).
    let diagnostics = check_strict(
        r#"
interface Options { a?: string; b: boolean; }
declare const cond: boolean;
declare const opts: Options;
const obj = cond ? { a: "x" } : opts;
const value = obj.b;
"#,
    );
    assert!(
        ts2339(&diagnostics).iter().any(|m| m.contains("b")),
        "direct member access of a required prop absent from a fresh union member should still emit TS2339: {diagnostics:#?}"
    );
}

#[test]
fn destructure_vs_direct_access_diverge_on_fresh_member() {
    // Parity contrast verified against tsc 5.7 with `b` REQUIRED on the named
    // arm, so the fresh `{ a: "x" }` arm genuinely lacks `b`:
    //   - destructuring `const { a, b } = ...` is CLEAN (this fix), because the
    //     destructure path contributes implicit undefined for the fresh member;
    //   - direct access `obj.b` still errors with TS2339, because the member-
    //     access path has no such tolerance.
    // This proves the tolerance is local to the destructure loop.
    let destructure = check_strict(
        r#"
interface Options { a?: string; b: boolean; }
declare const cond: boolean;
declare const opts: Options;
const { a, b } = cond ? { a: "x" } : opts;
const sink = [a, b];
"#,
    );
    assert!(
        ts2339(&destructure).is_empty(),
        "destructuring a fresh member lacking a required prop should be clean: {destructure:#?}"
    );

    let direct = check_strict(
        r#"
interface Options { a?: string; b: boolean; }
declare const cond: boolean;
declare const opts: Options;
const obj = cond ? { a: "x" } : opts;
const value = obj.b;
"#,
    );
    assert!(
        ts2339(&direct).iter().any(|m| m.contains("b")),
        "direct access of a required prop absent from a fresh member should still error: {direct:#?}"
    );
}
