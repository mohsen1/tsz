use super::*;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::def::resolver::TypeEnvironment;
use crate::types::{PropertyInfo, TypeId};

fn atom_names(interner: &TypeInterner, atoms: &[tsz_common::interner::Atom]) -> Vec<String> {
    let mut names: Vec<String> = atoms
        .iter()
        .map(|atom| interner.resolve_atom(*atom))
        .collect();
    names.sort();
    names
}

fn lazy_chain_to(
    interner: &TypeInterner,
    env: &mut TypeEnvironment,
    first_def: u32,
    len: u32,
    leaf: TypeId,
) -> TypeId {
    debug_assert!(len > 0);
    for offset in 0..len {
        let def_id = DefId(first_def + offset);
        let body = if offset + 1 == len {
            leaf
        } else {
            interner.lazy(DefId(first_def + offset + 1))
        };
        env.insert_def(def_id, body);
    }
    interner.lazy(DefId(first_def))
}

#[test]
fn mapped_keyof_keys_depth_state_continues_at_exact_limit() {
    let interner = TypeInterner::new();
    let prop = interner.intern_string("value");
    let leaf = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    let mut env = TypeEnvironment::new();
    let operand = lazy_chain_to(&interner, &mut env, 100, KEYOF_KEYS_MAX_DEPTH, leaf);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let keys = checker
        .try_get_keyof_keys(operand)
        .expect("exact depth cap should still inspect the leaf object");

    assert_eq!(atom_names(&interner, &keys), vec!["value"]);
}

#[test]
fn mapped_keyof_keys_depth_state_preserves_unexpandable_past_limit() {
    let interner = TypeInterner::new();
    let prop = interner.intern_string("value");
    let leaf = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    let mut env = TypeEnvironment::new();
    let operand = lazy_chain_to(&interner, &mut env, 200, KEYOF_KEYS_MAX_DEPTH + 1, leaf);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(
        checker.try_get_keyof_keys(operand).is_none(),
        "past the cap, the existing fallback keeps mapped expansion unresolvable"
    );
}
