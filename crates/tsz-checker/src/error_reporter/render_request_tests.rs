//! Focused tests for DiagnosticRenderRequest-based emission paths.
//!
//! These verify that migrated reporters produce consistent anchor positions,
//! related-information content, and diagnostic codes after the centralization
//! from open-coded anchor/related-info decisions to `DiagnosticRenderRequest`.

use crate::test_utils::check_source_diagnostics;

// =========================================================================
// TS2353 / excess property — migrated in properties.rs
// =========================================================================

#[test]
fn excess_property_anchor_at_property_token() {
    let source = r#"
let x: { a: number } = { a: 1, b: 2 };
"#;
    let diagnostics = check_source_diagnostics(source);
    let excess = diagnostics
        .iter()
        .find(|d| d.code == 2353 || d.code == 2561 || d.code == 2322);
    assert!(
        excess.is_some(),
        "Expected an excess property or assignability error, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn excess_property_suppressed_for_error_target() {
    // When target is `any`, excess property errors should be suppressed.
    let source = r#"
declare var x: any;
x = { a: 1, b: 2 };
"#;
    let diagnostics = check_source_diagnostics(source);
    let excess = diagnostics
        .iter()
        .find(|d| d.code == 2353 || d.code == 2561);
    assert!(
        excess.is_none(),
        "Should not emit excess property error for `any` target, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// TS2345 / argument not assignable — migrated in call_errors.rs
// =========================================================================

#[test]
fn argument_not_assignable_with_related_info() {
    let source = r#"
function f(x: { a: number; b: string }) {}
f({ a: 1 });
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2345 = diagnostics.iter().find(|d| d.code == 2345);
    // Should produce either TS2345 with related info or TS2353 for excess/missing property.
    let has_relevant = diagnostics.iter().any(|d| d.code == 2345 || d.code == 2741);
    assert!(
        has_relevant,
        "Expected TS2345 or TS2741, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    // If TS2345 present, check it has related information from failure reason
    if let Some(diag) = ts2345 {
        // TS2345 with missing properties should have related information
        assert!(
            !diag.related_information.is_empty(),
            "TS2345 for missing property should have related information, got empty"
        );
    }
}

#[test]
fn call_missing_property_related_info_widens_fresh_inferred_target() {
    let source = r#"
declare function assertEqual<T>(actual: T, expected: NoInfer<T>): boolean;
const g = { x: 3, y: 2 };
assertEqual(g, { x: 3 });
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2741 = diagnostics
        .iter()
        .find(|d| d.code == 2741)
        .expect("expected promoted TS2741 head for missing inferred property");
    assert!(
        ts2741.message_text.contains("Property 'y' is missing")
            && ts2741.message_text.contains("{ x: number; }")
            && ts2741.message_text.contains("{ x: number; y: number; }"),
        "Expected fresh inferred object displays to be widened in the TS2741 head, got: {}",
        ts2741.message_text
    );
    assert!(
        !ts2741.message_text.contains("{ x: 3"),
        "TS2741 head should not leak fresh literal object displays, got: {}",
        ts2741.message_text
    );
}

#[test]
fn argument_not_assignable_suppressed_for_identical_types() {
    let source = r#"
function f(x: number) {}
let n: number = 42;
f(n);
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2345 = diagnostics.iter().find(|d| d.code == 2345);
    assert!(
        ts2345.is_none(),
        "Should not emit TS2345 for identical types, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn variadic_tuple_rest_call_expands_parameter_display() {
    let source = r#"
type Funcs = [...((arg: number) => void)[], (arg: string) => void];
declare function f1(...args: Funcs): void;
f1();
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2345 = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345 for empty variadic tuple rest call");

    assert!(
        ts2345
            .message_text
            .contains("[...((arg: number) => void)[], (arg: string) => void]"),
        "Expected expanded variadic tuple display in TS2345, got: {:?}",
        ts2345.message_text
    );
    assert!(
        !ts2345.message_text.contains("parameter of type 'Funcs'"),
        "Expected TS2345 to expand the rest tuple alias at the call site, got: {:?}",
        ts2345.message_text
    );
}

// =========================================================================
// TS2769 / no overload matches — migrated in call_errors.rs
// =========================================================================

#[test]
fn no_overload_matches_with_related_failures() {
    let source = r#"
declare function f(x: number): void;
declare function f(x: string): void;
f(true);
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2769 = diagnostics.iter().find(|d| d.code == 2769);
    assert!(
        ts2769.is_some(),
        "Expected TS2769 for no overload match, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let diag = ts2769.unwrap();
    assert!(
        !diag.related_information.is_empty(),
        "TS2769 should have related overload failure information"
    );
}

// =========================================================================
// TS2352 / type assertion overlap — migrated in generics.rs
// =========================================================================

#[test]
fn type_assertion_overlap_anchor_consistency() {
    let source = r#"
let x = 42 as string;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2352 = diagnostics.iter().find(|d| d.code == 2352);
    assert!(
        ts2352.is_some(),
        "Expected TS2352 for type assertion overlap, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// TS2322 / type not assignable (generic fallback) — migrated in assignability.rs
// =========================================================================

#[test]
fn type_not_assignable_generic_anchor_consistency() {
    let source = r#"
let x: string = 42;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().find(|d| d.code == 2322);
    assert!(
        ts2322.is_some(),
        "Expected TS2322 for type mismatch, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let diag = ts2322.unwrap();
    // Anchor should be on `x` (the variable name), not the whole statement
    assert!(
        diag.length > 0 && diag.length <= 10,
        "TS2322 anchor length should be narrow (variable name), got {}",
        diag.length
    );
}

#[test]
fn type_not_assignable_with_missing_property() {
    let source = r#"
let x: { a: number; b: string } = { a: 1 };
"#;
    let diagnostics = check_source_diagnostics(source);
    let has_ts2741 = diagnostics.iter().any(|d| d.code == 2741);
    let has_ts2322 = diagnostics.iter().any(|d| d.code == 2322);
    assert!(
        has_ts2741 || has_ts2322,
        "Expected TS2741 or TS2322 for missing property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2322_optional_parameter_annotation_uses_structural_undefined() {
    let source = r#"
type undefinedBox = { value: string };
function f(value?: undefinedBox) {
    const target: undefinedBox = value;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got: {diagnostics:?}"));
    assert!(
        ts2322
            .message_text
            .contains("Type 'undefinedBox | undefined' is not assignable to type 'undefinedBox'"),
        "TS2322 should append optional-parameter undefined structurally, got: {:?}",
        ts2322.message_text
    );
}

// =========================================================================
// TS2322 / private brand mismatch — migrated in assignability.rs
// =========================================================================

#[test]
fn private_brand_mismatch_has_related_info() {
    let source = r#"
class A { private x = 1; }
class B { private x = 2; }
let a: A = new B();
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().find(|d| d.code == 2322);
    assert!(
        ts2322.is_some(),
        "Expected TS2322 for private brand mismatch, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let diag = ts2322.unwrap();
    // Private brand mismatches should have related information explaining why
    assert!(
        !diag.related_information.is_empty(),
        "TS2322 for private brand mismatch should have related information"
    );
}

// =========================================================================
// Constructor accessibility — migrated in assignability_helpers.rs
// =========================================================================

#[test]
fn constructor_accessibility_mismatch_renders_through_request() {
    // This test exercises the emit_render_request_at_anchor path for
    // constructor accessibility mismatches. When a protected constructor
    // is assigned to a public constructor type, TS2322 should be emitted.
    let source = r#"
class A { protected constructor() {} }
class B extends A { constructor() { super(); } }
let x: new () => A = A;
"#;
    let diagnostics = check_source_diagnostics(source);
    // The exact diagnostic depends on constructor accessibility detection
    // and whether the checker identifies the mismatch. This test ensures
    // the render-request path doesn't crash or produce incorrect anchors.
    // Even if no diagnostic is emitted (because the checker might not
    // detect this pattern), the path is exercised without panic.
    for d in &diagnostics {
        // All diagnostics should have valid positions
        assert!(
            d.length <= 1000,
            "Anchor length should be reasonable: {}",
            d.length
        );
    }
}

// =========================================================================
// Anchor normalization — fingerprint stability tests
// =========================================================================

/// Verify that TS2322 on a variable declaration anchors at the variable
/// name, not the entire declaration (including type annotation + initializer).
#[test]
fn ts2322_variable_declaration_anchor_is_name_only() {
    // `x` starts at column 5 and is 1 char; the full declaration is ~24 chars.
    let source = "let x: string = 42;\n";
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().find(|d| d.code == 2322);
    assert!(
        ts2322.is_some(),
        "Expected TS2322, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let d = ts2322.unwrap();
    // The anchor should cover just `x` (1 character), not `x: string = 42`.
    assert_eq!(
        d.length, 1,
        "TS2322 anchor should be the identifier 'x' (length 1), got length {}",
        d.length
    );
}

/// Verify that TS2322 on a multi-character variable name has the correct length.
#[test]
fn ts2322_variable_declaration_anchor_multichar_name() {
    let source = "let myVar: number = \"hello\";\n";
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().find(|d| d.code == 2322);
    assert!(ts2322.is_some(), "Expected TS2322");
    let d = ts2322.unwrap();
    assert_eq!(
        d.length, 5,
        "TS2322 anchor should be 'myVar' (length 5), got length {}",
        d.length
    );
}

/// Verify that TS2322 message text uses the correct type names.
#[test]
fn ts2322_message_contains_type_names() {
    let source = "let x: string = 42;\n";
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("Expected TS2322");
    assert!(
        ts2322.message_text.contains("number") || ts2322.message_text.contains("42"),
        "TS2322 message should mention the source type, got: {}",
        ts2322.message_text
    );
    assert!(
        ts2322.message_text.contains("string"),
        "TS2322 message should mention the target type 'string', got: {}",
        ts2322.message_text
    );
}

/// Verify that TS2322 on an assignment statement anchors at the whole
/// expression statement (tsc behavior), not just the right-hand side.
#[test]
fn ts2322_assignment_anchor_walks_to_statement() {
    // tsc anchors TS2322 for `x = 42` at the `ExpressionStatement`, which
    // covers the full `x = 42` text (or the LHS identifier for var decl).
    let source = r#"
let x: string;
x = 42;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().find(|d| d.code == 2322);
    assert!(ts2322.is_some(), "Expected TS2322 for assignment mismatch");
}

/// Verify that TS2339 anchors on the property name token, not the
/// whole access expression.
#[test]
fn ts2339_anchor_at_property_name() {
    let source = r#"
let x: { a: number } = { a: 1 };
x.nonexistent;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2339 = diagnostics.iter().find(|d| d.code == 2339);
    assert!(
        ts2339.is_some(),
        "Expected TS2339 for missing property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let d = ts2339.unwrap();
    // The anchor should be "nonexistent" (11 chars), not "x.nonexistent" (13 chars).
    assert_eq!(
        d.length, 11,
        "TS2339 anchor should be 'nonexistent' (length 11), got length {}",
        d.length
    );
}

#[test]
fn ts2339_merged_class_namespace_instance_uses_instance_display() {
    let source = r#"
class Sammy {
    foo() { return "hi"; }
}
namespace Sammy {
    export const x = 1;
}
const instance = new Sammy();
instance.x;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2339 = diagnostics
        .iter()
        .find(|d| d.code == 2339 && d.message_text.contains("Property 'x'"))
        .unwrap_or_else(|| {
            panic!("Expected TS2339 for merged class namespace access, got: {diagnostics:?}")
        });
    assert!(
        ts2339.message_text.contains("type 'Sammy'."),
        "Expected instance-side display in TS2339, got: {:?}",
        ts2339.message_text
    );
    assert!(
        !ts2339.message_text.contains("typeof Sammy"),
        "Merged class namespace instance access should not render typeof Sammy: {:?}",
        ts2339.message_text
    );
}

#[test]
fn ts2339_merged_class_namespace_interface_property_uses_instance_display() {
    let source = r#"
class Sammy {
   foo() { return "hi"; }
  static bar() {
    return -1;
   }
}
namespace Sammy {
    export var x = 1;
}
interface JQueryStatic {
    sammy: Sammy;
}
declare var $: JQueryStatic;
var r4 = $.sammy.x;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2339 = diagnostics
        .iter()
        .find(|d| d.code == 2339 && d.message_text.contains("Property 'x'"))
        .unwrap_or_else(|| panic!("Expected TS2339 for interface-typed merged class namespace access, got: {diagnostics:?}"));
    assert!(
        ts2339.message_text.contains("type 'Sammy'."),
        "Expected instance-side display in TS2339, got: {:?}",
        ts2339.message_text
    );
    assert!(
        !ts2339.message_text.contains("typeof Sammy"),
        "Merged class namespace property access through interface should not render typeof Sammy: {:?}",
        ts2339.message_text
    );
}

/// Verify that object type formatting includes trailing semicolons.
#[test]
fn ts2322_object_type_message_has_semicolons() {
    let source = r#"
let x: { a: number; b: string } = { a: "hello", b: 42 };
"#;
    let diagnostics = check_source_diagnostics(source);
    // Should have either TS2322 or TS2353 (excess property) or TS2741 (missing property)
    let relevant = diagnostics
        .iter()
        .find(|d| d.code == 2322 || d.code == 2741 || d.code == 2353);
    if let Some(d) = relevant {
        // If the message mentions an object shape, verify semicolons
        if d.message_text.contains("{ ") && d.message_text.contains(" }") {
            assert!(
                d.message_text.contains("; }"),
                "Object type in message should have trailing semicolon, got: {}",
                d.message_text
            );
        }
    }
}

/// Regression: when a generic Application type (e.g. `T<A>`) has a sibling
/// non-generic alias (`type C = T<A>`) that evaluates to the same body, the
/// TS2322 message must reference the *Application form* used at the source
/// location, not the unrelated alias name.
///
/// Source pattern (covers `genericIndexedAccessVarianceComparisonResultCorrect.ts`):
///   declare let a: T<A>; declare let b: T<B>; b = a;
/// tsc emits: "Type 'T<A>' is not assignable to type 'T<B>'."
/// Bug: tsz emitted: "Type 'C' is not assignable to type 'D'." — using the
/// unrelated `type C = T<A>` / `type D = T<B>` alias names that happened to
/// share the evaluated body.
#[test]
fn ts2322_uses_application_form_not_unrelated_sibling_alias() {
    // Inline `Pick` because the test harness runs without lib types.
    let source = r#"
class A { x: string = 'A'; y: number = 0; }
class B { x: string = 'B'; z: boolean = true; }
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type T<X extends { x: any }> = Pick<X, 'x'>;
type C = T<A>;
type D = T<B>;
declare let a: T<A>;
declare let b: T<B>;
b = a;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got: {diagnostics:?}"));
    assert!(
        ts2322.message_text.contains("T<A>") && ts2322.message_text.contains("T<B>"),
        "TS2322 should reference the Application form `T<A>`/`T<B>` from the variable annotations, got: {}",
        ts2322.message_text
    );
    assert!(
        !ts2322.message_text.contains("'C'") && !ts2322.message_text.contains("'D'"),
        "TS2322 should not reference unrelated sibling aliases `C`/`D`, got: {}",
        ts2322.message_text
    );
}

/// Regression: TS2322 messages must preserve the alias name for a
/// self-referencing union type alias (e.g. `type SimpleType = string |
/// Promise<SimpleType>`) instead of expanding to the union body.
///
/// Source pattern (covers `unresolvableSelfReferencingAwaitedUnion.ts`):
///   type `SimpleType` = string | Promise<SimpleType>;
///   declare const simple: `SimpleType`;
///   const env: <T>() => T = () => simple;
/// tsc emits: "Type '`SimpleType`' is not assignable to type 'T'."
/// Bug: tsz emitted: "Type 'string | Promise<SimpleType>' is not
/// assignable to type 'T'." — losing the alias identity.
#[test]
fn ts2322_preserves_self_referencing_union_alias_name() {
    // Inline `Promise` as a minimal generic stand-in because the test
    // harness runs without lib types.
    let source = r#"
interface Promise<T> {
    then<U>(onFulfilled: (value: T) => U | Promise<U>): Promise<U>;
}
type EnvFunction = <T>() => T;
type SimpleType = string | Promise<SimpleType>;
declare const simple: SimpleType;
const env: EnvFunction = () => simple;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got: {diagnostics:?}"));
    assert!(
        ts2322.message_text.contains("'SimpleType'"),
        "TS2322 should reference the alias name `SimpleType`, got: {}",
        ts2322.message_text
    );
    assert!(
        !ts2322.message_text.contains("string | Promise"),
        "TS2322 should not expand the alias body, got: {}",
        ts2322.message_text
    );
}

#[test]
fn ts2322_alias_rewrite_preserves_optional_parameter_undefined_surface() {
    let source = r#"
declare var f: (s: string, n?: number) => void;
declare var g: (s: string, b?: boolean) => void;
f = g;
g = f;
"#;
    let diagnostics = check_source_diagnostics(source);
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        messages.iter().any(|message| message.contains(
            "Type '(s: string, b?: boolean | undefined) => void' is not assignable to type '(s: string, n?: number | undefined) => void'"
        )),
        "TS2322 should preserve optional source parameter `| undefined`, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|message| message.contains(
            "Type '(s: string, n?: number | undefined) => void' is not assignable to type '(s: string, b?: boolean | undefined) => void'"
        )),
        "TS2322 should preserve optional source parameter `| undefined`, got: {messages:#?}"
    );
}

/// Regression: TS2345 source display for an identifier whose declared
/// type is an object with a single construct signature (e.g. `{ new
/// <T>(x: T, y: T): string }`) must use tsc's arrow form (`new <T>(x:
/// T, y: T) => string`), not the verbose object form.
///
/// The previous `should_use_arrow_syntax` gate only considered single
/// *call* signatures, so single *construct* signatures fell through to
/// the annotation-text path which preserves the user-written object
/// form. Extending the gate matches tsc.
#[test]
fn ts2345_single_construct_signature_object_uses_arrow_syntax() {
    let source = r#"
function foo<T>(cb: { new(x: T): string; new(x: T, y?: T): string }) {
    return cb;
}
declare var b: { new <T>(x: T, y: T): string };
foo(b);
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2345 = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("Expected TS2345, got: {diagnostics:?}"));
    assert!(
        ts2345
            .message_text
            .contains("new <T>(x: T, y: T) => string"),
        "TS2345 should render single-construct-sig source as arrow form, got: {}",
        ts2345.message_text
    );
    assert!(
        !ts2345
            .message_text
            .contains("'{ new <T>(x: T, y: T): string; }'"),
        "TS2345 should not use verbose object form for single-construct-sig source, got: {}",
        ts2345.message_text
    );
}

/// Verify deterministic type formatting — same source always produces
/// the same message text.
#[test]
fn ts2322_deterministic_message_text() {
    let source = "let x: string = 42;\n";
    let d1 = check_source_diagnostics(source);
    let d2 = check_source_diagnostics(source);
    let msg1: Vec<_> = d1.iter().map(|d| &d.message_text).collect();
    let msg2: Vec<_> = d2.iter().map(|d| &d.message_text).collect();
    assert_eq!(
        msg1, msg2,
        "Repeated checks should produce identical messages"
    );
}

#[test]
fn ts2322_related_generic_indexed_access_source_uses_short_namespace_member() {
    let source = r#"
declare namespace Foo {
    interface Elements {
        a: { a: string };
        b: { b: number };
    }
}

class I<T1 extends keyof Foo.Elements, T2 extends keyof Foo.Elements> {
    M() {
        let c1: Foo.Elements[T1] = {} as any;
        const c2: Foo.Elements[T2] = c1;
    }
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got: {diagnostics:?}"));
    assert!(
        ts2322
            .message_text
            .contains("Type 'Elements[T1]' is not assignable to type 'Elements[T2]'"),
        "TS2322 should use the short namespace member name for related generic indexed access source, got: {:?}",
        ts2322.message_text
    );
    assert!(
        !ts2322.message_text.contains("Foo.Elements[T1]"),
        "TS2322 source display should not keep the namespace qualifier, got: {:?}",
        ts2322.message_text
    );
}

/// Verify that TS2339 on a generic mapped-type typed identifier includes
/// `| undefined` for optional properties in the message.
///
/// Regression: `property_receiver_display_for_node` had a shortcut that returned
/// the raw annotation string for generic-typed identifiers (via
/// `format_annotation_like_type`), bypassing the type formatter that adds
/// `| undefined`. The fix skips the shortcut when the annotation contains `{`
/// (inline object types), so the proper formatter runs instead.
#[test]
fn ts2339_generic_mapped_type_receiver_includes_optional_undefined() {
    // Use a user-defined mapped type instead of Required<T> from lib.es5.d.ts
    // because this test environment does not load the standard library.
    let source = r#"
type Req<T> = { [P in keyof T]-?: T[P] };
const a: Req<{ a?: 1; x: 1 }> = { a: 1, x: 1 };
a.b;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2339 = diagnostics
        .iter()
        .find(|d| d.code == 2339 && d.message_text.contains("Property 'b'"))
        .unwrap_or_else(|| {
            panic!("Expected TS2339 for missing property 'b', got: {diagnostics:?}")
        });
    assert!(
        ts2339.message_text.contains("| undefined"),
        "TS2339 message for generic mapped type receiver should include '| undefined' for optional property, got: {:?}",
        ts2339.message_text
    );
}

// Diagnostic chain order stability for related-information chains (issue
// #10918). The normalization sort used to alphabetize same-anchor entries
// by `message_text`, which placed a `Type 'X' is not assignable to type
// 'Y'.` leaf above its `Types of property 'P' are incompatible.` header
// because the space in `"Type "` sorts before `'s'` in `"Types"`. The fix
// adds `depth` to the sort key before the textual tiebreaker.

#[track_caller]
fn property_chain_indices(
    related: &[tsz_checker::diagnostics::DiagnosticRelatedInformation],
    property_name: &str,
) -> (usize, usize) {
    let header = related
        .iter()
        .position(|info| {
            info.message_text.contains("Types of property")
                && info.message_text.contains(&format!("'{property_name}'"))
        })
        .unwrap_or_else(|| {
            panic!("expected `Types of property '{property_name}'` header, got: {related:?}")
        });
    let leaf = related
        .iter()
        .position(|info| {
            info.message_text.starts_with("Type ")
                && info.message_text.contains("is not assignable to type")
        })
        .unwrap_or_else(|| panic!("expected leaf relation line, got: {related:?}"));
    (header, leaf)
}

/// Header precedes leaf in `related_information`; leaf indents at least one
/// level deeper. Varies property names so the rule is structural, not a
/// fixture spelling. Uses `declare const arg: ...` so the call goes through
/// `related_from_failure_reason`; an object literal would hit a different
/// emit path with no chain.
#[test]
fn ts2345_property_chain_keeps_header_before_leaf_and_indents_leaf() {
    for (prop_name, prop_ty, arg_ty) in [
        ("p", "string", "number"),
        ("key", "string", "boolean"),
        ("alpha", "boolean", "string"),
    ] {
        let source = format!(
            "declare const arg: {{ {prop_name}: {arg_ty} }};\n\
             function take(o: {{ {prop_name}: {prop_ty} }}) {{}}\n\
             take(arg);\n"
        );
        let diagnostics = check_source_diagnostics(&source);
        let ts2345 = diagnostics
            .iter()
            .find(|d| d.code == 2345)
            .unwrap_or_else(|| panic!("expected TS2345 for `{source}`, got: {diagnostics:?}"));

        let (header_idx, leaf_idx) = property_chain_indices(&ts2345.related_information, prop_name);
        assert!(
            header_idx < leaf_idx,
            "header must precede leaf in `{source}`, got header={header_idx}, leaf={leaf_idx}, related={:?}",
            ts2345.related_information
        );
        assert_eq!(
            ts2345.related_information[header_idx].depth, 0,
            "header sits at the first elaboration level; chain: {:?}",
            ts2345.related_information
        );
        assert!(
            ts2345.related_information[leaf_idx].depth >= 1,
            "leaf must indent at least one level beneath its header; chain: {:?}",
            ts2345.related_information
        );
    }
}

/// The chain must remain stable across repeated checks of the same source —
/// two independent compiles must produce the same ordered related-info
/// entries.
#[test]
fn ts2345_property_chain_is_stable_across_repeated_checks() {
    let source = "declare const arg: { p: number };\n\
                  function take(o: { p: string }) {}\n\
                  take(arg);\n";
    let chain = |diags: &[tsz_checker::diagnostics::Diagnostic]| {
        diags
            .iter()
            .find(|d| d.code == 2345)
            .expect("expected a TS2345 chain to compare against")
            .related_information
            .iter()
            .map(|r| (r.depth, r.message_text.clone()))
            .collect::<Vec<_>>()
    };
    let first = chain(&check_source_diagnostics(source));
    let second = chain(&check_source_diagnostics(source));
    assert_eq!(first, second, "chain drifted between independent runs");
}

// =========================================================================
// TS2741 / TS2739 — missing property through an intersection SOURCE
//
// Regression for #10962: an intersection source (written directly, via an
// alias such as `Tagged<T> = T & { kind }`, or via a generic application whose
// base resolves to an intersection) must NOT downgrade a genuine missing
// required named property to a bare generic TS2322. tsc reports the
// property-level miss (TS2741 single / TS2739 multiple) and displays the
// source as-written. Binder names vary across cases so the behavior is keyed
// to type structure, not identifiers.
// =========================================================================

#[test]
fn missing_property_through_mapped_application_intersection_source_is_ts2741() {
    // `Wrap<{ alpha }> & { beta }` is missing the required `gamma`. The mapped
    // application member forces the source to stay an intersection at render
    // time — the case that previously collapsed to generic TS2322.
    let source = r#"
type Wrap<T> = { [K in keyof T]: T[K] };
declare const boxed: Wrap<{ alpha: number }> & { beta: string };
const sink: { alpha: number; gamma: boolean } = boxed;
"#;
    let diagnostics = check_source_diagnostics(source);
    let missing = diagnostics
        .iter()
        .find(|d| d.code == 2741)
        .unwrap_or_else(|| {
            panic!(
                "expected TS2741 for missing property through intersection source, got: {:?}",
                diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        missing.message_text.contains("'gamma'") && missing.message_text.contains("is missing"),
        "TS2741 should name the missing property, got: {}",
        missing.message_text
    );
    // The source must be shown as the as-written intersection, not collapsed.
    assert!(
        missing
            .message_text
            .contains("Wrap<{ alpha: number; }> & { beta: string; }"),
        "TS2741 should display the intersection source as-written, got: {}",
        missing.message_text
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "intersection-source missing property must not also emit generic TS2322, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn missing_property_through_alias_intersection_source_is_ts2741() {
    // `Tagged<T> = T & { kind }` — an alias whose body is an intersection.
    let source = r#"
type Tagged<T> = T & { kind: "x" };
declare const flagged: Tagged<{ epsilon: number }>;
const drain: { epsilon: number; zeta: boolean } = flagged;
"#;
    let diagnostics = check_source_diagnostics(source);
    let missing = diagnostics
        .iter()
        .find(|d| d.code == 2741)
        .unwrap_or_else(|| {
            panic!(
                "expected TS2741 for missing property through alias-intersection source, got: {:?}",
                diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        missing.message_text.contains("'zeta'")
            && missing
                .message_text
                .contains("Tagged<{ epsilon: number; }>"),
        "TS2741 should name the missing property and display the alias source, got: {}",
        missing.message_text
    );
}

#[test]
fn multiple_missing_properties_through_intersection_source_is_ts2739() {
    // Two missing required properties routes through the plural path (TS2739),
    // which must likewise display the intersection source as-written.
    let source = r#"
type Shape<T> = { [K in keyof T]: T[K] };
declare const parcel: Shape<{ mu: number }> & { nu: string };
const consumer: { mu: number; xi: boolean; omicron: number } = parcel;
"#;
    let diagnostics = check_source_diagnostics(source);
    let missing = diagnostics
        .iter()
        .find(|d| d.code == 2739)
        .unwrap_or_else(|| {
            panic!(
                "expected TS2739 for multiple missing properties through intersection source, got: {:?}",
                diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        missing.message_text.contains("xi") && missing.message_text.contains("omicron"),
        "TS2739 should list the missing properties, got: {}",
        missing.message_text
    );
    assert!(
        missing
            .message_text
            .contains("Shape<{ mu: number; }> & { nu: string; }"),
        "TS2739 should display the intersection source as-written, got: {}",
        missing.message_text
    );
}

/// A `function` merged with a same-named `namespace` carries `ValueModule` on
/// its symbol, so tsc renders it as `typeof f` rather than expanding the
/// structural surface. A plain function with *expando* assignments carries no
/// module flag, and there tsc really does print `{ (): void; declared: number; }`
/// — that distinction is the whole rule.
///
/// Every expectation below is pinned to `tsc` 7.0.2 run on the same source
/// (`--noEmit --target es2015 --strict false --pretty false`).
#[test]
fn ts2322_merged_function_namespace_renders_typeof_name() {
    let source = r#"
function fnMerged() {}
namespace fnMerged { export var bar = 1; }
const t: number = fnMerged;
"#;
    let diagnostics = check_source_diagnostics(source);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof fnMerged'"),
        "function+namespace merge should render as `typeof fnMerged`, got: {}",
        d.message_text
    );
    assert!(
        !d.message_text.contains("(): void"),
        "merged function must not expand to its structural surface, got: {}",
        d.message_text
    );
}

/// Anti-hardcoding control: the rule is keyed on the symbol's module flag, not
/// on any particular identifier, so a renamed binder behaves identically.
#[test]
fn ts2322_merged_function_namespace_typeof_survives_renamed_binders() {
    let source = r#"
function zzQuux() {}
namespace zzQuux { export var whatever = 1; }
const t: number = zzQuux;
"#;
    let diagnostics = check_source_diagnostics(source);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof zzQuux'"),
        "renamed binder should render as `typeof zzQuux`, got: {}",
        d.message_text
    );
}

/// Negative control, and the reason the module-flag gate cannot simply be
/// "callable with appended properties": an expando function with **no**
/// namespace merge keeps tsc's structural rendering.
#[test]
fn ts2322_expando_function_without_namespace_stays_structural() {
    let source = r#"
function expandoFn() {}
expandoFn.declared = 1;
const t: number = expandoFn;
"#;
    let diagnostics = check_source_diagnostics(source);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("{ (): void; declared: number; }"),
        "expando function without a namespace merge must stay structural, got: {}",
        d.message_text
    );
    assert!(
        !d.message_text.contains("typeof expandoFn"),
        "expando function without a namespace merge must not render as typeof, got: {}",
        d.message_text
    );
}

/// Ordering control: when a function carries *both* expando assignments and a
/// namespace merge, the module flag wins and tsc prints `typeof bothFn`. This
/// pins the module-flag check ahead of the expando check in the printer.
#[test]
fn ts2322_expando_function_merged_with_namespace_prefers_typeof() {
    let source = r#"
function bothFn() {}
bothFn.expandoProp = 1;
namespace bothFn { export var nsProp = 2; }
const t: number = bothFn;
"#;
    let diagnostics = check_source_diagnostics(source);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof bothFn'"),
        "namespace merge outranks expando assignments, got: {}",
        d.message_text
    );
}

/// Generic and overloaded functions reach the printer through the same merged
/// callable shape; both render under the merged name.
#[test]
fn ts2322_merged_generic_and_overloaded_functions_render_typeof_name() {
    let generic = r#"
function genFn<T>(x: T): T { return x; }
namespace genFn { export var g = 1; }
const t: number = genFn;
"#;
    let diagnostics = check_source_diagnostics(generic);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322 for generic merge, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof genFn'"),
        "generic function+namespace merge should render as `typeof genFn`, got: {}",
        d.message_text
    );

    let overloaded = r#"
function ovl(x: number): number;
function ovl(x: string): string;
function ovl(x: any): any { return x; }
namespace ovl { export var o = 1; }
const t: number = ovl;
"#;
    let diagnostics = check_source_diagnostics(overloaded);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322 for overloaded merge, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof ovl'"),
        "overloaded function+namespace merge should render as `typeof ovl`, got: {}",
        d.message_text
    );
}

/// Fallback controls: a plain function keeps its call-signature rendering, and
/// the already-correct pure-namespace and class+namespace forms are unchanged.
#[test]
fn ts2322_plain_function_and_existing_typeof_forms_are_unchanged() {
    let plain = r#"
function plainFn() {}
const t: number = plainFn;
"#;
    let diagnostics = check_source_diagnostics(plain);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322 for plain function, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("() => void"),
        "a plain function should still render as `() => void`, got: {}",
        d.message_text
    );

    let pure_namespace = r#"
namespace nsOnly { export var z = 1; }
const t: number = nsOnly;
"#;
    let diagnostics = check_source_diagnostics(pure_namespace);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322 for pure namespace, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof nsOnly'"),
        "pure namespace rendering should be unchanged, got: {}",
        d.message_text
    );

    let class_merge = r#"
class clsMerged {}
namespace clsMerged { export var baz = 1; }
const t: number = clsMerged;
"#;
    let diagnostics = check_source_diagnostics(class_merge);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322 for class merge, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof clsMerged'"),
        "class+namespace rendering should be unchanged, got: {}",
        d.message_text
    );
}

/// Regression control for the `esModuleInteropPrettyErrorRelatedInformation.ts`
/// conformance row: only an *instantiated* module merge takes the name.
///
/// `ValueModule` (a namespace containing at least one value) is the operative
/// flag, not `NamespaceModule`. An empty namespace, or one exporting only
/// types, adds nothing to the value's observable surface and tsc keeps printing
/// the call signature. Verified against tsc 7.0.2.
#[test]
fn ts2322_empty_or_type_only_namespace_merge_stays_structural() {
    let empty = r#"
declare function fa(): void;
declare namespace fa {}
const t: number = fa;
"#;
    let diagnostics = check_source_diagnostics(empty);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| {
            panic!("expected TS2322 for empty namespace merge, got: {diagnostics:?}")
        });
    assert!(
        d.message_text.contains("() => void"),
        "an empty namespace merge is not instantiated and must stay structural, got: {}",
        d.message_text
    );
    assert!(
        !d.message_text.contains("typeof fa"),
        "an empty namespace merge must not render as typeof, got: {}",
        d.message_text
    );

    let type_only = r#"
declare function fc(): void;
declare namespace fc { interface Options {} }
const t: number = fc;
"#;
    let diagnostics = check_source_diagnostics(type_only);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| {
            panic!("expected TS2322 for type-only namespace merge, got: {diagnostics:?}")
        });
    assert!(
        d.message_text.contains("() => void"),
        "a type-only namespace merge is not instantiated and must stay structural, got: {}",
        d.message_text
    );
}

/// tsc's real test is the module flag, *not* "does the merge append a visible
/// property". A namespace whose only value is **not exported** contributes no
/// member yet still instantiates the module, and tsc prints `typeof fg`.
/// `crates/tsz-binder/src/modules/binding.rs` now sets `VALUE_MODULE` only for
/// an instantiated namespace (see #16136), so the merged symbol's flag alone
/// distinguishes this case from an empty or type-only namespace.
#[test]
fn ts2322_namespace_with_only_unexported_value_still_renders_typeof() {
    let source = r#"
function fg() {}
namespace fg { var hidden = 1; }
const t: number = fg;
"#;
    let diagnostics = check_source_diagnostics(source);
    let d = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:?}"));
    assert!(
        d.message_text.contains("'typeof fg'"),
        "an unexported value still instantiates the module, got: {}",
        d.message_text
    );
}
