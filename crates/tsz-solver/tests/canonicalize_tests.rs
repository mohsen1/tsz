use super::*;
use crate::TypeDatabase;
use crate::def::{DefId, DefKind};
use crate::intern::TypeInterner;
use crate::relations::subtype::{TypeEnvironment, TypeResolver};
use crate::types::{SymbolRef, TypeData};

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

struct ExpandingAliasResolver;

impl TypeResolver for ExpandingAliasResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        Some(interner.lazy(DefId(def_id.0 + 1)))
    }

    fn get_def_kind(&self, _def_id: DefId) -> Option<DefKind> {
        Some(DefKind::TypeAlias)
    }
}

#[test]
fn test_canonicalize_expanding_alias_chain_terminates() {
    let interner = TypeInterner::new();
    let resolver = ExpandingAliasResolver;
    let mut canon = Canonicalizer::new(&interner, &resolver);

    let start = interner.lazy(DefId(1));
    let result = canon.canonicalize(start);

    assert!(
        matches!(interner.lookup(result), Some(TypeData::Lazy(_))),
        "canonicalization should terminate with a lazy fallback for expanding aliases"
    );
}
