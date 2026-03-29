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
        lib_contexts: Arc::new(vec![]),
        all_arenas: Arc::new(vec![]),
        all_binders: Arc::new(vec![]),
        skeleton_declared_modules: None,
        skeleton_expando_index: None,
        symbol_file_targets: Arc::new(vec![]),
        global_symbol_file_index: None,
        global_file_locals_index: None,
        global_module_exports_index: None,
        global_module_augmentations_index: None,
        global_augmentation_targets_index: None,
        global_module_binder_index: None,
        global_arena_index: None,
        resolved_module_paths: Arc::new(FxHashMap::default()),
        resolved_module_request_paths: Arc::new(FxHashMap::default()),
        resolved_module_errors: Arc::new(FxHashMap::default()),
        resolved_module_request_errors: Arc::new(FxHashMap::default()),
        is_external_module_by_file: Arc::new(FxHashMap::default()),
        file_is_esm_map: Arc::new(FxHashMap::default()),
        typescript_dom_replacement_globals: (false, false, false),
        has_deprecation_diagnostics: false,
        last_skeleton_fingerprint: None,
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
    assert!(checker.ctx.resolved_module_request_paths.is_some());
    assert!(checker.ctx.resolved_module_errors.is_some());
    assert!(checker.ctx.resolved_module_request_errors.is_some());
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
fn apply_to_populates_symbol_file_targets_fallback() {
    // Without global_symbol_file_index, entries go into the local overlay.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut env = empty_project_env();
    env.symbol_file_targets = Arc::new(vec![(SymbolId(1), 0), (SymbolId(2), 1)]);
    // Do NOT call build_global_symbol_file_index — exercises fallback path.
    env.apply_to(&mut checker.ctx);

    // resolve_symbol_file_index should find them in the local overlay.
    assert_eq!(checker.ctx.resolve_symbol_file_index(SymbolId(1)), Some(0));
    assert_eq!(checker.ctx.resolve_symbol_file_index(SymbolId(2)), Some(1));
}

#[test]
fn apply_to_uses_global_index_skips_local_copy() {
    // With global_symbol_file_index built, apply_to shares via Arc, NOT local overlay.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut env = empty_project_env();
    env.symbol_file_targets = Arc::new(vec![(SymbolId(1), 0), (SymbolId(2), 1)]);
    env.build_global_symbol_file_index();
    env.apply_to(&mut checker.ctx);

    // The local overlay should be empty (entries served from global index).
    assert!(
        checker.ctx.cross_file_symbol_targets.borrow().is_empty(),
        "With global index, local overlay should be empty after apply_to"
    );
    // But resolve_symbol_file_index should still find them via global fallback.
    assert_eq!(checker.ctx.resolve_symbol_file_index(SymbolId(1)), Some(0));
    assert_eq!(checker.ctx.resolve_symbol_file_index(SymbolId(2)), Some(1));
    assert!(checker.ctx.has_symbol_file_index(SymbolId(1)));
    assert!(!checker.ctx.has_symbol_file_index(SymbolId(999)));
}

#[test]
fn copy_and_merge_symbol_file_targets() {
    // Test the copy_symbol_file_targets_to / merge_symbol_file_targets_from helpers.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let parent = make_checker(&arena, &binder, &query_cache);
    let mut child = make_checker(&arena, &binder, &query_cache);

    // Add entries to parent overlay.
    parent.ctx.register_symbol_file_target(SymbolId(10), 0);
    parent.ctx.register_symbol_file_target(SymbolId(20), 1);

    // Copy to child.
    parent.ctx.copy_symbol_file_targets_to(&mut child.ctx);
    assert_eq!(child.ctx.resolve_symbol_file_index(SymbolId(10)), Some(0));
    assert_eq!(child.ctx.resolve_symbol_file_index(SymbolId(20)), Some(1));

    // Child discovers a new mapping.
    child.ctx.register_symbol_file_target(SymbolId(30), 2);

    // Merge back to parent.
    parent.ctx.merge_symbol_file_targets_from(&child.ctx);
    assert_eq!(
        parent.ctx.resolve_symbol_file_index(SymbolId(30)),
        Some(2),
        "New entry from child should be merged into parent"
    );
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

#[test]
fn build_global_indices_if_changed_rebuilds_on_first_call() {
    let mut env = empty_project_env();
    assert!(env.last_skeleton_fingerprint.is_none());

    let rebuilt = env.build_global_indices_if_changed(0xCAFE);
    assert!(rebuilt, "First call should always rebuild");
    assert_eq!(env.last_skeleton_fingerprint, Some(0xCAFE));
    // Global indices should be populated.
    assert!(env.global_file_locals_index.is_some());
    assert!(env.global_module_exports_index.is_some());
    assert!(env.global_module_augmentations_index.is_some());
    assert!(env.global_augmentation_targets_index.is_some());
    assert!(env.global_module_binder_index.is_some());
}

#[test]
fn build_global_indices_populates_module_binder_index() {
    let mut binder_a = BinderState::new();
    binder_a
        .module_exports
        .entry("\"my-lib\"".to_string())
        .or_default()
        .set("foo".to_string(), SymbolId(1));

    let mut binder_b = BinderState::new();
    binder_b
        .module_exports
        .entry("\"other-lib\"".to_string())
        .or_default()
        .set("bar".to_string(), SymbolId(2));

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(binder_a), Arc::new(binder_b)]);
    env.build_global_indices();

    let idx = env.global_module_binder_index.as_ref().unwrap();

    // Raw key (with quotes) should map to binder index
    let binders_raw = idx.get("\"my-lib\"").unwrap();
    assert!(binders_raw.contains(&0));
    assert!(!binders_raw.contains(&1));

    // Normalized key (without quotes) should also map to the same binder
    let binders_norm = idx.get("my-lib").unwrap();
    assert!(binders_norm.contains(&0));

    // other-lib maps to binder 1
    let binders_other = idx.get("other-lib").unwrap();
    assert!(binders_other.contains(&1));
    assert!(!binders_other.contains(&0));
}

#[test]
fn build_global_indices_if_changed_skips_when_fingerprint_matches() {
    let mut env = empty_project_env();

    // First build populates everything.
    let rebuilt = env.build_global_indices_if_changed(42);
    assert!(rebuilt);
    assert_eq!(env.last_skeleton_fingerprint, Some(42));

    // Second call with same fingerprint should skip.
    let rebuilt = env.build_global_indices_if_changed(42);
    assert!(!rebuilt, "Same fingerprint should skip rebuild");
    assert_eq!(env.last_skeleton_fingerprint, Some(42));
}

#[test]
fn build_global_indices_if_changed_rebuilds_on_different_fingerprint() {
    let mut env = empty_project_env();

    env.build_global_indices_if_changed(100);
    assert_eq!(env.last_skeleton_fingerprint, Some(100));

    // Different fingerprint triggers rebuild.
    let rebuilt = env.build_global_indices_if_changed(200);
    assert!(rebuilt, "Different fingerprint should trigger rebuild");
    assert_eq!(env.last_skeleton_fingerprint, Some(200));
}
