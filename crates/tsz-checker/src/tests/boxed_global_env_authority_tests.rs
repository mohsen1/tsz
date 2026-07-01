use crate::context::{CheckerContext, CheckerOptions};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefId;
use tsz_solver::{IntrinsicKind, TypeId, TypeParamInfo};

const GLOBAL_RS: &str = include_str!("../types/type_checking/global.rs");

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

/// Boxed-global metadata registrations use the same race-safe deferred-write
/// discipline as body publications: a write that loses the env borrow race is
/// queued and replayed, never dropped or partially applied.
#[test]
fn boxed_global_metadata_writes_defer_then_replay_under_borrow() {
    use tsz_common::interner::Atom;

    let (arena, binder, types) = minimal_checker_ctx();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );

    let kind = IntrinsicKind::String;
    let boxed_type = TypeId::STRING;
    let array_base = TypeId::NUMBER;
    let def_id = DefId(31_001);
    let params = vec![TypeParamInfo::simple(Atom::default())];

    {
        let held = ctx.type_env.borrow();
        ctx.register_boxed_type_in_env(kind, boxed_type);
        ctx.register_array_base_type_in_env(array_base, params.clone());
        ctx.register_boxed_def_in_env(kind, boxed_type, def_id);

        assert_eq!(held.get_boxed_type(kind), None);
        assert_eq!(held.get_array_base_type(), None);
        assert_eq!(held.get_def(def_id), None);
        assert!(!held.is_boxed_def_id(def_id, kind));
        assert_eq!(
            ctx.deferred_env_write_count(),
            3,
            "boxed metadata writes must queue while the env is borrowed"
        );
    }

    ctx.flush_deferred_env_writes();
    assert_eq!(ctx.deferred_env_write_count(), 0);
    let env = ctx.type_env.borrow();
    assert_eq!(env.get_boxed_type(kind), Some(boxed_type));
    assert_eq!(env.get_array_base_type(), Some(array_base));
    assert_eq!(env.get_array_base_type_params(), params.as_slice());
    assert_eq!(env.get_def(def_id), Some(boxed_type));
    assert!(env.is_boxed_def_id(def_id, kind));
}

#[test]
fn register_boxed_types_uses_context_env_authority_for_env_publication() {
    let start = GLOBAL_RS
        .find("pub(crate) fn register_boxed_types")
        .expect("register_boxed_types remains in global.rs");
    let end = GLOBAL_RS[start..]
        .find("fn has_user_declared_registerable_global")
        .map(|offset| start + offset)
        .expect("next helper remains after register_boxed_types");
    let body = &GLOBAL_RS[start..end];

    for required in [
        "register_boxed_type_in_env",
        "register_array_base_type_in_env",
        "register_boxed_def_in_env",
    ] {
        assert!(
            body.contains(required),
            "boxed global env publication should go through CheckerContext::{required}"
        );
    }

    for forbidden in [
        "self.ctx.type_env.try_borrow_mut()",
        "env.set_boxed_type",
        "env.set_array_base_type",
        "env.insert_def",
        "env.register_boxed_def_id",
    ] {
        assert!(
            !body.contains(forbidden),
            "register_boxed_types must not publish boxed globals through raw env mutation: {forbidden}"
        );
    }
}
