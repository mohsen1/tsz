//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/parse_rules/utils.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b2847722dfbeb80560a46d2dbc7f24370e3bb00a4a3b41b062a8f14f25a00fdb 281 is_identifier_or_keyword_accepts_identifier
    #[test]
    fn is_identifier_or_keyword_accepts_identifier() {
        assert!(is_identifier_or_keyword(SyntaxKind::Identifier));
    }
// TSZ_INLINE_TEST_END b2847722dfbeb80560a46d2dbc7f24370e3bb00a4a3b41b062a8f14f25a00fdb

// TSZ_INLINE_TEST_BEGIN 8bd13594323440d310c85807cbee4ef80101bd65b5fe9be9c9e496b0696d080e 286 is_identifier_or_keyword_accepts_reserved_keyword
    #[test]
    fn is_identifier_or_keyword_accepts_reserved_keyword() {
        assert!(is_identifier_or_keyword(SyntaxKind::ClassKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::ImportKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::ReturnKeyword));
    }
// TSZ_INLINE_TEST_END 8bd13594323440d310c85807cbee4ef80101bd65b5fe9be9c9e496b0696d080e

// TSZ_INLINE_TEST_BEGIN 45943d516cfdbebb2be28e8ca187bd44b51e47a961ddb48d132f628ccbb1be7d 293 is_identifier_or_keyword_accepts_contextual_keyword
    #[test]
    fn is_identifier_or_keyword_accepts_contextual_keyword() {
        assert!(is_identifier_or_keyword(SyntaxKind::TypeKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::AsyncKeyword));
        assert!(is_identifier_or_keyword(SyntaxKind::OfKeyword));
    }
// TSZ_INLINE_TEST_END 45943d516cfdbebb2be28e8ca187bd44b51e47a961ddb48d132f628ccbb1be7d

// TSZ_INLINE_TEST_BEGIN 033e28f45aad24585df2664c8386209173a7e75530f477299c800e7c45c8322c 300 is_identifier_or_keyword_rejects_punctuation_and_literals
    #[test]
    fn is_identifier_or_keyword_rejects_punctuation_and_literals() {
        assert!(!is_identifier_or_keyword(SyntaxKind::OpenBraceToken));
        assert!(!is_identifier_or_keyword(SyntaxKind::EqualsToken));
        assert!(!is_identifier_or_keyword(SyntaxKind::StringLiteral));
        assert!(!is_identifier_or_keyword(SyntaxKind::NumericLiteral));
    }
// TSZ_INLINE_TEST_END 033e28f45aad24585df2664c8386209173a7e75530f477299c800e7c45c8322c

// TSZ_INLINE_TEST_BEGIN e50ef1d676ede93067bb9e4228c19955d87217751c2a93e8eb59b8093c6534b7 310 contextual_only_accepts_identifier_and_non_reserved_keywords
    #[test]
    fn contextual_only_accepts_identifier_and_non_reserved_keywords() {
        assert!(is_identifier_or_contextual_keyword(SyntaxKind::Identifier));
        assert!(is_identifier_or_contextual_keyword(SyntaxKind::TypeKeyword));
        assert!(is_identifier_or_contextual_keyword(SyntaxKind::OfKeyword));
        assert!(is_identifier_or_contextual_keyword(
            SyntaxKind::AsyncKeyword
        ));
    }
// TSZ_INLINE_TEST_END e50ef1d676ede93067bb9e4228c19955d87217751c2a93e8eb59b8093c6534b7

// TSZ_INLINE_TEST_BEGIN 9e5f3d6aafa32eeda3212863fe8e1b2cec3dc336257fb3eecb30ed4f8107565c 320 contextual_only_rejects_reserved_words
    #[test]
    fn contextual_only_rejects_reserved_words() {
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::ClassKeyword
        ));
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::ImportKeyword
        ));
        assert!(!is_identifier_or_contextual_keyword(SyntaxKind::ForKeyword));
    }
// TSZ_INLINE_TEST_END 9e5f3d6aafa32eeda3212863fe8e1b2cec3dc336257fb3eecb30ed4f8107565c

// TSZ_INLINE_TEST_BEGIN 793147ee00539c624a0f786f81abe1c2a609ed2d6e04ebcec7cfce639c08cf5c 331 contextual_only_rejects_punctuation_and_literals
    #[test]
    fn contextual_only_rejects_punctuation_and_literals() {
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::OpenBraceToken
        ));
        assert!(!is_identifier_or_contextual_keyword(
            SyntaxKind::StringLiteral
        ));
    }
// TSZ_INLINE_TEST_END 793147ee00539c624a0f786f81abe1c2a609ed2d6e04ebcec7cfce639c08cf5c

// TSZ_INLINE_TEST_BEGIN 1824e4cf1eb918f763c43a78005d718e0343228dbc8191241675adc98d59b886 343 look_ahead_is_does_not_advance_scanner
    #[test]
    fn look_ahead_is_does_not_advance_scanner() {
        let (mut scanner, current) = scanner_after_first("foo bar");
        assert_eq!(current, SyntaxKind::Identifier);
        let result = look_ahead_is(&mut scanner, current, |t| t == SyntaxKind::Identifier);
        assert!(result, "expected `bar` to be classified as an identifier");
        // After the look-ahead, scanning again must see `bar`, proving the
        // snapshot was restored.
        let after = scanner.scan();
        assert_eq!(after, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "bar");
    }
// TSZ_INLINE_TEST_END 1824e4cf1eb918f763c43a78005d718e0343228dbc8191241675adc98d59b886

// TSZ_INLINE_TEST_BEGIN 6339cbed7e3c1a3ba34c99d0455d0e035467236dab643faad4328f213425fcfd 356 look_ahead_is_returns_false_when_check_fails
    #[test]
    fn look_ahead_is_returns_false_when_check_fails() {
        let (mut scanner, current) = scanner_after_first("foo;");
        let result = look_ahead_is(&mut scanner, current, |t| t == SyntaxKind::OpenBraceToken);
        assert!(!result);
    }
// TSZ_INLINE_TEST_END 6339cbed7e3c1a3ba34c99d0455d0e035467236dab643faad4328f213425fcfd

// TSZ_INLINE_TEST_BEGIN 3888bbcebc4f86d9b3c03279f45e0584027b5b3d0cb9d8feb285e6eac6d5c4fe 365 look_ahead_is_on_same_line_true_without_line_break
    #[test]
    fn look_ahead_is_on_same_line_true_without_line_break() {
        let (mut scanner, current) = scanner_after_first("foo bar");
        let result =
            look_ahead_is_on_same_line(&mut scanner, current, |t| t == SyntaxKind::Identifier);
        assert!(result);
    }
// TSZ_INLINE_TEST_END 3888bbcebc4f86d9b3c03279f45e0584027b5b3d0cb9d8feb285e6eac6d5c4fe

// TSZ_INLINE_TEST_BEGIN 643b7b114e42ddd33280be94de561dbca613b9fef7e822d85f20c90b1cc5004a 373 look_ahead_is_on_same_line_false_with_line_break
    #[test]
    fn look_ahead_is_on_same_line_false_with_line_break() {
        let (mut scanner, current) = scanner_after_first("foo\nbar");
        let result =
            look_ahead_is_on_same_line(&mut scanner, current, |t| t == SyntaxKind::Identifier);
        assert!(
            !result,
            "ASI: a line break before `bar` must make `look_ahead_is_on_same_line` return false"
        );
    }
// TSZ_INLINE_TEST_END 643b7b114e42ddd33280be94de561dbca613b9fef7e822d85f20c90b1cc5004a

// TSZ_INLINE_TEST_BEGIN ae58149f81693f51606dda6c80f47e06f39f43cef3d7c880bcd71a91a080a4d0 386 look_ahead_is_async_declaration_accepts_function_class_interface_etc
    #[test]
    fn look_ahead_is_async_declaration_accepts_function_class_interface_etc() {
        for next in &["function f(){}", "class C{}", "interface I{}", "enum E{}"] {
            let source = format!("async {next}");
            let (mut scanner, current) = scanner_after_first(&source);
            assert_eq!(current, SyntaxKind::AsyncKeyword);
            assert!(
                look_ahead_is_async_declaration(&mut scanner, current),
                "expected `async {next}` to be classified as an async declaration"
            );
        }
    }
// TSZ_INLINE_TEST_END ae58149f81693f51606dda6c80f47e06f39f43cef3d7c880bcd71a91a080a4d0

// TSZ_INLINE_TEST_BEGIN 16e188e9d80a0e694683ef99e65ce9e294ede51cb70db85f32163b28842e397e 399 look_ahead_is_async_declaration_rejects_arrow
    #[test]
    fn look_ahead_is_async_declaration_rejects_arrow() {
        let (mut scanner, current) = scanner_after_first("async () => 1");
        assert!(!look_ahead_is_async_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 16e188e9d80a0e694683ef99e65ce9e294ede51cb70db85f32163b28842e397e

// TSZ_INLINE_TEST_BEGIN 0bc7b467b857af21688a99853451aa1449aa5536e9d6362d2b3b25da60e11ac3 407 look_ahead_is_abstract_declaration_accepts_class
    #[test]
    fn look_ahead_is_abstract_declaration_accepts_class() {
        let (mut scanner, current) = scanner_after_first("abstract class C {}");
        assert_eq!(current, SyntaxKind::AbstractKeyword);
        assert!(look_ahead_is_abstract_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 0bc7b467b857af21688a99853451aa1449aa5536e9d6362d2b3b25da60e11ac3

// TSZ_INLINE_TEST_BEGIN 6a4aa175bb23e23e63eecc11e715c3d3612a5660569aa69f1a7f2b56ed36ff7d 414 look_ahead_is_abstract_declaration_rejects_identifier_after
    #[test]
    fn look_ahead_is_abstract_declaration_rejects_identifier_after() {
        let (mut scanner, current) = scanner_after_first("abstract foo");
        assert!(!look_ahead_is_abstract_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 6a4aa175bb23e23e63eecc11e715c3d3612a5660569aa69f1a7f2b56ed36ff7d

// TSZ_INLINE_TEST_BEGIN 09fcc07ef6fcbd1974db36a1982b5017ef26658089eaa8424696f42237530a12 422 look_ahead_is_module_declaration_accepts_string_literal_name
    #[test]
    fn look_ahead_is_module_declaration_accepts_string_literal_name() {
        let (mut scanner, current) = scanner_after_first(r#"module "external" {}"#);
        assert_eq!(current, SyntaxKind::ModuleKeyword);
        assert!(look_ahead_is_module_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 09fcc07ef6fcbd1974db36a1982b5017ef26658089eaa8424696f42237530a12

// TSZ_INLINE_TEST_BEGIN 839fd9815f5d94a49b1b553e1f13921611b54c051addd151869ea42f64078f14 429 look_ahead_is_module_declaration_accepts_identifier_name
    #[test]
    fn look_ahead_is_module_declaration_accepts_identifier_name() {
        let (mut scanner, current) = scanner_after_first("namespace Foo {}");
        assert_eq!(current, SyntaxKind::NamespaceKeyword);
        assert!(look_ahead_is_module_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 839fd9815f5d94a49b1b553e1f13921611b54c051addd151869ea42f64078f14

// TSZ_INLINE_TEST_BEGIN f5f59bfc08f51e535f54def83a4815295c12a00402916e6ea5f23917a9937a21 436 look_ahead_is_module_declaration_rejects_in_keyword
    #[test]
    fn look_ahead_is_module_declaration_rejects_in_keyword() {
        // Binary `in` / `instanceof` are intentionally rejected so that
        // `module in obj` parses as an expression, not as a namespace decl.
        let (mut scanner, current) = scanner_after_first("module in obj");
        assert!(!look_ahead_is_module_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END f5f59bfc08f51e535f54def83a4815295c12a00402916e6ea5f23917a9937a21

// TSZ_INLINE_TEST_BEGIN 0b99997e5f92e792f73868cd7e684631b5e2c84bbb0a88379f5cfb9c92ea34b2 444 look_ahead_is_module_declaration_false_after_line_break
    #[test]
    fn look_ahead_is_module_declaration_false_after_line_break() {
        // ASI: `namespace\nFoo` must NOT parse as a namespace decl.
        let (mut scanner, current) = scanner_after_first("namespace\nFoo {}");
        assert!(!look_ahead_is_module_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 0b99997e5f92e792f73868cd7e684631b5e2c84bbb0a88379f5cfb9c92ea34b2

// TSZ_INLINE_TEST_BEGIN 6124d23bcccd02c5baff3b5b5b82a3f1c31841f66c8a56cc52e88611bb3da335 453 look_ahead_is_type_alias_declaration_accepts_identifier
    #[test]
    fn look_ahead_is_type_alias_declaration_accepts_identifier() {
        let (mut scanner, current) = scanner_after_first("type Foo = number");
        assert_eq!(current, SyntaxKind::TypeKeyword);
        assert!(look_ahead_is_type_alias_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 6124d23bcccd02c5baff3b5b5b82a3f1c31841f66c8a56cc52e88611bb3da335

// TSZ_INLINE_TEST_BEGIN c319ea9aa764e6a40f350197971bb49659f79c1e3829e938f50903f3a7da86e8 460 look_ahead_is_type_alias_declaration_false_after_line_break
    #[test]
    fn look_ahead_is_type_alias_declaration_false_after_line_break() {
        // ASI: `type\nFoo = ...` must not parse as a type alias decl.
        let (mut scanner, current) = scanner_after_first("type\nFoo = number");
        assert!(!look_ahead_is_type_alias_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END c319ea9aa764e6a40f350197971bb49659f79c1e3829e938f50903f3a7da86e8

// TSZ_INLINE_TEST_BEGIN dbf098a4ba9582a3ebabe944a7cbaa4ba821f24af3c3d1c07302f3108b7d5617 467 look_ahead_is_type_alias_declaration_rejects_void_keyword_for_statement_recovery
    #[test]
    fn look_ahead_is_type_alias_declaration_rejects_void_keyword_for_statement_recovery() {
        let (mut scanner, current) = scanner_after_first("type void = T");
        assert_eq!(current, SyntaxKind::TypeKeyword);
        assert!(!look_ahead_is_type_alias_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END dbf098a4ba9582a3ebabe944a7cbaa4ba821f24af3c3d1c07302f3108b7d5617

// TSZ_INLINE_TEST_BEGIN 527f9a8382c37eedcf199c58bef3e7e890d3a189a4260d478ac1857b0c929538 474 look_ahead_is_type_alias_declaration_rejects_reserved_word
    #[test]
    fn look_ahead_is_type_alias_declaration_rejects_reserved_word() {
        let (mut scanner, current) = scanner_after_first("type class = T");
        assert_eq!(current, SyntaxKind::TypeKeyword);
        assert!(!look_ahead_is_type_alias_declaration(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 527f9a8382c37eedcf199c58bef3e7e890d3a189a4260d478ac1857b0c929538

// TSZ_INLINE_TEST_BEGIN 3849b51c849708144f34d118047da89fa8fee6de8642f81d1772658799a154f0 483 look_ahead_is_const_enum_true_for_const_enum
    #[test]
    fn look_ahead_is_const_enum_true_for_const_enum() {
        let (mut scanner, current) = scanner_after_first("const enum E {}");
        assert_eq!(current, SyntaxKind::ConstKeyword);
        assert!(look_ahead_is_const_enum(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 3849b51c849708144f34d118047da89fa8fee6de8642f81d1772658799a154f0

// TSZ_INLINE_TEST_BEGIN a60ab741e173847f87dc85c590cc3a521f25b657fd5714ae78c7f73ba9efc14a 490 look_ahead_is_const_enum_false_for_const_var
    #[test]
    fn look_ahead_is_const_enum_false_for_const_var() {
        let (mut scanner, current) = scanner_after_first("const x = 1");
        assert!(!look_ahead_is_const_enum(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END a60ab741e173847f87dc85c590cc3a521f25b657fd5714ae78c7f73ba9efc14a

// TSZ_INLINE_TEST_BEGIN cad6dbf2063534bed1158f22cc95f1e57d0eb5a81984e4d5dc1da126565258f1 498 look_ahead_is_import_call_accepts_open_paren
    #[test]
    fn look_ahead_is_import_call_accepts_open_paren() {
        let (mut scanner, current) = scanner_after_first(r#"import("./mod")"#);
        assert_eq!(current, SyntaxKind::ImportKeyword);
        assert!(look_ahead_is_import_call(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END cad6dbf2063534bed1158f22cc95f1e57d0eb5a81984e4d5dc1da126565258f1

// TSZ_INLINE_TEST_BEGIN 50d0bd798f279d65f37c0828887da1b074ef5bdf878ccda975a067c4055d4243 505 look_ahead_is_import_call_accepts_dot_for_meta
    #[test]
    fn look_ahead_is_import_call_accepts_dot_for_meta() {
        let (mut scanner, current) = scanner_after_first("import.meta");
        assert!(look_ahead_is_import_call(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 50d0bd798f279d65f37c0828887da1b074ef5bdf878ccda975a067c4055d4243

// TSZ_INLINE_TEST_BEGIN f718334d6095642801334584246ac2dbd5b255f0d9441dbe02816e4f190c2a22 511 look_ahead_is_import_call_accepts_less_than_for_generic
    #[test]
    fn look_ahead_is_import_call_accepts_less_than_for_generic() {
        // `import<` is intentionally captured so the expression parser can
        // emit TS1326 instead of routing into the import-decl path.
        let (mut scanner, current) = scanner_after_first("import<T>");
        assert!(look_ahead_is_import_call(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END f718334d6095642801334584246ac2dbd5b255f0d9441dbe02816e4f190c2a22

// TSZ_INLINE_TEST_BEGIN 20e73cbd7895bc1cae9d033673596d1305f052757c4e15a860bd2bdeabb4a0e0 519 look_ahead_is_import_call_rejects_identifier
    #[test]
    fn look_ahead_is_import_call_rejects_identifier() {
        let (mut scanner, current) = scanner_after_first(r#"import foo from "m""#);
        assert!(!look_ahead_is_import_call(&mut scanner, current));
    }
// TSZ_INLINE_TEST_END 20e73cbd7895bc1cae9d033673596d1305f052757c4e15a860bd2bdeabb4a0e0
