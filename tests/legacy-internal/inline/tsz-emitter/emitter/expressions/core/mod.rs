//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/expressions/core/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 103508fac712efc6ba3da8dade82f842640759d125246e3b6303a9b3ab0ae628 13 multiline_parenthesized_erased_assertion_keeps_comment_layout
    #[test]
    fn multiline_parenthesized_erased_assertion_keeps_comment_layout() {
        let source = r#"class Foo {
    foo() {
        return (
            /* keep */ this.client
                .getThing() as Promise<void>
        );
    }
}"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("return (\n        /* keep */ this.client\n            .getThing());"),
            "Multiline parenthesized erased assertion should preserve its comment layout.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 103508fac712efc6ba3da8dade82f842640759d125246e3b6303a9b3ab0ae628

// TSZ_INLINE_TEST_BEGIN a188b47c43bfc1cffc240c3903691cfb015b21d32912cbfc5fb575da1e6b2c81 37 parenthesized_expression_preserves_comments_around_close_paren
    #[test]
    fn parenthesized_expression_preserves_comments_around_close_paren() {
        let source = "/*1*/(/*2*/ \"foo\" /*3*/)/*4*/;\n// open\n/*1*/(\n    // next\n    /*2*/\"foo\"\n    //close\n    /*3*/)/*4*/;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("/*1*/ ( /*2*/\"foo\" /*3*/) /*4*/;"),
            "Same-line parenthesized comments should stay before the semicolon.\nOutput:\n{output}"
        );
        assert!(
            output.contains("//close\n/*3*/ ) /*4*/;"),
            "Multiline parenthesized comments should stay around the closing paren.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END a188b47c43bfc1cffc240c3903691cfb015b21d32912cbfc5fb575da1e6b2c81

// TSZ_INLINE_TEST_BEGIN b0ea6bd04fa97113298611d4b57ac3e3d22a126d836465b07680747cf4d092f5 62 dynamic_import_emits_import_keyword
    /// Dynamic `import('path')` expressions must emit the `import` keyword.
    /// Previously the emitter's `emit_node_by_kind` dispatch had no handler for
    /// `SyntaxKind::ImportKeyword`, so the keyword was silently dropped and the
    /// output became just `('path')`.
    #[test]
    fn dynamic_import_emits_import_keyword() {
        let source = r#"const m = import("./module");"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./module")"#),
            "Dynamic import must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END b0ea6bd04fa97113298611d4b57ac3e3d22a126d836465b07680747cf4d092f5

// TSZ_INLINE_TEST_BEGIN e9b409ccafa58146d31e5e0f6fccc9bd69dc2dabdc8892610db79a8adfec12aa 80 import_meta_emits_import_keyword
    /// `import.meta` property access must emit the `import` keyword.
    #[test]
    fn import_meta_emits_import_keyword() {
        let source = r#"const url = import.meta.url;"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("import.meta.url"),
            "import.meta must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END e9b409ccafa58146d31e5e0f6fccc9bd69dc2dabdc8892610db79a8adfec12aa

// TSZ_INLINE_TEST_BEGIN dec7e50d2d4c6f98c98e1c52e7f4bccf2fffb4b8aa1d1ed0127202135bb7a9bf 98 dynamic_import_in_async_function
    /// Dynamic import inside an async function body.
    #[test]
    fn dynamic_import_in_async_function() {
        let source = r#"async function load() { return await import("./lib"); }"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./lib")"#),
            "Dynamic import inside async function must emit 'import' keyword.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END dec7e50d2d4c6f98c98e1c52e7f4bccf2fffb4b8aa1d1ed0127202135bb7a9bf

// TSZ_INLINE_TEST_BEGIN 9238e6c3bbdb5446609d2532fb3ce18fe5089d33b9fdabae1bb31238aaaac703 121 yield_from_await_no_extra_parens_in_assignment_rhs
    /// When async functions are lowered to generator functions (ES2015 target),
    /// `await expr` becomes `yield expr`. Yield has lower precedence than most
    /// operators, so it needs parens inside binary operators like `||`:
    /// `await p || a` → `(yield p) || a`. But assignment RHS and comma
    /// expression operands accept `AssignmentExpression` (which includes yield),
    /// so no extra parens are needed there.
    #[test]
    fn yield_from_await_no_extra_parens_in_assignment_rhs() {
        let source = r#"async function func() { o.a = await p; }"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("o.a = yield p;"),
            "yield-from-await in assignment RHS must NOT have extra parens.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(yield p)"),
            "yield-from-await in assignment RHS should not be wrapped in parens.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 9238e6c3bbdb5446609d2532fb3ce18fe5089d33b9fdabae1bb31238aaaac703

// TSZ_INLINE_TEST_BEGIN 372f6a854d18f5b77e7e6517c31451277f9bc7fdb2bd4b75ef985ddb80671ca9 144 yield_from_await_no_extra_parens_in_comma_expr
    /// Yield-from-await in comma expression LHS should not have extra parens.
    /// `(await p, a)` → `(yield p, a)`, NOT `((yield p), a)`.
    #[test]
    fn yield_from_await_no_extra_parens_in_comma_expr() {
        let source = r#"async function func() { var b = (await p, a); }"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p, a)"),
            "yield-from-await in comma expression must NOT have extra parens.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("((yield p)"),
            "yield-from-await should not be double-wrapped.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 372f6a854d18f5b77e7e6517c31451277f9bc7fdb2bd4b75ef985ddb80671ca9

// TSZ_INLINE_TEST_BEGIN c2de7f5c89abc87409be4b036405dcd709402a7421455a4326f1bdcd02d8afec 167 yield_from_await_keeps_parens_in_binary_operator
    /// Yield-from-await inside a binary operator like `||` still NEEDS parens.
    /// `await p || a` → `(yield p) || a` (otherwise it would parse as `yield (p || a)`).
    #[test]
    fn yield_from_await_keeps_parens_in_binary_operator() {
        let source = r#"async function func() { var b = await p || a; }"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p) || a"),
            "yield-from-await in || operand MUST have parens for correct precedence.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END c2de7f5c89abc87409be4b036405dcd709402a7421455a4326f1bdcd02d8afec

// TSZ_INLINE_TEST_BEGIN 21b220d0be7f2531e8359149a23161b7256bcec9b1d8f2a83b9945f00e943e46 187 invalid_await_in_function_emits_yield_for_es2015
    /// The ES2017 transformer rewrites non-top-level `await` expressions to
    /// `yield` for targets below ES2017, even when the surrounding function is
    /// missing `async` and the checker reports a recovery error.
    #[test]
    fn invalid_await_in_function_emits_yield_for_es2015() {
        let source = "function f() {\n    await 1;\n}\nawait 2;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("function f() {\n    yield 1;\n}"),
            "Non-top-level await should downlevel to yield for ES2015.\nOutput:\n{output}"
        );
        assert!(
            output.contains("await 2;"),
            "Top-level await should stay as module syntax.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 21b220d0be7f2531e8359149a23161b7256bcec9b1d8f2a83b9945f00e943e46

// TSZ_INLINE_TEST_BEGIN 1bdfb25f84b31aa750358c34cd67297a9b63cb94860e32e6fcfa92b9b20d768d 209 yield_expression_comments_preserve_expected_spacing
    /// Preserve spacing and ordering around comments in `yield` expressions.
    #[test]
    fn yield_expression_comments_preserve_expected_spacing() {
        let source = r#"function * foo2() {
            /*comment1*/ yield 1;
            yield /*comment2*/ 2;
            yield 3 /*comment3*/
            yield */*comment4*/ [4];
            yield /*comment5*/* [5];
        }"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("/*comment1*/ yield 1;"),
            "Leading comment before `yield` should stay before keyword with spacing.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield /*comment2*/ 2"),
            "Inline comment after `yield` should keep a single separating space.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield 3; /*comment3*/"),
            "Trailing comment should remain after expression when `yield` has no right operand.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield* /*comment4*/ [4]"),
            "Comment after `yield*` should stay after `*`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("yield /*comment5*/* [5]"),
            "Comment before `yield*` operator should stay before `*`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 1bdfb25f84b31aa750358c34cd67297a9b63cb94860e32e6fcfa92b9b20d768d

// TSZ_INLINE_TEST_BEGIN 781a1adbd0de0ee3669002801b4dea542ebd0f10a710b4a23860b5e1f64dd101 248 yield_without_operand_has_no_trailing_space
    #[test]
    fn yield_without_operand_has_no_trailing_space() {
        let source = "function* foo() {\n    yield;\n}\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("yield;"),
            "Yield without an operand must keep tight `yield;` form.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("yield ;"),
            "Yield without an operand must not include a separating space.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 781a1adbd0de0ee3669002801b4dea542ebd0f10a710b4a23860b5e1f64dd101

// TSZ_INLINE_TEST_BEGIN 58efcc659985a2e13e548efc152be708d4510e2017a9daeeb72d83e9cc069f72 272 yield_preserves_parens_for_line_comment_in_type_assertion
    /// When a parenthesized type assertion wraps a line comment between `yield`
    /// and its operand, the parens must be preserved to prevent ASI.
    /// `yield (// comment\n a as any)` -> `yield (\n// comment\n a)` (not `yield // comment\n a`)
    #[test]
    fn yield_preserves_parens_for_line_comment_in_type_assertion() {
        let source =
            "function *t1() {\n    yield (\n        // comment\n        a as any\n    );\n}\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("yield ("),
            "yield with line comment before operand must preserve opening paren.\nOutput:\n{output}"
        );
        assert!(
            output.contains("// comment"),
            "Line comment must be preserved in output.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("yield // comment"),
            "yield must not be directly followed by the line comment (ASI hazard).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 58efcc659985a2e13e548efc152be708d4510e2017a9daeeb72d83e9cc069f72

// TSZ_INLINE_TEST_BEGIN a1731600d2061dc3125ec317df33a6ed53ef2f381e02c43eca29bc02556cf3e0 300 yield_preserves_same_line_open_paren_comment_in_type_assertion
    /// A line comment that starts on the same source line as `(` should remain
    /// on that line; the following newline still prevents ASI after `yield`.
    #[test]
    fn yield_preserves_same_line_open_paren_comment_in_type_assertion() {
        let source =
            "const value = 1;\nfunction* g(): any {\n  yield ( // keep\n    value as any);\n}\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("yield ( // keep\n    value);"),
            "Same-line comment after `yield (` should stay on the opening paren line.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("yield (\n    // keep"),
            "Same-line comment after `yield (` must not be forced onto a new line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END a1731600d2061dc3125ec317df33a6ed53ef2f381e02c43eca29bc02556cf3e0

// TSZ_INLINE_TEST_BEGIN b2a9a98e69e9f110009c878028ea509bfe8849da74de02fbe9aa6eb4de61ec3c 324 inline_block_comment_before_statement_gets_trailing_space
    /// Block comments on the same line as a statement must have a space after `*/`.
    /// This ensures `/*comment*/ var x` rather than `/*comment*/var x`.
    #[test]
    fn inline_block_comment_before_statement_gets_trailing_space() {
        // A block comment on the same line as a var declaration
        let source = "{\n    /*comment*/ var x = 1;\n}\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("/*comment*/ var"),
            "Inline block comment must have a space before the next token.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("/*comment*/var"),
            "Block comment must not be glued to the next token.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END b2a9a98e69e9f110009c878028ea509bfe8849da74de02fbe9aa6eb4de61ec3c

// TSZ_INLINE_TEST_BEGIN 5a99479da0751340f5294f6e5ab6f8f0fd2c4129469a165a0aed65918ea270db 348 conditional_preserves_newline_after_colon
    /// Multiline ternary: colon trailing on previous line, alternate on next.
    /// `a ? b :\n    c` must preserve the line break after `:`.
    #[test]
    fn conditional_preserves_newline_after_colon() {
        let source = "var v = a ? b :\n  c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ? b :\n"),
            "Ternary with colon trailing must preserve newline after `:`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    c"),
            "Alternate must be indented on the new line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 5a99479da0751340f5294f6e5ab6f8f0fd2c4129469a165a0aed65918ea270db

// TSZ_INLINE_TEST_BEGIN 43eac52a159508f3353620d03d48c23f07636a1bb3ad184736399b3f0ebff5ba 371 conditional_preserves_newline_before_colon
    /// Multiline ternary: colon leading on new line.
    /// `a ? b\n    : c` must preserve the line break before `:`.
    #[test]
    fn conditional_preserves_newline_before_colon() {
        let source = "var v = a ? b\n  : c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ? b\n"),
            "Ternary with colon leading must preserve newline before `:`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c"),
            "Colon must lead on the new indented line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 43eac52a159508f3353620d03d48c23f07636a1bb3ad184736399b3f0ebff5ba

// TSZ_INLINE_TEST_BEGIN 86ce6837cd97f49ccec9c530393f9860996e8bc8b0aa19ae57829d1641974dcd 394 conditional_preserves_newline_before_question_and_colon
    /// Multiline ternary: both `?` and `:` on new lines.
    /// `a\n    ? b\n    : c` must preserve both line breaks.
    #[test]
    fn conditional_preserves_newline_before_question_and_colon() {
        let source = "var v = a\n  ? b\n  : c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a\n"),
            "Must preserve newline after condition.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    ? b\n"),
            "Question mark must lead on the new indented line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c"),
            "Colon must lead on the new indented line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 86ce6837cd97f49ccec9c530393f9860996e8bc8b0aa19ae57829d1641974dcd

// TSZ_INLINE_TEST_BEGIN 6fd131d06c4860eaf25488417a7a802137455cf0f8b827e996a204a71c9fbb2d 419 conditional_preserves_operand_comments_before_question_and_colon
    #[test]
    fn conditional_preserves_operand_comments_before_question_and_colon() {
        let source = r#"function f(x: string | number | boolean) {
    return typeof x !== "string"
        && (typeof x !== "number" // number | boolean
        ? x // boolean
        : x === 10);
}
"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("typeof x !== \"number\" // number | boolean\n            ? x // boolean\n            : x === 10"),
            "Conditional operand comments should stay before `?` and `:`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6fd131d06c4860eaf25488417a7a802137455cf0f8b827e996a204a71c9fbb2d

// TSZ_INLINE_TEST_BEGIN 4fea406e6578ddc078547e1894836f000cd368164490f8bf3e2dcdb2180fad67 442 conditional_comment_line_before_question_keeps_continuation_indent
    #[test]
    fn conditional_comment_line_before_question_keeps_continuation_indent() {
        let source = r#"function f(x: string | number | boolean) {
    return typeof x !== "string"
        && (typeof x === "number"
        // change value
        ? ((x = 10) && x.toString())
        : x);
}
"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(
                "typeof x === \"number\"\n            // change value\n            ? ((x = 10)"
            ),
            "Comment-only lines before `?` should align with the continuation operator.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4fea406e6578ddc078547e1894836f000cd368164490f8bf3e2dcdb2180fad67

// TSZ_INLINE_TEST_BEGIN e386eb5413f4a57e995f163e38beef8d167dae80a411920757953f38deeea073 468 parenthesized_return_comment_waits_for_inserted_semicolon
    #[test]
    fn parenthesized_return_comment_waits_for_inserted_semicolon() {
        let source = r#"function f(x: string | number | boolean) {
    return (typeof x !== "number"
        ? x
        : x === 10) // number
}
"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(": x === 10); // number"),
            "Inserted return semicolon should precede the trailing line comment.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END e386eb5413f4a57e995f163e38beef8d167dae80a411920757953f38deeea073

// TSZ_INLINE_TEST_BEGIN f6b0b89ab04c4227df08c61d088e204e5c9940f56c5a1cd8010645aa094b84b7 490 binary_continuation_operator_preserves_left_line_comment
    #[test]
    fn binary_continuation_operator_preserves_left_line_comment() {
        let source = r#"function foo(x: string | number | boolean) {
    return typeof x !== "string" // string | number | boolean
        && typeof x !== "number" // number | boolean
        && x; // boolean
}
"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        let lines: Vec<&str> = output.lines().collect();

        assert!(
            lines
                .iter()
                .any(|line| line.trim_end().ends_with("// string | number | boolean")),
            "The first `&&` should remain on the continuation line after the left operand comment.\nOutput:\n{output}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim_start().starts_with("&& typeof")),
            "The first `&&` should start the continuation line.\nOutput:\n{output}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim_end().ends_with("// number | boolean")),
            "Line comments before continuation operators should stay on the operand line.\nOutput:\n{output}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim_start().starts_with("&& x;")),
            "The second `&&` should start the continuation line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END f6b0b89ab04c4227df08c61d088e204e5c9940f56c5a1cd8010645aa094b84b7

// TSZ_INLINE_TEST_BEGIN 7a8069d683112f47f8f9ea310d508839dc2c3c6b8d6be7bf92c4d6f1ada4d7ca 533 binary_or_continuation_operator_preserves_left_line_comment
    #[test]
    fn binary_or_continuation_operator_preserves_left_line_comment() {
        let source = r#"function foo(x: string | number | boolean) {
    return typeof x === "string" // string | number | boolean
        || typeof x === "number" // number | boolean
        || x; // boolean
}
"#;

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        let lines: Vec<&str> = output.lines().collect();

        assert!(
            lines
                .iter()
                .any(|line| line.trim_end().ends_with("// string | number | boolean")),
            "The first `||` should remain on the continuation line after the left operand comment.\nOutput:\n{output}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim_start().starts_with("|| typeof")),
            "The first `||` should start the continuation line.\nOutput:\n{output}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim_end().ends_with("// number | boolean")),
            "Line comments before `||` continuation operators should stay on the operand line.\nOutput:\n{output}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.trim_start().starts_with("|| x;")),
            "The second `||` should start the continuation line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 7a8069d683112f47f8f9ea310d508839dc2c3c6b8d6be7bf92c4d6f1ada4d7ca

// TSZ_INLINE_TEST_BEGIN 853a32f3751bbef3e738fb23fb27cfac73261c459bf017258da91849e45755ca 578 type_assertion_call_expression_strips_parens
    /// Type assertion around a call expression should strip parens:
    /// `(<any>a.b()).c` → `a.b().c` (not `(a.b()).c`).
    #[test]
    fn type_assertion_call_expression_strips_parens() {
        let source = "var b = (<any>a.b()).c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a.b().c"),
            "Parens around type-asserted call expression should be stripped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(a.b()).c"),
            "Should not have redundant parens around call expression.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 853a32f3751bbef3e738fb23fb27cfac73261c459bf017258da91849e45755ca

// TSZ_INLINE_TEST_BEGIN 1331aa5e55cc36ef0da43d701c3e2701eaab43eb156a415fdd48b3b07fb8de05 601 type_assertion_new_expression_strips_parens
    /// Type assertion around `new` expression strips parens when not in access position:
    /// `(<any>new a)` → `new a`.
    #[test]
    fn type_assertion_new_expression_strips_parens() {
        let source = "var b = (<any>new a);\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a;"),
            "Parens around type-asserted new expression should be stripped.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 1331aa5e55cc36ef0da43d701c3e2701eaab43eb156a415fdd48b3b07fb8de05

// TSZ_INLINE_TEST_BEGIN 26844303815bc3162068c57393e57cedd92ed3f741e2ed963a0a16dab239e044 620 type_assertion_new_expression_with_member_strips_parens
    /// Type assertion around `new a.b` strips parens when not in access position:
    /// `(<any>new a.b)` → `new a.b`.
    #[test]
    fn type_assertion_new_expression_with_member_strips_parens() {
        let source = "var b = (<any>new a.b);\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a.b;"),
            "Parens around type-asserted new a.b should be stripped.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 26844303815bc3162068c57393e57cedd92ed3f741e2ed963a0a16dab239e044

// TSZ_INLINE_TEST_BEGIN 57e0f50fa2bf0c445a87248ad8b3be0838a537ce7cd3819cce321d0020820386 638 invalid_new_type_assertion_callee_preserves_recovery_text
    /// Invalid `new <T>Expr()` preserves the recovered type assertion text.
    #[test]
    fn invalid_new_type_assertion_callee_preserves_recovery_text() {
        let source = "var b = new <any>Test2();\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new  < any > Test2();"),
            "Recovered type assertion in new callee should be preserved.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 57e0f50fa2bf0c445a87248ad8b3be0838a537ce7cd3819cce321d0020820386

// TSZ_INLINE_TEST_BEGIN a8e1df9feae180236a7bdb1abf74dbd127a14b1df45ac930a0b08dc8f9bfbe64 657 type_assertion_new_expression_keeps_parens_in_access
    /// Type assertion around `new a` keeps parens when in property access position:
    /// `(<any>new a).b` → `(new a).b` (removing parens would change semantics).
    #[test]
    fn type_assertion_new_expression_keeps_parens_in_access() {
        let source = "var b = (<any>new a).b;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(new a).b"),
            "Parens around new expression in access position must be preserved.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END a8e1df9feae180236a7bdb1abf74dbd127a14b1df45ac930a0b08dc8f9bfbe64

// TSZ_INLINE_TEST_BEGIN 9512bdd1efbc9b526e79f0d828f4a6fd3cb9674cd5851d2eb930b54a7380d736 676 type_assertion_call_in_new_callee_keeps_parens
    /// Type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())` (not `new x()` which has different semantics).
    #[test]
    fn type_assertion_call_in_new_callee_keeps_parens() {
        let source = "new (x() as any);\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around call expression in new callee must be preserved.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("new x()"),
            "Should NOT strip parens to `new x()` (different semantics).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 9512bdd1efbc9b526e79f0d828f4a6fd3cb9674cd5851d2eb930b54a7380d736

// TSZ_INLINE_TEST_BEGIN a1e9874a902234f00fca11aca5e2e1eb6c9fa8bac59b09d9c1aa9859f7b15271 699 as_assertion_call_in_new_callee_keeps_parens
    /// `as` type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())`.
    #[test]
    fn as_assertion_call_in_new_callee_keeps_parens() {
        // Use angle-bracket style too: `new (<any>x())`
        let source = "new (<any>x());\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around angle-bracket-asserted call in new callee must be preserved.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END a1e9874a902234f00fca11aca5e2e1eb6c9fa8bac59b09d9c1aa9859f7b15271

// TSZ_INLINE_TEST_BEGIN 24c6a3b631b52dec06f51cef75df575bd142a9d168adf0f4dc279f0aa18c4c37 719 type_assertion_call_outside_new_still_strips_parens
    /// Call expressions with type assertions outside `new` context still strip parens:
    /// `(<any>x()).foo` → `x().foo`.
    #[test]
    fn type_assertion_call_outside_new_still_strips_parens() {
        let source = "var b = (<any>x()).foo;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("x().foo"),
            "Parens around type-asserted call in access position should still strip.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(x()).foo"),
            "Should not have redundant parens.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 24c6a3b631b52dec06f51cef75df575bd142a9d168adf0f4dc279f0aa18c4c37

// TSZ_INLINE_TEST_BEGIN cc7dd2d456f78317b7f0d2fa0d8503e2c4ba1351428d1bd288fba534f544eca5 744 nullish_coalescing_emits_hoisted_temp_var_decl
    /// When lowering nullish coalescing (`??`) to ES2019 and below for complex
    /// (non-identifier) LHS expressions, the emitter uses a temp variable:
    /// `(temp = f()) !== null && temp !== void 0 ? temp : 'fallback'`
    /// This temp must be declared as `var _a;` at the top of the enclosing scope.
    #[test]
    fn nullish_coalescing_emits_hoisted_temp_var_decl() {
        // Top-level: hoisted temp goes at file scope
        let source = "let gg = f() ?? 'foo';\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _a;"),
            "Nullish coalescing lowering must emit `var _a;` for the hoisted temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(_a = f())"),
            "Nullish coalescing lowering must use temp in assignment.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END cc7dd2d456f78317b7f0d2fa0d8503e2c4ba1351428d1bd288fba534f544eca5

// TSZ_INLINE_TEST_BEGIN 6d8df87f0951ff4fb64fa8e45f0218809ecef9bec2cc491d3ec4a97427be61b2 768 prefix_plus_plus_gets_space
    /// Nested unary `+` operators must be separated by a space to prevent
    /// `+ +y` from collapsing to `++y` (pre-increment).
    #[test]
    fn prefix_plus_plus_gets_space() {
        let source = "var z = + +y;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ +y"),
            "Nested unary `+` must have space between to avoid `++y`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("++y"),
            "Must NOT collapse `+ +y` into `++y` (pre-increment).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6d8df87f0951ff4fb64fa8e45f0218809ecef9bec2cc491d3ec4a97427be61b2

// TSZ_INLINE_TEST_BEGIN 21882ec361518d4e9d78a21b23959b056d969a495da496d11e342dccba18e59e 791 prefix_minus_minus_gets_space
    /// Nested unary `-` operators must be separated by a space to prevent
    /// `- -y` from collapsing to `--y` (pre-decrement).
    #[test]
    fn prefix_minus_minus_gets_space() {
        let source = "var c = - -y;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("- -y"),
            "Nested unary `-` must have space between to avoid `--y`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("--y"),
            "Must NOT collapse `- -y` into `--y` (pre-decrement).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 21882ec361518d4e9d78a21b23959b056d969a495da496d11e342dccba18e59e

// TSZ_INLINE_TEST_BEGIN 546864ecefd65fd56d2ea3b321bfd310ad671b3f5ed749796e9055e6b9b2a994 813 prefix_plus_before_increment_gets_space
    /// Unary `+` before `++` must insert a space: `+ ++x` not `+++x`.
    #[test]
    fn prefix_plus_before_increment_gets_space() {
        let source = "var z = + ++x;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ ++x"),
            "Unary `+` before `++x` must have space.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 546864ecefd65fd56d2ea3b321bfd310ad671b3f5ed749796e9055e6b9b2a994

// TSZ_INLINE_TEST_BEGIN 6998d8969b88977a870da901cfe0692ec5222f02b2a4698d71b6ab5ada551b23 836 conditional_case_a_trailing_colon
    /// Case A with trailing colon: `a ?\n  b :\n  c` → `a ?\n    b :\n    c`
    /// This is the conditionalExpressionNewLine7 pattern.
    #[test]
    fn conditional_case_a_trailing_colon() {
        let source = "var v = a ?\n  b :\n  c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ?\n"),
            "Case A: `?` must trail on condition line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    b :\n"),
            "Case A: `:` must trail on when_true line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    c;"),
            "Case A: when_false must be indented on new line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 6998d8969b88977a870da901cfe0692ec5222f02b2a4698d71b6ab5ada551b23

// TSZ_INLINE_TEST_BEGIN 4de7975be4ef8623ba824eae52e364098d92f5808201cb5b3163f9c653f5a912 862 conditional_case_a_inline_colon
    /// Case A with same-line colon: `a ?\n  b : c` → `a ?\n    b : c`
    #[test]
    fn conditional_case_a_inline_colon() {
        let source = "var v = a ?\n  b : c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ?\n"),
            "Case A: `?` must trail on condition line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    b : c;"),
            "Case A: `:` and when_false inline.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4de7975be4ef8623ba824eae52e364098d92f5808201cb5b3163f9c653f5a912

// TSZ_INLINE_TEST_BEGIN d3edc4f8d1c3784f942d7b8071b113840b3f4a8507bb1f6e482b03989d0654d5 883 conditional_case_a_missing_false_branch_breaks_before_semicolon
    #[test]
    fn conditional_case_a_missing_false_branch_breaks_before_semicolon() {
        let source = "function f() {\n    return true ?\n        : ;\n}\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("return true ?\n        :\n    ;"),
            "Missing false branch should keep `:` and the return semicolon on separate lines.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END d3edc4f8d1c3784f942d7b8071b113840b3f4a8507bb1f6e482b03989d0654d5

// TSZ_INLINE_TEST_BEGIN 3ca513aeefeb95436e0f884662fb2600b32ebb0b1f018d42218f4b89add91cf8 901 conditional_case_b_nested_ternaries
    /// Case B with nested ternaries: `a\n  ? b ? d : e\n  : c ? f : g`
    #[test]
    fn conditional_case_b_nested_ternaries() {
        let source = "var v = a\n  ? b ? d : e\n  : c ? f : g;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("    ? b ? d : e\n"),
            "Case B: nested when_true must be on indented line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c ? f : g;"),
            "Case B: nested when_false must be on indented line.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 3ca513aeefeb95436e0f884662fb2600b32ebb0b1f018d42218f4b89add91cf8

// TSZ_INLINE_TEST_BEGIN 7d758587b4ff54132af235deba940515451a96c8b9dcaa376f6e351ffefee4a3 926 nullish_coalescing_in_binary_gets_parens
    /// When `??` is lowered in a binary expression operand (e.g., `(a ?? b) || c`),
    /// the lowered ternary must be wrapped in parens to preserve precedence.
    /// Without parens: `a !== null && a !== void 0 ? a : b || c` (wrong — `||` binds to `b`)
    /// With parens: `(a !== null && a !== void 0 ? a : b) || c` (correct)
    #[test]
    fn nullish_coalescing_in_binary_gets_parens() {
        // a ?? b || c — the ?? is the left operand of ||, needs parens when lowered
        let source = "a ?? b || c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(a !== null && a !== void 0 ? a : b) || c"),
            "Lowered ?? in binary operand must be wrapped in parens.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 7d758587b4ff54132af235deba940515451a96c8b9dcaa376f6e351ffefee4a3

// TSZ_INLINE_TEST_BEGIN f666ccef5e316238aa44735471eae6d4a78f7ccba540d3b4d531373b03777943 947 nullish_coalescing_in_conditional_condition_gets_parens
    /// When `??` is lowered in the condition of a ternary, the lowered ternary
    /// must be wrapped in parens to avoid ambiguity with the outer `?:`.
    /// e.g., `a ?? 'foo' ? 1 : 2` → `(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2`
    #[test]
    fn nullish_coalescing_in_conditional_condition_gets_parens() {
        let source = "const r = a ?? 'foo' ? 1 : 2;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2"),
            "Lowered ?? in conditional condition must be wrapped in parens.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END f666ccef5e316238aa44735471eae6d4a78f7ccba540d3b4d531373b03777943

// TSZ_INLINE_TEST_BEGIN dbb49a8528d3112264a525d3a0f75dec8d5a1723df7a8064fcd507caba19c5ee 967 nullish_coalescing_with_explicit_parens_no_double_wrap
    /// When the source already has explicit parens `(a ?? b)`, the lowered ternary
    /// must NOT be double-parenthesized. The `ParenthesizedExpression` provides the
    /// outer parens; the `nullish_coalescing_needs_parens` flag is cleared inside.
    #[test]
    fn nullish_coalescing_with_explicit_parens_no_double_wrap() {
        // Source has explicit parens: (a ?? b) || c
        let source = "(a ?? b) || c;\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should have single parens, not double
        assert!(
            output.contains("(a !== null && a !== void 0 ? a : b) || c"),
            "Must have single parens from source ParenthesizedExpression.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("((a !== null"),
            "Must NOT have double parens.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END dbb49a8528d3112264a525d3a0f75dec8d5a1723df7a8064fcd507caba19c5ee

// TSZ_INLINE_TEST_BEGIN bdbdfcc262f3f68f31ae8378a4a2889e6f4c2e172e28ad4fd53a829938b1241c 990 binary_missing_rhs_no_spurious_indent
    #[test]
    fn binary_missing_rhs_no_spurious_indent() {
        // Source: `[#abc]=` — private identifier in array destructuring context followed by
        // `=` with no RHS (parse error, TS1109). A trailing newline after `=` makes the
        // binary emitter detect `has_newline_after_op`. The missing RHS must NOT cause the
        // statement-level `;` to be indented. Matches tsc baseline output.
        let (parser, root) = parse_test_source("[#abc]=\n");
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text("[#abc]=\n");
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("[#abc] =\n;"),
            "Missing RHS after `=` must not produce spurious indent before `;`.\nOutput:\n{output}"
        );
        // Also verify with a different spelling to ensure the fix is structural, not name-based
        let (parser2, root2) = parse_test_source("[#xyz]=\n");
        let mut printer2 = Printer::new(&parser2.arena, PrintOptions::default());
        printer2.set_source_text("[#xyz]=\n");
        printer2.print(root2);
        let output2 = printer2.finish().code;
        assert!(
            output2.contains("[#xyz] =\n;"),
            "Missing RHS with different name must also have no spurious indent.\nOutput:\n{output2}"
        );
    }
// TSZ_INLINE_TEST_END bdbdfcc262f3f68f31ae8378a4a2889e6f4c2e172e28ad4fd53a829938b1241c
