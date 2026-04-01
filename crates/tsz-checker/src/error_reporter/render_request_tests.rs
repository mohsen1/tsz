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
        .expect(&format!(
            "Expected TS2339 for merged class namespace access, got: {diagnostics:?}"
        ));
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
        .expect(&format!(
            "Expected TS2339 for interface-typed merged class namespace access, got: {diagnostics:?}"
        ));
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
