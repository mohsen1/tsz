use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

/// A class extending a mixin whose base intersection lacks the property
/// must NOT emit TS2416 ("not assignable to ... base type ... 'never'").
///
/// Property access on an intersection where no member declares the
/// property resolves to `Success { type_id: never }` (rather than
/// `PropertyNotFound`) in our solver. The heritage-override check used
/// to treat that as "member exists in base" and emit a spurious
/// TS2416 against `never`. tsc only fires TS2416 when the base actually
/// declares the property; we now match by treating `never` as "no such
/// member" in the existence probe.
///
/// Mirrors the failing pattern in
/// `tests/cases/conformance/classes/mixinAccessModifiers.ts` (class C4
/// extending `Mix(Protected, Protected2)` declares `f`, base lacks `f`).
#[test]
fn mixin_class_method_not_in_intersection_base_no_ts2416() {
    let source = r#"
class A { protected p: string = ""; }
class B { protected p: string = ""; }
declare function Mix<T, U>(c1: T, c2: U): T & U;

class C extends Mix(A, B) {
  f(c: C) { return c.p; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert!(
        ts2416.is_empty(),
        "Expected no TS2416 (no override; base has no `f`), got: {diags:#?}"
    );
}

/// Verify the inverse still fires: when the base genuinely has the
/// property and the derived signature is incompatible, TS2416 must
/// still be emitted.
#[test]
fn class_method_incompatible_with_present_base_member_emits_ts2416() {
    let source = r#"
class Base { f(): string { return ""; } }
class Derived extends Base {
  f(): number { return 1; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    assert_eq!(
        ts2416.len(),
        1,
        "Expected TS2416 when derived overrides with incompatible signature, got: {diags:#?}"
    );
}

#[test]
fn ordinary_static_override_does_not_use_mixin_static_fallback() {
    let source = r#"
class Base {
  protected static x = 1;
}
class Derived extends Base {
  public static x = 1;
}
Derived.x;
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2445: Vec<_> = diags.iter().filter(|d| d.code == 2445).collect();
    assert!(
        ts2445.is_empty(),
        "Expected ordinary static access to use Derived.x, not protected Base.x, got: {diags:#?}"
    );
}
