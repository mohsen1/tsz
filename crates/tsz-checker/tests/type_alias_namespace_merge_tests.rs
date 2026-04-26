//! Tests for type alias + namespace declaration merge resolution.
//!
//! When a type alias and namespace share the same name (merged declaration),
//! the type alias body must be resolved correctly in type position.
//! Regression: the namespace path ran first, returning Lazy(DefId) that
//! self-referenced, preventing the alias from ever resolving.

fn get_error_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source_codes(source)
}

#[allow(dead_code)]
fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

#[test]
fn test_type_alias_namespace_merge_basic() {
    // type Foo = Foo.A | Foo.B merged with namespace Foo should resolve
    // Foo to `number | string` in type position
    let codes = get_error_codes(
        r#"
export type Foo = Foo.A | Foo.B;
export namespace Foo {
    export type A = number;
    export type B = string;
}
const x: Foo = 42;
const y: Foo = "hello";
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322: number and string are assignable to Foo = number | string. Got codes: {codes:?}"
    );
}

#[test]
fn test_type_alias_namespace_merge_rejects_wrong_type() {
    // Foo = number | string should reject boolean
    let codes = get_error_codes(
        r#"
export type Foo = Foo.A | Foo.B;
export namespace Foo {
    export type A = number;
    export type B = string;
}
const x: Foo = true;
"#,
    );
    assert!(
        codes.contains(&2322),
        "Should emit TS2322: boolean is not assignable to number | string. Got: {codes:?}"
    );
}

#[test]
fn test_type_alias_namespace_merge_used_as_constraint() {
    // Type alias merged with namespace used as a generic constraint
    let codes = get_error_codes(
        r#"
export type ElChildren = ElChildren.Void | ElChildren.Text;
export namespace ElChildren {
    export type Void = undefined;
    export type Text = string;
}
function f<C extends ElChildren>(c: C): C { return c; }
const v1: undefined = f(undefined);
const v2: string = f("hello");
"#,
    );
    // The type alias should resolve, allowing the constraint check
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322: constraint ElChildren should resolve to undefined | string. Got: {codes:?}"
    );
}

#[test]
fn test_namespace_member_access_still_works() {
    // Namespace member access (Foo.A in type position) should still work
    let codes = get_error_codes(
        r#"
export type Foo = Foo.A | Foo.B;
export namespace Foo {
    export type A = number;
    export type B = string;
}
const x: Foo.A = 42;
const y: Foo.B = "hello";
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Namespace member type access should still work. Got: {codes:?}"
    );
}
