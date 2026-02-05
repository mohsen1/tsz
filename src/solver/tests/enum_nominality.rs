//! Unit tests for enum nominal typing in the Solver layer.
//!
//! These tests verify that TypeKey::Enum wrapper is preserved during type lowering,
//! ensuring that enum member types maintain their nominal identity.

use crate::solver::compat::CompatChecker;
use crate::solver::def::DefId;
use crate::solver::types::{LiteralValue, TypeKey};
use crate::solver::{TypeId, TypeInterner};

/// Test that TypeKey::Enum wrapper is created for enum members.
#[test]
fn test_enum_member_typekey_wrapper() {
    let interner = TypeInterner::new();

    // Create an enum type with member
    let enum_def_id = DefId(42);
    let literal_zero = interner.literal_number(0.0);

    // Create TypeKey::Enum(member_def_id, literal_type)
    let member_type = interner.intern(TypeKey::Enum(enum_def_id, literal_zero));

    // Verify the type is TypeKey::Enum
    if let Some(TypeKey::Enum(def_id, inner)) = interner.lookup(member_type) {
        assert_eq!(def_id.0, enum_def_id.0, "Enum def_id should be preserved");
        // Inner type should be the literal
        assert_eq!(
            inner, literal_zero,
            "Inner type should be the literal value"
        );
    } else {
        panic!(
            "Expected TypeKey::Enum, got {:?}",
            interner.lookup(member_type)
        );
    }
}

/// Test that different enum members have different TypeKey::Enum types.
#[test]
fn test_different_enum_members_different_types() {
    let interner = TypeInterner::new();

    let enum_def_id = DefId(42);
    let literal_zero = interner.literal_number(0.0);
    let literal_one = interner.literal_number(1.0);

    let member_a = interner.intern(TypeKey::Enum(enum_def_id, literal_zero));
    let member_b = interner.intern(TypeKey::Enum(enum_def_id, literal_one));

    // They should be different types (different inner literals)
    assert_ne!(
        member_a, member_b,
        "Different enum members should have different types"
    );

    // But both should be TypeKey::Enum with same DefId
    if let (Some(TypeKey::Enum(def_a, _)), Some(TypeKey::Enum(def_b, _))) =
        (interner.lookup(member_a), interner.lookup(member_b))
    {
        assert_eq!(def_a.0, enum_def_id.0);
        assert_eq!(def_b.0, enum_def_id.0);
    } else {
        panic!("Both should be TypeKey::Enum");
    }
}

/// Test that enum members from different enums have different DefIds.
#[test]
fn test_different_enums_different_defids() {
    let interner = TypeInterner::new();

    let enum_e_def = DefId(42);
    let enum_f_def = DefId(43);
    let literal_zero = interner.literal_number(0.0);

    let member_e = interner.intern(TypeKey::Enum(enum_e_def, literal_zero));
    let member_f = interner.intern(TypeKey::Enum(enum_f_def, literal_zero));

    // Should be different types (different DefIds)
    assert_ne!(
        member_e, member_f,
        "Enum members from different enums should have different types"
    );

    // Both should be TypeKey::Enum but with different DefIds
    if let (Some(TypeKey::Enum(def_e, _)), Some(TypeKey::Enum(def_f, _))) =
        (interner.lookup(member_e), interner.lookup(member_f))
    {
        assert_eq!(def_e.0, enum_e_def.0);
        assert_eq!(def_f.0, enum_f_def.0);
        assert_ne!(def_e, def_f, "DefIds should differ");
    } else {
        panic!("Both should be TypeKey::Enum");
    }
}

/// Test that TypeKey::Enum preserves literal type information.
#[test]
fn test_enum_preserves_literal_type() {
    let interner = TypeInterner::new();

    let enum_def = DefId(42);

    // Numeric literal member
    let num_literal = interner.literal_number(42.0);
    let num_member = interner.intern(TypeKey::Enum(enum_def, num_literal));

    if let Some(TypeKey::Enum(_, inner)) = interner.lookup(num_member) {
        assert_eq!(
            inner, num_literal,
            "Numeric enum member should preserve number literal"
        );
    } else {
        panic!("Expected TypeKey::Enum");
    }

    // String literal member
    let str_literal = interner.literal_string("hello");
    let str_member = interner.intern(TypeKey::Enum(enum_def, str_literal));

    if let Some(TypeKey::Enum(_, inner)) = interner.lookup(str_member) {
        if let Some(TypeKey::Literal(LiteralValue::String(s))) = interner.lookup(inner) {
            assert_eq!(
                interner.string_interner.resolve(s).as_ref(),
                "hello",
                "String enum member should preserve string literal"
            );
        } else {
            panic!("Inner should be a string literal");
        }
    } else {
        panic!("Expected TypeKey::Enum");
    }
}

/// Test that unwrapped literals don't have nominal identity.
#[test]
fn test_unwrapped_literals_no_nominality() {
    let interner = TypeInterner::new();

    // Just the literal, no TypeKey::Enum wrapper
    let literal_zero = interner.literal_number(0.0);
    let literal_one = interner.literal_number(1.0);

    // These should be different TypeIds (different literal values)
    // The key point: no TypeKey::Enum wrapper means no nominal identity
    assert_ne!(
        literal_zero, literal_one,
        "Different literal values have different types"
    );

    // Verify they're just TypeKey::Literal, not TypeKey::Enum
    assert!(matches!(
        interner.lookup(literal_zero),
        Some(TypeKey::Literal(_))
    ));
    assert!(matches!(
        interner.lookup(literal_one),
        Some(TypeKey::Literal(_))
    ));
}

/// Test that TypeKey::Enum with same DefId but different literals are different types.
#[test]
fn test_same_enum_different_members_different() {
    let interner = TypeInterner::new();

    let enum_def = DefId(42);
    let literal_a = interner.literal_number(0.0);
    let literal_b = interner.literal_number(1.0);

    let member_a = interner.intern(TypeKey::Enum(enum_def, literal_a));
    let member_b = interner.intern(TypeKey::Enum(enum_def, literal_b));

    // Same enum, different members → different types
    assert_ne!(
        member_a, member_b,
        "Same enum, different members should have different types"
    );

    // Verify structure
    if let (Some(TypeKey::Enum(def_a, inner_a)), Some(TypeKey::Enum(def_b, inner_b))) =
        (interner.lookup(member_a), interner.lookup(member_b))
    {
        assert_eq!(def_a.0, enum_def.0);
        assert_eq!(def_b.0, enum_def.0);
        assert_eq!(def_a, def_b, "Same DefId");
        assert_ne!(inner_a, inner_b, "Different inner literals");
    } else {
        panic!("Both should be TypeKey::Enum");
    }
}

/// Test that enum members from different enums are NOT assignable (nominal typing).
#[test]
fn test_enum_nominal_typing_different_enums() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let enum_a_def = DefId(42);
    let enum_b_def = DefId(43);
    let literal_zero = interner.literal_number(0.0);

    // Create EnumA.X and EnumB.Y with same value but different DefIds
    let enum_a_x = interner.intern(TypeKey::Enum(enum_a_def, literal_zero));
    let enum_b_y = interner.intern(TypeKey::Enum(enum_b_def, literal_zero));

    // Should NOT be assignable (different DefIds = nominal mismatch)
    assert!(
        !checker.is_assignable(enum_a_x, enum_b_y),
        "EnumA.X should NOT be assignable to EnumB.Y (nominal typing)"
    );
}

/// Test that enum members from the SAME enum ARE assignable.
#[test]
fn test_enum_nominal_typing_same_enum() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let enum_def = DefId(42);
    let literal_zero = interner.literal_number(0.0);
    let literal_one = interner.literal_number(1.0);

    // Create EnumA.X and EnumA.Y
    let enum_a_x = interner.intern(TypeKey::Enum(enum_def, literal_zero));
    let enum_a_y = interner.intern(TypeKey::Enum(enum_def, literal_one));

    // Should NOT be assignable (different members of same enum)
    // In TypeScript, enum member X is not assignable to member Y
    assert!(
        !checker.is_assignable(enum_a_x, enum_a_y),
        "EnumA.X should NOT be assignable to EnumA.Y (different members)"
    );
}

/// Test that enum members ARE assignable to number in Solver layer (structural).
/// Note: The Checker layer implements Rule #7 (numeric enums) with is_numeric_enum
/// to prevent number <-> enum assignability when appropriate. The Solver layer
/// defaults to structural checking when it lacks checker context.
#[test]
fn test_enum_member_assignable_to_number_structural() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let enum_def = DefId(42);
    let literal_zero = interner.literal_number(0.0);

    let enum_member = interner.intern(TypeKey::Enum(enum_def, literal_zero));

    // In the Solver layer without is_numeric_enum context,
    // we fall back to structural checking: the inner literal 0 IS assignable to number
    assert!(
        checker.is_assignable(enum_member, TypeId::NUMBER),
        "Enum member should be assignable to number via structural checking (inner literal 0 is a number)"
    );
}

/// Test that number is NOT assignable to enum type in Solver layer.
/// Note: The Checker layer implements Rule #7 (numeric enums) with is_numeric_enum.
#[test]
fn test_number_not_assignable_to_enum_member() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let enum_def = DefId(42);
    let literal_zero = interner.literal_number(0.0);

    let enum_member = interner.intern(TypeKey::Enum(enum_def, literal_zero));

    // In the Solver layer without is_numeric_enum context,
    // number is NOT assignable to enum types
    assert!(
        !checker.is_assignable(TypeId::NUMBER, enum_member),
        "Number should NOT be assignable to enum member without numeric enum context"
    );
}
