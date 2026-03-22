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
        global_symbol_file_index: None,
        global_file_locals_index: None,
        global_module_exports_index: None,
        global_module_augmentations_index: None,
        global_augmentation_targets_index: None,
        global_module_binder_index: None,
        global_arena_index: None,
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
            is_exported: false,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
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
            is_exported: false,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
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
            is_exported: false,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
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
            is_exported: false,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
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
fn apply_to_pre_populates_enum_member_names() {
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    // Create a binder with an enum that has member names.
    let mut other_binder = BinderState::new();
    let sym_id = SymbolId(50);
    other_binder.semantic_defs.insert(
        sym_id,
        SemanticDefEntry {
            kind: SemanticDefKind::Enum,
            name: "Direction".to_string(),
            file_id: 0,
            span_start: 0,
            type_param_count: 0,
            is_exported: false,
            enum_member_names: vec![
                "Up".to_string(),
                "Down".to_string(),
                "Left".to_string(),
                "Right".to_string(),
            ],
            is_const: true,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
        },
    );

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(other_binder)]);
    env.apply_to(&mut checker.ctx);

    // The pre-populated DefId should exist and have enum_members populated.
    let def_id = checker
        .ctx
        .get_existing_def_id(sym_id)
        .expect("Direction should be pre-populated");
    let info = checker
        .ctx
        .definition_store
        .get(def_id)
        .expect("DefId should exist in store");
    assert_eq!(
        info.enum_members.len(),
        4,
        "Enum with 4 members should have 4 pre-populated enum_members"
    );
    // Member names should be interned Atoms -- verify via DefinitionInfo
    let member_names: Vec<String> = info
        .enum_members
        .iter()
        .map(|(atom, _)| interner.resolve_atom(*atom))
        .collect();
    assert_eq!(
        member_names,
        vec!["Up", "Down", "Left", "Right"],
        "Enum member names should be preserved through pre-population"
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

#[test]
fn apply_to_pre_populates_def_ids_for_all_declaration_families() {
    // All seven declaration kinds captured by binder semantic_defs should
    // produce stable DefIds after apply_to. This is the key identity test:
    // no checker fallback should be needed for these symbols.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    let mut other_binder = BinderState::new();
    let kinds_and_ids = vec![
        (SemanticDefKind::Class, SymbolId(10), "MyClass"),
        (SemanticDefKind::Interface, SymbolId(11), "MyInterface"),
        (SemanticDefKind::TypeAlias, SymbolId(12), "MyType"),
        (SemanticDefKind::Enum, SymbolId(13), "MyEnum"),
        (SemanticDefKind::Namespace, SymbolId(14), "MyNS"),
        (SemanticDefKind::Function, SymbolId(15), "myFunc"),
        (SemanticDefKind::Variable, SymbolId(16), "myVar"),
    ];
    for (kind, sym_id, name) in &kinds_and_ids {
        other_binder.semantic_defs.insert(
            *sym_id,
            SemanticDefEntry {
                kind: *kind,
                name: name.to_string(),
                file_id: 0,
                span_start: 0,
                type_param_count: 0,
                is_exported: true,
                enum_member_names: Vec::new(),
                is_const: false,
                is_abstract: false,
                extends_names: Vec::new(),
                implements_names: Vec::new(),
            },
        );
    }

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(other_binder)]);
    env.apply_to(&mut checker.ctx);

    // All seven should be pre-populated — no fallback needed
    for (_, sym_id, name) in &kinds_and_ids {
        let def_id = checker.ctx.get_existing_def_id(*sym_id);
        assert!(
            def_id.is_some(),
            "DefId for {} (SymbolId({})) should be pre-populated",
            name,
            sym_id.0
        );
    }
    // Fallback counter should be zero
    assert_eq!(
        checker.ctx.def_fallback_count.get(),
        0,
        "No checker fallback should fire for pre-populated symbols"
    );
}

#[test]
fn pre_populated_def_ids_survive_multi_binder_merge() {
    // When the same declaration name appears in two binders (declaration
    // merging), the first binder's DefId should win and the second should
    // find the existing entry, not create a duplicate.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let mut checker = make_checker(&arena, &binder, &query_cache);

    // Two binders with the same SymbolId for "Shared" (simulates
    // declaration merging where the same global SymbolId is used).
    let shared_sym = SymbolId(42);

    let mut binder1 = BinderState::new();
    binder1.semantic_defs.insert(
        shared_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Shared".to_string(),
            file_id: 0,
            span_start: 10,
            type_param_count: 1,
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
        },
    );

    let mut binder2 = BinderState::new();
    binder2.semantic_defs.insert(
        shared_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Shared".to_string(),
            file_id: 1,
            span_start: 20,
            type_param_count: 1,
            is_exported: false,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
        },
    );

    let mut env = empty_project_env();
    env.all_binders = Arc::new(vec![Arc::new(binder1), Arc::new(binder2)]);
    env.apply_to(&mut checker.ctx);

    // Should have exactly one DefId for the shared symbol
    let def_id = checker
        .ctx
        .get_existing_def_id(shared_sym)
        .expect("Shared should have a DefId");

    // The DefId should exist in the definition store
    let info = checker
        .ctx
        .definition_store
        .get(def_id)
        .expect("DefId should be in store");
    assert_eq!(interner.resolve_atom(info.name), "Shared");
    assert_eq!(
        info.type_params.len(),
        1,
        "type_param_count should be preserved"
    );
}

#[test]
fn cross_batch_heritage_resolves_extends_across_binders() {
    // When a class in one binder extends a class in another binder,
    // the cross-batch heritage resolution should link them via DefId.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();

    // Set up the primary binder with a class that extends "Base"
    let mut binder = BinderState::new();
    let child_sym = SymbolId(10);
    binder.semantic_defs.insert(
        child_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Class,
            name: "Child".to_string(),
            file_id: 0,
            span_start: 0,
            type_param_count: 0,
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: vec!["Base".to_string()],
            implements_names: Vec::new(),
        },
    );

    let mut checker = make_checker(&arena, &binder, &query_cache);

    // Pre-populate from the primary binder first (no "Base" yet)
    let count1 = checker.ctx.pre_populate_def_ids_from_binder();
    assert_eq!(count1, 1, "Should pre-populate Child");

    // Create a second binder with "Base" and set it as all_binders
    let mut base_binder = BinderState::new();
    let base_sym = SymbolId(20);
    base_binder.semantic_defs.insert(
        base_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Class,
            name: "Base".to_string(),
            file_id: 1,
            span_start: 0,
            type_param_count: 0,
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
        },
    );
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::new(base_binder)]));
    let count2 = checker.ctx.pre_populate_def_ids_from_all_binders();
    assert_eq!(count2, 1, "Should pre-populate Base");

    // Now resolve cross-batch heritage
    let resolved = checker.ctx.resolve_cross_batch_heritage();
    assert!(
        resolved >= 1,
        "Should resolve at least 1 cross-batch heritage link (Child extends Base)"
    );

    // Verify the heritage link
    let child_def = checker
        .ctx
        .get_existing_def_id(child_sym)
        .expect("Child should have DefId");
    let child_info = checker
        .ctx
        .definition_store
        .get(child_def)
        .expect("Child should be in store");
    assert!(
        child_info.extends.is_some(),
        "Child should have extends set after cross-batch resolution"
    );

    let base_def = checker
        .ctx
        .get_existing_def_id(base_sym)
        .expect("Base should have DefId");
    assert_eq!(
        child_info.extends,
        Some(base_def),
        "Child.extends should point to Base's DefId"
    );
}

#[test]
fn cross_batch_heritage_resolves_implements_across_binders() {
    // Class implementing an interface from another binder.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();

    let mut binder = BinderState::new();
    let cls_sym = SymbolId(30);
    binder.semantic_defs.insert(
        cls_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Class,
            name: "Widget".to_string(),
            file_id: 0,
            span_start: 0,
            type_param_count: 0,
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: vec!["Renderable".to_string(), "Serializable".to_string()],
        },
    );

    let mut checker = make_checker(&arena, &binder, &query_cache);
    checker.ctx.pre_populate_def_ids_from_binder();

    // Create binder with interfaces
    let mut iface_binder = BinderState::new();
    let renderable_sym = SymbolId(40);
    let serializable_sym = SymbolId(41);
    iface_binder.semantic_defs.insert(
        renderable_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Renderable".to_string(),
            file_id: 1,
            span_start: 0,
            type_param_count: 0,
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
        },
    );
    iface_binder.semantic_defs.insert(
        serializable_sym,
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Serializable".to_string(),
            file_id: 1,
            span_start: 50,
            type_param_count: 0,
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
        },
    );

    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::new(iface_binder)]));
    checker.ctx.pre_populate_def_ids_from_all_binders();

    let resolved = checker.ctx.resolve_cross_batch_heritage();
    assert!(resolved >= 1, "Should resolve implements heritage");

    let cls_def = checker.ctx.get_existing_def_id(cls_sym).unwrap();
    let cls_info = checker.ctx.definition_store.get(cls_def).unwrap();
    assert_eq!(
        cls_info.implements.len(),
        2,
        "Widget should implement 2 interfaces"
    );
}

#[test]
fn stable_identity_survives_merge_rebind_for_all_families() {
    // Pre-populate DefIds for all 7 declaration kinds, then verify
    // the identity survives a simulated merge/rebind by re-populating
    // from a second batch and checking DefIds are stable.
    let interner = TypeInterner::new();
    let query_cache = QueryCache::new(&interner);
    let arena = NodeArena::new();

    let entries = vec![
        (SemanticDefKind::Class, SymbolId(100), "MyClass"),
        (SemanticDefKind::Interface, SymbolId(101), "MyInterface"),
        (SemanticDefKind::TypeAlias, SymbolId(102), "MyType"),
        (SemanticDefKind::Enum, SymbolId(103), "MyEnum"),
        (SemanticDefKind::Namespace, SymbolId(104), "MyNS"),
        (SemanticDefKind::Function, SymbolId(105), "myFunc"),
        (SemanticDefKind::Variable, SymbolId(106), "myVar"),
    ];

    let mut binder = BinderState::new();
    for &(kind, sym_id, name) in &entries {
        binder.semantic_defs.insert(
            sym_id,
            SemanticDefEntry {
                kind,
                name: name.to_string(),
                file_id: 0,
                span_start: sym_id.0 * 10,
                type_param_count: 0,
                is_exported: true,
                enum_member_names: Vec::new(),
                is_const: false,
                is_abstract: false,
                extends_names: Vec::new(),
                implements_names: Vec::new(),
            },
        );
    }

    let mut checker = make_checker(&arena, &binder, &query_cache);
    let count = checker.ctx.pre_populate_def_ids_from_binder();
    assert_eq!(count, 7, "All 7 families should be pre-populated");

    // Record DefIds from first population
    let first_def_ids: Vec<_> = entries
        .iter()
        .map(|&(_, sym_id, _)| {
            checker
                .ctx
                .get_existing_def_id(sym_id)
                .expect("Should have DefId")
        })
        .collect();

    // Simulate merge/rebind: re-populate from same semantic_defs
    // (same SymbolIds). DefIds should be stable (same values).
    let count2 = checker.ctx.pre_populate_def_ids_from_binder();
    assert_eq!(
        count2, 0,
        "Re-population should skip already-registered entries"
    );

    for (i, &(_, sym_id, _)) in entries.iter().enumerate() {
        let def_id = checker
            .ctx
            .get_existing_def_id(sym_id)
            .expect("Should still have DefId");
        assert_eq!(
            def_id, first_def_ids[i],
            "DefId should be stable across re-population"
        );
    }

    // Verify all DefIds have correct kinds in the DefinitionStore
    for (i, &(kind, _, _)) in entries.iter().enumerate() {
        let info = checker
            .ctx
            .definition_store
            .get(first_def_ids[i])
            .expect("DefId should be in store");
        let expected_kind = match kind {
            SemanticDefKind::Class => tsz_solver::def::DefKind::Class,
            SemanticDefKind::Interface => tsz_solver::def::DefKind::Interface,
            SemanticDefKind::TypeAlias => tsz_solver::def::DefKind::TypeAlias,
            SemanticDefKind::Enum => tsz_solver::def::DefKind::Enum,
            SemanticDefKind::Namespace => tsz_solver::def::DefKind::Namespace,
            SemanticDefKind::Function => tsz_solver::def::DefKind::Function,
            SemanticDefKind::Variable => tsz_solver::def::DefKind::Variable,
        };
        assert_eq!(
            info.kind, expected_kind,
            "DefKind should match SemanticDefKind"
        );
    }
}

#[test]
fn name_index_enables_cross_batch_lookup() {
    // Verify the DefinitionStore's name-based index works correctly
    // for finding definitions by name across pre-population batches.
    use tsz_solver::def::DefinitionStore;

    let store = DefinitionStore::new();
    let interner = TypeInterner::new();

    let error_name = interner.intern_string("Error");
    let info = tsz_solver::def::DefinitionInfo {
        kind: tsz_solver::def::DefKind::Interface,
        name: error_name,
        type_params: Vec::new(),
        body: None,
        instance_shape: None,
        static_shape: None,
        extends: None,
        implements: Vec::new(),
        enum_members: Vec::new(),
        exports: Vec::new(),
        file_id: Some(0),
        span: None,
        symbol_id: Some(1),
    };

    let def_id = store.register(info);

    // Should be findable by name
    let found = store.find_def_by_name(error_name);
    assert_eq!(found, Some(def_id), "Should find Error by name");

    // Unknown name should return None
    let unknown = interner.intern_string("Nonexistent");
    assert_eq!(
        store.find_def_by_name(unknown),
        None,
        "Unknown name should return None"
    );
}
