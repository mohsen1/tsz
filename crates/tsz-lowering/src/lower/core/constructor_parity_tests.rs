use super::*;
use tsz_solver::construction::TypeInterner;

fn assert_invariant_defaults(lowering: &TypeLowering<'_>) {
    // Caches/shared state must always start fresh.
    assert!(lowering.type_param_scopes.borrow().is_empty());
    assert_eq!(*lowering.operations.borrow(), 0);
    assert!(!*lowering.limit_exceeded.borrow());
    // Optional knobs default to disabled.
    assert!(lowering.computed_name_resolver.is_none());
    assert!(lowering.lazy_type_params_resolver.is_none());
    assert!(!lowering.prefer_name_def_id_resolution);
    assert!(lowering.preferred_self_name.is_none());
    assert!(lowering.preferred_self_def_id.is_none());
    assert!(lowering.name_def_id_resolver.is_none());
    assert!(!lowering.strict_null_checks);
    assert!(lowering.type_query_override.is_none());
}

#[test]
fn new_initializes_invariant_defaults() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);
    assert_invariant_defaults(&lowering);
    assert!(lowering.type_resolver.is_none());
    assert!(lowering.def_id_resolver.is_none());
    assert!(lowering.value_resolver.is_none());
}

#[test]
fn with_resolver_initializes_invariant_defaults() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let resolver = |_: NodeIndex| -> Option<u32> { None };
    let lowering = TypeLowering::with_resolver(&arena, &interner, &resolver);
    assert_invariant_defaults(&lowering);
    // `with_resolver` wires the same closure into both type and value slots.
    assert!(lowering.type_resolver.is_some());
    assert!(lowering.def_id_resolver.is_none());
    assert!(lowering.value_resolver.is_some());
}

#[test]
fn with_resolvers_initializes_invariant_defaults() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let type_resolver = |_: NodeIndex| -> Option<u32> { None };
    let value_resolver = |_: NodeIndex| -> Option<u32> { None };
    let lowering = TypeLowering::with_resolvers(&arena, &interner, &type_resolver, &value_resolver);
    assert_invariant_defaults(&lowering);
    assert!(lowering.type_resolver.is_some());
    assert!(lowering.def_id_resolver.is_none());
    assert!(lowering.value_resolver.is_some());
}

#[test]
fn with_def_id_resolver_initializes_invariant_defaults() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let def_id_resolver = |_: NodeIndex| -> Option<DefId> { None };
    let value_resolver = |_: NodeIndex| -> Option<u32> { None };
    let lowering =
        TypeLowering::with_def_id_resolver(&arena, &interner, &def_id_resolver, &value_resolver);
    assert_invariant_defaults(&lowering);
    assert!(lowering.type_resolver.is_none());
    assert!(lowering.def_id_resolver.is_some());
    assert!(lowering.value_resolver.is_some());
}

#[test]
fn with_hybrid_resolver_initializes_invariant_defaults() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let type_resolver = |_: NodeIndex| -> Option<u32> { None };
    let def_id_resolver = |_: NodeIndex| -> Option<DefId> { None };
    let value_resolver = |_: NodeIndex| -> Option<u32> { None };
    let lowering = TypeLowering::with_hybrid_resolver(
        &arena,
        &interner,
        &type_resolver,
        &def_id_resolver,
        &value_resolver,
    );
    assert_invariant_defaults(&lowering);
    assert!(lowering.type_resolver.is_some());
    assert!(lowering.def_id_resolver.is_some());
    assert!(lowering.value_resolver.is_some());
}

#[test]
fn type_param_scope_update_replaces_placeholder_binding() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);
    let name = interner.intern_string("T");

    lowering.push_type_param_scope();
    lowering.add_type_param_binding(name, TypeId::STRING);
    lowering.update_type_param_binding(name, TypeId::NUMBER);

    let scopes = lowering.type_param_scopes.borrow();
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].len(), 1);
    let (stored_name, binding) = scopes[0].iter().next().expect("binding");
    assert_eq!(stored_name.as_ref(), "T");
    assert_eq!(binding.type_id, TypeId::NUMBER);
    drop(scopes);

    assert_eq!(lowering.lookup_type_param("T"), Some(TypeId::NUMBER));
    assert_eq!(lowering.lookup_type_param("U"), None);
    lowering.pop_type_param_scope();
}
