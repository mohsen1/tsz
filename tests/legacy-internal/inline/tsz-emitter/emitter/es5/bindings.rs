//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/es5/bindings.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bfd23917c1eb79fe028192481b816889599ef79582f06f438c7abd799531ef2e 1764 emit_using_declaration_es5
    #[test]
    fn emit_using_declaration_es5() {
        let source = "using d = { [Symbol.dispose]() {} };\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var env_1"),
            "Expected disposable env temp allocation.\nOutput:\n{output}"
        );
        assert!(
            output.contains("__addDisposableResource"),
            "Expected __addDisposableResource helper call for using declarations.\nOutput:\n{output}"
        );
        assert!(
            output.contains("__disposeResources"),
            "Expected __disposeResources helper call for using declarations.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("using d"),
            "Raw using syntax should be downleveled on ES5.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END bfd23917c1eb79fe028192481b816889599ef79582f06f438c7abd799531ef2e

// TSZ_INLINE_TEST_BEGIN 6b57f5503f949390c32dd86e93c601506cdba12fe3f9f04fa5e00e178297ae4e 1793 destructuring_new_expr_gets_parens_for_property_access
    #[test]
    fn destructuring_new_expr_gets_parens_for_property_access() {
        // var { x } = <any>new Foo; → var x = (new Foo).x;
        let source = "var { x } = <any>new Foo;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(new Foo).x"),
            "Destructured new expression needs parens for property access.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("new Foo.x"),
            "Should NOT produce `new Foo.x` (different semantics).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6b57f5503f949390c32dd86e93c601506cdba12fe3f9f04fa5e00e178297ae4e

// TSZ_INLINE_TEST_BEGIN a74d7b7ca00439291ba23e8f6fb24cd3af0dac281992852f7da01c783637a776 1815 destructuring_new_with_args_no_extra_parens
    #[test]
    fn destructuring_new_with_args_no_extra_parens() {
        // var { x } = <any>new Foo(); → var x = new Foo().x; (no extra parens needed)
        let source = "var { x } = <any>new Foo();\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new Foo().x"),
            "new Foo() with args should NOT have extra parens.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END a74d7b7ca00439291ba23e8f6fb24cd3af0dac281992852f7da01c783637a776

// TSZ_INLINE_TEST_BEGIN 50e3e4337bcd72762ef67550582e4f636b84d3dacf1e2dd8b49419fd9b482ccb 1833 empty_binding_patterns_with_identifier_rhs_emit_temp
    #[test]
    fn empty_binding_patterns_with_identifier_rhs_emit_temp() {
        let source = "let {} = undefined;\nlet {} = maybe;\nlet [] = xs;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _a = undefined;"),
            "Empty object binding with `undefined` RHS should still evaluate through a temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _b = maybe;"),
            "Empty object binding with identifier RHS should still evaluate through a temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _c = xs;"),
            "Empty array binding with identifier RHS should still evaluate through a temp.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("var ;"),
            "Empty binding patterns must not emit an empty variable declaration.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 50e3e4337bcd72762ef67550582e4f636b84d3dacf1e2dd8b49419fd9b482ccb
