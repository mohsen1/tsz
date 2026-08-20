//! Unit tests for `def_mapping.rs` (dual-environment registration and
//! deferred flow-analyzer-env mirror replay).

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

/// Pins the dual-env store-wiring invariant as updated for #14348: the shared
/// `DefinitionStore` reaches both envs through the deferred-write authority,
/// including when the flow-analyzer env is borrowed and must receive replay.
#[test]
fn ensure_both_envs_wires_store_into_type_environment() {
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

    // Reset type_environment to a fresh instance: no local def_kinds, no store pointer.
    *ctx.type_environment.borrow_mut() = TypeEnvironment::new();

    // Without the store, the flow-analyzer env cannot find the DefKind.
    assert_eq!(
        ctx.type_environment.borrow().get_def_kind(def_id),
        None,
        "type_environment must not find a store-only DefKind before wiring"
    );

    {
        let held_flow = ctx.type_environment.borrow();
        ctx.ensure_both_envs_have_definition_store();
        assert_eq!(
            ctx.type_env.borrow().get_def_kind(def_id),
            Some(DefKind::TypeAlias),
            "type_env (authoritative) must find store-only DefKind eagerly"
        );
        assert_eq!(
            held_flow.get_def_kind(def_id),
            None,
            "held flow env must not see the store before deferred replay"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "flow-env store wiring must queue when the flow env is borrowed"
        );
    }

    ctx.flush_deferred_flow_env_writes();

    assert_eq!(
        ctx.type_environment.borrow().get_def_kind(def_id),
        Some(DefKind::TypeAlias),
        "type_environment must find store-only DefKind after deferred replay"
    );
}

/// Idempotency companion: calling `ensure_both_envs_have_definition_store`
/// a second time with the same store must not change either environment's
/// generation (the `Arc::ptr_eq` guard in `set_definition_store` must fire).
#[test]
fn ensure_both_envs_is_generation_idempotent_on_repeated_calls() {
    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    // First call wires the store.
    ctx.ensure_both_envs_have_definition_store();
    let gen_env = ctx.type_env.borrow().generation();
    let gen_flow = ctx.type_environment.borrow().generation();

    // Second call with the same Arc must not bump either generation.
    ctx.ensure_both_envs_have_definition_store();
    assert_eq!(
        ctx.type_env.borrow().generation(),
        gen_env,
        "type_env generation must not change on idempotent reinstall"
    );
    assert_eq!(
        ctx.type_environment.borrow().generation(),
        gen_flow,
        "type_environment generation must not change on idempotent reinstall"
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
    let after_eval_env = TypeResolver::resolver_generation(&ctx);
    assert!(
        after_eval_env > initial,
        "authoritative env mutations must move the resolver generation"
    );

    ctx.type_environment
        .borrow_mut()
        .insert(SymbolRef(2), TypeId::NUMBER);
    let after_flow_env = TypeResolver::resolver_generation(&ctx);
    assert!(
        after_flow_env > after_eval_env,
        "flow env mutations must move the resolver generation"
    );

    ctx.symbol_types.insert(SymbolId(1), TypeId::BOOLEAN);
    let after_symbol_type = TypeResolver::resolver_generation(&ctx);
    assert!(
        after_symbol_type > after_flow_env,
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

/// Regression for #8269: a dual-env registration whose flow-analyzer-env
/// mirror loses the `RefCell` borrow race must be *deferred and replayed*,
/// never silently dropped. Uses `register_class_extends` because the
/// `class_extends` map is flow-analyzer-env-local with no `DefinitionStore`
/// fallback, so the assertion isolates the local mirror write.
#[test]
fn flow_env_mirror_is_deferred_under_borrow_then_replayed() {
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

    // Simulate the flow-analyzer holding `type_environment` borrowed during
    // recursive resolution: the mirror-write cannot acquire the cell.
    {
        let held = ctx.type_environment.borrow();
        ctx.register_class_extends_in_envs(child, parent);

        // Evaluator env got the write directly.
        assert_eq!(
            ctx.type_env.borrow().get_class_extends_def(child),
            Some(parent),
            "evaluator env must receive the registration directly"
        );
        // Flow-analyzer env write was deferred, not dropped or applied.
        assert_eq!(
            held.get_class_extends_def(child),
            None,
            "flow-analyzer env must not yet have the deferred write"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "the lost mirror-write must be queued for replay"
        );
    }

    // Once the borrow is released the deferred write replays.
    ctx.flush_deferred_flow_env_writes();
    assert_eq!(
        ctx.deferred_flow_env_write_count(),
        0,
        "deferred queue must drain on flush"
    );
    assert_eq!(
        ctx.type_environment.borrow().get_class_extends_def(child),
        Some(parent),
        "flow-analyzer env must receive the replayed mirror-write"
    );
}

/// A subsequent successful mirror-write must drain the backlog before
/// applying itself, so deferred writes never require an explicit flush to
/// become visible during ongoing registration.
#[test]
fn flow_env_backlog_drains_on_next_successful_mirror() {
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
        let _held = ctx.type_environment.borrow();
        ctx.register_class_extends_in_envs(a, a);
        assert_eq!(ctx.deferred_flow_env_write_count(), 1);
    }

    // Next registration (no borrow held) drains the backlog, then applies.
    ctx.register_class_extends_in_envs(b, b);
    assert_eq!(
        ctx.deferred_flow_env_write_count(),
        0,
        "successful mirror-write must drain the backlog first"
    );
    let flow = ctx.type_environment.borrow();
    assert_eq!(
        flow.get_class_extends_def(a),
        Some(a),
        "previously-deferred write must be visible"
    );
    assert_eq!(
        flow.get_class_extends_def(b),
        Some(b),
        "the draining write itself must be visible"
    );
}

/// A class-instance registration that loses the authoritative `type_env` borrow
/// race (recursive resolution already holds it) must be deferred and replayed,
/// never dropped. Dropping it previously also dropped the shared-store
/// write-through, collapsing the instance type to `never` for later consumers.
#[test]
fn eval_env_class_instance_is_deferred_under_borrow_then_replayed() {
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
    ctx.ensure_both_envs_have_definition_store();

    let class_def = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let instance_type = TypeId::STRING;

    // Simulate recursive resolution holding `type_env` borrowed: the
    // authoritative write cannot acquire the cell.
    {
        let held = ctx.type_env.borrow();
        ctx.register_class_instance_in_envs(class_def, instance_type);

        // The authoritative write was deferred, not dropped or applied.
        assert_eq!(
            held.get_class_instance_type(class_def),
            None,
            "evaluator env must not yet have the deferred class-instance write"
        );
        assert_eq!(
            ctx.deferred_eval_env_write_count(),
            1,
            "the lost authoritative write must be queued for replay"
        );
    }

    // Once the borrow is released the deferred write replays into `type_env`.
    ctx.flush_deferred_eval_env_writes();
    assert_eq!(
        ctx.deferred_eval_env_write_count(),
        0,
        "deferred evaluator queue must drain on flush"
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_instance_type(class_def),
        Some(instance_type),
        "evaluator env must receive the replayed class-instance write"
    );
    // The replay went through the real env mutator, so the shared store's
    // write-through must also be populated (no `never` collapse for cross-file
    // consumers).
    assert_eq!(
        ctx.definition_store.get_class_instance_type(class_def),
        Some(instance_type),
        "shared store must receive the class-instance write-through on replay"
    );
}

/// A subsequent successful authoritative write must drain the evaluator backlog
/// before applying itself, so deferred writes become visible without an
/// explicit flush during ongoing registration.
#[test]
fn eval_env_backlog_drains_on_next_successful_write() {
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
        ctx.register_class_instance_in_envs(a, TypeId::STRING);
        assert_eq!(ctx.deferred_eval_env_write_count(), 1);
    }

    // Next registration (no borrow held) drains the backlog, then applies.
    ctx.register_class_instance_in_envs(b, TypeId::NUMBER);
    assert_eq!(
        ctx.deferred_eval_env_write_count(),
        0,
        "successful authoritative write must drain the backlog first"
    );
    let eval = ctx.type_env.borrow();
    assert_eq!(
        eval.get_class_instance_type(a),
        Some(TypeId::STRING),
        "previously-deferred class-instance write must be visible"
    );
    assert_eq!(
        eval.get_class_instance_type(b),
        Some(TypeId::NUMBER),
        "the draining write itself must be visible"
    );
}

#[test]
fn shared_store_seed_uses_borrowed_eval_env_and_deferred_flow_mirror() {
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
        let held_flow = ctx.type_environment.borrow();
        let mut eval_env = ctx.type_env.borrow_mut();
        ctx.seed_shared_store_def_in_envs(
            Some(&mut eval_env),
            def_id,
            TypeId::STRING,
            params.clone(),
        );

        assert_eq!(
            eval_env.get_def(def_id),
            Some(TypeId::STRING),
            "borrowed evaluator env must receive the warm-up body directly"
        );
        assert_eq!(
            eval_env.get_def_params(def_id),
            Some(params.as_slice()),
            "borrowed evaluator env must receive warm-up type params"
        );
        assert_eq!(
            ctx.deferred_eval_env_write_count(),
            0,
            "using the borrowed evaluator env must not queue an authoritative write"
        );
        assert_eq!(
            held_flow.get_def(def_id),
            None,
            "held flow env must not receive the mirror until replay"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "flow mirror must be queued rather than dropped"
        );
    }

    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.deferred_flow_env_write_count(), 0);
    assert_eq!(
        ctx.type_environment.borrow().get_def(def_id),
        Some(TypeId::STRING),
        "flow env must receive the replayed warm-up body"
    );
    assert_eq!(
        ctx.type_environment.borrow().get_def_params(def_id),
        Some(params.as_slice()),
        "flow env must receive replayed warm-up type params"
    );
}

#[test]
fn shared_store_seed_defers_authoritative_write_when_eval_env_is_borrowed() {
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
        let held_eval = ctx.type_env.borrow();
        ctx.seed_shared_store_def_in_envs(None, def_id, TypeId::BOOLEAN, Vec::new());

        assert_eq!(
            held_eval.get_def(def_id),
            None,
            "held evaluator env must not receive the queued warm-up body yet"
        );
        assert_eq!(
            ctx.deferred_eval_env_write_count(),
            1,
            "lost authoritative warm-up write must be queued"
        );
        assert_eq!(
            ctx.type_environment.borrow().get_def(def_id),
            Some(TypeId::BOOLEAN),
            "flow env mirror can still apply when it is borrowable"
        );
    }

    ctx.flush_deferred_eval_env_writes();
    assert_eq!(ctx.deferred_eval_env_write_count(), 0);
    assert_eq!(
        ctx.type_env.borrow().get_def(def_id),
        Some(TypeId::BOOLEAN),
        "evaluator env must receive the replayed warm-up body"
    );
}

/// Both type environments share one race-safe write discipline
/// (`apply_or_defer_env_write`). When a single dual-env registration races
/// *both* cells at once — the exact recursive-resolution scenario where the flow
/// analyzer holds `type_environment` while the re-entrant evaluator holds
/// `type_env` — neither write may be dropped: each must defer onto its own queue
/// and replay symmetrically on flush.
#[test]
fn both_envs_defer_then_replay_under_simultaneous_borrow() {
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

    // Hold both cells borrowed, so the evaluator write *and* the flow mirror
    // each lose the borrow race. `class_extends` is env-local with no shared
    // store fallback, so the reads below isolate the per-env map.
    {
        let held_eval = ctx.type_env.borrow();
        let held_flow = ctx.type_environment.borrow();
        ctx.register_class_extends_in_envs(child, parent);

        assert_eq!(
            held_eval.get_class_extends_def(child),
            None,
            "evaluator env must not yet have the deferred write"
        );
        assert_eq!(
            held_flow.get_class_extends_def(child),
            None,
            "flow-analyzer env must not yet have the deferred write"
        );
        assert_eq!(
            ctx.deferred_eval_env_write_count(),
            1,
            "the lost evaluator write must be queued for replay"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "the lost flow mirror must be queued for replay"
        );
    }

    // Both queues drain symmetrically once the cells are borrowable again.
    ctx.flush_deferred_eval_env_writes();
    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.deferred_eval_env_write_count(), 0);
    assert_eq!(ctx.deferred_flow_env_write_count(), 0);
    assert_eq!(
        ctx.type_env.borrow().get_class_extends_def(child),
        Some(parent),
        "evaluator env must receive the replayed write"
    );
    assert_eq!(
        ctx.type_environment.borrow().get_class_extends_def(child),
        Some(parent),
        "flow-analyzer env must receive the replayed write"
    );
}

/// Enum metadata published by symbol resolution uses the same dual-env
/// deferred-write discipline as bodies and class instances. Holding both envs
/// borrowed must queue, then replay, the numeric-enum marker and member-parent
/// edge symmetrically.
#[test]
fn enum_metadata_defer_then_replay_under_simultaneous_borrow() {
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
        let held_eval = ctx.type_env.borrow();
        let held_flow = ctx.type_environment.borrow();
        ctx.register_numeric_enum_in_envs(enum_def);
        ctx.register_enum_parent_in_envs(member_def, enum_def);

        assert!(!held_eval.is_numeric_enum(enum_def));
        assert!(!held_flow.is_numeric_enum(enum_def));
        assert_eq!(held_eval.get_enum_parent(member_def), None);
        assert_eq!(held_flow.get_enum_parent(member_def), None);
        assert_eq!(ctx.deferred_eval_env_write_count(), 2);
        assert_eq!(ctx.deferred_flow_env_write_count(), 2);
    }

    ctx.flush_deferred_eval_env_writes();
    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.deferred_eval_env_write_count(), 0);
    assert_eq!(ctx.deferred_flow_env_write_count(), 0);
    assert!(ctx.type_env.borrow().is_numeric_enum(enum_def));
    assert!(ctx.type_environment.borrow().is_numeric_enum(enum_def));
    assert_eq!(
        ctx.type_env.borrow().get_enum_parent(member_def),
        Some(enum_def)
    );
    assert_eq!(
        ctx.type_environment.borrow().get_enum_parent(member_def),
        Some(enum_def)
    );
}

/// Child checker snapshots are parent-wins vacancy fills, not body rewrites.
/// They still must use the same deferred dual-env write discipline as ordinary
/// registrations: a borrow conflict queues the merge for replay, and replay
/// inserts only when the parent env has not already published the entry.
#[test]
fn child_snapshot_merge_defer_then_replay_preserves_parent_entries() {
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
        let held_eval = ctx.type_env.borrow();
        let held_flow = ctx.type_environment.borrow();
        ctx.merge_def_if_missing_in_envs(def_id, TypeId::NUMBER);
        ctx.merge_class_instance_if_missing_in_envs(class_def, TypeId::STRING);
        ctx.merge_class_extends_if_missing_in_envs(child_def, parent_def);

        assert_eq!(held_eval.get_def(def_id), None);
        assert_eq!(held_flow.get_def(def_id), None);
        assert_eq!(held_eval.get_class_instance_type(class_def), None);
        assert_eq!(held_flow.get_class_instance_type(class_def), None);
        assert_eq!(held_eval.get_class_extends_def(child_def), None);
        assert_eq!(held_flow.get_class_extends_def(child_def), None);
        assert_eq!(ctx.deferred_eval_env_write_count(), 3);
        assert_eq!(ctx.deferred_flow_env_write_count(), 3);
    }

    ctx.flush_deferred_eval_env_writes();
    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.type_env.borrow().get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        ctx.type_environment.borrow().get_def(def_id),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_instance_type(class_def),
        Some(TypeId::STRING)
    );
    assert_eq!(
        ctx.type_environment
            .borrow()
            .get_class_instance_type(class_def),
        Some(TypeId::STRING)
    );
    assert_eq!(
        ctx.type_env.borrow().get_class_extends_def(child_def),
        Some(parent_def)
    );
    assert_eq!(
        ctx.type_environment
            .borrow()
            .get_class_extends_def(child_def),
        Some(parent_def)
    );

    let other_parent = DefId(25);
    ctx.merge_def_if_missing_in_envs(def_id, TypeId::STRING);
    ctx.merge_class_instance_if_missing_in_envs(class_def, TypeId::BOOLEAN);
    ctx.merge_class_extends_if_missing_in_envs(child_def, other_parent);

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

/// Pins the body-registration construction rule shared by
/// `register_resolved_def_in_envs` and `mirror_def_in_type_environment`:
/// empty params build the non-generic `InsertDef` variant, non-empty params
/// build `InsertDefWithParams` with the threaded-through variances. This is the
/// single rule both pure-construction sites now route through
/// (`DeferredFlowEnvWrite::insert_def_choosing_params`), so a drift in one of
/// them is caught here. Uses synthetic `DefId`s, so it is binder-name
/// independent.
#[test]
fn insert_def_choosing_params_selects_variant_by_params_emptiness() {
    use super::super::deferred_flow_env_write::DeferredFlowEnvWrite;
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;
    use tsz_solver::def::DefId;

    let def_id = DefId(7);
    let body = TypeId::STRING;

    // Empty params -> non-generic InsertDef, variances ignored.
    let empty = DeferredFlowEnvWrite::insert_def_choosing_params(def_id, body, vec![], None);
    assert!(
        matches!(
            empty,
            DeferredFlowEnvWrite::InsertDef { def_id: d, body: b } if d == def_id && b == body
        ),
        "empty params must build the non-generic InsertDef variant"
    );

    // Non-empty params -> generic InsertDefWithParams; `None` variances flow through.
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];
    let generic =
        DeferredFlowEnvWrite::insert_def_choosing_params(def_id, body, params.clone(), None);
    assert!(
        matches!(
            generic,
            DeferredFlowEnvWrite::InsertDefWithParams {
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
/// the flow-analyzer env sees a non-generic value/constructor mapping while the
/// evaluator env sees the generic one.
#[test]
fn deferred_symbol_type_write_preserves_params_on_replay() {
    use super::super::deferred_flow_env_write::DeferredFlowEnvWrite;
    use tsz_common::interner::Atom;
    use tsz_solver::{SymbolRef, TypeId};

    let symbol = SymbolRef(17);
    let params = vec![tsz_solver::TypeParamInfo::simple(Atom::default())];
    let mut env = TypeEnvironment::new();

    DeferredFlowEnvWrite::InsertSymbolType {
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

/// `register_symbol_type_in_envs` must use the same deferred-write path for
/// generic `SymbolRef` mappings as it does for `DefId` mappings. Holding the
/// flow env borrowed forces the mirror leg onto the queue, proving replay keeps
/// both envs' params aligned.
#[test]
fn symbol_type_registration_deferred_mirror_preserves_params() {
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
        let held_flow = ctx.type_environment.borrow();
        ctx.register_symbol_type_in_envs(symbol, TypeId::NUMBER, params.clone());

        assert_eq!(
            ctx.type_env.borrow().get_params(symbol),
            Some(params.as_slice()),
            "evaluator env must receive params immediately"
        );
        assert_eq!(
            held_flow.get(symbol),
            None,
            "flow env write must wait while the env is borrowed"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "generic SymbolRef mirror must be queued rather than dropped"
        );
    }

    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.deferred_flow_env_write_count(), 0);
    assert_eq!(
        ctx.type_environment.borrow().get(symbol),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        ctx.type_environment.borrow().get_params(symbol),
        Some(params.as_slice()),
        "flow env replay must preserve the same params as the evaluator env"
    );
}

/// Augmentation merges re-publish an existing `DefId` body. The replay path
/// must preserve the def's type params; otherwise a generic interface merged by
/// module/global augmentation becomes non-generic in whichever env lost the
/// borrow race.
#[test]
fn augmented_def_registration_deferred_mirror_preserves_params() {
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
    ctx.register_def_with_params_in_envs(def_id, TypeId::STRING, params.clone());

    {
        let held_flow = ctx.type_environment.borrow();
        ctx.register_augmented_def_in_envs(def_id, TypeId::NUMBER, false);

        let eval = ctx.type_env.borrow();
        assert_eq!(eval.get_def(def_id), Some(TypeId::NUMBER));
        assert_eq!(
            eval.get_def_params(def_id),
            Some(params.as_slice()),
            "evaluator env must preserve params while publishing the augmented body"
        );
        assert_eq!(
            held_flow.get_def(def_id),
            Some(TypeId::STRING),
            "flow env write must wait while the env is borrowed"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "augmented def mirror must be queued rather than dropped"
        );
    }

    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.deferred_flow_env_write_count(), 0);
    let flow = ctx.type_environment.borrow();
    assert_eq!(flow.get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        flow.get_def_params(def_id),
        Some(params.as_slice()),
        "flow env replay must preserve generic params from the original def"
    );
}

/// Cross-file reads of a standard-library alias can temporarily recover the
/// alias's public identity (`Lazy(def)`) or its pre-materialization placeholder
/// (`UNKNOWN`). Once the structural body is published, neither result may
/// rewrite the shared body or either evaluator environment. Program aliases
/// remain outside this guard because `type Local = unknown` is meaningful.
#[test]
fn non_program_alias_placeholders_do_not_rewrite_materialized_bodies() {
    use tsz_solver::TypeId;
    use tsz_solver::def::{DefinitionInfo, DefinitionStore};

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let type_param = tsz_solver::TypeParamInfo::simple(types.intern_string("Element"));
    let type_params = vec![type_param];
    let mut lib_alias = DefinitionInfo::type_alias(
        types.intern_string("SelectKeys"),
        type_params.clone(),
        TypeId::STRING,
    );
    lib_alias.file_id = Some(DefinitionStore::NON_PROGRAM_FILE_SENTINEL);
    let lib_def = ctx.definition_store.register(lib_alias);
    ctx.register_def_with_params_in_envs(lib_def, TypeId::STRING, type_params.clone());

    let store_generation = ctx.definition_store.generation();
    let eval_generation = ctx.type_env.borrow().generation();
    let flow_generation = ctx.type_environment.borrow().generation();
    let self_identity = types.lazy(lib_def);

    ctx.register_def_with_params_in_envs(lib_def, TypeId::UNKNOWN, type_params.clone());
    ctx.register_def_with_params_in_envs(lib_def, self_identity, type_params.clone());
    assert_eq!(
        ctx.definition_body_for_env_registration(lib_def, TypeId::UNKNOWN),
        Some(TypeId::STRING),
    );
    assert_eq!(
        ctx.definition_body_for_env_registration(lib_def, self_identity),
        Some(TypeId::STRING),
    );
    assert!(!ctx.publish_definition_body(lib_def, TypeId::UNKNOWN));
    assert!(!ctx.publish_definition_body_with_params(lib_def, self_identity, type_params.clone(),));

    assert_eq!(ctx.definition_store.get_body(lib_def), Some(TypeId::STRING));
    assert_eq!(
        ctx.definition_store.get_type_params(lib_def),
        Some(type_params.clone()),
    );
    assert_eq!(ctx.type_env.borrow().get_def(lib_def), Some(TypeId::STRING));
    assert_eq!(
        ctx.type_environment.borrow().get_def(lib_def),
        Some(TypeId::STRING),
    );
    assert_eq!(
        ctx.definition_store.generation(),
        store_generation,
        "rejected placeholders must not invalidate shared definition readers",
    );
    assert_eq!(
        ctx.type_env.borrow().generation(),
        eval_generation,
        "rejected placeholders must not invalidate evaluator caches",
    );
    assert_eq!(
        ctx.type_environment.borrow().generation(),
        flow_generation,
        "rejected placeholders must not invalidate flow caches",
    );

    // A different file checker can share the canonical store while starting
    // with empty local evaluator environments. Replaying its placeholder must
    // seed those environments with the structural body, not merely avoid the
    // shared-store rewrite.
    *ctx.type_env.borrow_mut() = TypeEnvironment::new();
    *ctx.type_environment.borrow_mut() = TypeEnvironment::new();
    ctx.register_def_with_params_in_envs(lib_def, TypeId::UNKNOWN, type_params.clone());
    assert_eq!(ctx.type_env.borrow().get_def(lib_def), Some(TypeId::STRING));
    assert_eq!(
        ctx.type_env.borrow().get_def_params(lib_def),
        Some(type_params.as_slice()),
    );
    assert_eq!(
        ctx.type_environment.borrow().get_def(lib_def),
        Some(TypeId::STRING),
    );
    assert_eq!(
        ctx.type_environment.borrow().get_def_params(lib_def),
        Some(type_params.as_slice()),
    );

    let mut local_alias = DefinitionInfo::type_alias(
        types.intern_string("LocalUnknown"),
        Vec::new(),
        TypeId::STRING,
    );
    local_alias.file_id = Some(0);
    let local_def = ctx.definition_store.register(local_alias);
    ctx.register_def_in_envs(local_def, TypeId::UNKNOWN);
    assert_eq!(
        ctx.definition_store.get_body(local_def),
        Some(TypeId::UNKNOWN),
        "program aliases must retain genuine `unknown` bodies",
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

    // Attach the shared store to both envs, but leave their local param maps
    // empty. `get_def_params_owned` can see the params; `get_def_params` cannot.
    ctx.ensure_both_envs_have_definition_store();
    assert_eq!(ctx.type_env.borrow().get_def_params(def_id), None);
    assert_eq!(ctx.type_environment.borrow().get_def_params(def_id), None);
    assert_eq!(
        ctx.type_env.borrow().get_def_params_owned(def_id),
        Some(params.clone())
    );

    {
        let held_eval = ctx.type_env.borrow();
        ctx.register_augmented_def_in_envs(def_id, TypeId::NUMBER, false);

        assert_eq!(
            held_eval.get_def_params(def_id),
            None,
            "evaluator env local params must not be updated while type_env is borrowed"
        );
        assert_eq!(
            ctx.deferred_eval_env_write_count(),
            1,
            "authoritative augmented def write must be queued"
        );
        let flow = ctx.type_environment.borrow();
        assert_eq!(flow.get_def(def_id), Some(TypeId::NUMBER));
        assert_eq!(
            flow.get_def_params(def_id),
            Some(params.as_slice()),
            "flow env direct write must preserve params from the store"
        );
    }

    ctx.flush_deferred_eval_env_writes();
    assert_eq!(ctx.deferred_eval_env_write_count(), 0);
    let eval = ctx.type_env.borrow();
    assert_eq!(eval.get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        eval.get_def_params(def_id),
        Some(params.as_slice()),
        "evaluator env replay must preserve store-only params"
    );
}

/// Regression for #13944 / #13086: a `DefId -> TypeId` body re-published by the
/// lazy-resolution path (`register_resolved_def_in_envs`, the gateway
/// `try_insert_def_in_type_env` now uses) must travel the race-safe deferred
/// path. When a re-resolution writes a *different* body while the flow-analyzer
/// holds `type_environment` borrowed, the mirror must be deferred and replayed,
/// never silently dropped — a dropped mirror would leave the two envs disagreeing
/// on this shared `def_types` entry, which is exactly the divergence the
/// file-prep reconciliation guard reports.
#[test]
fn resolved_def_mirror_is_deferred_under_borrow_then_envs_reconcile() {
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

    // First publication (no borrow contention): both envs agree on the body.
    ctx.register_resolved_def_in_envs(def_id, TypeId::STRING);
    assert_eq!(ctx.type_env.borrow().get_def(def_id), Some(TypeId::STRING));
    assert_eq!(
        ctx.type_environment.borrow().get_def(def_id),
        Some(TypeId::STRING)
    );

    // A re-resolution publishes a *different* (e.g. further-unfolded) body while
    // the flow-analyzer holds `type_environment` borrowed during recursive
    // narrowing. The evaluator env advances; the flow-analyzer mirror is deferred.
    {
        let held = ctx.type_environment.borrow();
        ctx.register_resolved_def_in_envs(def_id, TypeId::NUMBER);

        assert_eq!(
            ctx.type_env.borrow().get_def(def_id),
            Some(TypeId::NUMBER),
            "evaluator env must advance to the latest resolved body"
        );
        assert_eq!(
            held.get_def(def_id),
            Some(TypeId::STRING),
            "flow-analyzer env must not yet have the deferred re-resolution write"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "the lost mirror-write must be queued, not dropped"
        );
    }

    // After flush both envs converge on the authoritative latest body, so the
    // reconciliation probe reports no divergence on the shared `def_types` entry.
    ctx.flush_deferred_flow_env_writes();
    assert_eq!(ctx.deferred_flow_env_write_count(), 0);
    assert_eq!(ctx.type_env.borrow().get_def(def_id), Some(TypeId::NUMBER));
    assert_eq!(
        ctx.type_environment.borrow().get_def(def_id),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        ctx.type_environment
            .borrow()
            .first_def_divergence_from(&ctx.type_env.borrow()),
        None,
        "envs must agree on every shared DefId -> TypeId after reconciliation"
    );
}

/// #14348: the unresolved-name resolution cache is authority-routed — it
/// reaches BOTH environments, and a borrow race defers (never silently
/// skips, which was the pre-#14348 behavior of the raw `try_borrow_mut`).
#[test]
fn unresolved_resolution_write_reaches_both_envs_and_defers_under_borrow() {
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let def_a = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));
    let def_b = ctx.definition_store.register(DefinitionInfo::type_alias(
        Atom::default(),
        vec![],
        TypeId::UNKNOWN,
    ));

    // Uncontended: both envs receive the cache entry directly.
    ctx.register_unresolved_resolution_in_envs("AliasedName".to_string(), def_a);
    assert_eq!(
        ctx.type_env.borrow().unresolved_resolution("AliasedName"),
        Some(def_a),
        "evaluator env must receive the resolution directly"
    );
    assert_eq!(
        ctx.type_environment
            .borrow()
            .unresolved_resolution("AliasedName"),
        Some(def_a),
        "flow-analyzer env must receive the mirrored resolution"
    );

    // Contended: the flow-env write defers and replays instead of skipping.
    {
        let held = ctx.type_environment.borrow();
        ctx.register_unresolved_resolution_in_envs("RenamedBinder".to_string(), def_b);
        assert_eq!(
            ctx.type_env.borrow().unresolved_resolution("RenamedBinder"),
            Some(def_b),
            "evaluator env must receive the resolution under flow-env borrow"
        );
        assert_eq!(
            held.unresolved_resolution("RenamedBinder"),
            None,
            "flow-analyzer env must not observe the write mid-borrow"
        );
        assert_eq!(
            ctx.deferred_flow_env_write_count(),
            1,
            "the mirror-write must queue for replay"
        );
    }
    ctx.flush_deferred_flow_env_writes();
    assert_eq!(
        ctx.type_environment
            .borrow()
            .unresolved_resolution("RenamedBinder"),
        Some(def_b),
        "flow-analyzer env must receive the replayed resolution"
    );
}

/// #14348: file-prep reconciliation must assert missing flow-env entries
/// instead of repairing them by copying from the evaluator env.
#[test]
fn reconcile_uses_missing_entry_probe_not_overlay_repair() {
    let source_file =
        std::fs::read_to_string("src/state/state_checking/source_file_env_reconcile.rs")
            .expect("failed to read source_file_env_reconcile.rs");
    let state_file = std::fs::read_to_string("src/state/state_checking/source_file.rs")
        .expect("failed to read source_file.rs");

    assert!(
        !source_file.contains(".overlay_missing_from("),
        "flow/evaluator reconcile must not repair missing entries with overlay_missing_from"
    );
    assert!(
        source_file.contains("first_missing_entry_from("),
        "flow/evaluator reconcile should assert missing entries through a read-only probe"
    );
    assert!(
        state_file.contains("flush_deferred_flow_env_writes();"),
        "file preparation must replay deferred flow-env writes before reconcile assertions"
    );
}
