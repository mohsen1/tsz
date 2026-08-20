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
//! remains the safety gate, so heritage/computed/complex shapes still fall back
//! to the child-checker path.

use crate::context::{CheckerContext, CheckerOptions, LibContext};
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

/// Give the calling test a private, thread-scoped counter set instead of the
/// process-wide atomics: `direct_interface_lowering_count` /
/// `with_parent_cache_constructed_count` below take before/after snapshots
/// and diff them, and a plain before/after delta on process-wide counters is
/// immune to increments made before the window but not to increments a
/// sibling thread makes *inside* it under a shared-process runner (`cargo
/// test`, not `nextest`). Keep the returned guard alive for the rest of the
/// test.
#[must_use]
fn scoped_perf_counters_for_direct_lowering_test() -> tsz_common::perf_counters::ScopedPerfCounters
{
    let scope = tsz_common::perf_counters::ScopedPerfCounters::new();
    assert!(
        tsz_common::perf_counters::enabled_fast(),
        "direct-lowering branch tests need perf counters enabled"
    );
    scope
}

fn direct_interface_lowering_count(outcome: DirectCrossFileInterfaceLoweringOutcome) -> u64 {
    PerfCounters::snapshot().direct_interface_lowering_outcomes[outcome.as_index()].count
}

fn with_parent_cache_constructed_count() -> u64 {
    PerfCounters::snapshot()
        .checker
        .with_parent_cache_constructed
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

    let _scope = scoped_perf_counters_for_direct_lowering_test();
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

    let _scope = scoped_perf_counters_for_direct_lowering_test();
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

    let _scope = scoped_perf_counters_for_direct_lowering_test();
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
fn delegate_cross_arena_source_module_augmentation_member_substitutes_generic_interface_ref() {
    let (registry_arena, registry_binder, types) = parse_bound_source_with_name(
        "HKT.ts",
        r#"
                export interface URItoKind2<E, A> {}
            "#,
    );
    let (augmentation_arena, augmentation_binder, _) = parse_bound_source_with_name(
        "io-either.ts",
        r#"
                import { URItoKind2 } from "./HKT";
                declare module "./HKT" {
                    interface URItoKind2<E, A> {
                        readonly IOEither: IOEither<E, A>;
                    }
                }
                export interface IOEither<E, A> { readonly value: E | A; }
            "#,
    );
    let mut ctx = CheckerContext::new_with_shared_def_store(
        registry_arena.as_ref(),
        registry_binder.as_ref(),
        &types,
        "HKT.ts".to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&registry_arena),
        Arc::clone(&augmentation_arena),
    ]));
    ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&registry_binder),
        Arc::clone(&augmentation_binder),
    ]));
    ctx.set_current_file_idx(0);
    let mut state = CheckerState { ctx };

    let augmentation = augmentation_binder
        .module_augmentations
        .get("./HKT")
        .and_then(|augmentations| {
            augmentations
                .iter()
                .find(|augmentation| augmentation.name == "URItoKind2")
        })
        .expect("URItoKind2 module augmentation");
    let registry_decl = augmentation.node;
    let registry_interface = augmentation_arena
        .get(registry_decl)
        .and_then(|node| augmentation_arena.get_interface(node))
        .expect("URItoKind2 augmentation interface");
    let io_member = registry_interface.members.nodes[0];
    let type_args = [TypeId::STRING, TypeId::NUMBER];

    let member_types = state
        .delegate_cross_arena_interface_member_simple_types(
            registry_decl,
            &[io_member],
            augmentation_arena.as_ref(),
            Some(&type_args),
            true,
        )
        .expect("source-file module augmentation member should lower directly");
    let member_type = member_types
        .get(&io_member)
        .copied()
        .expect("IOEither member type");

    assert_ne!(
        member_type,
        TypeId::ANY,
        "source-file module augmentation member must not fall back to ANY"
    );
    assert_ne!(member_type, TypeId::UNKNOWN);
    assert_ne!(member_type, TypeId::ERROR);
}

#[test]
fn delegate_cross_arena_source_module_augmentation_member_lowers_global_map_ref() {
    let (registry_arena, registry_binder, types) = parse_bound_source_with_name(
        "HKT.ts",
        r#"
                export interface URItoKind2<E, A> {}
            "#,
    );
    let (augmentation_arena, augmentation_binder, _) = parse_bound_source_with_name(
        "table.ts",
        r#"
                import { URItoKind2 } from "./HKT";
                declare module "./HKT" {
                    interface URItoKind2<E, A> {
                        readonly Table: ReadonlyMap<E, A>;
                    }
                }
            "#,
    );
    let lib_files = load_lib_files(&["es5.d.ts", "es2015.collection.d.ts"]);
    let mut ctx = CheckerContext::new_with_shared_def_store(
        registry_arena.as_ref(),
        registry_binder.as_ref(),
        &types,
        "HKT.ts".to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&registry_arena),
        Arc::clone(&augmentation_arena),
    ]));
    ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&registry_binder),
        Arc::clone(&augmentation_binder),
    ]));
    ctx.set_current_file_idx(0);
    ctx.set_lib_contexts(
        lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect(),
    );
    ctx.set_actual_lib_file_count(lib_files.len());
    let mut state = CheckerState { ctx };

    let augmentation = augmentation_binder
        .module_augmentations
        .get("./HKT")
        .and_then(|augmentations| {
            augmentations
                .iter()
                .find(|augmentation| augmentation.name == "URItoKind2")
        })
        .expect("URItoKind2 module augmentation");
    let registry_decl = augmentation.node;
    let registry_interface = augmentation_arena
        .get(registry_decl)
        .and_then(|node| augmentation_arena.get_interface(node))
        .expect("URItoKind2 augmentation interface");
    let table_member = registry_interface.members.nodes[0];
    let type_args = [TypeId::STRING, TypeId::NUMBER];

    let member_types = state
        .delegate_cross_arena_interface_member_simple_types(
            registry_decl,
            &[table_member],
            augmentation_arena.as_ref(),
            Some(&type_args),
            true,
        )
        .expect("source-file module augmentation member with global map ref should lower directly");
    let member_type = member_types
        .get(&table_member)
        .copied()
        .expect("Table member type");

    assert_ne!(
        member_type,
        TypeId::ANY,
        "global `ReadonlyMap` member must not fall back to ANY"
    );
    assert_ne!(member_type, TypeId::UNKNOWN);
    assert_ne!(member_type, TypeId::ERROR);
    assert!(
        crate::query_boundaries::common::application_info(
            state.ctx.types.as_type_database(),
            member_type,
        )
        .is_some(),
        "global `ReadonlyMap<E, A>` should lower as an application"
    );
}

/// Safety gate: a source-file interface with heritage is not an option-bag
/// shape, so the structural guard rejects it and it still falls back to the
/// child-checker path. Decoupling must not broaden the admitted shape.
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

    let _scope = scoped_perf_counters_for_direct_lowering_test();
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

    // Contract (re-asserted 2026-07-13, was a stale perf-counter guard): the
    // direct-lowering guard now ADMITS source-file heritage interfaces with
    // no computed member names (`source_file_expand_direct_lowerable_
    // interface_heritage`), so heritage-bearing `ComplexOptions` lowers
    // directly — no child checker is constructed. Observable behavior is
    // tsc-identical either way (byte-identical TS2741 for a missing
    // inherited member via the CLI); this locks the direct-path admission
    // so it does not silently regress to the expensive delegation.
    assert_eq!(
        success_after - success_before,
        1,
        "no-computed-name heritage source-file interfaces are admitted to direct lowering"
    );
    assert_eq!(
        complex_after, complex_before,
        "the structural guard must not classify plain extends-heritage as complex"
    );
    assert_eq!(
        child_checkers_after, child_checkers_before,
        "direct lowering must not construct a delegated child checker"
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
