//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — parameter recovery.

use crate::parser::test_fixture::{parse_source, parse_source_named};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn parameter_array_binding_reserved_words_match_tsc_recovery_fingerprints() {
    let source = "function a4([while, for, public]){ }\nfunction a5(...while) { }\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let first_comma = source.find(", for").expect("comma after while") as u32;
    let for_pos = source.find("for,").expect("for token") as u32;
    let second_comma = source.find(", public").expect("comma after for") as u32;
    let close_bracket = source.find("])").expect("array close bracket") as u32;
    let close_paren = close_bracket + 1;
    let rest_close_paren =
        source.find("while) {").expect("rest while") as u32 + "while".len() as u32;

    for (code, start, message) in [
        (diagnostic_codes::EXPECTED, first_comma, "'(' expected."),
        (
            diagnostic_codes::EXPRESSION_EXPECTED,
            for_pos,
            "Expression expected.",
        ),
        (diagnostic_codes::EXPECTED, second_comma, "'(' expected."),
        (diagnostic_codes::EXPECTED, close_bracket, "';' expected."),
        (
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            close_paren,
            "Declaration or statement expected.",
        ),
        (
            diagnostic_codes::EXPECTED,
            rest_close_paren,
            "'(' expected.",
        ),
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == code && diag.start == start && diag.message == message),
            "Expected diagnostic {code} at {start} with {message:?}; got {diagnostics:?}",
        );
    }

    assert!(
        diagnostics.iter().all(|diag| {
            !(diag.code == diagnostic_codes::EXPECTED
                && diag.start == first_comma
                && diag.message == "';' expected.")
        }),
        "First comma should use tsc's '(' recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().all(|diag| {
            !(diag.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                && diag.start == for_pos)
        }),
        "TS1128 should be anchored after the recovered binding pattern, got {diagnostics:?}",
    );
}

#[test]
fn parameter_type_predicate_tail_reports_comma_at_type_name() {
    let source = "function b2(a: b is A) {};";
    let (parser, root) = parse_source(source);
    let line_map = LineMap::build(source);

    let fingerprints: Vec<(u32, u32, u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.clone(),
            )
        })
        .collect();

    assert!(
        fingerprints.contains(&(
            diagnostic_codes::EXPECTED,
            1,
            18,
            "',' expected.".to_string()
        )),
        "expected TS1005 at `is`, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::EXPECTED,
            1,
            21,
            "',' expected.".to_string()
        )),
        "expected TS1005 at the predicate type name, got {fingerprints:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let function_node = arena.get(source_file.statements.nodes[0]).unwrap();
    let function = arena.get_function(function_node).unwrap();
    let parameter_texts: Vec<&str> = function
        .parameters
        .nodes
        .iter()
        .map(|&param_idx| {
            let param = arena.get_parameter(arena.get(param_idx).unwrap()).unwrap();
            let name = arena.get(param.name).unwrap();
            &source[name.pos as usize..name.end as usize]
        })
        .collect();
    assert_eq!(
        parameter_texts,
        vec!["a", "is", "A"],
        "invalid parameter type predicates should recover the tail as parameter names"
    );
}

#[test]
fn test_es5_bind_signature_with_this_parameter_parses() {
    let source = r#"
interface Test {
  bind<T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R, thisArg: T, ...args: A): (...args: B) => R;
}
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "ES5-style bind signature with a this parameter should parse cleanly: {diagnostics:?}"
    );
}

#[test]
fn test_function_parameter_list_missing_close_paren_reports_at_body_end() {
    let source = "function f(a {\n}";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let body_start = source.find('{').expect("body start") as u32;
    let body_end = source.rfind('}').expect("body end") as u32 + 1;
    let close_paren_diags: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 1005 && diag.message == "')' expected.")
        .collect();

    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == body_start
            && diag.message == "',' expected."),
        "Expected missing comma at the body opener, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == body_end
            && diag.message == "')' expected."),
        "Expected missing ')' after the recovered body, got {diagnostics:?}"
    );
    assert_eq!(
        close_paren_diags.len(),
        1,
        "Expected only one missing ')' recovery diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn test_optional_rest_parameter_reports_at_question_mark() {
    let source = "(...arg?) => 102;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let question_pos = source.find('?').expect("question position") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1047
                && diag.start == question_pos
                && diag.message == "A rest parameter cannot be optional."
        }),
        "Expected TS1047 at the question mark, got {diagnostics:?}"
    );
}

#[test]
fn test_reserved_word_type_reference_in_parameter_does_not_emit_ts1359() {
    let source = "class Foo { public banana(x: break) { } }";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().all(|diag| diag.code != 1359),
        "Type positions should not reject reserved-word identifiers with TS1359: {diagnostics:?}"
    );
}

#[test]
fn test_parameters_with_line_break_no_comma() {
    // Function parameters without comma but with line break
    // Should be more permissive to avoid false positives
    let source = r"
function foo(
    a: number
    b: string
) {
    return a + b;
}
";
    let (parser, _root) = parse_source(source);

    // Should not emit TS1005 for missing comma when there's a line break
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert!(
        ts1005_count <= 1,
        "Expected at most 1 TS1005 error, got {ts1005_count}",
    );
}

#[test]
fn js_flow_style_type_parameter_recovery_preserves_class_members() {
    let source = r#"
class B<T: BaseA> {
    _AClass: Class<T>;
    constructor(AClass: Class<T>) {
        this._AClass = AClass;
    }
}
"#;
    let (parser, root) = parse_source_named("test.js", source);

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let class_node = arena
        .get(source_file.statements.nodes[0])
        .expect("class declaration");
    let class_data = arena.get_class(class_node).expect("class data");
    assert_eq!(
        class_data.members.nodes.len(),
        2,
        "malformed JS type parameter recovery should still parse class members"
    );

    let property_node = arena
        .get(class_data.members.nodes[0])
        .expect("property member");
    let property = arena
        .get_property_decl(property_node)
        .expect("property data");
    assert!(
        property.type_annotation.is_some(),
        "property type annotation should be preserved for JS grammar diagnostics"
    );

    let constructor_node = arena
        .get(class_data.members.nodes[1])
        .expect("constructor member");
    let constructor = arena
        .get_constructor(constructor_node)
        .expect("constructor data");
    let parameter_node = arena
        .get(constructor.parameters.nodes[0])
        .expect("constructor parameter");
    let parameter = arena.get_parameter(parameter_node).expect("parameter data");
    assert!(
        parameter.type_annotation.is_some(),
        "constructor parameter type annotation should be preserved for JS grammar diagnostics"
    );
}

#[test]
fn test_argument_list_recovery_on_return_keyword() {
    let source = r"
const x = fn(
  return
);
const y = 1;
";

    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1135_count = diagnostics.iter().filter(|d| d.code == 1135).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1135_count >= 1,
        "Expected at least 1 TS1135 for malformed argument list, got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts1005_count <= 2,
        "Expected limited TS1005 cascade for malformed argument list, got {ts1005_count} diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_argument_list_colon_followed_by_var_keyword_emits_ts1135() {
    // Regression for `f(x: var ...)` parser recovery.
    //
    // tsc emits:
    //   - TS1005 ',' expected at the spurious `:`
    //   - TS1135 "Argument expression expected." at `var`
    //   - TS1134 "Variable declaration expected." at `(`
    // The keyword should also break the argument list so the outer statement
    // parser can keep recovering. This prevents earlier behaviour where the
    // colon branch tried to parse `var` as a type, followed by another TS1005
    // ',' expected at `(`.
    let source = "f(x: var (--a)\n);";

    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1135 = diagnostics.iter().filter(|d| d.code == 1135).count();
    let ts1134 = diagnostics.iter().filter(|d| d.code == 1134).count();
    let ts1110 = diagnostics.iter().filter(|d| d.code == 1110).count();

    assert!(
        ts1135 >= 1,
        "Expected TS1135 'Argument expression expected.' at `var`, got: {diagnostics:?}"
    );
    assert!(
        ts1134 >= 1,
        "Expected TS1134 'Variable declaration expected.' downstream of `var (`, got: {diagnostics:?}"
    );
    assert_eq!(
        ts1110, 0,
        "Expected no TS1110 'Type expected.' (the colon branch must not parse `var` as a type), got: {diagnostics:?}"
    );

    // Ensure we don't double-report `,` expected at `:` and at `(` of the
    // call site (the previous bug emitted both).
    let ts1005_at_paren = diagnostics
        .iter()
        .filter(|d| d.code == 1005)
        .filter(|d| {
            let pos = d.start as usize;
            pos < source.len() && &source[pos..=pos] == "("
        })
        .count();
    assert_eq!(
        ts1005_at_paren, 0,
        "TS1005 should not be emitted at `(` of `var (...)` after recovery, got: {diagnostics:?}"
    );
}

#[test]
fn test_parameter_list_stray_colon_recovers_through_object_binding_tail() {
    // Regression for `parametersSyntaxErrorNoCrash1.ts`. After the stray second
    // colon, tsc keeps parsing the following `{ return arg; }` as a malformed
    // object binding parameter, producing the full recovery tail.
    let source = "\n// https://github.com/microsoft/TypeScript/issues/59422\n\nfunction identity<T>(arg: T: T {\n    return arg;\n}";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32, String)> = diagnostics
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (d.code, pos.line + 1, pos.character + 1, d.message.clone())
        })
        .collect();
    fingerprints.sort();

    let mut expected = vec![
        (
            diagnostic_codes::EXPECTED,
            4,
            28,
            "',' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            4,
            32,
            "',' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            5,
            12,
            "':' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            5,
            15,
            "',' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            6,
            2,
            "')' expected.".to_string(),
        ),
    ];
    expected.sort();

    assert_eq!(
        fingerprints, expected,
        "parameter-list recovery fingerprints must match tsc, got: {diagnostics:?}"
    );
}

/// Collect `(code, line, column, message)` fingerprints for a source string.
fn parameter_diag_fingerprints(source: &str) -> Vec<(u32, u32, u32, String)> {
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.clone(),
            )
        })
        .collect()
}

#[test]
fn empty_parameter_slot_in_function_declaration_reports_ts1138() {
    // `(a, , b)` — the empty middle slot is `Parameter declaration expected.`
    // (TS1138) in tsc, not `Identifier expected.` (TS1003).
    let fingerprints = parameter_diag_fingerprints("function g(a, , b) {}");
    assert_eq!(
        fingerprints,
        vec![(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            15,
            "Parameter declaration expected.".to_string(),
        )],
        "empty parameter slot must report a single TS1138 at the stray comma"
    );
}

#[test]
fn empty_parameter_slot_does_not_depend_on_binder_names() {
    // The recovery is structural: renaming the binders must not change it.
    for (source, column) in [
        ("function g(alpha, , beta) {}", 19),
        ("function h(longName, , other) {}", 22),
    ] {
        let fingerprints = parameter_diag_fingerprints(source);
        assert_eq!(
            fingerprints,
            vec![(
                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
                1,
                column,
                "Parameter declaration expected.".to_string(),
            )],
            "TS1138 recovery must not depend on parameter names ({source})"
        );
    }
}

#[test]
fn empty_parameter_slot_in_object_method_reports_ts1138() {
    let fingerprints = parameter_diag_fingerprints("const o = { m(a, , b) {} };");
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            18,
            "Parameter declaration expected.".to_string(),
        )),
        "object-literal method empty slot must report TS1138, got {fingerprints:?}"
    );
    assert!(
        fingerprints
            .iter()
            .all(|(code, _, _, _)| *code != diagnostic_codes::IDENTIFIER_EXPECTED),
        "object-literal method empty slot must not fall back to TS1003, got {fingerprints:?}"
    );
}

#[test]
fn empty_parameter_slot_in_class_method_and_constructor_reports_ts1138() {
    let method = parameter_diag_fingerprints("class C { m(a, , b) {} }");
    assert!(
        method.contains(&(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            16,
            "Parameter declaration expected.".to_string(),
        )),
        "class method empty slot must report TS1138, got {method:?}"
    );

    let ctor = parameter_diag_fingerprints("class D { constructor(a, , b) {} }");
    assert!(
        ctor.contains(&(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            26,
            "Parameter declaration expected.".to_string(),
        )),
        "constructor empty slot must report TS1138, got {ctor:?}"
    );
}

#[test]
fn empty_parameter_slot_in_arrow_function_reports_only_ts1138() {
    // The arrow form previously mis-parsed into a parenthesized expression
    // (TS1109 + a TS1005 cascade). tsc recovers it in place as an arrow with a
    // single TS1138. A single TS1138 with no TS1109/TS1005 cascade is itself
    // proof the arrow was recognized: had it mis-parsed into a parenthesized
    // expression, the empty slot would surface as TS1109 `Expression expected.`
    let fingerprints = parameter_diag_fingerprints("const f = (a, , b) => a;");
    assert_eq!(
        fingerprints,
        vec![(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            15,
            "Parameter declaration expected.".to_string(),
        )],
        "arrow empty slot must report a single TS1138, got {fingerprints:?}"
    );
}

#[test]
fn empty_parameter_slot_in_generic_arrow_reports_ts1138() {
    let fingerprints = parameter_diag_fingerprints("const f = <T,>(a, , b) => a;");
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            19,
            "Parameter declaration expected.".to_string(),
        )),
        "generic arrow empty slot must report TS1138, got {fingerprints:?}"
    );
}

#[test]
fn leading_empty_parameter_slot_in_arrow_stays_parenthesized_expression() {
    // A LEADING empty slot is genuinely a parenthesized expression in both tsc
    // and tsz (TS1109 + TS1005). The recovery must NOT promote it to an arrow.
    let fingerprints = parameter_diag_fingerprints("const h = (, a) => a;");
    assert!(
        fingerprints
            .iter()
            .any(|(code, _, _, _)| *code == diagnostic_codes::EXPRESSION_EXPECTED),
        "leading empty slot must remain TS1109, got {fingerprints:?}"
    );
    assert!(
        fingerprints
            .iter()
            .all(|(code, _, _, _)| *code != diagnostic_codes::PARAMETER_DECLARATION_EXPECTED),
        "leading empty slot must NOT be recovered as an arrow parameter list, got {fingerprints:?}"
    );
}

#[test]
fn multiple_empty_parameter_slots_report_one_ts1138_each() {
    let fingerprints = parameter_diag_fingerprints("function f(a, , , b) {}");
    let ts1138: Vec<_> = fingerprints
        .iter()
        .filter(|(code, _, _, _)| *code == diagnostic_codes::PARAMETER_DECLARATION_EXPECTED)
        .collect();
    assert_eq!(
        ts1138.len(),
        2,
        "each empty slot reports its own TS1138, got {fingerprints:?}"
    );
}

#[test]
fn trailing_empty_parameter_slot_reports_ts1138() {
    let fingerprints = parameter_diag_fingerprints("function j(a, , ) {}");
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            1,
            15,
            "Parameter declaration expected.".to_string(),
        )),
        "trailing empty slot must report TS1138, got {fingerprints:?}"
    );
}

#[test]
fn well_formed_parameter_lists_remain_diagnostic_free() {
    // Guard against the recovery introducing false positives on valid lists.
    for source in [
        "function okfn(a, b) {}",
        "const ok = (a, b) => a;",
        "const okGeneric = <T,>(a: T, b: T) => a;",
        "class C { m(a, b) {} constructor(x, y) {} }",
        "function trailing(a, b,) {}",
    ] {
        let (parser, _root) = parse_source(source);
        assert!(
            parser.get_diagnostics().is_empty(),
            "well-formed parameter list should parse cleanly ({source}): {:?}",
            parser.get_diagnostics()
        );
    }
}
