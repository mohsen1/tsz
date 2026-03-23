//! Tests for enum initializer cycle detection and self-reference handling.
//!
//! Ensures that self-referencing enums, mutually-recursive enums, and multi-hop
//! cycles do not cause stack overflows, and that appropriate diagnostics are emitted.

use crate::test_utils::check_source_diagnostics;

// =========================================================================
// Direct self-reference
// =========================================================================

/// `enum E { A = E.A }` — direct property-access self-reference.
/// Should emit TS2565 and must not stack-overflow.
#[test]
fn enum_direct_self_reference_property_access() {
    let diags = check_source_diagnostics("enum E { A = E.A }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for direct self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = E["A"] }` — element-access self-reference.
#[test]
fn enum_direct_self_reference_element_access() {
    let diags = check_source_diagnostics("enum E { A = E[\"A\"] }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for element-access self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = 1, B = E.B + 1 }` — self-reference embedded in binary expr.
#[test]
fn enum_self_reference_in_binary_expression() {
    let diags = check_source_diagnostics("enum E { A = 1, B = E.B + 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for self-reference in binary expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Mutual recursion between non-const enums
// =========================================================================

/// Two-enum cycle: `enum E { A = F.B }; enum F { B = E.A }`.
/// Must not stack-overflow.
#[test]
fn enum_mutual_recursion_two_enums() {
    let diags = check_source_diagnostics("enum E { A = F.B }\nenum F { B = E.A }");
    // The critical invariant is no panic / stack overflow.
    let _ = diags;
}

/// Three-enum cycle: `E -> F -> G -> E`.
#[test]
fn enum_mutual_recursion_three_enums() {
    let diags =
        check_source_diagnostics("enum E { A = F.B }\nenum F { B = G.C }\nenum G { C = E.A }");
    let _ = diags;
}

/// Mutual recursion with auto-incremented members mixed in.
#[test]
fn enum_mutual_recursion_with_auto_increment() {
    let diags = check_source_diagnostics("enum E { X, A = F.B }\nenum F { Y, B = E.A }");
    let _ = diags;
}

// =========================================================================
// Mutual recursion between const enums
// =========================================================================

/// `const enum E { A = F.B }; const enum F { B = E.A }` — must not overflow.
#[test]
fn const_enum_mutual_recursion_two_enums() {
    let diags = check_source_diagnostics("const enum E { A = F.B }\nconst enum F { B = E.A }");
    let _ = diags;
}

/// `const enum E { A = F.B }; const enum F { B = G.C }; const enum G { C = E.A }`.
#[test]
fn const_enum_mutual_recursion_three_enums() {
    let diags = check_source_diagnostics(
        "const enum E { A = F.B }\nconst enum F { B = G.C }\nconst enum G { C = E.A }",
    );
    let _ = diags;
}

/// Self-referencing const enum member: `const enum E { A = E.A }`.
#[test]
fn const_enum_direct_self_reference() {
    let diags = check_source_diagnostics("const enum E { A = E.A }");
    // Should emit TS2565 (self-reference) and must not overflow.
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for const enum self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Forward references (TS2651)
// =========================================================================

/// `enum E { A = B, B = 1 }` — forward reference to later member.
#[test]
fn enum_forward_reference_bare_identifier() {
    let diags = check_source_diagnostics("enum E { A = B, B = 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for forward reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = E.B, B = 1 }` — forward reference via property access.
#[test]
fn enum_forward_reference_property_access() {
    let diags = check_source_diagnostics("enum E { A = E.B, B = 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for forward reference via property access, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Valid cross-enum chains (no cycle)
// =========================================================================

/// Deep non-circular chain: `A -> B -> C -> literal`.
/// Should resolve without errors.
#[test]
fn const_enum_deep_chain_resolves_correctly() {
    let diags = check_source_diagnostics(
        "const enum A { X = 42 }\nconst enum B { Y = A.X }\nconst enum C { Z = B.Y }",
    );
    let ts2474 = diags.iter().filter(|d| d.code == 2474).count();
    assert_eq!(
        ts2474,
        0,
        "Unexpected TS2474 for valid const enum chain, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Non-const enum chain: `A -> B -> literal` with non-const enums.
#[test]
fn enum_non_const_chain_resolves_correctly() {
    let diags =
        check_source_diagnostics("enum A { X = 10 }\nenum B { Y = A.X }\nenum C { Z = B.Y }");
    // Non-const cross-enum references should resolve without crash.
    let _ = diags;
}
