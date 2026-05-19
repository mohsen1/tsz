use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::{check_source_with_libs, load_compiled_lib_files, load_lib_files};
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
    let (value_merged_type, value_merged_params) = state
        .direct_actual_lib_symbol_type(
            value_merged_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(value_merged_arena),
            false,
        )
        .expect("value-merged DOM interfaces should lower directly with heritage merged");
    assert!(value_merged_params.is_empty());
    let inner_html = state.ctx.types.intern_string("innerHTML");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types,
            value_merged_type,
            inner_html,
        )
        .is_some(),
        "direct-lowered HTMLDivElement should preserve inherited HTMLElement members",
    );
    assert!(
        state
            .ctx
            .lib_delegation_cache
            .contains_symbol_type(value_merged_sym_id),
        "direct value-merged DOM interfaces should populate lib delegation cache",
    );
}

#[test]
fn direct_value_merged_builtin_dom_interface_symbol_type_returns_type_position_lazy_ref() {
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

    let validity_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("ValidityState")
        .expect("ValidityState should resolve to a value-merged dom lib symbol");
    let validity_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&validity_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("ValidityState should have a delegate arena");
    assert!(super::is_builtin_lib_declaration_arena(validity_arena));
    assert!(
        state
            .ctx
            .symbol_is_from_actual_or_cloned_lib(validity_sym_id)
    );
    let validity_symbol = state
        .get_cross_file_symbol(validity_sym_id)
        .expect("ValidityState cross-file symbol");
    assert!(
        validity_symbol.flags & symbol_flags::INTERFACE != 0
            && validity_symbol.flags & symbol_flags::VALUE != 0,
        "ValidityState cross-file flags: {}",
        validity_symbol.flags,
    );
    assert!(
        validity_symbol.flags
            & (symbol_flags::CLASS
                | symbol_flags::TYPE_ALIAS
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE)
            == 0,
        "ValidityState flags: {}",
        validity_symbol.flags,
    );
    assert!(!state.lib_name_locally_augmented("ValidityState"));
    let (validity_state, params) = state
        .direct_value_merged_builtin_lib_interface_symbol_type(
            validity_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(validity_arena),
            false,
        )
        .expect("value-merged builtin DOM interface should resolve through lib type identity");
    assert!(params.is_empty());
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, validity_state).is_some(),
        "value-merged DOM interfaces should return a type-position Lazy ref",
    );

    let document_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Document")
        .expect("Document should resolve to a value-merged dom lib symbol");
    let document_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&document_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("Document should have a delegate arena");
    let (document_type, document_params) = state
        .direct_value_merged_builtin_lib_interface_symbol_type(
            document_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(document_arena),
            false,
        )
        .expect("method-bearing DOM interfaces should lower directly with heritage merged");
    assert!(document_params.is_empty());
    let query_selector = state.ctx.types.intern_string("querySelector");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types,
            document_type,
            query_selector,
        )
        .is_some(),
        "direct-lowered Document should preserve inherited ParentNode methods",
    );

    let error_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Error")
        .expect("Error should resolve to an es lib symbol");
    let error_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&error_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("Error should have a delegate arena");
    assert!(
        state
            .direct_value_merged_builtin_lib_interface_symbol_type(
                error_sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(error_arena),
                false,
            )
            .is_none(),
        "non-DOM value-merged lib interfaces have lib-set-sensitive shapes and should stay on the existing path",
    );
}

#[test]
fn value_merged_builtin_dom_interface_type_argument_keeps_inherited_members() {
    let lib_files = load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
        "lib.es2016.array.include.d.ts",
        "lib.es2016.d.ts",
        "lib.es2017.arraybuffer.d.ts",
        "lib.es2017.date.d.ts",
        "lib.es2017.object.d.ts",
        "lib.es2017.sharedmemory.d.ts",
        "lib.es2017.string.d.ts",
        "lib.es2017.typedarrays.d.ts",
        "lib.es2017.d.ts",
        "lib.es2018.asyncgenerator.d.ts",
        "lib.es2018.asynciterable.d.ts",
        "lib.es2018.promise.d.ts",
        "lib.es2018.regexp.d.ts",
        "lib.es2018.d.ts",
        "lib.es2019.array.d.ts",
        "lib.es2019.object.d.ts",
        "lib.es2019.string.d.ts",
        "lib.es2019.symbol.d.ts",
        "lib.es2019.d.ts",
        "lib.es2020.bigint.d.ts",
        "lib.es2020.date.d.ts",
        "lib.es2020.number.d.ts",
        "lib.es2020.promise.d.ts",
        "lib.es2020.sharedmemory.d.ts",
        "lib.es2020.string.d.ts",
        "lib.es2020.symbol.wellknown.d.ts",
        "lib.es2020.d.ts",
        "lib.dom.d.ts",
        "lib.dom.iterable.d.ts",
    ]);
    let diagnostics = check_source_with_libs(
        r##"
const app = document.querySelector<HTMLDivElement>("#app");
if (app) {
  app.innerHTML = "";
}
"##,
        "fixture.ts",
        CheckerOptions::default(),
        &lib_files,
    );

    assert!(
        diagnostics.is_empty(),
        "expected DOM querySelector type argument to keep inherited members, got: {diagnostics:?}",
    );
}
