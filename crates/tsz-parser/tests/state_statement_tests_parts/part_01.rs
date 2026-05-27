/// `break <reserved-word>` in a context where the reserved word cannot be used
/// as a label must emit TS1003 (Identifier expected) AND continue parsing the
/// reserved word as an expression — for `await` in a static block this yields
/// TS1109 (Expression expected) at the missing operand. tsc emits both; tsz
/// previously consumed `await` as a (mis-named) label and missed the TS1109.
#[test]
fn test_break_reserved_word_in_static_block_emits_ts1003_and_ts1109() {
    let source = r#"class C {
    static {
        break await;
    }
}"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for `break await` reserved-label in static block, got: {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 for the await expression missing operand, got: {codes:?}"
    );
}

/// Same structural rule for `continue <reserved-word>` — exercises the
/// parallel branch in `parse_continue_statement`.
#[test]
fn test_continue_reserved_word_in_static_block_emits_ts1003_and_ts1109() {
    let source = r#"class C {
    static {
        foo: while (true) {
            continue await;
        }
    }
}"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for `continue await` reserved-label in static block, got: {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 for the await expression missing operand, got: {codes:?}"
    );
}

/// Negative: a normal identifier label after `break` must still be accepted
/// without emitting any of these diagnostics.
#[test]
fn test_break_named_label_still_parses_cleanly() {
    let source = r#"function f() {
    outer: for (let i = 0; i < 10; i++) {
        break outer;
    }
}"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Did not expect TS1003 for `break <named-label>`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Did not expect TS1109 for `break <named-label>`, got: {codes:?}"
    );
}

/// `declare class C extends await {}` in a `.d.ts` file is valid: `await` is
/// allowed as an identifier in declaration files. Match tsc by suppressing
/// the parser-level TS1109 emission in `parse_heritage_left_hand_expression_base`.
#[test]
fn test_await_as_heritage_identifier_in_declaration_file_no_ts1109() {
    use crate::parser::test_fixture::parse_source_named;
    let source = r#"export {};
declare const await: any;
declare class C extends await {}
"#;
    let (parser, _root) = parse_source_named("index.d.ts", source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "TS1109 must not fire for `await` in heritage clause inside .d.ts, got: {codes:?}"
    );
}

/// In a regular `.ts` file, the same `extends await` should still emit TS1109
/// because `await` is a reserved word at expression positions outside .d.ts.
#[test]
fn test_await_as_heritage_identifier_in_ts_file_emits_ts1109() {
    use crate::parser::test_fixture::parse_source_named;
    let source = r#"declare const await: any;
declare class C extends await {}
"#;
    let (parser, _root) = parse_source_named("test.ts", source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 for `await` in heritage clause inside .ts, got: {codes:?}"
    );
}

/// Test that `[await]` as a class member computed property name in a nested class inside
/// a static block emits TS1109 (Expression expected) instead of TS1213. tsc treats `await`
/// as a keyword in static blocks, so using it as a computed property name in a class
/// defined within a static block should fail with TS1109.
#[test]
fn test_await_as_computed_property_in_class_in_static_block_emits_ts1109_not_ts1213() {
    let source = r#"class C {
    static {
        class D {
            [await] = 1;
        }
    }
}"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 for class D '[await]' computed property in static block, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO
        ),
        "Should NOT emit TS1213 for '[await]' in static block, got codes: {codes:?}"
    );
}

/// Test that `await => {}` (arrow function with `await` as single parameter) in a static
/// block emits TS1109 (Expression expected) at the `await` position. Previously, `STATIC_BLOCK`
/// was cleared before arrow parameter parsing, so `await` was accepted as a valid identifier
/// and the error was a misleading TS1005 at the arrow.
#[test]
fn test_await_as_arrow_single_param_in_static_block_emits_ts1109() {
    let source = r#"class C {
    static {
        await => {};
    }
}"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 for 'await =>' in static block, got codes: {codes:?}"
    );
}

#[test]
fn static_block_bare_await_computed_names_report_at_close_bracket() {
    let source = r#"class C {
    static {
        ({ [await]: 1 });
        class D {
            [await] = 1;
        }
    }
}"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let object_await_pos = source.find("[await]").unwrap() as u32 + 1;
    let object_close_bracket_pos = source.find("[await]").unwrap() as u32 + "[await".len() as u32;
    let class_await_pos = source.rfind("[await]").expect("class computed await") as u32 + 1;
    let class_close_bracket_pos =
        source.rfind("[await]").expect("class computed await") as u32 + "[await".len() as u32;

    for expected_pos in [object_close_bracket_pos, class_close_bracket_pos] {
        assert!(
            diags.iter().any(|diag| {
                diag.code == diagnostic_codes::EXPRESSION_EXPECTED && diag.start == expected_pos
            }),
            "expected TS1109 at closing bracket byte {expected_pos}, got {diags:?}"
        );
    }
    for early_pos in [object_await_pos, class_await_pos] {
        assert!(
            !diags.iter().any(|diag| {
                diag.code == diagnostic_codes::EXPRESSION_EXPECTED && diag.start == early_pos
            }),
            "TS1109 should not be anchored at bare `await` byte {early_pos}: {diags:?}"
        );
    }
}

#[test]
fn static_block_await_arrow_candidates_recover_as_await_expressions() {
    let source = r#"class C {
    static {
        const ff = (await) => { };
        const fff = await => { };
    }
}"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let parenthesized_close_pos = source.find("(await)").unwrap() as u32 + "(await".len() as u32;
    let parenthesized_arrow_pos = source.find(") =>").unwrap() as u32 + ") ".len() as u32;
    let bare_arrow_pos = source.rfind("=>").expect("bare await arrow") as u32;

    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                && diag.start == parenthesized_close_pos
        }),
        "expected TS1109 at `)` after `(await`, got {diags:?}"
    );
    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::EXPECTED
                && diag.start == parenthesized_arrow_pos
                && diag.message == "';' expected."
        }),
        "expected TS1005 ';' at parenthesized `=>`, got {diags:?}"
    );
    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::EXPRESSION_EXPECTED && diag.start == bare_arrow_pos
        }),
        "expected TS1109 at `=>` after bare `await`, got {diags:?}"
    );
}

#[test]
fn comma_between_consecutive_function_overloads_recovers_at_statement_boundary() {
    let source = r#"
function f1(), function f1();
function f2(), function f2() {}
function f3() {}, function f3();

class C {
    m1(), m1();
    m2(), m2() {}
    m3() {}, m3();
}
"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::OR_EXPECTED,
            diagnostic_codes::OR_EXPECTED,
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            diagnostic_codes::OR_EXPECTED,
            diagnostic_codes::OR_EXPECTED,
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        ],
        "expected comma recovery to match overloadConsecutiveness syntax diagnostics, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "comma recovery should not parse following overloads as malformed function expressions: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "comma after a function body should recover as a statement-list separator, not an expression: {:?}",
        parser.get_diagnostics()
    );
}
