//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/classify/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN acc20ab6e02867b48c8c500fccf01a403c0d13bfe977eeed17a46bf625d226c8 368 classifies_core_flag_vocabulary
    #[test]
    fn classifies_core_flag_vocabulary() {
        assert_eq!(
            classify_symbol_flags(symbol_flags::FUNCTION),
            LspSymbolClass::Function
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::CLASS),
            LspSymbolClass::Class
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::INTERFACE),
            LspSymbolClass::Interface
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::REGULAR_ENUM),
            LspSymbolClass::Enum
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::CONST_ENUM),
            LspSymbolClass::Enum
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::ENUM_MEMBER),
            LspSymbolClass::EnumMember
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::TYPE_ALIAS),
            LspSymbolClass::TypeAlias
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::TYPE_PARAMETER),
            LspSymbolClass::TypeParameter
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::VALUE_MODULE),
            LspSymbolClass::Module
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::ALIAS),
            LspSymbolClass::Alias
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::BLOCK_SCOPED_VARIABLE),
            LspSymbolClass::BlockScopedVariable
        );
        assert_eq!(
            classify_symbol_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE),
            LspSymbolClass::FunctionScopedVariable
        );
        assert_eq!(classify_symbol_flags(0), LspSymbolClass::Other);
    }
// TSZ_INLINE_TEST_END acc20ab6e02867b48c8c500fccf01a403c0d13bfe977eeed17a46bf625d226c8

// TSZ_INLINE_TEST_BEGIN 814051dfe8a7ef026794c32ccf78497c5cf55a618447d9849846735284997817 421 alias_wins_over_re_exported_value
    #[test]
    fn alias_wins_over_re_exported_value() {
        // An import/export alias is presented as an alias regardless of the
        // value flags it may also carry.
        let flags = symbol_flags::ALIAS | symbol_flags::FUNCTION;
        assert_eq!(classify_symbol_flags(flags), LspSymbolClass::Alias);
    }
// TSZ_INLINE_TEST_END 814051dfe8a7ef026794c32ccf78497c5cf55a618447d9849846735284997817

// TSZ_INLINE_TEST_BEGIN 5852fff1f40e9c6a0eee187a8b9235dd1fc37c39e12c77c78c14c9bd0cff67ab 429 const_enum_is_enum_not_variable
    #[test]
    fn const_enum_is_enum_not_variable() {
        // Specificity: a const-enum must classify as `Enum`, never a variable.
        let flags = symbol_flags::CONST_ENUM | symbol_flags::BLOCK_SCOPED_VARIABLE;
        assert_eq!(classify_symbol_flags(flags), LspSymbolClass::Enum);
    }
// TSZ_INLINE_TEST_END 5852fff1f40e9c6a0eee187a8b9235dd1fc37c39e12c77c78c14c9bd0cff67ab

// TSZ_INLINE_TEST_BEGIN 621431e7a0b619d8ec96f2bb660d31a11a6d97cde475a4948a69c1ea9d6dcb38 436 symbol_kind_collapses_non_lsp_classes
    #[test]
    fn symbol_kind_collapses_non_lsp_classes() {
        assert_eq!(LspSymbolClass::Alias.to_symbol_kind(), SymbolKind::Variable);
        assert_eq!(
            LspSymbolClass::Accessor.to_symbol_kind(),
            SymbolKind::Property
        );
        assert_eq!(
            LspSymbolClass::TypeAlias.to_symbol_kind(),
            SymbolKind::TypeParameter
        );
        assert_eq!(
            LspSymbolClass::EnumMember.to_symbol_kind(),
            SymbolKind::EnumMember
        );
    }
// TSZ_INLINE_TEST_END 621431e7a0b619d8ec96f2bb660d31a11a6d97cde475a4948a69c1ea9d6dcb38

// TSZ_INLINE_TEST_BEGIN 8bae9ae3de8a45f1dc03baa92f9b1aeddd9bcfe1ba816bbdf4bdf307ce8afd3b 453 completion_kind_block_scoped_default_is_let
    #[test]
    fn completion_kind_block_scoped_default_is_let() {
        // Const/let split is the caller's job; the class default is Let.
        assert_eq!(
            LspSymbolClass::BlockScopedVariable.to_completion_kind(),
            CompletionItemKind::Let
        );
        assert_eq!(
            LspSymbolClass::Alias.to_completion_kind(),
            CompletionItemKind::Alias
        );
        assert_eq!(
            LspSymbolClass::Accessor.to_completion_kind(),
            CompletionItemKind::Variable
        );
    }
// TSZ_INLINE_TEST_END 8bae9ae3de8a45f1dc03baa92f9b1aeddd9bcfe1ba816bbdf4bdf307ce8afd3b

// TSZ_INLINE_TEST_BEGIN 9e298cc727ae33ea0ef5c41191e5698eb7d4bfb381973cb9d1e929317935284e 470 tsserver_kind_str_labels
    #[test]
    fn tsserver_kind_str_labels() {
        assert_eq!(LspSymbolClass::Alias.tsserver_kind_str(), "alias");
        assert_eq!(
            LspSymbolClass::EnumMember.tsserver_kind_str(),
            "enum member"
        );
        assert_eq!(
            LspSymbolClass::TypeParameter.tsserver_kind_str(),
            "type parameter"
        );
        assert_eq!(LspSymbolClass::Other.tsserver_kind_str(), "");
    }
// TSZ_INLINE_TEST_END 9e298cc727ae33ea0ef5c41191e5698eb7d4bfb381973cb9d1e929317935284e

// TSZ_INLINE_TEST_BEGIN e2ab2033d2c8efd62a05a4d348a5773f6c6653d108ab968fccee2298b3e93d5a 484 detail_str_labels_and_gaps
    #[test]
    fn detail_str_labels_and_gaps() {
        assert_eq!(
            LspSymbolClass::BlockScopedVariable.detail_str(),
            Some("let/const")
        );
        assert_eq!(
            LspSymbolClass::FunctionScopedVariable.detail_str(),
            Some("var")
        );
        assert_eq!(LspSymbolClass::Alias.detail_str(), None);
        assert_eq!(LspSymbolClass::EnumMember.detail_str(), None);
    }
// TSZ_INLINE_TEST_END e2ab2033d2c8efd62a05a4d348a5773f6c6653d108ab968fccee2298b3e93d5a

// TSZ_INLINE_TEST_BEGIN ef71acbaeb5abd16e04377dbf0534d48c96ff5b889040cf9b931f906d4bc9e77 498 rename_kind_has_no_constructor_or_accessor
    #[test]
    fn rename_kind_has_no_constructor_or_accessor() {
        assert_eq!(
            LspSymbolClass::Constructor.to_rename_kind(),
            RenameSymbolKind::Unknown
        );
        assert_eq!(
            LspSymbolClass::Accessor.to_rename_kind(),
            RenameSymbolKind::Unknown
        );
        assert_eq!(
            LspSymbolClass::FunctionScopedVariable.to_rename_kind(),
            RenameSymbolKind::Var
        );
    }
// TSZ_INLINE_TEST_END ef71acbaeb5abd16e04377dbf0534d48c96ff5b889040cf9b931f906d4bc9e77
