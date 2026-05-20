use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::{BinderState, ModuleAugmentation};
use tsz_common::perf_counters::{DirectCrossFileInterfaceLoweringOutcome, PerfCounters};
use tsz_parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;
use tsz_solver::def::DefinitionStore;

fn parse_bound_source_with_name(
    file_name: &str,
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (
        Arc::new(parser.get_arena().clone()),
        Arc::new(binder),
        TypeInterner::new(),
    )
}

fn setup_cross_file_index_state<'a>(
    symbol_name: &str,
    types: &'a TypeInterner,
    requester_arena: &'a Arc<tsz_parser::parser::node::NodeArena>,
    requester_binder: &'a Arc<BinderState>,
    target_arena: &Arc<tsz_parser::parser::node::NodeArena>,
    target_binder: &Arc<BinderState>,
) -> (CheckerState<'a>, tsz_binder::SymbolId) {
    let sym = target_binder
        .file_locals
        .get(symbol_name)
        .unwrap_or_else(|| panic!("{symbol_name} symbol not found in target binder"));

    let requester_file_name = requester_arena
        .source_files
        .first()
        .expect("requester arena has source file")
        .file_name
        .clone();
    let mut ctx = CheckerContext::new_with_shared_def_store(
        requester_arena.as_ref(),
        requester_binder.as_ref(),
        types,
        requester_file_name,
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    ctx.share_owner_symbol_type_results = true;
    ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(requester_arena),
        Arc::clone(target_arena),
    ]));
    ctx.set_all_binders(Arc::new(vec![
        Arc::clone(requester_binder),
        Arc::clone(target_binder),
    ]));
    let state = CheckerState { ctx };

    let target_file_idx = state
        .ctx
        .get_file_idx_for_arena(target_arena.as_ref())
        .expect("target arena should be indexed");
    state.ctx.register_symbol_file_index(sym, target_file_idx);
    (state, sym)
}

fn enable_perf_counters_for_direct_lowering_test() {
    #[cfg(any(test, debug_assertions))]
    tsz_common::perf_counters::force_enable_perf_counters_for_tests();
    assert!(
        tsz_common::perf_counters::enabled_fast(),
        "direct-lowering branch tests need perf counters enabled"
    );
}

fn direct_interface_lowering_count(outcome: DirectCrossFileInterfaceLoweringOutcome) -> u64 {
    PerfCounters::snapshot().direct_interface_lowering_outcomes[outcome.as_index()].count
}

fn with_parent_cache_constructed_count() -> u64 {
    PerfCounters::snapshot()
        .checker
        .with_parent_cache_constructed
}

#[test]
fn delegate_cross_arena_source_option_bag_lowers_directly_via_cross_file_index() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "config.ts",
        r#"
                export interface PluginConfig {
                    enabled: boolean;
                    timeout: number;
                    tag: "fast" | "slow";
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("app.ts", "// imports PluginConfig from config");

    let (mut state, plugin_sym) = setup_cross_file_index_state(
        "PluginConfig",
        &types,
        &requester_arena,
        &requester_binder,
        &target_arena,
        &target_binder,
    );

    enable_perf_counters_for_direct_lowering_test();
    let success_before =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_before = with_parent_cache_constructed_count();
    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(plugin_sym)
        .expect("cross-file source-file option-bag interface should delegate successfully");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after - success_before,
        1,
        "PluginConfig should hit direct cross-file interface lowering"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "direct source-file option-bag lowering must not construct a delegated child checker"
    );

    assert_ne!(
        ty,
        TypeId::UNKNOWN,
        "PluginConfig must not lower to UNKNOWN"
    );
    assert_ne!(ty, TypeId::ERROR, "PluginConfig must not lower to ERROR");
    assert!(params.is_empty(), "PluginConfig should be non-generic");
    let enabled = state.ctx.types.intern_string("enabled");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            enabled,
        )
        .is_some(),
        "directly lowered PluginConfig should retain 'enabled' property",
    );
}

#[test]
fn delegate_cross_arena_source_option_bag_with_sibling_alias_lowers_directly_via_cross_file_index()
{
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "task.ts",
        r#"
                type Priority = "high" | "low" | "none";
                export interface WorkItem {
                    priority: Priority;
                    retries: number;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("runner.ts", "// imports WorkItem from task");

    let (mut state, work_item_sym) = setup_cross_file_index_state(
        "WorkItem",
        &types,
        &requester_arena,
        &requester_binder,
        &target_arena,
        &target_binder,
    );

    enable_perf_counters_for_direct_lowering_test();
    let success_before =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_before = with_parent_cache_constructed_count();
    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(work_item_sym)
        .expect("cross-file option-bag with sibling alias should delegate successfully");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after - success_before,
        1,
        "WorkItem should hit direct cross-file interface lowering"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "direct source-file option-bag lowering with sibling aliases must not construct a delegated child checker"
    );

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "WorkItem should be non-generic");
    let priority = state.ctx.types.intern_string("priority");
    let priority_type = crate::query_boundaries::common::raw_property_type(
        state.ctx.types.as_type_database(),
        ty,
        priority,
    )
    .expect("directly lowered WorkItem should retain 'priority' property");
    let resolved_priority = state.resolve_lazy_type(priority_type);
    assert_ne!(
        resolved_priority,
        TypeId::UNKNOWN,
        "Priority sibling alias should resolve through lazy DefId"
    );
    assert_ne!(resolved_priority, TypeId::ERROR);
}

#[test]
fn delegate_cross_arena_source_option_bag_resolves_in_program_with_module_augmentations() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "options.ts",
        r#"
                export interface BuildOptions {
                    minify: boolean;
                    sourcemap: boolean;
                }
            "#,
    );
    let (requester_arena, mut requester_binder, _) =
        parse_bound_source_with_name("build.ts", "// imports BuildOptions from options");
    let build_opts_sym = target_binder
        .file_locals
        .get("BuildOptions")
        .expect("BuildOptions symbol");
    let build_opts_decl = target_binder
        .get_symbol(build_opts_sym)
        .expect("BuildOptions symbol data")
        .declarations[0];
    {
        let rb = Arc::make_mut(&mut requester_binder);
        Arc::make_mut(&mut rb.symbol_arenas).insert(build_opts_sym, Arc::clone(&target_arena));
        Arc::make_mut(&mut rb.declaration_arenas)
            .entry((build_opts_sym, build_opts_decl))
            .or_default()
            .push(Arc::clone(&target_arena));
        Arc::make_mut(&mut rb.module_augmentations).insert(
            "./other-module".to_string(),
            vec![ModuleAugmentation::new("x".to_string(), NodeIndex::NONE)],
        );
    }

    let mut ctx = CheckerContext::new_with_shared_def_store(
        requester_arena.as_ref(),
        requester_binder.as_ref(),
        &types,
        "build.ts".to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    ctx.share_owner_symbol_type_results = true;
    ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&requester_arena),
        Arc::clone(&target_arena),
    ]));
    ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&target_binder),
    ]));
    let mut state = CheckerState { ctx };
    assert!(
        state.ctx.program_has_module_augmentations(),
        "fixture should make the source-file symbol-arena cache ineligible"
    );

    enable_perf_counters_for_direct_lowering_test();
    let success_before =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_before = with_parent_cache_constructed_count();
    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(build_opts_sym)
        .expect(
            "source-file option-bag should delegate even when the program has module augmentations",
        );
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after - success_before,
        1,
        "BuildOptions should hit direct lowering even when module augmentations disable shared source-file symbol caching"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "module-augmentation source-file option-bag lowering must not construct a delegated child checker"
    );

    assert_ne!(
        ty,
        TypeId::UNKNOWN,
        "BuildOptions must not lower to UNKNOWN"
    );
    assert_ne!(ty, TypeId::ERROR, "BuildOptions must not lower to ERROR");
    assert!(params.is_empty(), "BuildOptions should be non-generic");
    let minify = state.ctx.types.intern_string("minify");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            minify,
        )
        .is_some(),
        "BuildOptions should retain 'minify' property even with program-level augmentations present",
    );
}

#[test]
fn delegate_cross_arena_source_interface_with_heritage_still_falls_back() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "complex.ts",
        r#"
                export interface BaseOptions {
                    enabled: boolean;
                }
                export interface ComplexOptions extends BaseOptions {
                    timeout: number;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("consumer.ts", "// imports ComplexOptions from complex");

    let (mut state, complex_sym) = setup_cross_file_index_state(
        "ComplexOptions",
        &types,
        &requester_arena,
        &requester_binder,
        &target_arena,
        &target_binder,
    );

    enable_perf_counters_for_direct_lowering_test();
    let success_before =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let complex_before = direct_interface_lowering_count(
        DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration,
    );
    let child_checkers_before = with_parent_cache_constructed_count();
    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(complex_sym)
        .expect("complex source-file interface should still delegate through fallback");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let complex_after = direct_interface_lowering_count(
        DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration,
    );
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after, success_before,
        "heritage-bearing source-file interfaces must not be admitted to direct lowering"
    );
    assert_eq!(
        complex_after - complex_before,
        1,
        "heritage-bearing source-file interfaces should be rejected by the structural direct-lowering guard"
    );
    assert_eq!(
        child_checkers_after - child_checkers_before,
        1,
        "complex source-file interfaces should fall back to delegated child-checker resolution"
    );

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "ComplexOptions should be non-generic");
    let timeout = state.ctx.types.intern_string("timeout");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            timeout,
        )
        .is_some(),
        "fallback-lowered ComplexOptions should retain its own property",
    );
}
