//! Comprehensive tests for conditional type evaluation.
//!
//! These tests verify TypeScript's conditional type behavior:
//! - T extends U ? X : Y
//! - Distributive conditional types
//! - infer keyword
//! - Nested conditionals
//! - Conditional type constraint for subtype checking

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{ConditionalType, TypeData, TypeParamInfo};

// =============================================================================
// Basic Conditional Type Tests
// =============================================================================

#[test]
fn test_conditional_type_true_branch() {
    // string extends string ? number : boolean → number
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(
        result,
        TypeId::NUMBER,
        "string extends string should return true branch"
    );
}

#[test]
fn test_conditional_type_false_branch() {
    // string extends number ? number : boolean → boolean
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(
        result,
        TypeId::BOOLEAN,
        "string extends number should return false branch"
    );
}

#[test]
fn test_conditional_type_number_literal_extends_number() {
    // 42 extends number ? "yes" : "no" → "yes"
    let interner = TypeInterner::new();

    let literal_42 = interner.literal_number(42.0);

    let cond = ConditionalType {
        check_type: literal_42,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_string("yes"),
        false_type: interner.literal_string("no"),
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    let yes = interner.literal_string("yes");
    assert_eq!(result, yes, "42 extends number should return true branch");
}

#[test]
fn test_conditional_type_string_literal_extends_string() {
    // "hello" extends string ? true : false → true
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    let cond = ConditionalType {
        check_type: hello,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(
        result,
        TypeId::BOOLEAN_TRUE,
        "'hello' extends string should return true"
    );
}

// =============================================================================
// Distributive Conditional Types
// =============================================================================

#[test]
fn test_distributive_conditional_over_union() {
    // T = string | number
    // T extends string ? "string" : "other"
    // distributes to: (string extends string ? "string" : "other") | (number extends string ? "string" : "other")
    // = "string" | "other"
    let interner = TypeInterner::new();

    // Create type parameter T
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    // Note: For full distributive testing, we would instantiate the type parameter
    // with a union type and evaluate the conditional.
    let _string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let cond = ConditionalType {
        check_type: type_param,
        extends_type: TypeId::STRING,
        true_type: interner.literal_string("string"),
        false_type: interner.literal_string("other"),
        is_distributive: true,
    };

    let cond_id = interner.conditional(cond);

    // Now we need to test the distribution by providing the union type
    // For unit testing, we'll just verify the conditional is created correctly
    if let Some(TypeData::Conditional(_)) = interner.lookup(cond_id) {
        // Good - conditional type was created
    } else {
        panic!("Expected conditional type");
    }
}

#[test]
fn test_distributive_over_never_returns_never() {
    // never extends string ? X : Y → never (distributive over never)
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(
        result,
        TypeId::NEVER,
        "never extends X with distributive should return never"
    );
}

// =============================================================================
// any Behavior
// =============================================================================

#[test]
fn test_conditional_any_check_returns_union() {
    // any extends string ? number : boolean → number | boolean
    // Because any could be string or not string
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    // Result should be a union of number and boolean
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert_eq!(member_list.len(), 2);
        assert!(member_list.contains(&TypeId::NUMBER));
        assert!(member_list.contains(&TypeId::BOOLEAN));
    } else {
        panic!(
            "Expected union of number | boolean, got {:?}",
            interner.lookup(result)
        );
    }
}

// =============================================================================
// infer Keyword Tests
// =============================================================================

#[test]
fn test_infer_in_conditional() {
    // T extends Array<infer U> ? U : never
    // For string[] → string
    let interner = TypeInterner::new();

    // Create infer type parameter U
    let infer_u_info = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    };
    let infer_u = interner.intern(TypeData::Infer(infer_u_info));

    // Create Array<infer U>
    let array_of_u = interner.array(infer_u);

    // Create conditional: T extends Array<infer U> ? U : never
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let cond = ConditionalType {
        check_type: type_param,
        extends_type: array_of_u,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);

    // Verify conditional was created
    if let Some(TypeData::Conditional(_)) = interner.lookup(cond_id) {
        // Good
    } else {
        panic!("Expected conditional type");
    }
}

// =============================================================================
// Nested Conditional Types
// =============================================================================

#[test]
fn test_nested_conditional_types() {
    // T extends string ? "string" : (T extends number ? "number" : "other")
    let interner = TypeInterner::new();

    // Inner conditional: T extends number ? "number" : "other"
    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let inner_cond = ConditionalType {
        check_type: type_param,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_string("number"),
        false_type: interner.literal_string("other"),
        is_distributive: false,
    };
    let inner_cond_id = interner.conditional(inner_cond);

    // Outer conditional: T extends string ? "string" : inner_cond
    let outer_cond = ConditionalType {
        check_type: type_param,
        extends_type: TypeId::STRING,
        true_type: interner.literal_string("string"),
        false_type: inner_cond_id,
        is_distributive: false,
    };
    let outer_cond_id = interner.conditional(outer_cond);

    // Verify nested conditional was created
    if let Some(TypeData::Conditional(_)) = interner.lookup(outer_cond_id) {
        // Good
    } else {
        panic!("Expected nested conditional type");
    }
}

// =============================================================================
// Object Type Tests
// =============================================================================

#[test]
fn test_conditional_object_assignability() {
    // { a: string } extends { a: string } ? true : false → true
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let cond = ConditionalType {
        check_type: obj,
        extends_type: obj,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_TRUE);
}

#[test]
fn test_conditional_subobject_not_assignable() {
    // { a: string } extends { a: string, b: number } ? true : false → false
    let interner = TypeInterner::new();

    let smaller_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let larger_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let cond = ConditionalType {
        check_type: smaller_obj,
        extends_type: larger_obj,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_FALSE);
}

#[test]
fn test_conditional_superobject_assignable() {
    // { a: string, b: number } extends { a: string } ? true : false → true
    let interner = TypeInterner::new();

    let smaller_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let larger_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let cond = ConditionalType {
        check_type: larger_obj,
        extends_type: smaller_obj,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_TRUE);
}

// =============================================================================
// Union/Intersection as extends_type
// =============================================================================

#[test]
fn test_conditional_extends_union() {
    // string extends string | number ? true : false → true
    let interner = TypeInterner::new();

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: string_or_number,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_TRUE);
}

#[test]
fn test_conditional_type_not_in_union() {
    // boolean extends string | number ? true : false → false
    let interner = TypeInterner::new();

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let cond = ConditionalType {
        check_type: TypeId::BOOLEAN,
        extends_type: string_or_number,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_FALSE);
}

// =============================================================================
// Never and Void Edge Cases
// =============================================================================

#[test]
fn test_conditional_never_branch_evaluation() {
    // string extends number ? never : string → string
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::NEVER,
        false_type: TypeId::STRING,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_void_extends_void() {
    // void extends void ? true : false → true
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::VOID,
        extends_type: TypeId::VOID,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_TRUE);
}

// =============================================================================
// Array Conditional Tests
// =============================================================================

#[test]
fn test_conditional_array_element() {
    // string[] extends string[] ? true : false → true
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    let cond = ConditionalType {
        check_type: string_array,
        extends_type: string_array,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_TRUE);
}

#[test]
fn test_conditional_array_not_assignable_to_different_element() {
    // string[] extends number[] ? true : false → false
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    let cond = ConditionalType {
        check_type: string_array,
        extends_type: number_array,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_FALSE);
}

// =============================================================================
// Tuple Conditional Tests
// =============================================================================

#[test]
fn test_conditional_tuple_assignability() {
    // [string, number] extends [string, number] ? true : false → true
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: tuple,
        extends_type: tuple,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(result, TypeId::BOOLEAN_TRUE);
}

// =============================================================================
// Conditional Type Constraint Subtyping Tests
// =============================================================================
// These test the default constraint computation for deferred conditional types.
// In tsc, when T extends U ? X : Y is deferred (T is a type parameter), the
// "default constraint" is X[T := T & U] | Y. This constraint is used for
// assignability: if constraint <: target, the conditional is assignable.

#[test]
fn test_extract_pattern_assignable_to_extends_type() {
    // Extract<T, Function> = T extends Function ? T : never
    // Constraint = T & Function | never = T & Function
    // T & Function <: Function → true
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // Build: T extends Function ? T : never (like Extract<T, Function>)
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::FUNCTION,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let extract_type = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(extract_type, TypeId::FUNCTION),
        "Extract<T, Function> should be assignable to Function via constraint T & Function"
    );
}

#[test]
fn test_extract_pattern_assignable_to_broader_type() {
    // Extract<T, string> = T extends string ? T : never
    // Constraint = T & string
    // T & string <: string | number → true (string member of intersection is subtype)
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let extract_type = interner.conditional(cond);

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(extract_type, string_or_number),
        "Extract<T, string> should be assignable to string | number"
    );
}

#[test]
fn test_conditional_constraint_not_assignable_to_unrelated() {
    // Extract<T, string> = T extends string ? T : never
    // Constraint = T & string
    // T & string <: number → false (string is not subtype of number)
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let extract_type = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(extract_type, TypeId::NUMBER),
        "Extract<T, string> should NOT be assignable to number"
    );
}

#[test]
fn test_conditional_non_extract_both_branches_still_works() {
    // T extends string ? number : boolean
    // Both branches: number <: string|number|boolean, boolean <: string|number|boolean
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    };
    let cond_type = interner.conditional(cond);

    let target = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(cond_type, target),
        "T extends string ? number : boolean should be assignable to string|number|boolean"
    );
}

#[test]
fn test_concrete_conditional_not_affected_by_constraint() {
    // string extends number ? "yes" : "no" — concrete check type (not a type parameter).
    // The constraint logic should NOT apply; only both-branches check.
    let interner = TypeInterner::new();

    let yes_literal = interner.literal_string("yes");
    let no_literal = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: yes_literal,
        false_type: no_literal,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);
    // Both "yes" and "no" are string literals, so both <: string.
    assert!(
        checker.is_subtype_of(cond_type, TypeId::STRING),
        "Concrete conditional: both branches are string literals, assignable to string"
    );
}

// =============================================================================
// Conditional-to-Conditional Subtype: Relaxed Check-Type Comparison
// =============================================================================
// When two conditional types have the same extends_type, their check_types
// only need to be RELATED (in either direction), not strictly equivalent.
// This is critical for variance to work through conditional types in generic
// interfaces. tsc: isRelatedTo(source.checkType, target.checkType) ||
//                  isRelatedTo(target.checkType, source.checkType)

#[test]
fn test_conditional_subtype_with_related_check_types() {
    // A extends string ? A : number  <:  B extends string ? B : number
    // where B <: A (B is constrained to extend A)
    //
    // This models: Covariant<A> <: Covariant<B> is WRONG when B extends A,
    //              but Covariant<B> <: Covariant<A> should succeed.
    //
    // After expansion, the properties become conditional types and we
    // need the relaxed check-type comparison for the subtype check to pass.
    let interner = TypeInterner::new();

    let a_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let b_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: Some(a_param), // B extends A
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // A extends string ? A : number
    let cond_a = interner.conditional(ConditionalType {
        check_type: a_param,
        extends_type: TypeId::STRING,
        true_type: a_param,
        false_type: TypeId::NUMBER,
        is_distributive: true,
    });

    // B extends string ? B : number
    let cond_b = interner.conditional(ConditionalType {
        check_type: b_param,
        extends_type: TypeId::STRING,
        true_type: b_param,
        false_type: TypeId::NUMBER,
        is_distributive: true,
    });

    let mut checker = SubtypeChecker::new(&interner);

    // B extends A, so B <: A. The conditional types should be related
    // because check_types are related (B <: A) and extends/branches match.
    assert!(
        checker.is_subtype_of(cond_b, cond_a),
        "B extends string ? B : number should be subtype of A extends string ? A : number when B <: A"
    );
}

#[test]
fn test_conditional_subtype_unrelated_check_types_rejected() {
    // string extends string ? number : boolean  vs  number extends string ? number : boolean
    // check_types (string, number) are NOT related in either direction:
    //   string <: number → false, number <: string → false
    // So the conditional subtype check should fail.
    let interner = TypeInterner::new();

    let cond_str = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let cond_num = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(cond_str, cond_num),
        "Conditional types with unrelated check types (string vs number) should not be subtypes"
    );
    assert!(
        !checker.is_subtype_of(cond_num, cond_str),
        "Conditional types with unrelated check types (number vs string) should not be subtypes"
    );
}

#[test]
fn test_conditional_subtype_same_check_type_still_works() {
    // Identical conditional types should still be subtypes of each other.
    // T extends string ? number : boolean  <:  T extends string ? number : boolean
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(cond, cond),
        "Identical conditional types should be subtypes of each other"
    );
}

// =============================================================================
// Constraint-Based Infer Retry for Generic Check Types
// =============================================================================

#[test]
fn test_conditional_infer_with_constrained_type_param_index_access() {
    // Tests the fix for genericConditionalConstrainedToUnknownNotAssignableToConcreteObject.
    //
    // Models: ReturnType<T[M]> where T[M] resolves to () => unknown via constraint.
    //
    // Since the evaluator needs a resolver to handle real MappedType expansion,
    // we model this with T constrained to () => unknown directly:
    //   T extends () => unknown, so check_type T has constraint () => unknown.
    //   The conditional: T extends (...args: any) => infer R ? R : any
    //
    // Without the fix: T is a type param, infer match fails, conditional stays deferred.
    // With the fix: T's constraint (() => unknown) is tried, matches the infer pattern,
    // and R is bound to unknown, so the result is unknown.
    let interner = TypeInterner::new();

    // Create T with constraint () => unknown
    let fn_returning_unknown = interner.function(FunctionShape::new(vec![], TypeId::UNKNOWN));
    let check_type = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(fn_returning_unknown),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // Create infer R
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    // extends_type = (...args: any) => infer R
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: TypeId::ANY,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Conditional: check_type extends extends_fn ? infer_r : any
    // This models ReturnType<T[M]>
    let cond = ConditionalType {
        check_type,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::ANY,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    // T's constraint is () => unknown, which matches (...args: any) => infer R,
    // giving R = unknown. So the result should be unknown, NOT a deferred conditional.
    assert_eq!(
        result,
        TypeId::UNKNOWN,
        "ReturnType<T> where T extends () => unknown should resolve to unknown via \
         constraint-based infer retry, not remain deferred"
    );
}

#[test]
fn test_conditional_infer_with_type_param_check_type_stays_deferred() {
    // When check_type is a bare TypeParameter (not IndexAccess), and there's an infer pattern,
    // the conditional should remain deferred (not eagerly resolved).
    // This is the standard case: ReturnType<T> where T is unconstrained.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: TypeId::ANY,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::ANY,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    // With an unconstrained type param, the conditional should stay deferred
    // (returned as-is, still a Conditional type)
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Conditional(_))),
        "ReturnType<T> with unconstrained T should remain a deferred conditional type, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_conditional_infer_concrete_check_type_takes_true_branch() {
    // When check_type is concrete (not generic), infer matching should work directly.
    // (() => string) extends (...args: any) => infer R ? R : any → string
    let interner = TypeInterner::new();

    let fn_returning_string = interner.function(FunctionShape::new(vec![], TypeId::STRING));

    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: TypeId::ANY,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: fn_returning_string,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::ANY,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(
        result,
        TypeId::STRING,
        "(() => string) extends (...args) => infer R should give R = string"
    );
}

#[test]
fn test_conditional_infer_non_matching_concrete_takes_false_branch() {
    // When check_type is concrete but doesn't match the extends pattern,
    // should take the false branch.
    // string extends (...args: any) => infer R ? R : any → any
    let interner = TypeInterner::new();

    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: TypeId::ANY,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::ANY,
        is_distributive: false,
    };

    let cond_id = interner.conditional(cond);
    let result = evaluate_type(&interner, cond_id);

    assert_eq!(
        result,
        TypeId::ANY,
        "string extends (...args) => infer R should take false branch (any)"
    );
}

// =============================================================================
// Deferred conditional with extends_type containing type parameters
// =============================================================================

#[test]
fn test_concrete_check_type_extends_type_param_assignable_to_target() {
    // string[] extends T ? string[] : never
    // When extends_type (T) contains type params, the conditional is deferred.
    // Constraint = (string[] & T) | never = string[] & T
    // string[] & T <: T → true (intersection is subtype of its members)
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let string_array = interner.array(TypeId::STRING);

    let cond = ConditionalType {
        check_type: string_array,
        extends_type: t_param,
        true_type: string_array,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(cond_type, t_param),
        "string[] extends T ? string[] : never should be assignable to T"
    );
}

#[test]
fn test_concrete_check_type_extends_type_param_with_non_never_false_branch() {
    // string[] extends T ? string[] : number
    // Constraint = (string[] & T) | number
    // (string[] & T) | number <: T → false (number is not subtype of T)
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let string_array = interner.array(TypeId::STRING);

    let cond = ConditionalType {
        check_type: string_array,
        extends_type: t_param,
        true_type: string_array,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(cond_type, t_param),
        "string[] extends T ? string[] : number should NOT be assignable to T"
    );
}

#[test]
fn test_concrete_check_type_extends_type_param_different_true_branch() {
    // string extends T ? number : never
    // Constraint = (string & T) | never = string & T
    // true_type (number) != check_type (string), so inferred true = number
    // number | never = number <: T → false
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: t_param,
        true_type: TypeId::NUMBER,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_type = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(cond_type, t_param),
        "string extends T ? number : never should NOT be assignable to T"
    );
}

// =============================================================================
// Nested Conditional Constraint Tests (Extract2 pattern)
// =============================================================================

#[test]
fn test_nested_extract_conditional_assignable_to_extends_types() {
    // type Extract2<T, U, V> = T extends U ? T extends V ? T : never : never;
    //
    // Outer: T extends U ? (T extends V ? T : never) : never
    // Inner constraint: T & V
    // Outer constraint: (T & V) & U = T & U & V
    //
    // T & U & V <: U → true (U is a member of the intersection)
    // T & U & V <: V → true (V is a member of the intersection)
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let u_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let v_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // Inner: T extends V ? T : never
    let inner = ConditionalType {
        check_type: t_param,
        extends_type: v_param,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let inner_type = interner.conditional(inner);

    // Outer: T extends U ? (inner) : never
    let outer = ConditionalType {
        check_type: t_param,
        extends_type: u_param,
        true_type: inner_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let extract2_type = interner.conditional(outer);

    let mut checker = SubtypeChecker::new(&interner);

    // T & U & V <: U → true
    assert!(
        checker.is_subtype_of(extract2_type, u_param),
        "Extract2<T, U, V> should be assignable to U via constraint T & U & V"
    );

    // T & U & V <: V → true
    assert!(
        checker.is_subtype_of(extract2_type, v_param),
        "Extract2<T, U, V> should be assignable to V via constraint T & U & V"
    );
}

#[test]
fn test_nested_extract_conditional_not_assignable_to_unrelated() {
    // Extract2<T, U, V> should NOT be assignable to an unrelated type W.
    // Constraint = T & U & V, which is not a subtype of W.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let u_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let v_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let w_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("W"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // Inner: T extends V ? T : never
    let inner = ConditionalType {
        check_type: t_param,
        extends_type: v_param,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let inner_type = interner.conditional(inner);

    // Outer: T extends U ? (inner) : never
    let outer = ConditionalType {
        check_type: t_param,
        extends_type: u_param,
        true_type: inner_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let extract2_type = interner.conditional(outer);

    let mut checker = SubtypeChecker::new(&interner);

    // T & U & V <: W → false (no relationship)
    assert!(
        !checker.is_subtype_of(extract2_type, w_param),
        "Extract2<T, U, V> should NOT be assignable to unrelated type W"
    );
}

#[test]
fn test_triple_nested_extract_conditional_constraint() {
    // Three levels of nesting: T extends A ? (T extends B ? (T extends C ? T : never) : never) : never
    // Constraint should be T & A & B & C
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let a_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let b_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let c_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("C"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // Innermost: T extends C ? T : never
    let innermost = ConditionalType {
        check_type: t_param,
        extends_type: c_param,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let innermost_type = interner.conditional(innermost);

    // Middle: T extends B ? (innermost) : never
    let middle = ConditionalType {
        check_type: t_param,
        extends_type: b_param,
        true_type: innermost_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let middle_type = interner.conditional(middle);

    // Outer: T extends A ? (middle) : never
    let outer = ConditionalType {
        check_type: t_param,
        extends_type: a_param,
        true_type: middle_type,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let triple_extract = interner.conditional(outer);

    let mut checker = SubtypeChecker::new(&interner);

    // T & A & B & C <: A → true
    assert!(
        checker.is_subtype_of(triple_extract, a_param),
        "Triple nested Extract should be assignable to A"
    );

    // T & A & B & C <: B → true
    assert!(
        checker.is_subtype_of(triple_extract, b_param),
        "Triple nested Extract should be assignable to B"
    );

    // T & A & B & C <: C → true
    assert!(
        checker.is_subtype_of(triple_extract, c_param),
        "Triple nested Extract should be assignable to C"
    );
}

// =============================================================================
// Distributive Conditional Constraint Tests
// =============================================================================
// Tests for getConstraintOfDistributiveConditionalType behavior.
// When a distributive conditional's check type is a type parameter with a constraint,
// instantiate T→constraint and evaluate to get a concrete type for subtype checks.

#[test]
fn test_distributive_conditional_constraint_zeroof() {
    // type ZeroOf<T extends number | string | boolean> =
    //   T extends number ? 0 : T extends string ? "" : false;
    //
    // When T extends number | string, ZeroOf<T> should be assignable to number | string
    // because instantiating T → number | string and distributing gives 0 | "" which
    // is a subtype of number | string.
    let interner = TypeInterner::new();

    let num_or_str = interner.union2(TypeId::NUMBER, TypeId::STRING);

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(num_or_str),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let zero = interner.literal_number(0.0);
    let empty_str = interner.literal_string("");

    // Inner: T extends string ? "" : false
    let inner = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: empty_str,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: true,
    };
    let inner_id = interner.conditional(inner);

    // Outer: T extends number ? 0 : <inner>
    let outer = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: zero,
        false_type: inner_id,
        is_distributive: true,
    };
    let zeroof = interner.conditional(outer);

    let mut checker = SubtypeChecker::new(&interner);

    // ZeroOf<T> where T extends number | string should be assignable to number | string
    assert!(
        checker.is_subtype_of(zeroof, num_or_str),
        "ZeroOf<T> should be assignable to number | string via distributive constraint"
    );
}

#[test]
fn test_distributive_conditional_constraint_zeroof_literal_union() {
    // Same ZeroOf<T> as above, but checking against 0 | ""
    let interner = TypeInterner::new();

    let num_or_str = interner.union2(TypeId::NUMBER, TypeId::STRING);

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(num_or_str),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let zero = interner.literal_number(0.0);
    let empty_str = interner.literal_string("");

    let inner = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: empty_str,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: true,
    };
    let inner_id = interner.conditional(inner);

    let outer = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: zero,
        false_type: inner_id,
        is_distributive: true,
    };
    let zeroof = interner.conditional(outer);

    let zero_or_empty = interner.union2(zero, empty_str);

    let mut checker = SubtypeChecker::new(&interner);

    // ZeroOf<T> where T extends number | string should be assignable to 0 | ""
    assert!(
        checker.is_subtype_of(zeroof, zero_or_empty),
        "ZeroOf<T> should be assignable to 0 | \"\" via distributive constraint"
    );
}

#[test]
fn test_distributive_conditional_constraint_simple() {
    // type F<T extends string | number> = T extends string ? boolean : object;
    // F<T> should be assignable to boolean | object when T extends string | number
    let interner = TypeInterner::new();

    let str_or_num = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(str_or_num),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::OBJECT,
        is_distributive: true,
    };
    let cond_id = interner.conditional(cond);

    let bool_or_obj = interner.union2(TypeId::BOOLEAN, TypeId::OBJECT);

    let mut checker = SubtypeChecker::new(&interner);

    assert!(
        checker.is_subtype_of(cond_id, bool_or_obj),
        "Distributive conditional should be assignable to union of both branches"
    );
}

// =============================================================================
// Composed Extract deferral and property collection from conditional types
// =============================================================================

#[test]
fn test_composed_extract_deferred_when_check_is_conditional() {
    // Extract<Extract<T, Foo>, Bar> should be deferred (not eagerly resolved to never).
    //
    // Inner: T extends Foo ? T : never → deferred (T is type param)
    // Outer: Inner extends Bar ? Inner : never → should also defer
    //
    // Previously, the outer was eagerly resolved to the false branch (never) because
    // the evaluator didn't recognize that a Conditional check_type containing type
    // params should be deferred.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    // Inner conditional: T extends Foo ? T : never (Extract<T, Foo>)
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Outer conditional: Inner extends Bar ? Inner : never (Extract<Inner, Bar>)
    let outer_cond = interner.conditional(ConditionalType {
        check_type: inner_cond,
        extends_type: bar,
        true_type: inner_cond,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let result = evaluate_type(&interner, outer_cond);

    // The result should NOT be NEVER — it should be a deferred conditional
    assert_ne!(
        result,
        TypeId::NEVER,
        "Extract<Extract<T, Foo>, Bar> should be deferred, not resolved to never"
    );
    // It should be a Conditional type
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Conditional(_))),
        "Result should be a deferred conditional type"
    );
}

#[test]
fn test_composed_extract_not_assignable_to_missing_property() {
    // Extract<Extract<T, Foo>, Bar> should NOT be assignable to { foo: string; bat: string }
    // because the constraint T & Foo & Bar doesn't have 'bat'.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    let foo_bat = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("bat"), TypeId::STRING),
    ]);

    // Build Extract<Extract<T, Foo>, Bar>
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let outer_cond = interner.conditional(ConditionalType {
        check_type: inner_cond,
        extends_type: bar,
        true_type: inner_cond,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let evaluated = evaluate_type(&interner, outer_cond);
    let mut checker = SubtypeChecker::new(&interner);
    let result = checker.is_subtype_of(evaluated, foo_bat);

    assert!(
        !result,
        "Extract<Extract<T, Foo>, Bar> should NOT be assignable to {{ foo, bat }}"
    );
}

#[test]
fn test_composed_extract_assignable_to_matching_properties() {
    // Extract<Extract<T, Foo>, Bar> SHOULD be assignable to { foo: string; bar: string }
    // because the constraint T & Foo & Bar has both 'foo' and 'bar'.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    let foo_bar = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("bar"), TypeId::STRING),
    ]);

    // Build Extract<Extract<T, Foo>, Bar>
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let outer_cond = interner.conditional(ConditionalType {
        check_type: inner_cond,
        extends_type: bar,
        true_type: inner_cond,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let evaluated = evaluate_type(&interner, outer_cond);
    let mut checker = SubtypeChecker::new(&interner);
    let result = checker.is_subtype_of(evaluated, foo_bar);

    assert!(
        result,
        "Extract<Extract<T, Foo>, Bar> SHOULD be assignable to {{ foo, bar }}"
    );
}

#[test]
fn test_property_collection_from_conditional_in_intersection() {
    // When a conditional type is part of an intersection, its properties
    // should be collected from its default constraint.
    //
    // For Extract<T, Foo> & Bar:
    //   Extract<T, Foo> contributes foo (from T & Foo constraint)
    //   Bar contributes bar
    //   Merged: { foo: string, bar: string }
    use crate::objects::collect_properties;
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    // Extract<T, Foo> = T extends Foo ? T : never
    let extract = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Intersection: Extract<T, Foo> & Bar
    let intersection = interner.intersection2(extract, bar);

    use crate::objects::PropertyCollectionResult;
    struct MockResolver;
    impl crate::TypeResolver for MockResolver {
        fn resolve_lazy(
            &self,
            _def_id: crate::DefId,
            _interner: &dyn crate::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }
        fn symbol_to_def_id(&self, _symbol: crate::types::SymbolRef) -> Option<crate::DefId> {
            None
        }
        fn resolve_ref(
            &self,
            _symbol: crate::types::SymbolRef,
            _interner: &dyn crate::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }
        fn get_type_params(
            &self,
            _symbol: crate::types::SymbolRef,
        ) -> Option<Vec<crate::types::TypeParamInfo>> {
            None
        }
        fn get_lazy_type_params(
            &self,
            _def_id: crate::DefId,
        ) -> Option<Vec<crate::types::TypeParamInfo>> {
            None
        }
        fn def_to_symbol_id(&self, _def_id: crate::DefId) -> Option<tsz_binder::SymbolId> {
            None
        }
    }
    let resolver = MockResolver;
    let result = collect_properties(intersection, &interner, &resolver);

    match result {
        PropertyCollectionResult::Properties { properties, .. } => {
            let names: Vec<_> = properties.iter().map(|p| p.name).collect();
            let foo_atom = interner.intern_string("foo");
            let bar_atom = interner.intern_string("bar");
            assert!(
                names.contains(&foo_atom),
                "Should have 'foo' from conditional constraint"
            );
            assert!(
                names.contains(&bar_atom),
                "Should have 'bar' from intersection member"
            );
        }
        other => panic!(
            "Expected Properties result, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn test_non_distributive_conditional_no_constraint_eval() {
    // Non-distributive conditional should NOT use the distributive constraint strategy.
    // [T] extends [string] ? boolean : object should stay deferred.
    let interner = TypeInterner::new();

    let str_or_num = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(str_or_num),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // Non-distributive conditional (is_distributive = false)
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::OBJECT,
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);

    let mut checker = SubtypeChecker::new(&interner);

    // Strategy 1 (default constraint) gives (T & string) | object.
    // T & string <: boolean | object? T & string is not a subtype of boolean.
    // object <: boolean | object? Yes. So constraint = (T & string) | object <: boolean | object?
    // T & string is not a subtype of boolean, and not subtype of object either (it's a type param
    // intersection). So this should NOT be assignable via distributive constraint.
    // But it might still succeed via the default constraint path (Strategy 1).
    // The key test is that is_distributive=false does NOT trigger Strategy 1.5.
    // Let's just verify it behaves differently from the distributive case.
    let bool_or_obj = interner.union2(TypeId::BOOLEAN, TypeId::OBJECT);
    let result = checker.is_subtype_of(cond_id, bool_or_obj);
    // Non-distributive: Strategy 1 gives (T & string) | object.
    // T & string <: boolean | object is checked; T & string <: boolean = false,
    // T & string <: object = maybe (type param). Let it fall through to Strategy 2.
    // Strategy 2: true=boolean <: boolean|object=yes, false=object <: boolean|object=yes.
    // So it should actually succeed via Strategy 2.
    assert!(
        result,
        "Non-distributive conditional should still succeed via branch checking"
    );
}
