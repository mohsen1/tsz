use super::*;
use crate::def::DefinitionInfo;
use crate::types::IntrinsicKind;
use std::sync::Arc;

/// Regression test: `is_boxed_type_id` must not match a TypeId that is
/// registered as the direct boxed type for a DIFFERENT kind.
///
/// Previously, a String DefId's resolved type in `def_types` could match
/// Function's Object TypeId, causing `is_boxed_type_id(Function_TypeId`, String)
/// to return true. This made `string` incorrectly assignable to `Function`.
#[test]
fn test_is_boxed_type_id_cross_kind_guard() {
    let mut env = TypeEnvironment::new();

    let string_type = TypeId(100);
    let function_type = TypeId(200);
    let string_def = DefId(50);

    // Register String boxed type (direct)
    env.set_boxed_type(IntrinsicKind::String, string_type);
    // Register Function boxed type (direct)
    env.set_boxed_type(IntrinsicKind::Function, function_type);
    // Register String DefId
    env.register_boxed_def_id(IntrinsicKind::String, string_def);
    // Simulate the bug: String DefId resolves to Function's TypeId
    env.insert_def(string_def, function_type);

    // Without the guard, this would return true (String DefId resolves to function_type)
    // With the guard, it should return false (function_type is registered for Function kind)
    assert!(
        !env.is_boxed_type_id(function_type, IntrinsicKind::String),
        "Function TypeId should NOT be identified as String's boxed type"
    );

    // Function TypeId should still match its own kind
    assert!(
        env.is_boxed_type_id(function_type, IntrinsicKind::Function),
        "Function TypeId should match Function kind"
    );

    // String TypeId should match its own kind
    assert!(
        env.is_boxed_type_id(string_type, IntrinsicKind::String),
        "String TypeId should match String kind"
    );
}

#[test]
fn resolve_lazy_raw_symbol_fallback_redirects_to_real_def() {
    let mut env = TypeEnvironment::new();
    let interner = crate::construction::TypeInterner::new();

    let raw_symbol = SymbolRef(7);
    let real_def = DefId(42);
    let resolved_type = TypeId(99);

    env.symbol_to_def.insert(raw_symbol.0, real_def);
    env.insert_def(real_def, resolved_type);

    assert_eq!(
        env.resolve_lazy(DefId(raw_symbol.0), &interner),
        Some(resolved_type)
    );
}

#[test]
fn canonical_decl_site_symbol_lookup_uses_shared_store_identity() {
    let interner = crate::construction::TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());
    let name = interner.intern_string("Registry");

    let mut home = DefinitionInfo::interface(name, Vec::new(), Vec::new());
    home.file_id = Some(7);
    home.span = Some((42, 42));
    home.symbol_id = Some(100);
    let home_def = store.register(home);

    let mut consuming = DefinitionInfo::interface(name, Vec::new(), Vec::new());
    consuming.file_id = Some(7);
    consuming.span = Some((42, 42));
    consuming.symbol_id = Some(200);
    let consuming_def = store.register(consuming);

    let mut env = TypeEnvironment::new();
    env.set_definition_store(Arc::clone(&store));

    assert_eq!(env.symbol_to_def_id(SymbolRef(200)), Some(consuming_def));
    assert_eq!(
        env.canonical_decl_site_def_for_symbol(SymbolRef(200)),
        Some(home_def)
    );
}

/// Regression (#13862): `resolve_lazy` must never reinterpret a *registered*
/// `DefId`'s numeric value as a `SymbolId`.
///
/// `Lazy(DefId(N))` is created both for genuine registered definitions and
/// for zombie references (`interner.reference(SymbolRef(N))`, where `N` is a
/// raw `SymbolId`). The raw-symbol fallback is only sound for the zombie
/// case. For a registered `DefId` whose body is not yet materialized,
/// resolving it through `find_def_by_symbol(def_id.0)` collides with the
/// unrelated definition that merely shares that raw numeric id — exactly the
/// cross-lib-binder defect where `HTMLElementTagNameMap["div"]` resolved
/// `HTMLDivElement` (= `DefId(218)`) to the def whose *symbol* id is `218`
/// (`FileSystemEntry`). Lib symbols all carry the `u32::MAX` declaration-file
/// sentinel, so the file-agnostic symbol→def index is first-writer-wins
/// across lib binders, making the collision routine.
#[test]
fn resolve_lazy_does_not_symbol_conflate_a_registered_def() {
    let interner = crate::construction::TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());

    // The "real" def (HTMLDivElement-like): registered, body NOT materialized.
    let real = store.register(DefinitionInfo::interface(
        interner.intern_string("ElementLike"),
        vec![],
        vec![],
    ));

    // A collider whose *SymbolId* equals `real`'s `DefId` numeric value and
    // which carries a concrete body, so `find_def_by_symbol(real.0)` resolves
    // to it.
    let mut collider_info =
        DefinitionInfo::type_alias(interner.intern_string("Collider"), vec![], TypeId::STRING);
    collider_info.symbol_id = Some(real.0);
    let collider = store.register(collider_info);
    store.set_body(collider, TypeId::STRING);
    assert_eq!(store.find_def_by_symbol(real.0), Some(collider));

    let mut env = TypeEnvironment::new();
    env.set_definition_store(Arc::clone(&store));

    // `real`'s body is unmaterialized. The pre-fix code returned the
    // collider's body via symbol conflation; the fix defers instead.
    assert_eq!(env.resolve_lazy(real, &interner), None);
}

/// #14344 identity-collision observability: the wrong-decl counter fires on
/// a GENUINE content collision (the `#13862`-suppressed
/// `HTMLDivElement(218)` -> `FileSystemEntry(symbol 218)` class) and stays
/// silent for a store-registered def with no different-named collider. This
/// is measurement only — `resolve_lazy` returns `None` either way (behavior
/// unchanged). The counter is the migration's md5-stability regression
/// signal.
#[test]
fn identity_collision_counter_fires_on_genuine_content_collision_only() {
    use tsz_common::perf_counters::{counters, force_enable_perf_counters_for_tests};

    // Force the gate on so we can observe `fetch_add` deltas regardless of
    // env / `OnceLock` state (the recorder short-circuits when disabled).
    force_enable_perf_counters_for_tests();

    let read = || {
        counters()
            .identity_collision_wrong_decl_suppressed
            .load(std::sync::atomic::Ordering::Relaxed)
    };

    let interner = crate::construction::TypeInterner::new();

    // --- Case 1: genuine content collision (different-named decls). ---
    let store = Arc::new(DefinitionStore::new());
    let real = store.register(DefinitionInfo::interface(
        interner.intern_string("ElementLike"),
        vec![],
        vec![],
    ));
    let mut collider_info =
        DefinitionInfo::type_alias(interner.intern_string("Collider"), vec![], TypeId::STRING);
    collider_info.symbol_id = Some(real.0);
    let collider = store.register(collider_info);
    store.set_body(collider, TypeId::STRING);
    // Precondition: the raw-`u32` reinterpretation lands on the differently
    // named collider, i.e. the content actually differs.
    assert_eq!(store.find_def_by_symbol(real.0), Some(collider));

    let mut env = TypeEnvironment::new();
    env.set_definition_store(Arc::clone(&store));

    let before = read();
    // Behavior is unchanged: the `#13862` guard still defers.
    assert_eq!(env.resolve_lazy(real, &interner), None);
    assert_eq!(
        read() - before,
        1,
        "a genuine different-named raw-u32 collision must be counted exactly once"
    );

    // --- Case 2: store-registered def with NO collider at its raw id. ---
    // A fresh store whose only registered def has no symbol sharing its
    // `DefId` numeric value: the fallback still defers, but there is no
    // content collision, so the counter must not move.
    let store2 = Arc::new(DefinitionStore::new());
    let lonely = store2.register(DefinitionInfo::interface(
        interner.intern_string("Lonely"),
        vec![],
        vec![],
    ));
    // No def carries `symbol_id == lonely.0`, so the raw reinterpretation
    // finds nothing to collide with.
    assert_eq!(store2.find_def_by_symbol(lonely.0), None);
    let mut env2 = TypeEnvironment::new();
    env2.set_definition_store(Arc::clone(&store2));

    let before2 = read();
    assert_eq!(env2.resolve_lazy(lonely, &interner), None);
    assert_eq!(
        read() - before2,
        0,
        "raw-u32 overlap with nothing (no different-named decl) must not be counted"
    );
}

/// The symbol-conflation fallback stays valid for *zombie* `DefId`s — those
/// minted by `interner.reference(SymbolRef(N))` where `DefId(N)` is itself
/// unregistered.
#[test]
fn resolve_lazy_store_zombie_fallback_still_redirects() {
    let interner = crate::construction::TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());

    let mut info =
        DefinitionInfo::type_alias(interner.intern_string("Real"), vec![], TypeId::NUMBER);
    info.symbol_id = Some(9000); // raw SymbolId 9000
    let real = store.register(info);
    store.set_body(real, TypeId::NUMBER);

    let mut env = TypeEnvironment::new();
    env.set_definition_store(Arc::clone(&store));

    // DefId(9000) is unregistered, so the fallback legitimately reads 9000 as
    // a SymbolId and redirects to `real`'s body.
    assert!(store.get(DefId(9000)).is_none());
    assert_eq!(
        env.resolve_lazy(DefId(9000), &interner),
        Some(TypeId::NUMBER)
    );
}

#[test]
fn resolve_lazy_raw_symbol_fallback_preserves_class_instance_type() {
    let mut env = TypeEnvironment::new();
    let interner = crate::construction::TypeInterner::new();

    let raw_symbol = SymbolRef(7);
    let real_def = DefId(42);
    let constructor_type = TypeId(99);
    let instance_type = TypeId(123);

    env.symbol_to_def.insert(raw_symbol.0, real_def);
    env.insert_def(real_def, constructor_type);
    env.class_instance_types.insert(real_def.0, instance_type);

    assert_eq!(
        env.resolve_lazy(DefId(raw_symbol.0), &interner),
        Some(instance_type)
    );
}

#[test]
fn test_type_environment_generation_tracks_shared_store_mutations() {
    let store = Arc::new(DefinitionStore::new());
    let mut env = TypeEnvironment::new();

    let initial_generation = env.resolver_generation();
    env.set_definition_store(Arc::clone(&store));
    assert!(env.resolver_generation() > initial_generation);

    let before_store_write = env.resolver_generation();
    store.set_body(DefId(1), TypeId::STRING);
    assert!(env.resolver_generation() > before_store_write);

    let before_local_write = env.resolver_generation();
    env.insert_def(DefId(2), TypeId::NUMBER);
    assert!(env.resolver_generation() > before_local_write);
}

/// Pins the idempotency invariant introduced in #8269:
/// repeated `set_definition_store` calls with the same `Arc` pointer must
/// not bump the environment generation, while a subsequent store mutation
/// must still be visible.
#[test]
fn set_definition_store_same_arc_is_generation_idempotent() {
    let store = Arc::new(DefinitionStore::new());
    let mut env = TypeEnvironment::new();

    // First install bumps generation.
    let gen_before_first = env.resolver_generation();
    env.set_definition_store(Arc::clone(&store));
    let gen_after_first = env.resolver_generation();
    assert!(
        gen_after_first > gen_before_first,
        "first set_definition_store must bump generation"
    );

    // Second install with the same Arc pointer must NOT bump generation.
    env.set_definition_store(Arc::clone(&store));
    assert_eq!(
        env.resolver_generation(),
        gen_after_first,
        "repeated set_definition_store with the same Arc must not bump generation"
    );

    // A subsequent store mutation is still visible through the generation sum.
    let gen_before_mutation = env.resolver_generation();
    store.set_body(DefId(99), TypeId::STRING);
    assert!(
        env.resolver_generation() > gen_before_mutation,
        "store mutation must still be visible after idempotent reinstall"
    );
}

#[test]
fn boxed_def_id_registration_is_idempotent() {
    let mut env = TypeEnvironment::new();
    let def_id = DefId(7);

    env.register_boxed_def_id(IntrinsicKind::Function, def_id);
    let gen_after_first = env.resolver_generation();
    env.register_boxed_def_id(IntrinsicKind::Function, def_id);

    assert_eq!(
        env.resolver_generation(),
        gen_after_first,
        "re-registering the same boxed DefId must not invalidate env caches"
    );
    assert_eq!(
        env.snapshot_boxed_def_ids()
            .get(&IntrinsicKind::Function)
            .map(Vec::as_slice),
        Some(&[def_id][..])
    );
}

/// Issue #8720 infrastructure: `insert_class_instance_type` must write
/// through to the shared `DefinitionStore::class_to_instance` slot when a
/// store is attached, so any checker can observe the producer's instance
/// type cross-file. This is the foundation for a follow-up that wires
/// type-position class lookups through a boundary helper.
///
/// The full `resolve_lazy` consumer path is intentionally out of scope
/// for this PR — generic `resolve_lazy(class_def_id)` is read by both
/// type-position and value-position callers, so the shared slot must be
/// consulted only behind a position-aware boundary, not in the generic
/// resolver.
#[test]
fn insert_class_instance_type_writes_through_to_shared_store() {
    let store = Arc::new(DefinitionStore::new());
    let class_def = DefId(8);
    let instance_type = TypeId(64);

    let mut env = TypeEnvironment::new();
    env.set_definition_store(Arc::clone(&store));
    env.insert_class_instance_type(class_def, instance_type);

    assert_eq!(
        store.get_class_instance_type(class_def),
        Some(instance_type)
    );
}

/// `insert_class_instance_type` on a store-less environment must still
/// populate the local cache (used during checker setup before the store
/// is wired). The local cache is consulted by `resolve_lazy` today; the
/// shared cache write-through is purely additive.
#[test]
fn insert_class_instance_type_without_store_populates_local_only() {
    let class_def = DefId(8);
    let instance_type = TypeId(64);
    let interner = crate::construction::TypeInterner::new();

    let mut env = TypeEnvironment::new();
    env.insert_class_instance_type(class_def, instance_type);

    // Local cache is populated.
    assert_eq!(
        env.class_instance_types.get(&class_def.0),
        Some(&instance_type)
    );
    // `resolve_lazy` finds it through the local cache.
    assert_eq!(env.resolve_lazy(class_def, &interner), Some(instance_type));
}

/// The shared slot has independent producer/consumer semantics — a
/// consumer that never received an `insert_class_instance_type` call
/// (cross-file scenario) can still read the producer's published value
/// via `DefinitionStore::get_class_instance_type`. This pins the
/// infrastructure invariant the future boundary helper will rely on.
#[test]
fn shared_class_to_instance_visible_to_independent_environments() {
    let store = Arc::new(DefinitionStore::new());
    let class_def = DefId(101);
    let instance_type = TypeId(202);

    // Producer environment publishes via the write-through path.
    let mut producer_env = TypeEnvironment::new();
    producer_env.set_definition_store(Arc::clone(&store));
    producer_env.insert_class_instance_type(class_def, instance_type);

    // A separate consumer environment with no local entry can still
    // read the producer's instance type through the shared store.
    let consumer_env = TypeEnvironment::new();
    assert_eq!(
        consumer_env.class_instance_types.get(&class_def.0),
        None,
        "consumer's local cache must remain cold"
    );
    assert_eq!(
        store.get_class_instance_type(class_def),
        Some(instance_type),
        "shared slot must be visible to any checker holding the store"
    );
}

/// Pins the cross-module enum-parent residency fix: a producing env's
/// `register_enum_parent` write-through publishes the member->parent edge to
/// the shared `DefinitionStore`, so a *fresh* consumer env (modeling the
/// per-file `TypeEnvironment::new()` session reset) recovers the parent via
/// the store fallback even though its local `enum_parents` map is empty.
///
/// Without the fallback, cross-file enum discriminant narrowing reads the
/// consuming file's reset env, gets `None`, and collapses the receiver to
/// `never` (false TS2339 in mobx's `IDerivationState_` cascade).
#[test]
fn enum_parent_survives_file_reset_via_shared_store() {
    let store = Arc::new(DefinitionStore::new());
    let member_def = DefId(501);
    let parent_def = DefId(500);

    // Producing file's env registers the member->parent edge and, because
    // it holds the shared store, writes through to it.
    let mut producer_env = TypeEnvironment::new();
    producer_env.set_definition_store(Arc::clone(&store));
    producer_env.register_enum_parent(member_def, parent_def);
    assert_eq!(
        store.get_enum_parent(member_def),
        Some(parent_def),
        "register_enum_parent must write through to the shared store"
    );

    // Consuming file's env is fresh (its local enum_parents is empty, as
    // after a file-session reset) but shares the same store.
    let mut consumer_env = TypeEnvironment::new();
    consumer_env.set_definition_store(Arc::clone(&store));
    assert!(
        !consumer_env.enum_parents.contains_key(&member_def.0),
        "consumer's local enum_parents must be cold"
    );
    assert_eq!(
        consumer_env.get_enum_parent(member_def),
        Some(parent_def),
        "consumer must recover the parent via the shared-store fallback"
    );
    assert_eq!(
        TypeResolver::get_enum_parent_def_id(&consumer_env, member_def),
        Some(parent_def),
        "resolver-trait accessor must use the same fallback path"
    );

    // An env with no store still returns None (no panic, no false edge).
    let bare_env = TypeEnvironment::new();
    assert_eq!(bare_env.get_enum_parent(member_def), None);
}

/// Pins the `get_def_kind` store-fallback path:
/// an entry registered only in the `DefinitionStore` (not the local map)
/// must be found once `set_definition_store` is called.
#[test]
fn get_def_kind_falls_back_to_definition_store() {
    use crate::TypeId;
    use crate::def::DefKind;
    use crate::def::core::DefinitionInfo;
    use tsz_common::interner::Atom;

    let store = Arc::new(DefinitionStore::new());
    let def_id = store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    let mut env = TypeEnvironment::new();
    // No store wired → fallback returns None.
    assert_eq!(
        env.get_def_kind(def_id),
        None,
        "get_def_kind must return None when no store is wired"
    );

    // Wire the store → fallback finds the kind.
    env.set_definition_store(Arc::clone(&store));
    assert_eq!(
        env.get_def_kind(def_id),
        Some(DefKind::TypeAlias),
        "get_def_kind must find kind via store fallback after set_definition_store"
    );
}

/// `first_missing_entry_from` reports evaluator-only registrations without
/// repairing the flow-analyzer env. Missing entries must be fixed by routing
/// the writer through the checker's dual-env authority.
#[test]
fn first_missing_entry_from_reports_evaluator_only_maps_without_repairing() {
    let mut evaluator = TypeEnvironment::new();
    evaluator.insert(SymbolRef(7), TypeId(200));

    let flow = TypeEnvironment::new();
    assert_eq!(
        flow.first_missing_entry_from(&evaluator),
        Some(("types", "7".to_string())),
        "evaluator-only symbol-keyed type must be reported"
    );
    assert_eq!(
        flow.get(SymbolRef(7)),
        None,
        "missing-entry probe must not mutate the flow env"
    );

    let mut mirrored = TypeEnvironment::new();
    mirrored.insert(SymbolRef(7), TypeId(200));
    assert_eq!(mirrored.first_missing_entry_from(&evaluator), None);
}

#[test]
fn first_missing_entry_from_reports_missing_definition_store_without_repairing() {
    let store = Arc::new(DefinitionStore::new());

    let mut evaluator = TypeEnvironment::new();
    evaluator.set_definition_store(Arc::clone(&store));

    let flow = TypeEnvironment::new();
    assert_eq!(
        flow.first_missing_entry_from(&evaluator),
        Some(("definition_store", "shared".to_string())),
        "missing shared DefinitionStore must be reported as an evaluator-only scalar"
    );

    let mut mirrored = TypeEnvironment::new();
    mirrored.set_definition_store(Arc::clone(&store));
    assert_eq!(mirrored.first_missing_entry_from(&evaluator), None);
}

#[test]
fn first_def_divergence_from_detects_conflicting_shared_def_body() {
    let shared_def = DefId(42);

    let mut evaluator = TypeEnvironment::new();
    evaluator.insert_def(shared_def, TypeId(100));
    // An evaluator-only entry the flow env never received: not a divergence.
    evaluator.insert_def(DefId(7), TypeId(101));

    let mut flow = TypeEnvironment::new();
    flow.insert_def(shared_def, TypeId(200));

    // Probe is order-symmetric on the conflicting key.
    let divergence = flow.first_def_divergence_from(&evaluator);
    assert_eq!(
        divergence,
        Some(("def_types", shared_def.0, 200, 100)),
        "conflicting shared def body must be reported"
    );
}

#[test]
fn first_def_divergence_from_clean_after_mirrored_entries() {
    let shared_def = DefId(42);
    let flow_only_class = DefId(99);

    let mut evaluator = TypeEnvironment::new();
    evaluator.insert_def(shared_def, TypeId(100));

    let mut flow = TypeEnvironment::new();
    flow.insert_def(shared_def, TypeId(100));
    // Flow-analyzer-only mapping the evaluator never wrote.
    flow.register_class_extends(flow_only_class, DefId(98));

    assert_eq!(
        flow.first_def_divergence_from(&evaluator),
        None,
        "mirrored envs must agree on every shared DefId entry"
    );
    assert_eq!(
        flow.first_missing_entry_from(&evaluator),
        None,
        "mirrored envs must not need evaluator-to-flow repair"
    );
}

#[test]
fn collect_def_type_divergences_reports_every_present_but_different_shared_def() {
    let conflict_a = DefId(42);
    let conflict_b = DefId(43);
    let agree = DefId(44);

    let mut evaluator = TypeEnvironment::new();
    evaluator.insert_def(conflict_a, TypeId(100));
    evaluator.insert_def(conflict_b, TypeId(110));
    evaluator.insert_def(agree, TypeId(120));
    // Evaluator-only entry: vacancy, never a divergence.
    evaluator.insert_def(DefId(7), TypeId(101));

    let mut flow = TypeEnvironment::new();
    flow.insert_def(conflict_a, TypeId(200));
    flow.insert_def(conflict_b, TypeId(210));
    flow.insert_def(agree, TypeId(120));

    let mut divergences = flow.collect_def_type_divergences_from(&evaluator);
    divergences.sort_by_key(|&(key, ..)| key);
    assert_eq!(
        divergences,
        vec![
            (conflict_a.0, TypeId(200), TypeId(100)),
            (conflict_b.0, TypeId(210), TypeId(110)),
        ],
        "only present-but-different shared defs are divergences; agreeing and \
             vacant keys are excluded"
    );
}

#[test]
fn set_local_def_type_canonicalizes_without_store_write_through() {
    let store = Arc::new(DefinitionStore::default());
    let shared_def = DefId(42);

    let mut evaluator = TypeEnvironment::new();
    evaluator.set_definition_store(Arc::clone(&store));
    evaluator.insert_def(shared_def, TypeId(100)); // store body = 100 (authoritative)

    let mut flow = TypeEnvironment::new();
    flow.set_definition_store(Arc::clone(&store));
    // Simulate a local-only divergence: the flow env's local cache holds a
    // different `TypeId` than the authoritative store/evaluator (as happens
    // when a recursive interface materializes to distinct interned ids per
    // env). `set_local_def_type` is the only writer that touches the local
    // cache without store write-through, so use it to seed the divergence
    // too — proving the store is untouched in both directions.
    flow.set_local_def_type(shared_def.0, TypeId(200));
    assert_eq!(
        flow.first_def_divergence_from(&evaluator),
        Some(("def_types", shared_def.0, 200, 100)),
        "local-only flow cache must diverge from the authoritative evaluator value"
    );

    // Canonicalize the flow env's local cache onto the evaluator's value.
    flow.set_local_def_type(shared_def.0, TypeId(100));
    assert_eq!(
        flow.first_def_divergence_from(&evaluator),
        None,
        "set_local_def_type must converge the flow env onto the authoritative value"
    );
    // The shared store still holds the body the evaluator published; the
    // local-only setter never writes through it in either direction.
    assert_eq!(
        store.get_body(shared_def),
        Some(TypeId(100)),
        "set_local_def_type must not write through to the shared DefinitionStore"
    );
}

/// #14337: a lib/ambient utility alias (e.g. `Omit`) whose real
/// `Pick<…>` body has not yet materialized reads the `unknown` sentinel as
/// its registered body, exactly like a genuine `type C = unknown`. The
/// genuine-unknown classifier must NOT treat the lib placeholder as genuine
/// (which would reduce `Omit<T, K>` to bare `unknown`, dropping the picked
/// properties — the ts-rest `params`/`body` TS2339 false positives), while a
/// real user-program `type C = unknown` must still classify as genuine. The
/// discriminator is the def's file origin, NOT its name.
#[test]
fn lib_placeholder_unknown_alias_is_not_genuine_unknown_issue_14337() {
    let interner = crate::construction::TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());

    // Lib/ambient utility whose body sentinel is `unknown` (unmaterialized).
    // The binder name is varied to confirm the rule is structural, not a
    // name match.
    let mut lib_info =
        DefinitionInfo::type_alias(interner.intern_string("Strip"), vec![], TypeId::UNKNOWN);
    lib_info.file_id = Some(DefinitionStore::NON_PROGRAM_FILE_SENTINEL);
    let lib_def = store.register(lib_info);
    store.set_body(lib_def, TypeId::UNKNOWN);

    // User-program alias genuinely declared `type C = unknown`.
    let mut user_info = DefinitionInfo::type_alias(
        interner.intern_string("GenuineUnknown"),
        vec![],
        TypeId::UNKNOWN,
    );
    user_info.file_id = Some(0);
    let user_def = store.register(user_info);
    store.set_body(user_def, TypeId::UNKNOWN);

    let mut env = TypeEnvironment::new();
    env.set_definition_store(Arc::clone(&store));

    assert!(
        store.def_is_non_program(lib_def),
        "lib def must be classified non-program by its file sentinel"
    );
    assert!(
        !store.def_is_non_program(user_def),
        "user-program def must NOT be non-program"
    );

    assert!(
        !env.is_genuine_unknown_alias_body(lib_def, &interner),
        "a lib utility's not-yet-materialized `unknown` body must NOT be a \
             genuine `unknown` alias (#14337)"
    );
    assert!(
        env.is_genuine_unknown_alias_body(user_def, &interner),
        "a user-program `type C = unknown` must still be a genuine `unknown` alias"
    );
}
