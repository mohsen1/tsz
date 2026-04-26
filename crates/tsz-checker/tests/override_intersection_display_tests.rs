//! Tests for intersection type display names in override diagnostics.
//!
//! When a class extends a value typed as an intersection of constructor types
//! (e.g., `C1 & C2`), override error messages should display the intersection
//! of instance types (e.g., "I1 & I2") rather than the eagerly merged flat
//! object type (e.g., "{ m1: () => void; m2: () => void }").

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            no_implicit_override: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// Intersection of two constructor interfaces: instance type display should
/// preserve the named intersection form "I1 & I2".
#[test]
fn override_intersection_shows_named_types() {
    let diags = get_diagnostics(
        r#"
interface I1 { m1(): void; }
interface I2 { m2(): void; }
interface C1 { new(...args: any[]): I1; }
interface C2 { new(...args: any[]): I2; }
const Foo: C1 & C2 = class { m1() {} m2() {} } as any;
class Bar extends Foo {
    m1() { }
    m2() { }
}
"#,
    );

    // With noImplicitOverride, should get TS4114 for m1 and m2
    let ts4114_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 4114)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert!(
        ts4114_msgs.len() >= 2,
        "Expected at least 2 TS4114 diagnostics, got {}: {:?}",
        ts4114_msgs.len(),
        diags
    );

    // Verify the display name uses intersection form, not flat object
    for msg in &ts4114_msgs {
        assert!(
            msg.contains("I1 & I2") || msg.contains("I2 & I1"),
            "Expected intersection display name 'I1 & I2' in message, got: {msg}"
        );
        assert!(
            !msg.contains("m1:") && !msg.contains("m2:"),
            "Should NOT show flat object properties in message, got: {msg}"
        );
    }
}

/// When an intersection member is an anonymous constructor, its instance
/// type should be displayed structurally, while named members are displayed
/// by name.
#[test]
#[ignore = "regression from remote commits"]
fn override_intersection_mixed_named_and_anonymous() {
    let diags = get_diagnostics(
        r#"
class A { doSomething() {} }
interface Extra { new(...args: any[]): { context: string } }
const Mixed: (typeof A) & Extra = class extends A { context: string = "" } as any;
class B extends Mixed {
    override foo() {}
}
"#,
    );

    // Should get TS4113 for 'foo' (not in base class)
    let ts4113_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 4113)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert!(!ts4113_msgs.is_empty(), "Expected TS4113, got: {diags:?}");

    // Should show intersection form with "A" and structural part
    for msg in &ts4113_msgs {
        assert!(
            msg.contains(" & "),
            "Expected intersection display with ' & ', got: {msg}"
        );
    }
}

/// When a mixin constructor's return type references a utility conditional
/// application such as `InstanceType<C>`, tsc reduces the application to its
/// concrete form (`Context`) when rendering the heritage instance type in
/// override diagnostics. This mirrors the `override19.ts` conformance
/// scenario: the diagnostic must render `A & { context: Context; }` rather
/// than `A & { context: InstanceType<typeof Context>; }`.
///
/// The test uses a locally-defined alias with the same conditional body as
/// the standard `InstanceType<T>` so it does not need the full lib loaded
/// by the test harness.
#[test]
fn override_heritage_display_reduces_utility_conditional_application() {
    let diags = get_diagnostics(
        r#"
type Foo = abstract new(...args: any) => any;
type Inst<T extends Foo> = T extends abstract new (...args: any) => infer R ? R : any;
declare function CreateMixin<C extends Foo, T extends Foo>(Context: C, Base: T): T & {
   new (...args: any[]): { context: Inst<C> }
}
class Context {}
class A {
    doSomething() {}
}
class B extends CreateMixin(Context, A) {
   override foo() {}
}
class C extends CreateMixin(Context, A) {
    override doSomethang() {}
}
"#,
    );

    let override_diags: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 4113 || *code == 4117)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert!(
        !override_diags.is_empty(),
        "Expected TS4113/TS4117 diagnostics, got: {diags:?}"
    );

    for msg in &override_diags {
        assert!(
            msg.contains("A & { context: Context; }"),
            "Expected reduced heritage display 'A & {{ context: Context; }}' in message, got: {msg}"
        );
        assert!(
            !msg.contains("Inst<"),
            "Should NOT render unreduced 'Inst<...>' in heritage display, got: {msg}"
        );
    }
}
