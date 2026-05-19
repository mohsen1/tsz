use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

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

fn set_two_file_import_context(
    state: &mut CheckerState<'_>,
    producer_arena: &Arc<tsz_parser::parser::node::NodeArena>,
    producer_binder: &Arc<BinderState>,
    consumer_arena: &Arc<tsz_parser::parser::node::NodeArena>,
    consumer_binder: &Arc<BinderState>,
) {
    state.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(producer_arena),
        Arc::clone(consumer_arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(producer_binder),
        Arc::clone(consumer_binder),
    ]));
    state.ctx.set_current_file_idx(1);
    let mut resolved_module_paths = FxHashMap::default();
    resolved_module_paths.insert((1, "./metrics".to_string()), 0);
    state
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
}

#[test]
fn direct_cross_file_interface_lowering_accepts_imported_source_member_refs() {
    let (metrics_arena, metrics_binder, types) = parse_bound_source_with_name(
        "metrics.ts",
        r#"
                export interface DataPoint {
                    label: string;
                    value: number;
                }
            "#,
    );
    let (view_arena, view_binder, _) = parse_bound_source_with_name(
        "view.ts",
        r#"
                import type { DataPoint as Point } from "./metrics";
                export interface Model {
                    points: Point[];
                }
            "#,
    );
    let ctx = CheckerContext::new(
        view_arena.as_ref(),
        view_binder.as_ref(),
        &types,
        "view.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    set_two_file_import_context(
        &mut state,
        &metrics_arena,
        &metrics_binder,
        &view_arena,
        &view_binder,
    );

    let model_sym = view_binder.file_locals.get("Model").expect("Model symbol");
    let (model_type, params) = state
        .direct_cross_file_interface_lowering(
            model_sym,
            view_binder.as_ref(),
            view_arena.as_ref(),
            false,
            true,
        )
        .expect("source interface with direct-lowerable imported member should lower");

    assert!(params.is_empty());
    let points = types.intern_string("points");
    let points_type = crate::query_boundaries::common::raw_property_type(
        state.ctx.types.as_type_database(),
        model_type,
        points,
    )
    .expect("points property should lower");
    let element_type = crate::query_boundaries::common::array_element_type(
        state.ctx.types.as_type_database(),
        points_type,
    )
    .expect("points should lower as an array");
    let data_point_sym = metrics_binder
        .file_locals
        .get("DataPoint")
        .expect("DataPoint symbol");
    let data_point_def = state
        .ctx
        .get_existing_def_id(data_point_sym)
        .expect("imported DataPoint should be registered");

    assert_eq!(
        crate::query_boundaries::common::lazy_def_id(&types, element_type),
        Some(data_point_def),
        "imported member reference should point at the exported source interface",
    );
    let resolved_element = state.resolve_lazy_type(element_type);
    let value = types.intern_string("value");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            resolved_element,
            value,
        )
        .is_some(),
        "imported source interface body should be available without falling back",
    );
}

#[test]
fn direct_cross_file_interface_lowering_accepts_imported_source_return_type_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    let (metrics_arena, metrics_binder, types) = parse_bound_source_with_name(
        "metrics.ts",
        r#"
                export interface SeriesSummary {
                    mean: number;
                }
                export function summarize(values: readonly number[]): SeriesSummary {
                    return { mean: 0 };
                }
            "#,
    );
    let view_source = r#"
                import { summarize as summarizeValues } from "./metrics";
                export interface Model {
                    summary: ReturnType<typeof summarizeValues>;
                }
            "#;
    let mut view_parser = ParserState::new("view.ts".to_string(), view_source.to_string());
    let view_root = view_parser.parse_source_file();
    let mut view_binder = BinderState::new();
    view_binder.bind_source_file_with_libs(view_parser.get_arena(), view_root, &lib_files);
    let view_arena = Arc::new(view_parser.get_arena().clone());
    let view_binder = Arc::new(view_binder);
    let ctx = CheckerContext::new(
        view_arena.as_ref(),
        view_binder.as_ref(),
        &types,
        "view.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    set_two_file_import_context(
        &mut state,
        &metrics_arena,
        &metrics_binder,
        &view_arena,
        &view_binder,
    );
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let model_sym = view_binder.file_locals.get("Model").expect("Model symbol");
    let (model_type, params) = state
        .direct_cross_file_interface_lowering(
            model_sym,
            view_binder.as_ref(),
            view_arena.as_ref(),
            false,
            true,
        )
        .expect("source interface with imported ReturnType query should lower");

    assert!(params.is_empty());
    let summary = types.intern_string("summary");
    let summary_type = crate::query_boundaries::common::raw_property_type(
        state.ctx.types.as_type_database(),
        model_type,
        summary,
    )
    .expect("summary property should lower");
    let evaluated_summary = state.evaluate_type_with_env(summary_type);
    let mean = types.intern_string("mean");
    assert_eq!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            evaluated_summary,
            mean,
        ),
        Some(TypeId::NUMBER),
        "`ReturnType<typeof importedFunction>` should expose the explicit source return shape",
    );
}
