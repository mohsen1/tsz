//! Integration tests for comment preservation in emitter

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print, parse_and_print, parse_and_print_with_opts};

#[test]
fn test_comment_between_call_arguments() {
    let source = r#"function test() {
    var x = foo(/*comment*/ "arg");
}"#;

    let output = parse_and_print(source);

    // The comment should be preserved before the string literal
    assert!(
        output.contains("/*comment*/"),
        "Comment should be preserved in output: {output}"
    );
}

#[test]
fn empty_call_argument_list_comments_stay_inside_parens() {
    let source = r#"declare var a;
a(/*1*/);
a(
    /*first*/
    // foo
    /*middle*/
    // bar
    /*last*/
);"#;
    let output = parse_and_print(source);

    assert!(
        output.contains("a( /*1*/);"),
        "Inline empty argument-list comment should stay inside call parens.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a(\n/*first*/\n// foo\n/*middle*/\n// bar\n/*last*/\n);"),
        "Multiline empty argument-list comments should stay inside call parens.\nOutput:\n{output}"
    );
}

#[test]
fn multiline_comment_before_first_call_argument_starts_on_next_line() {
    let source = r#"var Person = makeClass(
   /**
     @scope Person
   */
   {
   }
);"#;

    let output = parse_and_print(source);

    assert!(
        output.contains("makeClass(\n/**\n  @scope Person\n*/\n{}"),
        "Multiline comment before first call argument should stay on the line after `(`.\nOutput:\n{output}"
    );
}

#[test]
fn object_literal_accessor_leading_comment_stays_before_accessor() {
    let source = r#"var v = {
 /**
  * @type {number}
  */
 get bar(): number {
  return 12;
 }
}"#;

    let output = parse_and_print(source);

    assert!(
        output.contains("var v = {\n    /**\n     * @type {number}\n     */\n    get bar() {"),
        "Object literal accessor leading comment should stay before the accessor.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("get bar() {\n/**"),
        "Object literal accessor leading comment should not move into the accessor body.\nOutput:\n{output}"
    );
}

#[test]
fn object_literal_property_comments_stay_around_function_value() {
    let source = r#"var Person = makeClass(
   {
       /**
        This is just another way to define a constructor.
        @constructs
        @param {string} name The name of the person.
        */
       initialize: function(name) {
           this.name = name;
       } /* trailing comment 1*/,
   }
);"#;

    let output = parse_and_print(source);

    assert!(
        output.contains("*/\n    initialize: function (name) {"),
        "Leading block comment should end before the property line.\nOutput:\n{output}"
    );
    assert!(
        output.contains("    } /* trailing comment 1*/,"),
        "Trailing block comment before a property comma should stay before the comma.\nOutput:\n{output}"
    );
}

#[test]
fn es5_object_literal_method_comments_stay_on_members() {
    let source = r#"var v = {
 //property
 prop: 1 /* multiple trailing comments */ /*trailing comments*/,
 //property
 func: function () {
 },
 //PropertyName + CallSignature
 func1() { },
 //getter
 get a() {
  return this.prop;
 } /*trailing 1*/,
 //setter
 set a(value) {
  this.prop = value;
 } // trailing 2
};"#;

    let output = parse_and_lower_print(source, PrintOptions::es5());

    assert!(
        output.contains(
            "    //property\n    prop: 1 /* multiple trailing comments */ /*trailing comments*/,"
        ),
        "ES5 object literal property comments should stay on the property.\nOutput:\n{output}"
    );
    assert!(
        output.contains("    //PropertyName + CallSignature\n    func1: function () { },"),
        "ES5 method-lowering should preserve method-leading comments.\nOutput:\n{output}"
    );
    assert!(
        output.contains("    } /*trailing 1*/,\n    //setter\n    set a(value) {"),
        "Accessor trailing and next-member leading comments should not drift into parameters.\nOutput:\n{output}"
    );
}

#[test]
fn element_access_internal_comments_stay_with_brackets() {
    let source = r#"/*0*/ Array /*1*/[ /*2*/ "toString" /*3*/ ] /*4*/;

/*0*/ Array
    // single line
    /*1*/[ /*2*/ "toString"
    // single line
    /*3*/ ] /*4*/"#;

    let output = parse_and_print(source);

    assert!(
        output.contains(r#"/*0*/ Array /*1*/[ /*2*/"toString" /*3*/] /*4*/;"#),
        "Single-line element access comments should stay around brackets.\nOutput:\n{output}"
    );
    assert!(
        output.contains("/*0*/ Array\n// single line\n/*1*/"),
        "Multiline comments between element access base and bracket should stay in place.\nOutput:\n{output}"
    );
    assert!(
        output.contains("/*2*/\"toString\"\n// single line\n/*3*/"),
        "Multiline comments inside element access brackets should stay in place.\nOutput:\n{output}"
    );
}

#[test]
fn element_access_open_bracket_finder_skips_comments_and_strings() {
    // A '[' inside a block comment between the base expression and the
    // argument should not be mistaken for the actual element-access bracket.
    let source = r#"var x = Array /* [ */ ["toString"];
"#;
    let output = parse_and_print(source);
    assert!(
        output.contains(r#"Array /* [ */["toString"]"#),
        "Block comment containing '[' before element-access bracket should be preserved without mis-positioning.\nOutput:\n{output}"
    );

    // A '[' inside a string literal in a preceding sibling context should
    // also not perturb the open-bracket finder.
    let source2 = r#"var y = obj["[hello"][0];
"#;
    let output2 = parse_and_print(source2);
    assert!(
        output2.contains(r#"obj["[hello"][0]"#),
        "String literal containing '[' should not confuse element-access open-bracket finder.\nOutput:\n{output2}"
    );
}

#[test]
fn object_literal_comments_stay_after_source_commas() {
    let source = r#"const a = {
    p0: 0, // Comment 0
    p1: 0, /* Comment 1
    A multiline comment. */
    p2: 0, // Comment 2
};"#;

    let output = parse_and_lower_print(source, PrintOptions::es5());

    assert!(
        output.contains("    p0: 0, // Comment 0\n"),
        "Line comment after a member comma should stay after the comma.\nOutput:\n{output}"
    );
    assert!(
        output.contains("    p1: 0, /* Comment 1\n    A multiline comment. */\n    p2: 0,"),
        "Multiline comment after a member comma should not force a later synthetic comma.\nOutput:\n{output}"
    );
}

#[test]
fn comments_after_spread_tokens_stay_after_ellipsis() {
    let source = r#"const a = { .../*#__PURE__*/identity({ b: 1 }) };
const b = {
  ...
  /*#__PURE__*/identity({ b: 1 })
};
const c = {
  ...
  /*#__PURE__*/
  identity({ b: 1 })
};
function f(first, .../* comment f */rest) {
  const [fa, fb] = [.../* comment fab */
    rest
  ];
  const { x, .../* comment rest */remaining } = rest;
  return { .../* comment return */ remaining };
}"#;

    let output = parse_and_print(source);

    assert!(
        output.contains("... /*#__PURE__*/identity({"),
        "Object spread block comment should stay after `...` without a post-comment gap.\nOutput:\n{output}"
    );
    assert!(
        output.contains("...\n    /*#__PURE__*/ identity({"),
        "Object spread comment after a source newline should stay on the following line.\nOutput:\n{output}"
    );
    assert!(
        output.contains("...\n    /*#__PURE__*/\n    identity({"),
        "Object spread comment followed by a source newline should keep the operand on the next line.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function f(first, ... /* comment f */rest)"),
        "Rest parameter comments should stay between `...` and the name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[... /* comment fab */rest\n    ]"),
        "Array spread comments should stay between `...` and the operand.\nOutput:\n{output}"
    );
    assert!(
        output.contains("{ x, ... /* comment rest */remaining }"),
        "Object binding rest comments should stay between `...` and the name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return { ... /* comment return */remaining };"),
        "Return object spread comments should stay after `...`.\nOutput:\n{output}"
    );
}

#[test]
fn template_substitution_comment_with_dollar_brace_is_preserved() {
    let source = "var x = `${/* ${ */ value}`;\n";

    let output = parse_and_print(source);

    assert!(
        output.contains("`${/* ${ */ value}`"),
        "Template substitution comment containing `${{` should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn parenthesized_expression_open_paren_comment_has_no_extra_space_after_block() {
    let source = "var j;\nvar f: () => any;\n<any>( /* Preserve */ j = f());\n";
    let output = parse_and_print_with_opts(source, PrintOptions::es6());

    assert!(
        output.contains("( /* Preserve */j = f());"),
        "block comment after open paren should not force an extra post-comment space; output:\n{output}"
    );
}

#[test]
fn test_skip_whitespace_forward_only_skips_whitespace() {
    use tsz_emitter::emitter::Printer;
    use tsz_parser::parser::node::NodeArena;

    let arena = NodeArena::new();
    let mut printer = Printer::new(&arena);
    printer.set_source_text("  /*comment*/ text");

    // Should skip whitespace but not comments
    let result = printer.skip_whitespace_forward(0, 20);
    assert_eq!(result, 2); // Only skips the two spaces, stops at '/*'
}

#[test]
fn test_skip_whitespace_forward_no_whitespace() {
    use tsz_emitter::emitter::Printer;
    use tsz_parser::parser::node::NodeArena;

    let arena = NodeArena::new();
    let mut printer = Printer::new(&arena);
    printer.set_source_text("abc");

    // Should return start position when no whitespace
    let result = printer.skip_whitespace_forward(0, 3);
    assert_eq!(result, 0);
}

#[test]
fn test_block_comment_after_comma_in_multiline_object() {
    // Block comments after commas in multi-line object literals need a space before them
    let source = r#"var x = {
    a: 1, /* comment */
    b: 2
};"#;

    let output = parse_and_print(source);

    // The block comment should have a space before it (not "1,/* comment */")
    assert!(
        output.contains(", /* comment */") || output.contains(",  /* comment */"),
        "Should have space before block comment after comma. Got:\n{output}"
    );
}

/// A comment after a single-line source catch block belongs after the closing
/// brace once the block is expanded, not on the emitted opening brace line.
#[test]
fn trailing_comment_after_single_line_catch_block_stays_after_block() {
    let source = "try { } catch (x: unknown) { x.foo; } // error in the body\n";

    let output = parse_and_print(source);

    assert!(
        output.contains("catch (x) {\n    x.foo;\n} // error in the body"),
        "Trailing comment after catch block should stay after the closing brace.\nOutput:\n{output}"
    );
}

#[test]
fn leading_comment_before_catch_stays_before_catch_keyword() {
    let source = "try { console.log(); }\n// @ts-ignore\ncatch (e: number) { console.log(e); }\n";

    let output = parse_and_print(source);

    assert!(
        output.contains("}\n// @ts-ignore\ncatch (e) {"),
        "Leading comment before catch should not move inside catch parens.\nOutput:\n{output}"
    );
}

/// When an erased type-only declaration (interface) is followed by a non-erased
/// statement (`;`) on the same line, trailing comments on the non-erased
/// statement must be preserved. Regression test for the initialization filter
/// that was over-consuming comments belonging to non-erased siblings.
///
/// Reproduces the pattern from `circularBaseTypes`:
///   `interface Foo {};  // Error`
/// tsc output: `; // Error`
/// Previous bug: `// Error` was stripped because the erased range for the
/// *next* erased statement captured it.
/// Previously broken since commit 118ebd752 — fixed by capping erased statement
/// comment consumption at non-erased sibling boundaries.
#[test]
fn trailing_comment_after_erased_interface_sibling_preserved() {
    // Simplified version of circularBaseTypes
    let source = "interface Foo {}; // keep this\nvar x = 1;\n";

    let output = parse_and_print(source);

    // The `; // keep this` comment must be preserved in output
    assert!(
        output.contains("// keep this"),
        "Trailing comment on non-erased sibling after erased interface should be preserved.\nOutput:\n{output}"
    );
}

/// When an erased declaration is followed by another erased declaration,
/// comments between them (leading trivia of the second erased decl) should
/// still be erased. This ensures the fix for preserving non-erased sibling
/// comments doesn't break erasure of inter-erased comments.
#[test]
fn comments_between_consecutive_erased_declarations_are_erased() {
    let source = "interface Foo {}\n// belongs to type Bar\ntype Bar = string;\nvar x = 1;\n";

    let output = parse_and_print(source);

    // The comment between two erased declarations should be erased
    assert!(
        !output.contains("belongs to type Bar"),
        "Comment between consecutive erased declarations should be erased.\nOutput:\n{output}"
    );
    // The runtime statement should still be present
    assert!(
        output.contains("var x = 1"),
        "Runtime statement should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn block_comment_before_semicolon_preserves_space() {
    // Source has a block comment followed by an empty statement (;)
    let source = "/*existing trivia*/ ;\n";

    let output = parse_and_print(source);

    assert!(
        output.contains("/*existing trivia*/ ;"),
        "Block comment before semicolon should have space between comment and semicolon.\nOutput:\n{output}"
    );
}

#[test]
fn pinned_comments_preserved_when_remove_comments_true() {
    // /*! ... */ comments should be preserved even with removeComments: true,
    // but only when detached (separated by a blank line from the next content).
    let source = "/*! Copyright 2024 */\n\nclass C {}\n";

    let opts = PrintOptions {
        remove_comments: true,
        ..Default::default()
    };
    let output = parse_and_print_with_opts(source, opts);

    assert!(
        output.contains("/*! Copyright 2024 */"),
        "Pinned /*! ... */ comments should be preserved even with removeComments.\nOutput:\n{output}"
    );
}

#[test]
fn attached_pinned_comments_stripped_when_remove_comments_true() {
    // Attached /*! ... */ comments (no blank line before code) should be stripped
    // when removeComments: true, matching tsc behavior.
    let source = "/*! attached comment */\nclass C {}\n";

    let opts = PrintOptions {
        remove_comments: true,
        ..Default::default()
    };
    let output = parse_and_print_with_opts(source, opts);

    assert!(
        !output.contains("/*! attached comment */"),
        "Attached pinned comments should be stripped with removeComments.\nOutput:\n{output}"
    );
}

/// Comments inside erased type arguments of heritage clauses (extends)
/// must not leak into the JS output. tsc strips `<T>` in `extends Base<T>`
/// along with any comments inside.
#[test]
fn test_heritage_type_arg_comments_do_not_leak() {
    let source = "class Foo extends Bar</* type comment */ string> { }";

    let output = parse_and_print(source);

    assert!(
        !output.contains("type comment"),
        "Comments inside erased heritage type arguments should not appear in JS output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("extends Bar"),
        "The extends clause should still be present.\nOutput:\n{output}"
    );
}

/// Multiple type arguments with comments in heritage clauses should all be stripped.
#[test]
fn test_heritage_multiple_type_arg_comments_do_not_leak() {
    let source = "class Foo extends Map</* key */ string, /* value */ number> { }";

    let output = parse_and_print(source);

    assert!(
        !output.contains("key"),
        "Comments inside erased heritage type arguments should not appear in JS output.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("value"),
        "Comments inside erased heritage type arguments should not appear in JS output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("extends Map"),
        "The extends clause should still be present.\nOutput:\n{output}"
    );
}

/// Regression test: `skip_block_opening_line_comments` must search
/// FORWARD from `block_node.pos` for `{`, not backward. In the
/// TypeScript AST, `node.pos` includes leading trivia so `{` is at or
/// after `node.pos`, never before it. With the previous backward
/// search the helper could find a `{` from a much earlier construct
/// (e.g. an outer block's brace) and then advance `comment_emit_idx`
/// past comments on that earlier line, losing them from the output.
/// Devin review: <https://github.com/mohsen1/tsz/pull/2248#discussion_r3176256604>
#[test]
fn test_skip_block_opening_line_comments_uses_forward_search_for_param_lowered_block() {
    use tsz_common::ScriptTarget;

    // Verify the helper does NOT scan backward into earlier source by
    // emitting an inner function whose body block goes through the
    // `emit_block_with_param_prologue` path (default-valued parameter).
    // The output should compile and contain the body. A backward search
    // bug typically manifests as panic, mis-indented body, or a lost
    // body statement; we keep the assertion narrow on the body presence.
    let source = "var s = '{';\nfunction outer() {\n    function inner(x = 1) {\n        return x + 1;\n    }\n    return inner();\n}\n";

    let output = parse_and_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    // With the forward-search fix the inner body still emits cleanly.
    // The default-parameter prologue must be present.
    assert!(
        output.contains("if (x === void 0)"),
        "ES5 default-parameter prologue must be emitted for inner function.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return x + 1;"),
        "Inner function body must be present in output.\nOutput:\n{output}"
    );
}
