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
use tsz_binder::{BinderState, symbol_flags};
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
    with_seeded_trivial_checker(types, |_| (), |checker, ()| probe(checker))
}

fn with_seeded_trivial_checker<R, S>(
    types: &TypeInterner,
    seed_binder: impl FnOnce(&mut BinderState) -> S,
    probe: impl FnOnce(&mut CheckerState<'_>, S) -> R,
) -> R {
    let mut parser = ParserState::new("fixture.ts".to_string(), "export {};".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let seed = seed_binder(&mut binder);
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
    probe(&mut checker, seed)
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

/// A *bare* `Lazy(DefId)` (no `Application` wrapper) whose base has nothing
/// registered is the same registration-window artifact, reached through the
/// evaluator's canonical bare-`Lazy` path (`visit_lazy`) rather than the
/// application path. The evaluator must mark `unresolved_def_seen` there too, so
/// `evaluate_type_with_env` refuses to persist the opaque `input -> input`
/// result in the `TypeId`-keyed `env_eval_cache`. Without the mark the
/// under-resolved `Lazy` would be cached and shadow the real expansion once the
/// def registers (the cross-arena member-degradation class, #14347 / #13484 /
/// #10663). This guards every consumer that bottoms out at an unresolved bare
/// `Lazy` through `evaluate` (template-literal spans, string-intrinsic
/// arguments, mapped-type constraints, …).
#[test]
fn unresolved_bare_lazy_is_not_persisted_to_env_eval_cache() {
    let types = TypeInterner::new();
    let unregistered = types.lazy(DefId(987_655));

    with_trivial_checker(&types, |checker| {
        let _ = checker.evaluate_type_with_env(unregistered);
        assert!(
            checker.ctx.lookup_env_eval_cache(unregistered).is_none(),
            "a bare Lazy(DefId) evaluated while its base def was unresolved is a \
             registration-window artifact and must not be persisted in the TypeId-keyed \
             env_eval_cache (issue #14347)",
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

/// An `IndexAccess` whose object is a `Lazy(DefId)` with nothing registered is
/// the same registration-window artifact as the unresolved-base `Application`:
/// the indexed-access evaluator (`visit_lazy`) cannot resolve a body, so it falls
/// back to the deferred `IndexAccess(Lazy, K)` that re-interns to the input
/// `TypeId` (no progress). Before the indexed-access taint (issue #14347) this
/// opaque result was persisted in the `TypeId`-keyed `env_eval_cache` and would
/// permanently shadow the real member type once the declaring file published the
/// body. The evaluator now marks `unresolved_def_seen`, so the backstop must
/// refuse the write — mirroring the `Application`/`keyof`/conditional deferrals.
#[test]
fn unresolved_lazy_index_access_is_not_persisted_to_env_eval_cache() {
    let types = TypeInterner::new();
    let unregistered = types.lazy(DefId(987_655));
    let index_access = types.index_access(unregistered, TypeId::STRING);

    with_trivial_checker(&types, |checker| {
        let _ = checker.evaluate_type_with_env(index_access);
        assert!(
            checker.ctx.lookup_env_eval_cache(index_access).is_none(),
            "an index access whose object def was unresolved is a registration-window \
             artifact and must not be persisted in the TypeId-keyed env_eval_cache \
             (issue #14347)",
        );
    });
}

/// Precision floor for the indexed-access taint: a fully-resolvable index access
/// (`string[][number]` → `string`) observes no unresolved def and makes real
/// progress on the root, so the backstop must leave its `env_eval_cache` write
/// intact. This proves the indexed-access taint is keyed on an actually-missing
/// body, not a blanket disablement of the index-access result memo.
#[test]
fn resolvable_index_access_is_still_persisted_to_env_eval_cache() {
    let types = TypeInterner::new();
    let index_access = types.index_access(types.array(TypeId::STRING), TypeId::NUMBER);

    with_trivial_checker(&types, |checker| {
        let result = checker.evaluate_type_with_env(index_access);
        assert_eq!(
            result,
            TypeId::STRING,
            "string[][number] must evaluate to string",
        );
        assert!(
            checker.ctx.lookup_env_eval_cache(index_access).is_some(),
            "a fully-resolvable index access observed no unresolved def, so the backstop \
             must not suppress its env_eval_cache write (issue #14347)",
        );
    });
}

/// Relation-readiness prewalks have their own local fuel counters. They must
/// not also spend the session-global lazy-resolution fuel that guards actual
/// type-resolution work in [`CheckerContext::consume_fuel`]; otherwise a
/// prewalk over many references can starve the resolver before it reaches the
/// semantic operation that needs the budget.
#[test]
fn application_readiness_spends_local_fuel_without_global_lazy_fuel() {
    let types = TypeInterner::new();
    let unregistered = types.lazy(DefId(987_656));

    with_trivial_checker(&types, |checker| {
        checker.ctx.eval_session.reset_lazy_readiness_guards();
        checker.ctx.eval_session.reset_lazy_resolution_fuel();

        checker.ensure_application_symbols_resolved(unregistered);

        assert_eq!(
            checker.ctx.eval_session.app_symbol_resolution_fuel(),
            1,
            "application-symbol readiness should charge its local prewalk budget",
        );
        assert_eq!(
            checker.ctx.eval_session.lazy_resolution_fuel_value(),
            0,
            "application-symbol readiness must not spend the shared lazy-resolution budget",
        );
    });
}

/// The refs prewalk has the same ownership boundary as application-symbol
/// readiness: local prewalk fuel bounds graph walking, while the shared lazy
/// fuel belongs to actual type-resolution calls.
#[test]
fn refs_readiness_spends_local_fuel_without_global_lazy_fuel() {
    let types = TypeInterner::new();
    let unregistered = types.lazy(DefId(987_657));

    with_trivial_checker(&types, |checker| {
        checker.ctx.eval_session.reset_lazy_readiness_guards();
        checker.ctx.eval_session.reset_lazy_resolution_fuel();

        checker.ensure_refs_resolved(unregistered);

        assert_eq!(
            checker.ctx.eval_session.refs_resolution_fuel(),
            1,
            "refs readiness should charge its local prewalk budget",
        );
        assert_eq!(
            checker.ctx.eval_session.lazy_resolution_fuel_value(),
            0,
            "refs readiness must not spend the shared lazy-resolution budget",
        );
    });
}

/// At the local refs-fuel edge, readiness must still register the direct
/// `DefId` body needed by the current relation input, but the transitive tail
/// stays bounded. This preserves the #12144 direct-resolution fix while keeping
/// the shared lazy-resolution fuel owned by actual type-resolution work.
#[test]
fn refs_readiness_resolves_direct_def_at_local_fuel_edge_without_tail() {
    let types = TypeInterner::new();

    with_seeded_trivial_checker(
        &types,
        |binder| {
            let root_sym = binder
                .symbols
                .alloc(symbol_flags::TYPE_ALIAS, "Root".to_string());
            let leaf_sym = binder
                .symbols
                .alloc(symbol_flags::TYPE_ALIAS, "Leaf".to_string());
            (root_sym, leaf_sym)
        },
        |checker, (root_sym, leaf_sym)| {
            let root_def = checker.ctx.get_or_create_def_id(root_sym);
            let leaf_def = checker.ctx.get_or_create_def_id(leaf_sym);
            let root_lazy = types.lazy(root_def);
            let leaf_lazy = types.lazy(leaf_def);

            checker.ctx.symbol_types.insert(root_sym, leaf_lazy);
            checker.ctx.eval_session.reset_lazy_readiness_guards();
            checker.ctx.eval_session.reset_lazy_resolution_fuel();

            {
                let eval_session = std::rc::Rc::clone(&checker.ctx.eval_session);
                let _refs_scope = eval_session.enter_refs_resolution_scope();
                for _ in 0..eval_session.refs_resolution_fuel_limit().saturating_sub(1) {
                    eval_session.increment_refs_resolution_fuel();
                }

                checker.ensure_refs_resolved(root_lazy);
            }

            assert_eq!(
                checker.ctx.eval_session.refs_resolution_fuel(),
                checker.ctx.eval_session.refs_resolution_fuel_limit(),
                "refs readiness should spend exactly the final local fuel unit on the direct def",
            );
            assert_eq!(
                checker.ctx.eval_session.lazy_resolution_fuel_value(),
                0,
                "refs readiness at its local fuel edge must not spend shared lazy-resolution fuel",
            );
            let type_env = checker.ctx.type_env.borrow();
            assert_eq!(
                type_env.get_def(root_def),
                Some(leaf_lazy),
                "the direct root def should still be registered at the refs-fuel edge",
            );
            assert_eq!(
                type_env.get_def(leaf_def),
                None,
                "the transitive tail should not be walked after local refs fuel is exhausted",
            );
        },
    );
}
