//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/transforms/tslib_helper_naming.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 28c7ad59341c4cbba365bfee6e56852511c34a967926c3c622c3216b88dd34d8 103 bare_name_when_no_prefix_or_alias
    #[test]
    fn bare_name_when_no_prefix_or_alias() {
        let naming = TslibHelperNaming::default();
        assert_eq!(naming.helper_name("__decorate"), "__decorate");
        let mut buf = String::new();
        naming.write_into(&mut buf, "__decorate");
        assert_eq!(buf, "__decorate");
    }
// TSZ_INLINE_TEST_END 28c7ad59341c4cbba365bfee6e56852511c34a967926c3c622c3216b88dd34d8

// TSZ_INLINE_TEST_BEGIN 9d3259f05b53a9ed550b5f02ad48783e915c76a70d39cddf1e31dafb11d0beb9 112 commonjs_prefix_takes_precedence_over_alias
    #[test]
    fn commonjs_prefix_takes_precedence_over_alias() {
        let mut naming = TslibHelperNaming::default();
        naming.set_prefix(true);
        naming.set_binding("tslib_1".to_string());
        let mut aliases = FxHashMap::default();
        aliases.insert("__awaiter".to_string(), "__awaiter_1".to_string());
        naming.set_aliases(aliases);
        assert_eq!(naming.helper_name("__awaiter"), "tslib_1.__awaiter");
        let mut buf = String::new();
        naming.write_into(&mut buf, "__awaiter");
        assert_eq!(buf, "tslib_1.__awaiter");
    }
// TSZ_INLINE_TEST_END 9d3259f05b53a9ed550b5f02ad48783e915c76a70d39cddf1e31dafb11d0beb9

// TSZ_INLINE_TEST_BEGIN e0436b09e69b3a4f684cf0b90c1999bfb780a301b70a5508dbdffe135e6a9cf3 126 esm_alias_used_when_present_and_unprefixed
    #[test]
    fn esm_alias_used_when_present_and_unprefixed() {
        let mut naming = TslibHelperNaming::default();
        let mut aliases = FxHashMap::default();
        aliases.insert("__awaiter".to_string(), "__awaiter_1".to_string());
        naming.set_aliases(aliases);
        assert_eq!(naming.helper_name("__awaiter"), "__awaiter_1");
        assert_eq!(naming.helper_name("__generator"), "__generator");
        let mut buf = String::new();
        naming.write_into(&mut buf, "__awaiter");
        assert_eq!(buf, "__awaiter_1");
    }
// TSZ_INLINE_TEST_END e0436b09e69b3a4f684cf0b90c1999bfb780a301b70a5508dbdffe135e6a9cf3

// TSZ_INLINE_TEST_BEGIN 95d93833f673c4c37e9a16218ff4103db732f17045f698b4dc8d960d3b0db445 139 custom_binding_is_honored
    #[test]
    fn custom_binding_is_honored() {
        let mut naming = TslibHelperNaming::default();
        naming.set_prefix(true);
        naming.set_binding("tslib_42".to_string());
        assert_eq!(naming.helper_name("__extends"), "tslib_42.__extends");
    }
// TSZ_INLINE_TEST_END 95d93833f673c4c37e9a16218ff4103db732f17045f698b4dc8d960d3b0db445
