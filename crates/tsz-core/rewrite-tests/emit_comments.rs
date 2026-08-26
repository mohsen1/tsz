use std::fmt::Write as _;
use std::{collections::BTreeMap, sync::Arc};

use tsz::source::{FileId, SourceText};
use tsz::syntax::{ExpressionKind, StatementKind, parse_source};
use tsz::{Compiler, CompilerOptions, SourceInput};

fn javascript(source: &str, module: &str, remove_comments: bool) -> String {
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            module: module.to_string(),
            no_check: true,
            remove_comments,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    output
        .emitted_files
        .into_iter()
        .find(|file| !file.declaration)
        .expect("JavaScript output")
        .text
}

fn javascript_files(sources: &[(&str, &str)]) -> BTreeMap<String, String> {
    let output = Compiler::new().compile(
        sources
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &CompilerOptions {
            target: "es2015".to_string(),
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    output
        .emitted_files
        .into_iter()
        .filter(|file| !file.declaration)
        .map(|file| (file.path.to_string_lossy().into_owned(), file.text))
        .collect()
}

#[test]
fn emitted_nodes_own_leading_trailing_inline_detached_and_eof_comments() {
    let source = concat!(
        "/*! source pinned */\n",
        "const first = 1; // same-line\n",
        "/* block leading */ const second = 2;\n",
        "// line leading\n",
        "const third = /* inline block */ 3;\n\n",
        "// detached line\n\n",
        "const fourth = 4;\n",
        "// eof detached\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "/*! source pinned */\n",
            "const first = 1; // same-line\n",
            "/* block leading */ const second = 2;\n",
            "// line leading\n",
            "const third = /* inline block */ 3;\n",
            "// detached line\n",
            "const fourth = 4;\n",
            "// eof detached\n",
        )
    );
}

#[test]
fn erased_statements_and_overloads_consume_their_comment_ranges_once() {
    let source = concat!(
        "// erased type lead\n",
        "type Hidden = string; // erased type tail\n",
        "// erased ambient lead\n",
        "declare let ambient: {}; // erased ambient tail\n",
        "class Renamed {\n",
        "    // erased overload lead\n",
        "    method(value: string); // erased overload tail\n",
        "    // kept implementation lead\n",
        "    method(value: string) { } // kept implementation tail\n",
        "}\n",
        "// erased export lead\n",
        "export type { Hidden }; // erased export tail\n",
        "// kept export lead\n",
        "export const kept = 1; // kept export tail\n",
    );
    assert_eq!(
        javascript(source, "esnext", false),
        concat!(
            "class Renamed {\n",
            "    // kept implementation lead\n",
            "    method(value) { } // kept implementation tail\n",
            "}\n",
            "// kept export lead\n",
            "export const kept = 1; // kept export tail\n",
        )
    );
}

#[test]
fn nested_multiline_object_comments_use_authored_layout_facts() {
    let source = concat!(
        "const renamed = {\n",
        "    outer: /*! outer */ {\n",
        "        line:\n",
        "        // before value\n",
        "        1,\n",
        "        block:\n",
        "        /* before block value */\n",
        "        2,\n",
        "    },\n",
        "};\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "const renamed = {\n",
            "    outer: /*! outer */ {\n",
            "        line: \n",
            "        // before value\n",
            "        1,\n",
            "        block: \n",
            "        /* before block value */\n",
            "        2,\n",
            "    },\n",
            "};\n",
        )
    );
}

#[test]
fn erased_attached_comments_drop_but_recognized_source_reference_transfers() {
    let source = concat!(
        "/// <reference path=\"./a.d.ts\" />\n",
        "/// ordinary triple slash\n",
        "/*! erased pinned */\n",
        "/** @license erased */\n",
        "type Hidden = string;\n",
        "/*! kept pinned */\n",
        "/** @license kept */\n",
        "const kept = 1;\n",
        "/// <reference path=\"./later.d.ts\" />\n",
        "type Later = number;\n",
        "const finalValue = 2;\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "/// <reference path=\"./a.d.ts\" />\n",
            "/*! kept pinned */\n",
            "/** @license kept */\n",
            "const kept = 1;\n",
            "const finalValue = 2;\n",
        )
    );
}

#[test]
fn source_detached_pinned_policy_matches_remove_comments_and_erased_nodes() {
    let ambient = concat!(
        "/*! detached pinned */\r\n\r\n",
        "/*! attached pinned */\r\n",
        "/** @license attached */\r\n",
        "declare var erased: number;\r\n",
    );
    let expected = concat!("\"use strict\";\n", "/*! detached pinned */\n");
    assert_eq!(javascript(ambient, "", false), expected);
    assert_eq!(javascript(ambient, "", true), expected);

    let emitted = concat!(
        "/*! attached emitted pinned */\n",
        "/** @license emitted */\n",
        "const kept = 1;\n",
    );
    assert_eq!(
        javascript(emitted, "", false),
        concat!(
            "\"use strict\";\n",
            "/*! attached emitted pinned */\n",
            "/** @license emitted */\n",
            "const kept = 1;\n",
        )
    );
    assert_eq!(
        javascript(emitted, "", true),
        "\"use strict\";\nconst kept = 1;\n"
    );

    let non_top = "const first = 1;\n/*! non-top pinned */\n\ndeclare var erased: number;\n";
    assert_eq!(
        javascript(non_top, "", false),
        "\"use strict\";\nconst first = 1;\n"
    );
}

#[test]
fn parameter_comment_slots_consume_first_middle_final_this_default_and_trailing_comma() {
    let source = concat!(
        "function slots(\n",
        " /* first before */ first /* first after */,\n",
        " /* middle before */ middle: number /* after type */ = /* after equals */ 1 /* after default */,\n",
        " /* final before */ ... /* after rest */ final: number[] /* final after */\n",
        ") {}\n",
        "function withThis(\n",
        " /* this before */ this: void /* this after */,\n",
        " /* kept before */ kept: number /* kept after */\n",
        ") {}\n",
        "function trailing(first /* first tail */, /* dropped trailing */) {}\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "function slots(\n",
            "/* first before */ first /* first after */, \n",
            "/* middle before */ middle = 1 /* after default */, \n",
            "/* final before */ ... /* after rest */final /* final after */) { }\n",
            "function withThis(\n",
            "/* kept before */ kept /* kept after */) { }\n",
            "function trailing(first /* first tail */) { }\n",
        )
    );
}

#[test]
fn element_access_slots_follow_token_adjacency_not_comment_placement() {
    let source = concat!(
        "/*0*/ Array /*1*/[ /*2*/ \"toString\" /*3*/ ] /*4*/; /*5*/\n\n",
        "/*0*/ Array \n",
        "    // single line\n",
        "    /*1*/[ /*2*/ \"toString\"\n",
        "    // single line\n",
        "    /*3*/ ] /*4*/\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "/*0*/ Array /*1*/[ /*2*/\"toString\" /*3*/] /*4*/; /*5*/\n",
            "/*0*/ Array\n",
            "// single line\n",
            "/*1*/ [ /*2*/\"toString\"\n",
            "// single line\n",
            "/*3*/ ]; /*4*/\n",
        )
    );
}

#[test]
fn variable_declaration_token_gaps_match_ts7() {
    let source = concat!(
        "var a = /*some comment*/ null;\n",
        "var b /*some comment*/ = null;\n",
        "var /*some comment*/ c = null;\n",
        "\n// no space\n",
        "var a=/*some comment*/null;\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "var a = /*some comment*/ null;\n",
            "var b /*some comment*/ = null;\n",
            "var /*some comment*/ c = null;\n",
            "// no space\n",
            "var a = /*some comment*/ null;\n",
        )
    );
}

#[test]
fn binary_operator_token_gaps_match_ts7_continuation_layout() {
    let source = concat!(
        "var alpha = 'some'\n    // comment\n    + 'text';\n\n",
        "var beta = 'some'\n    /* comment */\n    + 'text';\n\n",
        "var gamma = 'some'\n    /* comment */\n    + /*comment1*/\n    'text';\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "var alpha = 'some'\n    // comment\n    + 'text';\n",
            "var beta = 'some'\n    /* comment */\n    + 'text';\n",
            "var gamma = 'some'\n    /* comment */\n    + /*comment1*/\n        'text';\n",
        )
    );
}

#[test]
fn property_and_call_token_gaps_match_ts7_once_in_source_order() {
    let source = concat!(
        "fn /*call pre-paren*/ ();\n",
        "renamed /*member pre-dot*/ . /*member after-dot*/ value;\n",
        "renamed.outer /*nested pre-dot*/ . /*nested after-dot*/ leaf;\n",
        "chain.first. /*later dot only*/second;\n",
        "/*1*/Array/*2*/./*3*/toString/*4*/\n",
        "/*1*/Array\n/*2*/./*3*/\n    // Single-line comment\n    toString/*4*/\n",
        "/*1*/Array/*2*/./*3*/\n    // Single-line comment\n    toString/*4*/\n",
        "/*1*/Array\n    // Single-line comment\n    /*2*/./*3*/toString/*4*/\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "fn /*call pre-paren*/();\n",
            "renamed /*member pre-dot*/. /*member after-dot*/value;\n",
            "renamed.outer /*nested pre-dot*/. /*nested after-dot*/leaf;\n",
            "chain.first. /*later dot only*/second;\n",
            "/*1*/ Array /*2*/. /*3*/toString; /*4*/\n",
            "/*1*/ Array\n    /*2*/ . /*3*/\n        // Single-line comment\n        toString; /*4*/\n",
            "/*1*/ Array /*2*/. /*3*/\n    // Single-line comment\n    toString; /*4*/\n",
            "/*1*/ Array\n    // Single-line comment\n    /*2*/ . /*3*/toString; /*4*/\n",
        )
    );
}

#[test]
fn if_statement_token_gaps_match_ts7_keyword_and_delimiter_owners() {
    let source = concat!(
        "/*1*/ if /*2*/ ( /*3*/ true /*4*/ ) /*5*/ {}\n\n",
        "/*1*/ if /*2*/ ( /*3*/ true /*4*/ ) /*5*/ {} /*6*/ else /*7*/  {}\n",
        "if (renamed) { work(); } else { fallback(); }\n",
        "if (renamed) work(); else fallback();\n",
        "if (renamed) work(); /* block boundary */ else fallback();\n",
        "if (renamed) work(); // line boundary\nelse fallback();\n",
        "function wrapper() { if (renamed) work(); else fallback(); }\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "/*1*/ if /*2*/ ( /*3*/true /*4*/) /*5*/ { }\n",
            "/*1*/ if /*2*/ ( /*3*/true /*4*/) /*5*/ { } /*6*/\n",
            "else /*7*/ { }\n",
            "if (renamed) {\n",
            "    work();\n",
            "}\n",
            "else {\n",
            "    fallback();\n",
            "}\n",
            "if (renamed)\n",
            "    work();\n",
            "else\n",
            "    fallback();\n",
            "if (renamed)\n",
            "    work(); /* block boundary */\n",
            "else\n",
            "    fallback();\n",
            "if (renamed)\n",
            "    work(); // line boundary\n",
            "else\n",
            "    fallback();\n",
            "function wrapper() { if (renamed)\n",
            "    work();\n",
            "else\n",
            "    fallback(); }\n",
        )
    );
}

#[test]
fn no_initializer_variable_line_comment_gap_matches_ts7_nested_and_removed() {
    let source = concat!(
        "let renamed // top line\n",
        ";\n",
        "function wrapper() {\n",
        "    var nested // nested line\n",
        "    ;\n",
        "}\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "let renamed // top line\n",
            ";\n",
            "function wrapper() {\n",
            "    var nested // nested line\n",
            "    ;\n",
            "}\n",
        )
    );
    assert_eq!(
        javascript(source, "", true),
        concat!(
            "\"use strict\";\n",
            "let renamed;\n",
            "function wrapper() {\n",
            "    var nested;\n",
            "}\n",
        )
    );
}

#[test]
fn token_gap_comments_remove_cleanly_without_changing_nested_runtime_shape() {
    let source = concat!(
        "fn /*call*/ ();\n",
        "renamed /*pre-dot*/ . /*after-dot*/ value;\n",
        "const alpha /*after-name*/ = /*after-equals*/ 1;\n",
        "const /*before-name*/ beta = 2;\n",
        "const sum = alpha\n// before-op\n+ /*after-op*/ beta;\n",
        "if /*after-if*/ (/*after-open*/ true /*before-close*/) /*after-close*/ {\n",
        "    fn /*nested-call*/ ();\n",
        "} /*before-else*/ else /*after-else*/ { renamed /*pre-dot*/ . /*after-dot*/ value; }\n",
        "function wrapper() {\n",
        "    let nested /*removed block*/ // removed line\n",
        "    ;\n",
        "    if /*nested-if*/ (renamed) work /*then-call*/ (); /*between*/ else fallback /*else-call*/ ();\n",
        "}\n",
    );
    assert_eq!(
        javascript(source, "", true),
        concat!(
            "\"use strict\";\n",
            "fn();\n",
            "renamed.value;\n",
            "const alpha = 1;\n",
            "const beta = 2;\n",
            "const sum = alpha + beta;\n",
            "if (true) {\n",
            "    fn();\n",
            "}\n",
            "else {\n",
            "    renamed.value;\n",
            "}\n",
            "function wrapper() {\n",
            "    let nested;\n",
            "    if (renamed)\n",
            "        work();\n",
            "    else\n",
            "        fallback();\n",
            "}\n",
        )
    );
}

#[test]
fn unsupported_module_comment_slot_is_nonclaimed_before_emit_and_other_files_survive() {
    let sources = [
        (
            "blocked.ts",
            "const value = 1;\nexport { /* module slot */ value };\n",
        ),
        ("unrelated.ts", "const unrelated = 2;\n"),
    ];
    let compile = |sources: &[(&str, &str)]| {
        Compiler::new().compile(
            sources
                .iter()
                .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
                .collect(),
            &CompilerOptions {
                target: "es2015".to_string(),
                module: "esnext".to_string(),
                no_check: true,
                ..CompilerOptions::default()
            },
        )
    };
    for roots in [&sources[..], &[sources[1], sources[0]][..]] {
        let output = compile(roots);
        assert_eq!(output.diagnostics, []);
        assert_eq!(
            output.semantic_completion,
            tsz::SemanticCompletion::Deferred
        );
        assert_eq!(
            output
                .emitted_files
                .iter()
                .filter(|file| !file.declaration)
                .map(|file| (file.path.to_string_lossy().into_owned(), file.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(
                "unrelated.js".to_string(),
                "\"use strict\";\nconst unrelated = 2;\n"
            )]
        );
    }
}

#[test]
fn remove_comments_and_repeated_root_order_do_not_reuse_comment_identity() {
    let commented = "/*! lead */ const first = 1; // tail\n// eof\n";
    assert_eq!(
        javascript(commented, "", true),
        "\"use strict\";\nconst first = 1;\n"
    );

    let sources = [
        ("alpha.ts", "// alpha\nconst alpha = 1; // alpha tail\n"),
        ("beta.ts", "/* beta */ const beta = 2; // beta tail\n"),
    ];
    let forward = javascript_files(&sources);
    let reverse = javascript_files(&[sources[1], sources[0]]);
    assert_eq!(forward, reverse);
    assert_eq!(
        forward.get("alpha.js").map(String::as_str),
        Some("\"use strict\";\n// alpha\nconst alpha = 1; // alpha tail\n")
    );
    assert_eq!(
        forward.get("beta.js").map(String::as_str),
        Some("\"use strict\";\n/* beta */ const beta = 2; // beta tail\n")
    );
}

#[test]
fn indentation_change_keeps_representative_comment_free_emit_exact() {
    let source = concat!(
        "function kept(value: number) {\n",
        "    if (value) {\n",
        "        return value;\n",
        "    }\n",
        "    else {\n",
        "        return 0;\n",
        "    }\n",
        "}\n",
        "class Box {\n",
        "    read() { }\n",
        "}\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "function kept(value) {\n",
            "    if (value) {\n",
            "        return value;\n",
            "    }\n",
            "    else {\n",
            "        return 0;\n",
            "    }\n",
            "}\n",
            "class Box {\n",
            "    read() { }\n",
            "}\n",
        )
    );
}

#[test]
fn erased_export_declare_keeps_the_external_module_marker() {
    assert_eq!(
        javascript("export declare const renamed: number;\n", "esnext", false),
        "export {};\n"
    );
}

#[test]
fn jsdoc_attachment_is_node_scoped_and_captured_before_async() {
    let source = SourceText::new(
        FileId(0),
        "authored.js".into(),
        Arc::<str>::from(concat!(
            "/** attached declaration */ async function renamed() {}\n",
            "/** detached declaration */\n\nasync function detached() {}\n",
            "const arrow = /** attached arrow */ async () => {};\n",
            "const detachedArrow = /** detached arrow */\n\nasync () => {};\n",
            "const ordinary = /* ordinary block */ async () => {};\n",
        )),
    );
    let parsed = parse_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let declaration_jsdoc = |index: usize| match &parsed.unit.statements[index].kind {
        StatementKind::Function(declaration) => declaration.has_leading_jsdoc,
        _ => panic!("expected function declaration at {index}"),
    };
    let expression_jsdoc = |index: usize| match &parsed.unit.statements[index].kind {
        StatementKind::Variable(declaration) => {
            match declaration.declarators[0].initializer.as_ref() {
                Some(expression) => match &expression.kind {
                    ExpressionKind::FunctionLike(function) => function.has_leading_jsdoc,
                    _ => panic!("expected function-like initializer at {index}"),
                },
                None => panic!("expected initializer at {index}"),
            }
        }
        _ => panic!("expected variable declaration at {index}"),
    };

    assert!(declaration_jsdoc(0));
    assert!(!declaration_jsdoc(1));
    assert!(expression_jsdoc(2));
    assert!(!expression_jsdoc(3));
    assert!(!expression_jsdoc(4));
}

#[test]
fn one_line_object_comments_and_trailing_comma_keep_authored_slots() {
    let source = concat!(
        "const renamed = { /* before */ alpha: 1, /* between */ beta: 2, };\n",
        "const closing = { value: 1, /* before close */ };\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "const renamed = { /* before */ alpha: 1, /* between */ beta: 2, };\n",
            "const closing = { value: 1, /* before close */ };\n",
        )
    );
}

#[test]
fn closing_delimiter_comments_dedent_with_the_delimiter_owner() {
    let source = concat!(
        "const nested = {\n",
        "    alpha: 1,\n",
        "    // before object close\n",
        "};\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "const nested = {\n",
            "    alpha: 1,\n",
            "    // before object close\n",
            "};\n",
        )
    );
}

#[test]
fn authored_body_spans_own_empty_and_nested_closing_comments() {
    let source = concat!(
        "function renamed() {\n",
        "    // function close\n",
        "}\n",
        "function sameLine() { /* same-line body comment */ }\n",
        "class Container {\n",
        "    constructor() {\n",
        "        // constructor close\n",
        "    }\n",
        "    method() {\n",
        "        // method close\n",
        "    }\n",
        "    // class close\n",
        "}\n",
        "const arrow = () => {\n",
        "    // arrow close\n",
        "};\n",
        "{\n",
        "    // block close\n",
        "}\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "function renamed() {\n",
            "    // function close\n",
            "}\n",
            "function sameLine() { /* same-line body comment */ }\n",
            "class Container {\n",
            "    constructor() {\n",
            "        // constructor close\n",
            "    }\n",
            "    method() {\n",
            "        // method close\n",
            "    }\n",
            "    // class close\n",
            "}\n",
            "const arrow = () => {\n",
            "    // arrow close\n",
            "};\n",
            "{\n",
            "    // block close\n",
            "}\n",
        )
    );
}

#[test]
fn authored_single_line_function_bodies_share_one_layout_owner() {
    let source = concat!(
        "function renamed<Value>(value: Value): Value { const kept = value; return kept; }\n",
        "class Container {\n",
        "    method(value: number): number { return value; }\n",
        "    get \"quoted\"(): number { return 1; }\n",
        "    controlled(value: boolean) { if (value) return; }\n",
        "    constructor(value: boolean) { if (value) return; }\n",
        "}\n",
        "const callback = () => { renamed(1); };\n",
        "function commented() { 0; /* keep occupied trailing */ }\n",
        "function controlled(value: boolean) { if (value) return; }\n",
        "function erasedLeading() { interface Gone {} return 1; }\n",
        "function erasedMiddle() { 0; interface Gone {} return 1; }\n",
        "function erasedTrailing() { return 1; interface Gone {} }\n",
        "function erasedOnly() { interface Gone {} }\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "function renamed(value) { const kept = value; return kept; }\n",
            "class Container {\n",
            "    method(value) { return value; }\n",
            "    get \"quoted\"() { return 1; }\n",
            "    controlled(value) { if (value)\n",
            "        return; }\n",
            "    constructor(value) { if (value)\n",
            "        return; }\n",
            "}\n",
            "const callback = () => { renamed(1); };\n",
            "function commented() { 0; /* keep occupied trailing */ }\n",
            "function controlled(value) { if (value)\n",
            "    return; }\n",
            "function erasedLeading() {  return 1; }\n",
            "function erasedMiddle() { 0;  return 1; }\n",
            "function erasedTrailing() { return 1;  }\n",
            "function erasedOnly() {  }\n",
        )
    );
}

#[test]
fn function_body_layout_preserves_every_multiline_fallback() {
    let source = concat!(
        "function multiline(value: number) {\n",
        "    return value;\n",
        "}\n",
        "function directive() { \"use strict\"; return 1; }\n",
        "function extended() { \"not\\u{20}strict\"; return 2; }\n",
        "function empty() { }\n",
        "class Container {\n",
        "    constructor() { consume(); }\n",
        "}\n",
        "class Property { constructor(public value: number) { consume(); } }\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "function multiline(value) {\n",
            "    return value;\n",
            "}\n",
            "function directive() {\n",
            "    \"use strict\";\n",
            "    return 1;\n",
            "}\n",
            "function extended() {\n",
            "    \"not\\u{20}strict\";\n",
            "    return 2;\n",
            "}\n",
            "function empty() { }\n",
            "class Container {\n",
            "    constructor() { consume(); }\n",
            "}\n",
            "class Property {\n",
            "    constructor(value) {\n",
            "        this.value = value;\n",
            "        consume();\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn represented_empty_delimiters_consume_their_interior_comments_once() {
    let source = concat!(
        "declare function call(): void;\n",
        "declare class Renamed { }\n",
        "const object = { /* object */ };\n",
        "const array = [/* array */];\n",
        "call(/* call */);\n",
        "new Renamed(/* new */);\n",
        "const grouped = (/* grouped */ object /* close */);\n",
        "function parameters(/** nothing */) { }\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "const object = { /* object */ };\n",
            "const array = [ /* array */];\n",
            "call( /* call */);\n",
            "new Renamed( /* new */);\n",
            "const grouped = ( /* grouped */object /* close */);\n",
            "function parameters( /** nothing */) { }\n",
        )
    );
}

#[test]
fn nested_delimiter_owners_keep_inline_and_multiline_closing_trivia() {
    let source = concat!(
        "declare function target(value?: number): void;\n",
        "declare class Constructed { }\n",
        "function nested() {\n",
        "    const inlineArray = [1 /* array inline */];\n",
        "    target(1 /* call inline */);\n",
        "    new Constructed(1 /* new inline */);\n",
        "    const inlineGroup = (/* group open */ 1 /* group inline */);\n",
        "    const emptyArray = [\n",
        "        // empty array close\n",
        "    ];\n",
        "    target(\n",
        "        // empty call close\n",
        "    );\n",
        "    new Constructed(\n",
        "        // empty new close\n",
        "    );\n",
        "    const multilineArray = [1\n",
        "        // array close\n",
        "    ];\n",
        "    target(1\n",
        "        // call close\n",
        "    );\n",
        "    new Constructed(1\n",
        "        // new close\n",
        "    );\n",
        "    const multilineGroup = (\n",
        "        // group open\n",
        "        1\n",
        "        // group close\n",
        "    );\n",
        "}\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "function nested() {\n",
            "    const inlineArray = [1 /* array inline */];\n",
            "    target(1 /* call inline */);\n",
            "    new Constructed(1 /* new inline */);\n",
            "    const inlineGroup = ( /* group open */1 /* group inline */);\n",
            "    const emptyArray = [\n",
            "    // empty array close\n",
            "    ];\n",
            "    target(\n",
            "    // empty call close\n",
            "    );\n",
            "    new Constructed(\n",
            "    // empty new close\n",
            "    );\n",
            "    const multilineArray = [1\n",
            "    // array close\n",
            "    ];\n",
            "    target(1\n",
            "    // call close\n",
            "    );\n",
            "    new Constructed(1\n",
            "    // new close\n",
            "    );\n",
            "    const multilineGroup = (\n",
            "    // group open\n",
            "    1\n",
            "    // group close\n",
            "    );\n",
            "}\n",
        )
    );
    assert_eq!(
        javascript(source, "", true),
        concat!(
            "\"use strict\";\n",
            "function nested() {\n",
            "    const inlineArray = [1];\n",
            "    target(1);\n",
            "    new Constructed(1);\n",
            "    const inlineGroup = (1);\n",
            "    const emptyArray = [];\n",
            "    target();\n",
            "    new Constructed();\n",
            "    const multilineArray = [1];\n",
            "    target(1);\n",
            "    new Constructed(1);\n",
            "    const multilineGroup = (1);\n",
            "}\n",
        )
    );
}

#[test]
fn leading_block_comments_are_separated_from_closing_delimiters() {
    let source = concat!(
        "const parenthesized = (\n",
        "    value\n",
        "    // line\n",
        "    /* leading */);\n",
        "const emptyArray = [\n",
        "    /* leading */];\n",
        "call(\n",
        "    /* leading */);\n",
        "new Constructed(\n",
        "    /* leading */);\n",
        "const trailing = (value /* trailing */);\n",
        "const ownLine = (\n",
        "    value\n",
        "    /* own line */\n",
        ");\n",
        "/* eof */\n",
    );
    assert_eq!(
        javascript(source, "", false),
        concat!(
            "\"use strict\";\n",
            "const parenthesized = (value\n",
            "// line\n",
            "/* leading */ );\n",
            "const emptyArray = [\n",
            "/* leading */ ];\n",
            "call(\n",
            "/* leading */ );\n",
            "new Constructed(\n",
            "/* leading */ );\n",
            "const trailing = (value /* trailing */);\n",
            "const ownLine = (value\n",
            "/* own line */\n",
            ");\n",
            "/* eof */\n",
        )
    );
}

#[test]
fn comment_index_scales_by_consumed_comment_count_and_is_repeatable() {
    const COMMENT_COUNT: usize = 1_024;
    let mut source = String::new();
    for index in 0..COMMENT_COUNT {
        writeln!(source, "// owned {index}\nconst value{index} = {index};").unwrap();
    }
    let first = javascript(&source, "", false);
    let second = javascript(&source, "", false);
    assert_eq!(first, second);
    assert_eq!(first.matches("// owned ").count(), COMMENT_COUNT);
}
