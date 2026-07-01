//! Unit tests for `def_mapping.rs` (single-environment registration and
//! deferred env-write replay, #14348).

use super::super::{CheckerContext, CheckerOptions};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::computation::{TypeEnvironment, TypeResolver};
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefinitionInfo;

fn minimal_checker_ctx() -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new("fixture.ts".to_string(), "type T = string;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (
        Arc::new(parser.get_arena().clone()),
        Arc::new(binder),
        TypeInterner::new(),
    )
}

/// The shared `DefinitionStore` reaches the environment eagerly through
/// `ensure_env_has_definition_store`, so `get_def_kind` can fall back to it
/// for entries whose local registration was never written.
#[test]
fn ensure_env_has_definition_store_wires_store_fallback() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefKind;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    // Register a definition only in the shared store (not in any local env map).
    let def_id = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    // Reset the env to a fresh instance: no local def_kinds, no store pointer.
    *ctx.type_env.borrow_mut() = TypeEnvironment::new();

    // Without the store, the env cannot find the DefKind.
    assert_eq!(
        ctx.type_env.borrow().get_def_kind(def_id),
        None,
        "the env must not find a store-only DefKind before wiring"
    );

    ctx.ensure_env_has_definition_store();

    assert_eq!(
        ctx.type_env.borrow().get_def_kind(def_id),
        Some(DefKind::TypeAlias),
        "the env must find store-only DefKind through the store fallback"
    );
}

/// Idempotency companion: calling `ensure_env_has_definition_store` a second
/// time with the same store must not change the environment's generation (the
/// `Arc::ptr_eq` guard in `set_definition_store` must fire).
#[test]
fn ensure_env_has_definition_store_is_generation_idempotent() {
    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    ctx.ensure_env_has_definition_store();
    let generation = ctx.type_env.borrow().generation();

    ctx.ensure_env_has_definition_store();
    assert_eq!(
        ctx.type_env.borrow().generation(),
        generation,
        "type_env generation must not change on idempotent reinstall"
    );
}

#[test]
fn checker_context_resolver_generation_tracks_env_and_symbol_cache_state() {
    use tsz_binder::SymbolId;
    use tsz_solver::{SymbolRef, TypeId};

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let initial = TypeResolver::resolver_generation(&ctx);

    ctx.type_env
        .borrow_mut()
        .insert(SymbolRef(1), TypeId::STRING);
    let after_env = TypeResolver::resolver_generation(&ctx);
    assert!(
        after_env > initial,
        "env mutations must move the resolver generation"
    );

    ctx.symbol_types.insert(SymbolId(1), TypeId::BOOLEAN);
    let after_symbol_type = TypeResolver::resolver_generation(&ctx);
    assert!(
        after_symbol_type > after_env,
        "`symbol_types` mutations must move the resolver generation"
    );

    ctx.symbol_instance_types
        .insert(SymbolId(2), TypeId::BIGINT);
    let after_symbol_instance = TypeResolver::resolver_generation(&ctx);
    assert!(
        after_symbol_instance > after_symbol_type,
        "`symbol_instance_types` mutations must move the resolver generation"
    );
}

/// Regression for #8269/#14348: a registration that loses the `RefCell` borrow
/// race (e.g. flow analysis holds the env borrowed during recursive
/// resolution) must be *deferred and replayed*, never silently dropped. Uses
/// `register_class_extends` because the `class_extends` map is env-local with
/// no `DefinitionStore` fallback, so the assertion isolates the local write.
#[test]
fn env_write_is_deferred_under_borrow_then_replayed() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let child = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let parent = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    // Simulate flow analysis holding the env borrowed during recursive
    // resolution: the write cannot acquire the cell.
    {
        let held = ctx.type_env.borrow();
        ctx.register_class_extends_in_env(child, parent);

        assert_eq!(
            held.get_class_extends_def(child),
            None,
            "the env must not yet have the deferred write"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            1,
            "the lost write must be queued for replay"
        );
    }

    // Once the borrow is released the deferred write replays.
    ctx.flush_deferred_env_writes();
    assert_eq!(
        ctx.deferred_env_write_count(),
        0,
        "deferred queue must drain on flush"
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_extends_def(child),
        Some(parent),
        "the env must receive the replayed write"
    );
}

/// A deferred class-instance write must replay through the real env mutator so
/// the shared store's write-through also fires (no `never` collapse for
/// cross-file consumers).
#[test]
fn class_instance_write_through_reaches_shared_store_on_replay() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    // Wire the shared store so the write-through path is exercised on replay.
    ctx.ensure_env_has_definition_store();

    let class_def = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let instance_type = TypeId::STRING;

    {
        let held = ctx.type_env.borrow();
        ctx.register_class_instance_in_env(class_def, instance_type);
        assert_eq!(
            held.get_class_instance_type(class_def),
            None,
            "the env must not yet have the deferred class-instance write"
        );
        assert_eq!(ctx.deferred_env_write_count(), 1);
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    assert_eq!(
        ctx.type_env.borrow().get_class_instance_type(class_def),
        Some(instance_type),
        "the env must receive the replayed class-instance write"
    );
    assert_eq!(
        ctx.definition_store.get_class_instance_type(class_def),
        Some(instance_type),
        "shared store must receive the class-instance write-through on replay"
    );
}

/// A subsequent successful write must drain the backlog before applying
/// itself, so deferred writes become visible without an explicit flush during
/// ongoing registration.
#[test]
fn env_backlog_drains_on_next_successful_write() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let a = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let b = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    {
        let _held = ctx.type_env.borrow();
        ctx.register_class_instance_in_env(a, TypeId::STRING);
        assert_eq!(ctx.deferred_env_write_count(), 1);
    }

    // Next registration (no borrow held) drains the backlog, then applies.
    ctx.register_class_instance_in_env(b, TypeId::NUMBER);
    assert_eq!(
        ctx.deferred_env_write_count(),
        0,
        "successful write must drain the backlog first"
    );
    let env = ctx.type_env.borrow();
    assert_eq!(
        env.get_class_instance_type(a),
        Some(TypeId::STRING),
        "previously-deferred class-instance write must be visible"
    );
    assert_eq!(
        env.get_class_instance_type(b),
        Some(TypeId::NUMBER),
        "the draining write itself must be visible"
    );
}

/// Shared-store warm-up holds a long-lived mutable borrow of the env while
/// iterating symbols; `seed_shared_store_def_in_env` must apply directly to
/// that borrow (in queue-drain order) without queueing anything.
#[test]
fn shared_store_seed_uses_borrowed_env_directly() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_id = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let params = vec![tsz_solver::TypeParamInfo::simple(types.intern_string("T"))];
    ctx.definition_store
        .set_body_with_params(def_id, TypeId::STRING, Some(params.clone()));

    {
        let mut env = ctx.type_env.borrow_mut();
        ctx.seed_shared_store_def_in_env(Some(&mut env), def_id, TypeId::STRING, params.clone());

        assert_eq!(
            env.get_def(def_id),
            Some(TypeId::STRING),
            "borrowed env must receive the warm-up body directly"
        );
        assert_eq!(
            env.get_def_params(def_id),
            Some(params.as_slice()),
            "borrowed env must receive warm-up type params"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            0,
            "using the borrowed env must not queue a write"
        );
    }
}

/// Without a caller-held borrow, warm-up seeding routes through the ordinary
/// race-safe queue: a live read borrow defers the write for replay.
#[test]
fn shared_store_seed_defers_when_env_is_borrowed() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_id = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    ctx.definition_store.set_body(def_id, TypeId::BOOLEAN);

    {
        let held = ctx.type_env.borrow();
        ctx.seed_shared_store_def_in_env(None, def_id, TypeId::BOOLEAN, Vec::new());

        assert_eq!(
            held.get_def(def_id),
            None,
            "held env must not receive the queued warm-up body yet"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            1,
            "lost warm-up write must be queued"
        );
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    assert_eq!(
        ctx.type_env.borrow().get_def(def_id),
        Some(TypeId::BOOLEAN),
        "the env must receive the replayed warm-up body"
    );
}

/// Enum metadata published by symbol resolution uses the same deferred-write
/// discipline as bodies and class instances. Holding the env borrowed must
/// queue, then replay, the numeric-enum marker and member-parent edge.
#[test]
fn enum_metadata_defer_then_replay_under_borrow() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let enum_def = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let member_def = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    {
        let held = ctx.type_env.borrow();
        ctx.register_numeric_enum_in_env(enum_def);
        ctx.register_enum_parent_in_env(member_def, enum_def);

        assert!(!held.is_numeric_enum(enum_def));
        assert_eq!(held.get_enum_parent(member_def), None);
        assert_eq!(ctx.deferred_env_write_count(), 2);
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    assert!(ctx.type_env.borrow().is_numeric_enum(enum_def));
    assert_eq!(
        ctx.type_env.borrow().get_enum_parent(member_def),
        Some(enum_def)
    );
}

/// Child checker snapshots are parent-wins vacancy fills, not body rewrites.
/// They still must use the same deferred write discipline as ordinary
/// registrations: a borrow conflict queues the merge for replay, and replay
/// inserts only when the parent env has not already published the entry.
#[test]
fn child_snapshot_merge_defer_then_replay_preserves_existing_entries() {
    use tsz_solver::TypeId;
    use tsz_solver::def::DefId;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_id = DefId(21);
    let class_def = DefId(22);
    let child_def = DefId(23);
    let parent_def = DefId(24);

    {
        let held = ctx.type_env.borrow();
        ctx.merge_def_if_missing_in_env(def_id, TypeId::NUMBER);
        ctx.merge_class_instance_if_missing_in_env(class_def, TypeId::STRING);
        ctx.merge_class_extends_if_missing_in_env(child_def, parent_def);

        assert_eq!(held.get_def(def_id), None);
        assert_eq!(held.get_class_instance_type(class_def), None);
        assert_eq!(held.get_class_extends_def(child_def), None);
        assert_eq!(ctx.deferred_env_write_count(), 3);
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.type_env.borrow().get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        ctx.type_env.borrow().get_class_instance_type(class_def),
        Some(TypeId::STRING)
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_extends_def(child_def),
        Some(parent_def)
    );

    let other_parent = DefId(25);
    ctx.merge_def_if_missing_in_env(def_id, TypeId::STRING);
    ctx.merge_class_instance_if_missing_in_env(class_def, TypeId::BOOLEAN);
    ctx.merge_class_extends_if_missing_in_env(child_def, other_parent);

    assert_eq!(
        ctx.type_env.borrow().get_def(def_id),
        Some(TypeId::NUMBER),
        "merge-if-missing must not overwrite an existing parent body"
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_instance_type(class_def),
        Some(TypeId::STRING),
        "merge-if-missing must not overwrite an existing parent instance"
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_extends_def(child_def),
        Some(parent_def),
        "merge-if-missing must not overwrite an existing parent extends edge"
    );
}

/// Pins the body-registration construction rule shared by every
/// body-registration site (`DeferredEnvWrite::insert_def_choosing_params`):
/// empty params build the non-generic `InsertDef` variant, non-empty params
/// build `InsertDefWithParams` with the threaded-through variances. Uses
/// synthetic `DefId`s, so it is binder-name independent.
#[test]
fn insert_def_choosing_params_selects_variant_by_params_emptiness() {
    use super::super::deferred_env_write::DeferredEnvWrite;
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefId;

    let def_id = DefId(7);
    let body = TypeId::STRING;

    // Empty params -> non-generic InsertDef, variances ignored.
    let empty = DeferredEnvWrite::insert_def_choosing_params(def_id, body, vec![], None);
    assert!(
        matches!(
            empty,
            DeferredEnvWrite::InsertDef { def_id: d, body: b } if d == def_id && b == body
        ),
        "empty params must build the non-generic InsertDef variant"
    );

    // Non-empty params -> generic InsertDefWithParams; `None` variances flow through.
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];
    let generic = DeferredEnvWrite::insert_def_choosing_params(def_id, body, params.clone(), None);
    assert!(
        matches!(
            generic,
            DeferredEnvWrite::InsertDefWithParams {
                def_id: d,
                body: b,
                params: ref p,
                variances: None,
            } if d == def_id && b == body && p.len() == params.len()
        ),
        "non-empty params must build the generic InsertDefWithParams variant with the given variances"
    );
}

/// A `SymbolRef -> TypeId` registration can be generic too. Replaying a
/// deferred env write must preserve the symbol's type-parameter list, otherwise
/// the env sees a non-generic value/constructor mapping after replay.
#[test]
fn deferred_symbol_type_write_preserves_params_on_replay() {
    use super::super::deferred_env_write::DeferredEnvWrite;
    use tsz_common::interner::Atom;
    use tsz_solver::{SymbolRef, TypeId};

    let symbol = SymbolRef(17);
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];
    let mut env = TypeEnvironment::new();

    DeferredEnvWrite::InsertSymbolType {
        symbol,
        ty: TypeId::STRING,
        params: params.clone(),
    }
    .apply(&mut env);

    assert_eq!(env.get(symbol), Some(TypeId::STRING));
    assert_eq!(
        env.get_params(symbol),
        Some(params.as_slice()),
        "deferred symbol writes must replay generic params"
    );
}

/// `register_symbol_type_in_env` must use the same deferred-write path for
/// generic `SymbolRef` mappings as it does for `DefId` mappings, preserving
/// params on replay.
#[test]
fn symbol_type_registration_deferred_write_preserves_params() {
    use tsz_common::interner::Atom;
    use tsz_solver::{SymbolRef, TypeId};

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let symbol = SymbolRef(23);
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];

    {
        let held = ctx.type_env.borrow();
        ctx.register_symbol_type_in_env(symbol, TypeId::NUMBER, params.clone());

        assert_eq!(
            held.get(symbol),
            None,
            "the env write must wait while the env is borrowed"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            1,
            "generic SymbolRef write must be queued rather than dropped"
        );
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    assert_eq!(ctx.type_env.borrow().get(symbol), Some(TypeId::NUMBER));
    assert_eq!(
        ctx.type_env.borrow().get_params(symbol),
        Some(params.as_slice()),
        "replay must preserve the registration's params"
    );
}

/// Augmentation merges re-publish an existing `DefId` body. The replay path
/// must preserve the def's type params; otherwise a generic interface merged by
/// module/global augmentation becomes non-generic after a lost borrow race.
#[test]
fn augmented_def_registration_preserves_params_on_replay() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_id = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];
    ctx.register_def_with_params_in_env(def_id, TypeId::STRING, params.clone());

    {
        let held = ctx.type_env.borrow();
        ctx.register_augmented_def_in_env(def_id, TypeId::NUMBER, false);

        assert_eq!(
            held.get_def(def_id),
            Some(TypeId::STRING),
            "the env write must wait while the env is borrowed"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            1,
            "augmented def write must be queued rather than dropped"
        );
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    let env = ctx.type_env.borrow();
    assert_eq!(env.get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        env.get_def_params(def_id),
        Some(params.as_slice()),
        "replay must preserve generic params from the original def"
    );
}

/// Companion for the shared-store path: a cross-file checker can observe a
/// generic def whose params live only in `DefinitionStore`, not in its local
/// `TypeEnvironment`. Replaying an augmentation merge must still re-insert the
/// augmented body as generic by using `get_def_params_owned`.
#[test]
fn augmented_def_registration_uses_store_only_params_on_replay() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_id = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];
    ctx.definition_store.set_type_params(def_id, params.clone());

    // Attach the shared store, but leave the local param map empty.
    // `get_def_params_owned` can see the params; `get_def_params` cannot.
    ctx.ensure_env_has_definition_store();
    assert_eq!(ctx.type_env.borrow().get_def_params(def_id), None);
    assert_eq!(
        ctx.type_env.borrow().get_def_params_owned(def_id),
        Some(params.clone())
    );

    {
        let held = ctx.type_env.borrow();
        ctx.register_augmented_def_in_env(def_id, TypeId::NUMBER, false);

        assert_eq!(
            held.get_def_params(def_id),
            None,
            "local params must not be updated while the env is borrowed"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            1,
            "augmented def write must be queued"
        );
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    let env = ctx.type_env.borrow();
    assert_eq!(env.get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        env.get_def_params(def_id),
        Some(params.as_slice()),
        "replay must preserve store-only params"
    );
}

/// Regression for #13944 / #13086: a `DefId -> TypeId` body re-published by the
/// lazy-resolution path (`register_resolved_def_in_env`, the gateway
/// `try_insert_def_in_type_env` now uses) must travel the race-safe deferred
/// path. When a re-resolution writes a *different* body while the env is
/// borrowed, the write must be deferred and replayed, never silently dropped.
#[test]
fn resolved_def_republication_is_deferred_under_borrow_then_replayed() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefinitionInfo;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_id = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    // First publication (no borrow contention).
    ctx.register_resolved_def_in_env(def_id, TypeId::STRING);
    assert_eq!(ctx.type_env.borrow().get_def(def_id), Some(TypeId::STRING));

    // A re-resolution publishes a *different* (e.g. further-unfolded) body
    // while the env is borrowed (e.g. by flow narrowing). The write is
    // deferred and the env keeps the previous body until replay.
    {
        let held = ctx.type_env.borrow();
        ctx.register_resolved_def_in_env(def_id, TypeId::NUMBER);

        assert_eq!(
            held.get_def(def_id),
            Some(TypeId::STRING),
            "the env must not yet have the deferred re-resolution write"
        );
        assert_eq!(
            ctx.deferred_env_write_count(),
            1,
            "the lost write must be queued, not dropped"
        );
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    assert_eq!(
        ctx.type_env.borrow().get_def(def_id),
        Some(TypeId::NUMBER),
        "the env must advance to the latest resolved body after replay"
    );
}
