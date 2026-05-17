use super::*;
use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

#[test]
fn cached_final_builtin_base_allows_deep_dom_leaf_direct_lowering() {
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

    let html_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("HTMLElement")
        .expect("HTMLElement should resolve to a dom lib symbol");
    let html_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&html_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("HTMLElement should have a delegate arena");
    assert!(
        state
            .direct_cross_file_interface_lowering(
                html_sym_id,
                state.ctx.binder,
                html_arena,
                false,
                false,
            )
            .is_none(),
        "deep builtin dom bases should fall back while their base is not final",
    );

    let html_ty = state
        .resolve_lib_type_by_name("HTMLElement")
        .expect("HTMLElement should resolve through the mature lib path");
    let shared_cache = Arc::new(dashmap::DashMap::new());
    shared_cache.insert("HTMLElement".to_string(), Some(html_ty));
    state.ctx.shared_lib_type_cache = Some(shared_cache);
    state.ctx.lib_type_resolution_cache.remove("HTMLElement");
    assert_eq!(
        state
            .ctx
            .lib_type_resolution_cache
            .get("HTMLElement")
            .copied()
            .flatten(),
        None,
        "the direct guard should be able to rely on the shared final-base cache",
    );

    let div_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("HTMLDivElement")
        .expect("HTMLDivElement should resolve to a dom lib symbol");
    let div_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&div_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("HTMLDivElement should have a delegate arena");
    let (div_ty, div_params) = state
        .direct_cross_file_interface_lowering(div_sym_id, state.ctx.binder, div_arena, false, false)
        .expect("builtin dom interfaces with a cached final base should lower directly");
    assert!(div_params.is_empty());

    let tag_name = state.ctx.types.intern_string("tagName");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            div_ty,
            tag_name,
        )
        .is_some(),
        "HTMLDivElement should include inherited Element members through the cached final base",
    );
}
