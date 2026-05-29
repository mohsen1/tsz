//! Regression coverage for issue #10459 / the tsc conformance test
//! `unionPropertyOfProtectedAndIntersectionProperty.ts`.
//!
//! Rule: a union exposes a protected/private member only when every constituent
//! shares a *common declaration* of it. An intersection constituent contributes
//! the declarations of all of its parts, so `(Foo | (Foo & Bar))["foo"]` is OK
//! (the intersection arm carries `Foo`'s protected `foo`, shared with the plain
//! `Foo` arm), while `(Foo | Bar)["foo"]` still errors because the two protected
//! declarations are unrelated.
//!
//! The comparison is keyed on declaring-class identity, never on class /
//! property / type-parameter names, so the tests vary all of those.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2339_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(code, _)| *code == 2339).count()
}

#[test]
fn union_with_intersection_arm_sharing_protected_declaration_is_accessible() {
    let diags = check_source_code_messages(
        r#"
class Foo { protected foo = 0; }
class Bar { protected foo = 0; }
type Ok = (Foo | (Foo & Bar))["foo"];
"#,
    );
    assert_eq!(
        ts2339_count(&diags),
        0,
        "an intersection arm sharing a protected declaration must expose the member: {diags:?}"
    );
}

#[test]
fn union_of_unrelated_protected_declarations_still_errors() {
    let diags = check_source_code_messages(
        r#"
class Foo { protected foo = 0; }
class Bar { protected foo = 0; }
type Bad = (Foo | Bar)["foo"];
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("Foo | Bar")),
        "unrelated protected declarations on a union must still report TS2339: {diags:?}"
    );
}

#[test]
fn intersection_protected_rule_is_independent_of_names() {
    // Same shapes as the repro but with renamed classes and a renamed property,
    // proving the fix keys on declaration identity rather than spellings.
    let diags = check_source_code_messages(
        r#"
class Alpha { protected slot = ""; }
class Beta { protected slot = ""; }
type Ok = (Alpha | (Alpha & Beta))["slot"];
type Bad = (Alpha | Beta)["slot"];
"#,
    );
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "exactly the unrelated union should report TS2339: {diags:?}"
    );
    assert!(
        ts2339[0].1.contains("Alpha | Beta"),
        "the single TS2339 must be for the unrelated `Alpha | Beta` union: {diags:?}"
    );
}

#[test]
fn union_with_intersection_arm_sharing_inherited_protected_declaration_is_accessible() {
    // `Sub` inherits `value` from `Base`; the `(Base & Mix)` arm also carries
    // `Base`'s declaration, so the union shares a common declaration.
    let diags = check_source_code_messages(
        r#"
class Base { protected value = 0; }
class Mix { protected value = 0; }
class Sub extends Base {}
type Ok = (Sub | (Base & Mix))["value"];
"#,
    );
    assert_eq!(
        ts2339_count(&diags),
        0,
        "an inherited shared protected declaration must expose the member: {diags:?}"
    );
}

#[test]
fn union_three_members_unrelated_protected_arm_breaks_sharing() {
    // The `(Foo & Bar)` arm shares with `Foo`, but the third arm `Baz` declares
    // its own unrelated protected `foo`, so the union no longer shares a common
    // declaration.
    let diags = check_source_code_messages(
        r#"
class Foo { protected foo = 0; }
class Bar { protected foo = 0; }
class Baz { protected foo = 0; }
type Bad = (Foo | (Foo & Bar) | Baz)["foo"];
"#,
    );
    assert!(
        ts2339_count(&diags) >= 1,
        "a third unrelated protected arm must break the shared declaration: {diags:?}"
    );
}

#[test]
fn union_of_public_properties_from_different_classes_is_accessible() {
    // Public members from different classes were never gated by this rule;
    // guard against the fix over-reaching into public unions.
    let diags = check_source_code_messages(
        r#"
class A { x = 0; }
class B { x = ""; }
type Ok = (A | B)["x"];
"#,
    );
    assert_eq!(
        ts2339_count(&diags),
        0,
        "public properties on a union must remain accessible: {diags:?}"
    );
}
