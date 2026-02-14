use super::*;
use crate::TypeInterner;
use crate::subtype::NoopResolver;

#[test]
fn test_non_application_passthrough() {
    let interner = TypeInterner::new();
    let string_type = interner.intern(TypeKey::Intrinsic(IntrinsicKind::String));

    let evaluator = ApplicationEvaluator::new(&interner, &NoopResolver);
    let result = evaluator.evaluate(string_type);

    assert!(matches!(result, ApplicationResult::NotApplication(_)));
}

#[test]
fn test_primitives_are_not_applications() {
    let interner = TypeInterner::new();
    let evaluator = ApplicationEvaluator::new(&interner, &NoopResolver);

    // Primitives should pass through as NotApplication
    assert!(matches!(
        evaluator.evaluate(TypeId::ANY),
        ApplicationResult::NotApplication(_)
    ));
    assert!(matches!(
        evaluator.evaluate(TypeId::NEVER),
        ApplicationResult::NotApplication(_)
    ));
    assert!(matches!(
        evaluator.evaluate(TypeId::STRING),
        ApplicationResult::NotApplication(_)
    ));
}
