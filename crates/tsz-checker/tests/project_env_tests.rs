//! Tests for `ProjectEnv` — the project-wide shared environment struct.
//!
//! Validates that `ProjectEnv::apply_to` correctly populates a checker context
//! with all project-level shared state in a single call.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::{BinderState, SemanticDefEntry, SemanticDefKind, SymbolId};
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
        global_file_locals_index: None,
        global_module_exports_index: None,
        global_module_augmentations_index: None,
        global_augmentation_targets_index: None,
        global_module_binder_index: None,
        resolved_module_paths: Arc::new(FxHashMap::default()),
        resolved_module_errors: Arc::new(FxHashMap::default()),
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

#[test]
fn apply_to_pre_populates_cross_file_def_ids() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    // Create a second binder simulating a cross-file source with a top-level class.
    let mut other_binder = BinderState::new();
    let other_sym_id = SymbolId(42);
    other_binder.semantic_defs.insert(
        other_sym_id,
        SemanticDefEntry {
            kind: SemanticDefKind::Class,
            name: "MyClass".to_string(),
            file_id: 1,
            span_start: 0,
            type_param_count: 0,
        },
    );

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(other_binder)]);
    env.apply_to(&mut checker.ctx);

    // The cross-file DefId should now be resolvable via get_existing_def_id
    // without hitting the O(N) repair path in get_or_create_def_id.
    let def_id = checker.ctx.get_existing_def_id(other_sym_id);
    assert!(
        def_id.is_some(),
        "Cross-file semantic_defs should be pre-populated by apply_to"
    );
}

#[test]
fn apply_to_pre_populates_multiple_cross_file_binders() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    // Two cross-file binders with different declarations.
    let mut binder_a = BinderState::new();
    binder_a.semantic_defs.insert(
        SymbolId(10),
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Foo".to_string(),
            file_id: 0,
            span_start: 0,
            type_param_count: 0,
        },
    );
    let mut binder_b = BinderState::new();
    binder_b.semantic_defs.insert(
        SymbolId(20),
        SemanticDefEntry {
            kind: SemanticDefKind::TypeAlias,
            name: "Bar".to_string(),
            file_id: 1,
            span_start: 100,
            type_param_count: 0,
        },
    );

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(binder_a), Arc::new(binder_b)]);
    env.apply_to(&mut checker.ctx);

    assert!(
        checker.ctx.get_existing_def_id(SymbolId(10)).is_some(),
        "Foo from binder_a should be pre-populated"
    );
    assert!(
        checker.ctx.get_existing_def_id(SymbolId(20)).is_some(),
        "Bar from binder_b should be pre-populated"
    );
    // Different symbols should get different DefIds.
    let def_a = checker.ctx.get_existing_def_id(SymbolId(10)).unwrap();
    let def_b = checker.ctx.get_existing_def_id(SymbolId(20)).unwrap();
    assert_ne!(def_a, def_b, "Different symbols must get distinct DefIds");
}

#[test]
fn apply_to_pre_populates_generic_type_param_stubs() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    // Create a binder with a generic interface (3 type params).
    let mut other_binder = BinderState::new();
    let sym_id = SymbolId(99);
    other_binder.semantic_defs.insert(
        sym_id,
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Triple".to_string(),
            file_id: 0,
            span_start: 0,
            type_param_count: 3,
        },
    );

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(other_binder)]);
    env.apply_to(&mut checker.ctx);

    // The pre-populated DefId should exist and have 3 stub type params.
    let def_id = checker
        .ctx
        .get_existing_def_id(sym_id)
        .expect("Triple should be pre-populated");
    let info = checker
        .ctx
        .definition_store
        .get(def_id)
        .expect("DefId should exist in store");
    assert_eq!(
        info.type_params.len(),
        3,
        "Generic interface with 3 type params should have 3 stub TypeParamInfo entries"
    );
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
