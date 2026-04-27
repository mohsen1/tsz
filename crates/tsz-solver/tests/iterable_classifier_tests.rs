//! Unit tests for `type_queries::iterable` classifier helpers.
//!
//! `classify_full_iterable_type`, `classify_async_iterable_type`, and
//! `classify_for_of_element_type` each translate a `TypeData` variant into
//! a downstream-facing enum kind. They are consumed by the checker's
//! `iterable_checker.rs` (10+ call sites, used for spread / for-of /
//! async iteration) and by the solver's iterable boundary helper. A new
//! `TypeData` variant silently slipping into the wrong arm would only
//! surface as flaky for-of / spread / async iterable diagnostics
//! downstream — so we lock every variant -> kind row here.

use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::type_queries::iterable::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind, classify_async_iterable_type,
    classify_for_of_element_type, classify_full_iterable_type,
};
use crate::types::{
    CallableShape, FunctionShape, IndexSignature, LiteralValue, MappedModifier, MappedType,
    ObjectFlags, ObjectShape, OrderedFloat, ParamInfo, PropertyInfo, StringIntrinsicKind,
    SymbolRef, TemplateSpan, TupleElement, TypeData, TypeParamInfo,
};

// =============================================================================
// classify_full_iterable_type
// =============================================================================

#[test]
fn full_iterable_array_returns_array_with_element_type() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    match classify_full_iterable_type(&interner, arr) {
        FullIterableTypeKind::Array(elem) => assert_eq!(elem, TypeId::NUMBER),
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn full_iterable_tuple_returns_tuple_elements() {
    let interner = TypeInterner::new();
    let tup = interner.tuple(vec![
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
    match classify_full_iterable_type(&interner, tup) {
        FullIterableTypeKind::Tuple(elements) => {
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn full_iterable_string_literal_returns_string_literal_atom() {
    let interner = TypeInterner::new();
    let atom = interner.intern_string("hi");
    let lit = interner.intern(TypeData::Literal(LiteralValue::String(atom)));
    match classify_full_iterable_type(&interner, lit) {
        FullIterableTypeKind::StringLiteral(a) => assert_eq!(a, atom),
        other => panic!("expected StringLiteral, got {other:?}"),
    }
}

#[test]
fn full_iterable_union_returns_union_members() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    let union = interner.union(vec![arr, TypeId::STRING]);
    match classify_full_iterable_type(&interner, union) {
        FullIterableTypeKind::Union(members) => {
            assert!(members.contains(&arr));
            assert!(members.contains(&TypeId::STRING));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn full_iterable_intersection_returns_intersection_members() {
    let interner = TypeInterner::new();
    // Use type parameters: their intersection is never simplified by the
    // interner because the resolver cannot prove disjointness or merger.
    // Distinct primitive intersections collapse to `never`; distinct
    // object literals collapse into a single merged `ObjectShape`.
    let tp_a = interner.type_param(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let tp_b = interner.type_param(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let isect = interner.intersection(vec![tp_a, tp_b]);
    match classify_full_iterable_type(&interner, isect) {
        FullIterableTypeKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn full_iterable_object_returns_object_shape() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let kind = classify_full_iterable_type(&interner, obj);
    assert!(
        matches!(kind, FullIterableTypeKind::Object(_)),
        "expected Object, got {kind:?}"
    );
}

#[test]
fn full_iterable_application_returns_application_with_base() {
    let interner = TypeInterner::new();
    let base = interner.lazy(DefId(42));
    let app = interner.application(base, vec![TypeId::NUMBER]);
    match classify_full_iterable_type(&interner, app) {
        FullIterableTypeKind::Application { base: b } => assert_eq!(b, base),
        other => panic!("expected Application, got {other:?}"),
    }
}

#[test]
fn full_iterable_type_parameter_carries_constraint() {
    let interner = TypeInterner::new();
    let constraint = interner.array(TypeId::STRING);
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    match classify_full_iterable_type(&interner, tp) {
        FullIterableTypeKind::TypeParameter { constraint: c } => assert_eq!(c, Some(constraint)),
        other => panic!("expected TypeParameter, got {other:?}"),
    }
}

#[test]
fn full_iterable_type_parameter_without_constraint_yields_none() {
    let interner = TypeInterner::new();
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    match classify_full_iterable_type(&interner, tp) {
        FullIterableTypeKind::TypeParameter { constraint } => assert_eq!(constraint, None),
        other => panic!("expected TypeParameter, got {other:?}"),
    }
}

#[test]
fn full_iterable_infer_type_carries_constraint_like_type_parameter() {
    let interner = TypeInterner::new();
    let infer_ty = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    assert!(
        matches!(
            classify_full_iterable_type(&interner, infer_ty),
            FullIterableTypeKind::TypeParameter { constraint: None }
        ),
        "Infer must classify as TypeParameter (shared arm with TypeData::TypeParameter)",
    );
}

#[test]
fn full_iterable_readonly_returns_inner() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    let ro = interner.readonly_type(arr);
    match classify_full_iterable_type(&interner, ro) {
        FullIterableTypeKind::Readonly(inner) => assert_eq!(inner, arr),
        other => panic!("expected Readonly, got {other:?}"),
    }
}

#[test]
fn full_iterable_no_infer_treated_as_readonly_inner() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::STRING);
    // `NoInfer<T>` is mapped onto the `Readonly` arm because the classifier
    // unwraps the inner type and lets downstream re-classify.
    let no_infer = interner.intern(TypeData::NoInfer(arr));
    match classify_full_iterable_type(&interner, no_infer) {
        FullIterableTypeKind::Readonly(inner) => assert_eq!(inner, arr),
        other => panic!("expected Readonly (NoInfer arm), got {other:?}"),
    }
}

#[test]
fn full_iterable_function_is_function_or_callable() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(TypeId::NUMBER)],
        TypeId::STRING,
    ));
    assert!(
        matches!(
            classify_full_iterable_type(&interner, func),
            FullIterableTypeKind::FunctionOrCallable
        ),
        "Function must classify as FunctionOrCallable",
    );
}

#[test]
fn full_iterable_callable_is_function_or_callable() {
    let interner = TypeInterner::new();
    let callable = interner.callable(CallableShape::default());
    assert!(
        matches!(
            classify_full_iterable_type(&interner, callable),
            FullIterableTypeKind::FunctionOrCallable
        ),
        "Callable must classify as FunctionOrCallable",
    );
}

#[test]
fn full_iterable_index_access_is_complex() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let key = interner.literal_string("x");
    let ia = interner.index_access(obj, key);
    assert!(
        matches!(
            classify_full_iterable_type(&interner, ia),
            FullIterableTypeKind::ComplexType
        ),
        "IndexAccess must classify as ComplexType",
    );
}

#[test]
fn full_iterable_keyof_is_not_iterable() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let ko = interner.keyof(obj);
    // KeyOf is a deferred operation but is not handled as ComplexType
    // — it falls into the "all other deferred operators" NotIterable arm.
    assert!(
        matches!(
            classify_full_iterable_type(&interner, ko),
            FullIterableTypeKind::NotIterable
        ),
        "KeyOf must classify as NotIterable",
    );
}

#[test]
fn full_iterable_intrinsic_primitives_are_not_iterable() {
    let interner = TypeInterner::new();
    for ty in [
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::BIGINT,
        TypeId::NULL,
        TypeId::UNDEFINED,
        TypeId::VOID,
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::SYMBOL,
    ] {
        assert!(
            matches!(
                classify_full_iterable_type(&interner, ty),
                FullIterableTypeKind::NotIterable
            ),
            "{ty:?} must classify as NotIterable",
        );
    }
}

#[test]
fn full_iterable_number_literal_is_not_iterable() {
    // String literals are iterable; non-string literals are not.
    let interner = TypeInterner::new();
    let n = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0))));
    let b = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    let bi = interner.intern(TypeData::Literal(LiteralValue::BigInt(
        interner.intern_string("1"),
    )));
    for ty in [n, b, bi] {
        assert!(
            matches!(
                classify_full_iterable_type(&interner, ty),
                FullIterableTypeKind::NotIterable
            ),
            "non-string literal {ty:?} must classify as NotIterable",
        );
    }
}

#[test]
fn full_iterable_template_literal_is_not_iterable() {
    // Even though `string` is iterable in TS, a template-literal *type* is a
    // type-level pattern, not the iterable string runtime value, and the
    // classifier intentionally keeps it on the `NotIterable` arm.
    let interner = TypeInterner::new();
    let tl = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    assert!(matches!(
        classify_full_iterable_type(&interner, tl),
        FullIterableTypeKind::NotIterable
    ));
}

#[test]
fn full_iterable_unique_symbol_and_this_type_are_not_iterable() {
    let interner = TypeInterner::new();
    let unique = interner.intern(TypeData::UniqueSymbol(SymbolRef(7)));
    let this_ty = interner.intern(TypeData::ThisType);
    for ty in [unique, this_ty] {
        assert!(matches!(
            classify_full_iterable_type(&interner, ty),
            FullIterableTypeKind::NotIterable
        ));
    }
}

#[test]
fn full_iterable_type_query_is_not_iterable() {
    let interner = TypeInterner::new();
    let tq = interner.type_query(SymbolRef(3));
    assert!(matches!(
        classify_full_iterable_type(&interner, tq),
        FullIterableTypeKind::NotIterable
    ));
}

#[test]
fn full_iterable_string_intrinsic_is_not_iterable() {
    let interner = TypeInterner::new();
    let upper = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: TypeId::STRING,
    });
    assert!(matches!(
        classify_full_iterable_type(&interner, upper),
        FullIterableTypeKind::NotIterable
    ));
}

#[test]
fn full_iterable_module_namespace_is_not_iterable() {
    let interner = TypeInterner::new();
    let ns = interner.intern(TypeData::ModuleNamespace(SymbolRef(11)));
    assert!(matches!(
        classify_full_iterable_type(&interner, ns),
        FullIterableTypeKind::NotIterable
    ));
}

#[test]
fn full_iterable_lazy_and_unresolved_are_not_iterable() {
    let interner = TypeInterner::new();
    let lazy = interner.lazy(DefId(99));
    let unresolved = interner.intern(TypeData::UnresolvedTypeName(interner.intern_string("Foo")));
    let err = TypeId::ERROR;
    for ty in [lazy, unresolved, err] {
        assert!(matches!(
            classify_full_iterable_type(&interner, ty),
            FullIterableTypeKind::NotIterable
        ));
    }
}

#[test]
fn full_iterable_mapped_is_complex() {
    let interner = TypeInterner::new();
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });
    assert!(matches!(
        classify_full_iterable_type(&interner, mapped),
        FullIterableTypeKind::ComplexType
    ));
}

// =============================================================================
// classify_async_iterable_type
// =============================================================================

#[test]
fn async_iterable_union_returns_union_members() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let union = interner.union(vec![obj, TypeId::STRING]);
    match classify_async_iterable_type(&interner, union) {
        AsyncIterableTypeKind::Union(members) => {
            assert!(members.contains(&obj));
            assert!(members.contains(&TypeId::STRING));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn async_iterable_object_returns_object_shape() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("next"),
        TypeId::ANY,
    )]);
    assert!(matches!(
        classify_async_iterable_type(&interner, obj),
        AsyncIterableTypeKind::Object(_)
    ));
}

#[test]
fn async_iterable_readonly_returns_inner() {
    let interner = TypeInterner::new();
    let inner = interner.array(TypeId::NUMBER);
    let ro = interner.readonly_type(inner);
    match classify_async_iterable_type(&interner, ro) {
        AsyncIterableTypeKind::Readonly(t) => assert_eq!(t, inner),
        other => panic!("expected Readonly, got {other:?}"),
    }
}

#[test]
fn async_iterable_array_is_not_async_iterable() {
    // `Array<T>` is sync iterable, NOT async iterable. The classifier must
    // explicitly NOT short-circuit Array into Object.
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    assert!(matches!(
        classify_async_iterable_type(&interner, arr),
        AsyncIterableTypeKind::NotAsyncIterable
    ));
}

#[test]
fn async_iterable_tuple_is_not_async_iterable() {
    let interner = TypeInterner::new();
    let tup = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert!(matches!(
        classify_async_iterable_type(&interner, tup),
        AsyncIterableTypeKind::NotAsyncIterable
    ));
}

#[test]
fn async_iterable_string_literal_is_not_async_iterable() {
    let interner = TypeInterner::new();
    let s = interner.literal_string("x");
    assert!(matches!(
        classify_async_iterable_type(&interner, s),
        AsyncIterableTypeKind::NotAsyncIterable
    ));
}

#[test]
fn async_iterable_intrinsics_are_not_async_iterable() {
    let interner = TypeInterner::new();
    for ty in [
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::SYMBOL,
        TypeId::NULL,
        TypeId::UNDEFINED,
        TypeId::ERROR,
    ] {
        assert!(
            matches!(
                classify_async_iterable_type(&interner, ty),
                AsyncIterableTypeKind::NotAsyncIterable
            ),
            "{ty:?} must classify as NotAsyncIterable",
        );
    }
}

#[test]
fn async_iterable_object_with_index_returns_object_shape() {
    let interner = TypeInterner::new();
    // ObjectWithIndex shares the Object arm. Use a property with an
    // index-signature-shaped object_with_index call.
    let shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("next"),
            TypeId::ANY,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    };
    let obj_idx = interner.object_with_index(shape);
    assert!(matches!(
        classify_async_iterable_type(&interner, obj_idx),
        AsyncIterableTypeKind::Object(_)
    ));
}

// =============================================================================
// classify_for_of_element_type
// =============================================================================

#[test]
fn for_of_array_returns_array_element_type() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    match classify_for_of_element_type(&interner, arr) {
        ForOfElementKind::Array(elem) => assert_eq!(elem, TypeId::NUMBER),
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn for_of_tuple_returns_tuple_elements() {
    let interner = TypeInterner::new();
    let tup = interner.tuple(vec![
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
    match classify_for_of_element_type(&interner, tup) {
        ForOfElementKind::Tuple(elements) => {
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn for_of_union_returns_union_members() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    let union = interner.union(vec![arr, TypeId::STRING]);
    match classify_for_of_element_type(&interner, union) {
        ForOfElementKind::Union(members) => {
            assert!(members.contains(&arr));
            assert!(members.contains(&TypeId::STRING));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn for_of_intersection_returns_intersection_members() {
    let interner = TypeInterner::new();
    // See `full_iterable_intersection_returns_intersection_members` for why
    // we use type parameters here — primitive and object intersections both
    // collapse before reaching the classifier.
    let tp_a = interner.type_param(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let tp_b = interner.type_param(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let isect = interner.intersection(vec![tp_a, tp_b]);
    match classify_for_of_element_type(&interner, isect) {
        ForOfElementKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn for_of_readonly_returns_inner() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    let ro = interner.readonly_type(arr);
    match classify_for_of_element_type(&interner, ro) {
        ForOfElementKind::Readonly(inner) => assert_eq!(inner, arr),
        other => panic!("expected Readonly, got {other:?}"),
    }
}

#[test]
fn for_of_no_infer_unwraps_via_readonly_arm() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    let no_infer = interner.intern(TypeData::NoInfer(arr));
    match classify_for_of_element_type(&interner, no_infer) {
        ForOfElementKind::Readonly(inner) => assert_eq!(inner, arr),
        other => panic!("expected Readonly (NoInfer arm), got {other:?}"),
    }
}

#[test]
fn for_of_string_literal_returns_string_kind() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("hello");
    assert!(matches!(
        classify_for_of_element_type(&interner, lit),
        ForOfElementKind::String
    ));
}

#[test]
fn for_of_object_returns_other() {
    // Plain object (not an array/tuple/etc.) reaches the iterator-protocol
    // fallback path via the `Other` arm — the classifier intentionally does
    // NOT inline iterable detection for arbitrary objects.
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(matches!(
        classify_for_of_element_type(&interner, obj),
        ForOfElementKind::Other
    ));
}

#[test]
fn for_of_intrinsic_string_is_other_not_string_kind() {
    // Only the *literal string* type maps to ForOfElementKind::String. The
    // intrinsic `string` falls through to `Other`, where the iterator
    // protocol resolves it. This row guards against accidentally widening
    // the `String` arm to cover the primitive too.
    let interner = TypeInterner::new();
    assert!(matches!(
        classify_for_of_element_type(&interner, TypeId::STRING),
        ForOfElementKind::Other
    ));
}

#[test]
fn for_of_number_literal_is_other() {
    let interner = TypeInterner::new();
    let n = interner.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(7.0))));
    assert!(matches!(
        classify_for_of_element_type(&interner, n),
        ForOfElementKind::Other
    ));
}

#[test]
fn for_of_intrinsics_default_to_other() {
    let interner = TypeInterner::new();
    for ty in [
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::ERROR,
    ] {
        assert!(
            matches!(
                classify_for_of_element_type(&interner, ty),
                ForOfElementKind::Other
            ),
            "{ty:?} must classify as Other",
        );
    }
}

#[test]
fn for_of_function_callable_and_lazy_default_to_other() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let callable = interner.callable(CallableShape::default());
    let lazy = interner.lazy(DefId(5));
    for ty in [func, callable, lazy] {
        assert!(matches!(
            classify_for_of_element_type(&interner, ty),
            ForOfElementKind::Other
        ));
    }
}
