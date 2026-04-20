use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
/// `BigInt` with negative value
#[test]
fn test_bigint_negative_literal() {
    let interner = TypeInterner::new();

    let neg_bigint = interner.literal_bigint_with_sign(true, "42");
    let pos_bigint = interner.literal_bigint("42");

    // Negative and positive should be different
    assert_ne!(neg_bigint, pos_bigint);

    // Negative bigint extends bigint
    let cond = ConditionalType {
        check_type: neg_bigint,
        extends_type: TypeId::BIGINT,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Symbol type doesn't extend string
#[test]
fn test_symbol_not_extends_string() {
    let interner = TypeInterner::new();

    // symbol extends string ? true : false
    let cond = ConditionalType {
        check_type: TypeId::SYMBOL,
        extends_type: TypeId::STRING,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// Symbol extends symbol
#[test]
fn test_symbol_extends_symbol() {
    let interner = TypeInterner::new();

    // symbol extends symbol ? true : false
    let cond = ConditionalType {
        check_type: TypeId::SYMBOL,
        extends_type: TypeId::SYMBOL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Unique symbol extends base symbol
#[test]
fn test_unique_symbol_extends_symbol() {
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));

    // unique symbol extends symbol ? true : false
    let cond = ConditionalType {
        check_type: unique_sym,
        extends_type: TypeId::SYMBOL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Unique symbols with different refs are distinct
#[test]
fn test_unique_symbol_distinct_refs() {
    let interner = TypeInterner::new();

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    // Different refs produce different types
    assert_ne!(sym_a, sym_b);

    // Same ref produces same type
    let sym_a_dup = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    assert_eq!(sym_a, sym_a_dup);
}

/// Unique symbol in union with base symbol
#[test]
fn test_unique_symbol_union_with_symbol() {
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let union = interner.union(vec![unique_sym, TypeId::SYMBOL]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            // Union should have 2 members (unique symbol and symbol)
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Unique symbol as union member
#[test]
fn test_unique_symbol_in_union() {
    let interner = TypeInterner::new();

    let sym1 = interner.intern(TypeData::UniqueSymbol(SymbolRef(100)));
    let sym2 = interner.intern(TypeData::UniqueSymbol(SymbolRef(101)));
    let sym3 = interner.intern(TypeData::UniqueSymbol(SymbolRef(102)));

    // Create union of unique symbols
    let union = interner.union(vec![sym1, sym2, sym3]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 3);
        }
        _ => panic!("Expected union"),
    }
}

/// Number literal type creation and comparison
#[test]
fn test_number_literal_creation() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);
    let num_42_dup = interner.literal_number(42.0);

    // Same number literal should produce same TypeId
    assert_eq!(num_42, num_42_dup);

    // Different number literals should produce different TypeIds
    let num_100 = interner.literal_number(100.0);
    assert_ne!(num_42, num_100);
}

/// Number literal extends number
#[test]
fn test_number_literal_extends_number() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);

    // 42 extends number ? true : false
    let cond = ConditionalType {
        check_type: num_42,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Number literal doesn't extend different literal
#[test]
fn test_number_literal_not_extends_different() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);
    let num_100 = interner.literal_number(100.0);

    // 42 extends 100 ? true : false
    let cond = ConditionalType {
        check_type: num_42,
        extends_type: num_100,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// Number literal union
#[test]
fn test_number_literal_union() {
    let interner = TypeInterner::new();

    let num_1 = interner.literal_number(1.0);
    let num_2 = interner.literal_number(2.0);
    let num_3 = interner.literal_number(3.0);

    let union = interner.union(vec![num_1, num_2, num_3]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 3);
        }
        _ => panic!("Expected union"),
    }
}

/// String literal type comparison
#[test]
fn test_string_literal_comparison() {
    let interner = TypeInterner::new();

    let str_hello = interner.literal_string("hello");
    let str_hello_dup = interner.literal_string("hello");

    // Same string literal should produce same TypeId
    assert_eq!(str_hello, str_hello_dup);

    // Different string literals should produce different TypeIds
    let str_world = interner.literal_string("world");
    assert_ne!(str_hello, str_world);
}

/// String literal union narrowing via conditional
#[test]
fn test_string_literal_union_conditional() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let union_ab = interner.union(vec![lit_a, lit_b]);

    // "a" extends "a" | "b" ? true : false
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: union_ab,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, interner.literal_boolean(true));

    // "c" extends "a" | "b" ? true : false
    let cond_c = ConditionalType {
        check_type: lit_c,
        extends_type: union_ab,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    let result_c = evaluate_conditional(&interner, &cond_c);
    assert_eq!(result_c, interner.literal_boolean(false));
}

/// Mixed numeric literal types (number and bigint)
#[test]
fn test_mixed_numeric_literal_types() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);
    let bigint_42 = interner.literal_bigint("42");

    // Number and bigint with same value are different types
    assert_ne!(num_42, bigint_42);

    // Create union of number and bigint literals
    let union = interner.union(vec![num_42, bigint_42]);
    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Floating point number literals
#[test]
fn test_float_number_literal() {
    let interner = TypeInterner::new();

    const APPROX_PI: f64 = 3.15;
    const APPROX_E: f64 = 2.72;

    let float_pi = interner.literal_number(APPROX_PI);
    let float_e = interner.literal_number(APPROX_E);

    assert_ne!(float_pi, float_e);

    // Float extends number
    let cond = ConditionalType {
        check_type: float_pi,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Negative number literals
#[test]
fn test_negative_number_literal() {
    let interner = TypeInterner::new();

    let neg_42 = interner.literal_number(-42.0);
    let pos_42 = interner.literal_number(42.0);

    // Negative and positive are different
    assert_ne!(neg_42, pos_42);

    // Negative number extends number
    let cond = ConditionalType {
        check_type: neg_42,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Zero and negative zero number literals
#[test]
fn test_zero_number_literal() {
    let interner = TypeInterner::new();

    let zero = interner.literal_number(0.0);
    let neg_zero = interner.literal_number(-0.0);

    // In IEEE 754, 0.0 and -0.0 are equal for comparison purposes
    // but may or may not intern to the same TypeId depending on implementation
    // The key test is that both extend number
    let cond_zero = ConditionalType {
        check_type: zero,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_zero),
        interner.literal_boolean(true)
    );

    let cond_neg = ConditionalType {
        check_type: neg_zero,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_neg),
        interner.literal_boolean(true)
    );
}

/// Boolean literal type operations
#[test]
fn test_boolean_literal_operations() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // true and false are different
    assert_ne!(lit_true, lit_false);

    // Both extend boolean
    let cond_true = ConditionalType {
        check_type: lit_true,
        extends_type: TypeId::BOOLEAN,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(evaluate_conditional(&interner, &cond_true), lit_true);

    let cond_false = ConditionalType {
        check_type: lit_false,
        extends_type: TypeId::BOOLEAN,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(evaluate_conditional(&interner, &cond_false), lit_true);
}

/// Boolean literal union equals boolean
#[test]
fn test_boolean_literal_union() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // true | false should simplify to boolean (or remain as union)
    let union = interner.union(vec![lit_true, lit_false]);

    // Either it's BOOLEAN or a union of two
    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert!(members.len() == 2);
        }
        Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => {
            // Simplified to boolean
        }
        _ => panic!("Expected union or boolean"),
    }
}

/// Intrinsic types are not equal to each other
#[test]
fn test_intrinsic_types_distinct() {
    // Verify all intrinsic types are distinct
    assert_ne!(TypeId::STRING, TypeId::NUMBER);
    assert_ne!(TypeId::NUMBER, TypeId::BOOLEAN);
    assert_ne!(TypeId::BOOLEAN, TypeId::BIGINT);
    assert_ne!(TypeId::BIGINT, TypeId::SYMBOL);
    assert_ne!(TypeId::SYMBOL, TypeId::NULL);
    assert_ne!(TypeId::NULL, TypeId::UNDEFINED);
    assert_ne!(TypeId::UNDEFINED, TypeId::VOID);
    assert_ne!(TypeId::VOID, TypeId::NEVER);
    assert_ne!(TypeId::NEVER, TypeId::ANY);
    assert_ne!(TypeId::ANY, TypeId::UNKNOWN);
    assert_ne!(TypeId::UNKNOWN, TypeId::OBJECT);
}

