//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/computed/builtin_iterator_return_alias.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b162f53fa9f64bc96b674657a0ff2a3e5ed2810e81aa9636e85c06607060d013 157 bare_intrinsic_alias_body_is_builtin_iterator_return_intrinsic
    #[test]
    fn bare_intrinsic_alias_body_is_builtin_iterator_return_intrinsic() {
        let (arena, alias_idx) = first_statement("type BuiltinIteratorReturn = intrinsic;");

        assert!(
            CheckerState::type_alias_declaration_is_builtin_iterator_return_intrinsic(
                &arena, alias_idx,
            ),
            "bare intrinsic alias body should be classified structurally"
        );
    }
// TSZ_INLINE_TEST_END b162f53fa9f64bc96b674657a0ff2a3e5ed2810e81aa9636e85c06607060d013

// TSZ_INLINE_TEST_BEGIN 37ea19f6e3db5364a92d90779458a6f6f2a5a9d33feec0be7b412bac9430cc09 169 parenthesized_intrinsic_alias_body_is_not_builtin_iterator_return_intrinsic
    #[test]
    fn parenthesized_intrinsic_alias_body_is_not_builtin_iterator_return_intrinsic() {
        let (arena, alias_idx) = first_statement("type BuiltinIteratorReturn = (intrinsic);");

        assert!(
            !CheckerState::type_alias_declaration_is_builtin_iterator_return_intrinsic(
                &arena, alias_idx,
            ),
            "parenthesized intrinsic must be rejected from AST shape, not source text"
        );
    }
// TSZ_INLINE_TEST_END 37ea19f6e3db5364a92d90779458a6f6f2a5a9d33feec0be7b412bac9430cc09
