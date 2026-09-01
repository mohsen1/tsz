//! Unit tests for property/element access emit. Split out of `access.rs` to
//! keep the production module under the 2000-line limit (§19).

use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::output::printer::{PrintOptions, Printer};
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn emit_es6(source: &str) -> String {
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

fn emit_js(source: &str) -> String {
    let (parser, root) = parse_test_source(source);

    let mut printer = EmitterPrinter::with_options(&parser.arena, PrinterOptions::default());
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn js_emit_comment_positions_around_names_and_property_access() {
    let output = emit_js(
        "function /*1*/makePoint(x: number) {}\nvar /*2*/point = makePoint(2);\nvar y = point./*3*/x;\n",
    );

    assert!(
        output.contains("function makePoint(x)"),
        "Comment before a function declaration name should be erased, not reattached to the first parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var /*2*/ point = makePoint(2);"),
        "Comment before a variable declaration name should stay after `var`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("point. /*3*/x"),
        "Comment after a property-access dot should stay after the dot.\nOutput:\n{output}"
    );
}

#[test]
fn property_access_preserves_comments_between_base_and_dot() {
    let output = emit_es6(
        r#"let z = this.then(x => result)/*S*/.then(x => "abc")/*string*/.then(x => x.length)/*number*/;"#,
    );

    assert!(
            output.contains(
                r#"this.then(x => result) /*S*/.then(x => "abc") /*string*/.then(x => x.length) /*number*/"#
            ),
            "Comments between a property-access base and dot must stay before the dot.\nOutput:\n{output}"
        );
}

#[test]
fn property_access_dot_locator_skips_comment_dots() {
    let output = emit_es6(r#"const y = point/* has . in comment */.x;"#);

    assert!(
        output.contains("/* has . in comment */.x") || output.contains("/* has . in comment */ .x"),
        "Dot lookup should skip dots inside comments and keep the member-access dot after the comment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(". /* has . in comment */x"),
        "Dot lookup must not treat comment text as the member-access dot.\nOutput:\n{output}"
    );
}

#[test]
fn property_access_line_comment_before_dot_stays_with_callee_chain() {
    let output = emit_es6(
        "const result = values.map((arr) => arr // keep with arr\n    .filter((obj) => obj) // keep with body\n);\n",
    );

    assert!(
        output.contains("arr // keep with arr\n    .filter((obj) => obj)"),
        "Line comments before a member-access dot should stay before the dot, not move into the call arguments.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(".filter(// keep with arr"),
        "Call argument comment scanning must start at the actual argument-list paren.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".filter((obj) => obj) // keep with body"),
        "Trailing comments on concise arrow body expressions should stay with the body before the outer call closes.\nOutput:\n{output}"
    );
}

#[test]
fn property_access_own_line_comment_before_dot_uses_single_newline() {
    let output = emit_es6(
        "const result = values.map((arr) => arr\n    // keep with arr\n    .filter((obj) => obj));\n",
    );

    assert!(
        output.contains("arr\n    // keep with arr\n    .filter((obj) => obj)"),
        "Own-line comments before a member-access dot should keep exactly the source line break before the dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("arr\n\n    // keep with arr"),
        "Property-access comment emission should not pre-write a newline and replay the same leading trivia.\nOutput:\n{output}"
    );
}

#[test]
fn erased_object_literal_assertion_call_keeps_call_inside_grouping() {
    let output = emit_js("class A { }\n(<A>{}).toString();\n");

    assert!(
        output.contains("({}.toString());"),
        "Call on erased object-literal assertion should stay inside object-literal grouping.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({}.toString)())"),
        "Call on erased object-literal assertion should not parenthesize the callee separately.\nOutput:\n{output}"
    );
}

/// When lowering optional property access (`?.`) to ES2019 and below for
/// complex base expressions, the emitter uses a temp variable:
/// `(temp = expr) === null || temp === void 0 ? void 0 : temp.prop`
/// This temp must be declared as `var _a;` at the top of the enclosing scope.
#[test]
fn optional_chain_emits_hoisted_temp_var_decl() {
    // Multi-line function body to exercise the function-scoped hoisting path
    let source = "function h() {\n    let x = getObj()?.value;\n    return x;\n}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var _a;"),
        "Optional chain lowering must emit `var _a;` for the hoisted temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = getObj())"),
        "Optional chain lowering must use temp in assignment.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_new_optional_chain_lowers_as_optional_access_on_new_base() {
    let source = "class A { b(x?: number) {} }\nnew A?.b();\nnew A?.b(1);\nnew A?.b.c;\nnew A?.[\"b\"].c;\nnew A()?.b();\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(_a = new A) === null || _a === void 0 ? void 0 : _a.b();"),
        "Invalid `new A?.b()` should lower as optional access on `new A`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = new A) === null || _b === void 0 ? void 0 : _b.b(1);"),
        "Invalid `new A?.b(1)` should keep call arguments on the optional tail.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_c = new A) === null || _c === void 0 ? void 0 : _c.b.c;"),
        "Invalid `new A?.b.c` should keep the non-optional property tail in the branch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_d = new A) === null || _d === void 0 ? void 0 : _d[\"b\"].c;"),
        "Invalid `new A?.[\"b\"].c` should keep element and property tails in the branch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_e = new A()) === null || _e === void 0 ? void 0 : _e.b();"),
        "Valid `new A()?.b()` should keep the constructed base expression.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_new_optional_chain_preserves_parent_context_and_callee_grouping() {
    let source =
        "declare function makeCtor(): any;\nnew A?.b() + 1;\nnew (makeCtor() as any)?.b();\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("((_a = new A) === null || _a === void 0 ? void 0 : _a.b()) + 1;"),
        "Invalid-new optional chain should be grouped as a binary operand.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = new (makeCtor())) === null || _b === void 0 ? void 0 : _b.b();"),
        "Invalid-new optional chain should preserve call grouping in the constructed base.\nOutput:\n{output}"
    );
}

/// Optional method call on a simple identifier should NOT use a temp variable.
/// `o?.b()` → `o === null || o === void 0 ? void 0 : o.b()` (no `_a`).
#[test]
fn optional_method_call_simple_identifier_no_temp() {
    let source = "declare const o: any;\no?.b();\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("o === null || o === void 0 ? void 0 : o.b()"),
        "Simple identifier should be used directly, no temp var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a"),
        "No temp variable should be allocated for simple identifier.\nOutput:\n{output}"
    );
}

/// Optional method call with `.call()` on a simple identifier should use the
/// identifier directly as the `.call()` receiver.
/// `o.b?.()` → `(_a = o.b) === null || _a === void 0 ? void 0 : _a.call(o)`
#[test]
fn optional_call_simple_receiver_uses_identifier_in_call() {
    let source = "declare const o: any;\no.b?.();\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains(".call(o)"),
        "Simple identifier should be used directly as .call() receiver.\nOutput:\n{output}"
    );
    // Should only have one temp (_a for the method), not two
    assert!(
        !output.contains("_b"),
        "Only one temp var should be allocated.\nOutput:\n{output}"
    );
}

/// A normal call whose callee is a parenthesized optional member access
/// still needs tsc's method-call binding preservation:
/// `(o?.b)(x)` -> `(o === null || o === void 0 ? void 0 : o.b).call(o, x)`.
#[test]
fn parenthesized_optional_member_call_preserves_simple_receiver() {
    let source = "const o = { b(n: number) { return n; } };\n(o?.b)(1);\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o.b).call(o, 1)"),
        "Parenthesized optional member callee must preserve `this` with .call(o).\nOutput:\n{output}"
    );
}

/// When the final member receiver is produced inside the optional branch,
/// capture that receiver once and use it as the `.call(...)` receiver.
#[test]
fn parenthesized_optional_member_call_preserves_nested_receiver() {
    let source = "\
const o = { nested() { return { b(n: number) { return n; } }; } };
(o?.nested().b)(2);
";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : (_a = o.nested()).b).call(_a, 2)"),
        "Parenthesized optional member callee must capture the nested receiver for .call(_a).\nOutput:\n{output}"
    );
}

#[test]
fn parenthesized_optional_element_call_preserves_nested_receiver() {
    let source = "\
const o = { nested() { return { b(n: number) { return n; } }; } };
(o?.nested()[\"b\"])(2);
";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains(
            "(o === null || o === void 0 ? void 0 : (_a = o.nested())[\"b\"]).call(_a, 2)"
        ),
        "Parenthesized optional element callee must capture the nested receiver for .call(_a).\nOutput:\n{output}"
    );
}

#[test]
fn parenthesized_optional_tail_property_call_preserves_tail_receiver() {
    let source = "\
const o = { a: { b(n: number) { return n; } } };
(o?.a.b)(1);
";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : (_a = o.a).b).call(_a, 1)"),
        "Parenthesized optional tail property callee must capture `o.a` for .call(_a).\nOutput:\n{output}"
    );
}

#[test]
fn parenthesized_optional_tail_element_call_preserves_tail_receiver() {
    let source = "\
const o = { a: { b(n: number) { return n; } } };
(o?.a[\"b\"])(1);
";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : (_a = o.a)[\"b\"]).call(_a, 1)"),
        "Parenthesized optional tail element callee must capture `o.a` for .call(_a).\nOutput:\n{output}"
    );
}

/// Complex (non-identifier) expression in optional method call MUST use a temp.
/// `f()?.b()` needs a temp to avoid calling `f()` twice.
#[test]
fn optional_method_call_complex_expr_uses_temp() {
    let source = "declare function f(): any;\nf()?.b();\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("_a = f()"),
        "Complex expression must be captured in temp var.\nOutput:\n{output}"
    );
    assert!(
        output.contains("=== null"),
        "Must have null check.\nOutput:\n{output}"
    );
}

/// When a downlevel optional chain is used as a ternary condition,
/// the lowered ternary must be wrapped in parens to preserve precedence.
/// `o?.b ? 1 : 0` → `(o === null || o === void 0 ? void 0 : o.b) ? 1 : 0`
/// Without parens: `o === null || o === void 0 ? void 0 : o.b ? 1 : 0`
/// would parse as the wrong ternary nesting.
#[test]
fn optional_chain_in_ternary_condition_gets_parens() {
    let source = "declare const o: any;\no?.b ? 1 : 0;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o.b) ? 1 : 0"),
        "Lowered optional chain in ternary condition must be wrapped in parens.\nOutput:\n{output}"
    );
}

/// When a downlevel optional chain is used as an operand of `===`,
/// the lowered ternary must be wrapped in parens.
/// `o?.x === 1` → `(o === null || o === void 0 ? void 0 : o.x) === 1`
#[test]
fn optional_chain_in_binary_equals_gets_parens() {
    let source = "declare const o: any;\no?.x === 1;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o.x) === 1"),
        "Lowered optional chain in === operand must be wrapped in parens.\nOutput:\n{output}"
    );
}

/// When a downlevel optional chain is used with postfix `++`,
/// the lowered ternary must be wrapped in parens.
/// `o?.a++` → `(o === null || o === void 0 ? void 0 : o.a)++`
#[test]
fn optional_chain_in_postfix_increment_gets_parens() {
    let source = "declare const o: any;\no?.a++;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o.a)++"),
        "Lowered optional chain in postfix ++ must be wrapped in parens.\nOutput:\n{output}"
    );
}

/// When a downlevel optional chain with a non-optional tail is used with
/// postfix `++`/`--`, the whole access path remains in the ternary branch.
/// `o?.a.b++` -> `(o === null || o === void 0 ? void 0 : o.a.b)++`
#[test]
fn optional_chain_in_postfix_update_keeps_tail_inside_branch() {
    let source = "declare const o: any;\no?.a.b++;\no?.a[0]--;\no?.[\"a\"]++;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o.a.b)++"),
        "Postfix update must keep property tail inside optional-chain branch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o.a[0])--"),
        "Postfix update must keep element tail inside optional-chain branch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(o === null || o === void 0 ? void 0 : o[\"a\"])++"),
        "Postfix update must support optional element roots.\nOutput:\n{output}"
    );
}

/// Prefix `++`/`--` also wraps the complete lowered optional-chain path.
/// `++o?.a.b` -> `++(o === null || o === void 0 ? void 0 : o.a.b)`
#[test]
fn optional_chain_in_prefix_update_keeps_tail_inside_branch() {
    let source = "declare const o: any;\n++o?.a.b;\n--o?.a[0];\n++o?.[\"a\"];\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2019,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("++(o === null || o === void 0 ? void 0 : o.a.b)"),
        "Prefix update must keep property tail inside optional-chain branch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("--(o === null || o === void 0 ? void 0 : o.a[0])"),
        "Prefix update must keep element tail inside optional-chain branch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("++(o === null || o === void 0 ? void 0 : o[\"a\"])"),
        "Prefix update must support optional element roots.\nOutput:\n{output}"
    );
}

#[test]
fn optional_chain_array_rest_assignment_uses_rest_lowering() {
    let source = "declare const obj: any;\ndeclare const foo: any;\n[...obj?.[\"a\"]] = [];\n[...obj?.a[\"b\"]] = [];\n[...obj[foo?.bar]] = [];\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("obj === null || obj === void 0 ? void 0 : obj[\"a\"] = [].slice(0);"),
        "Optional element rest targets should still use ES5 rest-assignment lowering.\nOutput:\n{output}"
    );
    assert!(
        output.contains("obj === null || obj === void 0 ? void 0 : obj.a[\"b\"] = [].slice(0);"),
        "Optional-chain rest targets should keep non-optional tails inside the lowered assignment target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("obj[foo === null || foo === void 0 ? void 0 : foo.bar] = [].slice(0);"),
        "Optional chains inside computed keys are valid element targets and must stay on the normal rest-lowering path.\nOutput:\n{output}"
    );
}

// =====================================================================
// write_dot_token: numeric literal double-dot disambiguation
// =====================================================================

/// Plain integer property access needs `..` to disambiguate from float literal.
/// `1.toString()` is a syntax error; `1..toString()` is correct.
#[test]
fn numeric_literal_property_access_plain_integer() {
    let source = "1 .foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("1..foo"),
        "Plain integer property access must use `..`.\nOutput:\n{output}"
    );
}

/// A source newline between an integer literal and the property dot
/// already disambiguates the access, so tsc keeps a single emitted dot.
#[test]
fn numeric_literal_property_access_newline_before_dot_uses_single_dot() {
    let source = "3\n    .foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("3\n    .foo"),
        "Newline before property dot should use one dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("..foo"),
        "Newline-separated numeric access must not use `..`.\nOutput:\n{output}"
    );
}

/// A preserved comment between an integer literal and `.` also separates
/// the tokens, so the property access writes one dot.
#[test]
fn numeric_literal_property_access_preserved_comment_before_dot_uses_single_dot() {
    let source = "0 /* comment */.foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("0 /* comment */.foo"),
        "Preserved comment should separate integer literal from property dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("/* comment */..foo"),
        "Comment-separated numeric access must not use `..` while comments are preserved.\nOutput:\n{output}"
    );
}

/// A line comment before the dot stays attached to the numeric literal;
/// only the following property dot moves to the next line.
#[test]
fn numeric_literal_property_access_preserved_line_comment_stays_inline() {
    let source = "3 // comment\n    .foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("3 // comment\n    .foo"),
        "Line comment before property dot should stay on the numeric literal line.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("3\n    // comment"),
        "Line comment must not be moved to its own line.\nOutput:\n{output}"
    );
}

/// When comments are removed, the separator disappears and integer
/// property access must go back to `..`.
#[test]
fn numeric_literal_property_access_removed_comment_before_dot_uses_double_dot() {
    let source = "0 /* comment */.foo;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        remove_comments: true,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("0..foo"),
        "Removed comment should require `..` for integer property access.\nOutput:\n{output}"
    );
}

/// Removing comments must not erase a source newline that separated an
/// integer literal from the property dot.
#[test]
fn numeric_literal_property_access_removed_comment_after_newline_uses_single_dot() {
    let source = "3\n    /* comment */ .foo;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        remove_comments: true,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("3\n    .foo"),
        "Source newline should still separate integer literal from property dot when comments are removed.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("3..foo"),
        "Removed comment after newline must not collapse numeric access to `..`.\nOutput:\n{output}"
    );
}

/// Erased type wrappers should use the inner numeric literal when deciding
/// whether source trivia still separates the number from the property dot.
#[test]
fn numeric_literal_property_access_erased_wrapper_preserved_comment_uses_single_dot() {
    let source = "(<any>3) /* comment */.foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("3 /* comment */.foo"),
        "Preserved comment after erased type assertion should separate integer literal from property dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("/* comment */..foo"),
        "Comment-separated erased wrapper access must not use `..` while comments are preserved.\nOutput:\n{output}"
    );
}

/// If comments are removed and no source newline survives, an erased
/// wrapper around an integer literal still needs the double-dot form.
#[test]
fn numeric_literal_property_access_erased_wrapper_removed_comment_uses_double_dot() {
    let source = "(3 as any) /* comment */.foo;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        remove_comments: true,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("3..foo"),
        "Removed comment after erased `as` expression should require `..` for integer property access.\nOutput:\n{output}"
    );
}

/// A source newline survives comment removal even when it crosses an erased
/// `satisfies` wrapper.
#[test]
fn numeric_literal_property_access_erased_wrapper_removed_comment_keeps_newline_separator() {
    let source = "(3 satisfies number)\n    /* comment */.foo;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        remove_comments: true,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("3\n    .foo"),
        "Source newline after erased `satisfies` expression should keep a single property dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("3..foo"),
        "Surviving newline after erased wrapper must not collapse numeric access to `..`.\nOutput:\n{output}"
    );
}

/// Float literals already have a `.`, so they need only one dot for property access.
#[test]
fn numeric_literal_property_access_float() {
    let source = "1.0 .foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("1.0.foo"),
        "Float literal should use single dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("1.0..foo"),
        "Float literal must NOT use double-dot.\nOutput:\n{output}"
    );
}

/// Exponent literals (e.g., `1e0`) don't need `..` because the exponent
/// part prevents the parser from treating the dot as part of the number.
#[test]
fn numeric_literal_property_access_exponent() {
    let source = "1e0 .foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("1e0..foo"),
        "Exponent literal must NOT use double-dot.\nOutput:\n{output}"
    );
}

/// Downleveled decimal/exponent spellings can emit as plain integers.
/// The dot decision must be based on emitted text: `08.8e5.foo` becomes
/// `880000..foo`, not `880000.foo`.
#[test]
fn downleveled_numeric_literal_property_access_plain_integer() {
    let source = "08.8e5 .foo;\n";

    let (parser, root) = parse_test_source(source);

    let opts = PrintOptions {
        target: tsz_common::common::ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("880000..foo"),
        "Downleveled numeric literal property access must use `..` when emitted as an integer.\nOutput:\n{output}"
    );
}

/// Hex literal `0xff` doesn't need `..` because the `0x` prefix disambiguates.
#[test]
fn numeric_literal_property_access_hex() {
    let source = "0xff .foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("0xff..foo"),
        "Hex literal must NOT use double-dot.\nOutput:\n{output}"
    );
}

/// Type assertion wrapping a numeric literal: `(<any>1).foo` → `1..foo`
/// after type erasure removes the assertion and redundant parens.
#[test]
fn numeric_literal_property_access_through_type_assertion() {
    let source = "(<any>1).foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("1..foo"),
        "Type-asserted integer must use `..` after erasure.\nOutput:\n{output}"
    );
}

/// `as` assertion wrapping a numeric literal: `(1 as any).foo` → `1..foo`
#[test]
fn numeric_literal_property_access_through_as_expression() {
    let source = "(1 as any).foo;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("1..foo"),
        "`as` asserted integer must use `..` after erasure.\nOutput:\n{output}"
    );
}

// =====================================================================
// Const enum inlining
// =====================================================================

/// Const enum property access is inlined: `G.A` → `1 /* G.A */`
#[test]
fn const_enum_property_access_inlined() {
    let source = "const enum G { A = 1, B = 2, C = A + B }\nvar a = G.A;\nvar c = G.C;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("1 /* G.A */"),
        "Const enum property access must be inlined with comment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("3 /* G.C */"),
        "Computed const enum member (A+B=3) must be folded.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= G.A"),
        "Original property access must not appear in output.\nOutput:\n{output}"
    );
}

/// Const enum element access is inlined: `G["A"]` → `1 /* G["A"] */`
#[test]
fn const_enum_element_access_inlined() {
    let source = "const enum G { A = 1, B = 2 }\nvar a = G[\"A\"];\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("1 /* G[\"A\"] */"),
        "Const enum element access must be inlined with comment.\nOutput:\n{output}"
    );
}

/// Const enum declaration is erased (not emitted) when `preserve_const_enums` is false.
#[test]
fn const_enum_declaration_erased() {
    let source = "const enum Direction { Up = 1, Down = 2 }\nvar x = Direction.Up;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        !output.contains("Direction)"),
        "Const enum IIFE must not appear in output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("1 /* Direction.Up */"),
        "Const enum usage must be inlined.\nOutput:\n{output}"
    );
}

#[test]
fn const_enum_access_through_namespace_import_alias_inlined() {
    let source = "namespace Outer {\n    export var x = 1;\n}\n\nnamespace Outer {\n    export const enum A { X }\n}\n\nnamespace B {\n    import O = Outer;\n    var x = O.A.X;\n    var y = O.x;\n}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var x = 0 /* O.A.X */;"),
        "Const enum access through namespace import alias must be inlined.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var y = O.x;"),
        "Non-enum namespace member access through the same alias must be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn const_enum_declared_in_namespace_inlines_local_and_qualified_access() {
    let source =
        "namespace N {\n    export const enum E { A }\n    var x = E.A;\n}\nvar y = N.E.A;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var x = 0 /* E.A */;"),
        "Namespace-local const enum access must be inlined by simple name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var y = 0 /* N.E.A */;"),
        "Qualified namespace const enum access must still be inlined outside the namespace.\nOutput:\n{output}"
    );
}

/// String const enum values are inlined with proper quoting.
#[test]
fn const_enum_string_values_inlined() {
    let source = "const enum S { Hello = \"hello\", World = \"world\" }\nvar x = S.Hello;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("\"hello\" /* S.Hello */"),
        "String const enum must be inlined with quoted value.\nOutput:\n{output}"
    );
}
