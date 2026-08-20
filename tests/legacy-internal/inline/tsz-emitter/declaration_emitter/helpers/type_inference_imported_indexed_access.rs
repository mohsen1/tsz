//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_imported_indexed_access.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 44ff2c04614eca0fd362cb058070e191a3a67d71a6d3df47eae8d18c1da889a5 143 package_root_imported_indexed_access_expands_member_annotation
    #[test]
    fn package_root_imported_indexed_access_expands_member_annotation() {
        let mut package_parser = ParserState::new(
            "/project/node_modules/create-emotion-styled/index.d.ts".to_string(),
            r#"
export interface StyledOtherComponentList {
    "div": import("react").DetailedHTMLProps<import("react").HTMLAttributes<HTMLDivElement>, HTMLDivElement>;
}
"#
            .to_string(),
        );
        let package_root = package_parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(&package_parser.arena, package_root);
        let list_sym = binder
            .symbols
            .iter()
            .find(|symbol| symbol.escaped_name == "StyledOtherComponentList")
            .map(|symbol| symbol.id)
            .expect("missing list symbol");
        let package_arena = Arc::new(package_parser.arena.clone());
        let mut symbol_arenas = rustc_hash::FxHashMap::default();
        symbol_arenas.insert(list_sym, Arc::clone(&package_arena));
        binder.symbol_arenas = Arc::new(symbol_arenas);
        let mut exports = SymbolTable::new();
        exports.set("StyledOtherComponentList".to_string(), list_sym);
        let package_path = "/project/node_modules/create-emotion-styled/index.d.ts".to_string();
        let mut module_exports = rustc_hash::FxHashMap::default();
        module_exports.insert(package_path.clone(), exports);
        binder.module_exports = Arc::new(module_exports);

        let mut current_parser = ParserState::new("/project/index.ts".to_string(), String::new());
        let _ = current_parser.parse_source_file();
        let interner = tsz_solver::construction::TypeInterner::new();
        let mut emitter = DeclarationEmitter::with_type_info(
            &current_parser.arena,
            TypeCacheView::default(),
            &interner,
            &binder,
        );
        emitter.current_file_path = Some("/project/index.ts".to_string());
        let mut arena_to_path = rustc_hash::FxHashMap::default();
        arena_to_path.insert(Arc::as_ptr(&package_arena) as usize, package_path);
        emitter.set_arena_to_path(arena_to_path);

        let expanded = emitter
            .expand_imported_indexed_access_type_text(
                r#"import("create-emotion-styled").StyledOtherComponentList["div"]"#,
            )
            .expect("expected package-root indexed access expansion");

        assert_eq!(
            expanded,
            r#"import("react").DetailedHTMLProps<import("react").HTMLAttributes<HTMLDivElement>, HTMLDivElement>"#
        );
    }
// TSZ_INLINE_TEST_END 44ff2c04614eca0fd362cb058070e191a3a67d71a6d3df47eae8d18c1da889a5
