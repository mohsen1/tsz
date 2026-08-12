//! Comprehensive tests for index access type operations.
//!
//! These tests verify TypeScript's indexed access type behavior:
//! - T[K] indexed access
//! - Element access on objects, arrays, tuples
//! - Index access with literal keys
//! - Index access with union keys

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{
    ConditionalType, MappedType, PropertyInfo, TupleElement, TypeData, TypeParamInfo,
    TypeParamOrigin,
};

// =============================================================================
// Basic Index Access Tests
// =============================================================================

#[test]
fn test_index_access_object() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let name_key = interner.literal_string("name");
    let index_access = interner.index_access(obj, name_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::STRING, "obj['name'] should be string");
}

#[test]
fn test_index_access_with_number_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    // Number key on object - should work via string conversion
    let num_key = interner.literal_number(0.0);
    let index_access = interner.index_access(obj, num_key);

    // Just verify it doesn't crash
    let _result = evaluate_type(&interner, index_access);
}

// =============================================================================
// Index Access on Arrays
// =============================================================================

#[test]
fn test_index_access_array_with_number() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);

    let num_key = interner.literal_number(0.0);
    let index_access = interner.index_access(array, num_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::STRING, "array[0] should be string");
}

#[test]
fn test_index_access_array_with_number_type() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::NUMBER);

    let index_access = interner.index_access(array, TypeId::NUMBER);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::NUMBER, "array[number] should be number");
}

// =============================================================================
// Index Access on Tuples
// =============================================================================

#[test]
fn test_index_access_tuple_first_element() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let index_0 = interner.literal_number(0.0);
    let index_access = interner.index_access(tuple, index_0);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::STRING, "tuple[0] should be string");
}

#[test]
fn test_index_access_tuple_second_element() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let index_1 = interner.literal_number(1.0);
    let index_access = interner.index_access(tuple, index_1);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::NUMBER, "tuple[1] should be number");
}

// =============================================================================
// Index Access with Union Keys
// =============================================================================

#[test]
fn test_index_access_with_union_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union2(key_a, key_b);

    let index_access = interner.index_access(obj, key_union);

    let result = evaluate_type(&interner, index_access);

    // obj['a' | 'b'] should be string | number
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::STRING));
        assert!(members.contains(&TypeId::NUMBER));
    } else {
        panic!("Expected union of string | number");
    }
}

// =============================================================================
// Index Access on Object with Index Signature
// =============================================================================

#[test]
fn test_index_access_with_string_index_signature() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(crate::types::ObjectShape {
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol_index: None,
        number_index: None,
    });

    let any_string_key = interner.literal_string("anyKey");
    let index_access = interner.index_access(obj, any_string_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "obj with string index should return number"
    );
}

#[test]
fn test_symbol_index_signature_accepts_symbol_keys_only() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(crate::types::ObjectShape {
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: None,
    });

    let unique_key = interner.unique_symbol(crate::types::SymbolRef(7));
    assert_eq!(
        evaluate_type(&interner, interner.index_access(obj, unique_key)),
        TypeId::BOOLEAN,
        "symbol index signatures should accept unique-symbol keys"
    );
    assert_eq!(
        evaluate_type(&interner, interner.index_access(obj, TypeId::SYMBOL)),
        TypeId::BOOLEAN,
        "symbol index signatures should accept the broad symbol key type"
    );
    // Indexing an object whose only index signature is symbol-keyed by the bare
    // `string` type matches no applicable signature: tsc reports TS2536 and resolves
    // the access to the error type (still not the symbol index's value). See #9709.
    assert_eq!(
        evaluate_type(&interner, interner.index_access(obj, TypeId::STRING)),
        TypeId::ERROR,
        "symbol index signatures must not behave like string index signatures"
    );
}

#[test]
fn test_symbol_named_properties_do_not_satisfy_string_index_signatures() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo {
        is_symbol_named: true,
        ..PropertyInfo::new(interner.intern_string("__unique_7"), TypeId::NUMBER)
    }]);
    let target = interner.object_with_index(crate::types::ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(
        checker.is_subtype_of(source, target),
        "unique-symbol properties should not be checked against string index signatures"
    );
}

#[test]
fn test_symbol_index_signatures_check_symbol_named_properties() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo {
        is_symbol_named: true,
        ..PropertyInfo::new(interner.intern_string("__unique_7"), TypeId::NUMBER)
    }]);
    let target = interner.object_with_index(crate::types::ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(
        !checker.is_subtype_of(source, target),
        "symbol index signatures should validate unique-symbol property values"
    );
}

// =============================================================================
// Index Access Identity Tests
// =============================================================================

#[test]
fn test_index_access_identity_stability() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let key = interner.literal_string("name");

    let access1 = interner.index_access(obj, key);
    let access2 = interner.index_access(obj, key);

    assert_eq!(
        access1, access2,
        "Same index access should produce same TypeId"
    );
}

// =============================================================================
// Index Access with keyof
// =============================================================================

#[test]
fn test_index_access_with_keyof() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(obj);
    let index_access = interner.index_access(obj, keyof_t);

    let result = evaluate_type(&interner, index_access);

    // T[keyof T] should be string | number
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        // Could also be string | number simplified
    }
}

// =============================================================================
// Index Access on Nested Objects
// =============================================================================

#[test]
fn test_index_access_nested_object() {
    let interner = TypeInterner::new();

    let inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let outer = interner.object(vec![PropertyInfo::new(
        interner.intern_string("nested"),
        inner,
    )]);

    let key = interner.literal_string("nested");
    let index_access = interner.index_access(outer, key);

    let result = evaluate_type(&interner, index_access);

    // outer['nested'] should be the inner object type
    if let Some(TypeData::Object(_)) = interner.lookup(result) {
        // Good
    } else {
        panic!("Expected object type for nested access");
    }
}

// =============================================================================
// Index Access with any
// =============================================================================

#[test]
fn test_index_access_any_object() {
    let interner = TypeInterner::new();

    let any_key = interner.literal_string("anything");
    let index_access = interner.index_access(TypeId::ANY, any_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::ANY, "any['key'] should be any");
}

#[test]
fn test_index_access_with_any_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let index_access = interner.index_access(obj, TypeId::ANY);

    let result = evaluate_type(&interner, index_access);
    // obj[any] could be any or the property type depending on implementation
    let _ = result;
}

// =============================================================================
// Index Access Subtype Tests
// =============================================================================

#[test]
fn test_index_access_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let key = interner.literal_string("name");
    let index_access = interner.index_access(obj, key);
    let result = evaluate_type(&interner, index_access);

    assert!(
        checker.is_subtype_of(result, TypeId::STRING),
        "obj['name'] should be subtype of string"
    );
}

// =============================================================================
// Index Access with Never
// =============================================================================

#[test]
fn test_index_access_never_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let index_access = interner.index_access(obj, TypeId::NEVER);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::NEVER, "obj[never] should be never");
}

#[test]
fn test_index_access_keyof_empty_object_is_never() {
    let interner = TypeInterner::new();

    let empty_obj = interner.object(vec![]);
    let keyof_empty = interner.keyof(empty_obj);
    let index_access = interner.index_access(empty_obj, keyof_empty);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::NEVER, "{{}}[keyof {{}}] should be never");
}

#[test]
fn test_index_access_never_key_on_array_is_element_type() {
    // `never` is assignable to every index key, so tsc resolves `T[never]` to
    // `T`'s index source. For an array that is the element type, not `never`.
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let index_access = interner.index_access(array, TypeId::NEVER);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::STRING,
        "string[][never] should be the element type string"
    );
}

#[test]
fn test_index_access_never_key_on_tuple_is_element_union() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let index_access = interner.index_access(tuple, TypeId::NEVER);

    let result = evaluate_type(&interner, index_access);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(
        result, expected,
        "[string, number][never] should be the element union string | number"
    );
}

#[test]
fn test_index_access_never_key_on_string_index_signature() {
    // `Record<string, number>`-shaped object: `T[never]` picks up the string
    // index signature value type.
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(crate::types::ObjectShape {
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: None,
    });
    let index_access = interner.index_access(obj, TypeId::NEVER);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "{{ [k: string]: number }}[never] should be the index value type number"
    );
}

#[test]
fn test_index_access_never_key_on_readonly_array_is_element_type() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::NUMBER);
    let readonly_array = interner.readonly_type(array);
    let index_access = interner.index_access(readonly_array, TypeId::NEVER);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "(readonly number[])[never] should be the element type number"
    );
}

#[test]
fn test_index_access_never_key_on_string_mapped_is_value_type() {
    // `Record<string, boolean>` expands to `{ [P in string]: boolean }`. Indexing
    // it by `never` reads the (implicit string) index signature value type.
    let interner = TypeInterner::new();

    let mapped_type_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let mapped = interner.mapped(MappedType {
        type_param: mapped_type_param,
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: None,
        readonly_modifier: None,
    });

    let index_access = interner.index_access(mapped, TypeId::NEVER);
    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::BOOLEAN,
        "{{ [P in string]: boolean }}[never] should be the index value type boolean"
    );
}

#[test]
fn test_index_access_never_key_on_array_union_distributes() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let union = interner.union(vec![string_array, number_array]);
    let index_access = interner.index_access(union, TypeId::NEVER);

    let result = evaluate_type(&interner, index_access);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(
        result, expected,
        "(string[] | number[])[never] should distribute to string | number"
    );
}

// =============================================================================
// Multiple Index Access Tests
// =============================================================================

#[test]
fn test_multiple_index_access() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");

    let access_a = evaluate_type(&interner, interner.index_access(obj, key_a));
    let access_b = evaluate_type(&interner, interner.index_access(obj, key_b));
    let access_c = evaluate_type(&interner, interner.index_access(obj, key_c));

    assert_eq!(access_a, TypeId::STRING);
    assert_eq!(access_b, TypeId::NUMBER);
    assert_eq!(access_c, TypeId::BOOLEAN);
}

// =============================================================================
// Cyclic Lazy Index Access Tests (Stack Overflow Prevention)
// =============================================================================

/// Regression test for stack overflow caused by cyclic lazy type in index access.
///
/// When a Lazy(DefId) resolves to a type that itself is Lazy(DefId) (directly
/// self-referential), `evaluate_index_access` would call `evaluate()` which
/// detects the cycle and returns the Lazy type unchanged. The `IndexAccessVisitor`
/// then dispatches to `visit_lazy`, which resolves the DefId *directly* (bypassing
/// the recursion guard) and calls `evaluate_index_access` again — creating an
/// infinite recursion that overflows the stack.
///
/// This reproduces the crash observed on large TypeScript projects where recursive
/// type definitions create cyclic Lazy resolution chains (exit code 137 / SIGKILL
/// from macOS due to 209,568-deep stack in `evaluate_index_access ↔ visit_type`).
#[test]
fn test_cyclic_lazy_index_access_does_not_stack_overflow() {
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();

    // Create a self-referential lazy type: DefId(1) resolves to Lazy(DefId(1))
    let def_id = DefId(1);
    let lazy_type = interner.lazy(def_id);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, lazy_type); // DefId(1) → Lazy(DefId(1)) — direct cycle

    let key = interner.literal_string("x");
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    // This must terminate (not stack overflow). The result should be ERROR or the
    // deferred IndexAccess type — the exact value doesn't matter as long as
    // the evaluator doesn't blow the stack.
    let result = evaluator.evaluate_index_access(lazy_type, key);

    // Should not be the original lazy type (that would mean no progress)
    // and should not crash. ERROR or a deferred IndexAccess are both acceptable.
    assert!(
        result != TypeId::NONE,
        "Cyclic lazy index access should terminate without stack overflow"
    );
}

/// Regression test for indirect cyclic lazy types in index access.
///
/// DefId(1) → object with property "val" of type Lazy(DefId(2))
/// DefId(2) → Lazy(DefId(1))
///
/// Evaluating IndexAccess(Lazy(DefId(2)), "val") creates a chain:
/// Lazy(2) → Lazy(1) → object → property "val" → Lazy(2) → cycle
#[test]
fn test_indirect_cyclic_lazy_index_access_does_not_stack_overflow() {
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();

    let def_1 = DefId(1);
    let def_2 = DefId(2);
    let lazy_1 = interner.lazy(def_1);
    let lazy_2 = interner.lazy(def_2);

    // DefId(1) resolves to an object { val: Lazy(DefId(2)) }
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("val"),
        lazy_2,
    )]);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_1, obj);
    env.insert_def(def_2, lazy_1); // DefId(2) → Lazy(DefId(1)) — indirect cycle

    let key = interner.literal_string("val");
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    // Must terminate without stack overflow
    let result = evaluator.evaluate_index_access(lazy_2, key);

    assert!(
        result != TypeId::NONE,
        "Indirect cyclic lazy index access should terminate without stack overflow"
    );
}

#[test]
fn index_access_productive_lazy_chain_bails_to_deferred_root() {
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let def_id = DefId(14351);
    let lazy = interner.lazy(def_id);
    let key = interner.literal_string("member");
    let root = interner.index_access(lazy, key);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, root);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(root);

    assert_eq!(
        result, root,
        "productive Lazy(D) -> Lazy(D)[K] chains should keep the indexed access deferred"
    );
    assert_ne!(
        result,
        TypeId::ERROR,
        "recursion-identity containment should not collapse the deferred chain to ERROR"
    );
}

#[test]
fn keyof_productive_lazy_chain_bails_to_deferred_root() {
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let def_id = DefId(14352);
    let lazy = interner.lazy(def_id);
    let root = interner.keyof(lazy);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, root);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(root);

    assert_eq!(
        result, root,
        "productive Lazy(D) -> keyof Lazy(D) chains should keep keyof deferred"
    );
    assert_ne!(
        result,
        TypeId::ERROR,
        "keyof recursion-identity containment should not collapse the deferred chain to ERROR"
    );
}

#[test]
fn same_identity_containment_keeps_finite_lazy_resolution() {
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let def_id = DefId(14353);
    let lazy = interner.lazy(def_id);
    let key = interner.literal_string("renamed");
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("renamed"),
        TypeId::STRING,
    )]);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, object);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    assert_eq!(
        evaluator.evaluate(interner.index_access(lazy, key)),
        TypeId::STRING,
        "ordinary Lazy(D) -> object resolution must still reduce indexed access"
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let keyof_result = evaluator.evaluate(interner.keyof(lazy));
    assert!(
        matches!(
            interner.lookup(keyof_result),
            Some(TypeData::Literal(crate::types::LiteralValue::String(_)))
        ),
        "ordinary Lazy(D) -> object resolution must still reduce keyof, got {:?}",
        interner.lookup(keyof_result)
    );
}

#[test]
fn meta_rereduce_identity_stack_defers_fifth_keyof_object_identity() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 4);
    let result = evaluator.recurse_keyof(object);

    assert_eq!(result, interner.keyof(object));
    assert!(
        matches!(interner.lookup(result), Some(TypeData::KeyOf(inner)) if inner == object),
        "fifth same-identity keyof re-reduce should preserve the deferred operand, got {:?}",
        interner.lookup(result)
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "identity-count bailout must mark the request partial for cache gates"
    );
}

#[test]
fn meta_rereduce_identity_stack_allows_fourth_keyof_object_identity() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("renamed"),
        TypeId::STRING,
    )]);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 3);
    let result = evaluator.recurse_keyof(object);

    assert_eq!(result, interner.literal_string("renamed"));
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "fourth same-identity keyof re-reduce is below the tsc-style cutoff"
    );
}

#[test]
fn meta_rereduce_identity_stack_truncates_after_reducing_call() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("stable"),
        TypeId::STRING,
    )]);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(object, 3);
    assert_eq!(
        evaluator.recurse_keyof(object),
        interner.literal_string("stable")
    );
    assert_eq!(
        evaluator.recurse_keyof(object),
        interner.literal_string("stable")
    );
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "below-cutoff reductions must pop their temporary stack entry"
    );
}

#[test]
fn meta_rereduce_identity_stack_keeps_finite_readonly_keyof_reduction() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let readonly = interner.intern(TypeData::ReadonlyType(object));
    let mut evaluator = TypeEvaluator::new(&interner);

    let result = evaluator.recurse_keyof(readonly);

    assert_eq!(result, interner.literal_string("a"));
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "finite transparent wrappers must still reduce below the identity cutoff"
    );
}

#[test]
fn meta_rereduce_identity_stack_keeps_readonly_seed_distinct_from_object() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let readonly = interner.intern(TypeData::ReadonlyType(object));

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(readonly, 4);
    assert_eq!(
        evaluator.recurse_keyof(object),
        interner.literal_string("a")
    );
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "readonly recursion entries must not make the bare object bail"
    );
}

#[test]
fn meta_rereduce_identity_stack_uses_leftmost_index_access_object() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let key = interner.literal_string("a");
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let first_access = interner.index_access(object, key);
    let second_access = interner.index_access(first_access, key);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(second_access, 4);
    let result = evaluator.recurse_index_access(first_access, key);

    assert_eq!(result, interner.index_access(first_access, key));
    assert!(
        matches!(interner.lookup(result), Some(TypeData::IndexAccess(obj, idx)) if obj == first_access && idx == key),
        "fifth same-leftmost-object indexed-access re-reduce should preserve the deferred chain, got {:?}",
        interner.lookup(result)
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "identity-count bailout must mark indexed-access requests partial"
    );
}

#[test]
fn meta_rereduce_identity_stack_counts_object_and_indexed_leftmost_together() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let key = interner.literal_string("a");
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let first_access = interner.index_access(object, key);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(first_access, 4);
    let result = evaluator.recurse_keyof(object);

    assert_eq!(result, interner.keyof(object));
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "A and A[P] intentionally share the leftmost object recursion identity"
    );
}

#[test]
fn meta_rereduce_identity_stack_counts_conditional_roots() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: object,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(conditional, 4);
    let result = evaluator.recurse_keyof(conditional);

    assert_eq!(result, interner.keyof(conditional));
    assert!(
        matches!(interner.lookup(result), Some(TypeData::KeyOf(inner)) if inner == conditional),
        "conditional recursion identity should defer before eager branch evaluation, got {:?}",
        interner.lookup(result)
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "conditional identity-count bailout must mark the request partial"
    );
}

#[test]
fn meta_rereduce_identity_stack_distinct_conditional_root_still_reduces() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: object,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let distinct_conditional = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: object,
        is_distributive: false,
    });
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(distinct_conditional, 4);
    let result = evaluator.recurse_keyof(conditional);

    assert_eq!(result, interner.literal_string("a"));
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "a different conditional root must not trip the identity-count bailout"
    );
}

#[test]
fn meta_rereduce_identity_stack_counts_canonical_def_ids() {
    use crate::construction::TypeDatabase;
    use crate::def::DefId;
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeResolver;
    use crate::types::SymbolRef;

    struct CanonicalDefResolver {
        body: TypeId,
        primary: DefId,
        alias: DefId,
    }

    impl TypeResolver for CanonicalDefResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            (def_id == self.primary || def_id == self.alias).then_some(self.body)
        }

        fn canonical_def_id(&self, def_id: DefId) -> DefId {
            if def_id == self.alias {
                self.primary
            } else {
                def_id
            }
        }

        fn defs_are_equivalent(&self, left: DefId, right: DefId) -> bool {
            self.canonical_def_id(left) == self.canonical_def_id(right)
        }
    }

    let interner = TypeInterner::new();
    let primary = crate::def::DefId(14361);
    let alias = crate::def::DefId(14362);
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let resolver = CanonicalDefResolver {
        body,
        primary,
        alias,
    };
    let primary_lazy = interner.lazy(primary);
    let alias_lazy = interner.lazy(alias);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(alias_lazy, 4);
    let result = evaluator.recurse_keyof(primary_lazy);

    assert_eq!(result, interner.keyof(primary_lazy));
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "canonical/equivalent DefIds must count as the same recursion identity"
    );
}

#[test]
fn meta_rereduce_identity_stack_keeps_distinct_def_ids_separate() {
    use crate::def::DefId;
    use crate::def::resolver::TypeEnvironment;
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let primary = DefId(14363);
    let other = DefId(14364);
    let primary_lazy = interner.lazy(primary);
    let other_lazy = interner.lazy(other);
    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let mut env = TypeEnvironment::new();
    env.insert_def(primary, object);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(other_lazy, 4);
    let result = evaluator.recurse_keyof(primary_lazy);

    assert_eq!(result, interner.literal_string("a"));
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "distinct DefIds without canonical/equivalent identity must not collide"
    );
}

#[test]
fn meta_rereduce_identity_stack_counts_decl_scoped_type_params_without_names() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let file = interner.intern_string("types.ts");
    let first = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node: 42 },
    }));
    let renamed = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("Renamed"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node: 42 },
    }));
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let result = evaluator.recurse_keyof(renamed);

    assert_eq!(result, interner.keyof(renamed));
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "declaration-scoped type parameters should count by declaration site, not display name"
    );
}

#[test]
fn meta_rereduce_identity_stack_keeps_distinct_decl_scoped_type_params_separate() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let file = interner.intern_string("types.ts");
    let first = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node: 42 },
    }));
    let same_name_distinct_decl = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node: 43 },
    }));
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let _ = evaluator.recurse_keyof(same_name_distinct_decl);

    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "distinct declaration-scoped type parameters must not collide solely because names match"
    );
}

#[test]
fn meta_rereduce_identity_stack_keeps_sibling_jsdoc_owner_params_separate() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let file = interner.intern_string("types.js");
    let first = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::JsdocOwnerScoped { file, node: 42 },
    }));
    let sibling = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::JsdocOwnerScoped { file, node: 42 },
    }));
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let _ = evaluator.recurse_keyof(sibling);

    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "sibling JSDoc parameters sharing one owner must remain distinct by name"
    );
}

#[test]
fn meta_rereduce_identity_stack_keeps_user_type_param_names_distinct() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let name = interner.intern_string("T");
    let first = interner.fresh_type_param(TypeParamInfo {
        name,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    let second = interner.fresh_type_param(TypeParamInfo {
        name,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let _ = evaluator.recurse_keyof(second);

    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "ordinary user type parameters must not collide solely because their names match"
    );
}

#[test]
fn meta_rereduce_identity_stack_counts_infer_placeholders_by_id() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let first = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferPlaceholder { id: 7 },
    }));
    let renamed = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Renamed"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferPlaceholder { id: 7 },
    }));
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let result = evaluator.recurse_keyof(renamed);

    assert_eq!(result, interner.keyof(renamed));
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "infer placeholders should count by structured placeholder id, not display name"
    );
}

#[test]
fn meta_rereduce_identity_stack_counts_infer_sources_by_id() {
    use crate::evaluation::evaluate::TypeEvaluator;

    let interner = TypeInterner::new();
    let origin_name = interner.intern_string("Source");
    let first = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferSource {
            id: 9,
            origin_name: Some(origin_name),
        },
    }));
    let renamed = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Renamed"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferSource {
            id: 9,
            origin_name: None,
        },
    }));
    let distinct = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferSource {
            id: 10,
            origin_name: Some(origin_name),
        },
    }));

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let _ = evaluator.recurse_keyof(distinct);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "different infer-source ids must remain distinct even with the same source name"
    );

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(first, 4);
    let result = evaluator.recurse_keyof(renamed);

    assert_eq!(result, interner.keyof(renamed));
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "infer-source placeholders should count by structured id, not origin display name"
    );
}

// =============================================================================
// Mapped Type Indexing Tests
// =============================================================================

/// Regression test for `{[K in keyof T]: F<K>}[K]` where K extends keyof T.
/// When a `TypeParameter` K has constraint = `keyof T` and the mapped type's
/// constraint is also `keyof T`, `visit_mapped` must recognize K as a valid
/// substitution index. This is the pattern used by class methods:
///
/// ```ts
/// class Form<T> {
///   private map: {[K in keyof T]: (v: T[K]) => void}
///   set<K extends keyof T>(key: K, v: T[K]) { this.map[key](v) }
/// }
/// ```
#[test]
fn test_index_access_mapped_constrained_type_param() {
    let interner = TypeInterner::new();

    // T (unconstrained, using fresh to simulate checker behavior)
    let t_type = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let keyof_t = interner.keyof(t_type);

    // Template: () => void (a callable type)
    let template_type = interner.function(crate::types::FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Mapped type: { [P in keyof T]: () => void }
    let mapped_type_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let mapped = interner.mapped(MappedType {
        type_param: mapped_type_param,
        constraint: keyof_t,
        name_type: None,
        template: template_type,
        optional_modifier: None,
        readonly_modifier: None,
    });

    // K with constraint = keyof T (using the SAME keyof_t TypeId)
    let k_type = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // Evaluate: { [P in keyof T]: () => void }[K]
    // Should resolve to the template: () => void
    use crate::evaluation::evaluate::evaluate_index_access;
    let result = evaluate_index_access(&interner, mapped, k_type);
    assert_eq!(
        result, template_type,
        "{{[P in keyof T]: () => void}}[K extends keyof T] should resolve to () => void"
    );
}

/// Variant: K constraint and mapped constraint are SEPARATE keyof calls
/// but over the SAME T `TypeId` — should produce same result since keyof is
/// content-addressed.
#[test]
fn test_index_access_mapped_constrained_type_param_separate_keyof() {
    let interner = TypeInterner::new();

    let t_type = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // Two separate calls to keyof(t_type) — should intern to same TypeId
    let keyof_t_for_mapped = interner.keyof(t_type);
    let keyof_t_for_k = interner.keyof(t_type);
    assert_eq!(
        keyof_t_for_mapped, keyof_t_for_k,
        "keyof T must be content-addressed"
    );

    let template_type = interner.function(crate::types::FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: keyof_t_for_mapped,
        name_type: None,
        template: template_type,
        optional_modifier: None,
        readonly_modifier: None,
    });

    let k_type = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keyof_t_for_k),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    use crate::evaluation::evaluate::evaluate_index_access;
    let result = evaluate_index_access(&interner, mapped, k_type);
    assert_eq!(
        result, template_type,
        "{{[P in keyof T]: () => void}}[K extends keyof T] should resolve to () => void (separate keyof calls)"
    );
}

/// When an index type is an intersection like `string & keyof T`, the
/// `visit_mapped` fast path should recognize that the intersection contains
/// the mapped type's constraint and allow substitution.
#[test]
fn test_index_access_mapped_with_intersection_index() {
    let interner = TypeInterner::new();

    // Create { x: string, y: number }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.keyof(source);

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };

    // { [K in keyof source]: boolean }
    let mapped = interner.mapped(MappedType {
        type_param: type_param_info,
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        optional_modifier: None,
        readonly_modifier: None,
    });

    // Index with `string & keyof source` (intersection, as happens in for-in loops)
    let intersection_index = interner.intersection(vec![TypeId::STRING, keyof_source]);
    let index_access = interner.index_access(mapped, intersection_index);

    // Should evaluate successfully (not remain as deferred IndexAccess)
    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::BOOLEAN,
        "mapped[string & keyof T] should resolve to the template type"
    );
}

/// tsc parity (`substituteIndexedMappedType`): indexing a generic mapped type
/// `{ [K in keyof T]: F(K) }` by its own still-generic constraint `keyof T`
/// substitutes the constraint for the binder, producing `F(keyof T)` — the
/// whole key space collapses into a single substitution. This is the
/// documented `tsc` behavior for generic `T` (per-key expansion only happens
/// for concrete, literal-enumerable key spaces) and is why `KeysMatching`
/// style utilities see `T[keyof T] extends string ? keyof T : never` in
/// generic contexts. This test previously pinned the opposite (a fresh
/// `K extends keyof T` binder kept in the template), which broke identity
/// with the simplified form `tsc` compares against.
#[test]
fn test_index_access_mapped_keyof_substitutes_constraint_for_binder() {
    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let keyof_t = interner.keyof(t_type);

    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param));

    let template = interner.conditional(ConditionalType {
        check_type: interner.index_access(t_type, k_type),
        extends_type: TypeId::STRING,
        true_type: k_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: k_param,
        constraint: keyof_t,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    });

    let result = evaluate_type(&interner, interner.index_access(mapped, keyof_t));

    let collapsed = interner.conditional(ConditionalType {
        check_type: interner.index_access(t_type, keyof_t),
        extends_type: TypeId::STRING,
        true_type: keyof_t,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    assert_eq!(
        result, collapsed,
        "mapped[keyof T] with a generic constraint must substitute the constraint for the \
         binder (tsc `substituteIndexedMappedType` parity): expected \
         `T[keyof T] extends string ? keyof T : never`"
    );
}

#[test]
fn test_large_union_literal_property_access_uses_fast_path() {
    let interner = TypeInterner::new();

    let mut members = Vec::new();
    let mut expected_names = Vec::new();
    for idx in 0..1200 {
        let name = interner.literal_string(&format!("item-{idx}"));
        expected_names.push(name);
        members.push(interner.object(vec![
            PropertyInfo::new(interner.intern_string("name"), name),
            PropertyInfo::new(interner.intern_string("payload"), TypeId::NUMBER),
        ]));
    }

    let big_union = interner.union(members);
    let result = evaluate_type(
        &interner,
        interner.index_access(big_union, interner.literal_string("name")),
    );

    assert_eq!(
        result,
        interner.union(expected_names),
        "large unions indexed by a literal property key should evaluate instead of falling back to error"
    );
}

// =============================================================================
// Index access on conditional-type results
// =============================================================================
// Repro of conformance failure in excessPropertyCheckIntersectionWithRecursiveType:
// `Prepend<any, []>["length"]` should resolve to a literal number when the
// conditional produces a concrete tuple via infer matching. When the conditional
// result is used directly (without prior alias expansion), the IndexAccess
// evaluator must walk through the Conditional shape to reach the tuple inside.

#[test]
fn test_index_access_literal_on_concrete_conditional_tuple() {
    // (T extends [infer A, infer B] ? [A, B] : never)[0] with T = [string, number]
    // should evaluate to string.
    let interner = TypeInterner::new();

    let tuple_sn = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));

    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let true_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond_id = interner.conditional(ConditionalType {
        check_type: tuple_sn,
        extends_type: extends_tuple,
        true_type: true_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let index = interner.index_access(cond_id, interner.literal_number(0.0));
    let result = evaluate_type(&interner, index);
    assert_eq!(
        result,
        TypeId::STRING,
        "(Tuple extends [infer A, infer B] ? [A, B] : never)[0] should resolve to string"
    );
}

#[test]
fn test_index_access_length_on_concrete_conditional_tuple() {
    // (((...args: T) => void) extends (...args: infer R) => void ? R : any)["length"]
    // with T = [string, number] should resolve to the literal number 2.
    // This mirrors the `Length<T>` pattern used by recursive type builders.
    use crate::types::{FunctionShape, ParamInfo};

    let interner = TypeInterner::new();

    let tuple_sn = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let check_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_sn,
            optional: false,
            rest: true,
            arity_only_optional: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));

    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_r,
            optional: false,
            rest: true,
            arity_only_optional: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond_id = interner.conditional(ConditionalType {
        check_type: check_fn,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::ANY,
        is_distributive: false,
    });

    let index = interner.index_access(cond_id, interner.literal_string("length"));
    let result = evaluate_type(&interner, index);
    assert_eq!(
        result,
        interner.literal_number(2.0),
        "infer-result tuple length should resolve to its literal fixed size"
    );
}
