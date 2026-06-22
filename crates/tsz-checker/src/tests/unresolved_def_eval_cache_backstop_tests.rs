//! Regression guard for the `env_eval_cache` poisoning backstop (issue #13980).
//!
//! `TypeEvaluator::is_unresolved_def_seen()` is surfaced on the checker's
//! `EvaluateResult` *specifically* so a caller that persists a result keyed
//! purely on the input `TypeId` (with no generation/registration guard) can
//! refuse the write for a *registration-window artifact* — an `Application`
//! evaluated while its base `DefId` had no resolvable body. Such a result is a
//! function of which refs happened to be resolved when the pass ran, not of the
//! input `TypeId`, so caching it lets the under-resolved answer permanently
//! shadow the correct one once the def registers. The hazard only became
//! observable once on-demand forcing (issue #12101) dropped the eager
//! `ensure_refs_resolved` pre-walk that previously kept the flag `false`.
//!
//! `evaluate_type_with_env` is the authoritative `env_eval_cache` writer for the
//! relation-input evaluation path (`ensure_relation_input_ready`). These tests
//! pin that it consults the flag: the unresolved-base application must NOT be
//! cached, while a fully-resolvable type with no unresolved reference still is —
//! so the backstop is precise and never over-suppresses.
//!
//! The synthesized-type construction (`application`/`lazy` over an unregistered
//! `DefId`) lives in `src/tests/` so it stays out of checker `src/` proper, as
//! the architecture contract requires.

use crate::context::{CheckerContext, CheckerOptions};
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefId;

/// Build a checker over a trivial module and run `probe` against its
/// [`CheckerState`].
fn with_trivial_checker<R>(
    types: &TypeInterner,
    probe: impl FnOnce(&mut CheckerState<'_>) -> R,
) -> R {
    let mut parser = ParserState::new("fixture.ts".to_string(), "export {};".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let arena = parser.get_arena().clone();
    let mut checker = CheckerState {
        ctx: CheckerContext::new(
            &arena,
            &binder,
            types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        ),
    };
    checker.check_source_file(root);
    probe(&mut checker)
}

/// An `Application` over a `Lazy(DefId)` whose base has nothing registered is the
/// registration-window artifact: the evaluator marks `unresolved_def_seen`, so
/// `evaluate_type_with_env` must refuse to persist the (under-resolved) result
/// in the `TypeId`-keyed `env_eval_cache`. Without the backstop the opaque
/// result would be cached and shadow the real expansion once the def registers.
#[test]
fn unresolved_base_application_is_not_persisted_to_env_eval_cache() {
    let types = TypeInterner::new();
    let unregistered = types.lazy(DefId(987_654));
    let app = types.application(unregistered, vec![TypeId::NUMBER]);

    with_trivial_checker(&types, |checker| {
        let _ = checker.evaluate_type_with_env(app);
        assert!(
            checker.ctx.lookup_env_eval_cache(app).is_none(),
            "a result computed while the application's base def was unresolved is a \
             registration-window artifact and must not be persisted in the TypeId-keyed \
             env_eval_cache (issue #13980)",
        );
    });
}

/// Precision floor: a fully-resolvable type with no unresolved reference never
/// trips `unresolved_def_seen`, so the backstop must leave its `env_eval_cache`
/// write intact. This proves the suppression is keyed on the taint flag, not a
/// blanket disablement of the relation-input result memo.
#[test]
fn fully_resolvable_type_is_still_persisted_to_env_eval_cache() {
    let types = TypeInterner::new();
    let resolvable = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    with_trivial_checker(&types, |checker| {
        let _ = checker.evaluate_type_with_env(resolvable);
        assert!(
            checker.ctx.lookup_env_eval_cache(resolvable).is_some(),
            "a fully-resolvable type observed no unresolved def, so the backstop must not \
             suppress its env_eval_cache write (issue #13980)",
        );
    });
}
