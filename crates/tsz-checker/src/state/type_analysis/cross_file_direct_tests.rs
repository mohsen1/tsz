use super::{
    is_builtin_lib_file_name, is_external_package_declaration_file_name,
    is_special_generic_direct_actual_lib_alias_body_admitted,
};
use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::perf_counters::{CrossArenaSymbolMissSource, DirectActualLibAliasBodyOutcome};
use tsz_parser::parser::{ParserState, syntax_kind_ext};
use tsz_solver::{TypeId, TypeInterner};

fn parse_interface_declarations(
    source: &str,
) -> (
    tsz_parser::parser::node::NodeArena,
    Vec<tsz_parser::NodeIndex>,
) {
    let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena().clone();
    let source_file = arena
        .get_source_file_at(root)
        .expect("source file should parse");
    let declarations = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .filter(|idx| {
            arena
                .get(*idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
        })
        .collect();
    (arena, declarations)
}

fn parse_bound_source(
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    parse_bound_source_with_name("fixture.ts", source)
}

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

#[test]
fn detects_npm_and_source_tree_builtin_lib_names() {
    assert!(is_builtin_lib_file_name("lib.es2024.d.ts"));
    assert!(is_builtin_lib_file_name("lib.dom.d.ts"));
    assert!(is_builtin_lib_file_name("es2024.d.ts"));
    assert!(is_builtin_lib_file_name("es2024.full.d.ts"));
    assert!(is_builtin_lib_file_name("dom.generated.d.ts"));
    assert!(is_builtin_lib_file_name("dom.iterable.generated.d.ts"));
    assert!(is_builtin_lib_file_name("webworker.asynciterable.d.ts"));
    assert!(is_builtin_lib_file_name("decorators.legacy.d.ts"));
}

#[test]
fn does_not_treat_arbitrary_declaration_files_as_builtin_libs() {
    assert!(!is_builtin_lib_file_name("react/index.d.ts"));
    assert!(!is_builtin_lib_file_name(
        "node_modules/@types/node/fs.d.ts"
    ));
    assert!(!is_builtin_lib_file_name("packages/foo/src/types.d.ts"));
}

#[test]
fn detects_external_package_declaration_paths() {
    assert!(is_external_package_declaration_file_name(
        "node_modules/react/index.d.ts"
    ));
    assert!(is_external_package_declaration_file_name(
        "/repo/node_modules/@types/node/fs.d.ts"
    ));
    assert!(is_external_package_declaration_file_name(
        r"C:\repo\node_modules\@types\node\fs.d.ts"
    ));
}

#[test]
fn does_not_treat_local_declaration_paths_as_external_packages() {
    assert!(!is_external_package_declaration_file_name(
        "packages/foo/src/types.d.ts"
    ));
    assert!(!is_external_package_declaration_file_name(
        "/repo/fixtures/node-modules-like/types.d.ts"
    ));
}

#[test]
fn source_file_direct_interface_lowering_accepts_scope_independent_members() {
    let (arena, declarations) = parse_interface_declarations(
        r#"
                interface Leaf {
                    value: number;
                    tag: "leaf";
                    flags: true | false;
                }
            "#,
    );
    let declarations = vec![(declarations[0], &arena)];

    assert!(CheckerState::source_file_interface_declarations_are_direct_lowerable(&declarations,));
}

#[test]
fn source_file_direct_interface_lowering_rejects_scope_dependent_members() {
    let (arena, declarations) = parse_interface_declarations(
        r#"
                interface Local { value: number; }
                interface UsesLocal { value: Local; }
            "#,
    );
    let declarations = vec![(declarations[1], &arena)];

    assert!(!CheckerState::source_file_interface_declarations_are_direct_lowerable(&declarations,));
}

#[test]
fn direct_source_file_variable_annotation_accepts_same_file_simple_interface() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Leaf { value: number; tag: "leaf"; }
                const leaf: Leaf = { value: 1, tag: "leaf" };
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let leaf_sym = binder.file_locals.get("leaf").expect("leaf symbol");

    let result = state
        .direct_source_file_variable_annotation_type(
            leaf_sym,
            binder.as_ref(),
            arena.as_ref(),
            true,
        )
        .expect("simple same-file interface annotation should lower directly");

    assert!(
        crate::query_boundaries::common::is_lazy_type(&types, result),
        "variable annotation should preserve the interface lazy type"
    );
}

#[test]
fn direct_source_file_variable_annotation_rejects_type_alias_reference() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                type Leaf = { value: number };
                const leaf: Leaf = { value: 1 };
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let leaf_sym = binder.file_locals.get("leaf").expect("leaf symbol");

    assert!(
        state
            .direct_source_file_variable_annotation_type(
                leaf_sym,
                binder.as_ref(),
                arena.as_ref(),
                true,
            )
            .is_none(),
    );
}

#[test]
fn resolves_intl_namespace_exported_lib_interface_directly() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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
    let sym_id = state
        .resolve_lib_namespace_export_symbol("Intl", "CollatorOptions")
        .expect("Intl.CollatorOptions export should resolve");

    let ty = state
        .resolve_lib_interface_type_by_symbol("Intl.CollatorOptions", sym_id)
        .expect("Intl.CollatorOptions should lower directly");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
}

#[test]
fn direct_actual_lib_delegation_cache_preserves_type_params() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("ArrayIterator")
        .expect("ArrayIterator should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("ArrayIterator should lower through the direct lib path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 1, "ArrayIterator should expose T");

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .get(&sym_id)
        .expect("direct lib path should populate the delegation cache");
    assert_eq!(*cached_ty, ty);
    assert_eq!(
        cached_params.len(),
        params.len(),
        "cache hits must preserve generic application metadata",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_selected_value_interfaces() {
    let lib_files = load_lib_files(&["es5.d.ts", "es2015.iterable.d.ts", "es2020.intl.d.ts"]);
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

    let mut failures = Vec::new();
    for name in [
        "DateTimeFormatOptions",
        "DisplayNamesOptions",
        "Function",
        "Locale",
        "LocaleOptions",
        "NumberFormatOptions",
        "NumberFormatOptionsCurrencyDisplayRegistry",
        "NumberFormatOptionsSignDisplayRegistry",
        "NumberFormatOptionsStyleRegistry",
        "NumberFormatOptionsUseGroupingRegistry",
        "Object",
        "RegExp",
        "RelativeTimeFormatOptions",
        "ResolvedRelativeTimeFormatOptions",
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .or_else(|| state.resolve_lib_namespace_export_symbol("Intl", name))
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        let symbol = state
            .ctx
            .binder
            .get_symbol(sym_id)
            .expect("symbol id should resolve")
            .clone();
        let direct_lib_only =
            state.symbol_declarations_are_direct_actual_lib_only(sym_id, &symbol, name);
        let proven_value_interface =
            state.symbol_is_proven_direct_actual_lib_value_interface(sym_id, &symbol, name);
        assert!(
            proven_value_interface,
            "{name} should be admitted by actual-lib value-interface proof",
        );

        let Some((ty, _)) = state.direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        ) else {
            failures.push(format!(
                    "{name} (flags=0x{:x}, has_type={}, has_value={}, direct_lib_only={direct_lib_only})",
                    symbol.flags,
                    symbol.has_any_flags(tsz_binder::symbol_flags::TYPE),
                    symbol.has_any_flags(tsz_binder::symbol_flags::VALUE),
                ));
            continue;
        };

        assert_ne!(ty, TypeId::UNKNOWN, "{name} must not lower to unknown");
        assert_ne!(ty, TypeId::ERROR, "{name} must not lower to error");
        assert!(
            state.ctx.lib_delegation_cache.contains_key(&sym_id),
            "{name} should populate the delegation cache",
        );
    }
    assert!(
        failures.is_empty(),
        "selected value interfaces should lower directly, failures: {failures:?}",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_iterator_interfaces_with_params() {
    let lib_files = load_lib_files(&[
        "es2015.iterable.d.ts",
        "es2020.symbol.wellknown.d.ts",
        "esnext.iterator.d.ts",
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

    for name in [
        "ArrayIterator",
        "Iterator",
        "IteratorObject",
        "RegExpStringIterator",
        "StringIterator",
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let symbol = state
            .ctx
            .binder
            .get_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should exist"))
            .clone();
        assert!(
            state.symbol_has_direct_actual_lib_interface_type_parameters(sym_id, &symbol),
            "{name} should be admitted to the param-preserving direct path by lib declaration shape",
        );
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        let (ty, params) = state
            .direct_actual_lib_symbol_type(
                sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                delegate_arena,
                false,
            )
            .unwrap_or_else(|| panic!("{name} should lower through the direct lib path"));

        assert_ne!(ty, TypeId::UNKNOWN, "{name} should not lower to UNKNOWN");
        assert_ne!(ty, TypeId::ERROR, "{name} should not lower to ERROR");
        assert!(
            !params.is_empty(),
            "{name} should preserve generic type parameters",
        );
    }
}

#[test]
fn direct_actual_lib_symbol_type_allows_iterator_without_declaration_arena_proof() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts", "esnext.iterator.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let iterator_sym_id = binder
        .file_locals
        .get("Iterator")
        .expect("Iterator should resolve to a lib symbol");
    let iterator_decls = binder
        .get_symbol(iterator_sym_id)
        .expect("Iterator symbol should exist")
        .declarations
        .clone();
    let declaration_arenas = std::sync::Arc::make_mut(&mut binder.declaration_arenas);
    for decl_idx in iterator_decls {
        declaration_arenas.remove(&(iterator_sym_id, decl_idx));
    }

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

    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&iterator_sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            iterator_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Iterator should still lower through the direct lib path");

    assert_ne!(ty, TypeId::UNKNOWN, "Iterator should not lower to UNKNOWN");
    assert_ne!(ty, TypeId::ERROR, "Iterator should not lower to ERROR");
    assert!(
        !params.is_empty(),
        "Iterator should preserve generic type parameters",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_non_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts", "decorators.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("DecoratorMetadataObject")
        .expect("DecoratorMetadataObject should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("non-generic actual-lib alias body should lower directly");

    assert!(
        params.is_empty(),
        "DecoratorMetadataObject should be non-generic",
    );
    assert!(
        crate::query_boundaries::common::lazy_def_id(&types, ty).is_none(),
        "direct alias result should return the registered alias body, not the opaque Lazy alias",
    );

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .get(&sym_id)
        .expect("direct alias path should populate the delegation cache");
    assert_eq!(*cached_ty, ty);
    assert!(cached_params.is_empty());
}

#[test]
fn direct_actual_lib_symbol_type_handles_property_key_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("PropertyKey")
        .expect("PropertyKey should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);
    let symbol = state
        .get_cross_file_symbol(sym_id)
        .expect("PropertyKey symbol should be available")
        .clone();

    let proof = state
        .direct_actual_lib_type_alias_body(
            sym_id,
            &symbol,
            "PropertyKey",
            delegate_arena.expect("PropertyKey should have a delegate arena"),
        )
        .expect("PropertyKey should have a proven actual-lib alias body");
    assert_eq!(proof.outcome, DirectActualLibAliasBodyOutcome::Success);
    assert!(proof.type_params.is_empty(), "PropertyKey is non-generic",);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("PropertyKey should lower through the direct alias body path");
    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "PropertyKey should remain non-generic");
}

#[test]
fn direct_actual_lib_symbol_type_admits_proven_non_generic_aliases_without_name_list() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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

    for name in ["WeakKey", "ArrayBufferLike"] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let symbol = state
            .get_cross_file_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should be available"))
            .clone();

        assert!(
            !is_special_generic_direct_actual_lib_alias_body_admitted(name),
            "{name} must prove the non-generic path rather than the generic name list",
        );

        let proof = state
            .direct_actual_lib_type_alias_body(sym_id, &symbol, name, delegate_arena)
            .unwrap_or_else(|| panic!("{name} should have a proven actual-lib alias body"));
        assert_eq!(
            proof.outcome,
            DirectActualLibAliasBodyOutcome::Success,
            "{name} should be admitted by non-generic provenance, not a hardcoded name",
        );
        assert!(
            proof.type_params.is_empty(),
            "{name} should remain non-generic",
        );
        assert_ne!(proof.body, TypeId::ANY, "{name} should not lower to any");
        assert_ne!(proof.body, TypeId::UNKNOWN, "{name} should not be unknown");
        assert_ne!(proof.body, TypeId::ERROR, "{name} should not be error");

        let (direct_ty, direct_params) = state
            .direct_actual_lib_symbol_type(
                sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(delegate_arena),
                false,
            )
            .unwrap_or_else(|| panic!("{name} should lower through direct alias path"));
        assert_eq!(direct_ty, proof.body);
        assert!(direct_params.is_empty(), "{name} should stay non-generic");
    }
}

#[test]
fn direct_actual_lib_symbol_type_handles_record_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Record")
        .expect("Record should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Record should lower through the direct alias body path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 2, "Record should expose K and T");
}

#[test]
fn direct_actual_lib_symbol_type_handles_intl_non_generic_alias_bodies() {
    let lib_files = load_lib_files(&["es5.d.ts", "es2020.intl.d.ts"]);
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
        "LocalesArgument",
        "NumberFormatOptionsCurrencyDisplay",
        "NumberFormatOptionsSignDisplay",
        "NumberFormatOptionsStyle",
        "NumberFormatOptionsUseGrouping",
        "UnicodeBCP47LocaleIdentifier",
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .or_else(|| state.resolve_lib_namespace_export_symbol("Intl", name))
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let symbol = state
            .get_cross_file_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should be available"))
            .clone();

        let proof = state
            .direct_actual_lib_type_alias_body(sym_id, &symbol, name, delegate_arena)
            .unwrap_or_else(|| panic!("{name} should have a proven actual-lib alias body"));
        assert_eq!(
            proof.outcome,
            DirectActualLibAliasBodyOutcome::Success,
            "{name} should be admitted by non-generic actual-lib alias proof",
        );
        assert!(
            proof.type_params.is_empty(),
            "{name} should remain non-generic",
        );

        let (direct_ty, direct_params) = state
            .direct_actual_lib_symbol_type(
                sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(delegate_arena),
                false,
            )
            .unwrap_or_else(|| panic!("{name} should lower through direct alias path"));
        assert_ne!(
            direct_ty,
            TypeId::UNKNOWN,
            "{name} should not lower to UNKNOWN"
        );
        assert_ne!(direct_ty, TypeId::ERROR, "{name} should not lower to ERROR");
        assert!(
            direct_params.is_empty(),
            "{name} should stay non-generic on direct path",
        );

        let (fallback_body, fallback_params) = state.compute_type_of_symbol(sym_id);
        assert_eq!(
            direct_ty, fallback_body,
            "{name} direct alias body must match child-checker fallback body",
        );
        assert!(
            fallback_params.is_empty(),
            "{name} fallback should remain non-generic",
        );
    }
}

#[test]
fn direct_actual_lib_symbol_type_handles_readonly_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Readonly")
        .expect("Readonly should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Readonly should lower through the direct alias body path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 1, "Readonly should expose T");

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .get(&sym_id)
        .expect("direct alias path should populate the delegation cache");
    assert_eq!(*cached_ty, ty);
    assert_eq!(
        cached_params.len(),
        params.len(),
        "cache hits must preserve generic alias metadata",
    );

    let (cached_result_ty, cached_result_params) = state
        .delegate_cross_arena_symbol_resolution(sym_id)
        .expect("Readonly cache hit should resolve through lib delegation cache");
    assert_eq!(cached_result_ty, ty);
    assert_eq!(
        cached_result_params.len(),
        params.len(),
        "Readonly cache hits must return the cached alias type params",
    );
}

#[test]
fn direct_actual_lib_alias_proof_matches_mapped_utility_fallback_bodies() {
    let lib_files = load_lib_files(&["es5.d.ts", "es2015.iterable.d.ts", "es2019.array.d.ts"]);
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

    for (name, expected_param_count, expected_outcome) in [
        ("FlatArray", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Awaited", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Exclude", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Extract", 2, DirectActualLibAliasBodyOutcome::Success),
        (
            "IteratorResult",
            2,
            DirectActualLibAliasBodyOutcome::Success,
        ),
        ("Parameters", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Record", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Partial", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Pick", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Required", 1, DirectActualLibAliasBodyOutcome::Success),
        ("ReturnType", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Readonly", 1, DirectActualLibAliasBodyOutcome::Success),
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let symbol = state
            .get_cross_file_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should be available"))
            .clone();

        let proof = state
            .direct_actual_lib_type_alias_body(sym_id, &symbol, name, delegate_arena)
            .unwrap_or_else(|| panic!("{name} should have a proven actual-lib alias body"));
        assert_eq!(proof.outcome, expected_outcome);
        assert_eq!(
            proof.type_params.len(),
            expected_param_count,
            "{name} should expose its declared type params",
        );

        let (fallback_body, fallback_params) = state.compute_type_of_symbol(sym_id);
        assert_eq!(
            fallback_body, proof.body,
            "{name} proof must match the existing child-checker fallback body",
        );
        assert_eq!(
            fallback_params.len(),
            proof.type_params.len(),
            "{name} proof must preserve the same type-parameter arity as fallback",
        );
    }
}
