// Cross-arena `UnresolvedTypeName` references must be recovered during
// evaluation in every position, not only as an `Application` base (refs #14322).
//
// Structural rule: the lowering pass leaves a bare `UnresolvedTypeName(name)`
// for a type reference inside a (generic) declaration body it could not bind to
// a `DefId` — most commonly a name in scope only in the *declaring* file,
// reached through an imported generic alias body (`type Lookup<K> =
// Registry[K]`). When the active resolver can recover the name (the importing
// checker seeds `name -> DefId`, or the wider `CheckerContext` walks the merged
// binder graph), evaluation must resolve the reference so deferred operators
// over it — here the index-access object `Registry[K]` — reduce just as the
// same-module path does. Witness: io-ts emitted a false `TS2322`/`TS7006`
// because `Lookup<"a">` stayed an opaque deferred index access at the use site.

#[test]
fn index_access_object_unresolved_name_resolves_through_seeded_resolution() {
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // The declaring file's `Registry` interface body, registered under its def
    // and reachable by name through the importing checker's seeded map.
    let registry_def = DefId(41);
    let registry = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);
    env.insert_def(registry_def, registry);
    env.insert_unresolved_resolution("Registry".to_string(), registry_def);

    // `Registry["a"]` where `Registry` survived as an `UnresolvedTypeName`.
    let unresolved_registry = interner.unresolved_type_name(interner.intern_string("Registry"));
    let index_access = interner.index_access(unresolved_registry, interner.literal_string("a"));

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(index_access);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "Registry[\"a\"] over a recoverable UnresolvedTypeName object must reduce to the \
         member type, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn bare_unresolved_name_resolves_through_seeded_resolution() {
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Renamed binder so the recovery cannot depend on any identifier text.
    let catalog_def = DefId(57);
    let catalog = interner.object(vec![PropertyInfo::new(
        interner.intern_string("first"),
        TypeId::BOOLEAN,
    )]);
    env.insert_def(catalog_def, catalog);
    env.insert_unresolved_resolution("Catalog".to_string(), catalog_def);

    let unresolved_catalog = interner.unresolved_type_name(interner.intern_string("Catalog"));

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(unresolved_catalog);
    assert_eq!(
        result, catalog,
        "a bare recoverable UnresolvedTypeName must evaluate to its registered body, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn unresolved_name_without_resolution_stays_deferred() {
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    // No seeded resolution and no registered def: the name genuinely cannot be
    // bound on this pass, so it must stay the display-preserving opaque form
    // (a later pass with a wider resolver / registered body recovers it) rather
    // than collapsing.
    let env = TypeEnvironment::new();

    let unresolved = interner.unresolved_type_name(interner.intern_string("Missing"));
    let index_access = interner.index_access(unresolved, interner.literal_string("a"));

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);

    let bare = evaluator.evaluate(unresolved);
    assert!(
        matches!(
            interner.lookup(bare),
            Some(TypeData::UnresolvedTypeName(_))
        ),
        "an unrecoverable name must pass through unchanged, got {:?}",
        interner.lookup(bare)
    );

    let deferred = evaluator.evaluate(index_access);
    assert!(
        matches!(interner.lookup(deferred), Some(TypeData::IndexAccess(_, _))),
        "an index access over an unrecoverable name must stay deferred, got {:?}",
        interner.lookup(deferred)
    );
}
