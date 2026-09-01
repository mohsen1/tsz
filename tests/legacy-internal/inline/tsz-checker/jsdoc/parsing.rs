//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/jsdoc/parsing.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8b8b475ef9af4c4ae1c91fa0666a2efbef1fe68c959008b3676edbb61703bf33 1401 import_named_alias_with_space
    #[test]
    fn import_named_alias_with_space() {
        let imports = parse("{ Foo as LocalFoo } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string(),
            )]
        );
    }
// TSZ_INLINE_TEST_END 8b8b475ef9af4c4ae1c91fa0666a2efbef1fe68c959008b3676edbb61703bf33

// TSZ_INLINE_TEST_BEGIN 7477a1a9ef0bccdd45b66c5cf21770f63e12174bedbc40319be89c7801b79615 1414 import_named_alias_with_tab
    #[test]
    fn import_named_alias_with_tab() {
        let imports = parse("{ Foo as\tLocalFoo } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string(),
            )]
        );
    }
// TSZ_INLINE_TEST_END 7477a1a9ef0bccdd45b66c5cf21770f63e12174bedbc40319be89c7801b79615

// TSZ_INLINE_TEST_BEGIN 9659f4137c094064d82b3473047b283e39f6f046647ae4af63b8fd7a71d068da 1427 import_named_alias_with_mixed_whitespace
    #[test]
    fn import_named_alias_with_mixed_whitespace() {
        let imports = parse("{ Foo \tas \tLocalFoo } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string(),
            )]
        );
    }
// TSZ_INLINE_TEST_END 9659f4137c094064d82b3473047b283e39f6f046647ae4af63b8fd7a71d068da

// TSZ_INLINE_TEST_BEGIN f7212376e8b4b2e4903d142193096263a96885c6dd42e157db68abee3c3d6481 1440 import_namespace_alias_with_tab
    #[test]
    fn import_namespace_alias_with_tab() {
        let imports = parse("*\tas\tNS from \"./dep\"");
        assert_eq!(
            imports,
            vec![("NS".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }
// TSZ_INLINE_TEST_END f7212376e8b4b2e4903d142193096263a96885c6dd42e157db68abee3c3d6481

// TSZ_INLINE_TEST_BEGIN c88211d7c8c8c62f7a8e588df866bf74b7c9fb9627ddef887adf02144259e418 1449 import_namespace_alias_with_space
    #[test]
    fn import_namespace_alias_with_space() {
        let imports = parse("* as NS from \"./dep\"");
        assert_eq!(
            imports,
            vec![("NS".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }
// TSZ_INLINE_TEST_END c88211d7c8c8c62f7a8e588df866bf74b7c9fb9627ddef887adf02144259e418

// TSZ_INLINE_TEST_BEGIN fc9e8312c3b9c4176463083c483645837697c3d1b836ae58ede97f81c46a06eb 1458 import_named_no_alias_unchanged
    #[test]
    fn import_named_no_alias_unchanged() {
        let imports = parse("{ Foo, Bar } from \"./dep\"");
        assert_eq!(
            imports,
            vec![
                ("Foo".to_string(), "./dep".to_string(), "Foo".to_string(),),
                ("Bar".to_string(), "./dep".to_string(), "Bar".to_string(),),
            ]
        );
    }
// TSZ_INLINE_TEST_END fc9e8312c3b9c4176463083c483645837697c3d1b836ae58ede97f81c46a06eb

// TSZ_INLINE_TEST_BEGIN 6969a3203dd228b9a32212426ea9002a88c4d71c925f52cc5a24a37b09fcc6db 1470 import_named_alias_does_not_match_inside_identifier
    #[test]
    fn import_named_alias_does_not_match_inside_identifier() {
        // `Class` contains the substring "as" but is not a renaming.
        let imports = parse("{ Class } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "Class".to_string(),
                "./dep".to_string(),
                "Class".to_string(),
            )]
        );
    }
// TSZ_INLINE_TEST_END 6969a3203dd228b9a32212426ea9002a88c4d71c925f52cc5a24a37b09fcc6db

// TSZ_INLINE_TEST_BEGIN ea62f94968f0971d0af0507a48a88ca82b192c8c15140a4fe6acba0f72984acf 1484 import_named_alias_with_identifier_containing_as
    #[test]
    fn import_named_alias_with_identifier_containing_as() {
        let imports = parse("{ Class as Klass } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "Klass".to_string(),
                "./dep".to_string(),
                "Class".to_string(),
            )]
        );
    }
// TSZ_INLINE_TEST_END ea62f94968f0971d0af0507a48a88ca82b192c8c15140a4fe6acba0f72984acf

// TSZ_INLINE_TEST_BEGIN 5a18f1008f5f380680658bca2571449ead46776139d95ef758180a25f1b5e0d9 1497 import_default_unchanged
    #[test]
    fn import_default_unchanged() {
        let imports = parse("Foo from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "Foo".to_string(),
                "./dep".to_string(),
                "default".to_string(),
            )]
        );
    }
// TSZ_INLINE_TEST_END 5a18f1008f5f380680658bca2571449ead46776139d95ef758180a25f1b5e0d9

// TSZ_INLINE_TEST_BEGIN a6c707ac2986661484a4bcf170a641305bdfb594d72332e200a3d5ecc90e1e56 1521 jsdoc_contains_tag_rejects_longer_identifier
    #[test]
    fn jsdoc_contains_tag_rejects_longer_identifier() {
        for tag in [
            "satisfies",
            "import",
            "override",
            "this",
            "typedef",
            "callback",
            "constructor",
        ] {
            let mismatched = format!("/** @{tag}x foo */");
            assert!(
                !CheckerState::jsdoc_contains_tag(&mismatched, tag),
                "@{tag}x must not be treated as @{tag} (input: {mismatched:?})"
            );
            let real = format!("/** @{tag} foo */");
            assert!(
                CheckerState::jsdoc_contains_tag(&real, tag),
                "@{tag} must still be detected (input: {real:?})"
            );
        }
    }
// TSZ_INLINE_TEST_END a6c707ac2986661484a4bcf170a641305bdfb594d72332e200a3d5ecc90e1e56

// TSZ_INLINE_TEST_BEGIN 241a8acbbe53acab83f7490d66d580cd9adffc2f9f6d29adb7c56ac269adf35b 1545 jsdoc_contains_tag_treats_underscore_suffix_as_distinct_tag
    #[test]
    fn jsdoc_contains_tag_treats_underscore_suffix_as_distinct_tag() {
        assert!(!CheckerState::jsdoc_contains_tag(
            "/** @typedef_internal {{ a }} */",
            "typedef"
        ));
        assert!(CheckerState::jsdoc_contains_tag(
            "/** @typedef\n@template T */",
            "typedef"
        ));
    }
// TSZ_INLINE_TEST_END 241a8acbbe53acab83f7490d66d580cd9adffc2f9f6d29adb7c56ac269adf35b

// TSZ_INLINE_TEST_BEGIN 632249e7ba7e3ee7fdc997c5e6fa01823d561c6a9c493af7e9939150f681632f 1557 jsdoc_tag_offset_skips_longer_match_and_finds_real_tag
    #[test]
    fn jsdoc_tag_offset_skips_longer_match_and_finds_real_tag() {
        let jsdoc = "/** @satisfiesx not a tag\n * @satisfies {Foo} */";
        let pos = CheckerState::jsdoc_tag_offset(jsdoc, "satisfies")
            .expect("real @satisfies tag must be located");
        assert_eq!(&jsdoc[pos..pos + "@satisfies".len()], "@satisfies");
        let after = &jsdoc[pos + "@satisfies".len()..];
        assert!(
            after.starts_with(' '),
            "boundary must be reached, got rest = {after:?}"
        );
    }
// TSZ_INLINE_TEST_END 632249e7ba7e3ee7fdc997c5e6fa01823d561c6a9c493af7e9939150f681632f

// TSZ_INLINE_TEST_BEGIN 2335eb9ea0867b96886e5f63e0d8f7fa387df7c8edad08d0a07d1567802df936 1570 jsdoc_tag_offsets_only_returns_real_tag_positions
    #[test]
    fn jsdoc_tag_offsets_only_returns_real_tag_positions() {
        let jsdoc = "@satisfiesx skip me\n@satisfies a\n@satisfies b\n@satisfiesy nope\n";
        let offsets = CheckerState::jsdoc_tag_offsets(jsdoc, "satisfies");
        assert_eq!(offsets.len(), 2);
        for off in &offsets {
            let after = &jsdoc[off + "@satisfies".len()..];
            let next = after.chars().next().unwrap_or(' ');
            assert!(
                !next.is_ascii_alphanumeric() && next != '_',
                "expected boundary at offset {off}, found {after:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END 2335eb9ea0867b96886e5f63e0d8f7fa387df7c8edad08d0a07d1567802df936

// TSZ_INLINE_TEST_BEGIN 7234f9f35bdc71cd54bcca3fb3fc91fd7ad609b2646324dae73d316300b0393a 1585 strip_jsdoc_tag_prefix_rejects_longer_identifiers
    #[test]
    fn strip_jsdoc_tag_prefix_rejects_longer_identifiers() {
        for tag in ["import", "typedef", "template", "param"] {
            let mismatched = format!("@{tag}x foo");
            assert!(
                CheckerState::strip_jsdoc_tag_prefix(&mismatched, tag).is_none(),
                "@{tag}x must not strip as @{tag} (input: {mismatched:?})"
            );
            let real = format!("@{tag} foo");
            assert_eq!(
                CheckerState::strip_jsdoc_tag_prefix(&real, tag),
                Some(" foo"),
                "@{tag} must strip with the trailing rest preserved"
            );
            // Bare tag with no trailing characters (end of input is a boundary).
            let bare = format!("@{tag}");
            assert_eq!(CheckerState::strip_jsdoc_tag_prefix(&bare, tag), Some(""));
        }
    }
// TSZ_INLINE_TEST_END 7234f9f35bdc71cd54bcca3fb3fc91fd7ad609b2646324dae73d316300b0393a

// TSZ_INLINE_TEST_BEGIN 87a3cc97cba8a11cca2ed41eb9a8875aaeda8c217d72f84f030d1bdcd960359c 1605 jsdoc_line_starts_with_tag_handles_boundaries
    #[test]
    fn jsdoc_line_starts_with_tag_handles_boundaries() {
        assert!(CheckerState::jsdoc_line_starts_with_tag(
            "@typedef {{a: number}} Foo",
            "typedef"
        ));
        assert!(!CheckerState::jsdoc_line_starts_with_tag(
            "@typedefx {{a: number}} Foo",
            "typedef"
        ));
        assert!(!CheckerState::jsdoc_line_starts_with_tag(
            "@typedef_inner",
            "typedef"
        ));
        assert!(CheckerState::jsdoc_line_starts_with_tag(
            "@typedef\trest",
            "typedef"
        ));
    }
// TSZ_INLINE_TEST_END 87a3cc97cba8a11cca2ed41eb9a8875aaeda8c217d72f84f030d1bdcd960359c

// TSZ_INLINE_TEST_BEGIN 2605fb508d34ce5ea3be2904877b9b1bf9d7ee8287816236fac332db4738ea6b 1625 extract_jsdoc_satisfies_expression_ignores_longer_prefix
    #[test]
    fn extract_jsdoc_satisfies_expression_ignores_longer_prefix() {
        // `@satisfiesx {Foo}` must not be parsed as `@satisfies {Foo}`.
        let bogus = "/** @satisfiesx {Foo} */";
        assert!(CheckerState::extract_jsdoc_satisfies_expression(bogus).is_none());
        let real = "/** @satisfies {Foo} */";
        assert_eq!(
            CheckerState::extract_jsdoc_satisfies_expression(real),
            Some("Foo")
        );
    }
// TSZ_INLINE_TEST_END 2605fb508d34ce5ea3be2904877b9b1bf9d7ee8287816236fac332db4738ea6b

// TSZ_INLINE_TEST_BEGIN 2a33e8cc9490d46f624ed2ed0d1a7c38884bedb4b2d0e65cb4de7c452d79a164 1637 parse_jsdoc_typedefs_ignores_typedefx_and_importx
    #[test]
    fn parse_jsdoc_typedefs_ignores_typedefx_and_importx() {
        // `@typedefx` must not register a typedef under the name following the
        // bogus tag.
        let typedefs = CheckerState::parse_jsdoc_typedefs("@typedefx {{ n: number }} Foo\n");
        assert_eq!(
            typedefs.len(),
            0,
            "expected no typedefs from @typedefx, got {} entries",
            typedefs.len()
        );

        // `@importx { Foo } from "./types"` must not create an import alias.
        let imports = CheckerState::parse_jsdoc_typedefs("@importx { Foo } from \"./types\"\n");
        assert_eq!(
            imports.len(),
            0,
            "expected no imports from @importx, got {} entries",
            imports.len()
        );

        // The real `@typedef` and `@import` must still be handled.
        let typedefs = CheckerState::parse_jsdoc_typedefs("@typedef {{ n: number }} Foo\n");
        assert_eq!(typedefs.len(), 1);
        assert_eq!(typedefs[0].0, "Foo");

        let imports = CheckerState::parse_jsdoc_typedefs("@import { Foo } from \"./types\"\n");
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].0, "Foo");
    }
// TSZ_INLINE_TEST_END 2a33e8cc9490d46f624ed2ed0d1a7c38884bedb4b2d0e65cb4de7c452d79a164

// TSZ_INLINE_TEST_BEGIN d9099bf73680d0c3d7b3cd35ed58227a94ac3a3ee13e5b297a30d049d1ee985c 1668 parse_jsdoc_callback_preserves_nested_object_return_type
    #[test]
    fn parse_jsdoc_callback_preserves_nested_object_return_type() {
        let typedefs = CheckerState::parse_jsdoc_typedefs(
            "\
@callback MakeBox
@returns {{ value: string }}
",
        );

        assert_eq!(typedefs.len(), 1);
        assert_eq!(typedefs[0].0, "MakeBox");
        let callback = typedefs[0].1.callback.as_ref().expect("callback parsed");
        assert_eq!(callback.return_type.as_deref(), Some("{ value: string }"));
    }
// TSZ_INLINE_TEST_END d9099bf73680d0c3d7b3cd35ed58227a94ac3a3ee13e5b297a30d049d1ee985c

// TSZ_INLINE_TEST_BEGIN f2f72448619f638558275526fbcb9ff721550feaa57d052ab71759b6bf61e852 1692 named_alias_with_space
    #[test]
    fn named_alias_with_space() {
        let got = parse(r#" { Foo as LocalFoo } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END f2f72448619f638558275526fbcb9ff721550feaa57d052ab71759b6bf61e852

// TSZ_INLINE_TEST_BEGIN 4fe2230fe3950c52bdf58974fb36aa1d59ccda3f8e6e5dea9cc984ac28eb2b1e 1705 named_alias_with_tab_after_as
    #[test]
    fn named_alias_with_tab_after_as() {
        let got = parse("\t{ Foo as\tLocalFoo } from \"./dep\"");
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END 4fe2230fe3950c52bdf58974fb36aa1d59ccda3f8e6e5dea9cc984ac28eb2b1e

// TSZ_INLINE_TEST_BEGIN 8b45b5662f9ba51a2f5a833bbd5eea5e5cb4c395ffa4495f1bd8f1b59ee82b3f 1718 named_alias_with_tab_before_and_after_as
    #[test]
    fn named_alias_with_tab_before_and_after_as() {
        let got = parse("{ Foo\tas\tLocalFoo } from \"./dep\"");
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END 8b45b5662f9ba51a2f5a833bbd5eea5e5cb4c395ffa4495f1bd8f1b59ee82b3f

// TSZ_INLINE_TEST_BEGIN bc6f4470b04a6d13717d2438f47d7be9dc700f4e0048872786a7c13856fadd02 1731 named_alias_with_multiple_spaces
    #[test]
    fn named_alias_with_multiple_spaces() {
        let got = parse(r#"{ Foo  as  LocalFoo } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END bc6f4470b04a6d13717d2438f47d7be9dc700f4e0048872786a7c13856fadd02

// TSZ_INLINE_TEST_BEGIN a070c121d2ba74df0c5683590aee33d8b3197044c8f74b19ea45163c57190949 1744 named_without_alias
    #[test]
    fn named_without_alias() {
        let got = parse(r#"{ Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![("Foo".to_string(), "./dep".to_string(), "Foo".to_string())]
        );
    }
// TSZ_INLINE_TEST_END a070c121d2ba74df0c5683590aee33d8b3197044c8f74b19ea45163c57190949

// TSZ_INLINE_TEST_BEGIN 9c8acac84333874ce1aac6693881dc39707a67e0dbec7a74c9cdb5712ebd849e 1753 named_alias_string_literal_export_names
    #[test]
    fn named_alias_string_literal_export_names() {
        let got =
            parse(r#"{ "a,b" as CommaName, "as" as AsName, "from" as FromName } from "./dep""#);
        assert_eq!(
            got,
            vec![
                (
                    "CommaName".to_string(),
                    "./dep".to_string(),
                    "a,b".to_string()
                ),
                ("AsName".to_string(), "./dep".to_string(), "as".to_string()),
                (
                    "FromName".to_string(),
                    "./dep".to_string(),
                    "from".to_string()
                )
            ]
        );
    }
// TSZ_INLINE_TEST_END 9c8acac84333874ce1aac6693881dc39707a67e0dbec7a74c9cdb5712ebd849e

// TSZ_INLINE_TEST_BEGIN b92efb24238ce2a29fc8b7d924592134878fd1f2f0810e5aa7e4e9b68b22baf3 1775 import_type_parses_quoted_member_name
    #[test]
    fn import_type_parses_quoted_member_name() {
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(r#"import("./dep")."a,b""#),
            Some(("./dep".to_string(), Some("a,b".to_string())))
        );
    }
// TSZ_INLINE_TEST_END b92efb24238ce2a29fc8b7d924592134878fd1f2f0810e5aa7e4e9b68b22baf3

// TSZ_INLINE_TEST_BEGIN 5c4a3f2222d2ab4e3081db3e81fc5e2db94f769599dc059fd645fc6cb703ef01 1783 import_type_member_keeps_full_qualified_path
    #[test]
    fn import_type_member_keeps_full_qualified_path() {
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(r#"import("./dep").A.B.C"#),
            Some(("./dep".to_string(), Some("A.B.C".to_string())))
        );
    }
// TSZ_INLINE_TEST_END 5c4a3f2222d2ab4e3081db3e81fc5e2db94f769599dc059fd645fc6cb703ef01

// TSZ_INLINE_TEST_BEGIN 3fc4ab1501f4fdfed8fd6d1229562a24c3f4c97fb0cf715119486354a526aae4 1791 import_type_member_stops_at_non_member_syntax
    #[test]
    fn import_type_member_stops_at_non_member_syntax() {
        // Generic arguments and array suffixes end the qualified path
        // without failing the parse.
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(r#"import("./dep").Foo<string>"#),
            Some(("./dep".to_string(), Some("Foo".to_string())))
        );
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(r#"import("./dep").A.B[]"#),
            Some(("./dep".to_string(), Some("A.B".to_string())))
        );
    }
// TSZ_INLINE_TEST_END 3fc4ab1501f4fdfed8fd6d1229562a24c3f4c97fb0cf715119486354a526aae4

// TSZ_INLINE_TEST_BEGIN 29790d30c0537338ae0b1f65b56466981412f611c2c52f56003cc3f1b023c50c 1805 namespace_alias_with_space
    #[test]
    fn namespace_alias_with_space() {
        let got = parse(r#"* as ns from "./dep""#);
        assert_eq!(
            got,
            vec![("ns".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }
// TSZ_INLINE_TEST_END 29790d30c0537338ae0b1f65b56466981412f611c2c52f56003cc3f1b023c50c

// TSZ_INLINE_TEST_BEGIN fd753c7170c374257fc3afd8261c4966901f1a54ec62d01a7689ded6741aa963 1814 namespace_alias_with_tab_around_as
    #[test]
    fn namespace_alias_with_tab_around_as() {
        let got = parse("*\tas\tns from \"./dep\"");
        assert_eq!(
            got,
            vec![("ns".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }
// TSZ_INLINE_TEST_END fd753c7170c374257fc3afd8261c4966901f1a54ec62d01a7689ded6741aa963

// TSZ_INLINE_TEST_BEGIN 555b0269b052a3a01e04609a8cee7668f615238eb0d4428ea1b188072897da95 1823 does_not_match_as_inside_identifier
    #[test]
    fn does_not_match_as_inside_identifier() {
        let got = parse(r#"{ Class } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "Class".to_string(),
                "./dep".to_string(),
                "Class".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END 555b0269b052a3a01e04609a8cee7668f615238eb0d4428ea1b188072897da95

// TSZ_INLINE_TEST_BEGIN 0eb1426b09dddaa93a8740316aa8a83e554b009db26dea3a59dbc6c88818452e 1836 alias_keyword_named_as
    #[test]
    fn alias_keyword_named_as() {
        let got = parse(r#"{ as as Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![("Foo".to_string(), "./dep".to_string(), "as".to_string())]
        );
    }
// TSZ_INLINE_TEST_END 0eb1426b09dddaa93a8740316aa8a83e554b009db26dea3a59dbc6c88818452e

// TSZ_INLINE_TEST_BEGIN 63de79654657cb8c2405360936a98e9fcffb2d704af38181504908aca09d0917 1845 default_alias
    #[test]
    fn default_alias() {
        let got = parse(r#"{ default as Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "Foo".to_string(),
                "./dep".to_string(),
                "default".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END 63de79654657cb8c2405360936a98e9fcffb2d704af38181504908aca09d0917
