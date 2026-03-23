//! Tests for enum initializer cycle detection and self-reference handling.
//!
//! Ensures that self-referencing enums, mutually-recursive enums, and multi-hop
//! cycles do not cause stack overflows, and that appropriate diagnostics are emitted.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

struct DiagInfo {
    code: u32,
}

fn check_source_diagnostics(source: &str) -> Vec<DiagInfo> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| DiagInfo { code: d.code })
        .collect()
}

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

/// `enum E { A = A }` — bare identifier self-reference.
#[test]
fn enum_direct_self_reference_bare_identifier() {
    let diags = check_source_diagnostics("enum E { A = A }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for bare identifier self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = 1, B = B | A }` — self-reference in bitwise expression.
#[test]
fn enum_self_reference_in_bitwise_expression() {
    let diags = check_source_diagnostics("enum E { A = 1, B = B | A }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for self-reference in bitwise expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = ~E.A }` — self-reference in unary expression.
#[test]
fn enum_self_reference_in_unary_expression() {
    let diags = check_source_diagnostics("enum E { A = ~E.A }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for self-reference in unary expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = (E.A) }` — self-reference inside parenthesized expression.
#[test]
fn enum_self_reference_in_parenthesized_expression() {
    let diags = check_source_diagnostics("enum E { A = (E.A) }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for self-reference in parenthesized expression, got: {:?}",
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

/// Four-enum cycle: `E -> F -> G -> H -> E`.
/// Verifies deeper chains don't overflow.
#[test]
fn enum_mutual_recursion_four_enums() {
    let diags = check_source_diagnostics(
        "enum E { A = F.B }\nenum F { B = G.C }\nenum G { C = H.D }\nenum H { D = E.A }",
    );
    let _ = diags;
}

/// Mutual recursion with auto-incremented members mixed in.
#[test]
fn enum_mutual_recursion_with_auto_increment() {
    let diags = check_source_diagnostics("enum E { X, A = F.B }\nenum F { Y, B = E.A }");
    let _ = diags;
}

/// Two-enum mutual recursion via element access: `E["A"]` style.
#[test]
fn enum_mutual_recursion_element_access() {
    let diags = check_source_diagnostics("enum E { A = F[\"B\"] }\nenum F { B = E[\"A\"] }");
    let _ = diags;
}

/// Two-enum mutual recursion where each has multiple members, only some cycle.
#[test]
fn enum_mutual_recursion_partial_cycle() {
    let diags = check_source_diagnostics(
        "enum E { X = 1, Y = F.B, Z = 3 }\nenum F { A = 10, B = E.Y, C = 20 }",
    );
    let _ = diags;
}

/// Mutual recursion through binary expressions.
/// `enum E { A = F.B + 1 }; enum F { B = E.A + 1 }` — both sides cycle.
#[test]
fn enum_mutual_recursion_in_binary_expression() {
    let diags = check_source_diagnostics("enum E { A = F.B + 1 }\nenum F { B = E.A + 1 }");
    let _ = diags;
}

/// Diamond mutual recursion: E -> F, E -> G, F -> H, G -> H, H -> E.
#[test]
fn enum_mutual_recursion_diamond() {
    let diags = check_source_diagnostics(
        "enum E { A = F.X + G.Y }\n\
         enum F { X = H.Z }\n\
         enum G { Y = H.Z }\n\
         enum H { Z = E.A }",
    );
    let _ = diags;
}

// =========================================================================
// Mutual recursion between const enums
// =========================================================================

/// `const enum E { A = F.B }; const enum F { B = E.A }` — must not overflow.
/// Should emit TS2474 for the circular initializers.
#[test]
fn const_enum_mutual_recursion_two_enums() {
    let diags = check_source_diagnostics("const enum E { A = F.B }\nconst enum F { B = E.A }");
    // Const enum circular references should produce TS2474 because
    // the initializers cannot be evaluated to constant values.
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum circular reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = F.B }; const enum F { B = G.C }; const enum G { C = E.A }`.
/// Three-way const enum cycle should emit TS2474.
#[test]
fn const_enum_mutual_recursion_three_enums() {
    let diags = check_source_diagnostics(
        "const enum E { A = F.B }\nconst enum F { B = G.C }\nconst enum G { C = E.A }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for three-way const enum cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
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

/// Const enum four-way cycle.
#[test]
fn const_enum_mutual_recursion_four_enums() {
    let diags = check_source_diagnostics(
        "const enum E { A = F.B }\n\
         const enum F { B = G.C }\n\
         const enum G { C = H.D }\n\
         const enum H { D = E.A }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for four-way const enum cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Const enum mutual recursion via element access.
#[test]
fn const_enum_mutual_recursion_element_access() {
    let diags =
        check_source_diagnostics("const enum E { A = F[\"B\"] }\nconst enum F { B = E[\"A\"] }");
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum element-access cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Const enum cycle through binary expression:
/// `const enum E { A = F.B + 1 }; const enum F { B = E.A + 1 }`.
#[test]
fn const_enum_mutual_recursion_in_binary_expression() {
    let diags =
        check_source_diagnostics("const enum E { A = F.B + 1 }\nconst enum F { B = E.A + 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum cycle in binary expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Mixed const / non-const mutual recursion
// =========================================================================

/// `const enum E { A = F.B }; enum F { B = E.A }` — mixed const/non-const cycle.
/// Must not stack-overflow.
#[test]
fn mixed_const_nonconst_mutual_recursion() {
    let diags = check_source_diagnostics("const enum E { A = F.B }\nenum F { B = E.A }");
    // Const enum E should get TS2474 since F.B can't be resolved as constant
    let _ = diags;
}

/// `enum E { A = F.B }; const enum F { B = E.A }` — non-const referencing const.
#[test]
fn mixed_nonconst_const_mutual_recursion() {
    let diags = check_source_diagnostics("enum E { A = F.B }\nconst enum F { B = E.A }");
    let _ = diags;
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

/// `enum E { A = E["B"], B = 1 }` — forward reference via element access.
#[test]
fn enum_forward_reference_element_access() {
    let diags = check_source_diagnostics("enum E { A = E[\"B\"], B = 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for forward reference via element access, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = B + C, B = 1, C = 2 }` — forward reference in binary expression.
#[test]
fn enum_forward_reference_in_binary_expression() {
    let diags = check_source_diagnostics("enum E { A = B + C, B = 1, C = 2 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for forward reference in binary expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = B, B = 1 }` — forward reference in const enum.
#[test]
fn const_enum_forward_reference() {
    let diags = check_source_diagnostics("const enum E { A = B, B = 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for const enum forward reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Indirect self-reference within same enum
// =========================================================================

/// `enum E { A = B, B = A }` — two members referencing each other within same enum.
/// B references A which is defined before B (not forward), but A references B (forward).
#[test]
fn enum_indirect_self_reference_two_members() {
    let diags = check_source_diagnostics("enum E { A = B, B = A }");
    // A = B is a forward reference (TS2651).
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for indirect self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `enum E { A = E.B, B = E.C, C = E.A }` — three-member cycle within single enum.
#[test]
fn enum_three_member_cycle_within_single_enum() {
    let diags = check_source_diagnostics("enum E { A = E.B, B = E.C, C = E.A }");
    // A and B reference forward members, C references A which is before it
    // but A -> B -> C -> A creates a cycle. At minimum A = E.B is forward ref.
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for cycle within single enum, got: {:?}",
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

/// Valid back-reference: `enum E { A = 1, B = A }` — A is before B, not forward.
#[test]
fn enum_valid_back_reference() {
    let diags = check_source_diagnostics("enum E { A = 1, B = A }");
    let ts2651 = diags.iter().filter(|d| d.code == 2651).count();
    let ts2565 = diags.iter().filter(|d| d.code == 2565).count();
    assert_eq!(
        ts2651,
        0,
        "Unexpected TS2651 for valid back-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert_eq!(
        ts2565,
        0,
        "Unexpected TS2565 for valid back-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Valid back-reference via property access: `enum E { A = 1, B = E.A }`.
#[test]
fn enum_valid_back_reference_property_access() {
    let diags = check_source_diagnostics("enum E { A = 1, B = E.A }");
    let ts2651 = diags.iter().filter(|d| d.code == 2651).count();
    let ts2565 = diags.iter().filter(|d| d.code == 2565).count();
    assert_eq!(
        ts2651, 0,
        "Unexpected TS2651 for valid property access back-reference"
    );
    assert_eq!(
        ts2565, 0,
        "Unexpected TS2565 for valid property access back-reference"
    );
}

/// Const enum with valid chain: `const enum E { A = 1, B = A + 1, C = B * 2 }`.
#[test]
fn const_enum_valid_chain_expression() {
    let diags = check_source_diagnostics("const enum E { A = 1, B = A + 1, C = B * 2 }");
    let ts2474 = diags.iter().filter(|d| d.code == 2474).count();
    assert_eq!(
        ts2474,
        0,
        "Unexpected TS2474 for valid const enum chain expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Const enum valid cross-enum with unary: `const enum A { X = 5 }; const enum B { Y = -A.X }`.
#[test]
fn const_enum_valid_cross_enum_unary() {
    let diags = check_source_diagnostics("const enum A { X = 5 }\nconst enum B { Y = -A.X }");
    let ts2474 = diags.iter().filter(|d| d.code == 2474).count();
    assert_eq!(
        ts2474, 0,
        "Unexpected TS2474 for valid const enum with unary"
    );
}

// =========================================================================
// Stress tests — deeper chains without cycles
// =========================================================================

/// 6-deep const enum chain: A -> B -> C -> D -> E -> F -> literal.
#[test]
fn const_enum_six_deep_chain() {
    let diags = check_source_diagnostics(
        "const enum F { V = 100 }\n\
         const enum E { V = F.V }\n\
         const enum D { V = E.V }\n\
         const enum C { V = D.V }\n\
         const enum B { V = C.V }\n\
         const enum A { V = B.V }",
    );
    let ts2474 = diags.iter().filter(|d| d.code == 2474).count();
    assert_eq!(
        ts2474, 0,
        "Unexpected TS2474 for valid 6-deep const enum chain"
    );
}

// =========================================================================
// Auto-increment cycle protection
// =========================================================================

/// Cycle through auto-incremented member:
/// `enum E { A = F.C }; enum F { B = E.A, C }`
/// F.C is auto-incremented but depends on F.B which depends on E.A which depends on F.C.
/// Must not stack-overflow.
#[test]
fn enum_cycle_through_auto_increment_member() {
    let diags = check_source_diagnostics("enum E { A = F.C }\nenum F { B = E.A, C }");
    // The critical invariant is no panic / stack overflow.
    let _ = diags;
}

/// Const enum cycle through auto-incremented member.
#[test]
fn const_enum_cycle_through_auto_increment_member() {
    let diags = check_source_diagnostics("const enum E { A = F.C }\nconst enum F { B = E.A, C }");
    // Must not overflow. The circular dependency prevents constant evaluation.
    let _ = diags;
}

/// Auto-increment member referencing another auto-increment member across enums.
/// `enum E { A, B = F.D }; enum F { C = E.B, D }`
#[test]
fn enum_mutual_auto_increment_cycle() {
    let diags = check_source_diagnostics("enum E { A, B = F.D }\nenum F { C = E.B, D }");
    let _ = diags;
}

/// Three-enum cycle going through auto-increment:
/// `enum E { A = F.C }; enum F { B = G.C, C }; enum G { B = E.A, C }`
#[test]
fn enum_three_way_cycle_through_auto_increment() {
    let diags = check_source_diagnostics(
        "enum E { A = F.C }\nenum F { B = G.C, C }\nenum G { B = E.A, C }",
    );
    let _ = diags;
}
