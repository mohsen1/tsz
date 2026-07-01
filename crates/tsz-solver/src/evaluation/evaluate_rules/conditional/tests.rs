use super::*;
use crate::caches::db::TypeApplicationEvalCache;
use crate::caches::query_cache::QueryCache;
use crate::evaluation::result::TerminationKind;
use crate::intern::TypeInterner;
use crate::types::{PropertyInfo, TypeId};

#[test]
fn test_is_primitive_vs_function_intrinsic() {
    let interner = TypeInterner::new();
    // Primitives should match against TypeId::FUNCTION
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::STRING,
            TypeId::FUNCTION
        )
    );
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::NUMBER,
            TypeId::FUNCTION
        )
    );
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::BOOLEAN,
            TypeId::FUNCTION
        )
    );
    // Non-primitives should not match
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::OBJECT,
            TypeId::FUNCTION
        )
    );
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::ANY,
            TypeId::FUNCTION
        )
    );
    // Primitives against non-Function target should not match
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::STRING,
            TypeId::OBJECT
        )
    );
}

#[test]
fn test_is_primitive_vs_function_structural() {
    let interner = TypeInterner::new();
    // Create an ObjectShape that looks like Function (has apply, call, bind)
    let apply = interner.intern_string("apply");
    let call = interner.intern_string("call");
    let bind = interner.intern_string("bind");
    let function_shape = interner.object(vec![
        crate::types::PropertyInfo {
            name: apply,
            type_id: TypeId::ANY,
            ..Default::default()
        },
        crate::types::PropertyInfo {
            name: call,
            type_id: TypeId::ANY,
            ..Default::default()
        },
        crate::types::PropertyInfo {
            name: bind,
            type_id: TypeId::ANY,
            ..Default::default()
        },
    ]);
    // string vs structural Function -> should match
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::STRING,
            function_shape
        )
    );
    // Non-Function object -> should not match
    let non_fn = interner.object(vec![crate::types::PropertyInfo {
        name: apply,
        type_id: TypeId::ANY,
        ..Default::default()
    }]);
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
            &interner,
            TypeId::STRING,
            non_fn
        )
    );
}

/// `Lazy(DefId)` is a reference to a concrete named type (interface, class, type alias).
/// It must NOT be treated as a generic ref -- it is always resolvable and not an
/// unresolved type parameter.
#[test]
fn test_is_generic_ref_lazy_is_not_generic() {
    let interner = TypeInterner::new();
    let lazy_a = interner.lazy(crate::def::DefId(100));
    let lazy_b = interner.lazy(crate::def::DefId(200));
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
            &interner, lazy_a
        ),
        "Lazy(DefId) should not be a generic ref"
    );
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
            &interner, lazy_b
        ),
        "Lazy(DefId) with different DefId should not be a generic ref"
    );
}

/// `TypeParameter` is a genuine unknown and must still trigger deferral.
/// Tests two different parameter names to prove name-independence.
#[test]
fn test_is_generic_ref_type_parameter_is_generic() {
    let interner = TypeInterner::new();
    let atom_t = interner.intern_string("T");
    let atom_k = interner.intern_string("K");
    let make_tp = |name| {
        interner.type_param(crate::types::TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        })
    };
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
            &interner,
            make_tp(atom_t)
        ),
        "TypeParameter T should be a generic ref"
    );
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
            &interner,
            make_tp(atom_k)
        ),
        "TypeParameter K (renamed) should be a generic ref"
    );
}

/// `IndexAccess(Lazy(DefId), string)` -- property access on a named interface -- must NOT
/// trigger deferral. This was the root cause of issue #6256 where
/// `Interface["prop"] extends Record<string, any>` was incorrectly deferred.
#[test]
fn test_is_generic_ref_index_access_lazy_is_not_generic() {
    let interner = TypeInterner::new();
    let lazy = interner.lazy(crate::def::DefId(42));
    let idx_access = interner.index_access(lazy, TypeId::STRING);
    assert!(
        !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
            &interner, idx_access
        ),
        "IndexAccess(Lazy(DefId), string) should not be a generic ref"
    );
}

/// `IndexAccess(TypeParam, K)` must remain a generic ref -- `T[K]` is indeterminate
/// until T and K are substituted.
#[test]
fn test_is_generic_ref_index_access_type_param_remains_generic() {
    let interner = TypeInterner::new();
    let atom_m = interner.intern_string("M");
    let tp_m = interner.type_param(crate::types::TypeParamInfo {
        name: atom_m,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let idx_access = interner.index_access(tp_m, TypeId::STRING);
    assert!(
        TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
            &interner, idx_access
        ),
        "IndexAccess(TypeParam, string) should be a generic ref"
    );
}

/// Intrinsic `TypeId`s (like `TypeId::STRING`) are never generic regardless of
/// what internal data they might map to.
#[test]
fn test_is_generic_ref_intrinsics_are_never_generic() {
    let interner = TypeInterner::new();
    for id in [
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
    ] {
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_generic_ref(
                &interner, id
            ),
            "intrinsic {id:?} should not be a generic ref"
        );
    }
}

/// An `Application` whose base alias the active resolver cannot expand survives
/// evaluation as an opaque `Application`. When such an opaque application sits
/// in a conditional's CHECK position, the structural relation has no structure
/// to compare and degrades the application toward the bottom type, so the
/// subtype check is vacuously satisfied and the conditional would take its TRUE
/// branch. That is the mechanism by which the key filter
/// `IsOptionalKeyOf<O, K> extends false ? never : K` collapses every key to
/// `never` and corrupts `RequiredKeysOf`/`OptionalKeysOf` (#13609). The
/// evaluator must instead DEFER the conditional (keep it a `Conditional`) so a
/// later resolver pass that can expand the application decides the branch.
///
/// `extends unknown` is used only because every type — including the
/// degraded-to-bottom opaque application — is a subtype of `unknown`, which
/// deterministically forces the vacuous-true path the guard intercepts without
/// depending on how a particular resolver treats an unresolvable base. The
/// real-world witness is the `extends false` key filter above. The alias
/// `DefId` and the branch types carry no special spelling (name-agnostic).
#[test]
fn opaque_application_check_type_defers_instead_of_taking_true_branch() {
    let interner = TypeInterner::new();
    // Opaque check type: `Unresolvable<string>` whose base `Lazy(DefId)` has no
    // body under the default `NoopResolver`.
    let unresolvable_base = interner.lazy(crate::def::DefId(4242));
    let opaque_app = interner.application(unresolvable_base, vec![TypeId::STRING]);
    // `<opaque> extends unknown ? never : string`.
    let cond = interner.conditional(crate::types::ConditionalType {
        check_type: opaque_app,
        extends_type: TypeId::UNKNOWN,
        true_type: TypeId::NEVER,
        false_type: TypeId::STRING,
        is_distributive: false,
    });

    let mut evaluator = TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
    let result = evaluator.evaluate(cond);

    // A deferred `Conditional` (rather than the `never` true branch or the
    // `string` false branch) proves the evaluator did not vacuously collapse the
    // opaque check type into a branch.
    assert!(
        matches!(
            interner.lookup(result),
            Some(crate::types::TypeData::Conditional(_))
        ),
        "opaque-application check type must keep the conditional deferred, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn incomplete_request_verdict_blocks_conditional_branch_persistent_write() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    let mut complete = TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&cache);
    assert_eq!(
        complete.conditional_subtype_relation(TypeId::STRING, TypeId::UNKNOWN),
        BranchRelation::Holds
    );
    assert_eq!(
        cache.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::UNKNOWN, false, false),
        Some(true)
    );

    let mut incomplete = TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&cache);
    incomplete.simulate_incomplete_request_verdict_for_test(TerminationKind::QueryOpBudget);
    assert_eq!(
        incomplete.conditional_subtype_relation(TypeId::NUMBER, TypeId::UNKNOWN),
        BranchRelation::Holds
    );
    assert_eq!(
        cache.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::UNKNOWN, false, false),
        None
    );
}

#[test]
fn conditional_subtype_relation_tracks_exact_optional_property_types() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);
    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo::new(name, TypeId::UNDEFINED)]);
    let target = interner.object(vec![PropertyInfo::opt(name, TypeId::NUMBER)]);

    let mut exact_off = TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&cache);
    exact_off.set_exact_optional_property_types(false);
    assert_eq!(
        exact_off.conditional_subtype_relation(source, target),
        BranchRelation::Holds
    );
    assert_eq!(
        cache.lookup_conditional_branch_verdict(source, target, false, false),
        Some(true)
    );

    let mut exact_on = TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&cache);
    exact_on.set_exact_optional_property_types(true);
    assert_eq!(
        exact_on.conditional_subtype_relation(source, target),
        BranchRelation::Fails
    );
    assert_eq!(
        cache.lookup_conditional_branch_verdict(source, target, false, true),
        Some(false)
    );

    let mut exact_off_again = TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&cache);
    exact_off_again.set_exact_optional_property_types(false);
    assert_eq!(
        exact_off_again.conditional_subtype_relation(source, target),
        BranchRelation::Holds
    );
}
