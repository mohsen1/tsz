//! Unit tests for `resolve_call`'s `Application` arm cross-file alias-of-callable
//! recovery (refs #13947).

use super::call_evaluator::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::TypeInterner;
use crate::def::DefId;
use crate::types::{FunctionShape, TypeId};

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
