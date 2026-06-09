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
