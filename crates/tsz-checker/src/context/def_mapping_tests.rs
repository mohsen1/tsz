//! Unit tests for `def_mapping.rs` (dual-environment registration and
//! deferred flow-analyzer-env mirror replay).

use super::super::{CheckerContext, CheckerOptions};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::computation::TypeEnvironment;
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

/// Pins the dual-env wiring invariant introduced in #8269:
/// `ensure_both_envs_have_definition_store` must give `type_environment`
/// (the flow-analyzer snapshot) the `DefinitionStore` fallback, so that
/// `get_def_kind` works there without relying on the clone-over in
/// `source_file.rs`.
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

    // Wire both environments.
    ctx.ensure_both_envs_have_definition_store();

    // Now type_environment reaches the kind via the store fallback.
    assert_eq!(
        ctx.type_environment.borrow().get_def_kind(def_id),
        Some(DefKind::TypeAlias),
        "type_environment must find store-only DefKind via fallback after ensure_both_envs_have_definition_store"
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
