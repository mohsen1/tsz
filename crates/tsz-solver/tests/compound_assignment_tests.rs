use crate::{
    TypeDatabase, TypeId, TypeInterner, fallback_compound_assignment_result,
    is_compound_assignment_operator, map_compound_assignment_to_binary,
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
