//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_completions_parameters.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a295a6ba47e9ab86d69aa8d62e594a6a5e49b5a1ec39c19e1d9ed87ae8c1fb0c 106 extracts_identifiers_from_trailing_function_declaration
    #[test]
    fn extracts_identifiers_from_trailing_function_declaration() {
        let blocked = names("function handle(first: string, second = 1, ...rest: unknown[]) {}");

        assert!(blocked.contains("first"));
        assert!(blocked.contains("second"));
        assert!(blocked.contains("rest"));
        assert_eq!(blocked.len(), 3);
    }
// TSZ_INLINE_TEST_END a295a6ba47e9ab86d69aa8d62e594a6a5e49b5a1ec39c19e1d9ed87ae8c1fb0c

// TSZ_INLINE_TEST_BEGIN 668402c8074daa3730fb3d1e21bef81f1f9ea02160116f2489ed3e775706e884 116 accepts_identifier_start_variants
    #[test]
    fn accepts_identifier_start_variants() {
        let blocked = names("function handle(_local: string, $value: number, 9bad: string) {}");

        assert!(blocked.contains("_local"));
        assert!(blocked.contains("$value"));
        assert!(!blocked.contains("9bad"));
        assert_eq!(blocked.len(), 2);
    }
// TSZ_INLINE_TEST_END 668402c8074daa3730fb3d1e21bef81f1f9ea02160116f2489ed3e775706e884

// TSZ_INLINE_TEST_BEGIN 69302ad964d8f39c0ee899334f865d7355d2f0ef77e510c96dc0cd356b186b5b 126 returns_empty_when_cursor_is_not_after_trailing_body
    #[test]
    fn returns_empty_when_cursor_is_not_after_trailing_body() {
        let blocked = trailing_function_parameter_names_at_declaration_end(
            "function handle(first: string) {",
            32,
        );

        assert!(blocked.is_empty());
    }
// TSZ_INLINE_TEST_END 69302ad964d8f39c0ee899334f865d7355d2f0ef77e510c96dc0cd356b186b5b

// TSZ_INLINE_TEST_BEGIN 37ce5fffa370d620eb9ee8ce74bf02417001618234bf94a106215b0852a9cd6b 136 returns_empty_when_function_parameter_list_is_incomplete
    #[test]
    fn returns_empty_when_function_parameter_list_is_incomplete() {
        let blocked = names("function handle(first: string { }");

        assert!(blocked.is_empty());
    }
// TSZ_INLINE_TEST_END 37ce5fffa370d620eb9ee8ce74bf02417001618234bf94a106215b0852a9cd6b
