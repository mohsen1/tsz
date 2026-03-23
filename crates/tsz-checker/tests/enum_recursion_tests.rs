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

// =========================================================================
// Bare identifier cycles and resolution
// =========================================================================

/// Bare identifier mutual reference within same enum: `enum E { A = B, B = A }`.
/// A references B (forward) and B references A. Must not stack-overflow.
/// A = B is a forward reference → TS2651.
#[test]
fn enum_bare_identifier_mutual_reference() {
    let diags = check_source_diagnostics("enum E { A = B, B = A }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for forward reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Three bare identifier cycle: `enum E { A = C, B = A, C = B }`.
/// A and C are forward refs. Must not overflow.
#[test]
fn enum_bare_identifier_three_way_cycle() {
    let diags = check_source_diagnostics("enum E { A = C, B = A, C = B }");
    // A = C is a forward reference → TS2651
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for forward reference in three-member bare cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Bare identifier referencing earlier member should resolve correctly.
/// `enum E { A = 10, B = A + 5 }` → B should resolve to 15.
#[test]
fn enum_bare_identifier_back_reference_resolves() {
    let diags = check_source_diagnostics("enum E { A = 10, B = A + 5 }");
    // No forward ref, no self-ref — should be clean
    let ts2651 = diags.iter().filter(|d| d.code == 2651).count();
    let ts2565 = diags.iter().filter(|d| d.code == 2565).count();
    assert_eq!(
        ts2651, 0,
        "Unexpected TS2651 for valid bare identifier back-reference"
    );
    assert_eq!(
        ts2565, 0,
        "Unexpected TS2565 for valid bare identifier back-reference"
    );
}

/// Mixed bare identifier and property access in expressions.
/// `enum E { A = 1, B = A | E.A }` — both forms should resolve.
#[test]
fn enum_mixed_bare_and_property_access() {
    let diags = check_source_diagnostics("enum E { A = 1, B = A | E.A }");
    let ts2651 = diags.iter().filter(|d| d.code == 2651).count();
    let ts2565 = diags.iter().filter(|d| d.code == 2565).count();
    assert_eq!(ts2651, 0, "Unexpected TS2651 for valid mixed references");
    assert_eq!(ts2565, 0, "Unexpected TS2565 for valid mixed references");
}

// =========================================================================
// Multiple mutually recursive enums (broader patterns)
// =========================================================================

/// Five-enum chain cycle: E → F → G → H → I → E.
/// Must not stack-overflow.
#[test]
fn enum_mutual_recursion_five_enums() {
    let diags = check_source_diagnostics(
        "enum E { A = F.B }\n\
         enum F { B = G.C }\n\
         enum G { C = H.D }\n\
         enum H { D = I.X }\n\
         enum I { X = E.A }",
    );
    let _ = diags;
}

/// Five-enum const cycle: all const enums in a ring.
/// Should emit TS2474.
#[test]
fn const_enum_mutual_recursion_five_enums() {
    let diags = check_source_diagnostics(
        "const enum E { A = F.B }\n\
         const enum F { B = G.C }\n\
         const enum G { C = H.D }\n\
         const enum H { D = I.X }\n\
         const enum I { X = E.A }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for five-way const enum cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Two enums with multiple members referencing each other.
/// `enum E { A = F.X, B = F.Y }; enum F { X = E.B, Y = 1 }`.
/// E.A -> F.X -> E.B -> F.Y = 1 (resolves). No cycle in this path.
/// Must not stack-overflow or emit spurious errors.
#[test]
fn enum_cross_reference_multiple_members_no_cycle() {
    let diags = check_source_diagnostics("enum E { A = F.X, B = F.Y }\nenum F { X = E.B, Y = 1 }");
    let _ = diags;
}

/// Two enums where both members cycle: `enum E { A = F.B, C = F.D }; enum F { B = E.C, D = E.A }`.
/// E.A -> F.B -> E.C -> F.D -> E.A (cycle). Must not overflow.
#[test]
fn enum_cross_reference_double_cycle() {
    let diags =
        check_source_diagnostics("enum E { A = F.B, C = F.D }\nenum F { B = E.C, D = E.A }");
    let _ = diags;
}

/// Const enum double cycle should emit TS2474.
#[test]
fn const_enum_cross_reference_double_cycle() {
    let diags = check_source_diagnostics(
        "const enum E { A = F.B, C = F.D }\nconst enum F { B = E.C, D = E.A }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum double cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Nested binary expressions referencing multiple enums in a cycle.
/// `enum E { A = F.B + G.C }; enum F { B = G.C + E.A }; enum G { C = E.A + F.B }`.
#[test]
fn enum_mutual_recursion_nested_binary_three_enums() {
    let diags = check_source_diagnostics(
        "enum E { A = F.B + G.C }\n\
         enum F { B = G.C + E.A }\n\
         enum G { C = E.A + F.B }",
    );
    let _ = diags;
}

/// Const enum version of the same nested binary three-enum cycle.
#[test]
fn const_enum_mutual_recursion_nested_binary_three_enums() {
    let diags = check_source_diagnostics(
        "const enum E { A = F.B + G.C }\n\
         const enum F { B = G.C + E.A }\n\
         const enum G { C = E.A + F.B }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum nested binary three-way cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Template literal element access in cycle.
#[test]
fn enum_mutual_recursion_template_literal_access() {
    let diags = check_source_diagnostics("enum E { A = F[`B`] }\nenum F { B = E[`A`] }");
    let _ = diags;
}

/// Const enum template literal cycle should emit TS2474.
#[test]
fn const_enum_mutual_recursion_template_literal_access() {
    let diags =
        check_source_diagnostics("const enum E { A = F[`B`] }\nconst enum F { B = E[`A`] }");
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum template literal cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Self-reference in const enum (TS2565)
// =========================================================================

/// `const enum E { A = E.A + 1 }` — self-reference in const enum binary expr.
#[test]
fn const_enum_self_reference_in_binary_expression() {
    let diags = check_source_diagnostics("const enum E { A = E.A + 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for const enum self-reference in binary expr, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = ~E.A }` — self-reference in const enum unary expr.
#[test]
fn const_enum_self_reference_in_unary_expression() {
    let diags = check_source_diagnostics("const enum E { A = ~E.A }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for const enum self-reference in unary expr, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = (E.A) }` — self-reference in const enum parenthesized expr.
#[test]
fn const_enum_self_reference_in_parenthesized_expression() {
    let diags = check_source_diagnostics("const enum E { A = (E.A) }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for const enum self-reference in parenthesized expr, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = E["A"] }` — element access self-reference in const enum.
#[test]
fn const_enum_self_reference_element_access() {
    let diags = check_source_diagnostics("const enum E { A = E[\"A\"] }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for const enum element-access self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = A }` — bare identifier self-reference in const enum.
#[test]
fn const_enum_self_reference_bare_identifier() {
    let diags = check_source_diagnostics("const enum E { A = A }");
    assert!(
        diags.iter().any(|d| d.code == 2565),
        "Expected TS2565 for const enum bare identifier self-reference, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Forward references in const enums (TS2651)
// =========================================================================

/// `const enum E { A = E.B, B = 1 }` — forward reference via property access in const enum.
#[test]
fn const_enum_forward_reference_property_access() {
    let diags = check_source_diagnostics("const enum E { A = E.B, B = 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for const enum forward reference via property access, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `const enum E { A = E["B"], B = 1 }` — forward reference via element access in const enum.
#[test]
fn const_enum_forward_reference_element_access() {
    let diags = check_source_diagnostics("const enum E { A = E[\"B\"], B = 1 }");
    assert!(
        diags.iter().any(|d| d.code == 2651),
        "Expected TS2651 for const enum forward reference via element access, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Mixed patterns: cycle + valid members in same enum
// =========================================================================

/// Enum with some valid members and some cycling members.
/// `enum E { X = 1, Y = F.Z, W = 3 }; enum F { Z = E.Y }`.
/// Y -> F.Z -> E.Y is a cycle. X and W are fine.
#[test]
fn enum_mixed_valid_and_cycling_members() {
    let diags = check_source_diagnostics("enum E { X = 1, Y = F.Z, W = 3 }\nenum F { Z = E.Y }");
    // Must not crash. The cycling members should not affect valid members.
    let _ = diags;
}

/// Const enum version: valid members + cycling members.
/// Only the cycling members should get TS2474.
#[test]
fn const_enum_mixed_valid_and_cycling_members() {
    let diags = check_source_diagnostics(
        "const enum E { X = 1, Y = F.Z, W = 3 }\nconst enum F { Z = E.Y }",
    );
    // Y and Z cycle → TS2474 for at least one
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for cycling const enum members, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Deeply nested expressions with cycles
// =========================================================================

/// Cycle hidden inside deeply nested expression:
/// `enum E { A = ((F.B)) }; enum F { B = -(E.A) }`.
#[test]
fn enum_cycle_in_deeply_nested_expression() {
    let diags = check_source_diagnostics("enum E { A = ((F.B)) }\nenum F { B = -(E.A) }");
    let _ = diags;
}

/// Const enum cycle in deeply nested expression should emit TS2474.
#[test]
fn const_enum_cycle_in_deeply_nested_expression() {
    let diags =
        check_source_diagnostics("const enum E { A = ((F.B)) }\nconst enum F { B = -(E.A) }");
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum cycle in nested expression, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Multiple mutually recursive enums — non-constant reporting
// =========================================================================

/// Three non-const enums with interleaved references across multiple members.
/// `enum A { X = B.Y + C.Z }; enum B { Y = C.Z + A.X }; enum C { Z = A.X + B.Y }`.
/// All members cycle. Must not stack-overflow.
#[test]
fn enum_three_way_interleaved_references() {
    let diags = check_source_diagnostics(
        "enum A { X = B.Y + C.Z }\n\
         enum B { Y = C.Z + A.X }\n\
         enum C { Z = A.X + B.Y }",
    );
    // All are circular — no crash is the critical invariant
    let _ = diags;
}

/// Const enum version of three-way interleaved references should emit TS2474.
#[test]
fn const_enum_three_way_interleaved_references() {
    let diags = check_source_diagnostics(
        "const enum A { X = B.Y + C.Z }\n\
         const enum B { Y = C.Z + A.X }\n\
         const enum C { Z = A.X + B.Y }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum interleaved three-way cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Multiple members in each enum, some cycling and some valid.
/// Valid members must still resolve correctly despite cycling siblings.
#[test]
fn enum_multiple_members_mixed_cycle_and_valid() {
    let diags = check_source_diagnostics(
        "enum A { V1 = 1, V2 = B.W2, V3 = 3 }\n\
         enum B { W1 = 10, W2 = C.X2, W3 = 30 }\n\
         enum C { X1 = 100, X2 = A.V2, X3 = 300 }",
    );
    // V2 -> W2 -> X2 -> V2 is a cycle. V1, V3, W1, W3, X1, X3 are fine.
    let _ = diags;
}

/// Const enum with multiple members — cycling members get TS2474, valid ones don't.
#[test]
fn const_enum_multiple_members_mixed_cycle_and_valid() {
    let diags = check_source_diagnostics(
        "const enum A { V1 = 1, V2 = B.W2, V3 = 3 }\n\
         const enum B { W1 = 10, W2 = C.X2, W3 = 30 }\n\
         const enum C { X1 = 100, X2 = A.V2, X3 = 300 }",
    );
    // Cycling members (V2, W2, X2) should get TS2474
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for cycling const enum members, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Six enums in a ring: A → B → C → D → E → F → A.
/// Must not stack-overflow.
#[test]
fn enum_six_way_ring_cycle() {
    let diags = check_source_diagnostics(
        "enum A { V = B.V }\n\
         enum B { V = C.V }\n\
         enum C { V = D.V }\n\
         enum D { V = E.V }\n\
         enum E { V = F.V }\n\
         enum F { V = A.V }",
    );
    let _ = diags;
}

/// Const enum six-way ring should emit TS2474.
#[test]
fn const_enum_six_way_ring_cycle() {
    let diags = check_source_diagnostics(
        "const enum A { V = B.V }\n\
         const enum B { V = C.V }\n\
         const enum C { V = D.V }\n\
         const enum D { V = E.V }\n\
         const enum E { V = F.V }\n\
         const enum F { V = A.V }",
    );
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for six-way const enum ring, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Nested auto-increment chains through multiple enums
// =========================================================================

/// Auto-increment members depending on cycling explicit initializers across enums.
/// `enum A { X = B.Z, Y }; enum B { W = A.Y, Z }`.
/// A.X depends on B.Z (auto-inc from B.W), B.W depends on A.Y (auto-inc from A.X).
/// This is an indirect cycle through auto-increment. Must not stack-overflow.
#[test]
fn enum_auto_increment_indirect_cycle_two_enums() {
    let diags = check_source_diagnostics("enum A { X = B.Z, Y }\nenum B { W = A.Y, Z }");
    let _ = diags;
}

/// Three enums with auto-increment members cycling indirectly.
/// `enum A { X = B.Z, Y }; enum B { W = C.Z, Z }; enum C { W = A.Y, Z }`.
#[test]
fn enum_auto_increment_indirect_cycle_three_enums() {
    let diags = check_source_diagnostics(
        "enum A { X = B.Z, Y }\n\
         enum B { W = C.Z, Z }\n\
         enum C { W = A.Y, Z }",
    );
    let _ = diags;
}

/// Const enum with auto-increment through cycle should emit TS2474.
#[test]
fn const_enum_auto_increment_indirect_cycle() {
    let diags =
        check_source_diagnostics("const enum A { X = B.Z, Y }\nconst enum B { W = A.Y, Z }");
    // The cycle prevents evaluation → TS2474
    assert!(
        diags.iter().any(|d| d.code == 2474),
        "Expected TS2474 for const enum auto-increment cycle, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =========================================================================
// Complex mixed const/non-const patterns
// =========================================================================

/// Non-const enum references const enum which references back.
/// `enum A { X = B.Y }; const enum B { Y = A.X }`.
/// The const evaluator cannot resolve A.X (non-const enum), so B.Y fails.
/// A.X evaluation may or may not succeed depending on path.
/// Must not stack-overflow.
#[test]
fn mixed_nonconst_references_const_references_back() {
    let diags = check_source_diagnostics("enum A { X = B.Y }\nconst enum B { Y = A.X }");
    let _ = diags;
}

/// Three-way mixed cycle: const → non-const → const → first.
/// `const enum A { X = B.Y }; enum B { Y = C.Z }; const enum C { Z = A.X }`.
#[test]
fn mixed_three_way_cycle() {
    let diags = check_source_diagnostics(
        "const enum A { X = B.Y }\n\
         enum B { Y = C.Z }\n\
         const enum C { Z = A.X }",
    );
    // Must not stack-overflow.
    let _ = diags;
}

// =========================================================================
// Memoization correctness: shared references should resolve correctly
// =========================================================================

/// Multiple members reference the same external member.
/// `enum A { X = B.V, Y = B.V + 1 }; enum B { V = 42 }`.
/// Both X and Y should resolve (memoization should return cached value for B.V).
#[test]
fn enum_shared_reference_resolves_with_memoization() {
    let diags = check_source_diagnostics("enum A { X = B.V, Y = B.V + 1 }\nenum B { V = 42 }");
    // No errors expected — both should resolve
    let error_codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !error_codes.iter().any(|&c| c == 2474 || c == 2565),
        "Unexpected errors for valid shared reference, got: {:?}",
        error_codes
    );
}

/// Const enum shared reference: multiple members in A reference B.V = 10.
#[test]
fn const_enum_shared_reference_memoization() {
    let diags = check_source_diagnostics(
        "const enum B { V = 10 }\n\
         const enum A { X = B.V, Y = B.V + 1, Z = B.V * 2 }",
    );
    let ts2474 = diags.iter().filter(|d| d.code == 2474).count();
    assert_eq!(
        ts2474, 0,
        "Unexpected TS2474 for valid const enum shared reference"
    );
}

/// Fan-out pattern: many enums reference a single shared base.
/// `enum Base { V = 5 }; enum A { X = Base.V }; enum B { X = Base.V }; enum C { X = Base.V }`.
#[test]
fn enum_fan_out_shared_base() {
    let diags = check_source_diagnostics(
        "enum Base { V = 5 }\n\
         enum A { X = Base.V }\n\
         enum B { X = Base.V }\n\
         enum C { X = Base.V }",
    );
    let _ = diags;
}

/// Fan-in pattern: one enum references members from many enums.
/// `enum A { V = 1 }; enum B { V = 2 }; enum C { V = 3 }; enum D { X = A.V + B.V + C.V }`.
#[test]
fn enum_fan_in_multiple_sources() {
    let diags = check_source_diagnostics(
        "enum A { V = 1 }\n\
         enum B { V = 2 }\n\
         enum C { V = 3 }\n\
         enum D { X = A.V + B.V + C.V }",
    );
    let _ = diags;
}

// =========================================================================
// Depth guard: deeply nested non-cyclic expressions
// =========================================================================

/// Deeply nested parenthesized expression should not overflow.
/// Even without cycles, deeply nested expressions are bounded by the depth guard.
#[test]
fn enum_deeply_nested_parenthesized_no_overflow() {
    // Create a deeply nested expression: (((((...(42)...)))))
    let mut expr = "42".to_string();
    for _ in 0..150 {
        expr = format!("({})", expr);
    }
    let source = format!("enum E {{ A = {} }}", expr);
    let diags = check_source_diagnostics(&source);
    // Should not panic or overflow. The value may or may not resolve
    // depending on the depth limit, but no crash.
    let _ = diags;
}

/// Deeply nested unary expressions should not overflow.
#[test]
fn enum_deeply_nested_unary_no_overflow() {
    // Create: ~~~...~~~42
    let mut expr = "42".to_string();
    for _ in 0..150 {
        expr = format!("~{}", expr);
    }
    let source = format!("enum E {{ A = {} }}", expr);
    let diags = check_source_diagnostics(&source);
    let _ = diags;
}

// =========================================================================
// Self-referencing enum with computed property name pattern
// =========================================================================

/// `enum E { A = 1, B = E["A"] + E["A"] }` — repeated element access, same member.
/// Should resolve correctly (A is before B, no cycle).
#[test]
fn enum_repeated_element_access_same_member() {
    let diags = check_source_diagnostics("enum E { A = 1, B = E[\"A\"] + E[\"A\"] }");
    let ts2651 = diags.iter().filter(|d| d.code == 2651).count();
    let ts2565 = diags.iter().filter(|d| d.code == 2565).count();
    assert_eq!(
        ts2651, 0,
        "Unexpected TS2651 for valid repeated element access"
    );
    assert_eq!(
        ts2565, 0,
        "Unexpected TS2565 for valid repeated element access"
    );
}

/// `enum E { A = 1, B = A + A + A }` — repeated bare identifier references.
#[test]
fn enum_repeated_bare_identifier_references() {
    let diags = check_source_diagnostics("enum E { A = 1, B = A + A + A }");
    let ts2651 = diags.iter().filter(|d| d.code == 2651).count();
    let ts2565 = diags.iter().filter(|d| d.code == 2565).count();
    assert_eq!(ts2651, 0, "Unexpected TS2651 for valid repeated bare refs");
    assert_eq!(ts2565, 0, "Unexpected TS2565 for valid repeated bare refs");
}
