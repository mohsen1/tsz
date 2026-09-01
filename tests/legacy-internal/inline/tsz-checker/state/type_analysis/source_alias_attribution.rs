//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/source_alias_attribution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a23a1c692a13210a512b87fcaa4089c15843f83a861a13c22f99f26d1b709da6 824 source_file_alias_type_reference_attribution_resolves_import_alias_target
    #[test]
    fn source_file_alias_type_reference_attribution_resolves_import_alias_target() {
        let (arena, alias_body) = alias_body_from_source("type Box = Alias;");

        let mut binder = BinderState::new();
        let target_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Target".to_string());
        let alias_sym = binder
            .symbols
            .alloc(symbol_flags::ALIAS, "Alias".to_string());
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("alias symbol");
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Target".to_string()));
        binder.file_locals.set("Alias".to_string(), alias_sym);
        let mut exports = SymbolTable::new();
        exports.set("Target".to_string(), target_sym);
        Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

        let kind = type_reference_rejection_kind(&arena, &binder, alias_body, &[]);

        assert_eq!(
            kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasNoArguments,
            "import aliases should be bucketed by resolved type target shape",
        );
        assert_eq!(
            binder.resolution_cache_statistics().export_cache_entries,
            0,
            "attribution must not populate semantic import-resolution caches",
        );
    }
// TSZ_INLINE_TEST_END a23a1c692a13210a512b87fcaa4089c15843f83a861a13c22f99f26d1b709da6

// TSZ_INLINE_TEST_BEGIN 00da029670e00fd3c438e1382315c41f89a61d0f1b2d8a171329579f9c8aaf4b 857 source_file_alias_type_reference_attribution_prefers_shadowing_array_symbol
    #[test]
    fn source_file_alias_type_reference_attribution_prefers_shadowing_array_symbol() {
        let (arena, alias_body) = alias_body_from_source("type Box = Array<string>;");

        let mut binder = BinderState::new();
        let array_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Array".to_string());
        binder.file_locals.set("Array".to_string(), array_sym);

        let kind = type_reference_rejection_kind(&arena, &binder, alias_body, &[]);

        assert_eq!(
            kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments,
            "a local Array symbol should be bucketed by symbol shape, not builtin name",
        );
    }
// TSZ_INLINE_TEST_END 00da029670e00fd3c438e1382315c41f89a61d0f1b2d8a171329579f9c8aaf4b

// TSZ_INLINE_TEST_BEGIN 64840199138a4996514b6e545f7cbb8ac9145a5e450d7acf59e2cea2c84c81f5 876 source_file_alias_type_reference_attribution_resolves_imported_array_symbol
    #[test]
    fn source_file_alias_type_reference_attribution_resolves_imported_array_symbol() {
        let (arena, alias_body) = alias_body_from_source("type Box = Array<string>;");

        let mut binder = BinderState::new();
        let target_sym = binder
            .symbols
            .alloc(symbol_flags::INTERFACE, "Array".to_string());
        let alias_sym = binder
            .symbols
            .alloc(symbol_flags::ALIAS, "Array".to_string());
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("alias symbol");
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Array".to_string()));
        binder.file_locals.set("Array".to_string(), alias_sym);
        let mut exports = SymbolTable::new();
        exports.set("Array".to_string(), target_sym);
        Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

        let kind = type_reference_rejection_kind(&arena, &binder, alias_body, &[]);

        assert_eq!(
            kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalInterfaceWithArguments,
            "an imported Array symbol should resolve before builtin name buckets",
        );
        assert_eq!(
            binder.resolution_cache_statistics().export_cache_entries,
            0,
            "attribution must not populate semantic import-resolution caches",
        );
    }
// TSZ_INLINE_TEST_END 64840199138a4996514b6e545f7cbb8ac9145a5e450d7acf59e2cea2c84c81f5

// TSZ_INLINE_TEST_BEGIN d38273c7394d4fb378494fd05a13ae5b050ba573703360ec70fd4a79af0825ad 909 source_file_alias_type_reference_attribution_walks_composite_bodies
    #[test]
    fn source_file_alias_type_reference_attribution_walks_composite_bodies() {
        let (arena, alias_body) = alias_body_from_source(
            "type Box<T> = T | null;\ntype Item = string;\ntype Result<T> = Box<T> | Item;",
        );
        let mut binder = BinderState::new();
        let box_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Box".to_string());
        let item_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Item".to_string());
        binder.file_locals.set("Box".to_string(), box_sym);
        binder.file_locals.set("Item".to_string(), item_sym);

        let kinds = type_reference_rejection_kinds_in_node(
            &arena,
            &binder,
            alias_body,
            &[String::from("T")],
        );

        assert!(kinds.contains(
            &DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments,
        ));
        assert!(kinds.contains(
            &DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasNoArguments,
        ));
        assert!(kinds.contains(
            &DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeParameter,
        ));
    }
// TSZ_INLINE_TEST_END d38273c7394d4fb378494fd05a13ae5b050ba573703360ec70fd4a79af0825ad

// TSZ_INLINE_TEST_BEGIN bb8d5cfa3f2bd7252648e04829b2e73b6ad081553bae1b0f24678fe84339dde2 942 source_file_alias_type_reference_counts_skip_lowerable_subtrees
    #[test]
    fn source_file_alias_type_reference_counts_skip_lowerable_subtrees() {
        let (arena, binder, alias_body) = bound_alias_body_from_source(
            "type Leaf = string;\ntype Box<T> = T | Leaf;\ntype Result<T> = Array<Box<T>> | Missing<T>;",
        );
        let global_type_is_lowerable = |name: &str| name == "Array";
        let type_param_names = vec![String::from("T")];
        let type_node_is_lowerable = |node_idx| {
            CheckerState::source_file_type_node_is_generic_local_alias_application_lowerable(
                &arena,
                &binder,
                node_idx,
                &type_param_names,
                &global_type_is_lowerable,
            )
        };

        let kinds = non_lowerable_type_reference_rejection_kinds_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        );

        assert_eq!(
            kinds,
            vec![DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier],
            "aggregate rejection counters should skip lowerable helper subtrees",
        );
    }
// TSZ_INLINE_TEST_END bb8d5cfa3f2bd7252648e04829b2e73b6ad081553bae1b0f24678fe84339dde2

// TSZ_INLINE_TEST_BEGIN ca3d44d3436e359e0b9fe6a5f4111d0d6003b919c5a7a88d07d47be699beb150 974 source_file_alias_first_type_reference_attribution_uses_source_order
    #[test]
    fn source_file_alias_first_type_reference_attribution_uses_source_order() {
        let (arena, alias_body) =
            alias_body_from_source("type Box<T> = T | null;\ntype Result<T> = Box<T> | Missing;");
        let mut binder = BinderState::new();
        let box_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Box".to_string());
        binder.file_locals.set("Box".to_string(), box_sym);

        let first = first_type_reference_rejection_kind_in_node(
            &arena,
            &binder,
            alias_body,
            &[String::from("T")],
        )
        .expect("first type reference");

        assert_eq!(
            first,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments,
            "first-reference attribution should classify the first source-order blocker",
        );
    }
// TSZ_INLINE_TEST_END ca3d44d3436e359e0b9fe6a5f4111d0d6003b919c5a7a88d07d47be699beb150

// TSZ_INLINE_TEST_BEGIN 53e94dcab583c0206eba5726b0d75f070ea7fd1c8c8672ab87842f642b3f6baa 999 source_file_alias_non_lowerable_type_reference_skips_lowerable_globals
    #[test]
    fn source_file_alias_non_lowerable_type_reference_skips_lowerable_globals() {
        let (arena, alias_body) = alias_body_from_source("type Result<T> = Array<T> | Missing<T>;");
        let binder = BinderState::new();
        let global_type_is_lowerable = |name: &str| name == "Array";
        let type_param_names = vec![String::from("T")];
        let type_node_is_lowerable = |node_idx| {
            CheckerState::source_file_type_node_is_generic_local_alias_application_lowerable(
                &arena,
                &binder,
                node_idx,
                &type_param_names,
                &global_type_is_lowerable,
            )
        };

        let first = first_non_lowerable_type_reference_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        )
        .expect("first non-lowerable type reference");

        assert_eq!(
            first.kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier,
            "non-lowerable attribution should skip the lowerable Array<T> subtree",
        );
        assert_eq!(first.name, Some("Missing"));
    }
// TSZ_INLINE_TEST_END 53e94dcab583c0206eba5726b0d75f070ea7fd1c8c8672ab87842f642b3f6baa

// TSZ_INLINE_TEST_BEGIN 66497dadd9e50bc2eb439460a70537a888f32f9dc374bfed619f4f9e85fa3ba1 1032 source_file_alias_non_lowerable_leaf_type_reference_descends_into_outer_failure
    #[test]
    fn source_file_alias_non_lowerable_leaf_type_reference_descends_into_outer_failure() {
        let (arena, alias_body) =
            alias_body_from_source("type Result<T> = Pick<T, Missing<keyof T>>;");
        let binder = BinderState::new();
        let global_type_is_lowerable = |name: &str| name == "Pick";
        let type_param_names = vec![String::from("T")];
        let type_node_is_lowerable = |node_idx| {
            CheckerState::source_file_type_node_is_generic_local_alias_application_lowerable(
                &arena,
                &binder,
                node_idx,
                &type_param_names,
                &global_type_is_lowerable,
            )
        };

        let first = first_non_lowerable_type_reference_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        )
        .expect("first non-lowerable type reference");
        let leaf = first_non_lowerable_leaf_type_reference_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        )
        .expect("first non-lowerable leaf type reference");

        assert_eq!(first.name, Some("Pick"));
        assert_eq!(
            leaf.kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier,
            "leaf attribution should identify the nested type reference that makes Pick fail",
        );
        assert_eq!(leaf.name, Some("Missing"));
    }
// TSZ_INLINE_TEST_END 66497dadd9e50bc2eb439460a70537a888f32f9dc374bfed619f4f9e85fa3ba1
