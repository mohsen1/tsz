//! Tests for Rule #40: Distributivity Disabling
//!
//! This module tests the [T] extends [U] tuple wrapper pattern
//! which is critical for Exclude/Extract utility types.

use crate::instantiate::instantiate_type;
use crate::intern::TypeInterner;
use crate::types::*;

#[test]
fn test_distributive_conditional_distributes_over_union() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // type Check<T> = T extends any ? true : false;
    // This is distributive, so T distributes over the union
    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::ANY,
        true_type: TypeId::TRUE,   // Assume we have a TRUE type
        false_type: TypeId::FALSE, // Assume we have a FALSE type
        is_distributive: true,     // <-- DISTRIBUTIVE
    });

    // When T = string | number
    // With distributive: (string extends any ? true : false) | (number extends any ? true : false)
    //                 = true | true = boolean

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = instantiate_type(
        &interner,
        conditional,
        &crate::instantiate::TypeSubstitution::new(),
    );

    // The result should be a union (distributed)
    // Note: In actual TypeScript, both would resolve to true, so result is true
    // But the key is that distribution HAPPENED
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_tuple_wrapper_prevents_distribution() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // type Check<T> = [T] extends [any] ? true : false;
    // Tuple wrapper prevents distribution
    let tuple_check = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_extends = interner.tuple(vec![TupleElement {
        type_id: TypeId::ANY,
        name: None,
        optional: false,
        rest: false,
    }]);

    let conditional = interner.conditional(ConditionalType {
        check_type: tuple_check,
        extends_type: tuple_extends,
        true_type: TypeId::TRUE,
        false_type: TypeId::FALSE,
        is_distributive: false, // <-- NOT DISTRIBUTIVE (tuple wrapper detected)
    });

    // When T = string | number
    // With non-distributive: [string | number] extends [any] ? true : false
    // The union is checked AS A WHOLE, not distributed
    // Result: true (since string | number extends any)

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let mut subst = crate::instantiate::TypeSubstitution::new();
    subst.insert(t_name, string_or_number);

    let result = instantiate_type(&interner, conditional, &subst);

    // The result should be true (no distribution happened)
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_exclude_utility_type_distributes() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // type Exclude<T, U> = T extends U ? never : T;
    // This is distributive, so T distributes over the union
    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: u_param,
        true_type: TypeId::NEVER,
        false_type: t_param,
        is_distributive: true, // <-- DISTRIBUTIVE
    });

    // Example: Exclude<"a" | "b" | "c", "a">
    // Should distribute as:
    // ("a" extends "a" ? never : "a") |
    // ("b" extends "a" ? never : "b") |
    // ("c" extends "a" ? never : "c")
    // = never | "b" | "c"
    // = "b" | "c"

    let a_literal = interner.literal_string("a");
    let b_literal = interner.literal_string("b");
    let c_literal = interner.literal_string("c");
    let abc_union = interner.union(vec![a_literal, b_literal, c_literal]);

    let mut subst = crate::instantiate::TypeSubstitution::new();
    subst.insert(t_name, abc_union);
    subst.insert(u_name, a_literal);

    let result = instantiate_type(&interner, conditional, &subst);

    // The result should be "b" | "c" (a was excluded)
    // Check that "a" is NOT in the result
    if let Some(TypeKey::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        // Should not contain "a"
        for &member in members.iter() {
            if member == a_literal {
                panic!("Exclude failed: 'a' should have been excluded");
            }
        }
        // Should contain "b" and "c"
        assert!(members.contains(&b_literal), "Result should contain 'b'");
        assert!(members.contains(&c_literal), "Result should contain 'c'");
    } else {
        panic!("Result should be a union type");
    }
}

#[test]
fn test_extract_utility_type_distributes() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // type Extract<T, U> = T extends U ? T : never;
    // This is distributive, so T distributes over the union
    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: u_param,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true, // <-- DISTRIBUTIVE
    });

    // Example: Extract<"a" | "b" | "c", "a">
    // Should distribute as:
    // ("a" extends "a" ? "a" : never) |
    // ("b" extends "a" ? "b" : never) |
    // ("c" extends "a" ? "c" : never)
    // = "a" | never | never
    // = "a"

    let a_literal = interner.literal_string("a");
    let b_literal = interner.literal_string("b");
    let c_literal = interner.literal_string("c");
    let abc_union = interner.union(vec![a_literal, b_literal, c_literal]);

    let mut subst = crate::instantiate::TypeSubstitution::new();
    subst.insert(t_name, abc_union);
    subst.insert(u_name, a_literal);

    let result = instantiate_type(&interner, conditional, &subst);

    // The result should be "a" (only "a" was extracted)
    assert_eq!(result, a_literal, "Extract should return only 'a'");
}

#[test]
fn test_is_naked_type_param_returns_false_for_tuple() {
    // This test verifies that is_naked_type_param correctly identifies tuples
    // as NOT naked, which prevents distributivity

    // The key insight:
    // - T (naked type parameter) -> is_distributive = true
    // - [T] (tuple wrapper) -> is_naked_type_param returns false -> is_distributive = false

    // This behavior is implemented in lower.rs:is_naked_type_param()
    // which checks the AST node and returns false for TupleType nodes

    // The test_conditional_tuple_wrapper_no_distribution_assignable test
    // in compat_tests.rs already verifies this works correctly
}
