use super::*;
use crate::construction::TypeDatabase;
use crate::def::{DefId, DefKind};
use crate::intern::TypeInterner;
use crate::relations::subtype::{TypeEnvironment, TypeResolver};
use crate::types::{PropertyInfo, SymbolRef, TypeData, TypeParamInfo};

// ===================================================================
// Helper resolvers for testing
// ===================================================================

#[test]
fn test_canonicalizer_creation() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let _canonicalizer = Canonicalizer::new(&interner, &env);
}

// ===================================================================
// Primitive identity preservation
// ===================================================================

#[test]
fn test_canonicalize_primitive() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let number = TypeId::NUMBER;
    let canon_number = canon.canonicalize(number);

    // Primitives should canonicalize to themselves
    assert_eq!(canon_number, number);
}

#[test]
fn canonicalize_all_primitives_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let primitives = [
        TypeId::NEVER,
        TypeId::UNKNOWN,
        TypeId::ANY,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
        TypeId::FUNCTION,
        TypeId::ERROR,
        TypeId::BOOLEAN_TRUE,
        TypeId::BOOLEAN_FALSE,
    ];

    for prim in primitives {
        let result = canon.canonicalize(prim);
        assert_eq!(
            result, prim,
            "Primitive TypeId({}) should canonicalize to itself",
            prim.0
        );
    }
}

// ===================================================================
// Literal identity preservation
// ===================================================================

#[test]
fn canonicalize_string_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let lit = interner.literal_string("hello");
    let result = canon.canonicalize(lit);
    assert_eq!(result, lit);
}

#[test]
fn canonicalize_number_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let lit = interner.literal_number(42.0);
    let result = canon.canonicalize(lit);
    assert_eq!(result, lit);
}

#[test]
fn canonicalize_boolean_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let t = interner.literal_boolean(true);
    let f = interner.literal_boolean(false);
    assert_eq!(canon.canonicalize(t), t);
    assert_eq!(canon.canonicalize(f), f);
}

// ===================================================================
// Array canonicalization
// ===================================================================

#[test]
fn canonicalize_array_of_primitive() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let arr = interner.array(TypeId::STRING);
    let result = canon.canonicalize(arr);
    // Array of primitive should be identical (element doesn't change)
    assert_eq!(result, arr);
}

#[test]
fn canonicalize_nested_array() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // number[][]
    let inner = interner.array(TypeId::NUMBER);
    let outer = interner.array(inner);
    let result = canon.canonicalize(outer);
    assert_eq!(result, outer);
}

#[test]
fn canonicalize_array_structural_equivalence() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // Two arrays with the same element type should produce the same canonical form
    let arr1 = interner.array(TypeId::NUMBER);
    let arr2 = interner.array(TypeId::NUMBER);
    assert_eq!(canon.canonicalize(arr1), canon.canonicalize(arr2));
}

// ===================================================================
// Tuple canonicalization
// ===================================================================

#[test]
fn canonicalize_tuple_of_primitives() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::TupleElement;
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

    let result = canon.canonicalize(tuple);
    assert_eq!(result, tuple);
}

#[test]
fn canonicalize_tuple_preserves_optional_and_rest() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::TupleElement;
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let result = canon.canonicalize(tuple);
    // Look up the result's tuple elements
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(result) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(elements.len(), 2);
        assert!(elements[0].optional);
        assert!(!elements[0].rest);
        assert!(!elements[1].optional);
        assert!(elements[1].rest);
    } else {
        panic!("Expected tuple type");
    }
}

// ===================================================================
// Union canonicalization (commutativity)
// ===================================================================

#[test]
fn canonicalize_union_commutativity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create unions with members in different orders
    let union_ab = interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_ba = interner.union_preserve_members(vec![TypeId::NUMBER, TypeId::STRING]);

    let mut canon1 = Canonicalizer::new(&interner, &env);
    let result1 = canon1.canonicalize(union_ab);

    let mut canon2 = Canonicalizer::new(&interner, &env);
    let result2 = canon2.canonicalize(union_ba);

    // Both orderings should produce the same canonical form
    assert_eq!(
        result1, result2,
        "Union(A, B) and Union(B, A) should canonicalize identically"
    );
}

#[test]
fn canonicalize_union_deduplication() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // Union with duplicates: string | number | string
    let union =
        interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER, TypeId::STRING]);
    let result = canon.canonicalize(union);

    // Should be deduplicated
    if let Some(TypeData::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Duplicate members should be deduplicated");
    }
    // (If the interner already normalizes, the input union may already be 2 members)
}

#[test]
fn canonicalize_union_three_members() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let u1 = interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let u2 = interner.union_preserve_members(vec![TypeId::BOOLEAN, TypeId::STRING, TypeId::NUMBER]);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);

    assert_eq!(
        c1.canonicalize(u1),
        c2.canonicalize(u2),
        "Three-member unions should canonicalize identically regardless of order"
    );
}

// ===================================================================
// Intersection canonicalization
// ===================================================================

#[test]
fn canonicalize_intersection_sorts_structural_members() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Use type parameters to create intersections that won't be reduced
    let t = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let u = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // T & U vs U & T — both should produce the same canonical intersection
    // since both are structural (non-callable) types
    let inter1 = interner.intersection(vec![t, u]);
    let inter2 = interner.intersection(vec![u, t]);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);

    assert_eq!(
        c1.canonicalize(inter1),
        c2.canonicalize(inter2),
        "Intersection(T, U) and Intersection(U, T) should canonicalize identically for structural types"
    );
}

// ===================================================================
// Function canonicalization
// ===================================================================

#[test]
fn canonicalize_function_type() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::{FunctionShape, ParamInfo};
    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(
            interner.intern_string("x"),
            TypeId::STRING,
        )],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = canon.canonicalize(func);
    // Function with only primitive types should be the same
    if let Some(TypeData::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.return_type, TypeId::NUMBER);
        assert_eq!(shape.params.len(), 1);
        assert_eq!(shape.params[0].type_id, TypeId::STRING);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn canonicalize_function_with_type_params_uses_bound_parameter() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::{FunctionShape, ParamInfo};

    // (x: T) => T  with param named "T"
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func_t = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo::required(interner.intern_string("x"), t_param)],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut canon = Canonicalizer::new(&interner, &env);
    let result = canon.canonicalize(func_t);

    // The canonicalized function should use BoundParameter(0) for T
    if let Some(TypeData::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        // The param type and return type should both be BoundParameter(0)
        assert!(
            matches!(
                interner.lookup(shape.params[0].type_id),
                Some(TypeData::BoundParameter(0))
            ),
            "Param type should be BoundParameter(0), got: {:?}",
            interner.lookup(shape.params[0].type_id)
        );
        assert!(
            matches!(
                interner.lookup(shape.return_type),
                Some(TypeData::BoundParameter(0))
            ),
            "Return type should be BoundParameter(0), got: {:?}",
            interner.lookup(shape.return_type)
        );
    } else {
        panic!("Expected function type, got: {:?}", interner.lookup(result));
    }
}

#[test]
fn canonicalize_function_type_params_alpha_equivalent_across_names() {
    // Two generic functions that differ only in their type-parameter name
    // (`<T>(x: T) => T` vs `<U>(x: U) => U`) are alpha-equivalent and must
    // canonicalize to the SAME identity: references are positional
    // (BoundParameter), so the declared name is identity-irrelevant — exactly
    // as mapped types already erase it. `tsc`'s compareTypeParametersIdentical
    // compares constraints only, never names.
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `<name>(x: name) => name`
    let make = |name: &str| {
        let info = TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let pref = interner.type_param(info);
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo::required(interner.intern_string("x"), pref)],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let r1 = c1.canonicalize(make("T"));
    let r2 = c2.canonicalize(make("U"));

    // Both use BoundParameter(0) in body and the now-erased type-param name, so
    // the two alpha-equivalent generic functions share one canonical identity.
    assert_eq!(
        r1, r2,
        "Functions differing only in type-parameter name are alpha-equivalent and \
         must share one canonical form (name erased, like mapped types)"
    );
}

/// Negative control: erasing the name must NOT erase the constraint. Two generic
/// functions whose type parameters differ only in their *constraint*
/// (`<T extends string>` vs `<U extends number>`) are not alpha-equivalent and
/// must keep distinct canonical identities.
#[test]
fn canonicalize_function_distinct_constraints_stay_distinct() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |name: &str, constraint: TypeId| {
        let atom = interner.intern_string(name);
        let info = TypeParamInfo {
            name: atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let pref = interner.type_param(info);
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo::required(interner.intern_string("x"), pref)],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let func_t = make("T", TypeId::STRING);
    let func_u = make("U", TypeId::NUMBER);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_ne!(
        c1.canonicalize(func_t),
        c2.canonicalize(func_u),
        "type parameters with different constraints must not be alpha-equivalent"
    );
}

/// Multi-parameter positional identity: renaming both parameters keeps identity
/// (`<A, B>(a: A) => B` ≡ `<X, Y>(x: X) => Y`), but swapping which positional
/// parameter the body references must change identity (`=> B` vs `=> A`). The
/// erased name cannot collapse the positional `BoundParameter` distinction.
#[test]
fn canonicalize_function_multi_param_positional_identity() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `<P0, P1>(a: P0) => <return is P1 or P0?>`
    let make = |n0: &str, n1: &str, return_second: bool| {
        let a0 = interner.intern_string(n0);
        let a1 = interner.intern_string(n1);
        let mk = |atom| TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let p0 = interner.type_param(mk(a0));
        let p1 = interner.type_param(mk(a1));
        interner.function(FunctionShape {
            type_params: vec![mk(a0), mk(a1)],
            params: vec![ParamInfo::required(interner.intern_string("a"), p0)],
            this_type: None,
            return_type: if return_second { p1 } else { p0 },
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let ab = make("A", "B", true); // <A,B>(a:A) => B
    let xy = make("X", "Y", true); // <X,Y>(x:X) => Y  — alpha-equivalent
    let aa = make("A", "B", false); // <A,B>(a:A) => A  — different positional ref

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(ab),
        c2.canonicalize(xy),
        "renaming both type parameters is alpha-equivalent"
    );
    assert_ne!(
        c1.canonicalize(ab),
        c3.canonicalize(aa),
        "the positional parameter the body references is identity-relevant"
    );
}

/// Call-signature path (`canonicalize_signature`, used by `Callable` for both
/// call and construct signatures): a callable with `<T>(x: T) => T` and one with
/// `<U>(x: U) => U` are alpha-equivalent.
#[test]
fn canonicalize_call_signature_alpha_equivalent_across_names() {
    use crate::types::{CallSignature, CallableShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |name: &str| {
        let atom = interner.intern_string(name);
        let info = TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let pref = interner.type_param(info);
        let sig = CallSignature {
            type_params: vec![info],
            params: vec![ParamInfo::required(interner.intern_string("x"), pref)],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_method: false,
        };
        interner.callable(CallableShape {
            call_signatures: vec![sig],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        })
    };

    let call_t = make("T");
    let call_u = make("U");

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(call_t),
        c2.canonicalize(call_u),
        "generic call signatures differing only in type-parameter name are alpha-equivalent"
    );
}

// ===================================================================
// Object canonicalization
// ===================================================================

#[test]
fn canonicalize_object_with_primitives() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let result = canon.canonicalize(obj);
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn canonicalize_empty_object() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let obj = interner.object(vec![]);
    let result = canon.canonicalize(obj);
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert!(shape.properties.is_empty());
    } else {
        panic!("Expected object type");
    }
}

// ===================================================================
// Application (generic) canonicalization
// ===================================================================

#[test]
fn canonicalize_application_with_primitive_args() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // Simulate Array<number> as Application(Lazy(DefId(1)), [number])
    let base = interner.lazy(DefId(1));
    let app = interner.application(base, vec![TypeId::NUMBER]);

    let result = canon.canonicalize(app);
    if let Some(TypeData::Application(app_id)) = interner.lookup(result) {
        let app_data = interner.type_application(app_id);
        // Args should still be [number]
        assert_eq!(app_data.args.len(), 1);
        assert_eq!(app_data.args[0], TypeId::NUMBER);
    } else {
        panic!("Expected application type");
    }
}

// ===================================================================
// Template literal canonicalization
// ===================================================================

#[test]
fn canonicalize_template_literal_with_primitive_type() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::TemplateSpan;
    let tl = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = canon.canonicalize(tl);
    if let Some(TypeData::TemplateLiteral(id)) = interner.lookup(result) {
        let spans = interner.template_list(id);
        assert_eq!(spans.len(), 2);
    } else {
        panic!("Expected template literal type");
    }
}

// ===================================================================
// String intrinsic canonicalization
// ===================================================================

#[test]
fn canonicalize_string_intrinsic() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::StringIntrinsicKind;
    let upper = interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);

    let result = canon.canonicalize(upper);
    if let Some(TypeData::StringIntrinsic { kind, type_arg }) = interner.lookup(result) {
        assert_eq!(kind, StringIntrinsicKind::Uppercase);
        assert_eq!(type_arg, TypeId::STRING);
    } else {
        panic!("Expected string intrinsic type");
    }
}

#[test]
fn canonicalize_string_intrinsic_in_function_uses_bound_parameter() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::{FunctionShape, ParamInfo, StringIntrinsicKind};

    // Uppercase<T> in function <T>(x: Uppercase<T>) => void
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let upper_t = interner.string_intrinsic(StringIntrinsicKind::Uppercase, t_param);
    let func_t = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo::required(interner.intern_string("x"), upper_t)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut canon = Canonicalizer::new(&interner, &env);
    let result = canon.canonicalize(func_t);

    // The canonicalized function should have Uppercase<BoundParameter(0)> as param type
    if let Some(TypeData::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        let param_type = shape.params[0].type_id;
        // The param should be StringIntrinsic(Uppercase, BoundParameter(0))
        if let Some(TypeData::StringIntrinsic { kind, type_arg }) = interner.lookup(param_type) {
            assert_eq!(kind, StringIntrinsicKind::Uppercase);
            assert!(
                matches!(interner.lookup(type_arg), Some(TypeData::BoundParameter(0))),
                "Intrinsic arg should be BoundParameter(0), got: {:?}",
                interner.lookup(type_arg)
            );
        } else {
            panic!(
                "Expected StringIntrinsic param type, got: {:?}",
                interner.lookup(param_type)
            );
        }
    } else {
        panic!("Expected function type");
    }
}

// ===================================================================
// Mapped type canonicalization (alpha-equivalence)
// ===================================================================

#[test]
fn canonicalize_mapped_type_alpha_equivalence() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::MappedType;

    // { [K in string]: number }
    let k_atom = interner.intern_string("K");
    let k_param = interner.type_param(TypeParamInfo {
        name: k_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mapped_k = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: TypeId::STRING,
        template: k_param,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // { [P in string]: number }
    let p_atom = interner.intern_string("P");
    let p_param = interner.type_param(TypeParamInfo {
        name: p_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mapped_p = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: p_atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: TypeId::STRING,
        template: p_param,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(mapped_k),
        c2.canonicalize(mapped_p),
        "{{ [K in string]: K }} and {{ [P in string]: P }} should be alpha-equivalent"
    );
}

// ===================================================================
// Expanding alias chain termination
// ===================================================================

struct ExpandingAliasResolver;

impl TypeResolver for ExpandingAliasResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        Some(interner.lazy(DefId(def_id.0 + 1)))
    }

    fn get_def_kind(&self, _def_id: DefId) -> Option<DefKind> {
        Some(DefKind::TypeAlias)
    }
}

#[test]
fn test_canonicalize_expanding_alias_chain_terminates() {
    let interner = TypeInterner::new();
    let resolver = ExpandingAliasResolver;
    let mut canon = Canonicalizer::new(&interner, &resolver);

    let start = interner.lazy(DefId(1));
    let result = canon.canonicalize(start);

    assert!(
        matches!(interner.lookup(result), Some(TypeData::Lazy(_))),
        "canonicalization should terminate with a lazy fallback for expanding aliases"
    );
}

// ===================================================================
// Self-referential type alias (Recursive index)
// ===================================================================

/// Resolver where DefId(1) is a type alias whose body is { value: DefId(1) }
/// i.e., type Node = { value: Node }
struct SelfReferentialResolver {
    body: std::cell::RefCell<Option<TypeId>>,
}

impl SelfReferentialResolver {
    fn new() -> Self {
        Self {
            body: std::cell::RefCell::new(None),
        }
    }

    fn set_body(&self, type_id: TypeId) {
        *self.body.borrow_mut() = Some(type_id);
    }
}

impl TypeResolver for SelfReferentialResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        if def_id == DefId(1) {
            *self.body.borrow()
        } else {
            None
        }
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
        if def_id == DefId(1) {
            Some(DefKind::TypeAlias)
        } else {
            None
        }
    }
}

#[test]
fn canonicalize_self_referential_alias_via_different_type_ids() {
    let interner = TypeInterner::new();
    let resolver = SelfReferentialResolver::new();

    // type Node = { value: Node }
    // The body is an object whose property references Lazy(DefId(1)).
    // When Lazy(DefId(1)) is the top-level input, the TypeId-level guard
    // detects the cycle (same TypeId visited twice) and returns the Lazy
    // type as-is. The def_stack-based Recursive(n) detection only triggers
    // when the self-reference goes through a DIFFERENT TypeId path.
    let lazy_self = interner.lazy(DefId(1));
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        lazy_self,
    )]);
    resolver.set_body(obj);

    let mut canon = Canonicalizer::new(&interner, &resolver);
    let result = canon.canonicalize(lazy_self);

    // The result is an object where the self-referencing property retains
    // its Lazy(DefId(1)) type because the TypeId guard caught the cycle
    // before def_stack-level Recursive index generation could fire.
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 1);
        let prop_type = shape.properties[0].type_id;
        // The guard returned the Lazy type on cycle detection
        assert!(
            matches!(interner.lookup(prop_type), Some(TypeData::Lazy(_))),
            "Self-referencing property retains Lazy type via guard cycle detection, got: {:?}",
            interner.lookup(prop_type)
        );
    } else {
        panic!("Expected object type, got: {:?}", interner.lookup(result));
    }
}

// ===================================================================
// Nominal types (Interface) preserved as Lazy
// ===================================================================

struct NominalResolver;

impl TypeResolver for NominalResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_def_kind(&self, _def_id: DefId) -> Option<DefKind> {
        // All DefIds are interfaces (nominal)
        Some(DefKind::Interface)
    }
}

#[test]
fn canonicalize_nominal_type_preserved() {
    let interner = TypeInterner::new();
    let resolver = NominalResolver;
    let mut canon = Canonicalizer::new(&interner, &resolver);

    let lazy = interner.lazy(DefId(42));
    let result = canon.canonicalize(lazy);

    // Nominal type should be preserved as-is
    assert_eq!(
        result, lazy,
        "Nominal (Interface) type should remain as Lazy(DefId)"
    );
}

// ===================================================================
// Caching
// ===================================================================

#[test]
fn canonicalize_caches_results() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result1 = canon.canonicalize(union);
    let result2 = canon.canonicalize(union);

    // Second call should use cache and return same result
    assert_eq!(result1, result2);
}

#[test]
fn canonicalizer_cache_statistics_account_for_entries_and_size() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let empty_stats = canon.cache_statistics();
    assert_eq!(empty_stats.cache_entries, 0);
    assert!(empty_stats.estimated_size_bytes > 0);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    canon.canonicalize(union);

    let populated_stats = canon.cache_statistics();
    assert_eq!(populated_stats.cache_entries, 1);
    assert!(populated_stats.estimated_size_bytes >= empty_stats.estimated_size_bytes);

    canon.canonicalize(union);
    assert_eq!(
        canon.cache_statistics().cache_entries,
        populated_stats.cache_entries
    );
}

// ===================================================================
// Nested composite types
// ===================================================================

#[test]
fn canonicalize_array_of_union() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // (string | number)[]
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let arr = interner.array(union);
    let result = canon.canonicalize(arr);

    if let Some(TypeData::Array(elem)) = interner.lookup(result) {
        // Element should be canonicalized union
        assert!(matches!(interner.lookup(elem), Some(TypeData::Union(_))));
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn canonicalize_union_of_arrays() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // string[] | number[]
    let str_arr = interner.array(TypeId::STRING);
    let num_arr = interner.array(TypeId::NUMBER);
    let union = interner.union(vec![str_arr, num_arr]);
    let result = canon.canonicalize(union);

    if let Some(TypeData::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union type");
    }
}

// ===================================================================
// Conditional type (passthrough)
// ===================================================================

#[test]
fn canonicalize_conditional_passthrough() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::ConditionalType;
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let result = canon.canonicalize(cond);
    // Conditional types fall through to the default case (preserved as-is)
    // since there's no explicit match arm for them in the canonicalizer
    assert_eq!(result, cond);
}

// ===================================================================
// Index access and keyof (passthrough)
// ===================================================================

#[test]
fn canonicalize_index_access_passthrough() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let idx = interner.index_access(TypeId::STRING, TypeId::NUMBER);
    let result = canon.canonicalize(idx);
    // IndexAccess falls through to default case
    assert_eq!(result, idx);
}

#[test]
fn canonicalize_keyof_passthrough() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let keyof = interner.keyof(TypeId::STRING);
    let result = canon.canonicalize(keyof);
    // KeyOf falls through to default case
    assert_eq!(result, keyof);
}

// ===================================================================
// NoInfer<T> wrapper canonicalization
//
// `NoInfer` is a single-nested structural wrapper (grouped with
// `Array`/`ReadonlyType` by `child_policy`); the canonicalizer must reduce
// its inner like the sibling wrappers, or two structurally-identical
// `NoInfer<…>` types fragment into distinct canonical identities (#13609
// family, NoInfer axis).
// ===================================================================

/// Build `<name>(x: name) => name` — a generic identity function whose only
/// declared type parameter is named `name`. Two such functions differing only
/// in the parameter name are alpha-equivalent.
fn make_generic_identity_fn(interner: &TypeInterner, name: &str) -> TypeId {
    use crate::types::{FunctionShape, ParamInfo};
    let info = TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let pref = interner.type_param(info);
    interner.function(FunctionShape {
        type_params: vec![info],
        params: vec![ParamInfo::required(interner.intern_string("x"), pref)],
        this_type: None,
        return_type: pref,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

#[test]
fn canonicalize_no_infer_inner_canonicalized_alpha_equivalent() {
    // `NoInfer<<T>(x: T) => T>` and `NoInfer<<U>(x: U) => U>` wrap two
    // alpha-equivalent generic functions. Their inners canonicalize to one
    // identity, so the wrapped types must too. Before the NoInfer arm was
    // added, the wrapper passed through the catch-all and the two distinct
    // inner TypeIds kept the wrapped types distinct.
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let inner_t = make_generic_identity_fn(&interner, "T");
    let inner_u = make_generic_identity_fn(&interner, "U");
    // The two inner functions are interned distinctly (names differ).
    assert_ne!(inner_t, inner_u);

    let no_infer_t = interner.no_infer(inner_t);
    let no_infer_u = interner.no_infer(inner_u);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let r1 = c1.canonicalize(no_infer_t);
    let r2 = c2.canonicalize(no_infer_u);

    assert_eq!(
        r1, r2,
        "NoInfer wrapping alpha-equivalent generic functions must share one \
         canonical identity (inner is canonicalized like Array/ReadonlyType)"
    );
}

#[test]
fn canonicalize_no_infer_recurses_inner_like_array() {
    // The canonical NoInfer inner is exactly the canonicalized inner: the
    // wrapper is transparent to canonicalization (mirrors the Array/ReadonlyType
    // structural-children policy), only normalizing what it wraps.
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let inner = make_generic_identity_fn(&interner, "T");
    let canon_inner = {
        let mut c = Canonicalizer::new(&interner, &env);
        c.canonicalize(inner)
    };
    let expected = interner.no_infer(canon_inner);

    let mut canon = Canonicalizer::new(&interner, &env);
    let result = canon.canonicalize(interner.no_infer(inner));

    assert_eq!(
        result, expected,
        "canonicalize(NoInfer<T>) must equal NoInfer<canonicalize(T)>"
    );
}

#[test]
fn canonicalize_no_infer_preserves_wrapper() {
    // NoInfer keeps a distinct identity from its inner in `tsc`; canonicalization
    // must NOT strip the wrapper (it is not an identity-irrelevant modifier — it
    // is a structural type constructor). `NoInfer<number>` stays a NoInfer.
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let no_infer_number = interner.no_infer(TypeId::NUMBER);
    let result = canon.canonicalize(no_infer_number);

    assert!(
        matches!(interner.lookup(result), Some(TypeData::NoInfer(inner)) if inner == TypeId::NUMBER),
        "NoInfer<number> must canonicalize to NoInfer<number>, not be stripped to number; got {:?}",
        interner.lookup(result)
    );
    // The inner is a primitive, so the wrapper is unchanged overall.
    assert_eq!(result, no_infer_number);
}

#[test]
fn canonicalize_no_infer_distinct_inner_stays_distinct() {
    // Negative control: canonicalizing the inner must not over-collapse.
    // `NoInfer<string>` and `NoInfer<number>` wrap different inners and must
    // keep distinct canonical identities.
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let no_infer_string = interner.no_infer(TypeId::STRING);
    let no_infer_number = interner.no_infer(TypeId::NUMBER);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let r1 = c1.canonicalize(no_infer_string);
    let r2 = c2.canonicalize(no_infer_number);

    assert_ne!(
        r1, r2,
        "NoInfer over distinct inner types must stay distinct"
    );
}

// ===================================================================
// Object with index signatures
// ===================================================================

#[test]
fn canonicalize_object_with_index_signature() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::{IndexSignature, ObjectShape};
    let shape = ObjectShape {
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = interner.object_with_index(shape);

    let result = canon.canonicalize(obj);
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert!(shape.string_index.is_some());
        let idx = shape.string_index.as_ref().unwrap();
        assert_eq!(idx.key_type, TypeId::STRING);
        assert_eq!(idx.value_type, TypeId::NUMBER);
    } else {
        panic!("Expected object type with index");
    }
}

// ===================================================================
// Type-parameter `default` must not fragment canonical identity (#13609)
// ===================================================================

// `tsc` never distinguishes type parameters by their declared `default` in
// relation/identity; the default is metadata consumed only at instantiation
// when no argument is supplied. tsz interns type parameters structurally and
// hashes the `default` field, so two references to the same parameter that
// differ only in whether the (optional) default was captured intern to
// distinct `TypeId`s. Those must still canonicalize to one identity so the
// relation's reflexive/identity fast path is not lost.
#[test]
fn canonicalize_type_param_ignores_optional_default() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("R");
    let constraint = Some(TypeId::STRING);

    // Same parameter, one with a captured default, one without.
    let r_with_default = interner.type_param(TypeParamInfo {
        name,
        constraint,
        default: Some(TypeId::NUMBER),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let r_no_default = interner.type_param(TypeParamInfo {
        name,
        constraint,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // Distinct interned identities (the structural hash includes `default`)...
    assert_ne!(
        r_with_default, r_no_default,
        "precondition: interning keeps the default distinct"
    );

    // ...but they canonicalize to the same identity.
    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(r_with_default),
        c2.canonicalize(r_no_default),
        "type-parameter default must not fragment canonical identity"
    );

    // Two *different* defaults on the same parameter also collapse.
    let r_other_default = interner.type_param(TypeParamInfo {
        name,
        constraint,
        default: Some(TypeId::BOOLEAN),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c3.canonicalize(r_with_default),
        c4.canonicalize(r_other_default),
        "differing defaults on the same parameter canonicalize identically"
    );
}

// A genuinely different parameter (different name/constraint) must stay
// distinct — the normalization only drops `default`, it does not erase the
// parameter's identity.
#[test]
fn canonicalize_type_param_default_drop_preserves_real_distinctions() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let r = interner.type_param(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: Some(TypeId::NUMBER),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    // Different name.
    let s = interner.type_param(TypeParamInfo {
        name: interner.intern_string("S"),
        constraint: None,
        default: Some(TypeId::NUMBER),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    // Different constraint.
    let r_constrained = interner.type_param(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: Some(TypeId::STRING),
        default: Some(TypeId::NUMBER),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    assert_ne!(
        c1.canonicalize(r),
        c2.canonicalize(s),
        "distinct parameter names stay distinct after dropping default"
    );
    assert_ne!(
        c3.canonicalize(r),
        c4.canonicalize(r_constrained),
        "distinct constraints stay distinct after dropping default"
    );
}

// The same identity rule applies when the type parameter is declared in a
// signature's type-parameter list: two generic function types that differ only
// in a type parameter's `default` must canonicalize to one identity, so the
// relation's reflexive short-circuit holds for `<R extends X = "json">() => R`
// against `<R extends X>() => R` (#13609).
#[test]
fn canonicalize_function_type_param_list_ignores_default() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let r = interner.intern_string("R");
    let make = |default: Option<TypeId>| {
        let body = interner.type_param(TypeParamInfo {
            name: r,
            constraint: Some(TypeId::STRING),
            default,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        interner.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: r,
                constraint: Some(TypeId::STRING),
                default,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            }],
            params: vec![ParamInfo::required(interner.intern_string("x"), body)],
            this_type: None,
            return_type: body,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let with_default = make(Some(TypeId::NUMBER));
    let without_default = make(None);
    assert_ne!(
        with_default, without_default,
        "precondition: interning keeps the default distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(with_default),
        c2.canonicalize(without_default),
        "type-parameter default in a signature list must not fragment identity"
    );
}

// ===================================================================
// A free type-parameter reference's constraint must canonicalize, so its
// identity does not fragment on the constraint's *resolution state* (#13609)
// ===================================================================

/// Resolver where `DefId(1)` is a type alias whose body is `string | number`.
/// Models a cross-module constraint reference that one signature captured as a
/// still-`Lazy` alias and another captured already expanded to its union body.
struct AliasConstraintResolver {
    body: TypeId,
}

impl TypeResolver for AliasConstraintResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        (def_id == DefId(1)).then_some(self.body)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
        (def_id == DefId(1)).then_some(DefKind::TypeAlias)
    }
}

// `tsc` identifies a type parameter by the parameter itself, never by what
// resolution state its constraint snapshot was in. tsz interns a free
// type-parameter reference structurally, hashing the constraint `TypeId`, so the
// *same* parameter `R` referenced with constraint `Lazy(Alias)` in one signature
// and the alias's expanded `string | number` body in another interns to distinct
// `TypeId`s. Both must canonicalize to one identity (the #13609 `507`/`516`
// constraint-snapshot fragmentation), restoring the relation's reflexive
// short-circuit.
#[test]
fn canonicalize_free_type_param_constraint_resolution_state_converges() {
    let interner = TypeInterner::new();
    let union_body = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let resolver = AliasConstraintResolver { body: union_body };

    let r = interner.intern_string("R");
    // Same parameter, constraint captured as the still-`Lazy` alias...
    let r_lazy_constraint = interner.type_param(TypeParamInfo {
        name: r,
        constraint: Some(interner.lazy(DefId(1))),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    // ...vs the same constraint already resolved to the alias body.
    let r_resolved_constraint = interner.type_param(TypeParamInfo {
        name: r,
        constraint: Some(union_body),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    assert_ne!(
        r_lazy_constraint, r_resolved_constraint,
        "precondition: interning keeps the two constraint snapshots distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &resolver);
    let mut c2 = Canonicalizer::new(&interner, &resolver);
    assert_eq!(
        c1.canonicalize(r_lazy_constraint),
        c2.canonicalize(r_resolved_constraint),
        "a free type parameter's constraint resolution state must not fragment identity"
    );

    // The default is still irrelevant on top of the constraint convergence.
    let r_lazy_with_default = interner.type_param(TypeParamInfo {
        name: r,
        constraint: Some(interner.lazy(DefId(1))),
        default: Some(TypeId::BOOLEAN),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &resolver);
    let mut c4 = Canonicalizer::new(&interner, &resolver);
    assert_eq!(
        c3.canonicalize(r_lazy_with_default),
        c4.canonicalize(r_resolved_constraint),
        "constraint convergence holds regardless of the captured default"
    );
}

// The convergence is structural, not name-driven: renaming the binder leaves the
// identity rule intact (anti-hardcoding gate), and a *genuinely* different
// constraint shape still stays distinct (the fix only collapses resolution-state
// differences, it does not erase real constraint distinctions).
#[test]
fn canonicalize_free_type_param_constraint_convergence_is_name_agnostic() {
    let interner = TypeInterner::new();
    let union_body = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let resolver = AliasConstraintResolver { body: union_body };

    // Renamed binder `Element` instead of `R`.
    let elem = interner.intern_string("Element");
    let lazy_constraint = interner.type_param(TypeParamInfo {
        name: elem,
        constraint: Some(interner.lazy(DefId(1))),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let resolved_constraint = interner.type_param(TypeParamInfo {
        name: elem,
        constraint: Some(union_body),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c1 = Canonicalizer::new(&interner, &resolver);
    let mut c2 = Canonicalizer::new(&interner, &resolver);
    assert_eq!(
        c1.canonicalize(lazy_constraint),
        c2.canonicalize(resolved_constraint),
        "constraint resolution-state convergence is independent of the binder name"
    );

    // Negative control: a genuinely different constraint shape (`string` only,
    // not the `string | number` the alias expands to) must stay distinct.
    let other_constraint = interner.type_param(TypeParamInfo {
        name: elem,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &resolver);
    let mut c4 = Canonicalizer::new(&interner, &resolver);
    assert_ne!(
        c3.canonicalize(resolved_constraint),
        c4.canonicalize(other_constraint),
        "a genuinely different constraint shape must not be merged by canonicalization"
    );
}

// ===================================================================
// `infer` parameters must canonicalize like free type parameters (#13609)
// ===================================================================

// An `infer R` declaration carries the same `TypeParamInfo` as a type
// parameter, so the same identity rule applies: `tsc` identifies the parameter
// by itself — its name and the *shape* of its constraint — never by the
// optional `default` nor the *resolution state* its constraint snapshot
// happened to be in. The canonicalizer previously left `Infer` in the
// catch-all passthrough (it only normalized `TypeParameter`), so two
// structurally-identical conditionals whose `infer` parameter captured a
// resolved constraint on one path and a still-`Lazy` alias on the other (or
// merely differed in a captured default) fragmented into distinct identities,
// losing the relation's reflexive/identity fast path. This is the `Infer`
// analogue of `canonicalize_free_type_param_*`.
#[test]
fn canonicalize_infer_param_ignores_optional_default() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("R");
    let constraint = Some(TypeId::STRING);
    let make_infer = |default| {
        interner.infer(TypeParamInfo {
            name,
            constraint,
            default,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        })
    };

    let infer_with_default = make_infer(Some(TypeId::NUMBER));
    let infer_no_default = make_infer(None);

    assert_ne!(
        infer_with_default, infer_no_default,
        "precondition: interning keeps the default distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(infer_with_default),
        c2.canonicalize(infer_no_default),
        "an infer parameter's default must not fragment canonical identity"
    );
}

// The same convergence the free-`TypeParameter` branch gives for a constraint
// captured as a still-`Lazy` alias vs its expanded body must hold for `infer`
// parameters too (`infer R extends SomeCrossFileAlias`), restoring the
// relation's reflexive short-circuit (#13609 `507`/`516` axis).
#[test]
fn canonicalize_infer_param_constraint_resolution_state_converges() {
    let interner = TypeInterner::new();
    let union_body = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let resolver = AliasConstraintResolver { body: union_body };

    let r = interner.intern_string("R");
    let make_infer = |constraint| {
        interner.infer(TypeParamInfo {
            name: r,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        })
    };
    let infer_lazy = make_infer(interner.lazy(DefId(1)));
    let infer_resolved = make_infer(union_body);

    assert_ne!(
        infer_lazy, infer_resolved,
        "precondition: interning keeps the two constraint snapshots distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &resolver);
    let mut c2 = Canonicalizer::new(&interner, &resolver);
    assert_eq!(
        c1.canonicalize(infer_lazy),
        c2.canonicalize(infer_resolved),
        "an infer parameter's constraint resolution state must not fragment identity"
    );
}

// Identity-widening must not erase genuine distinctions: the normalization only
// drops `default` and converges the constraint's resolution state. A different
// constraint *shape* or a different binder name must stay distinct, and the
// rule is name-agnostic (anti-hardcoding gate).
#[test]
fn canonicalize_infer_param_preserves_real_distinctions() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make_infer = |name, constraint, default| {
        interner.infer(TypeParamInfo {
            name,
            constraint: Some(constraint),
            default,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        })
    };
    let r = interner.intern_string("R");
    let infer_r = make_infer(r, TypeId::STRING, None);
    // Same parameter, differing only in a captured default — collapses.
    let infer_r_default = make_infer(r, TypeId::STRING, Some(TypeId::NUMBER));
    // Different constraint shape — stays distinct.
    let infer_r_num = make_infer(r, TypeId::NUMBER, None);
    // Different binder name — stays distinct.
    let infer_s = make_infer(interner.intern_string("S"), TypeId::STRING, None);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    let mut c5 = Canonicalizer::new(&interner, &env);

    assert_eq!(
        c1.canonicalize(infer_r),
        c2.canonicalize(infer_r_default),
        "infer params differing only in default canonicalize identically"
    );
    assert_ne!(
        c3.canonicalize(infer_r),
        c4.canonicalize(infer_r_num),
        "distinct infer constraints stay distinct after dropping default"
    );
    assert_ne!(
        c3.canonicalize(infer_r),
        c5.canonicalize(infer_s),
        "distinct infer parameter names stay distinct"
    );
}

// ===================================================================
// The `const` modifier must not fragment canonical type-parameter identity
// (#13609 — the same family as the dropped `default` and the `Infer` arm)
// ===================================================================

// `tsc` identifies a type parameter by itself (its name and the *shape* of its
// constraint), never by the `const` modifier. `const` (`<const R>`) is an
// inference-site modifier that preserves literal types at call sites
// (`compareTypeParametersIdentical` compares constraints only) — it is erased
// from the parameter's type identity, exactly like `default`. `TypeParamInfo`
// derives `Eq`/`Hash` over `is_const`, so a free reference to a `const`
// parameter and one to its non-`const` twin intern to distinct `TypeId`s; both
// must canonicalize to one identity or the relation's reflexive/identity fast
// path fragments.
#[test]
fn canonicalize_free_type_param_ignores_const_modifier() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("R");
    let constraint = Some(TypeId::STRING);
    let make = |is_const| {
        interner.type_param(TypeParamInfo {
            name,
            constraint,
            default: None,
            is_const,
            origin: crate::types::TypeParamOrigin::User,
        })
    };

    let r_const = make(true);
    let r_plain = make(false);

    assert_ne!(
        r_const, r_plain,
        "precondition: interning keeps the `const` modifier distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(r_const),
        c2.canonicalize(r_plain),
        "the `const` modifier must not fragment canonical identity"
    );

    // The `const` and `default` drops compose: a `const` param with a captured
    // default still collapses onto its bare twin.
    let r_const_default = interner.type_param(TypeParamInfo {
        name,
        constraint,
        default: Some(TypeId::NUMBER),
        is_const: true,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c3.canonicalize(r_const_default),
        c4.canonicalize(r_plain),
        "dropping `const` and `default` compose to one identity"
    );
}

// The same identity rule applies when the type parameter is declared in a
// signature's type-parameter list: two generic function types that differ only
// in a type parameter's `const` modifier must canonicalize to one identity, so
// the relation's reflexive short-circuit holds for `<const R extends X>() => R`
// against `<R extends X>() => R`.
#[test]
fn canonicalize_function_type_param_list_ignores_const_modifier() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let r = interner.intern_string("R");
    let make = |is_const: bool| {
        let body = interner.type_param(TypeParamInfo {
            name: r,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const,
            origin: crate::types::TypeParamOrigin::User,
        });
        interner.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: r,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const,
                origin: crate::types::TypeParamOrigin::User,
            }],
            params: vec![ParamInfo::required(interner.intern_string("x"), body)],
            this_type: None,
            return_type: body,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let const_fn = make(true);
    let plain_fn = make(false);
    assert_ne!(
        const_fn, plain_fn,
        "precondition: interning keeps the `const` modifier distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(const_fn),
        c2.canonicalize(plain_fn),
        "the `const` modifier in a signature list must not fragment identity"
    );
}

// The `Infer` arm follows the same rule: `infer R` differing only in the
// `const` modifier must canonicalize identically, while genuine distinctions
// (name, constraint) stay distinct. Anti-hardcoding: the rule keys on the
// structural `is_const` flag, not on any binder name.
#[test]
fn canonicalize_infer_param_ignores_const_modifier() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("R");
    let constraint = Some(TypeId::STRING);
    let make_infer = |is_const| {
        interner.infer(TypeParamInfo {
            name,
            constraint,
            default: None,
            is_const,
            origin: crate::types::TypeParamOrigin::User,
        })
    };

    let infer_const = make_infer(true);
    let infer_plain = make_infer(false);
    assert_ne!(
        infer_const, infer_plain,
        "precondition: interning keeps the `const` modifier distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(infer_const),
        c2.canonicalize(infer_plain),
        "an infer parameter's `const` modifier must not fragment canonical identity"
    );

    // Negative control: dropping `const` must not merge genuinely different
    // parameters (different constraint shape stays distinct).
    let infer_const_num = interner.infer(TypeParamInfo {
        name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: true,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    assert_ne!(
        c3.canonicalize(infer_plain),
        c4.canonicalize(infer_const_num),
        "a genuinely different constraint stays distinct after dropping `const`"
    );
}

// ===================================================================
// Value-name axis: parameter names / tuple labels / index-key names /
// predicate target identifiers are cosmetic and must not fragment
// canonical structural identity (#13609, value-name analogue of #14096).
// ===================================================================

#[test]
fn canonicalize_function_value_param_names_alpha_equivalent() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `(<name>: string) => number` — only the value-parameter name varies.
    let make = |name: &str| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::required(
                interner.intern_string(name),
                TypeId::STRING,
            )],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make("a")),
        c2.canonicalize(make("verbose_name")),
        "functions differing only in a value-parameter name are the same type \
         and must share one canonical form"
    );
}

#[test]
fn canonicalize_function_param_type_and_arity_stay_distinct() {
    // Negative control: dropping the name must not merge functions whose
    // parameter *types*, arity, or optional/rest flags differ.
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let one = |ty: TypeId, optional: bool, rest: bool| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("p")),
                type_id: ty,
                optional,
                rest,
                arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let string_param = one(TypeId::STRING, false, false);
    let number_param = one(TypeId::NUMBER, false, false);
    let optional_param = one(TypeId::STRING, true, false);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let cs = c1.canonicalize(string_param);
    let cn = c2.canonicalize(number_param);
    let co = c3.canonicalize(optional_param);
    assert_ne!(cs, cn, "different parameter types must stay distinct");
    assert_ne!(cs, co, "optional vs required must stay distinct");
}

#[test]
fn canonicalize_tuple_labels_alpha_equivalent() {
    use crate::types::TupleElement;
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `[<label>: number, number]` vs an unlabeled `[number, number]`.
    let labeled = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("first")),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("second")),
            optional: false,
            rest: false,
        },
    ]);
    let unlabeled = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(labeled),
        c2.canonicalize(unlabeled),
        "tuple labels are cosmetic and must not fragment canonical identity"
    );
}

#[test]
fn canonicalize_tuple_optional_and_element_type_stay_distinct() {
    // Negative control: labels drop, but optionality and element type are
    // identity-bearing.
    use crate::types::TupleElement;
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let elem = |ty: TypeId, optional: bool| {
        interner.tuple(vec![TupleElement {
            type_id: ty,
            name: Some(interner.intern_string("x")),
            optional,
            rest: false,
        }])
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let req_num = c1.canonicalize(elem(TypeId::NUMBER, false));
    let opt_num = c2.canonicalize(elem(TypeId::NUMBER, true));
    let req_str = c3.canonicalize(elem(TypeId::STRING, false));
    assert_ne!(req_num, opt_num, "optional element must stay distinct");
    assert_ne!(
        req_num, req_str,
        "different element type must stay distinct"
    );
}

#[test]
fn canonicalize_index_signature_key_name_alpha_equivalent() {
    use crate::types::{IndexSignature, ObjectShape};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `{ [<key>: string]: number }` — only the cosmetic key name varies.
    let make = |key_name: &str| {
        interner.object_with_index(ObjectShape {
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: Some(interner.intern_string(key_name)),
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
            flags: Default::default(),
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make("k")),
        c2.canonicalize(make("key")),
        "index-signature key names are cosmetic and must not fragment identity"
    );
}

#[test]
fn canonicalize_index_signature_readonly_and_value_stay_distinct() {
    // Negative control: key name drops, but readonly and value type are
    // identity-bearing.
    use crate::types::{IndexSignature, ObjectShape};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |value: TypeId, readonly: bool| {
        interner.object_with_index(ObjectShape {
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: value,
                readonly,
                param_name: Some(interner.intern_string("k")),
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
            flags: Default::default(),
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mutable_num = c1.canonicalize(make(TypeId::NUMBER, false));
    let readonly_num = c2.canonicalize(make(TypeId::NUMBER, true));
    let mutable_str = c3.canonicalize(make(TypeId::STRING, false));
    assert_ne!(mutable_num, readonly_num, "readonly must stay distinct");
    assert_ne!(mutable_num, mutable_str, "value type must stay distinct");
}

#[test]
fn canonicalize_predicate_identifier_target_alpha_equivalent() {
    use crate::types::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `(<name>: unknown): <name> is string` — both the value-parameter name and
    // the predicate target identifier vary, but `parameter_index` is the same.
    let make = |name: &str| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::required(
                interner.intern_string(name),
                TypeId::UNKNOWN,
            )],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(interner.intern_string(name)),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make("x")),
        c2.canonicalize(make("value")),
        "predicate functions differing only in the referenced parameter name \
         are the same type and must share one canonical form"
    );
}

#[test]
fn canonicalize_predicate_asserts_and_narrowed_type_stay_distinct() {
    // Negative control: the identifier name drops, but `asserts`, the narrowed
    // type, and the `This`/`Identifier` discriminant are identity-bearing.
    use crate::types::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |asserts: bool, narrowed: TypeId, target: TypePredicateTarget| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::required(
                interner.intern_string("x"),
                TypeId::UNKNOWN,
            )],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts,
                target,
                type_id: Some(narrowed),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        })
    };

    let ident = || TypePredicateTarget::Identifier(interner.intern_string("x"));
    let base = make(false, TypeId::STRING, ident());

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    let cbase = c1.canonicalize(base);
    let casserts = c2.canonicalize(make(true, TypeId::STRING, ident()));
    let cnarrow = c3.canonicalize(make(false, TypeId::NUMBER, ident()));
    let cthis = c4.canonicalize(make(false, TypeId::STRING, TypePredicateTarget::This));
    assert_ne!(cbase, casserts, "`asserts` must stay distinct");
    assert_ne!(cbase, cnarrow, "narrowed type must stay distinct");
    assert_ne!(
        cbase, cthis,
        "`this` vs identifier target must stay distinct"
    );
}
