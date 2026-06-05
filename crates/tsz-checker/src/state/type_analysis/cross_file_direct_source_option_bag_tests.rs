//! Tests for decoupling source-file option-bag direct lowering from
//! shared symbol-arena cache eligibility (issue #7531).
//!
//! Structural rule: when a cross-file source-file interface is an unmerged
//! option-bag shape (only property signatures with scope-independent or
//! direct-lowerable-sibling annotations), tsz can lower it directly via
//! `delegate_cross_arena_symbol_resolution` without constructing a child
//! checker, even when the shared symbol-arena cache is ineligible.
//!
//! Cache ineligibility happens on two hot project-row paths: the cross-file
//! delegation path (`needs_cross_file_delegation = true`), where
//! `symbol_type_cache_from_symbol_arena` is `false` by definition; and
//! programs with any module augmentation, where the shared-cache gate is
//! disabled program-wide.
//!
//! The structural option-bag guard inside `direct_cross_file_interface_lowering`
//! remains the safety gate, so only same-file, non-generic option-bag heritage
//! is admitted; computed/complex shapes still fall back to the child-checker
//! path.

use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::module_resolution::build_module_resolution_maps;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
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

/// Build a checker whose `requester` arena has no local declaration of
/// `symbol_name`, with `symbol_name` owned by the `target` file through the
/// cross-file symbol-file index. This forces `needs_cross_file_delegation`,
/// the path where `symbol_type_cache_from_symbol_arena` is always `false`.
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

fn with_program_state_with_libs<F, R>(
    files: &[(&str, &str)],
    requester_file: &str,
    target_file: &str,
    libs: &[&str],
    test: F,
) -> R
where
    F: FnOnce(&mut CheckerState<'_>, &Arc<BinderState>, usize) -> R,
{
    let lib_files = load_lib_files(libs);
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut file_names = Vec::with_capacity(files.len());
    let mut types = None;
    for (file_name, source) in files {
        let (arena, binder, file_types) = parse_bound_source_with_name(file_name, source);
        arenas.push(arena);
        binders.push(binder);
        file_names.push((*file_name).to_string());
        if types.is_none() {
            types = Some(file_types);
        }
    }
    let requester_idx = file_names
        .iter()
        .position(|name| name == requester_file)
        .unwrap_or_else(|| panic!("requester_file {requester_file:?} not found"));
    let target_idx = file_names
        .iter()
        .position(|name| name == target_file)
        .unwrap_or_else(|| panic!("target_file {target_file:?} not found"));
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = types.unwrap_or_else(TypeInterner::new);
    let ctx = CheckerContext::new_with_shared_def_store(
        all_arenas[requester_idx].as_ref(),
        all_binders[requester_idx].as_ref(),
        &types,
        requester_file.to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    let mut state = CheckerState { ctx };
    state.ctx.share_owner_symbol_type_results = true;
    state.ctx.set_all_arenas(Arc::clone(&all_arenas));
    state.ctx.set_all_binders(Arc::clone(&all_binders));
    state.ctx.set_current_file_idx(requester_idx);
    state
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    state.ctx.set_resolved_modules(resolved_modules);
    let lib_contexts = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect::<Vec<_>>();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());
    test(&mut state, &all_binders[target_idx], target_idx)
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
fn delegate_cross_arena_source_option_bag_lowers_imported_return_type_members() {
    with_program_state_with_libs(
        &[
            (
                "metrics.ts",
                r#"
                    export interface DataPoint {
                        label: string;
                        value: number;
                    }
                    export function summarizeSeries(points: readonly DataPoint[]): {
                        count: number;
                        total: number;
                    } {
                        return { count: points.length, total: 0 };
                    }
                "#,
            ),
            (
                "view.ts",
                r#"
                    import { summarizeSeries, type DataPoint } from "./metrics";

                    export interface DashboardInput {
                        title: string;
                        logos: readonly string[];
                    }

                    export interface DashboardModel extends DashboardInput {
                        points: DataPoint[];
                        summary: ReturnType<typeof summarizeSeries>;
                    }
                "#,
            ),
            (
                "main.ts",
                r#"import { type DashboardModel } from "./view";"#,
            ),
        ],
        "main.ts",
        "view.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let model_sym = target_binder
                .file_locals
                .get("DashboardModel")
                .expect("DashboardModel symbol");
            state.ctx.register_symbol_file_index(model_sym, target_idx);

            enable_perf_counters_for_direct_lowering_test();
            let success_before =
                direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
            let child_checkers_before = with_parent_cache_constructed_count();
            let (ty, params) = state
                .delegate_cross_arena_symbol_resolution(model_sym)
                .expect("imported source-file option-bag members should lower directly");
            let success_after =
                direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
            let child_checkers_after = with_parent_cache_constructed_count();

            assert_eq!(
                success_after - success_before,
                1,
                "DashboardModel should hit direct cross-file interface lowering"
            );
            assert_eq!(
                child_checkers_after, child_checkers_before,
                "imported option-bag members must not construct a delegated child checker"
            );
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "DashboardModel should be non-generic");

            for name in ["title", "points", "summary"] {
                let atom = state.ctx.types.intern_string(name);
                assert!(
                    crate::query_boundaries::common::raw_property_type(
                        state.ctx.types.as_type_database(),
                        ty,
                        atom,
                    )
                    .is_some(),
                    "directly lowered DashboardModel should retain '{name}' property",
                );
            }
        },
    );
}

#[test]
fn delegate_cross_arena_source_option_bag_uses_shadowed_return_type_member() {
    with_program_state_with_libs(
        &[
            (
                "metrics.ts",
                r#"
                    export interface DataPoint {
                        value: number;
                    }
                    export function summarizeSeries(points: readonly DataPoint[]): {
                        count: number;
                    } {
                        return { count: points.length };
                    }
                "#,
            ),
            (
                "view.ts",
                r#"
                    import { summarizeSeries, type DataPoint } from "./metrics";

                    type ReturnType<T> = { shadow: T };

                    export interface DashboardModel {
                        points: DataPoint[];
                        summary: ReturnType<typeof summarizeSeries>;
                    }
                "#,
            ),
            (
                "main.ts",
                r#"import { type DashboardModel } from "./view";"#,
            ),
        ],
        "main.ts",
        "view.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let model_sym = target_binder
                .file_locals
                .get("DashboardModel")
                .expect("DashboardModel symbol");
            state.ctx.register_symbol_file_index(model_sym, target_idx);

            let local_return_type_sym = target_binder
                .file_locals
                .get("ReturnType")
                .expect("local ReturnType symbol");
            let local_return_type_def = state.ctx.get_or_create_def_id(local_return_type_sym);

            enable_perf_counters_for_direct_lowering_test();
            let success_before =
                direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
            let (ty, _params) = state
                .delegate_cross_arena_symbol_resolution(model_sym)
                .expect("shadowed ReturnType source-file option-bag should resolve");
            let success_after =
                direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);

            assert_eq!(
                success_after - success_before,
                1,
                "lowerable local ReturnType shadow should stay on the direct interface path"
            );
            let summary = state.ctx.types.intern_string("summary");
            let summary_type = crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                ty,
                summary,
            )
            .expect("directly lowered DashboardModel should retain summary");
            let (base, _args) = crate::query_boundaries::common::application_info(
                state.ctx.types.as_type_database(),
                summary_type,
            )
            .expect("summary should be an application of the local ReturnType alias");
            assert_eq!(
                crate::query_boundaries::common::lazy_def_id(
                    state.ctx.types.as_type_database(),
                    base,
                ),
                Some(local_return_type_def),
                "same-named local ReturnType alias must shadow the actual-lib utility"
            );
        },
    );
}

/// Root cause #1: the cross-file delegation path always sees
/// `symbol_type_cache_from_symbol_arena == false`, so the option-bag fast path
/// was never eligible. After decoupling, a scope-independent option-bag
/// interface lowers directly without a child checker.
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

/// Same cross-file delegation path, but the option-bag references a same-file
/// sibling alias. The sibling must resolve through the delegate binder's lazy
/// `DefId`, still without a child checker.
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
        "direct option-bag lowering with sibling aliases must not construct a delegated child checker"
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
    // The sibling annotation must resolve through the delegate file's binder,
    // i.e. its lazy DefId points at the target file's `Priority`, not at a
    // same-spelled symbol in the requester file.
    let target_priority_sym = target_binder
        .file_locals
        .get("Priority")
        .expect("Priority sibling symbol in delegate file");
    let target_priority_def = state.ctx.get_or_create_def_id(target_priority_sym);
    assert_eq!(
        crate::query_boundaries::common::lazy_def_id(
            state.ctx.types.as_type_database(),
            priority_type,
        ),
        Some(target_priority_def),
        "the 'priority' annotation should resolve to the delegate file's Priority sibling",
    );
}

#[test]
fn delegate_cross_arena_source_option_bag_interface_type_uses_target_alias_with_requester_shadow() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "route.ts",
        r#"
                type ReturnType = "ok" | "redirect";
                export interface RouteConfig {
                    result: ReturnType;
                    route: "/dashboard" | "/settings";
                }
            "#,
    );
    let (requester_arena, requester_binder, _) = parse_bound_source_with_name(
        "app.ts",
        r#"
                type ReturnType = string;
            "#,
    );

    let (mut state, route_sym) = setup_cross_file_index_state(
        "RouteConfig",
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
    let ty = state
        .delegate_cross_arena_interface_type(route_sym)
        .expect("cross-file source-file option-bag interface type should lower directly");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after - success_before,
        1,
        "RouteConfig should hit direct cross-file interface lowering through delegate_cross_arena_interface_type"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "direct interface-type lowering must not construct a delegated child checker"
    );

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    let result = state.ctx.types.intern_string("result");
    let result_type = crate::query_boundaries::common::raw_property_type(
        state.ctx.types.as_type_database(),
        ty,
        result,
    )
    .expect("directly lowered RouteConfig should retain 'result' property");
    let target_return_sym = target_binder
        .file_locals
        .get("ReturnType")
        .expect("target ReturnType sibling symbol");
    let target_return_def = state.ctx.get_or_create_def_id(target_return_sym);
    assert_eq!(
        crate::query_boundaries::common::lazy_def_id(
            state.ctx.types.as_type_database(),
            result_type,
        ),
        Some(target_return_def),
        "the 'result' annotation should resolve to route.ts ReturnType, not the requester shadow",
    );
}

/// Root cause #2: when the program has any module augmentation, the shared
/// symbol-arena cache gate is disabled program-wide, so
/// `symbol_type_cache_from_symbol_arena` is `false` even on the symbol-arena
/// delegation path. The option-bag interface must still lower directly.
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
fn delegate_cross_arena_source_interface_with_simple_heritage_lowers_directly() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "options.ts",
        r#"
                export interface BaseOptions {
                    enabled: boolean;
                }
                export interface BuildOptions extends BaseOptions {
                    timeout: number;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("consumer.ts", "// imports BuildOptions from options");

    let (mut state, build_sym) = setup_cross_file_index_state(
        "BuildOptions",
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
        .delegate_cross_arena_symbol_resolution(build_sym)
        .expect("simple source-file heritage should lower directly");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let complex_after = direct_interface_lowering_count(
        DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration,
    );
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after - success_before,
        1,
        "same-file option-bag heritage should be admitted to direct lowering"
    );
    assert_eq!(
        complex_after, complex_before,
        "simple option-bag heritage should not be recorded as complex"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "simple source-file heritage should avoid delegated child-checker resolution"
    );

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "BuildOptions should be non-generic");
    let timeout = state.ctx.types.intern_string("timeout");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            timeout,
        )
        .is_some(),
        "direct-lowered BuildOptions should retain its own property",
    );
    let enabled = state.ctx.types.intern_string("enabled");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            enabled,
        )
        .is_some(),
        "direct-lowered BuildOptions should retain inherited properties",
    );
}

#[test]
fn delegate_cross_arena_source_interface_with_renamed_simple_heritage_lowers_directly() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "shapes.ts",
        r#"
                export interface SeedState {
                    ready: boolean;
                }
                export interface RenderPlan extends SeedState {
                    label: string;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("consumer.ts", "// imports RenderPlan from shapes");

    let (mut state, render_sym) = setup_cross_file_index_state(
        "RenderPlan",
        &types,
        &requester_arena,
        &requester_binder,
        &target_arena,
        &target_binder,
    );

    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(render_sym)
        .expect("renamed same-file option-bag heritage should lower directly");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "RenderPlan should be non-generic");
    let ready = state.ctx.types.intern_string("ready");
    let label = state.ctx.types.intern_string("label");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            ready,
        )
        .is_some(),
        "inherited property should survive direct lowering",
    );
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            label,
        )
        .is_some(),
        "own property should survive direct lowering",
    );
}

#[test]
fn delegate_cross_arena_interface_type_allows_source_option_bag_direct_lowering() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "points.ts",
        r#"
                export interface SamplePoint {
                    label: string;
                    value: number;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("consumer.ts", "// imports SamplePoint from points");

    let (mut state, point_sym) = setup_cross_file_index_state(
        "SamplePoint",
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
    let ty = state
        .delegate_cross_arena_interface_type(point_sym)
        .expect("source-file option-bag interface type should lower directly");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after - success_before,
        1,
        "source-file option-bag interface type should use direct lowering"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "source-file option-bag interface type should not construct a child checker"
    );
    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    let label = state.ctx.types.intern_string("label");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            label,
        )
        .is_some(),
        "direct-lowered SamplePoint should retain its label property",
    );
}

#[test]
fn delegate_cross_arena_source_interface_with_generic_heritage_still_falls_back() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "generic.ts",
        r#"
                export interface Box<T> {
                    value: T;
                }
                export interface WrappedOptions extends Box<string> {
                    label: string;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source_with_name("consumer.ts", "// imports WrappedOptions from generic");

    let (mut state, wrapped_sym) = setup_cross_file_index_state(
        "WrappedOptions",
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
        .delegate_cross_arena_symbol_resolution(wrapped_sym)
        .expect("generic source-file heritage should still delegate through fallback");
    let success_after =
        direct_interface_lowering_count(DirectCrossFileInterfaceLoweringOutcome::Success);
    let complex_after = direct_interface_lowering_count(
        DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration,
    );
    let child_checkers_after = with_parent_cache_constructed_count();

    assert_eq!(
        success_after, success_before,
        "generic source-file heritage must not be admitted to direct lowering"
    );
    assert_eq!(
        complex_after - complex_before,
        1,
        "generic heritage should be rejected by the structural direct-lowering guard"
    );
    assert_eq!(
        child_checkers_after - child_checkers_before,
        1,
        "generic source-file heritage should fall back to delegated child-checker resolution"
    );

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "WrappedOptions should be non-generic");
    let value = state.ctx.types.intern_string("value");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            value,
        )
        .is_some(),
        "fallback-lowered WrappedOptions should retain inherited generic properties",
    );
}
