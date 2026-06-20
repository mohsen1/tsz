use std::sync::OnceLock;

/// Debug kill-switch for extending the application-arg canonicalization in
/// `CheckerState::evaluate_application_type` (`core.rs`) to type-parameter-bearing
/// applications (#14101, step 1b).
///
/// Recursive-heritage materialization reaches the same logical `Base<Args>` along
/// many paths carrying structurally-equal-but-differently-minted arg `TypeId`s;
/// converging them onto one canonical-arg evaluation result bounds the
/// per-`(type, prop)` memo proliferation that drives the exit-124 timeouts
/// (drizzle-orm/typebox/xstate/arktype). The canonicalization reuses the existing
/// `resolve_lazy_type` arg mapping, which preserves `Recursive`/cyclic refs (its
/// `visited` guard) and is a no-op on non-`Lazy` args, so declaration/type-parameter
/// identity is not collapsed.
///
/// Set `TSZ_DISABLE_APP_CANON_ARG_IDENTITY=1` to restore the legacy
/// monomorphic-only canonicalization for byte-parity bisection; defaults to enabled.
pub(super) fn app_canon_arg_identity_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSZ_DISABLE_APP_CANON_ARG_IDENTITY").is_err())
}
