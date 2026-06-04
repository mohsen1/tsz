use crate::query_boundaries::common::{
    collect_type_queries, contains_lazy_or_recursive, enum_def_id, get_type_query_symbol_ref,
    lazy_def_id,
};

use crate::query_boundaries::state::type_environment as query;

use crate::query_boundaries::type_defaults::fill_application_defaults;

use crate::query_boundaries::type_predicates::contains_conditional_with_application_extends;

use crate::state::CheckerState;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_solver::TypeId;

use tsz_solver::computation::TypeResolver;

use crate::query_boundaries::state::type_environment::for_each_direct_referenced_type;

pub(crate) use super::lazy_fuel::{
    global_resolution_fuel_exhausted, global_resolution_fuel_value,
    increment_global_resolution_fuel, reset_global_resolution_fuel, restore_global_resolution_fuel,
};

thread_local! {
    static APP_SYMBOL_RESOLUTION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Total `DefId` resolutions within `ensure_application_symbols_resolved`.
    static APP_SYMBOL_RESOLUTION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Total `DefId` resolutions across recursive `ensure_refs_resolved` cascades.
    static REFS_RESOLUTION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Tracks whether we're inside a top-level `ensure_refs_resolved` call tree.
    static REFS_RESOLUTION_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    // Depth counter for recursive `evaluate_type_with_env_impl` calls.
    static EVAL_ENV_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Reset ALL thread-local state in the lazy resolution module.
/// Called between compilation sessions to prevent cross-compilation contamination.
pub(crate) fn reset_all_thread_local_state() {
    APP_SYMBOL_RESOLUTION_DEPTH.set(0);
    APP_SYMBOL_RESOLUTION_FUEL.set(0);
    REFS_RESOLUTION_FUEL.set(0);
    REFS_RESOLUTION_ACTIVE.set(false);
    EVAL_ENV_DEPTH.set(0);
    reset_global_resolution_fuel();
}

const MAX_APP_SYMBOL_RESOLUTION_DEPTH: u32 = 1;

const MAX_APP_SYMBOL_RESOLUTION_FUEL: u32 = 200;

const MAX_REFS_RESOLUTION_FUEL: u32 = 2000;

/// Check if refs resolution fuel is exhausted.
pub(crate) fn refs_resolution_fuel_exhausted() -> bool {
    REFS_RESOLUTION_FUEL.get() >= MAX_REFS_RESOLUTION_FUEL
}

/// Increment the refs resolution fuel counter. Called from `ensure_refs_resolved`
/// each time a DefId is resolved via `resolve_and_insert_def_type`.
pub(crate) fn increment_refs_resolution_fuel() {
    REFS_RESOLUTION_FUEL.set(REFS_RESOLUTION_FUEL.get() + 1);
}

/// Enter a top-level refs resolution scope. Resets fuel if not already active.
/// Returns true if this is the outermost call (and thus responsible for cleanup).
pub(crate) fn enter_refs_resolution_scope() -> bool {
    if REFS_RESOLUTION_ACTIVE.get() {
        false
    } else {
        REFS_RESOLUTION_ACTIVE.set(true);
        REFS_RESOLUTION_FUEL.set(0);
        true
    }
}

/// Exit a top-level refs resolution scope.
pub(crate) fn exit_refs_resolution_scope() {
    REFS_RESOLUTION_ACTIVE.set(false);
}

include!("lazy_parts/part1.rs");
include!("lazy_parts/part2.rs");
