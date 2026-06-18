use super::constructors::{missing_base_type_arg_fill, should_cache_base_expr_result};
use tsz_solver::TypeId;
use tsz_solver::construction::TypeInterner;

#[test]
fn base_expr_cache_predicate_only_caches_non_generic_paths() {
    assert!(should_cache_base_expr_result(0, false));
    assert!(!should_cache_base_expr_result(0, true));
    assert!(!should_cache_base_expr_result(1, false));
    assert!(!should_cache_base_expr_result(3, false));
}

// --- missing_base_type_arg_fill (#13484: `error` in a type-argument slot) ---
//
// A generic base class referenced with fewer type arguments than parameters
// fills the missing slots from `default -> constraint -> unknown` (tsc parity).
// The fill must never bake tsz's internal `error` cycle/fuel sentinel into an
// inherited member's type-argument slot, which is the kysely/zod/ts-pattern
// `error`/`never`-in-a-type-argument-slot leak family.

#[test]
fn fill_prefers_genuine_default_over_constraint() {
    let interner = TypeInterner::new();
    // default present and genuine: used verbatim, constraint ignored.
    assert_eq!(
        missing_base_type_arg_fill(&interner, Some(TypeId::STRING), Some(TypeId::NUMBER)),
        TypeId::STRING,
    );
}

#[test]
fn fill_falls_back_to_genuine_constraint_when_no_default() {
    let interner = TypeInterner::new();
    assert_eq!(
        missing_base_type_arg_fill(&interner, None, Some(TypeId::NUMBER)),
        TypeId::NUMBER,
    );
}

#[test]
fn fill_defaults_to_unknown_when_neither_present() {
    let interner = TypeInterner::new();
    assert_eq!(
        missing_base_type_arg_fill(&interner, None, None),
        TypeId::UNKNOWN,
    );
}

#[test]
fn error_sentinel_default_is_skipped_for_genuine_constraint() {
    let interner = TypeInterner::new();
    // A default that degraded to the internal `error` sentinel must not be
    // baked in; the genuine constraint is used instead.
    assert_eq!(
        missing_base_type_arg_fill(&interner, Some(TypeId::ERROR), Some(TypeId::NUMBER)),
        TypeId::NUMBER,
    );
}

#[test]
fn error_sentinel_in_both_slots_recovers_to_unknown() {
    let interner = TypeInterner::new();
    // The leak guard: both default and constraint degraded to `error`, so the
    // type-argument slot recovers to `unknown` rather than leaking `error`.
    assert_eq!(
        missing_base_type_arg_fill(&interner, Some(TypeId::ERROR), Some(TypeId::ERROR)),
        TypeId::UNKNOWN,
    );
    // Constraint-only degradation, no default, also recovers.
    assert_eq!(
        missing_base_type_arg_fill(&interner, None, Some(TypeId::ERROR)),
        TypeId::UNKNOWN,
    );
}
