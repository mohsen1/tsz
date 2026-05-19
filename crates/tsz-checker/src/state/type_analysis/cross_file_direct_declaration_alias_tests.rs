use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

#[test]
fn direct_declaration_file_type_alias_lowers_builtin_dom_alias_body() {
    let types = TypeInterner::new();
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
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

    let cases = [
        ("CanvasImageSource", false, false),
        ("NodeFilter", true, false),
        ("ReadableStreamReadResult", false, true),
    ];

    for (name, value_merged, generic) in cases {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a DOM lib symbol"));
        let symbol = state
            .ctx
            .binder
            .get_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should exist"));
        assert_ne!(symbol.flags & symbol_flags::TYPE_ALIAS, 0);
        assert_eq!(
            symbol.flags & symbol_flags::VALUE != 0,
            value_merged,
            "{name} merge-shape precondition changed",
        );
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));

        let (ty, params) = state
            .direct_declaration_file_type_alias_result(sym_id, delegate_arena)
            .unwrap_or_else(|| panic!("{name} declaration alias should lower directly"));
        assert_ne!(ty, TypeId::UNKNOWN);
        assert_ne!(ty, TypeId::ERROR);
        assert_eq!(
            params.is_empty(),
            !generic,
            "{name} generic parameter shape"
        );
        assert!(
            state.ctx.lib_delegation_cache.symbol_type(sym_id).is_some(),
            "{name} should populate the built-in lib delegation cache",
        );
    }

    for literal_union_alias in ["DocumentReadyState", "XMLHttpRequestResponseType"] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(literal_union_alias)
            .unwrap_or_else(|| panic!("{literal_union_alias} should resolve to a DOM lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{literal_union_alias} should have a delegate arena"));

        assert!(
            state
                .direct_declaration_file_type_alias_result(sym_id, delegate_arena)
                .is_none(),
            "{literal_union_alias} should stay on the normal cross-file path so recursive mapped inference preserves its string-like apparent shape",
        );
    }
}
