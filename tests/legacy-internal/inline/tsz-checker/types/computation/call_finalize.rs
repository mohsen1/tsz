//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/computation/call_finalize.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6835f1ef1fe2fbd70b8f4975ff0b583373a989c10b09d4dc4edacff9971eafc8 921 declared_type_of_identifier_argument_resolves_cross_file_stable_declaration
    #[test]
    fn declared_type_of_identifier_argument_resolves_cross_file_stable_declaration() {
        let files = [
            (
                "consumer.ts",
                r#"
declare function f<T>(...items: T[]): T;
f(data, { a: 2 });
"#,
            ),
            (
                "shared.ts",
                r#"
declare let data: { a: 1, b: "abc", c: true };
"#,
            ),
        ];

        let mut arenas = Vec::with_capacity(files.len());
        let mut binders = Vec::with_capacity(files.len());
        let mut roots = Vec::with_capacity(files.len());
        let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();
        for (file_idx, (name, source)) in files.iter().enumerate() {
            let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.set_file_idx(file_idx as u32);
            binder.bind_source_file(parser.get_arena(), root);
            arenas.push(Arc::new(parser.get_arena().clone()));
            binders.push(Arc::new(binder));
            roots.push(root);
        }

        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            all_arenas[0].as_ref(),
            all_binders[0].as_ref(),
            &types,
            file_names[0].clone(),
            CheckerOptions::default(),
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(0);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);

        checker.check_source_file(roots[0]);

        let data_arg_idx = checker
            .ctx
            .arena
            .nodes
            .iter()
            .find_map(|node| {
                if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return None;
                }
                let call = checker.ctx.arena.get_call_expr(node)?;
                let callee_node = checker.ctx.arena.get(call.expression)?;
                let callee_ident = checker.ctx.arena.get_identifier(callee_node)?;
                if callee_ident.escaped_text != "f" {
                    return None;
                }
                call.arguments.as_ref()?.nodes.first().copied()
            })
            .expect("expected to find f(data, ...) call in consumer.ts");

        let declared = checker.declared_type_of_identifier_argument(data_arg_idx);
        assert!(
            declared.is_some(),
            "cross-file typed identifier argument should resolve a declared type"
        );
    }
// TSZ_INLINE_TEST_END 6835f1ef1fe2fbd70b8f4975ff0b583373a989c10b09d4dc4edacff9971eafc8
