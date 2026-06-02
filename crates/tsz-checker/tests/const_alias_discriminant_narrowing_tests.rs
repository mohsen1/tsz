//! Tests for discriminant narrowing through const-aliased object references.
//!
//! Structural rule: When `const alias = target` (no type annotation), accessing
//! `alias.prop` in a discriminant condition `alias.prop === value` narrows
//! `target` by the same discriminant path as `target.prop === value` would.
//!
//! tsc follows const aliases in `isMatchingReference` so that narrowing of
//! `target` applies to both `target.prop === v` and `alias.prop === v` when
//! `const alias = target`.  tsz extends `relative_discriminant_path` to
//! resolve the const alias to its initializer before reporting failure.

use tsz_checker::test_utils::check_source_strict_codes;

fn assert_no_ts2339(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2339),
        "expected no TS2339 via const-alias discriminant narrowing, got {codes:?}\nsource:\n{source}"
    );
}

fn assert_has_ts2339(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2339),
        "expected TS2339 (alias not usable here), got {codes:?}\nsource:\n{source}"
    );
}

/// Primary repro: `const alias = target; alias.prop === "a"` narrows `target`.
///
/// Adjacent case: different discriminant member and property names, confirming
/// the rule does not rely on a specific spelling.
#[test]
fn const_alias_discriminant_narrows_original_target() {
    assert_no_ts2339(
        r#"
type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; side: number };
function f(shape: Circle | Square) {
    const s = shape;
    if (s.kind === "circle") {
        const r: number = shape.radius;
    }
}
"#,
    );
}

/// Rename invariant: different identifier names for alias and target, with a
/// different discriminant property name, confirming the rule is structural.
#[test]
fn const_alias_discriminant_narrows_original_rename_invariant() {
    assert_no_ts2339(
        r#"
type Left = { tag: "left"; payload: number };
type Right = { tag: "right"; payload: string };
function g(item: Left | Right) {
    const ref_ = item;
    if (ref_.tag === "left") {
        const n: number = item.payload;
    }
}
"#,
    );
}

/// Three-member union: const alias discriminant narrows to exactly the matching
/// member, not just to a two-member remainder.
#[test]
fn const_alias_discriminant_three_member_union_narrows_correctly() {
    assert_no_ts2339(
        r#"
type A = { kind: "a"; a: number };
type B = { kind: "b"; b: string };
type C = { kind: "c"; c: boolean };
function h(x: A | B | C) {
    const alias = x;
    if (alias.kind === "a") {
        const n: number = x.a;
    }
    if (alias.kind === "b") {
        const s: string = x.b;
    }
    if (alias.kind === "c") {
        const b: boolean = x.c;
    }
}
"#,
    );
}

/// False-branch exhaustion: in the `else` branch the matching member is
/// excluded, so only the remaining members are valid.
#[test]
fn const_alias_discriminant_false_branch_excludes_member() {
    assert_no_ts2339(
        r#"
type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; side: number };
function f(shape: Circle | Square) {
    const s = shape;
    if (s.kind === "circle") {
        const _r: number = shape.radius;
    } else {
        const _side: number = shape.side;
    }
}
"#,
    );
}

/// Negative: a `let` (mutable) alias should not project const-alias
/// discriminant narrowing onto the original target.  `let alias = target` does
/// NOT create a const-alias link because the binder treats `let` as mutable.
#[test]
fn let_alias_does_not_project_discriminant_narrowing() {
    assert_has_ts2339(
        r#"
type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; side: number };
function f(shape: Circle | Square) {
    let s = shape;
    if (s.kind === "circle") {
        // shape is not narrowed through a mutable alias
        const r: number = shape.radius;
    }
}
"#,
    );
}

/// Negative: an annotated const alias (`const alias: typeof target = target`)
/// should not project narrowing because tsc gates alias-based narrowing on
/// the absence of a type annotation (the annotation widens and breaks the
/// discriminant link).
#[test]
fn annotated_const_alias_does_not_project_discriminant_narrowing() {
    assert_has_ts2339(
        r#"
type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; side: number };
type Shape = Circle | Square;
function f(shape: Shape) {
    const s: Shape = shape;
    if (s.kind === "circle") {
        // type annotation on alias breaks the link
        const r: number = shape.radius;
    }
}
"#,
    );
}
