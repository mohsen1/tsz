#[cfg(test)]
mod jsdoc_import_as_whitespace_tests {
    use crate::state::CheckerState;

    fn parse(rest: &str) -> Vec<(String, String, String)> {
        CheckerState::parse_jsdoc_import_tag(rest)
    }

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

    #[test]
    fn import_namespace_alias_with_tab() {
        let imports = parse("*\tas\tNS from \"./dep\"");
        assert_eq!(
            imports,
            vec![("NS".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn import_namespace_alias_with_space() {
        let imports = parse("* as NS from \"./dep\"");
        assert_eq!(
            imports,
            vec![("NS".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

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
}

#[cfg(test)]
mod jsdoc_tag_boundary_tests {
    use crate::state::CheckerState;

    // Issue #2916: longer JSDoc tag names must not match shorter tags. The
    // tag-boundary helpers gate every JSDoc tag-detection path so identifiers
    // such as `@satisfiesx`, `@importx`, `@overridex`, `@thisx`, `@typedefx`,
    // `@callbackx`, and `@constructorx` are not silently treated as the
    // shorter real tags.

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
}

#[cfg(test)]
mod parse_jsdoc_import_tag_alias_tests {
    use crate::state::CheckerState;

    fn parse(rest: &str) -> Vec<(String, String, String)> {
        CheckerState::parse_jsdoc_import_tag(rest)
    }

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

    #[test]
    fn named_without_alias() {
        let got = parse(r#"{ Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![("Foo".to_string(), "./dep".to_string(), "Foo".to_string())]
        );
    }

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

    #[test]
    fn import_type_parses_quoted_member_name() {
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(r#"import("./dep")."a,b""#),
            Some(("./dep".to_string(), Some("a,b".to_string())))
        );
    }

    #[test]
    fn namespace_alias_with_space() {
        let got = parse(r#"* as ns from "./dep""#);
        assert_eq!(
            got,
            vec![("ns".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn namespace_alias_with_tab_around_as() {
        let got = parse("*\tas\tns from \"./dep\"");
        assert_eq!(
            got,
            vec![("ns".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

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

    #[test]
    fn alias_keyword_named_as() {
        let got = parse(r#"{ as as Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![("Foo".to_string(), "./dep".to_string(), "as".to_string())]
        );
    }

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
}
