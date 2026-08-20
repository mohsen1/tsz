//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/name_resolution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ae52a39b82eb9ae109649f99e371425ecd1ba8376fc2550f1925ead18a1bf7fe 1129 name_resolution_request_constructors
    #[test]
    fn name_resolution_request_constructors() {
        let idx = NodeIndex::NONE;

        let req = NameResolutionRequest::value("foo", idx);
        assert_eq!(req.kind, NameLookupKind::Value);
        assert_eq!(req.name, "foo");
        assert!(req.parent_symbol.is_none());

        let req = NameResolutionRequest::type_ref("Bar", idx);
        assert_eq!(req.kind, NameLookupKind::Type);

        let req = NameResolutionRequest::namespace("ns", idx);
        assert_eq!(req.kind, NameLookupKind::Namespace);
    }
// TSZ_INLINE_TEST_END ae52a39b82eb9ae109649f99e371425ecd1ba8376fc2550f1925ead18a1bf7fe

// TSZ_INLINE_TEST_BEGIN cabe8ef1f36e603759ef77c6ce7c7001bfb8566fe2d297ce59cff424b3b0e4ad 1145 resolution_failure_constructors
    #[test]
    fn resolution_failure_constructors() {
        let f = ResolutionFailure::not_found();
        assert!(!f.has_suggestions());
        assert!(matches!(f.kind, ResolutionFailureKind::NotFound));

        let f = ResolutionFailure::not_found_with_suggestions(vec!["bar".to_string()]);
        assert!(f.has_suggestions());
        assert_eq!(f.suggestions, vec!["bar"]);

        let sym = SymbolId(42);
        let f = ResolutionFailure::wrong_meaning(sym, NameLookupKind::Type);
        assert!(matches!(f.kind, ResolutionFailureKind::WrongMeaning { .. }));

        let f = ResolutionFailure::exported_member_missing(sym, "MyNs".to_string());
        assert!(matches!(
            f.kind,
            ResolutionFailureKind::ExportedMemberMissing { .. }
        ));

        let f = ResolutionFailure::exported_member_missing_with_suggestions(
            sym,
            "MyNs".to_string(),
            vec!["member1".to_string()],
        );
        assert!(f.has_suggestions());

        // Deferred not-found: no eagerly-computed suggestions, but the eligibility
        // flag controls whether the emit path runs the scan.
        let f = ResolutionFailure::not_found_deferred(true);
        assert!(!f.has_suggestions());
        assert!(f.suggestions_eligible);
        assert!(matches!(f.kind, ResolutionFailureKind::NotFound));

        let f = ResolutionFailure::not_found_deferred(false);
        assert!(!f.has_suggestions());
        assert!(!f.suggestions_eligible);
    }
// TSZ_INLINE_TEST_END cabe8ef1f36e603759ef77c6ce7c7001bfb8566fe2d297ce59cff424b3b0e4ad

// TSZ_INLINE_TEST_BEGIN 60e0b70fd82a4239224e814888b31a8e444b79e6321a81b876677500b2b83195 1189 deferred_spelling_scan_still_emits_did_you_mean
    /// The spelling-suggestion scan is deferred to the diagnostic-emission path,
    /// so a genuinely-misspelled user type name still produces the TS2552
    /// "did you mean" elaboration. Binder names are varied to prove the
    /// suggestion is driven by structural edit-distance over the in-scope
    /// symbols, not a fixed candidate.
    #[test]
    fn deferred_spelling_scan_still_emits_did_you_mean() {
        for (decl, typo) in [("Banana", "Banan"), ("Mango", "Mngo")] {
            let src = format!("type {decl} = number;\nlet v: {typo} = 0 as never;\n");
            let diagnostics = check_source_diagnostics(&src);
            let ts2552 = diagnostics.iter().find(|d| d.code == 2552);
            assert!(
                ts2552.is_some_and(|d| d.message_text.contains(decl)),
                "Expected TS2552 suggesting `{decl}` for `{typo}`, got: {:?}",
                diagnostics
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            );
        }
    }
// TSZ_INLINE_TEST_END 60e0b70fd82a4239224e814888b31a8e444b79e6321a81b876677500b2b83195

// TSZ_INLINE_TEST_BEGIN c895009dfa482fd8466d827c663b3eab20915a28cc871fe214bce75aca71d975 1210 deferred_spelling_scan_remains_eligible_in_foreign_current_arena
    /// Deferred spelling-suggestion eligibility follows the diagnostic that will
    /// be emitted, not whether the current arena is part of the user-file arena
    /// set. Selected declaration libs can surface missing-name diagnostics too,
    /// so a foreign/current arena must not pre-disable the later suggestion scan.
    #[test]
    fn deferred_spelling_scan_remains_eligible_in_foreign_current_arena() {
        for (decl, typo) in [
            ("NearbyType", "NearblyType"),
            ("CalendarThing", "CalenderThing"),
        ] {
            let mut user_parser =
                ParserState::new("entry.ts".to_string(), format!("type {decl} = number;\n"));
            let user_root = user_parser.parse_source_file();
            let mut user_binder = BinderState::new();
            user_binder.bind_source_file(user_parser.get_arena(), user_root);

            let mut foreign_parser =
                ParserState::new("selected-lib.d.ts".to_string(), format!("let v: {typo};\n"));
            let foreign_root = foreign_parser.parse_source_file();
            let mut foreign_binder = BinderState::new();
            foreign_binder.bind_source_file(foreign_parser.get_arena(), foreign_root);

            let types = TypeInterner::new();
            let mut checker = CheckerState::new(
                foreign_parser.get_arena(),
                &foreign_binder,
                &types,
                "selected-lib.d.ts".to_string(),
                CheckerOptions::default(),
            );
            checker
                .ctx
                .set_all_arenas(Arc::new(vec![Arc::new(user_parser.get_arena().clone())]));
            checker.ctx.set_lib_contexts(Vec::new());

            assert!(
                checker.suggestion_scan_eligible(typo, foreign_root),
                "foreign/current arena must not suppress deferred suggestions for `{typo}`"
            );
        }
    }
// TSZ_INLINE_TEST_END c895009dfa482fd8466d827c663b3eab20915a28cc871fe214bce75aca71d975

// TSZ_INLINE_TEST_BEGIN 26de791922f9a427faed1e1ff03baf6283ad083a86b271b24ccb411c6d25f702 1253 type_used_as_value_emits_ts2693
    /// TS2693: type used as value — routed through boundary
    #[test]
    fn type_used_as_value_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { x: number; }
const a = Foo;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for interface used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 26de791922f9a427faed1e1ff03baf6283ad083a86b271b24ccb411c6d25f702

// TSZ_INLINE_TEST_BEGIN e87551c66c7088893811070b16ac8b12d459128ec05e3f33386086bada2c3f80 1269 type_alias_used_as_value_emits_ts2693
    /// TS2693: type alias used as value — routed through boundary
    #[test]
    fn type_alias_used_as_value_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
type MyType = string | number;
const a = MyType;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for type alias used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END e87551c66c7088893811070b16ac8b12d459128ec05e3f33386086bada2c3f80

// TSZ_INLINE_TEST_BEGIN ba5fd8a9b988673fcfb91c6eab262de17abc05bd858b3e55373ff55408a7bdae 1285 value_used_as_type_emits_ts2749
    /// TS2749: value used as type — routed through boundary
    #[test]
    fn value_used_as_type_emits_ts2749() {
        let diagnostics = check_source_diagnostics(
            r#"
const myValue = 42;
let x: myValue;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2749),
            "Expected TS2749 for value used as type, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END ba5fd8a9b988673fcfb91c6eab262de17abc05bd858b3e55373ff55408a7bdae

// TSZ_INLINE_TEST_BEGIN 9ae7f5d741c8ea2a933560e93f39ea758b48553f8fe3c0a56eba192bebb87694 1301 namespace_used_as_value_emits_ts2708
    /// TS2708: namespace used as value — routed through boundary
    #[test]
    fn namespace_used_as_value_emits_ts2708() {
        let diagnostics = check_source_diagnostics(
            r#"
namespace MyNs {
    export interface I { x: number; }
}
const a = MyNs;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2708),
            "Expected TS2708 for namespace used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 9ae7f5d741c8ea2a933560e93f39ea758b48553f8fe3c0a56eba192bebb87694

// TSZ_INLINE_TEST_BEGIN 52174fc53a84184ef38bd54e788e5aeb88d3cf48f7f87c0d13f00ff9f862f740 1319 primitive_type_keyword_as_value_emits_ts2693
    /// TS2693: primitive type keyword used as value — routed through boundary
    #[test]
    fn primitive_type_keyword_as_value_emits_ts2693() {
        let diagnostics = check_source_diagnostics("const a = number;");
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for 'number' used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 52174fc53a84184ef38bd54e788e5aeb88d3cf48f7f87c0d13f00ff9f862f740

// TSZ_INLINE_TEST_BEGIN 3a76b2c745eb3e654725be217026a9110465a5e00b7c1267c081745aa46c9349 1330 value_used_as_type_in_type_literal_emits_ts2749
    /// TS2749 in type-literal context: value symbol in type position
    #[test]
    fn value_used_as_type_in_type_literal_emits_ts2749() {
        let diagnostics = check_source_diagnostics(
            r#"
function myFunc() {}
type T = { x: myFunc };
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2749),
            "Expected TS2749 for function used as type, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 3a76b2c745eb3e654725be217026a9110465a5e00b7c1267c081745aa46c9349

// TSZ_INLINE_TEST_BEGIN 6c896840fd42e26614c184b78a391c4a2d0f6e6986eb729155164ccd628fcb99 1346 type_in_new_expression_emits_ts2693
    /// TS2693 in new expression: type used with `new`
    #[test]
    fn type_in_new_expression_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { x: number; }
const a = new Foo();
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for interface in new expression, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 6c896840fd42e26614c184b78a391c4a2d0f6e6986eb729155164ccd628fcb99

// TSZ_INLINE_TEST_BEGIN 8769ed69387f2dfc22a75b03323533269a0e91ad9f44123abafda91ee839e6e1 1362 type_in_assignment_emits_ts2693
    /// TS2693 in assignment: type used in assignment target
    #[test]
    fn type_in_assignment_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { x: number; }
Foo = 42;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for type in assignment, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 8769ed69387f2dfc22a75b03323533269a0e91ad9f44123abafda91ee839e6e1

// TSZ_INLINE_TEST_BEGIN 318b5001f4134405737e99ace2052e3ab1a3a51233af4d22cbdbc3b7a0bb7ac0 1378 unknown_name_in_type_position_emits_ts2304
    /// TS2304 in type position: unknown name in type reference
    #[test]
    fn unknown_name_in_type_position_emits_ts2304() {
        let diagnostics = check_source_diagnostics("let x: NonExistentType;");
        assert!(
            diagnostics.iter().any(|d| d.code == 2304),
            "Expected TS2304 for unknown type name, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 318b5001f4134405737e99ace2052e3ab1a3a51233af4d22cbdbc3b7a0bb7ac0

// TSZ_INLINE_TEST_BEGIN be96cb53fe604bfea0f0154913393d6eb9365beb20693fa930c2113ee6ca1240 1389 namespace_in_extends_clause_emits_ts2708
    /// TS2708: namespace in extends clause — routed through boundary
    #[test]
    fn namespace_in_extends_clause_emits_ts2708() {
        let diagnostics = check_source_diagnostics(
            r#"
namespace NS {
    export interface I {}
}
class C extends NS {}
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2708),
            "Expected TS2708 for namespace in extends, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END be96cb53fe604bfea0f0154913393d6eb9365beb20693fa930c2113ee6ca1240
