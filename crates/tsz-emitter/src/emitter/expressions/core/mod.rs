mod helpers;
mod private_fields;

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

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

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("return (\n        /* keep */ this.client\n            .getThing());"),
            "Multiline parenthesized erased assertion should preserve its comment layout.\nOutput:\n{output}"
        );
    }

    /// Dynamic `import('path')` expressions must emit the `import` keyword.
    /// Previously the emitter's `emit_node_by_kind` dispatch had no handler for
    /// `SyntaxKind::ImportKeyword`, so the keyword was silently dropped and the
    /// output became just `('path')`.
    #[test]
    fn dynamic_import_emits_import_keyword() {
        let source = r#"const m = import("./module");"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./module")"#),
            "Dynamic import must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// `import.meta` property access must emit the `import` keyword.
    #[test]
    fn import_meta_emits_import_keyword() {
        let source = r#"const url = import.meta.url;"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("import.meta.url"),
            "import.meta must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// Dynamic import inside an async function body.
    #[test]
    fn dynamic_import_in_async_function() {
        let source = r#"async function load() { return await import("./lib"); }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./lib")"#),
            "Dynamic import inside async function must emit 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// When async functions are lowered to generator functions (ES2015 target),
    /// `await expr` becomes `yield expr`. Yield has lower precedence than most
    /// operators, so it needs parens inside binary operators like `||`:
    /// `await p || a` → `(yield p) || a`. But assignment RHS and comma
    /// expression operands accept `AssignmentExpression` (which includes yield),
    /// so no extra parens are needed there.
    #[test]
    fn yield_from_await_no_extra_parens_in_assignment_rhs() {
        let source = r#"async function func() { o.a = await p; }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Yield-from-await in comma expression LHS should not have extra parens.
    /// `(await p, a)` → `(yield p, a)`, NOT `((yield p), a)`.
    #[test]
    fn yield_from_await_no_extra_parens_in_comma_expr() {
        let source = r#"async function func() { var b = (await p, a); }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Yield-from-await inside a binary operator like `||` still NEEDS parens.
    /// `await p || a` → `(yield p) || a` (otherwise it would parse as `yield (p || a)`).
    #[test]
    fn yield_from_await_keeps_parens_in_binary_operator() {
        let source = r#"async function func() { var b = await p || a; }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p) || a"),
            "yield-from-await in || operand MUST have parens for correct precedence.\nOutput:\n{output}"
        );
    }

    /// The ES2017 transformer rewrites non-top-level `await` expressions to
    /// `yield` for targets below ES2017, even when the surrounding function is
    /// missing `async` and the checker reports a recovery error.
    #[test]
    fn invalid_await_in_function_emits_yield_for_es2015() {
        let source = "function f() {\n    await 1;\n}\nawait 2;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    #[test]
    fn yield_without_operand_has_no_trailing_space() {
        let source = "function* foo() {\n    yield;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// When a parenthesized type assertion wraps a line comment between `yield`
    /// and its operand, the parens must be preserved to prevent ASI.
    /// `yield (// comment\n a as any)` -> `yield (\n// comment\n a)` (not `yield // comment\n a`)
    #[test]
    fn yield_preserves_parens_for_line_comment_in_type_assertion() {
        let source =
            "function *t1() {\n    yield (\n        // comment\n        a as any\n    );\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Block comments on the same line as a statement must have a space after `*/`.
    /// This ensures `/*comment*/ var x` rather than `/*comment*/var x`.
    #[test]
    fn inline_block_comment_before_statement_gets_trailing_space() {
        // A block comment on the same line as a var declaration
        let source = "{\n    /*comment*/ var x = 1;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Multiline ternary: colon trailing on previous line, alternate on next.
    /// `a ? b :\n    c` must preserve the line break after `:`.
    #[test]
    fn conditional_preserves_newline_after_colon() {
        let source = "var v = a ? b :\n  c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Multiline ternary: colon leading on new line.
    /// `a ? b\n    : c` must preserve the line break before `:`.
    #[test]
    fn conditional_preserves_newline_before_colon() {
        let source = "var v = a ? b\n  : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Multiline ternary: both `?` and `:` on new lines.
    /// `a\n    ? b\n    : c` must preserve both line breaks.
    #[test]
    fn conditional_preserves_newline_before_question_and_colon() {
        let source = "var v = a\n  ? b\n  : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Type assertion around a call expression should strip parens:
    /// `(<any>a.b()).c` → `a.b().c` (not `(a.b()).c`).
    #[test]
    fn type_assertion_call_expression_strips_parens() {
        let source = "var b = (<any>a.b()).c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Type assertion around `new` expression strips parens when not in access position:
    /// `(<any>new a)` → `new a`.
    #[test]
    fn type_assertion_new_expression_strips_parens() {
        let source = "var b = (<any>new a);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a;"),
            "Parens around type-asserted new expression should be stripped.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new a.b` strips parens when not in access position:
    /// `(<any>new a.b)` → `new a.b`.
    #[test]
    fn type_assertion_new_expression_with_member_strips_parens() {
        let source = "var b = (<any>new a.b);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a.b;"),
            "Parens around type-asserted new a.b should be stripped.\nOutput:\n{output}"
        );
    }

    /// Invalid `new <T>Expr()` preserves the recovered type assertion text.
    #[test]
    fn invalid_new_type_assertion_callee_preserves_recovery_text() {
        let source = "var b = new <any>Test2();\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new  < any > Test2();"),
            "Recovered type assertion in new callee should be preserved.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new a` keeps parens when in property access position:
    /// `(<any>new a).b` → `(new a).b` (removing parens would change semantics).
    #[test]
    fn type_assertion_new_expression_keeps_parens_in_access() {
        let source = "var b = (<any>new a).b;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(new a).b"),
            "Parens around new expression in access position must be preserved.\nOutput:\n{output}"
        );
    }

    /// Type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())` (not `new x()` which has different semantics).
    #[test]
    fn type_assertion_call_in_new_callee_keeps_parens() {
        let source = "new (x() as any);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// `as` type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())`.
    #[test]
    fn as_assertion_call_in_new_callee_keeps_parens() {
        // Use angle-bracket style too: `new (<any>x())`
        let source = "new (<any>x());\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around angle-bracket-asserted call in new callee must be preserved.\nOutput:\n{output}"
        );
    }

    /// Call expressions with type assertions outside `new` context still strip parens:
    /// `(<any>x()).foo` → `x().foo`.
    #[test]
    fn type_assertion_call_outside_new_still_strips_parens() {
        let source = "var b = (<any>x()).foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// When lowering nullish coalescing (`??`) to ES2019 and below for complex
    /// (non-identifier) LHS expressions, the emitter uses a temp variable:
    /// `(temp = f()) !== null && temp !== void 0 ? temp : 'fallback'`
    /// This temp must be declared as `var _a;` at the top of the enclosing scope.
    #[test]
    fn nullish_coalescing_emits_hoisted_temp_var_decl() {
        // Top-level: hoisted temp goes at file scope
        let source = "let gg = f() ?? 'foo';\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Nested unary `+` operators must be separated by a space to prevent
    /// `+ +y` from collapsing to `++y` (pre-increment).
    #[test]
    fn prefix_plus_plus_gets_space() {
        let source = "var z = + +y;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Nested unary `-` operators must be separated by a space to prevent
    /// `- -y` from collapsing to `--y` (pre-decrement).
    #[test]
    fn prefix_minus_minus_gets_space() {
        let source = "var c = - -y;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Unary `+` before `++` must insert a space: `+ ++x` not `+++x`.
    #[test]
    fn prefix_plus_before_increment_gets_space() {
        let source = "var z = + ++x;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ ++x"),
            "Unary `+` before `++x` must have space.\nOutput:\n{output}"
        );
    }

    // =====================================================================
    // Case A ternary formatting tests (question on condition line)
    // =====================================================================

    /// Case A with trailing colon: `a ?\n  b :\n  c` → `a ?\n    b :\n    c`
    /// This is the conditionalExpressionNewLine7 pattern.
    #[test]
    fn conditional_case_a_trailing_colon() {
        let source = "var v = a ?\n  b :\n  c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Case A with same-line colon: `a ?\n  b : c` → `a ?\n    b : c`
    #[test]
    fn conditional_case_a_inline_colon() {
        let source = "var v = a ?\n  b : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// Case B with nested ternaries: `a\n  ? b ? d : e\n  : c ? f : g`
    #[test]
    fn conditional_case_b_nested_ternaries() {
        let source = "var v = a\n  ? b ? d : e\n  : c ? f : g;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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

    /// When `??` is lowered in a binary expression operand (e.g., `(a ?? b) || c`),
    /// the lowered ternary must be wrapped in parens to preserve precedence.
    /// Without parens: `a !== null && a !== void 0 ? a : b || c` (wrong — `||` binds to `b`)
    /// With parens: `(a !== null && a !== void 0 ? a : b) || c` (correct)
    #[test]
    fn nullish_coalescing_in_binary_gets_parens() {
        // a ?? b || c — the ?? is the left operand of ||, needs parens when lowered
        let source = "a ?? b || c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(a !== null && a !== void 0 ? a : b) || c"),
            "Lowered ?? in binary operand must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// When `??` is lowered in the condition of a ternary, the lowered ternary
    /// must be wrapped in parens to avoid ambiguity with the outer `?:`.
    /// e.g., `a ?? 'foo' ? 1 : 2` → `(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2`
    #[test]
    fn nullish_coalescing_in_conditional_condition_gets_parens() {
        let source = "const r = a ?? 'foo' ? 1 : 2;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2"),
            "Lowered ?? in conditional condition must be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// When the source already has explicit parens `(a ?? b)`, the lowered ternary
    /// must NOT be double-parenthesized. The `ParenthesizedExpression` provides the
    /// outer parens; the `nullish_coalescing_needs_parens` flag is cleared inside.
    #[test]
    fn nullish_coalescing_with_explicit_parens_no_double_wrap() {
        // Source has explicit parens: (a ?? b) || c
        let source = "(a ?? b) || c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

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
}
