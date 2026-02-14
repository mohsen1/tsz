use super::*;
use crate::TypeInterner;

#[test]
fn test_is_numeric_literal_name() {
    // Special values
    assert!(is_numeric_literal_name("NaN"));
    assert!(is_numeric_literal_name("Infinity"));
    assert!(is_numeric_literal_name("-Infinity"));

    // Regular numbers
    assert!(is_numeric_literal_name("0"));
    assert!(is_numeric_literal_name("1"));
    assert!(is_numeric_literal_name("42"));
    assert!(is_numeric_literal_name("-1"));
    assert!(is_numeric_literal_name("3.14"));

    // Non-numeric strings
    assert!(!is_numeric_literal_name("foo"));
    assert!(!is_numeric_literal_name(""));
    assert!(!is_numeric_literal_name("abc123"));
}

#[test]
fn test_type_id_ext_non_never() {
    // Test non_never
    assert_eq!(TypeId::UNKNOWN.non_never(), Some(TypeId::UNKNOWN));
    assert_eq!(TypeId::NEVER.non_never(), None);
}

#[test]
fn test_union_or_single() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    // Empty list -> NEVER
    let result = union_or_single(db, vec![]);
    assert_eq!(result, TypeId::NEVER);

    // Single element -> that element
    let result = union_or_single(db, vec![TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);

    // Multiple elements -> union
    let result = union_or_single(db, vec![TypeId::STRING, TypeId::NUMBER]);
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_intersection_or_single() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let result = intersection_or_single(db, vec![]);
    assert_eq!(result, TypeId::NEVER);

    let result = intersection_or_single(db, vec![TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);

    let result = intersection_or_single(db, vec![TypeId::STRING, TypeId::NUMBER]);
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}
