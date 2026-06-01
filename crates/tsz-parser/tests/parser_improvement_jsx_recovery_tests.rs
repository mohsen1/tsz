//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — jsx recovery.

use crate::parser::test_fixture::{
    assert_no_errors_labeled, assert_no_errors_named, parse_source_named,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn test_adjacent_jsx_roots_in_tsx_report_ts2657() {
    let source = r"
declare namespace JSX { interface Element { } }

<div></div>
<div></div>

var x = <div></div><div></div>
";
    let (parser, _root) = parse_source_named("test.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let ts2657_count = diagnostics.iter().filter(|d| d.code == 2657).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    // tsc emits TS2657 for adjacent JSX roots in ALL JSX files (.tsx, .jsx, .js)
    assert!(
        ts2657_count >= 1,
        "Expected TS2657 for adjacent JSX siblings in TSX, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Adjacent JSX recovery should not leak TS1003, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 0,
        "Adjacent JSX recovery should not leak TS1109, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_jsx_type_arguments_in_js_report_ts2657() {
    let source = r#"
/// <reference path="/.lib/react.d.ts" />
import { MyComp, Prop } from "./component";
import * as React from "react";

let x = <MyComp<Prop> a={10} b="hi" />; // error, no type arguments in js
"#;
    let (parser, _root) = parse_source_named("file.jsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2657),
        "Expected TS2657 for JSX type arguments in JS recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&1003),
        "Expected TS1003 alongside TS2657 for illegal JSX type-argument syntax, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_call_type_argument_syntax_prefers_relational_parsing() {
    let source = r#"
Foo<number>();
Foo<number>(1);
Foo<number>``;
"#;
    let (parser, _root) = parse_source_named("a.jsx", source);

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected only the empty-call JS generic syntax case to emit TS1109, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Non-JSX JS generic-call syntax should not leak JSX TS1003 recovery diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_jsx_type_arguments_in_js_with_closing_tag_report_ts17002() {
    let source = r#"
<Foo<number>></Foo>;
"#;
    let (parser, _root) = parse_source_named("a.jsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17002),
        "Expected TS17002 for the mismatched closing tag after JS JSX type-argument recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2657),
        "Expected TS2657 for the recovered adjacent JSX roots, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_unclosed_jsx_fragment_after_unary_plus_in_tsx_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let (parser, _root) = parse_source_named("index.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TSX unary `+ <>` recovery to report TS17014, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unclosed_jsx_fragment_after_unary_plus_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let (parser, _root) = parse_source_named("index.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TS17014 for JS unary `+ <>` JSX-fragment recovery, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_tilde_then_malformed_jsx_reports_ts1003() {
    let source = "~< <";
    let (parser, _root) = parse_source_named("a.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert!(
        codes.contains(&1003),
        "Expected TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 1,
        "Expected exactly one TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 1,
        "Expected exactly one trailing TS1109 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_plus_then_numeric_jsx_head_reports_ts1003_without_ts1109() {
    let source = r#"
const x = "oops";
const y = + <1234> x;
"#;
    let (parser, _root) = parse_source_named("index.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed JSX tag head `<1234>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 fallback for malformed numeric JSX tag head, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_unary_plus_mixed_type_assertion_and_fragment_matches_conformance_shape() {
    let source = r#"
const x = "oops";

const a = + <number> x;
const b = + <> x;
const c = + <1234> x;
"#;
    let (parser, _root) = parse_source_named("index.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17008 from unary `+ <number> x` JSX recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17014 from unary `+ <> x` JSX recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed numeric JSX tag head `<1234>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT),
        "Expected TS1382 on malformed numeric JSX tag head close token, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "Expected TS1005 recovery tail after malformed JSX unary expressions, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_bang_then_braced_jsx_head_reports_ts17008_without_ts1109() {
    let source = "!< {:>";
    let (parser, _root) = parse_source_named("a.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed braced JSX tag head, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17008 unclosed JSX element recovery for `!< {{:>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 fallback for malformed braced JSX tag head, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_malformed_extends_in_generic_arrow_ambiguity_prefers_jsx_ts1382() {
    let source = r#"
declare namespace JSX {
    interface Element { isElement; }
}

var x4 = <T extends={true}>() => {}</T>;
x4.isElement;

var x5 = <T extends>() => {}</T>;
x5.isElement;
"#;
    let (parser, _root) = parse_source_named("file.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1382_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT)
        .count();

    assert!(
        ts1382_count >= 2,
        "Expected malformed `extends` TSX ambiguity to emit TS1382 on both forms, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 Type expected diagnostics for malformed `extends` JSX ambiguity, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 diagnostics for malformed `extends` JSX ambiguity, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_jsx_and_type_assertion_conformance_codes_exclude_ts1003() {
    let source = r#"
declare var createElement: any;

class foo {}

var x: any;
x = <any> { test: <any></any> };

x = <any><any></any>;
 
x = <foo>hello {<foo>{}} </foo>;

x = <foo test={<foo>{}}>hello</foo>;

x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;

x = <foo>x</foo>, x = <foo/>;

<foo>{<foo><foo>{/foo/.test(x) ? <foo><foo></foo> : <foo><foo></foo>}</foo>}</foo>
"#;
    let (parser, _root) = parse_source_named("jsxAndTypeAssertion.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let malformed_jsx_statement_terminators = [
        "x = <foo>hello {<foo>{}} </foo>;",
        "x = <foo test={<foo>{}}>hello</foo>;",
        "x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;",
    ]
    .into_iter()
    .map(|statement| {
        source
            .find(statement)
            .map(|start| start as u32 + statement.len() as u32 - 1)
            .expect("target JSX statement should exist")
    })
    .collect::<Vec<_>>();

    assert_eq!(
        ts1003_count, 0,
        "Expected no TS1003 for jsxAndTypeAssertion.tsx parser diagnostics, got diagnostics: {diagnostics:?}"
    );
    for semicolon_pos in malformed_jsx_statement_terminators {
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::EXPECTED
                    && diag.start == semicolon_pos
                    && diag.message == "'}' expected."
            }),
            "Expected TS1005 \"'}}' expected.\" at malformed JSX statement terminator pos {semicolon_pos}, got diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn test_tsx_fragment_errors_conformance_shape_matches_mismatch_then_eof_sequence() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div>

<>eof
"#;
    let (parser, _root) = parse_source_named("file.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
            diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            diagnostic_codes::EXPECTED,
        ],
        "Expected TS17015/TS17014/TS1005 recovery for malformed + EOF JSX fragments, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_actual_conformance_file_matches_expected_codes() {
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../TypeScript/tests/cases/conformance/jsx/tsxFragmentErrors.tsx"
    );
    let source = match std::fs::read_to_string(fixture_path) {
        Ok(source) => source,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return;
        }
        Err(err) => {
            panic!("failed to read tsxFragmentErrors conformance fixture {fixture_path}: {err}")
        }
    };
    let (parser, _root) = parse_source_named("file.tsx", &source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
            diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            diagnostic_codes::EXPECTED,
        ],
        "Expected TS17015/TS17014/TS1005 on actual tsxFragmentErrors conformance file, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_stripped_source_matches_expected_positions() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div> // Error

<>eof   // Error
"#
    .to_string();
    let line_map = LineMap::build(&source);
    let (parser, _root) = parse_source_named("file.tsx", &source);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT
                    | diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG
            )
        })
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, &source);
            (diag.code, pos.line + 1, pos.character + 1)
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
                10,
                7,
            ),
            (
                diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
                10,
                11,
            ),
        ],
        "Expected JSX fragment recovery positions to match tsc for tsxFragmentErrors.tsx, got {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Adjacent JSX sibling recovery preserves every element as a synthetic comma
// expression (so emit prints all siblings, not just the first).
//
// Rule: when two or more JSX root elements appear adjacently in an expression
// context (`<a/><b/>`, or elements on consecutive lines), tsc recovers them by
// wrapping them in a synthetic comma `BinaryExpression` and reports TS2657.
// These tests verify the tree keeps the siblings; the spelling of the tag and
// the inline-vs-multiline layout must not change the structure.
// ---------------------------------------------------------------------------

use crate::parser::syntax_kind_ext;

/// Count JSX element/self-closing/fragment operands reachable through a chain of
/// comma `BinaryExpression`s starting at `node`. Returns 0 if `node` is not a
/// comma binary (i.e. recovery did not wrap siblings).
fn count_jsx_comma_chain_operands(
    parser: &crate::parser::ParserState,
    node: crate::parser::NodeIndex,
) -> usize {
    let arena = parser.get_arena();
    let Some(n) = arena.get(node) else {
        return 0;
    };
    if matches!(
        n.kind,
        syntax_kind_ext::JSX_ELEMENT
            | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
            | syntax_kind_ext::JSX_FRAGMENT
    ) {
        return 1;
    }
    if n.kind != syntax_kind_ext::BINARY_EXPRESSION {
        return 0;
    }
    let Some(bin) = arena.get_binary_expr(n) else {
        return 0;
    };
    if bin.operator_token != tsz_scanner::SyntaxKind::CommaToken as u16 {
        return 0;
    }
    count_jsx_comma_chain_operands(parser, bin.left)
        + count_jsx_comma_chain_operands(parser, bin.right)
}

/// Find the first comma-binary chain in the arena and return how many JSX
/// element operands it holds, plus the count of TS2657 diagnostics.
fn jsx_sibling_recovery_summary(source: &str, file: &str) -> (usize, usize) {
    let (parser, _root) = parse_source_named(file, source);
    let arena = parser.get_arena();
    let ts2657 = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::JSX_EXPRESSIONS_MUST_HAVE_ONE_PARENT_ELEMENT)
        .count();
    // The synthetic comma binary is the outermost comma node whose operands are
    // all JSX. Scan every node and take the maximum operand count found.
    let mut best = 0;
    for (idx, node) in arena.nodes.iter().enumerate() {
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            continue;
        }
        let ni = crate::parser::NodeIndex(idx as u32);
        let operands = count_jsx_comma_chain_operands(&parser, ni);
        if operands > best {
            best = operands;
        }
    }
    (best, ts2657)
}

#[test]
fn test_adjacent_jsx_siblings_multiline_wrapped_in_comma_expression() {
    // Two elements on consecutive lines — the tsxErrorRecovery2/3 file1 shape.
    let (operands, ts2657) = jsx_sibling_recovery_summary("<div></div>\n<div></div>\n", "f.tsx");
    assert_eq!(
        operands, 2,
        "multiline adjacent JSX siblings should be preserved as a 2-operand comma chain"
    );
    assert_eq!(ts2657, 1, "exactly one TS2657 for the sibling run");
}

#[test]
fn test_adjacent_jsx_siblings_inline_wrapped_in_comma_expression() {
    // Two elements with no whitespace between them — the file2 shape. Different
    // surface layout, same structural recovery.
    let (operands, ts2657) =
        jsx_sibling_recovery_summary("var x = <div></div><div></div>\n", "f.tsx");
    assert_eq!(
        operands, 2,
        "inline adjacent JSX siblings should be preserved as a 2-operand comma chain"
    );
    assert_eq!(ts2657, 1, "exactly one TS2657 for the inline sibling run");
}

#[test]
fn test_adjacent_jsx_siblings_renamed_tags_wrapped_in_comma_expression() {
    // Tag spelling must not matter: `<span/>` then `<section/>` self-closing.
    let (operands, ts2657) = jsx_sibling_recovery_summary("var x = <span /><section />\n", "f.tsx");
    assert_eq!(
        operands, 2,
        "self-closing adjacent siblings with different tag names should be preserved"
    );
    assert_eq!(ts2657, 1, "exactly one TS2657 regardless of tag names");
}

#[test]
fn test_three_adjacent_jsx_siblings_all_preserved() {
    // More than two siblings still collapse into one comma chain holding all.
    let (operands, ts2657) =
        jsx_sibling_recovery_summary("var x = <a></a><b></b><c></c>\n", "f.tsx");
    assert_eq!(
        operands, 3,
        "three adjacent JSX siblings should all be preserved in the comma chain"
    );
    assert_eq!(ts2657, 1, "exactly one TS2657 for the whole sibling run");
}

#[test]
fn test_single_jsx_element_not_wrapped_in_comma() {
    // Negative case: a lone JSX element must NOT be wrapped in a comma binary
    // and must NOT emit TS2657.
    let (operands, ts2657) = jsx_sibling_recovery_summary("var x = <div></div>;\n", "f.tsx");
    assert_eq!(
        operands, 0,
        "a single JSX element must not form a comma chain"
    );
    assert_eq!(ts2657, 0, "no TS2657 for a single element");
}

// ── Repro for issue #11332: conditional expression in TSX children ──────────

#[test]
fn test_jsx_conditional_expression_in_children_no_errors() {
    // Simple case: x ? <span/> : <strong/> inside JSX expression container
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>{x ? <span /> : <strong />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_expression_multiline_no_errors() {
    // Multiline conditional: when_true on a separate line
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>
  {x
    ? <span />
    : <strong />
  }
</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_expression_named_components_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>{x ? <Spinner /> : <Content />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_expression_with_null_branch_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>{x ? <span /> : null}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_expression_page_template_pattern_no_errors() {
    // nextjs page template pattern
    assert_no_errors_named(
        "page.tsx",
        r#"function Page({ isActive }: { isActive: boolean }) {
  return (
    <main>
      {isActive ? <ActiveView /> : <InactiveView />}
    </main>
  );
}"#,
    );
}

#[test]
fn test_jsx_conditional_with_nested_jsx_branches_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>
  {x
    ? <div className="active"><p>Active</p></div>
    : <div className="inactive"><p>Inactive</p></div>
  }
</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_renamed_branches_same_behavior() {
    // Rule: conditional JSX branches must work regardless of element name
    for source in [
        r#"const n = <div>{x ? <span /> : <strong />}</div>;"#,
        r#"const n = <div>{x ? <A /> : <B />}</div>;"#,
        r#"const n = <div>{x ? <Foo /> : <Bar />}</div>;"#,
        r#"const n = <div>{x ? <alpha /> : <beta />}</div>;"#,
    ] {
        assert_no_errors_named("test.tsx", source);
    }
}

#[test]
fn test_jsx_conditional_with_generic_component_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>{x ? <Comp<string> /> : <Other />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_with_template_literal_branches() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = <div>{x ? `hello` : <strong />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_with_template_expression_in_true_branch() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const v = "world";
const n = <div>{x ? <span>{`hello ${v}`}</span> : <strong />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_in_return_statement_tsx() {
    assert_no_errors_named(
        "test.tsx",
        r#"const Comp = ({ x }: { x: boolean }) => x ? <span /> : <strong />;"#,
    );
}

#[test]
fn test_jsx_conditional_with_generic_arrow_in_children() {
    // In .tsx files, <T>(v: T) => ... is ambiguous; tsc requires the <T,> form.
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const render = <T,>(v: T) => v ? <span /> : <strong />;
const n = <div>{render(x)}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_patterns_no_errors() {
    // Rule: conditional expressions with JSX branches must parse correctly
    // in all common nextjs template patterns.
    // Cases from both the original 11-case and 8-case suites, merged with
    // consistent (label, source) tuple order.
    let cases: &[(&str, &str)] = &[
        ("logical AND", r#"const n = <div>{x && <span />}</div>;"#),
        (
            "nested ternary",
            r#"const n = <div>{a ? (b ? <A /> : <B />) : <C />}</div>;"#,
        ),
        (
            "short-circuit null",
            r#"const n = <div>{x && (y ? <A /> : null)}</div>;"#,
        ),
        (
            "arrow callback ternary",
            r#"const n = <div>{items.map(i => i.x ? <A /> : <B />)}</div>;"#,
        ),
        (
            "multiple conditions",
            r#"const n = <div>{a && b ? <A /> : <B />}</div>;"#,
        ),
        (
            "parenthesized branches",
            r#"const n = <div>{x ? (<span />) : (<strong />)}</div>;"#,
        ),
        (
            "fragment branches",
            r#"const n = <div>{x ? <><span /></> : <><strong /></>}</div>;"#,
        ),
        (
            "string branch",
            r#"const n = <div>{x ? "yes" : <strong />}</div>;"#,
        ),
        (
            "children in branches",
            r#"const n = <div>{x ? <div><p>Yes</p></div> : <div><p>No</p></div>}</div>;"#,
        ),
        (
            "conditional with props",
            r#"const n = <div>{x ? <Comp key="a" data={d} /> : <Other />}</div>;"#,
        ),
        (
            "server component async return",
            r#"async function Page() { return <div>{loading ? <Spinner /> : <Content />}</div>; }"#,
        ),
        (
            "multiple expressions",
            r#"const n = <div>{a}{x ? <span/> : <strong/>}{b}</div>;"#,
        ),
        (
            "complex true branch",
            r#"const n = <div>{x ? <span className="test">text</span> : <strong/>}</div>;"#,
        ),
        (
            "nested false branch",
            r#"const n = <div>{x ? null : <div><span/></div>}</div>;"#,
        ),
        (
            "template condition",
            r#"const n = <div>{`${x}` ? <span/> : <strong/>}</div>;"#,
        ),
        (
            "type assertion condition",
            r#"const n = <div>{(x as boolean) ? <span/> : null}</div>;"#,
        ),
        (
            "nullish coalescing",
            r#"const n = <div>{x ?? <span/>}</div>;"#,
        ),
        (
            "arrow function",
            r#"const f = (x: boolean) => <div>{x ? <span/> : null}</div>;"#,
        ),
        (
            "map callback",
            r#"const n = <div>{items.map((i) => i ? <span key={i}/> : null)}</div>;"#,
        ),
    ];

    for (label, source) in cases {
        assert_no_errors_labeled("test.tsx", label, source);
    }
}

#[test]
fn test_jsx_conditional_in_complex_nextjs_page_template() {
    assert_no_errors_named(
        "layout.tsx",
        r#"import type { ReactNode } from 'react';

type Props = { children: ReactNode; params: { lang: string } };

export default function Layout({ children, params }: Props) {
  const isRtl = params.lang === 'ar';
  return (
    <html lang={params.lang} dir={isRtl ? 'rtl' : 'ltr'}>
      <body>
        {isRtl ? <RtlProvider>{children}</RtlProvider> : children}
      </body>
    </html>
  );
}"#,
    );
}

#[test]
fn test_jsx_conditional_in_attribute_value() {
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <Comp content={loading ? <Spinner /> : <Content />} />;"#,
    );
}

#[test]
fn test_jsx_conditional_in_template_span_expression() {
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <div className={`${loading ? "loading" : "ready"}`}>{loading ? <Spinner /> : <Content />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_after_template_literal_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <div>{`prefix`}{x ? <span /> : <strong />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_with_optional_chaining_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <div>{data?.isLoading ? <Spinner /> : <Content />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_in_arrow_returning_jsx() {
    assert_no_errors_named(
        "test.tsx",
        r#"const Comp = ({ x }: { x: boolean }) => (
  <div>
    {x ? <span>Yes</span> : <strong>No</strong>}
  </div>
);"#,
    );
}

#[test]
fn test_jsx_conditional_string_interpolation_class_then_jsx() {
    // Template literal for className THEN JSX conditional (scanner state transition)
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true;
const n = (
  <div className={`${x ? "active" : "inactive"}`}>
    {x ? <ActiveContent /> : <InactiveContent />}
  </div>
);"#,
    );
}

#[test]
fn test_jsx_conditional_in_map_callback() {
    assert_no_errors_named(
        "test.tsx",
        r#"const items = [{ id: 1, active: true }];
const n = <ul>{items.map(item => (
  <li key={item.id}>{item.active ? <Active /> : <Inactive />}</li>
))}</ul>;"#,
    );
}

#[test]
fn test_jsx_conditional_logical_and_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <div>{x && <span/>}{y || <strong/>}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_in_jsx_attribute_no_errors() {
    assert_no_errors_named("test.tsx", r#"const n = <A content={x ? <B/> : null} />;"#);
}

#[test]
fn test_jsx_conditional_generic_component_type_args_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <div>{x ? <Component<string>/> : null}</div>;"#,
    );
}

#[test]
fn generic_component_type_and_spread_no_errors() {
    // #11345: generic components that return a spread JSX attribute and are
    // called in the same TSX module should parse without token-reset diagnostics.
    for source in [
        r#"
type P<T> = { value: T };
function Comp<T>(props: P<T>) {
  return <div {...props} />;
}
Comp({ value: 1 });
"#,
        r#"
function Comp<K>(props: { value: K }) {
  return <div {...props} />;
}
const s = Comp<string>({ value: "x" });
"#,
        r#"
type Payload<T> = { label: T };
const Widget = function View<T>(payload: Payload<T>) {
  return <article {...payload} />;
};
Widget({ label: "x" });
"#,
        r#"
function render<T>(props: T) {
  return <div {...(props as any < 1 ? props : { fallback: 1 })} />;
}
render({ value: 123 });
"#,
    ] {
        assert_no_errors_named("test.tsx", source);
    }
}

#[test]
fn test_jsx_conditional_deeply_nested_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
const n = (
  <Outer>
    <Middle>
      {isLoading ? (
        <div className="loading">
          <Spinner size="large" />
        </div>
      ) : (
        <div className="content">
          <Title>Hello</Title>
          <Body>{text}</Body>
        </div>
      )}
    </Middle>
  </Outer>
);
"#,
    );
}

#[test]
fn test_jsx_page_component_conditional_render_no_errors() {
    assert_no_errors_named(
        "page.tsx",
        r#"
import React from 'react';

interface PageProps {
  isLoading: boolean;
  error: string | null;
  data: { title: string } | null;
}

export default function Page({ isLoading, error, data }: PageProps) {
  return (
    <main className="container">
      <header>
        <h1>My Page</h1>
      </header>
      <section>
        {isLoading ? (
          <div className="spinner">Loading...</div>
        ) : error ? (
          <div className="error">{error}</div>
        ) : (
          <div className="content">
            <h2>{data?.title}</h2>
          </div>
        )}
      </section>
    </main>
  );
}
"#,
    );
}

#[test]
fn test_jsx_conditional_both_branches_template_literals_no_errors() {
    // Exact pattern from nextjs dashboard.tsx: both branches are template spans.
    // This is the "template conditional boundary" from issue #11332.
    assert_no_errors_named(
        "test.tsx",
        r#"
function ActivityList({ activity }: { activity: { kind: string; version: string; status: string; priority: string; title: string; id: string }[] }) {
  return (
    <ol>
      {activity.map((item) => (
        <li key={item.id}>
          {item.kind === "release"
            ? `${item.version}: ${item.status}`
            : `${item.priority}: ${item.title}`}
        </li>
      ))}
    </ol>
  );
}
"#,
    );
}

#[test]
fn test_jsx_conditional_template_span_as_true_branch_jsx_false() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true; const v = "world"; const n = <div>{x ? `${v}` : <strong />}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_jsx_true_template_span_false() {
    assert_no_errors_named(
        "test.tsx",
        r#"const x = true; const v = "world"; const n = <div>{x ? <span /> : `${v}`}</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_both_branches_multivar_template_spans() {
    // Both branches are template spans with multiple variables — the actual nextjs dashboard pattern
    assert_no_errors_named(
        "test.tsx",
        r#"
const n = <div>{item.kind === "release"
  ? `${item.version}: ${item.status}`
  : `${item.priority}: ${item.title}`}
</div>;"#,
    );
}

#[test]
fn test_jsx_conditional_in_generic_function_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
function render<T extends { label: string }>(item: T, show: boolean): JSX.Element {
  return <div>{show ? <span>{item.label}</span> : null}</div>;
}
"#,
    );
}

#[test]
fn test_jsx_conditional_in_generic_arrow_tsx_no_errors() {
    // Generic arrow with JSX conditional (requires trailing comma for TSX disambiguation)
    assert_no_errors_named(
        "test.tsx",
        r#"
const render = <T extends { label: string },>(item: T, show: boolean) => (
  <div>{show ? <span>{item.label}</span> : null}</div>
);
"#,
    );
}

#[test]
fn test_generic_arrow_no_comma_no_params_tsx_no_errors() {
    // #11320: single-parameter generic arrow without explicit disambiguating
    // comma/default must still parse in TSX.
    for source in [
        r#"
export const f = <T>() => <div>{1 + 2}</div>;
"#,
        r#"
const g = <K>() => <div>{flag ? <Yes /> : <No />}</div>;
"#,
    ] {
        assert_no_errors_named("test.tsx", source);
    }
}

#[test]
fn test_generic_arrow_no_comma_wrapped_return_type_tsx_no_errors() {
    for source in [
        r#"
const build = <X>() => (
  <div className={classes.root}>
    {compute()}
  </div>
);
"#,
        r#"
const launch = <Y extends { text: string }>() => <Badge text={labelText(Y)} />;
"#,
    ] {
        assert_no_errors_named("test.tsx", source);
    }
}

#[test]
fn test_jsx_conditional_chained_ternary_template_no_errors() {
    // Chained ternary with template strings
    assert_no_errors_named(
        "test.tsx",
        r#"
const n = <div>{
  area === "parser"
    ? `prs-${title}`
    : area === "binder"
      ? `bnd-${title}`
      : area === "type-checker"
        ? `chk-${title}`
        : `emt-${title}`
}</div>;
"#,
    );
}

#[test]
fn test_jsx_conditional_clsx_and_jsx_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
const n = <li className={clsx("base", isActive && "active", isDisabled && "disabled")}>
  {isActive ? <ActiveIcon /> : <DefaultIcon />}
</li>;
"#,
    );
}

#[test]
fn test_jsx_conditional_with_type_argument_component_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
interface Props { value: string }
const n = <div>{x ? <Component<Props>/> : null}</div>;
"#,
    );
}

#[test]
fn test_jsx_conditional_allows_in_operator_in_expression() {
    // JSX embedded expressions clear CONTEXT_FLAG_DISALLOW_IN, so the `in` operator
    // is available even when an enclosing for-loop initializer sets the flag.
    assert_no_errors_named(
        "test.tsx",
        r#"
const set = new Set(["a", "b"]);
const items = ["a", "c"];
const n = (
  <ul>
    {items.map(item => (
      <li key={item}>{item in set ? <Active /> : <Inactive />}</li>
    ))}
  </ul>
);
"#,
    );
}

#[test]
fn test_jsx_conditional_in_parenthesized_expression_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
const x = true;
const n = (x ? <span /> : <strong />);
"#,
    );
}

#[test]
fn test_jsx_conditional_object_property_value_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
const x = true;
const obj = { content: x ? <span /> : <strong /> };
"#,
    );
}

#[test]
fn test_jsx_conditional_as_function_argument_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
const x = true;
const result = render(x ? <span /> : <strong />);
"#,
    );
}

#[test]
fn test_jsx_conditional_in_variable_declaration_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
const x = true;
const el = x ? <span>true</span> : <strong>false</strong>;
"#,
    );
}

#[test]
fn test_jsx_conditional_inside_template_span_inside_jsx_no_errors() {
    // JSX conditional inside a template span inside a JSX embedded expression —
    // the most complex "template conditional boundary" scenario.
    assert_no_errors_named(
        "test.tsx",
        r#"const n = <div>{`${cond ? <A/> : <B/>}`}</div>;"#,
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Regression tests: compound `>>` / `>>>` tokens in generic arrow function
// lookahead (`look_ahead_is_generic_arrow_function` depth counter).
//
// When a type parameter constraint itself is generic (e.g. `T extends Map<K,V>`)
// the scanner emits a `GreaterThanGreaterThanToken` (`>>`) for the adjacent
// closing brackets. The old depth counter only handled bare `>`, so it never
// detected the end of the parameter list and fell through to JSX parsing.
//
// Rule: when `<T extends Generic<…>>(…) =>` appears in a `.tsx` file, the
// parser must recognise it as a generic arrow function regardless of the
// type-parameter name (T, K, X, A, B, …) and nesting depth (2 or 3 levels).
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_generic_arrow_with_double_close_param_t_no_errors() {
    // `>>` produced by `Map<string, number>>`  — type parameter named T
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <T extends Map<string, number>>(x: T): T => x;"#,
    );
}

#[test]
fn test_generic_arrow_with_double_close_param_k_no_errors() {
    // Same structural rule; different type-parameter name (K).
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <K extends Map<string, number>>(x: K): K => x;"#,
    );
}

#[test]
fn test_generic_arrow_with_double_close_param_x_no_errors() {
    // Same structural rule; different type-parameter name (X).
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <X extends Map<string, number>>(x: X): X => x;"#,
    );
}

#[test]
fn test_generic_arrow_record_constraint_t_no_errors() {
    // `Record<string, unknown>` — single-level nesting, `>>` at close.
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <T extends Record<string, unknown>>(x: T) => x;"#,
    );
}

#[test]
fn test_generic_arrow_record_constraint_a_no_errors() {
    // Same as above with type-parameter name A.
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <A extends Record<string, unknown>>(x: A) => A;"#,
    );
}

#[test]
fn test_generic_arrow_multi_param_with_nested_no_errors() {
    // Two type parameters, second has a nested generic constraint.
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <T, K extends Record<string, T>>(x: T, y: K) => y;"#,
    );
}

#[test]
fn test_generic_arrow_triple_nesting_produces_three_greater_t_no_errors() {
    // Triple nesting: `ReadonlyArray<Set<T>>` closes with `>>>`.
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <T extends ReadonlyArray<Set<T>>>(x: T) => x;"#,
    );
}

#[test]
fn test_generic_arrow_triple_nesting_produces_three_greater_b_no_errors() {
    // Same triple-nesting rule with type-parameter name B.
    assert_no_errors_named(
        "test.tsx",
        r#"const f = <B extends ReadonlyArray<Set<B>>>(x: B) => x;"#,
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Regression tests: JSX not parsed inside ambient declaration contexts.
//
// Inside `declare namespace / module / class`, `<` is a type argument or
// relational operator — JSX elements are never valid there. The old parser
// entered JSX parsing unconditionally in `.tsx` files, causing false-positive
// TS1005 / TS1003 errors for valid ambient declarations that use angle-bracket
// operators or complex generic types.
//
// Rule: when the ambient-declaration context flag is set, `<` in expression
// position must not be treated as a JSX opener, regardless of file extension.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ambient_namespace_interface_key_remapping_no_errors() {
    // This is the canonical repro from issue #10930.
    // `[K in keyof T as T[K] extends string ? K : never]` uses `<` inside
    // a declare namespace; the old code incorrectly entered JSX parsing.
    assert_no_errors_named(
        "test.tsx",
        r#"
declare namespace Lib {
    type StringKeys<T> = {
        [K in keyof T as T[K] extends string ? K : never]: T[K];
    };
}
"#,
    );
}

#[test]
fn test_ambient_module_key_remap_with_as_clause_no_errors() {
    // Repro from issue #10934 (`parser-3-20`): key-remap mapped types inside
    // ambient module declarations should stay in type-context, not JSX.
    assert_no_errors_named(
        "test.tsx",
        r#"
declare module "row3-ambient" {
    export interface Source {
        "utility-types-project_value": string;
    }

    export type Remap<T> = {
        [K in keyof T as K extends `${infer Prefix}_${infer Suffix}`
            ? `${Prefix}${Capitalize<Suffix>}`
            : K]: T[K]
    };

    export type Result = Remap<Source>;
}
    "#,
    );
}

#[test]
fn test_ambient_module_key_remap_with_as_clause_and_renamed_bindings_no_errors() {
    // Exercise the same structural case with different binder names so recovery
    // is not tied to a single parameter identifier spelling.
    assert_no_errors_named(
        "test.tsx",
        r#"
declare module "row3-ambient-rename" {
    type RemapRow<Shape> = {
        [Key in keyof Shape as Key extends `${infer Head}_${infer Tail}`
            ? `${Head}${Capitalize<Tail>}`
            : Key]: Shape[Key]
    };

    export type Result<Value> = RemapRow<Value>;
}
    "#,
    );
}

#[test]
fn test_ambient_module_conditional_type_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
declare module "lib" {
    type IsString<T> = T extends string ? true : false;
}
"#,
    );
}

#[test]
fn test_ambient_namespace_deeply_nested_generic_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
declare namespace Deep {
    type Nested<A> = Map<string, Set<A>>;
}
"#,
    );
}

#[test]
fn test_ambient_class_method_generic_no_errors() {
    assert_no_errors_named(
        "test.tsx",
        r#"
declare class Container {
    get<T extends Record<string, unknown>>(key: string): T;
}
"#,
    );
}

#[test]
fn test_ambient_namespace_relational_operator_no_errors() {
    // `extends` conditional with `<` operator inside ambient namespace
    assert_no_errors_named(
        "test.tsx",
        r#"
declare namespace Util {
    type NonEmptyArray<T> = T extends Array<infer U>
        ? [U, ...U[]]
        : never;
}
"#,
    );
}
