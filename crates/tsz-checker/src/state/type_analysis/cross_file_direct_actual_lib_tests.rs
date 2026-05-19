use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_common::perf_counters::CrossArenaSymbolMissSource;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

#[test]
fn direct_cross_file_interface_lowering_handles_simple_builtin_dom_interfaces() {
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let simple_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("PaymentCurrencyAmount")
        .expect("PaymentCurrencyAmount should resolve to a dom lib symbol");
    let simple_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&simple_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("PaymentCurrencyAmount should have a delegate arena");
    let (simple_ty, simple_params) = state
        .direct_cross_file_interface_lowering(
            simple_sym_id,
            state.ctx.binder,
            simple_arena,
            false,
            false,
        )
        .expect("simple builtin dom interface should lower directly");
    assert_ne!(simple_ty, TypeId::UNKNOWN);
    assert_ne!(simple_ty, TypeId::ERROR);
    assert!(simple_params.is_empty());

    let heritage_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("AddEventListenerOptions")
        .expect("AddEventListenerOptions should resolve to a dom lib symbol");
    let heritage_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&heritage_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("AddEventListenerOptions should have a delegate arena");
    assert!(
        state
            .direct_cross_file_interface_lowering(
                heritage_sym_id,
                state.ctx.binder,
                heritage_arena,
                false,
                false,
            )
            .is_none(),
        "generic direct interface lowering still rejects heritage",
    );
    let (heritage_ty, heritage_params) = state
        .direct_actual_lib_symbol_type(
            heritage_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(heritage_arena),
            false,
        )
        .expect("builtin dom interface with safe heritage should resolve through lib identity");
    assert_ne!(heritage_ty, TypeId::UNKNOWN);
    assert_ne!(heritage_ty, TypeId::ERROR);
    assert!(heritage_params.is_empty());
    let once = state.ctx.types.intern_string("once");
    let capture = state.ctx.types.intern_string("capture");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            heritage_ty,
            once,
        )
        .is_some(),
        "direct lowering should keep own interface members",
    );
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            heritage_ty,
            capture,
        )
        .is_some(),
        "direct lowering should merge inherited EventListenerOptions members",
    );

    let value_merged_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("HTMLDivElement")
        .expect("HTMLDivElement should resolve to a value-merged dom lib symbol");
    let value_merged_symbol = state
        .ctx
        .binder
        .get_symbol(value_merged_sym_id)
        .expect("HTMLDivElement symbol should exist");
    assert!(
        value_merged_symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::VALUE),
        "HTMLDivElement should be both an interface and constructor value",
    );
    let value_merged_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&value_merged_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("HTMLDivElement should have a delegate arena");
    assert!(
        state
            .direct_builtin_lib_interface_symbol_type(
                value_merged_sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(value_merged_arena),
                false,
            )
            .is_none(),
        "value-merged dom interfaces must not use canonical lib interface identity",
    );
    assert!(
        state
            .direct_actual_lib_symbol_type(
                value_merged_sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(value_merged_arena),
                false,
            )
            .is_none(),
        "value-merged dom interfaces should stay on the existing fallback path",
    );
    assert!(
        !state
            .ctx
            .lib_delegation_cache
            .contains_symbol_type(value_merged_sym_id),
        "declined value-merged dom interfaces should not populate lib delegation cache",
    );
}
