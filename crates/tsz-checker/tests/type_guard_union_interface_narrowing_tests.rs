//! Native narrowing coverage for the `guard(x) && x.member` pattern over a
//! union of interface types.
//!
//! Rule under test:
//!
//! > When a user-defined type guard `g(x): x is T` gates the right operand of
//! > `&&`, the union-typed operand narrows to `T` in that operand, so a member
//! > access that only exists on `T` is valid, and the `&&` expression's type is
//! > `false | <member type>`.
//!
//! This behavior used to be faked for one TypeScript conformance fixture
//! (`typeGuardOfFormIsTypeOnInterfaces`) by a source-text-gated diagnostic
//! rewrite (`align_type_guard_interface_diagnostics`) in
//! `state_checking/source_file.rs`, matching hardcoded identifier strings
//! (`isC2`, `c1Orc2`, `C1 | C2`). The solver now narrows this pattern
//! natively, so that rewrite was dead and has been removed (see #14141). These
//! tests pin the *structural* rule with distinct binder spellings so no future
//! regression can silently reintroduce the need for a fixture-scoped hack.

use tsz_checker::test_utils::{
    check_source_strict_messages as check_strict, has_any_diagnostic_code,
    has_diagnostic_code_message,
};

/// A "property does not exist" error (TS2339, or TS7053 implicit-any index)
/// on the guarded member would mean narrowing never reached it.
fn assert_narrowing_reached_member(diags: &[(u32, String)]) {
    assert!(
        !has_any_diagnostic_code(diags, &[2339, 7053]),
        "narrowing should reach the guarded member; got: {diags:?}"
    );
}

/// The guarded member access is valid and the `&&` result (`false | number`)
/// is rejected against a `number` annotation.
fn assert_false_number_mismatch(diags: &[(u32, String)]) {
    assert_narrowing_reached_member(diags);
    assert!(
        diags.iter().any(|(code, msg)| *code == 2322
            && msg.contains("'number | false'")
            && msg.contains("'number'")),
        "expected TS2322 for `number | false` vs `number`; got: {diags:?}"
    );
}

/// Two-member union, guard narrows to the member carrying the accessed
/// property. Binders deliberately differ from the conformance fixture.
#[test]
fn guard_and_member_narrows_two_member_union_and_reports_false_union_mismatch() {
    let source = r#"
interface Alpha { a: string; }
interface Beta { b: number; }
declare function isBeta(x: unknown): x is Beta;
function f(ab: Alpha | Beta) {
    const n: number = isBeta(ab) && ab.b;
    return n;
}
"#;
    assert_false_number_mismatch(&check_strict(source));
}

/// Same rule where the guard target extends another union member; the unique
/// property lives on the extending interface.
#[test]
fn guard_and_member_narrows_to_extending_interface_member() {
    let source = r#"
interface One { p1: string; }
interface Two { p2: number; }
interface Three extends One { p3: number; }
declare function isThree(x: unknown): x is Three;
function g(u: Two | Three) {
    const m: number = isThree(u) && u.p3;
    return m;
}
"#;
    assert_false_number_mismatch(&check_strict(source));
}

/// Pure narrowing: when the `&&` result is not constrained by an annotation,
/// the guarded member access alone must be clean.
#[test]
fn guard_and_member_access_alone_is_clean() {
    let source = r#"
interface Dog { bark(): void; }
interface Cat { meow(): void; }
declare function isCat(x: unknown): x is Cat;
function h(pet: Dog | Cat) {
    const ok = isCat(pet) && pet.meow;
    return ok;
}
"#;
    assert_narrowing_reached_member(&check_strict(source));
}

/// The pre-narrowing member access (outside the guard) still fails, proving the
/// narrowing is scoped to the `&&` right operand and not a blanket suppression.
#[test]
fn member_access_without_guard_still_errors() {
    let source = r#"
interface Shape { kind: string; }
interface Circle { kind: string; radius: number; }
function area(s: Shape | Circle) {
    return s.radius;
}
"#;
    assert!(
        has_diagnostic_code_message(&check_strict(source), 2339, "radius"),
        "unguarded access to a non-common member must still error"
    );
}
