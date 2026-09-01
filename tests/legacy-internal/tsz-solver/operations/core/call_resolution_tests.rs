//! Unit tests for `resolve_call`'s `Application` arm cross-file alias-of-callable
//! recovery (refs #13947).

use super::call_evaluator::{
    AssignabilityChecker, CallEvaluator, CallResult, contextual_signature_test_probe,
};
use crate::TypeInterner;
use crate::def::DefId;
use crate::types::{FunctionShape, ParamInfo, TypeId};

/// Mock checker modelling the resolver-less cross-file case: `evaluate_type`
/// cannot reduce the application (the trait default returns it unchanged), but
/// `expand_type_alias_application` recovers the open body for the tracked
/// application `TypeId` — exactly what the real checker's DefId-keyed expansion
/// does for an imported `type Create<R> = Sig<R>` whose `Sig` base carries no
/// `SymbolId` in the calling file's context.
struct AliasExpandChecker {
    app: TypeId,
    expanded: Option<TypeId>,
}

impl AssignabilityChecker for AliasExpandChecker {
    fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
        true
    }

    fn expand_type_alias_application(&mut self, type_id: TypeId) -> Option<TypeId> {
        if type_id == self.app {
            self.expanded
        } else {
            None
        }
    }
}

/// A generic application `Base<string>` whose base is a bare `Lazy(DefId)` that
/// the (default) evaluator cannot resolve — the resolver-less call-target shape.
fn resolver_less_application(interner: &TypeInterner) -> TypeId {
    let base = interner.lazy(DefId(1));
    interner.application(base, vec![TypeId::STRING])
}

#[test]
fn application_call_target_falls_back_to_alias_expansion() {
    let interner = TypeInterner::new();
    let app = resolver_less_application(&interner);
    // The alias body is a callable `() => number`.
    let callable = interner.function(FunctionShape::new(vec![], TypeId::NUMBER));

    let mut checker = AliasExpandChecker {
        app,
        expanded: Some(callable),
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let result = evaluator.resolve_call(app, &[]);

    assert!(
        matches!(result, CallResult::Success(ret) if ret == TypeId::NUMBER),
        "a resolver-less alias-of-callable application used as a call target must \
         resolve through expand_type_alias_application rather than collapsing to \
         NotCallable, got {result:?}"
    );
}

#[test]
fn application_call_target_without_expansion_stays_not_callable() {
    let interner = TypeInterner::new();
    let app = resolver_less_application(&interner);

    // The checker cannot expand the application (genuinely opaque / non-callable):
    // the fallback must not invent callability.
    let mut checker = AliasExpandChecker {
        app,
        expanded: None,
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let result = evaluator.resolve_call(app, &[]);

    assert!(
        matches!(result, CallResult::NotCallable { .. }),
        "an application the checker cannot expand stays NotCallable, got {result:?}"
    );
}

#[test]
fn application_call_target_prefers_evaluation_when_available() {
    let interner = TypeInterner::new();
    let app = resolver_less_application(&interner);
    // If expansion yields the same type (no progress), the call stays NotCallable
    // rather than recursing forever.
    let mut checker = AliasExpandChecker {
        app,
        expanded: Some(app),
    };
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    let result = evaluator.resolve_call(app, &[]);

    assert!(
        matches!(result, CallResult::NotCallable { .. }),
        "a non-progressing expansion must not loop or fabricate callability, got {result:?}"
    );
}

#[test]
fn contextual_signature_memoizes_shared_union_intersection_constituents_per_walk() {
    let interner = TypeInterner::new();
    let param = interner.intern_string("value");
    let callable = |return_type| {
        interner.function(FunctionShape::new(
            vec![ParamInfo::required(param, TypeId::STRING)],
            return_type,
        ))
    };
    let shared = callable(TypeId::STRING);
    let left_only = callable(TypeId::NUMBER);
    let right_only = callable(TypeId::BOOLEAN);

    let left = interner.intersect_types_raw2(shared, left_only);
    let right = interner.intersect_types_raw2(shared, right_only);
    let contextual = interner.union_literal_reduce(vec![left, right]);

    let (signature, visits) = contextual_signature_test_probe::with_recorded_visits(|| {
        CallEvaluator::<AliasExpandChecker>::get_contextual_signature_cached(&interner, contextual)
    });

    assert!(
        signature.is_some(),
        "shared callable constituents in union/intersection contextual types \
         should still produce a contextual signature"
    );
    assert_eq!(
        visits.iter().filter(|&&visited| visited == shared).count(),
        1,
        "a shared callable constituent should be walked once per contextual \
         signature extraction, not once per DAG path"
    );
}

fn contextual_signature_in_flight_cycle() -> (TypeInterner, TypeId) {
    for raw_app in TypeId::FIRST_USER..TypeId::FIRST_USER + 512 {
        let interner = TypeInterner::new();
        let future_app = TypeId(raw_app);

        let wrapper = interner.no_infer(future_app);
        let base = interner.lazy(DefId(wrapper.0));
        let app = interner.application(base, vec![TypeId::STRING]);

        if app == future_app {
            assert_eq!(
                crate::evaluation::evaluate::evaluate_type(&interner, wrapper),
                app
            );
            return (interner, app);
        }
    }

    panic!("could not construct a stable contextual-signature cycle");
}

#[test]
fn contextual_signature_cycle_truncation_none_is_not_memoized() {
    let (interner, cyclic_app) = contextual_signature_in_flight_cycle();
    let param = interner.intern_string("value");
    let callable = |return_type| {
        interner.function(FunctionShape::new(
            vec![ParamInfo::required(param, TypeId::STRING)],
            return_type,
        ))
    };

    let left = interner.intersect_types_raw2(cyclic_app, callable(TypeId::NUMBER));
    let right = interner.intersect_types_raw2(cyclic_app, callable(TypeId::BOOLEAN));
    let contextual = interner.union_preserve_members(vec![left, right]);

    let (signature, visits) = contextual_signature_test_probe::with_recorded_visits(|| {
        CallEvaluator::<AliasExpandChecker>::get_contextual_signature_cached(&interner, contextual)
    });

    assert!(
        signature.is_some(),
        "a cycle-truncated application must not hide callable constituents"
    );
    assert_eq!(
        visits
            .iter()
            .filter(|&&visited| visited == cyclic_app)
            .count(),
        2,
        "cycle-truncated None for an in-flight contextual application must not \
         be memoized"
    );
}
