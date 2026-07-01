use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::{
    check_multi_file_with_libs, check_source_with_libs, load_compiled_lib_files, load_lib_files,
};
use std::sync::Arc;
use tsz_binder::{BinderState, lib_loader::LibFile, symbol_flags};
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_common::perf_counters::CrossArenaSymbolMissSource;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

#[test]
fn direct_actual_lib_value_interface_admission_uses_provenance_not_names() {
    let source = include_str!("cross_file_direct_actual_lib.rs");

    assert!(
        !source.contains("fn is_direct_actual_lib_value_interface_name"),
        "actual-lib value-interface admission should use symbol/declaration provenance, not a hardcoded name allowlist",
    );
}

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
        .get("ValidityState")
        .expect("ValidityState should resolve to a value-merged dom lib symbol");
    let value_merged_symbol = state
        .ctx
        .binder
        .get_symbol(value_merged_sym_id)
        .expect("ValidityState symbol should exist");
    assert!(
        value_merged_symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::VALUE),
        "ValidityState should be both an interface and constructor value",
    );
    let value_merged_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&value_merged_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("ValidityState should have a delegate arena");
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
    let (value_merged_ty, value_merged_params) = state
        .direct_actual_lib_symbol_type(
            value_merged_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(value_merged_arena),
            false,
        )
        .expect("value-merged dom interfaces without heritage should stay lazy");
    assert!(value_merged_params.is_empty());
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, value_merged_ty).is_some(),
        "value-merged dom interfaces should use a type-position Lazy ref",
    );
    assert!(
        state
            .ctx
            .lib_delegation_cache
            .contains_symbol_type(value_merged_sym_id),
        "admitted value-merged dom interfaces should populate lib delegation cache",
    );

    let html_div_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("HTMLDivElement")
        .expect("HTMLDivElement should resolve to a value-merged dom lib symbol");
    let html_div_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&html_div_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("HTMLDivElement should have a delegate arena");
    let (html_div_ty, html_div_params) = state
        .direct_value_merged_builtin_lib_interface_symbol_type(
            html_div_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(html_div_arena),
            false,
        )
        .expect("value-merged DOM interfaces with only void-return own methods should stay lazy");
    assert!(html_div_params.is_empty());
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, html_div_ty).is_some(),
        "HTMLDivElement should use a type-position Lazy ref",
    );

    let document_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("document")
        .expect("document should resolve to a dom lib variable");
    let document_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&document_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("document should have a delegate arena");
    let (document_ty, document_params) = state
        .direct_actual_lib_symbol_type(
            document_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(document_arena),
            false,
        )
        .expect("builtin DOM variables with simple type-reference annotations should stay lazy");
    assert!(document_params.is_empty());
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, document_ty).is_some(),
        "document should use a type-position Lazy ref",
    );
}

#[test]
fn direct_actual_lib_symbol_type_lowers_builtin_dom_alias_bodies() {
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

    for name in [
        "DOMHighResTimeStamp",
        "GLenum",
        "Base64URLString",
        "BigInteger",
        "BlobPart",
        "EventListenerOrEventListenerObject",
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a dom lib alias"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let (ty, params) = state
            .direct_actual_lib_symbol_type(
                sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(delegate_arena),
                false,
            )
            .unwrap_or_else(|| panic!("{name} should lower through the direct builtin alias path"));
        assert_ne!(ty, TypeId::UNKNOWN, "{name} must not lower to unknown");
        assert_ne!(ty, TypeId::ERROR, "{name} must not lower to error");
        assert!(
            params.is_empty(),
            "{name} should not synthesize type params"
        );
    }
}

#[test]
fn direct_builtin_dom_interface_uses_declaration_provenance_without_lib_symbol_flag() {
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    Arc::make_mut(&mut binder.lib_symbol_ids).clear();
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("PaymentCurrencyAmount")
        .expect("PaymentCurrencyAmount should resolve to a dom lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("PaymentCurrencyAmount should have a delegate arena");
    assert!(
        !state.ctx.symbol_is_from_actual_or_cloned_lib(sym_id),
        "test setup should exercise declaration-provenance fallback",
    );

    let (direct_type, params) = state
        .direct_builtin_lib_interface_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(delegate_arena),
            false,
        )
        .expect("builtin DOM declaration provenance should admit the direct lib path");
    assert_ne!(direct_type, TypeId::UNKNOWN);
    assert_ne!(direct_type, TypeId::ERROR);
    assert!(params.is_empty());
}

#[test]
fn direct_builtin_dom_member_batch_resolves_actual_lib_property_refs() {
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

    let dom = lib_files
        .iter()
        .find(|lib| {
            lib.arena
                .source_files
                .first()
                .is_some_and(|source_file| source_file.file_name.ends_with("dom.d.ts"))
        })
        .expect("dom lib should be loaded");
    let sym_id = dom
        .binder
        .file_locals
        .get("SVGURIReference")
        .expect("SVGURIReference should resolve to a dom lib symbol");
    let interface_idx = dom
        .binder
        .get_symbol(sym_id)
        .expect("SVGURIReference symbol should exist")
        .declarations[0];
    let interface = dom
        .arena
        .get(interface_idx)
        .and_then(|node| dom.arena.get_interface(node))
        .expect("SVGURIReference should have an interface declaration");
    let href_member = interface
        .members
        .nodes
        .iter()
        .copied()
        .find(|&member_idx| {
            dom.arena
                .get(member_idx)
                .and_then(|member| dom.arena.get_signature(member))
                .and_then(|sig| dom.arena.get_identifier_text(sig.name))
                == Some("href")
        })
        .expect("SVGURIReference.href should exist");

    let results = state
        .direct_cross_file_interface_member_simple_types(
            interface_idx,
            &[href_member],
            dom.arena.as_ref(),
            dom.binder.as_ref(),
            None,
            false,
        )
        .expect("DOM member batch should lower actual-lib property references");
    let href_type = results
        .get(&href_member)
        .copied()
        .expect("href member should lower directly");
    let expected_def_id = state
        .resolve_actual_lib_name_to_def_id_for_lowering("SVGAnimatedString")
        .expect("SVGAnimatedString should have actual-lib identity");
    let actual_def_id =
        crate::query_boundaries::common::lazy_def_id(state.ctx.types.as_type_database(), href_type);

    assert_eq!(actual_def_id, Some(expected_def_id));
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
    let (cached_validity, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(validity_sym_id)
        .expect("admitted value-merged DOM interfaces should populate the delegation cache");
    assert_eq!(cached_validity, validity_state);
    assert!(
        cached_params.is_empty(),
        "ValidityState should cache without generic params",
    );

    let html_div_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("HTMLDivElement")
        .expect("HTMLDivElement should resolve to a value-merged dom lib symbol");
    let html_div_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&html_div_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("HTMLDivElement should have a delegate arena");
    let (html_div, html_div_params) = state
        .direct_value_merged_builtin_lib_interface_symbol_type(
            html_div_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            Some(html_div_arena),
            false,
        )
        .expect("lazy-safe value/interface DOM types may still use local Lazy identities");
    assert!(html_div_params.is_empty());
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, html_div).is_some(),
        "HTMLDivElement should return a local type-position Lazy ref",
    );
    let inner_html = state
        .resolve_simple_lib_interface_own_property("HTMLDivElement", "innerHTML")
        .expect("single-member DOM resolver should walk non-generic inherited properties");
    assert_ne!(inner_html, TypeId::ERROR);
    assert_ne!(inner_html, TypeId::UNKNOWN);
    let append_child = state
        .resolve_simple_lib_interface_own_property("HTMLDivElement", "appendChild")
        .expect("single-member DOM resolver should walk inherited methods");
    assert_ne!(append_child, TypeId::ERROR);
    assert_ne!(append_child, TypeId::UNKNOWN);

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
    assert!(
        state
            .direct_value_merged_builtin_lib_interface_symbol_type(
                document_sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(document_arena),
                false,
            )
            .is_none(),
        "value-merged DOM interfaces with heritage stay on the existing child/interface path",
    );
    let query_selector = state
        .resolve_simple_lib_interface_own_property("Document", "querySelector")
        .expect("single-member DOM resolver should lower inherited method groups");
    assert_ne!(query_selector, TypeId::ERROR);
    assert_ne!(query_selector, TypeId::UNKNOWN);

    let document_value_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("document")
        .expect("document should resolve to a lib value symbol");
    let document_value_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&document_value_sym_id)
        .map(std::convert::AsRef::as_ref);
    let (document_value, document_value_params) = state
        .direct_actual_lib_symbol_type(
            document_value_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            document_value_arena,
            false,
        )
        .expect("document should lower to its annotated lazy lib interface");
    assert!(
        document_value_params.is_empty(),
        "document should not expose type parameters",
    );
    let document_def = state
        .resolve_actual_lib_name_to_def_id_for_lowering("Document")
        .expect("Document should resolve to a lib definition");
    assert_eq!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, document_value),
        Some(document_def),
        "document should preserve its Document annotation as a Lazy interface",
    );
    assert!(
        state
            .lazy_lib_member_receiver_def_id(document_value)
            .is_some(),
        "the returned lazy interface should remain eligible for lazy member lookup",
    );
    let (cached_document_value, cached_document_value_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(document_value_sym_id)
        .expect("direct annotation path should populate the delegation cache");
    assert_eq!(cached_document_value, document_value);
    assert!(cached_document_value_params.is_empty());

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
fn inherited_simple_lib_member_falls_back_on_duplicate_renamed_bases() {
    let lib_files = vec![Arc::new(LibFile::from_source(
        "lib.ambiguous-member.d.ts".to_string(),
        r#"
interface AlphaBase {
  sharedSlot: string;
}

interface BetaBase {
  sharedSlot: string;
}

interface CombinedTarget extends AlphaBase, BetaBase {}
"#
        .to_string(),
    ))];
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

    assert!(
        state
            .resolve_simple_lib_interface_own_property("CombinedTarget", "sharedSlot")
            .is_none(),
        "inherited simple-member fast path should fall back when multiple bases resolve the property",
    );
}

#[test]
fn simple_lib_member_with_lib_interface_reference_annotation_stays_lazy() {
    // A non-readonly member whose annotation is a bare reference to an eligible
    // simple lib interface (`inner: Leaf`) must resolve to a `Lazy(DefId)` ref
    // rather than the materialized `Leaf` shape, so chained access such as
    // `wrapper.inner.value` keeps each link on the single-member fast path. A
    // member referencing a generic lib interface (`boxed: Generic<string>`)
    // lowers to an `Application`, is ineligible for the lazy receiver path, and
    // must fall back to full materialization.
    let lib_files = vec![Arc::new(LibFile::from_source(
        "lib.lazy-ref-member.d.ts".to_string(),
        r#"
interface Leaf {
  value: string;
}

interface Generic<T> {
  item: T;
}

interface Wrapper {
  inner: Leaf;
  boxed: Generic<string>;
}
"#
        .to_string(),
    ))];
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

    let inner = state
        .resolve_simple_lib_interface_own_property("Wrapper", "inner")
        .expect("a non-readonly member typed as a simple lib interface should resolve lazily");
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, inner).is_some(),
        "the member should lower to a Lazy(DefId) reference, not a materialized object shape",
    );
    assert!(
        state.lazy_lib_member_receiver_def_id(inner).is_some(),
        "the lazily-resolved member should itself be eligible for further lazy member access",
    );

    assert!(
        state
            .resolve_simple_lib_interface_own_property("Wrapper", "boxed")
            .is_none(),
        "a member referencing a generic lib interface is ineligible and must fall back to full materialization",
    );
}

#[test]
fn force_eligible_lib_type_reference_defers_to_lazy() {
    // #13933: a bare type reference to a non-generic, heritage-bearing lib
    // interface (`HTMLDivElement extends HTMLElement extends … extends Node`)
    // must defer to a `Lazy(DefId)` at the reference site instead of
    // materializing the full transitive heritage closure. A generic lib
    // interface (`Array<T>`) is ineligible and must keep the legacy path so the
    // bare-Lazy receiver never has to supply unsubstituted type arguments.
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

    let html_div_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("HTMLDivElement")
        .expect("HTMLDivElement should resolve to a value-merged dom lib symbol");
    let deferred = state
        .try_defer_eligible_lib_type_reference(html_div_sym_id)
        .expect("a non-generic heritage-bearing dom interface must defer to a Lazy(DefId)");
    assert!(
        crate::query_boundaries::common::lazy_def_id(state.ctx.types, deferred).is_some(),
        "the deferred type reference must be a bare Lazy(DefId), not a materialized object shape",
    );
    assert!(
        state.lazy_lib_member_receiver_def_id(deferred).is_some(),
        "the deferred reference must itself be eligible for the lazy single-member path",
    );

    let array_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Array")
        .expect("Array should resolve to a generic lib symbol");
    assert!(
        state
            .try_defer_eligible_lib_type_reference(array_sym_id)
            .is_none(),
        "a generic lib interface must not defer (the bare-Lazy path cannot supply type arguments)",
    );

    let split_lib_files = load_lib_files(&["es5.d.ts", "es2018.regexp.d.ts"]);
    let mut split_parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let split_root = split_parser.parse_source_file();
    let mut split_binder = BinderState::new();
    split_binder.bind_source_file_with_libs(split_parser.get_arena(), split_root, &split_lib_files);
    let split_arena = Arc::new(split_parser.get_arena().clone());
    let split_binder = Arc::new(split_binder);
    let split_types = TypeInterner::new();
    let split_ctx = CheckerContext::new(
        split_arena.as_ref(),
        split_binder.as_ref(),
        &split_types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut split_state = CheckerState { ctx: split_ctx };
    let split_lib_contexts: Vec<LibContext> = split_lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    split_state.ctx.set_lib_contexts(split_lib_contexts);
    split_state
        .ctx
        .set_actual_lib_file_count(split_lib_files.len());

    let regexp_match_sym_id = split_state
        .ctx
        .binder
        .file_locals
        .get("RegExpMatchArray")
        .expect("RegExpMatchArray should resolve to a merged lib symbol");
    let regexp_match_symbol = split_state
        .ctx
        .binder
        .get_symbol(regexp_match_sym_id)
        .expect("RegExpMatchArray symbol should be present");
    assert!(
        regexp_match_symbol.declarations.len() > 1,
        "the regression witness must exercise a split lib interface",
    );
    assert!(
        split_state
            .try_defer_eligible_lib_type_reference(regexp_match_sym_id)
            .is_none(),
        "a split lib interface must keep the eager merged path so diagnostic provenance stays intact",
    );
}

#[test]
fn lazy_lib_member_lookup_caches_by_receiver_def_id() {
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
        "lib.es2017.object.d.ts",
        "lib.es2017.sharedmemory.d.ts",
        "lib.es2017.string.d.ts",
        "lib.es2017.intl.d.ts",
        "lib.es2017.typedarrays.d.ts",
        "lib.es2018.asyncgenerator.d.ts",
        "lib.es2018.asynciterable.d.ts",
        "lib.es2018.intl.d.ts",
        "lib.es2018.promise.d.ts",
        "lib.es2018.regexp.d.ts",
        "lib.es2018.d.ts",
        "lib.es2019.array.d.ts",
        "lib.es2019.object.d.ts",
        "lib.es2019.string.d.ts",
        "lib.es2019.symbol.d.ts",
        "lib.es2019.intl.d.ts",
        "lib.es2019.d.ts",
        "lib.es2020.bigint.d.ts",
        "lib.es2020.date.d.ts",
        "lib.es2020.promise.d.ts",
        "lib.es2020.sharedmemory.d.ts",
        "lib.es2020.string.d.ts",
        "lib.es2020.symbol.wellknown.d.ts",
        "lib.es2020.intl.d.ts",
        "lib.es2020.number.d.ts",
        "lib.es2020.d.ts",
        "lib.dom.d.ts",
        "lib.dom.iterable.d.ts",
    ]);
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

    let document_value_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("document")
        .expect("document should resolve to a lib value symbol");
    let document_value_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&document_value_sym_id)
        .map(std::convert::AsRef::as_ref);
    let (document_value, _) = state
        .direct_actual_lib_symbol_type(
            document_value_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            document_value_arena,
            false,
        )
        .expect("document should lower to its annotated lazy lib interface");

    let first = state
        .try_lazy_lib_member_property_access(document_value, "title")
        .expect("Document.title should resolve through the lazy member fast path");
    let entries_after_first = state
        .ctx
        .lib_type_resolution_caches
        .lazy_member_receiver_properties
        .borrow()
        .len();
    let second = state
        .try_lazy_lib_member_property_access(document_value, "title")
        .expect("cached Document.title should still resolve through the fast path");

    let first_type = match first {
        tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } => type_id,
        other => panic!("first lazy member lookup should succeed, got {other:?}"),
    };
    let second_type = match second {
        tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } => type_id,
        other => panic!("cached lazy member lookup should succeed, got {other:?}"),
    };
    assert_eq!(
        first_type, second_type,
        "cached lazy member lookup should return the same property-access result",
    );
    assert_eq!(
        entries_after_first, 1,
        "first lazy member lookup should record one receiver/property cache entry",
    );
    assert_eq!(
        state
            .ctx
            .lib_type_resolution_caches
            .lazy_member_receiver_properties
            .borrow()
            .len(),
        entries_after_first,
        "repeating the same receiver/property lookup should hit the DefId-keyed cache",
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
  app.addEventListener("click", ev => {
    ev.preventDefault();
  });
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

#[test]
fn value_merged_builtin_dom_interface_keeps_inherited_members_in_project_mode() {
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
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "main.ts",
                r##"
import { renderDashboard } from "./view";

const app = document.querySelector<HTMLDivElement>("#app");

if (app) {
  app.innerHTML = renderDashboard();
}
"##,
            ),
            (
                "view.ts",
                r#"
export function renderDashboard(): string {
  return "<main></main>";
}
"#,
            ),
            (
                "env.d.ts",
                r#"
export {};

declare global {
  interface ImportMetaEnv {
    readonly MODE: string;
  }
}
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            target: ScriptTarget::ES2020,
            module: ModuleKind::ESNext,
            module_explicitly_set: true,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    );

    assert!(
        diagnostics.is_empty(),
        "expected DOM querySelector type argument to keep inherited members in project mode, got: {diagnostics:?}",
    );
}

#[test]
fn cross_file_return_type_property_stays_assignable_to_declared_model() {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts"]);
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "metrics.ts",
                r#"
export interface DataPoint {
  label: string;
  value: number;
}

export interface SeriesSummary {
  min: number;
  max: number;
  mean: number;
  p95: number;
}

export function summarizeSeries(points: readonly DataPoint[]): SeriesSummary {
  const values = points.map((point) => point.value).sort((left, right) => left - right);
  const total = values.reduce((sum, value) => sum + value, 0);
  return {
    min: values[0] || 0,
    max: values[values.length - 1] || 0,
    mean: values.length === 0 ? 0 : total / values.length,
    p95: values[values.length - 1] || 0,
  };
}
"#,
            ),
            (
                "view.ts",
                r#"
import { summarizeSeries } from "./metrics";

interface LocalHarnessPaddingA {}
interface LocalHarnessPaddingB {}
interface LocalHarnessPaddingC {}
interface LocalHarnessPaddingD {}

export interface DashboardModel {
  title: string;
  points: { label: string; value: number }[];
  summary: ReturnType<typeof summarizeSeries>;
}

const points = [
  { label: "sample-1", value: 100 },
  { label: "sample-2", value: 200 },
];
const model: DashboardModel = {
  title: "ambient module benchmark",
  points,
  summary: summarizeSeries(points),
};
model.summary.mean.toFixed(2);
"#,
            ),
        ],
        "view.ts",
        CheckerOptions {
            target: ScriptTarget::ES2020,
            module: ModuleKind::ESNext,
            module_explicitly_set: true,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    );

    assert!(
        diagnostics.is_empty(),
        "expected imported ReturnType model property to stay assignable, got: {diagnostics:?}",
    );
}
