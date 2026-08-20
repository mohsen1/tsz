//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/types/printer/symbol_resolution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b7688e100fcab8563536eac84d3d64d2f940ed8ef12f314ac27b4e950397b2de 1478 module_local_qualified_name_does_not_use_colliding_global_root
    #[test]
    fn module_local_qualified_name_does_not_use_colliding_global_root() {
        let interner = TypeInterner::new();
        let mut arena = SymbolArena::new();

        let _global_a = arena.alloc(symbol_flags::NAMESPACE, "A".to_string());
        let module_symbol = arena.alloc(symbol_flags::MODULE, "\"pkg\"".to_string());
        let module_a = arena.alloc(symbol_flags::NAMESPACE, "A".to_string());
        let module_b = arena.alloc(symbol_flags::INTERFACE, "B".to_string());

        arena.get_mut(module_a).unwrap().parent = module_symbol;
        let module_b_symbol = arena.get_mut(module_b).unwrap();
        module_b_symbol.parent = module_a;
        module_b_symbol.is_exported = true;

        let module_path = |sym_id: SymbolId| {
            (sym_id == module_b || sym_id == module_a).then(|| "pkg".to_string())
        };

        let printer = TypePrinter::new(&interner)
            .with_symbols(&arena)
            .with_module_path_resolver(&module_path);

        assert_eq!(
            printer
                .print_named_symbol_reference(module_b, false)
                .as_deref(),
            Some(r#"import("pkg").A.B"#)
        );
    }
// TSZ_INLINE_TEST_END b7688e100fcab8563536eac84d3d64d2f940ed8ef12f314ac27b4e950397b2de

// TSZ_INLINE_TEST_BEGIN f865116f7dfd7c45940adc4928926eaeefa9272555060dde1a7fd57466557cad 1512 top_level_module_type_with_none_parent_uses_import_qualifier
    // When a top-level type alias in a module has parent == SymbolId::NONE (no binder
    // parent assigned), it must NOT be treated as globally accessible. The printer must
    // produce `import("./module").TypeName` so that TS7056 detection can find it.
    #[test]
    fn top_level_module_type_with_none_parent_uses_import_qualifier() {
        let interner = TypeInterner::new();
        let mut arena = SymbolArena::new();

        // Simulates `type TPromise<T, E> = ...` in http-client.ts
        // parent stays SymbolId::NONE (as tsz binder sets for top-level decls)
        let t_promise = arena.alloc(symbol_flags::TYPE_ALIAS, "TPromise".to_string());

        let module_path =
            |sym_id: SymbolId| (sym_id == t_promise).then(|| "./http-client".to_string());

        let printer = TypePrinter::new(&interner)
            .with_symbols(&arena)
            .with_module_path_resolver(&module_path);

        assert_eq!(
            printer
                .print_named_symbol_reference(t_promise, false)
                .as_deref(),
            Some(r#"import("./http-client").TPromise"#),
            "private type alias with None parent must use import() qualifier, not bare name"
        );
    }
// TSZ_INLINE_TEST_END f865116f7dfd7c45940adc4928926eaeefa9272555060dde1a7fd57466557cad

// TSZ_INLINE_TEST_BEGIN b80596410ccbe4226567b24db65d8ff067c8768bd1a3abc99ba3454762c95b3e 1539 top_level_module_type_different_name_uses_import_qualifier
    // Same requirement under a different name: structural rule must not depend on
    // the specific identifier spelling.
    #[test]
    fn top_level_module_type_different_name_uses_import_qualifier() {
        let interner = TypeInterner::new();
        let mut arena = SymbolArena::new();

        let request_state = arena.alloc(symbol_flags::TYPE_ALIAS, "RequestState".to_string());

        let module_path =
            |sym_id: SymbolId| (sym_id == request_state).then(|| "./client".to_string());

        let printer = TypePrinter::new(&interner)
            .with_symbols(&arena)
            .with_module_path_resolver(&module_path);

        assert_eq!(
            printer
                .print_named_symbol_reference(request_state, false)
                .as_deref(),
            Some(r#"import("./client").RequestState"#),
        );
    }
// TSZ_INLINE_TEST_END b80596410ccbe4226567b24db65d8ff067c8768bd1a3abc99ba3454762c95b3e

// TSZ_INLINE_TEST_BEGIN 7c74423aff1bd099403b6eb01bce243fa2d3c68e50fc3926e4edfe489aaaabfd 1563 truly_global_root_allows_bare_qualified_name
    // A truly global symbol (parent == NONE, no module path) must still be
    // printable by bare name when encountered as the root of a qualified ref.
    #[test]
    fn truly_global_root_allows_bare_qualified_name() {
        let interner = TypeInterner::new();
        let mut arena = SymbolArena::new();

        // Global namespace NS (parent stays NONE, no module path)
        let ns = arena.alloc(symbol_flags::NAMESPACE, "NS".to_string());
        // NS.T is exported from some module but NS itself is global
        let t = arena.alloc(symbol_flags::INTERFACE, "T".to_string());
        arena.get_mut(t).unwrap().parent = ns;
        arena.get_mut(t).unwrap().is_exported = true;

        // T has a module path but NS (the root) does not
        let module_path = |sym_id: SymbolId| (sym_id == t).then(|| "./lib".to_string());

        let printer = TypePrinter::new(&interner)
            .with_symbols(&arena)
            .with_module_path_resolver(&module_path);

        // NS has no module path → qualified_name_has_non_module_global_root returns true
        // → printer uses bare "NS.T"
        assert_eq!(
            printer.print_named_symbol_reference(t, false).as_deref(),
            Some("NS.T"),
            "when the root is a true global (no module path), bare qualified name is allowed"
        );
    }
// TSZ_INLINE_TEST_END 7c74423aff1bd099403b6eb01bce243fa2d3c68e50fc3926e4edfe489aaaabfd
