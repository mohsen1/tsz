use crate::output::printer::{PrintOptions, Printer};
use tsz_parser::ParserState;

/// Case clause with a single non-block statement on the same source line
/// should be emitted on one line: `case true: return "true";`
#[test]
fn case_clause_same_line_non_block_statement() {
    let source = r#"function f(x: boolean) {
    switch (x) {
        case true: return "true";
        case false: return "false";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains(r#"case true: return "true";"#),
        "Case clause with single statement on same line should stay on one line.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"case false: return "false";"#),
        "Case clause with single statement on same line should stay on one line.\nOutput:\n{output}"
    );
}

/// Case clause with a statement on a different line should be indented normally.
#[test]
fn case_clause_multiline_stays_indented() {
    let source = r#"function f(x: number) {
    switch (x) {
        case 1:
            return "one";
        case 2:
            return "two";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // Should NOT be on same line
    assert!(
        !output.contains("case 1: return"),
        "Case clause with statement on next line should remain multi-line.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 1:\n"),
        "Case clause should have newline after colon.\nOutput:\n{output}"
    );
}

/// Default clause with same-line statement should also be emitted on one line.
#[test]
fn default_clause_same_line_statement() {
    let source = r#"function f(x: number) {
    switch (x) {
        case 1: return "one";
        default: return "other";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains(r#"default: return "other";"#),
        "Default clause with single statement on same line should stay on one line.\nOutput:\n{output}"
    );
}

/// Case clause with a block on the same line should still work (existing behavior).
#[test]
fn case_clause_same_line_block_statement() {
    let source = r#"function f(x: number) {
    switch (x) {
        case 0: { break; }
        default: break;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("case 0: {"),
        "Case clause with block on same line should stay on one line.\nOutput:\n{output}"
    );
}

#[test]
fn ts_check_comment_preserved_in_output() {
    let source = "// @ts-check\nvar x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// @ts-check"),
        "// @ts-check directive should be preserved in output.\nOutput:\n{output}"
    );
}

#[test]
fn ts_nocheck_comment_preserved_in_output() {
    let source = "// @ts-nocheck\nvar x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// @ts-nocheck"),
        "// @ts-nocheck directive should be preserved in output.\nOutput:\n{output}"
    );
}

#[test]
fn test_at_directive_comments_preserved() {
    // tsc preserves all source-level `// @` comments in JS output.
    // The test harness strips actual test directives from the baseline
    // source before the emitter sees them, so any `// @` comment
    // in the source is a legitimate comment to preserve.
    let source = "// @target: esnext\nvar x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// @target"),
        "// @target directive should be preserved in output (tsc preserves all source comments).\nOutput:\n{output}"
    );
}

#[test]
fn test_ts_ignore_directive_preserved() {
    // // @ts-ignore is a runtime directive that tsc preserves.
    let source = "// @ts-ignore\nvar x: number = 'hello';\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// @ts-ignore"),
        "// @ts-ignore directive should be preserved in output.\nOutput:\n{output}"
    );
}

#[test]
fn test_ts_expect_error_directive_preserved() {
    // // @ts-expect-error is a runtime directive that tsc preserves.
    let source = "// @ts-expect-error\nvar x: number = 'hello';\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// @ts-expect-error"),
        "// @ts-expect-error directive should be preserved in output.\nOutput:\n{output}"
    );
}

/// Comments before case/default clauses should appear before the label,
/// not inside the clause body. tsc emits:
///   // comment
///   case X:
/// not:
///   case X:
///       // comment
#[test]
fn case_clause_leading_comment_before_label() {
    let source = r#"function f(x: number) {
    switch (x) {
        // First case
        case 0:
            return "zero";
        // Second case
        case 1:
            return "one";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // Comment must appear BEFORE the case keyword, not after.
    // The case clause is indented 2 levels (8 spaces) inside function + switch.
    assert!(
        output.contains("// First case\n        case 0:"),
        "Leading comment should appear before 'case 0:', not inside the body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("// Second case\n        case 1:"),
        "Leading comment should appear before 'case 1:', not inside the body.\nOutput:\n{output}"
    );
}

/// Comment before default clause should appear before 'default:', not inside the body.
#[test]
fn default_clause_leading_comment_before_label() {
    let source = r#"function f(x: number) {
    switch (x) {
        case 0:
            return "zero";
        // Fallback
        default:
            return "other";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// Fallback\n        default:"),
        "Leading comment should appear before 'default:', not inside the body.\nOutput:\n{output}"
    );
}

/// Trailing comment on opening `{` of a block should stay on the same line.
/// e.g. `if (cond) { // comment` should NOT become `if (cond) {\n    // comment`.
#[test]
fn trailing_comment_on_opening_brace_if_statement() {
    let source = r#"function f(x: string) {
    if (typeof x === "Object") { // comparison is OK
        console.log(x);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("{ // comparison is OK"),
        "Trailing comment should stay on the same line as opening brace.\nOutput:\n{output}"
    );
}

/// Trailing comment on opening `{` of a for-in loop body block.
#[test]
fn trailing_comment_on_opening_brace_for_in() {
    let source = r#"function f(x: object) {
    for (const key in x) { // iterate
        console.log(key);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("{ // iterate"),
        "Trailing comment should stay on the same line as opening brace.\nOutput:\n{output}"
    );
}

/// tsc drops trailing comments on function body opening `{`.
/// `function foo(x: number) { // comment` should emit `function foo(x) {` (no comment).
#[test]
fn function_body_brace_comment_suppressed() {
    let source = r#"function foo(x: number) { // param comment
    return x;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("// param comment"),
        "Trailing comment on function body `{{` should be suppressed.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return x;"),
        "Function body should still be emitted.\nOutput:\n{output}"
    );
}

/// tsc drops trailing comments on method body opening `{`, but preserves
/// trailing comments on control-flow blocks inside the method.
#[test]
fn method_body_brace_comment_suppressed_but_inner_block_preserved() {
    let source = r#"class C {
    foo(_i: number, ...rest) { // error
        if (true) { // ok
            var _i = 10;
        }
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("{ // error"),
        "Trailing comment on method body `{{` should be suppressed.\nOutput:\n{output}"
    );
    assert!(
        output.contains("{ // ok"),
        "Trailing comment on if-block `{{` should be preserved.\nOutput:\n{output}"
    );
}

/// tsc drops trailing comments on arrow function body opening `{`.
#[test]
fn arrow_function_body_brace_comment_suppressed() {
    let source = r#"const fn = (x: number) => { // arrow comment
    return x;
};"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("// arrow comment"),
        "Trailing comment on arrow function body `{{` should be suppressed.\nOutput:\n{output}"
    );
}

/// Empty function body with trailing comment on `{` should suppress the comment.
/// tsc: `function f4(_i, ...rest) {\n}` (comment dropped)
#[test]
fn empty_function_body_brace_comment_suppressed() {
    let source = "function f4(_i: any, ...rest) { // error\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("// error"),
        "Trailing comment on empty function body `{{` should be suppressed.\nOutput:\n{output}"
    );
}

/// Empty method body with comment should also be suppressed.
#[test]
fn empty_method_body_brace_comment_suppressed() {
    let source = "class C {\n    foo() { // comment\n    }\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("// comment"),
        "Trailing comment on empty method body `{{` should be suppressed.\nOutput:\n{output}"
    );
}

/// Control-flow empty blocks should still preserve comments.
#[test]
fn empty_if_block_comment_preserved() {
    let source = "function f() {\n    if (true) { // keep this\n    }\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// keep this"),
        "Trailing comment on control-flow empty block should be preserved.\nOutput:\n{output}"
    );
}

/// Empty method body with inner comment on a DIFFERENT line from `{` should
/// preserve the comment.  tsc: `foo() {\n    //return 4;\n}`
/// (This is distinct from same-line comments on `{` which ARE suppressed.)
#[test]
fn empty_method_body_inner_comment_on_next_line_preserved() {
    let source = "class Foo {\n    foo(): number {\n        //return 4;\n    }\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("//return 4;"),
        "Inner comment on a different line from `{{` in an empty method body \
         should be preserved (tsc preserves these).\nOutput:\n{output}"
    );
}

/// Empty constructor body with inner comment on a different line should
/// preserve the comment.  tsc: `constructor(x) {\n    // comment\n}`
#[test]
fn empty_constructor_body_inner_comment_preserved() {
    let source =
        "class Foo {\n    constructor(x: any) {\n        // WScript.Echo(\"test\");\n    }\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("// WScript.Echo"),
        "Inner comment in empty constructor body should be preserved.\nOutput:\n{output}"
    );
}

/// Single-line empty function body with same-line block comment should still
/// suppress the comment.  tsc: `bar1() { }` (comment dropped)
#[test]
fn empty_method_body_single_line_comment_still_suppressed() {
    let source = "class A {\n    bar1() { /*WScript.Echo(\"bar1\");*/ }\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("WScript"),
        "Same-line block comment in single-line empty method body should be \
         suppressed (tsc drops these).\nOutput:\n{output}"
    );
}

#[test]
fn accessor_object_literal_empty_body() {
    let source = "export const t1 = {\n    p: 'value',\n    get getter() {\n        return 'value';\n    }\n}\nexport const t2 = {\n    set setter(v) {}\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("set setter(v) { }"),
        "Accessor object-literal bodies without statements should be emitted as `{{ }}`.\nOutput:\n{output}"
    );
    // tsc always adds semicolons after variable statements
    assert!(
        output.contains("};"),
        "Variable statements with object literal initializers should end with `}};`.\nOutput:\n{output}"
    );
}

#[test]
fn accessor_object_literal_in_js_file_gets_trailing_semicolon() {
    let source = "export const t1 = {\n    p: 'value',\n    get getter() {\n        return 'value';\n    }\n}\nexport const t2 = {\n    set setter(v) {}\n}\nexport const t3 = {\n    get value() {\n        return 'value';\n    },\n    set value(v) {}\n}\n";

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut parser_output_printer = Printer::new(&parser.arena, PrintOptions::default());
    parser_output_printer.set_source_text(source);
    parser_output_printer.print(root);
    let output = parser_output_printer.finish().code;

    assert!(
        output.contains("set setter(v) {}"),
        "JS input should keep compact empty accessor formatting.\nOutput:\n{output}"
    );
    // tsc always emits trailing semicolons on variable declarations, even when
    // the source uses ASI. Our emitter must match.
    assert!(
        output.contains("};"),
        "JS input object-literal declarations must get trailing semicolons (matching tsc).\nOutput:\n{output}"
    );
}

// =========================================================================
// Trailing comments after semicolons on statement types
// =========================================================================

#[test]
fn trailing_comment_on_return_statement() {
    let source = "function f() {\n    return 42; // the answer\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("return 42; // the answer"),
        "Trailing comment on return should stay on the same line.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_comment_on_bare_return() {
    let source = "function f() {\n    return; // early exit\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("return; // early exit"),
        "Trailing comment on bare return should stay on the same line.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_comment_on_throw_statement() {
    let source = "function f() {\n    throw new Error(); // kaboom\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("throw new Error(); // kaboom"),
        "Trailing comment on throw should stay on the same line.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_comment_on_break_statement() {
    let source = r#"function f(x: number) {
    switch (x) {
        case 0:
            break; // done
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("break; // done"),
        "Trailing comment on break should stay on the same line.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_comment_on_continue_statement() {
    let source = r#"function f() {
    for (var i = 0; i < 10; i++) {
        continue; // skip
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("continue; // skip"),
        "Trailing comment on continue should stay on the same line.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_comment_on_do_while_statement() {
    let source = r#"function f() {
    var i = 0;
    do {
        i++;
    } while (i < 10); // loop end
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("while (i < 10); // loop end"),
        "Trailing comment on do-while should stay on the same line.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_comment_on_debugger_statement() {
    let source = "function f() {\n    debugger; // breakpoint\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("debugger; // breakpoint"),
        "Trailing comment on debugger should stay on the same line.\nOutput:\n{output}"
    );
}

/// Multi-line JSDoc comments inside class bodies should have their continuation
/// lines reindented to match the output indentation level.
/// Source uses 2-space indent, output uses 4-space indent.
#[test]
fn jsdoc_comment_reindented_in_class_body() {
    let source =
        "class C {\n  /**\n   * @type {number}\n   */\n  get bar(): number { return 1; }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The JSDoc continuation lines should be at 4-space indent + 1 relative space
    assert!(
        output.contains("    /**\n     * @type {number}\n     */"),
        "JSDoc continuation lines should be reindented to match output indent.\nOutput:\n{output}"
    );
}

/// When static class properties are lowered to `static { this.p1 = ""; }` blocks
/// inside the class body, their leading JSDoc comments should preserve class-level indent.
#[test]
fn jsdoc_comment_reindented_for_lowered_static_field() {
    let source = "class test {\n    /**\n     * p1 comment\n     */\n    static p1 = \"\";\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The static field is lowered to a static block inside the class,
    // so JSDoc should keep class-level indent (4 spaces)
    assert!(
        output.contains("/**\n     * p1 comment\n     */"),
        "JSDoc on lowered static field should preserve class-level indent.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static { this.p1 = \"\"; }"),
        "Static field should be lowered to a static block.\nOutput:\n{output}"
    );
}

/// Multi-line comments at top-level should preserve their content without
/// extra indentation being added.
#[test]
fn multiline_comment_top_level_preserved() {
    let source = "/*\n * top level comment\n */\nvar x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("/*\n * top level comment\n */"),
        "Top-level multi-line comment should be preserved.\nOutput:\n{output}"
    );
}

/// Non-block else body should be on a new indented line,
/// e.g., `else\n    return;` — matching tsc behavior.
#[test]
fn else_non_block_body_on_new_line() {
    let source = r#"function f(x: number) {
    if (x > 0)
        x++;
    else
        return;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("else\n        return;"),
        "Non-block else body should be on a new indented line.\nOutput:\n{output}"
    );
    // Must NOT produce `else return;` on the same line
    assert!(
        !output.contains("else return;"),
        "Non-block else body should NOT be on the same line as 'else'.\nOutput:\n{output}"
    );
}

/// Block else body should remain on the same line as `else`.
#[test]
fn else_block_body_on_same_line() {
    let source = r#"function f(x: number) {
    if (x > 0) {
        x++;
    } else {
        return;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("else {"),
        "Block else body should stay on the same line as 'else'.\nOutput:\n{output}"
    );
}

/// `else if` should remain on the same line as `else`.
#[test]
fn else_if_on_same_line() {
    let source = r#"function f(x: number) {
    if (x > 0)
        x++;
    else if (x < 0)
        x--;
    else
        return;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("else if (x < 0)"),
        "'else if' should stay on the same line.\nOutput:\n{output}"
    );
    assert!(
        output.contains("else\n"),
        "Final else with non-block body should be on new indented line.\nOutput:\n{output}"
    );
}

/// `declare import a = b;` should suppress the spurious `declare;` expression
/// statement and only emit the runtime import-equals binding.
#[test]
fn declare_modifier_on_import_suppressed() {
    let source = "declare import a = b;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("declare;"),
        "`declare;` should be suppressed when it's a modifier artifact.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var a = b;"),
        "The runtime binding should still be emitted.\nOutput:\n{output}"
    );
}

/// `declare declare var x;` inside a namespace should produce an empty body
/// (both `declare;` and `declare var x;` are erased).
#[test]
fn declare_declare_var_in_namespace_erased() {
    let source = r#"namespace M {
    declare declare var x;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("declare;"),
        "`declare;` should be suppressed inside namespace body.\nOutput:\n{output}"
    );
}

/// Legitimate `declare;` as a variable expression (with ASI on a new line)
/// should NOT be suppressed.
#[test]
fn declare_as_identifier_preserved() {
    // `declare` on its own line followed by a newline is a legitimate expression
    // statement using `declare` as a variable name (ASI terminates the statement).
    let source = "var declare = 5;\ndeclare;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("declare;"),
        "Legitimate `declare;` expression should be preserved.\nOutput:\n{output}"
    );
}

/// Comment on the line after the last statement but before `}` is preserved
/// inside the function body by tsc, at the block's indentation level.
#[test]
fn comment_before_closing_brace_stays_inside_function() {
    let source = "function foo(x: number): void {\n    return;\n    // trailing comment\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("    // trailing comment\n}"),
        "Comment before closing brace should stay inside the function body (tsc behavior).\nOutput:\n{output}"
    );
}

/// Comment after `return` and before `}` in a complex expression function
/// is preserved inside the function body by tsc.
#[test]
fn comment_before_closing_brace_after_return_expression() {
    let source = "function foo(p: number | null): number | null {\n    return p !== undefined ? p : null;\n    // Still typed as number | null\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("    // Still typed as number | null\n}"),
        "Comment before closing brace should stay inside the function body (tsc behavior).\nOutput:\n{output}"
    );
}

/// Multiple comments between the last statement and `}` are preserved
/// inside the function body by tsc.
#[test]
fn multiple_comments_before_closing_brace() {
    let source = "function foo(): void {\n    const x = 1;\n    // first comment\n    // second comment\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("    // first comment\n    // second comment\n}"),
        "Multiple comments before closing brace should stay inside the function body (tsc behavior).\nOutput:\n{output}"
    );
}

/// tsc always expands control-flow blocks (for, while, if, do) to multi-line,
/// even when the source code has them on a single line.
#[test]
fn for_loop_single_line_block_expands_to_multiline() {
    let source = r#"for (var i = 0;;) { throw i; }"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("{\n    throw i;\n}"),
        "for-loop single-line block should expand to multi-line.\nOutput:\n{output}"
    );
}

#[test]
fn if_single_line_block_expands_to_multiline() {
    let source = r#"if (x < 0) { throw new Error(); }"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("{\n    throw new Error();\n}"),
        "if-statement single-line block should expand to multi-line.\nOutput:\n{output}"
    );
}

#[test]
fn while_single_line_block_expands_to_multiline() {
    let source = r#"while (true) { break; }"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("{\n    break;\n}"),
        "while-loop single-line block should expand to multi-line.\nOutput:\n{output}"
    );
}

/// Function body single-line blocks should STAY single-line (tsc preserves these).
#[test]
fn function_body_single_line_stays_single_line() {
    let source = r#"function f() { return 1; }"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("{ return 1; }"),
        "Function body single-line block should stay single-line.\nOutput:\n{output}"
    );
}

/// Trailing comment scan for the last statement in a block must not overshoot
/// into comments belonging to the closing `}` line.
#[test]
fn trailing_comment_capped_at_block_close_brace() {
    let source = "function f() {\n    return 1; // return comment\n} // end of function\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The statement's trailing comment should stay with the statement
    assert!(
        output.contains("return 1; // return comment"),
        "Statement's trailing comment should be preserved.\nOutput:\n{output}"
    );
    // The closing brace comment must NOT be stolen by the statement
    assert!(
        !output.contains("return 1; // return comment // end of function"),
        "Closing brace comment must not be stolen by last statement.\nOutput:\n{output}"
    );
}

/// `using` declarations at `ESNext` target should have a trailing semicolon,
/// just like var/let/const. The semicolon was previously skipped because the
/// ES5 lowering path (`__addDisposableResource`) handles its own termination,
/// but when `using` passes through unchanged at ES2025+ it needs a semicolon.
#[test]
fn using_declaration_has_semicolon_at_esnext() {
    let source = "using x = getResource();\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Default target is ESNext, which supports ES2025 `using` natively
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("using x = getResource();"),
        "using declaration at ESNext should have trailing semicolon.\nOutput:\n{output}"
    );
}

/// Variable declarations with object literal initializers must always get a
/// trailing semicolon — even for `.js` source files (allowJs). Previously a
/// bug skipped the semicolon for JS sources with object-literal initialisers,
/// producing `}` instead of `};` at the end of the declaration.
#[test]
fn variable_declaration_object_literal_gets_semicolon() {
    let source = "const x = {\n  grey: {}\n};\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("};"),
        "Object literal variable declaration must end with `}};`.\nOutput:\n{output}"
    );
}

/// Same as above but the source file uses ASI (no explicit semicolon after `}`).
/// tsc always emits the semicolon regardless of the source's ASI usage.
#[test]
fn variable_declaration_object_literal_asi_still_gets_semicolon() {
    let source = "const x = {\n  grey: {}\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("};"),
        "Object literal variable declaration (ASI source) must still end with `}};`.\nOutput:\n{output}"
    );
}

/// `await using` declarations at `ESNext` target should also have a trailing semicolon.
#[test]
fn await_using_declaration_has_semicolon_at_esnext() {
    let source = "await using x = getResource();\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("await using x = getResource();"),
        "await using declaration at ESNext should have trailing semicolon.\nOutput:\n{output}"
    );
}

/// Comments inside erased `as` type annotations should not leak into JS output.
/// `expr as /* comment */ T` should emit `expr`, not `expr /* comment */`.
#[test]
fn as_expression_comment_in_type_skipped() {
    let source = "var x = (1 as /* type comment */ number);\nvar y = 2;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("type comment"),
        "Comment inside erased `as` type annotation should not appear in JS output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var x = 1;"),
        "Expression value should be preserved.\nOutput:\n{output}"
    );
}

/// Comments inside erased `satisfies` type annotations should not leak into JS output.
#[test]
fn satisfies_expression_comment_in_type_skipped() {
    let source = "var x = (42 satisfies /* check */ number);\nvar y = 2;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("check"),
        "Comment inside erased `satisfies` type annotation should not appear in JS output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var x = 42;"),
        "Expression value should be preserved.\nOutput:\n{output}"
    );
}

/// Comments inside `<T>` prefix type assertions are emitted before the expression.
/// `</* comment */T>expr` should emit `/* comment */ expr` (tsc behavior).
#[test]
fn type_assertion_prefix_comment_emitted() {
    let source = "var x = </* cast */ any>42;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("/* cast */"),
        "Comment inside `<T>` type assertion should be preserved (tsc behavior).\nOutput:\n{output}"
    );
}

/// `export as namespace X;` is a TypeScript-only UMD global declaration.
/// It must be completely erased in JS output, and any attached comments
/// must not leak into the output.
#[test]
fn namespace_export_declaration_erased_in_js() {
    let source = "export function foo() {}\n// ns export comment\nexport as namespace myLib;\nexport function bar() {}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("namespace"),
        "`export as namespace` should be erased in JS output.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("ns export comment"),
        "Comments attached to erased `export as namespace` should not leak.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export function foo()"),
        "Other exports should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export function bar()"),
        "Other exports should be preserved.\nOutput:\n{output}"
    );
}

/// `export as namespace X;` with a block comment on the same line should
/// also be erased completely.
#[test]
fn namespace_export_declaration_inline_comment_erased() {
    let source = "export function foo() {}\nexport as namespace myLib; /* global */\nexport function bar() {}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("namespace"),
        "`export as namespace` should be erased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("global"),
        "Trailing comment on erased `export as namespace` should not leak.\nOutput:\n{output}"
    );
}
