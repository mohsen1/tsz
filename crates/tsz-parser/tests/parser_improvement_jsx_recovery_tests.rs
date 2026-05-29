//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — jsx recovery.

use crate::parser::test_fixture::parse_source_named;
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
