use crate::{
    BinaryOpEvaluator, BinaryOpResult, QueryDatabase, TypeDatabase, TypeId, TypeInterner,
    fallback_compound_assignment_result, is_compound_assignment_operator,
    is_logical_compound_assignment_operator, map_compound_assignment_to_binary,
};
use tsz_scanner::SyntaxKind;

#[test]
fn recognizes_compound_assignment_tokens() {
    assert!(is_compound_assignment_operator(
        SyntaxKind::PlusEqualsToken as u16
    ));
    assert!(is_compound_assignment_operator(
        SyntaxKind::QuestionQuestionEqualsToken as u16
    ));
    assert!(!is_compound_assignment_operator(
        SyntaxKind::EqualsToken as u16
    ));
}

#[test]
fn recognizes_logical_compound_assignment_tokens() {
    assert!(is_logical_compound_assignment_operator(
        SyntaxKind::AmpersandAmpersandEqualsToken as u16
    ));
    assert!(is_logical_compound_assignment_operator(
        SyntaxKind::BarBarEqualsToken as u16
    ));
    assert!(is_logical_compound_assignment_operator(
        SyntaxKind::QuestionQuestionEqualsToken as u16
    ));
    // Non-logical compound forms are rejected.
    assert!(!is_logical_compound_assignment_operator(
        SyntaxKind::PlusEqualsToken as u16
    ));
    assert!(!is_logical_compound_assignment_operator(
        SyntaxKind::AmpersandEqualsToken as u16
    ));
    // Simple assignment is rejected.
    assert!(!is_logical_compound_assignment_operator(
        SyntaxKind::EqualsToken as u16
    ));
}

#[test]
fn maps_compound_assignment_to_binary_operator() {
    assert_eq!(
        map_compound_assignment_to_binary(SyntaxKind::AsteriskAsteriskEqualsToken as u16),
        Some("**")
    );
    assert_eq!(
        map_compound_assignment_to_binary(SyntaxKind::BarBarEqualsToken as u16),
        Some("||")
    );
    assert_eq!(
        map_compound_assignment_to_binary(SyntaxKind::EqualsToken as u16),
        None
    );
}

#[test]
fn fallback_result_keeps_plus_equals_unknown_unless_numeric_rhs() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    assert_eq!(
        fallback_compound_assignment_result(db, SyntaxKind::PlusEqualsToken as u16, None),
        None
    );
    assert_eq!(
        fallback_compound_assignment_result(
            db,
            SyntaxKind::PlusEqualsToken as u16,
            Some(interner.literal_number(1.0)),
        ),
        Some(TypeId::NUMBER)
    );
}

#[test]
fn fallback_result_numeric_operators_return_number() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    assert_eq!(
        fallback_compound_assignment_result(db, SyntaxKind::MinusEqualsToken as u16, None),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        fallback_compound_assignment_result(db, SyntaxKind::BarBarEqualsToken as u16, None),
        None
    );
}

// Tests for BinaryOpEvaluator::evaluate_plus used by += operator checking

#[test]
fn plus_boolean_and_void_is_type_error() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert!(matches!(
        evaluator.evaluate(TypeId::BOOLEAN, TypeId::VOID, "+"),
        BinaryOpResult::TypeError { .. }
    ));
}

#[test]
fn plus_boolean_and_boolean_is_type_error() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert!(matches!(
        evaluator.evaluate(TypeId::BOOLEAN, TypeId::BOOLEAN, "+"),
        BinaryOpResult::TypeError { .. }
    ));
}

#[test]
fn plus_boolean_and_number_is_type_error() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert!(matches!(
        evaluator.evaluate(TypeId::BOOLEAN, TypeId::NUMBER, "+"),
        BinaryOpResult::TypeError { .. }
    ));
}

#[test]
fn plus_number_and_number_succeeds() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert_eq!(
        evaluator.evaluate(TypeId::NUMBER, TypeId::NUMBER, "+"),
        BinaryOpResult::Success(TypeId::NUMBER)
    );
}

#[test]
fn plus_string_and_number_succeeds_as_string() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert_eq!(
        evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "+"),
        BinaryOpResult::Success(TypeId::STRING)
    );
}

#[test]
fn plus_any_and_null_succeeds_as_any() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert_eq!(
        evaluator.evaluate(TypeId::ANY, TypeId::NULL, "+"),
        BinaryOpResult::Success(TypeId::ANY)
    );
}

#[test]
fn null_and_undefined_are_not_arithmetic_operands() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;
    let evaluator = BinaryOpEvaluator::new(db);
    assert!(!evaluator.is_arithmetic_operand(TypeId::NULL));
    assert!(!evaluator.is_arithmetic_operand(TypeId::UNDEFINED));
    assert!(evaluator.is_arithmetic_operand(TypeId::NUMBER));
    assert!(evaluator.is_arithmetic_operand(TypeId::ANY));
}
