//! Unit tests for `type_queries::extended_constructors` classifier helpers.
//!
//! Pins each `TypeData` variant -> classifier-kind row for the seven public
//! helpers exposed from this module. They are consumed by the checker for
//! `new` expressions, abstract-class diagnostics (TS2511 / TS2515), interface
//! merging, and base-instance property merging — silent drift (a new
//! `TypeData` variant slipping into the wrong arm) would only surface as
//! flaky downstream diagnostics, so every variant -> kind row is locked here.
//!
//! Helpers covered:
//! - `classify_for_abstract_check`
//! - `classify_for_class_decl`
//! - `classify_for_constructor_check`
//! - `classify_for_instance_type`
//! - `classify_for_constructor_return_merge`
//! - `classify_for_base_instance_merge`
//! - `resolve_abstract_constructor_anchor`

use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::type_queries::extended_constructors::{
    AbstractClassCheckKind, AbstractConstructorAnchor, BaseInstanceMergeKind, ClassDeclTypeKind,
    ConstructorCheckKind, ConstructorReturnMergeKind, InstanceTypeKind,
    classify_for_abstract_check, classify_for_base_instance_merge, classify_for_class_decl,
    classify_for_constructor_check, classify_for_constructor_return_merge,
    classify_for_instance_type, resolve_abstract_constructor_anchor,
};
use crate::types::{
    CallableShape, ConditionalType, FunctionShape, IndexSignature, ObjectFlags, ObjectShape,
    PropertyInfo, SymbolRef, TypeData, TypeParamInfo,
};

// =============================================================================
// Helpers
// =============================================================================

/// Build a fresh, distinct type parameter (used to build a non-collapsing
/// intersection). The interner cannot prove disjointness or merger between
/// two type parameters, so `intersection(vec![tp_a, tp_b])` survives.
fn fresh_type_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.type_param(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    })
}

/// Build a non-collapsing intersection of two distinct type parameters.
fn distinct_intersection(interner: &TypeInterner) -> (TypeId, TypeId, TypeId) {
    let tp_a = fresh_type_param(interner, "A");
    let tp_b = fresh_type_param(interner, "B");
    let isect = interner.intersection(vec![tp_a, tp_b]);
    (tp_a, tp_b, isect)
}

// =============================================================================
// classify_for_abstract_check
// =============================================================================

#[test]
fn abstract_check_type_query_returns_type_query() {
    let interner = TypeInterner::new();
    let tq = interner.type_query(SymbolRef(7));
    match classify_for_abstract_check(&interner, tq) {
        AbstractClassCheckKind::TypeQuery(sym) => assert_eq!(sym, SymbolRef(7)),
        other => panic!("expected TypeQuery, got {other:?}"),
    }
}

#[test]
fn abstract_check_union_returns_union_members() {
    let interner = TypeInterner::new();
    let tq_a = interner.type_query(SymbolRef(1));
    let tq_b = interner.type_query(SymbolRef(2));
    let union = interner.union(vec![tq_a, tq_b]);
    match classify_for_abstract_check(&interner, union) {
        AbstractClassCheckKind::Union(members) => {
            assert!(members.contains(&tq_a));
            assert!(members.contains(&tq_b));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn abstract_check_intersection_returns_intersection_members() {
    let interner = TypeInterner::new();
    let (tp_a, tp_b, isect) = distinct_intersection(&interner);
    match classify_for_abstract_check(&interner, isect) {
        AbstractClassCheckKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn abstract_check_intrinsic_is_not_abstract() {
    let interner = TypeInterner::new();
    for ty in [
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BOOLEAN,
        TypeId::ERROR,
    ] {
        assert!(
            matches!(
                classify_for_abstract_check(&interner, ty),
                AbstractClassCheckKind::NotAbstract
            ),
            "intrinsic {ty:?} must classify as NotAbstract",
        );
    }
}

#[test]
fn abstract_check_object_is_not_abstract() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(matches!(
        classify_for_abstract_check(&interner, obj),
        AbstractClassCheckKind::NotAbstract
    ));
}

#[test]
fn abstract_check_function_callable_lazy_array_are_not_abstract() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let callable = interner.callable(CallableShape::default());
    let lazy = interner.lazy(DefId(11));
    let arr = interner.array(TypeId::NUMBER);
    for ty in [func, callable, lazy, arr] {
        assert!(matches!(
            classify_for_abstract_check(&interner, ty),
            AbstractClassCheckKind::NotAbstract
        ));
    }
}

// =============================================================================
// classify_for_class_decl
// =============================================================================

#[test]
fn class_decl_object_returns_object_shape() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(matches!(
        classify_for_class_decl(&interner, obj),
        ClassDeclTypeKind::Object(_)
    ));
}

#[test]
fn class_decl_object_with_index_returns_object_shape_via_shared_arm() {
    let interner = TypeInterner::new();
    let owi = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });
    assert!(
        matches!(
            classify_for_class_decl(&interner, owi),
            ClassDeclTypeKind::Object(_)
        ),
        "ObjectWithIndex must share the Object arm in classify_for_class_decl",
    );
}

#[test]
fn class_decl_union_returns_members() {
    let interner = TypeInterner::new();
    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let union = interner.union(vec![obj_a, obj_b]);
    match classify_for_class_decl(&interner, union) {
        ClassDeclTypeKind::Members(members) => {
            assert!(members.contains(&obj_a));
            assert!(members.contains(&obj_b));
        }
        other => panic!("expected Members, got {other:?}"),
    }
}

#[test]
fn class_decl_intersection_returns_members_via_shared_arm() {
    let interner = TypeInterner::new();
    let (tp_a, tp_b, isect) = distinct_intersection(&interner);
    match classify_for_class_decl(&interner, isect) {
        ClassDeclTypeKind::Members(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => {
            panic!("expected Members (Intersection arm shares the Members tag), got {other:?}")
        }
    }
}

#[test]
fn class_decl_intrinsics_and_function_are_not_object() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let callable = interner.callable(CallableShape::default());
    let arr = interner.array(TypeId::NUMBER);
    for ty in [
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::ANY,
        func,
        callable,
        arr,
    ] {
        assert!(matches!(
            classify_for_class_decl(&interner, ty),
            ClassDeclTypeKind::NotObject
        ));
    }
}

// =============================================================================
// classify_for_constructor_check
// =============================================================================

#[test]
fn constructor_check_type_parameter_carries_constraint() {
    let interner = TypeInterner::new();
    let constraint = interner.array(TypeId::NUMBER);
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    match classify_for_constructor_check(&interner, tp) {
        ConstructorCheckKind::TypeParameter { constraint: c } => assert_eq!(c, Some(constraint)),
        other => panic!("expected TypeParameter, got {other:?}"),
    }
}

#[test]
fn constructor_check_infer_shares_type_parameter_arm() {
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
            classify_for_constructor_check(&interner, infer_ty),
            ConstructorCheckKind::TypeParameter { constraint: None }
        ),
        "Infer must classify as TypeParameter (shared arm)",
    );
}

#[test]
fn constructor_check_intersection_returns_members() {
    let interner = TypeInterner::new();
    let (tp_a, tp_b, isect) = distinct_intersection(&interner);
    match classify_for_constructor_check(&interner, isect) {
        ConstructorCheckKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn constructor_check_union_returns_members() {
    let interner = TypeInterner::new();
    let lazy_a = interner.lazy(DefId(1));
    let lazy_b = interner.lazy(DefId(2));
    let union = interner.union(vec![lazy_a, lazy_b]);
    match classify_for_constructor_check(&interner, union) {
        ConstructorCheckKind::Union(members) => {
            assert!(members.contains(&lazy_a));
            assert!(members.contains(&lazy_b));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn constructor_check_application_returns_base() {
    let interner = TypeInterner::new();
    let base = interner.lazy(DefId(99));
    let app = interner.application(base, vec![TypeId::NUMBER]);
    match classify_for_constructor_check(&interner, app) {
        ConstructorCheckKind::Application { base: b } => assert_eq!(b, base),
        other => panic!("expected Application, got {other:?}"),
    }
}

#[test]
fn constructor_check_lazy_returns_def_id() {
    let interner = TypeInterner::new();
    let lazy = interner.lazy(DefId(123));
    match classify_for_constructor_check(&interner, lazy) {
        ConstructorCheckKind::Lazy(def) => assert_eq!(def, DefId(123)),
        other => panic!("expected Lazy, got {other:?}"),
    }
}

#[test]
fn constructor_check_type_query_returns_symbol_ref() {
    let interner = TypeInterner::new();
    let tq = interner.type_query(SymbolRef(42));
    match classify_for_constructor_check(&interner, tq) {
        ConstructorCheckKind::TypeQuery(sym) => assert_eq!(sym, SymbolRef(42)),
        other => panic!("expected TypeQuery, got {other:?}"),
    }
}

#[test]
fn constructor_check_conditional_returns_branches() {
    let interner = TypeInterner::new();
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });
    match classify_for_constructor_check(&interner, cond) {
        ConstructorCheckKind::Conditional {
            true_type,
            false_type,
        } => {
            assert_eq!(true_type, TypeId::BOOLEAN);
            assert_eq!(false_type, TypeId::NUMBER);
        }
        other => panic!("expected Conditional, got {other:?}"),
    }
}

#[test]
fn constructor_check_object_callable_function_intrinsic_default_to_other() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let callable = interner.callable(CallableShape::default());
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    for ty in [
        obj,
        callable,
        func,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BOOLEAN,
        TypeId::ANY,
        TypeId::ERROR,
    ] {
        assert!(
            matches!(
                classify_for_constructor_check(&interner, ty),
                ConstructorCheckKind::Other
            ),
            "{ty:?} must classify as Other",
        );
    }
}

// =============================================================================
// classify_for_instance_type
// =============================================================================

#[test]
fn instance_type_callable_returns_callable_shape() {
    let interner = TypeInterner::new();
    let callable = interner.callable(CallableShape::default());
    assert!(matches!(
        classify_for_instance_type(&interner, callable),
        InstanceTypeKind::Callable(_)
    ));
}

#[test]
fn instance_type_function_returns_function_shape() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    assert!(matches!(
        classify_for_instance_type(&interner, func),
        InstanceTypeKind::Function(_)
    ));
}

#[test]
fn instance_type_intersection_returns_members() {
    let interner = TypeInterner::new();
    let (tp_a, tp_b, isect) = distinct_intersection(&interner);
    match classify_for_instance_type(&interner, isect) {
        InstanceTypeKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn instance_type_union_returns_members() {
    let interner = TypeInterner::new();
    let lazy_a = interner.lazy(DefId(1));
    let lazy_b = interner.lazy(DefId(2));
    let union = interner.union(vec![lazy_a, lazy_b]);
    match classify_for_instance_type(&interner, union) {
        InstanceTypeKind::Union(members) => {
            assert!(members.contains(&lazy_a));
            assert!(members.contains(&lazy_b));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn instance_type_readonly_returns_inner() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::NUMBER);
    let ro = interner.readonly_type(arr);
    match classify_for_instance_type(&interner, ro) {
        InstanceTypeKind::Readonly(inner) => assert_eq!(inner, arr),
        other => panic!("expected Readonly, got {other:?}"),
    }
}

#[test]
fn instance_type_type_parameter_carries_constraint() {
    let interner = TypeInterner::new();
    let constraint = interner.lazy(DefId(7));
    let tp = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    match classify_for_instance_type(&interner, tp) {
        InstanceTypeKind::TypeParameter { constraint: c } => assert_eq!(c, Some(constraint)),
        other => panic!("expected TypeParameter, got {other:?}"),
    }
}

#[test]
fn instance_type_infer_shares_type_parameter_arm() {
    let interner = TypeInterner::new();
    let infer_ty = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    assert!(matches!(
        classify_for_instance_type(&interner, infer_ty),
        InstanceTypeKind::TypeParameter { constraint: None }
    ));
}

#[test]
fn instance_type_type_query_returns_symbol_ref() {
    let interner = TypeInterner::new();
    let tq = interner.type_query(SymbolRef(11));
    match classify_for_instance_type(&interner, tq) {
        InstanceTypeKind::SymbolRef(sym) => assert_eq!(sym, SymbolRef(11)),
        other => panic!("expected SymbolRef, got {other:?}"),
    }
}

#[test]
fn instance_type_conditional_needs_evaluation() {
    let interner = TypeInterner::new();
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });
    assert!(matches!(
        classify_for_instance_type(&interner, cond),
        InstanceTypeKind::NeedsEvaluation
    ));
}

#[test]
fn instance_type_keyof_needs_evaluation() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let keyof = interner.keyof(obj);
    assert!(matches!(
        classify_for_instance_type(&interner, keyof),
        InstanceTypeKind::NeedsEvaluation
    ));
}

#[test]
fn instance_type_index_access_needs_evaluation() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let ix = interner.index_access(obj, TypeId::STRING);
    assert!(matches!(
        classify_for_instance_type(&interner, ix),
        InstanceTypeKind::NeedsEvaluation
    ));
}

#[test]
fn instance_type_application_needs_evaluation() {
    let interner = TypeInterner::new();
    let base = interner.lazy(DefId(33));
    let app = interner.application(base, vec![TypeId::NUMBER]);
    assert!(matches!(
        classify_for_instance_type(&interner, app),
        InstanceTypeKind::NeedsEvaluation
    ));
}

#[test]
fn instance_type_intrinsic_object_array_are_not_constructor() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let arr = interner.array(TypeId::NUMBER);
    for ty in [TypeId::NUMBER, TypeId::STRING, TypeId::ANY, obj, arr] {
        assert!(
            matches!(
                classify_for_instance_type(&interner, ty),
                InstanceTypeKind::NotConstructor
            ),
            "{ty:?} must classify as NotConstructor",
        );
    }
}

// =============================================================================
// classify_for_constructor_return_merge
// =============================================================================

#[test]
fn ctor_return_merge_callable_returns_callable() {
    let interner = TypeInterner::new();
    let callable = interner.callable(CallableShape::default());
    assert!(matches!(
        classify_for_constructor_return_merge(&interner, callable),
        ConstructorReturnMergeKind::Callable(_)
    ));
}

#[test]
fn ctor_return_merge_function_returns_function() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    assert!(matches!(
        classify_for_constructor_return_merge(&interner, func),
        ConstructorReturnMergeKind::Function(_)
    ));
}

#[test]
fn ctor_return_merge_intersection_returns_members() {
    let interner = TypeInterner::new();
    let (tp_a, tp_b, isect) = distinct_intersection(&interner);
    match classify_for_constructor_return_merge(&interner, isect) {
        ConstructorReturnMergeKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn ctor_return_merge_union_object_intrinsic_are_other() {
    let interner = TypeInterner::new();
    let lazy_a = interner.lazy(DefId(1));
    let lazy_b = interner.lazy(DefId(2));
    let union = interner.union(vec![lazy_a, lazy_b]);
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    for ty in [union, obj, TypeId::NUMBER, TypeId::ANY, TypeId::ERROR] {
        assert!(
            matches!(
                classify_for_constructor_return_merge(&interner, ty),
                ConstructorReturnMergeKind::Other
            ),
            "{ty:?} must classify as Other (union/object/intrinsic do not merge into ctor returns)",
        );
    }
}

// =============================================================================
// classify_for_base_instance_merge
// =============================================================================

#[test]
fn base_instance_merge_object_returns_object_shape() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(matches!(
        classify_for_base_instance_merge(&interner, obj),
        BaseInstanceMergeKind::Object(_)
    ));
}

#[test]
fn base_instance_merge_object_with_index_shares_object_arm() {
    let interner = TypeInterner::new();
    let owi = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });
    assert!(
        matches!(
            classify_for_base_instance_merge(&interner, owi),
            BaseInstanceMergeKind::Object(_)
        ),
        "ObjectWithIndex must share the Object arm in classify_for_base_instance_merge",
    );
}

#[test]
fn base_instance_merge_intersection_returns_members() {
    let interner = TypeInterner::new();
    let (tp_a, tp_b, isect) = distinct_intersection(&interner);
    match classify_for_base_instance_merge(&interner, isect) {
        BaseInstanceMergeKind::Intersection(members) => {
            assert!(members.contains(&tp_a));
            assert!(members.contains(&tp_b));
        }
        other => panic!("expected Intersection, got {other:?}"),
    }
}

#[test]
fn base_instance_merge_union_returns_members() {
    let interner = TypeInterner::new();
    let lazy_a = interner.lazy(DefId(1));
    let lazy_b = interner.lazy(DefId(2));
    let union = interner.union(vec![lazy_a, lazy_b]);
    match classify_for_base_instance_merge(&interner, union) {
        BaseInstanceMergeKind::Union(members) => {
            assert!(members.contains(&lazy_a));
            assert!(members.contains(&lazy_b));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn base_instance_merge_function_callable_intrinsic_are_other() {
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let callable = interner.callable(CallableShape::default());
    let arr = interner.array(TypeId::NUMBER);
    for ty in [
        func,
        callable,
        arr,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::ANY,
        TypeId::ERROR,
    ] {
        assert!(
            matches!(
                classify_for_base_instance_merge(&interner, ty),
                BaseInstanceMergeKind::Other
            ),
            "{ty:?} must classify as Other for base-instance merge",
        );
    }
}

// =============================================================================
// resolve_abstract_constructor_anchor
// =============================================================================

#[test]
fn anchor_type_query_returns_type_query_anchor() {
    let interner = TypeInterner::new();
    let tq = interner.type_query(SymbolRef(13));
    assert_eq!(
        resolve_abstract_constructor_anchor(&interner, tq),
        AbstractConstructorAnchor::TypeQuery(SymbolRef(13)),
    );
}

#[test]
fn anchor_callable_returns_callable_type_anchor() {
    let interner = TypeInterner::new();
    let callable = interner.callable(CallableShape::default());
    assert_eq!(
        resolve_abstract_constructor_anchor(&interner, callable),
        AbstractConstructorAnchor::CallableType(callable),
    );
}

#[test]
fn anchor_application_unwraps_to_base_callable() {
    // Application(Callable, [args]) should peel one layer to the callable
    // base and return that as the CallableType anchor.
    let interner = TypeInterner::new();
    let callable = interner.callable(CallableShape::default());
    let app = interner.application(callable, vec![TypeId::NUMBER]);
    assert_eq!(
        resolve_abstract_constructor_anchor(&interner, app),
        AbstractConstructorAnchor::CallableType(callable),
    );
}

#[test]
fn anchor_application_unwraps_to_base_type_query() {
    let interner = TypeInterner::new();
    let tq = interner.type_query(SymbolRef(8));
    let app = interner.application(tq, vec![TypeId::NUMBER]);
    assert_eq!(
        resolve_abstract_constructor_anchor(&interner, app),
        AbstractConstructorAnchor::TypeQuery(SymbolRef(8)),
    );
}

#[test]
fn anchor_object_function_lazy_intrinsic_are_not_abstract() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let lazy = interner.lazy(DefId(5));
    let arr = interner.array(TypeId::NUMBER);
    for ty in [
        obj,
        func,
        lazy,
        arr,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::ANY,
        TypeId::ERROR,
    ] {
        assert_eq!(
            resolve_abstract_constructor_anchor(&interner, ty),
            AbstractConstructorAnchor::NotAbstract,
            "{ty:?} must resolve to NotAbstract",
        );
    }
}

#[test]
fn anchor_application_with_non_abstract_base_is_not_abstract() {
    // Application whose base is e.g. an Object (not callable / type query)
    // peels one layer and then falls into NotAbstract.
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let app = interner.application(obj, vec![TypeId::NUMBER]);
    assert_eq!(
        resolve_abstract_constructor_anchor(&interner, app),
        AbstractConstructorAnchor::NotAbstract,
    );
}
