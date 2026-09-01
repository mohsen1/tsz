//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_collection.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d38000d028a28942ab310a74b3f5b47211877905a9f169df5454b1786effe23b 843 classify_exports_dot_prop
    #[test]
    fn classify_exports_dot_prop() {
        assert_target(
            "exports.foo = 1;",
            &no_aliases(),
            "foo",
            CommonJsExportTargetRoot::Exports,
        );
    }
// TSZ_INLINE_TEST_END d38000d028a28942ab310a74b3f5b47211877905a9f169df5454b1786effe23b

// TSZ_INLINE_TEST_BEGIN 861137d47fa74b283d2127d9039fd7f74ca16de8839a99ba187ea6e2573a44b9 853 classify_exports_bracket_prop
    #[test]
    fn classify_exports_bracket_prop() {
        assert_target(
            r#"exports["bar"] = 2;"#,
            &no_aliases(),
            "bar",
            CommonJsExportTargetRoot::Exports,
        );
    }
// TSZ_INLINE_TEST_END 861137d47fa74b283d2127d9039fd7f74ca16de8839a99ba187ea6e2573a44b9

// TSZ_INLINE_TEST_BEGIN 435eb41253b0231794aabe93e26e19201abf2c974a2020f2746ab6b6fe4e00e2 863 classify_module_exports_dot_prop
    #[test]
    fn classify_module_exports_dot_prop() {
        assert_target(
            "module.exports.baz = 3;",
            &no_aliases(),
            "baz",
            CommonJsExportTargetRoot::ModuleExports,
        );
    }
// TSZ_INLINE_TEST_END 435eb41253b0231794aabe93e26e19201abf2c974a2020f2746ab6b6fe4e00e2

// TSZ_INLINE_TEST_BEGIN 16fb1dfb8524f35418e226796d5176a7a4eff5a7aedb3f734b93e0d02ca7de7b 873 classify_module_bracket_exports_dot_prop
    #[test]
    fn classify_module_bracket_exports_dot_prop() {
        assert_target(
            r#"module["exports"].qux = 4;"#,
            &no_aliases(),
            "qux",
            CommonJsExportTargetRoot::ModuleExports,
        );
    }
// TSZ_INLINE_TEST_END 16fb1dfb8524f35418e226796d5176a7a4eff5a7aedb3f734b93e0d02ca7de7b

// TSZ_INLINE_TEST_BEGIN 9190e5b8cade6ab71e96b6f31f0929c2dba0ba688c1b0efbaf8071540c99d57c 883 classify_alias_prop
    #[test]
    fn classify_alias_prop() {
        assert_target(
            "e.thing = 5;",
            &aliases(&["e"]),
            "thing",
            CommonJsExportTargetRoot::Alias,
        );
    }
// TSZ_INLINE_TEST_END 9190e5b8cade6ab71e96b6f31f0929c2dba0ba688c1b0efbaf8071540c99d57c

// TSZ_INLINE_TEST_BEGIN 99dd09c2d6c6dbe6558b602df541ce167d66ca72282d59ba4e25258ff4c0818d 893 classify_non_export_returns_none
    #[test]
    fn classify_non_export_returns_none() {
        let (arena, left) = binary_lhs_of("obj.prop = 6;");
        let result = CheckerState::commonjs_export_assignment_target(&arena, left, &no_aliases());
        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END 99dd09c2d6c6dbe6558b602df541ce167d66ca72282d59ba4e25258ff4c0818d

// TSZ_INLINE_TEST_BEGIN d0821b66c311db168f3f40bcf077b55ce02791633b2e9810443c856b0c5a85bf 900 classify_alias_not_in_set_returns_none
    #[test]
    fn classify_alias_not_in_set_returns_none() {
        let (arena, left) = binary_lhs_of("e.thing = 7;");
        let result = CheckerState::commonjs_export_assignment_target(&arena, left, &no_aliases());
        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END d0821b66c311db168f3f40bcf077b55ce02791633b2e9810443c856b0c5a85bf
