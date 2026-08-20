//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/file_context.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fc5313290f1235c5aeb3bb15c96ddfd8d07d7d51c4af55fb39d170ab699de14d 79 provider_context_matches_individual_accessors
    /// Sanity check: the borrowed view returned by
    /// `ProjectFile::provider_context` exposes the same five fields the
    /// individual file accessors expose, so the
    /// `define_lsp_provider!(binder ...)` macro's `from_context` constructor
    /// produces a provider identical to one built from the per-field accessors.
    #[test]
    fn provider_context_matches_individual_accessors() {
        let file = fixture("a.ts", "const x = 1;");

        let ctx = file.provider_context();

        // Pointer-identity for the borrowed members.
        assert!(std::ptr::eq(ctx.arena, file.arena()));
        assert!(std::ptr::eq(ctx.binder, file.binder()));
        assert!(std::ptr::eq(ctx.line_map, file.line_map()));
        assert!(std::ptr::eq(
            ctx.source_text.as_ptr(),
            file.source_text().as_ptr(),
        ));
        assert_eq!(ctx.file_name, file.file_name());
    }
// TSZ_INLINE_TEST_END fc5313290f1235c5aeb3bb15c96ddfd8d07d7d51c4af55fb39d170ab699de14d

// TSZ_INLINE_TEST_BEGIN 0a0c36bc15f6b91e3f4b0fc31406bf1dddc6be2aa96ede3dbf3ffd1e95eae747 98 provider_context_is_copy
    /// `LspProviderContext` is `Copy`, so feature dispatch can construct
    /// multiple providers from a single `file.provider_context()` call.
    #[test]
    fn provider_context_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<super::LspProviderContext<'_>>();

        let file = fixture("b.ts", "const y = 2;");
        let ctx = file.provider_context();
        let ctx2 = ctx; // does not move
        assert_eq!(ctx.file_name, ctx2.file_name);
    }
// TSZ_INLINE_TEST_END 0a0c36bc15f6b91e3f4b0fc31406bf1dddc6be2aa96ede3dbf3ffd1e95eae747
