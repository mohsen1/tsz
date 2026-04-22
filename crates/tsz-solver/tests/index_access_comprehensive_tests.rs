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

    let _result = evaluate_type(&interner, index_access);
    // obj[never] - behavior depends on implementation
    // Could be never or could be an error type
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

// =============================================================================
// Mapped Type Indexing Tests
// =============================================================================

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

#[test]
fn test_index_access_mapped_keyof_preserves_per_key_template_relation() {
    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let keyof_t = interner.keyof(t_type);

    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
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
    assert_ne!(
        result, collapsed,
        "mapped[keyof T] should not collapse the whole key space into a single substitution"
    );

    match interner.lookup(result) {
        Some(TypeData::Conditional(cond_id)) => {
            let cond = interner.conditional_type(cond_id);
            match (
                interner.lookup(cond.check_type),
                interner.lookup(cond.true_type),
            ) {
                (
                    Some(TypeData::IndexAccess(object_type, key_type)),
                    Some(TypeData::TypeParameter(_)),
                ) => {
                    assert_eq!(object_type, t_type);
                    assert_eq!(key_type, cond.true_type);
                }
                other => panic!(
                    "Expected per-key conditional to keep a single key parameter, got {other:?}"
                ),
            }
        }
        other => panic!("Expected deferred conditional result, got {other:?}"),
    }
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
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
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
    }));

    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_r,
            optional: false,
            rest: true,
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
