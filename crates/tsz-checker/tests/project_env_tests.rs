//! Tests for `ProjectEnv` — the project-wide shared environment struct.
//!
//! Validates that `ProjectEnv::apply_to` correctly populates a checker context
//! with all project-level shared state in a single call.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId};
use tsz_checker::context::{GlobalDeclaredModules, ProjectEnv};
use tsz_checker::state::CheckerState;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::QueryCache;
use tsz_solver::TypeInterner;

/// Helper: create a minimal `ProjectEnv` with all fields defaulted.
fn empty_project_env() -> ProjectEnv {
    ProjectEnv {
        lib_contexts: vec![],
        all_arenas: Arc::new(vec![]),
        all_binders: Arc::new(vec![]),
        skeleton_declared_modules: None,
        skeleton_expando_index: None,
        symbol_file_targets: Arc::new(vec![]),
        resolved_module_paths: Arc::new(FxHashMap::default()),
        resolved_module_errors: Arc::new(FxHashMap::default()),
        is_external_module_by_file: Arc::new(FxHashMap::default()),
        file_is_esm_map: Arc::new(FxHashMap::default()),
        typescript_dom_replacement_globals: (false, false, false),
        has_deprecation_diagnostics: false,
    }
}

/// Helper: create a minimal checker for testing.
fn make_checker<'a>(
    arena: &'a NodeArena,
    binder: &'a BinderState,
    query_cache: &'a QueryCache<'a>,
) -> CheckerState<'a> {
    CheckerState::new(
        arena,
        binder,
        query_cache,
        "test.ts".to_string(),
        Default::default(),
    )
}

#[test]
fn apply_to_sets_core_shared_state() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let env = empty_project_env();
    env.apply_to(&mut checker.ctx);

    // Verify project-level state was applied
    assert!(checker.ctx.all_arenas.is_some());
    assert!(checker.ctx.all_binders.is_some());
    assert!(checker.ctx.is_external_module_by_file.is_some());
    assert!(checker.ctx.file_is_esm_map.is_some());
    assert!(checker.ctx.resolved_module_paths.is_some());
    assert!(checker.ctx.resolved_module_errors.is_some());
}

#[test]
fn apply_to_populates_skeleton_declared_modules() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut exact = FxHashSet::default();
    exact.insert("my-module".to_string());
    let dm = GlobalDeclaredModules::from_skeleton(exact, vec![]);

    let mut env = empty_project_env();
    env.skeleton_declared_modules = Some(Arc::new(dm));
    env.apply_to(&mut checker.ctx);

    let declared = checker.ctx.global_declared_modules.as_ref().unwrap();
    assert!(declared.exact.contains("my-module"));
}

#[test]
fn apply_to_populates_symbol_file_targets() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut env = empty_project_env();
    env.symbol_file_targets = Arc::new(vec![(SymbolId(1), 0), (SymbolId(2), 1)]);
    env.apply_to(&mut checker.ctx);

    let targets = checker.ctx.cross_file_symbol_targets.borrow();
    assert_eq!(targets.get(&SymbolId(1)), Some(&0));
    assert_eq!(targets.get(&SymbolId(2)), Some(&1));
}

#[test]
fn apply_to_sets_dom_replacement_globals() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut env = empty_project_env();
    env.typescript_dom_replacement_globals = (true, true, false);
    env.apply_to(&mut checker.ctx);

    assert!(checker.ctx.typescript_dom_replacement_loaded);
    assert!(checker.ctx.typescript_dom_replacement_has_window);
    assert!(!checker.ctx.typescript_dom_replacement_has_self);
}

#[test]
fn apply_to_sets_deprecation_diagnostics() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut env = empty_project_env();
    env.has_deprecation_diagnostics = true;
    env.apply_to(&mut checker.ctx);

    assert!(checker.ctx.skip_lib_type_resolution);
}
