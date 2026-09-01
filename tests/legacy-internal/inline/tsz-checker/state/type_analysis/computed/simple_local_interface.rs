//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/computed/simple_local_interface.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 094cad8536b6e0b45f9d91a2057af7d23d87cf0eaf3b812a778ae09fd5d72c30 567 simple_actual_lib_interface_lowers_bare_lib_type_reference_property
    #[test]
    fn simple_actual_lib_interface_lowers_bare_lib_type_reference_property() {
        let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
        let dom = lib_files
            .iter()
            .find(|lib| {
                lib.arena
                    .source_files
                    .first()
                    .is_some_and(|source_file| source_file.file_name.ends_with("dom.d.ts"))
            })
            .expect("dom lib should be loaded");
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            dom.arena.as_ref(),
            dom.binder.as_ref(),
            &types,
            "dom.d.ts".to_string(),
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
            .get("SVGURIReference")
            .expect("SVGURIReference should be a DOM lib interface");
        assert!(
            state
                .resolve_actual_lib_name_to_def_id_for_lowering("SVGURIReference")
                .is_some()
        );
        let declarations = state
            .ctx
            .binder
            .get_symbol(sym_id)
            .expect("SVGURIReference symbol should exist")
            .declarations
            .clone();

        let interface_type = state
            .try_lower_simple_local_interface_object(
                sym_id,
                &declarations,
                SimpleLocalInterfaceFacts {
                    has_out_of_arena_decl: false,
                    has_cross_file_same_index: false,
                    has_local_interface_decl: true,
                    has_local_interface_heritage_extends: false,
                    has_local_computed_property_name: false,
                    suppress_missing_interface_decl_reject: false,
                    allow_actual_lib_type_references: true,
                },
            )
            .expect("simple DOM interface should lower with a bare lib type reference");
        let href = state.ctx.types.intern_string("href");
        let href_type = raw_property_type(state.ctx.types.as_type_database(), interface_type, href)
            .expect("href property should be present");
        let expected_def_id = state
            .resolve_actual_lib_name_to_def_id_for_lowering("SVGAnimatedString")
            .expect("SVGAnimatedString should have actual-lib identity");

        assert_eq!(
            lazy_def_id(state.ctx.types.as_type_database(), href_type),
            Some(expected_def_id),
        );
    }
// TSZ_INLINE_TEST_END 094cad8536b6e0b45f9d91a2057af7d23d87cf0eaf3b812a778ae09fd5d72c30

// TSZ_INLINE_TEST_BEGIN f724a1660f3d4c4b8e51f46ae0620ba976bb31d0eb0e7dc09310307391360cee 645 simple_actual_lib_interface_does_not_treat_lib_decl_as_source_shadow
    #[test]
    fn simple_actual_lib_interface_does_not_treat_lib_decl_as_source_shadow() {
        let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
        let dom = lib_files
            .iter()
            .find(|lib| {
                lib.arena
                    .source_files
                    .first()
                    .is_some_and(|source_file| source_file.file_name.ends_with("dom.d.ts"))
            })
            .expect("dom lib should be loaded");
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "import './side-effect';\nlet value;".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        assert!(
            binder.is_external_module(),
            "fixture should exercise external-module shadow logic"
        );
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            dom.arena.as_ref(),
            &binder,
            &types,
            "dom.d.ts".to_string(),
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
            .get("SVGURIReference")
            .expect("SVGURIReference should be merged into the source binder");
        assert!(
            state
                .ctx
                .file_local_type_shadow_for_lib_name("SVGAnimatedString"),
            "external-module source binder reports the merged lib declaration as a local shadow"
        );
        let declarations = state
            .ctx
            .binder
            .get_symbol(sym_id)
            .expect("SVGURIReference symbol should exist")
            .declarations
            .clone();

        let interface_type = state
            .try_lower_simple_local_interface_object(
                sym_id,
                &declarations,
                SimpleLocalInterfaceFacts {
                    has_out_of_arena_decl: false,
                    has_cross_file_same_index: false,
                    has_local_interface_decl: true,
                    has_local_interface_heritage_extends: false,
                    has_local_computed_property_name: false,
                    suppress_missing_interface_decl_reject: false,
                    allow_actual_lib_type_references: true,
                },
            )
            .expect("actual-lib declaration should not be rejected as its own source shadow");
        let href = state.ctx.types.intern_string("href");
        let href_type = raw_property_type(state.ctx.types.as_type_database(), interface_type, href)
            .expect("href property should be present");
        let expected_def_id = state
            .resolve_actual_lib_name_to_def_id_for_lowering("SVGAnimatedString")
            .expect("SVGAnimatedString should have actual-lib identity");

        assert_eq!(
            lazy_def_id(state.ctx.types.as_type_database(), href_type),
            Some(expected_def_id),
        );
    }
// TSZ_INLINE_TEST_END f724a1660f3d4c4b8e51f46ae0620ba976bb31d0eb0e7dc09310307391360cee
