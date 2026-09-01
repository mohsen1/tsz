//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking_members/overload_compatibility.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7ee827f035f6c076125e3165f301a2867abd216f7bb77c2c6f8a7db6d6bbcfa0 1464 ts2394_cross_file_unresolved_span_continues_to_later_resolvable_overload
    #[test]
    fn ts2394_cross_file_unresolved_span_continues_to_later_resolvable_overload() {
        let source = r#"
function parseArg(x: string): string;
function parseArg(x: boolean): boolean;
function parseArg(x: number): string {
    return "ok";
}
"#;

        let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let arena = Arc::new(parser.get_arena().clone());
        let types = TypeInterner::new();
        let parse_arg = binder
            .file_locals
            .get("parseArg")
            .unwrap_or_else(|| panic!("fixture symbol parseArg should exist"));
        let overloads = overload_decls_for_symbol(arena.as_ref(), parse_arg, &binder);
        assert!(
            overloads.len() >= 2,
            "fixture should have at least two overload signatures"
        );

        let baseline = diagnostics_for(&arena, &binder, root, &types);
        let baseline_ts2394 = baseline.iter().filter(|d| d.code == 2394).count();
        assert_eq!(
            baseline_ts2394, 1,
            "intra-file overload mismatch should report one TS2394 before declaration-arena injection, got: {baseline:?}",
        );

        let mut synthetic_arena = (*arena).clone();
        synthetic_arena.source_files.clear();
        Arc::make_mut(&mut binder.declaration_arenas).insert(
            (parse_arg, overloads[0]),
            smallvec![Arc::new(synthetic_arena)],
        );

        let injected = diagnostics_for(&arena, &binder, root, &types);
        let ts2394: Vec<_> = injected.iter().filter(|d| d.code == 2394).collect();
        assert_eq!(
            ts2394.len(),
            1,
            "unresolvable first overload span should be suppressed, then the later resolvable overload should still report TS2394; got: {injected:?}",
        );

        let second_overload_start = source
            .find("parseArg(x: boolean)")
            .expect("find second overload name") as u32;
        let impl_start = source
            .find("parseArg(x: number)")
            .expect("find implementation name") as u32;
        let diagnostic_start = ts2394[0].start;
        assert!(
            diagnostic_start >= second_overload_start && diagnostic_start < impl_start,
            "TS2394 should anchor to the later resolvable overload, not the implementation; start={diagnostic_start}, second_overload_start={second_overload_start}, impl_start={impl_start}"
        );
    }
// TSZ_INLINE_TEST_END 7ee827f035f6c076125e3165f301a2867abd216f7bb77c2c6f8a7db6d6bbcfa0
