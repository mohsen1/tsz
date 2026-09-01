//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/module_augmentation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1b109917267a8a04fd86c9447c335d533aa488c94b7d8975902dfb5eb33d2f68 1766 augmentation_member_key_name_resolves_identifier_string_and_computed_const
    /// `augmentation_member_key_name_in_arena` resolves identifier, string-literal,
    /// and computed string-`const` property keys, and leaves an unresolvable
    /// computed key as `None`. The const resolver is renamed (`TAG`, not a
    /// hard-coded fp-ts name) to lock the structural, name-independent contract.
    #[test]
    fn augmentation_member_key_name_resolves_identifier_string_and_computed_const() {
        let source = r#"
interface I {
    foo: number;
    "bar": number;
    [TAG]: number;
    [OTHER]: number;
}
"#;
        let mut parser = ParserState::new("t.ts".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());
        let sf = arena.source_files.first().expect("source file");
        let iface_idx = sf
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&idx| {
                arena
                    .get(idx)
                    .and_then(|n| arena.get_interface(n))
                    .is_some()
            })
            .expect("interface node");
        let iface = arena
            .get_interface(arena.get(iface_idx).expect("iface node"))
            .expect("interface data");

        let resolved: Vec<Option<String>> = iface
            .members
            .nodes
            .iter()
            .copied()
            .filter_map(|member_idx| {
                let member = arena.get(member_idx)?;
                let sig = arena.get_signature(member)?;
                Some(CheckerState::augmentation_member_key_name_in_arena(
                    &arena,
                    sig.name,
                    // Only `TAG` is a known string const; `OTHER` is unknown.
                    |name| (name == "TAG").then(|| "computed_tag".to_string()),
                ))
            })
            .collect();

        assert_eq!(
            resolved,
            vec![
                Some("foo".to_string()),
                Some("bar".to_string()),
                Some("computed_tag".to_string()),
                None,
            ],
            "key resolver must handle identifier, string-literal, and computed \
             string-const keys, and drop unresolvable computed keys"
        );
    }
// TSZ_INLINE_TEST_END 1b109917267a8a04fd86c9447c335d533aa488c94b7d8975902dfb5eb33d2f68

// TSZ_INLINE_TEST_BEGIN 63136e5d54db0a56e6e8fdc056bd9c3363f3ea6b4c40e0d95615a53f3c36f524 1826 module_augmentation_has_type_params_detects_type_alias_with_params
    #[test]
    fn module_augmentation_has_type_params_detects_type_alias_with_params() {
        // Set up a binder with a module augmentation that has a generic type alias.
        let mut binder = BinderState::new();
        let aug_name = "Row2".to_string();

        // Parse a type alias `type Row2<T> = {}` to get a node with type params.
        let source = "type Row2<T> = {}";
        let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());

        // Find the type alias declaration node
        let sf = arena.source_files.first().expect("source file");
        let type_alias_node = sf
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&idx| {
                arena
                    .get(idx)
                    .and_then(|n| arena.get_type_alias(n))
                    .is_some()
            })
            .expect("type alias node");

        // Register a module augmentation with the arena
        let aug = ModuleAugmentation::with_arena(aug_name, type_alias_node, Arc::clone(&arena));
        Arc::get_mut(&mut binder.module_augmentations)
            .expect("fresh Arc")
            .insert(".".to_string(), vec![aug]);

        // Set up CheckerState with the binder
        let types = tsz_solver::construction::TypeInterner::new();
        let main_arena = Arc::new(NodeArena::new());
        let checker = CheckerState::new(
            &main_arena,
            &binder,
            &types,
            "test.ts".to_string(),
            Default::default(),
        );

        assert!(
            checker.module_augmentation_has_type_params(".", "Row2"),
            "Should detect type params in module augmentation for '.' Row2"
        );
        assert!(
            !checker.module_augmentation_has_type_params(".", "Nonexistent"),
            "Should not detect type params for non-existent name"
        );
        assert!(
            !checker.module_augmentation_has_type_params("./other", "Row2"),
            "Should not detect type params for non-matching module specifier"
        );
    }
// TSZ_INLINE_TEST_END 63136e5d54db0a56e6e8fdc056bd9c3363f3ea6b4c40e0d95615a53f3c36f524

// TSZ_INLINE_TEST_BEGIN 741de3eae6caf8eb2ca67517db7eb0520fe8a04a2e65ca3b93ee22df742e9fed 1884 module_augmentation_has_type_params_rejects_non_generic_interface
    #[test]
    fn module_augmentation_has_type_params_rejects_non_generic_interface() {
        let mut binder = BinderState::new();
        let aug_name = "Foo".to_string();

        let source = "interface Foo {}";
        let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());

        let sf = arena.source_files.first().expect("source file");
        let iface_node = sf
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&idx| {
                arena
                    .get(idx)
                    .and_then(|n| arena.get_interface(n))
                    .is_some()
            })
            .expect("interface node");

        let aug = ModuleAugmentation::with_arena(aug_name, iface_node, Arc::clone(&arena));
        Arc::get_mut(&mut binder.module_augmentations)
            .expect("fresh Arc")
            .insert(".".to_string(), vec![aug]);

        let types = tsz_solver::construction::TypeInterner::new();
        let main_arena = Arc::new(NodeArena::new());
        let checker = CheckerState::new(
            &main_arena,
            &binder,
            &types,
            "test.ts".to_string(),
            Default::default(),
        );

        assert!(
            !checker.module_augmentation_has_type_params(".", "Foo"),
            "Should NOT detect type params for non-generic interface"
        );
    }
// TSZ_INLINE_TEST_END 741de3eae6caf8eb2ca67517db7eb0520fe8a04a2e65ca3b93ee22df742e9fed
