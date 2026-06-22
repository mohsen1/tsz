//! Tests for qualified `typeof a.b` member resolution under type operators.
//!
//! ## Structural rule
//!
//! A qualified value type query `typeof a.b` is exactly `(typeof a)["b"]`: the
//! member is resolved by indexing the value-space type of `a`. `tsc` resolves
//! it lazily, so the result is identical whether the query stands alone or is
//! the operand of a `keyof` / mapped / conditional / indexed-access type — and
//! whether or not the value object's members have been materialized at the
//! point the enclosing type is first forced.
//!
//! tsz previously resolved a qualified `typeof a.b` *eagerly* off the resolved
//! value object and, when that eager read had not yet materialized (e.g. a
//! `keyof`/mapped operand forced the query during type-alias evaluation, before
//! the object literal's property types existed), collapsed the query to the
//! internal `error` type. That `error` then leaked into the enclosing operator,
//! surfacing as a false `TS2322` (`keyof error` not assignable to the real key
//! union) or an empty mapped-type key set. The fix lowers a qualified value
//! `typeof a.b` to the deferred indexed access `(typeof a)["b"]`, which resolves
//! the member through the indexed-access machinery regardless of evaluation
//! order — matching the explicit `(typeof a)["b"]` spelling, which was always
//! correct.
//!
//! The rule is keyed on structure, not on identifier spelling: renaming the
//! value binding, its members, or the type aliases must not change the result
//! (anti-hardcoding directive). Each scenario is therefore covered with at least
//! two unrelated naming sets.

use tsz_checker::test_utils::check_source_codes;

/// `keyof typeof obj.member` — the canonical witness. The deferred resolution
/// must produce the member's key union, not `keyof error` (false TS2322).
#[test]
fn keyof_qualified_typeof_member_no_false_ts2322() {
    let codes = check_source_codes(
        r#"
const config = { routes: { home: "/", about: "/about" } };
type RouteKey = keyof typeof config.routes;
const rk: "home" | "about" = "home" as RouteKey;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no false TS2322 for `keyof typeof config.routes`, got: {codes:?}"
    );
}

/// Same structural shape, unrelated names — the decision must not key on the
/// `config`/`routes`/`RouteKey` spelling.
#[test]
fn keyof_qualified_typeof_member_name_agnostic() {
    let codes = check_source_codes(
        r#"
const registry = { handlers: { open: 1, close: 2 } };
type Slot = keyof typeof registry.handlers;
const s: "open" | "close" = "open" as Slot;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no false TS2322 for renamed `keyof typeof registry.handlers`, got: {codes:?}"
    );
}

/// Two levels of member access: `keyof typeof a.b.c`.
#[test]
fn keyof_qualified_typeof_two_level_member() {
    let codes = check_source_codes(
        r#"
const store = { outer: { inner: { leaf: true } } };
type K = keyof typeof store.outer.inner;
const k: "leaf" = "leaf" as K;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no false TS2322 for `keyof typeof store.outer.inner`, got: {codes:?}"
    );
}

/// A `declare const` (no inference) must resolve identically — the eager path's
/// premature-materialization failure was independent of how the value's type
/// was obtained.
#[test]
fn keyof_qualified_typeof_member_on_declared_const() {
    let codes = check_source_codes(
        r#"
declare const settings: { theme: { dark: boolean; light: boolean } };
type ThemeKey = keyof typeof settings.theme;
const tk: "dark" | "light" = "dark" as ThemeKey;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no false TS2322 for `keyof typeof settings.theme` on a declared const, got: {codes:?}"
    );
}

/// The query under a mapped type's key constraint is lowered through the
/// generic type-lowering pass; it must resolve the member set, not produce an
/// empty key space (which previously surfaced as TS2322/TS2353).
#[test]
fn mapped_key_over_qualified_typeof_member() {
    let codes = check_source_codes(
        r#"
const config = { routes: { home: "/", about: "/about" } };
type Flags = { [K in keyof typeof config.routes]: boolean };
const f: Flags = { home: true, about: false };
"#,
    );
    assert!(
        !codes.contains(&2322) && !codes.contains(&2353),
        "no false TS2322/TS2353 for a mapped type over `keyof typeof config.routes`, got: {codes:?}"
    );
}

/// Mapped-key variant with unrelated names.
#[test]
fn mapped_key_over_qualified_typeof_member_name_agnostic() {
    let codes = check_source_codes(
        r#"
const palette = { colors: { primary: 0, secondary: 1 } };
type Shades = { [Tone in keyof typeof palette.colors]: string };
const sh: Shades = { primary: "a", secondary: "b" };
"#,
    );
    assert!(
        !codes.contains(&2322) && !codes.contains(&2353),
        "no false TS2322/TS2353 for renamed mapped type over typeof member, got: {codes:?}"
    );
}

/// The query as a conditional check type must resolve to the member object so
/// the conditional selects the correct branch.
#[test]
fn conditional_over_qualified_typeof_member() {
    let codes = check_source_codes(
        r#"
const config = { routes: { home: "/" } };
type Picked = typeof config.routes extends { home: infer H } ? H : never;
const p: string = "x" as Picked;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no false TS2322 for a conditional over `typeof config.routes`, got: {codes:?}"
    );
}

/// A bare qualified `typeof a.b` standing alone was already correct; guard that
/// it stays correct (the fix must not change the working eager path).
#[test]
fn bare_qualified_typeof_member_still_resolves() {
    let codes = check_source_codes(
        r#"
const config = { routes: { home: "/" } };
type R = typeof config.routes;
const r: { home: string } = null as unknown as R;
"#,
    );
    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "bare `typeof config.routes` still resolves to its object type, got: {codes:?}"
    );
}

/// Indexing the deferred member access by a literal resolves to the property
/// type (the value path the deferred chain is built on).
#[test]
fn indexed_access_of_qualified_typeof_member() {
    let codes = check_source_codes(
        r#"
const config = { routes: { home: "/" } };
type Home = (typeof config.routes)["home"];
const h: string = "x" as Home;
"#,
    );
    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "indexing `(typeof config.routes)[\"home\"]` resolves to the property type, got: {codes:?}"
    );
}
