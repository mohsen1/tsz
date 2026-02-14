use super::*;
use crate::intern::TypeInterner;
use crate::subtype::TypeEnvironment;

#[test]
fn test_canonicalizer_creation() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let _canonicalizer = Canonicalizer::new(&interner, &env);
}

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
