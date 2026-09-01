//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes_enum_member.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 83a4c3b3c71319976f49461043c678b26625c4e1d3880db5ffaca14325532d68 144 add_missing_enum_member_simple_fallback_builds_an_insertion_plan_before_rendering
    #[test]
    fn add_missing_enum_member_simple_fallback_builds_an_insertion_plan_before_rendering() {
        let content = "enum Color {\n    Red\n}\nconst choice = Color.Blue;\n";
        let (enum_name, member_name) =
            Server::find_simple_missing_enum_member_reference(content).unwrap();
        assert_eq!(enum_name, "Color");
        assert_eq!(member_name, "Blue");

        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        let plan =
            Server::plan_simple_missing_enum_member_insertion(&lines, &enum_name, member_name)
                .unwrap();
        assert_eq!(plan.insertion_idx, 2);
        assert_eq!(plan.previous_member_idx, Some(1));

        let updated = Server::render_simple_missing_enum_member_edit(lines, &plan).join("\n");
        assert_eq!(
            updated,
            "enum Color {\n    Red,\n    Blue\n}\nconst choice = Color.Blue;"
        );
    }
// TSZ_INLINE_TEST_END 83a4c3b3c71319976f49461043c678b26625c4e1d3880db5ffaca14325532d68

// TSZ_INLINE_TEST_BEGIN e079a7a30b9693712fe9536e7e1b679cdf1be52c3972131aa36fa4b6dc3cf0d5 166 add_missing_enum_member_simple_fallback_preserves_existing_output_shape
    #[test]
    fn add_missing_enum_member_simple_fallback_preserves_existing_output_shape() {
        let content = "enum Mode {\n}\nconst mode = Mode.Active;\n";
        let (member_name, updated) =
            Server::apply_add_missing_enum_member_simple_fallback(content).unwrap();

        assert_eq!(member_name, "Active");
        assert_eq!(
            updated,
            "enum Mode {\n    Active\n}\nconst mode = Mode.Active;"
        );
    }
// TSZ_INLINE_TEST_END e079a7a30b9693712fe9536e7e1b679cdf1be52c3972131aa36fa4b6dc3cf0d5
