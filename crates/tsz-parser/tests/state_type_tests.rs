//! Tests for type expression parsing in the parser.
use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{
    assert_no_errors, assert_span, assert_span_on, count_nodes, parse_source, parse_source_named,
};

#[test]
fn parse_complex_type_expressions_have_no_errors() {
    let (parser, _root) = parse_source(
        "type T = { [K in keyof O]: O[K] } & Partial<{ a: string; b: number }>;\ntype U<T> = T extends { a: infer V } ? V : never;",
    );
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_tuple_indexed_access_type() {
    let (parser, _root) = parse_source("type NoInfer<T> = [T][0];");
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_conditional_and_infer_types_emit_expected_members() {
    let (parser, _root) =
        parse_source("type T<T> = T extends string ? { kind: 's' } : { kind: 'o' };");
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_invalid_type_member_reports_diagnostics() {
    let (parser, _root) = parse_source("type T = <; ");
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_flow_style_type_parameter_bound_reports_comma_expected() {
    let source = "export default class B<T: BaseA> {}";
    let (parser, _root) = parse_source_named("test.js", source);
    let diagnostics = parser.get_diagnostics();
    let colon_pos = source.find(':').expect("expected colon") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|d| { d.code == 1005 && d.start == colon_pos && d.message == "',' expected." }),
        "Expected TS1005 comma diagnostic at Flow-style type parameter bound, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !(d.code == 1005 && d.start == colon_pos && d.message == "'>' expected.")),
        "Type parameter list recovery should not report a closing `>` at the same colon, got {diagnostics:?}"
    );
}

#[test]
fn parse_modifier_like_type_parameter_names_without_empty_name_recovery() {
    let source = "function f<private, protected, public, static>() {}";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().all(|d| d.code != 1139),
        "modifier-like type parameter names should not recover as empty type parameters: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2300),
        "modifier-like type parameter names should not synthesize duplicate empty names: {diagnostics:?}"
    );
}

#[test]
fn parse_template_literal_type_with_placeholder() {
    let (parser, _root) = parse_source("type T = `a${string}b`;");
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_template_literal_type_with_multiple_placeholders() {
    let (parser, _root) = parse_source(
        "type Timestamp = `${number}-${number}-${number}T${number}:${number}:${number}Z`;",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_template_literal_type_as_generic_argument_in_assertion() {
    let (parser, _root) = parse_source(
        "type Brand<T extends string> = { value: T };\nconst value = `close-${String(x)}` as Brand<`close-${string}`>;",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_template_literal_type_after_typeof_generic_argument_in_assertion() {
    let (parser, _root) = parse_source(
        "type Brand<T extends string> = { value: T };\ntype Result<T, U extends string> = { value: U };\ndeclare function fallback<T>(value: T): T;\nfunction f(input: { domain: 'signal' }, extra: unknown) {\n  return fallback({\n    value: `close-${String((extra as { value: string }).value)}` as Brand<`close-${string}`>,\n  } as Result<typeof input, `close-${string}`>);\n}",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_variance_annotations_on_interface_type_parameters() {
    let (parser, _root) = parse_source(
        "interface SolverDispatcher<in TInput, out TOutput> { run(input: TInput): TOutput; }",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_multiline_generic_arrow_returning_parenthesized_object() {
    let (parser, _root) = parse_source(
        "type Box<T> = { value: T };\nexport const make = <\n  T extends string,\n>(input: T): Box<typeof input> => ({\n  value: input,\n});",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_const_generic_arrow_with_template_literal_constraint() {
    let (parser, _root) = parse_source(
        "export const signalKindSet = <const TSignals extends readonly `signal:${string}`[]>(\n  values: NoInfer<TSignals>,\n): Readonly<{ readonly values: TSignals; readonly keys: string[] }> => ({\n  values,\n  keys: values.map((value) => value.replace('signal:', '')),\n});",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_template_expression_in_returned_object_literal() {
    let (parser, _root) = parse_source(
        "export const make = (input: { a: string; b: string }) => ({\n  route: `${input.a}:${input.b}`,\n});",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_typed_arrow_argument_in_conditional_true_branch() {
    let (parser, _root) = parse_source(
        "type Row = { x: number };\ndeclare const cond: boolean;\ndeclare const values: number[];\ndeclare function empty(): Row;\nconst rows = cond\n  ? values.map((value): Row => {\n    return { x: value };\n  })\n  : [empty()];",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_type_predicate_arrow_argument_in_conditional_true_branch() {
    let (parser, _root) = parse_source(
        "type Route = 'all' | 'one';\ndeclare const route: Route;\ndeclare const allRoutes: readonly Route[];\nconst routes = route === 'all'\n  ? allRoutes.filter((candidate): candidate is Exclude<Route, 'all'> => candidate !== 'all')\n  : [route];",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_arrow_parameters_after_conditional_type_parameter() {
    let (parser, _root) = parse_source(
        "export const withScopeAsync = async <TValue extends object, TResult>(\n  name: NoInfer<TValue> extends string ? string : string,\n  callback: (scope: SubscriptionScope) => Promise<TResult> | TResult,\n): Promise<TResult> => callback(undefined as any);",
    );
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn parse_keyof_infer_tuple_type_without_tail_is_tolerated() {
    let (parser, _root) = parse_source("type T = keyof infer X");
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn parse_mapped_type_with_keyof_retrieval_has_no_errors() {
    let (parser, _root) = parse_source(
        "type Wrapped<T> = { [K in keyof T]: T[K] };\ntype ReadonlyWrapped = Wrapped<{ a: string; b: number; }>;",
    );
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_call_signature_with_arrow_reports_colon_expected_not_property_signature_expected() {
    let (parser, _root) = parse_source("type T = { (n: number) => string; };");
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1005 && d.message == "':' expected."),
        "Expected TS1005 ':' expected for malformed call signature, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1131),
        "Malformed call signature should not fall back to TS1131, got {diagnostics:?}"
    );
}

#[test]
fn parse_construct_signature_with_arrow_reports_colon_expected_not_property_signature_expected() {
    let (parser, _root) = parse_source("type T = { new (n: number) => string; };");
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1005 && d.message == "':' expected."),
        "Expected TS1005 ':' expected for malformed construct signature, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1131),
        "Malformed construct signature should not fall back to TS1131, got {diagnostics:?}"
    );
}

// -----------------------------------------------------------------------------
// JSDoc-legacy type error recovery — the invariants pinned down here come from
// `tsc`.  When these patterns appear in a `.ts` file tsc emits TS8020 (and, for
// some variants, TS17019/TS17020) *and nothing else*: the error should not
// cascade into downstream diagnostics such as TS2702 ("used as a namespace"),
// TS7051 ("parameter has a name but no type"), TS2300 ("duplicate identifier"),
// or spurious TS2554 arity mismatches at call sites.
// -----------------------------------------------------------------------------

#[test]
fn jsdoc_dot_generic_type_reference_does_not_cascade_into_qualified_name() {
    // `Array.<number>` is JSDoc syntax for `Array<number>`.  tsc emits a single
    // TS8020 at the `.` and then treats the reference as the generic form.
    let source = "var a: Array.<number> = [1, 2, 3];";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let dot_pos = source.find('.').expect("expected `.`") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 8020 && d.start == dot_pos && d.length == 1),
        "Expected TS8020 anchored at the `.`, got {diagnostics:?}"
    );
    // No other diagnostics should be emitted — the JSDoc `.<T>` pattern must
    // collapse into a regular generic reference rather than a namespace access.
    let others: Vec<_> = diagnostics.iter().filter(|d| d.code != 8020).collect();
    assert!(
        others.is_empty(),
        "Array.<number> should produce only TS8020, got additional {others:?}"
    );
}

#[test]
fn jsdoc_legacy_function_type_with_bare_types_does_not_cascade() {
    // TS 7.0.2 dropped the JSDoc-legacy `function(...)` type recovery:
    // `function` parses as a plain type-reference identifier and the `(`
    // re-enters declarator recovery. tsc reports exactly `','` expected at
    // the `(` and `';'` expected at the second `:` — no TS8020.
    let source = "var g: function(number, number): number = (n, m) => n + m;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1005, 1005],
        "expected tsc 7.0.2's two TS1005 recoveries, got {diagnostics:?}"
    );
}

#[test]
fn jsdoc_legacy_function_type_with_this_binding_preserves_it() {
    // TS 7.0.2: after the TS1005 at `(`, the `(this: number, string)` tail
    // speculates as arrow-function parameters (`this` is a valid parameter
    // name), so recovery ends with `'=>' expected` — never TS8020.
    let source = "var f: function(this: number, string): string;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1005, 1005],
        "expected tsc 7.0.2's `','`/`'=>'` TS1005 pair, got {diagnostics:?}"
    );
}

#[test]
fn jsdoc_legacy_function_type_with_new_marker_is_parsed_as_constructor() {
    // TS 7.0.2: `new` is a reserved word, so `(new: number, string)` cannot
    // be arrow parameters — the parens reparse as a parenthesized expression
    // whose `new` lacks an operand: `','` expected, `Expression expected`,
    // then `';'` expected. Never TS8020, never a constructor type.
    let source = "var c: function(new: number, string);";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1005, 1109, 1005],
        "expected tsc 7.0.2's paren-expression recovery, got {diagnostics:?}"
    );
}

#[test]
fn jsdoc_legacy_constructor_function_suffix_does_not_cascade() {
    // TS 7.0.2 parses `function` as a type-reference identifier and the
    // reserved `new` rejects the arrow speculation: `','` expected at `(`
    // and `Expression expected` at the `:` after `new`. tsc additionally
    // recovers the tail as garbage statements (TS1434/TS1128); tsz's
    // remaining tail recovery differs there (known gap) — this pin holds the
    // prefix and the absence of the removed TS8020 path.
    let source = "var c: function(new: number): string;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.starts_with(&[1005, 1109]),
        "expected the TS1005 + TS1109 recovery prefix, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(&8020),
        "the removed TS8020 JSDoc-legacy recovery must not fire, got {diagnostics:?}"
    );
}

/// Regression: a `TYPE_PREDICATE` node's source span must end at its
/// inner type, not at whatever token follows.  The parser used
/// `token_end()` after consuming the inner type, which reflected the
/// *next* token (e.g. `=>` in `(x): x is string => …`), so an
/// emit-side source-slice helper picked up `x is string =>` and
/// re-emitted it as `… => x is string =>;` in d.ts.
#[test]
fn parse_type_predicate_does_not_overshoot_into_following_arrow() {
    let source = "const f = (x: any): x is string => true;";
    let (parser, _root) = parse_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::TYPE_PREDICATE,
        "x is string",
    );
}

// --- composite type node span tests ---
// Each test verifies that the composite type node's end does NOT overshoot into
// the following token (e.g. `;`). Names are varied to prove the fix is structural.

#[test]
fn union_type_span_excludes_trailing_semicolon() {
    assert_span("type A = X | Y;", syntax_kind_ext::UNION_TYPE, "X | Y");
}

#[test]
fn union_type_span_varies_names() {
    assert_span(
        "type A = Foo | Bar;",
        syntax_kind_ext::UNION_TYPE,
        "Foo | Bar",
    );
}

#[test]
fn union_type_span_three_members() {
    assert_span(
        "type A = P | Q | R;",
        syntax_kind_ext::UNION_TYPE,
        "P | Q | R",
    );
}

#[test]
fn intersection_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = X & Y;",
        syntax_kind_ext::INTERSECTION_TYPE,
        "X & Y",
    );
}

#[test]
fn intersection_type_span_varies_names() {
    assert_span(
        "type A = Alpha & Beta;",
        syntax_kind_ext::INTERSECTION_TYPE,
        "Alpha & Beta",
    );
}

#[test]
fn array_type_span_excludes_trailing_semicolon() {
    assert_span("type A = X[];", syntax_kind_ext::ARRAY_TYPE, "X[]");
}

#[test]
fn array_type_span_varies_names() {
    assert_span(
        "type A = MyItem[];",
        syntax_kind_ext::ARRAY_TYPE,
        "MyItem[]",
    );
}

#[test]
fn indexed_access_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = T[K];",
        syntax_kind_ext::INDEXED_ACCESS_TYPE,
        "T[K]",
    );
}

#[test]
fn indexed_access_type_span_varies_names() {
    assert_span(
        "type A = Obj[Prop];",
        syntax_kind_ext::INDEXED_ACCESS_TYPE,
        "Obj[Prop]",
    );
}

#[test]
fn function_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = () => X;",
        syntax_kind_ext::FUNCTION_TYPE,
        "() => X",
    );
}

#[test]
fn function_type_span_varies_return_type_name() {
    assert_span(
        "type A = () => MyResult;",
        syntax_kind_ext::FUNCTION_TYPE,
        "() => MyResult",
    );
}

#[test]
fn generic_function_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = <T>(x: T) => T;",
        syntax_kind_ext::FUNCTION_TYPE,
        "<T>(x: T) => T",
    );
}

#[test]
fn constructor_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = new () => X;",
        syntax_kind_ext::CONSTRUCTOR_TYPE,
        "new () => X",
    );
}

#[test]
fn constructor_type_span_varies_names() {
    assert_span(
        "type A = new (arg: Param) => Instance;",
        syntax_kind_ext::CONSTRUCTOR_TYPE,
        "new (arg: Param) => Instance",
    );
}

#[test]
fn conditional_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = X extends Y ? P : Q;",
        syntax_kind_ext::CONDITIONAL_TYPE,
        "X extends Y ? P : Q",
    );
}

#[test]
fn conditional_type_span_varies_names() {
    assert_span(
        "type A = Input extends Base ? TrueResult : FalseResult;",
        syntax_kind_ext::CONDITIONAL_TYPE,
        "Input extends Base ? TrueResult : FalseResult",
    );
}

#[test]
fn tuple_type_span_excludes_trailing_semicolon() {
    assert_span("type A = [X, Y];", syntax_kind_ext::TUPLE_TYPE, "[X, Y]");
}

#[test]
fn tuple_type_span_varies_names() {
    assert_span(
        "type A = [First, Second, Third];",
        syntax_kind_ext::TUPLE_TYPE,
        "[First, Second, Third]",
    );
}

#[test]
fn parenthesized_type_span_excludes_trailing_semicolon() {
    assert_span("type A = (X);", syntax_kind_ext::PARENTHESIZED_TYPE, "(X)");
}

#[test]
fn parenthesized_type_span_varies_names() {
    assert_span(
        "type A = (MyWrapped);",
        syntax_kind_ext::PARENTHESIZED_TYPE,
        "(MyWrapped)",
    );
}

#[test]
fn literal_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = \"literal\";",
        syntax_kind_ext::LITERAL_TYPE,
        "\"literal\"",
    );
}

#[test]
fn prefix_literal_type_span_excludes_trailing_semicolon() {
    assert_span("type A = -42;", syntax_kind_ext::LITERAL_TYPE, "-42");
}

#[test]
fn template_literal_type_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = `prefix${T}suffix`;",
        syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
        "`prefix${T}suffix`",
    );
}

#[test]
fn template_literal_no_substitution_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = `plain`;",
        syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
        "`plain`",
    );
}

#[test]
fn keyof_type_operator_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = keyof T;",
        syntax_kind_ext::TYPE_OPERATOR,
        "keyof T",
    );
}

#[test]
fn unique_type_operator_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = unique symbol;",
        syntax_kind_ext::TYPE_OPERATOR,
        "unique symbol",
    );
}

#[test]
fn readonly_type_operator_span_excludes_trailing_semicolon() {
    assert_span(
        "type A = readonly T[];",
        syntax_kind_ext::TYPE_OPERATOR,
        "readonly T[]",
    );
}

#[test]
fn infer_type_span_excludes_trailing_token() {
    // infer only appears inside conditional types
    assert_span(
        "type A = X extends infer U ? U : never;",
        syntax_kind_ext::INFER_TYPE,
        "infer U",
    );
}

#[test]
fn infer_type_span_varies_names() {
    assert_span(
        "type A = X extends infer Captured ? Captured : never;",
        syntax_kind_ext::INFER_TYPE,
        "infer Captured",
    );
}

#[test]
fn rest_tuple_element_span_excludes_trailing_bracket() {
    // Unlabeled rest element: [...T] — REST_TYPE must end after T, not after ]
    assert_span("type A = [...T];", syntax_kind_ext::REST_TYPE, "...T");
}

#[test]
fn rest_tuple_element_span_varies_names() {
    assert_span(
        "type A = [...Items];",
        syntax_kind_ext::REST_TYPE,
        "...Items",
    );
}

#[test]
fn optional_tuple_element_span_excludes_trailing_bracket() {
    assert_span("type A = [T?];", syntax_kind_ext::OPTIONAL_TYPE, "T?");
}

#[test]
fn named_tuple_member_span_excludes_trailing_bracket() {
    assert_span(
        "type A = [label: T];",
        syntax_kind_ext::NAMED_TUPLE_MEMBER,
        "label: T",
    );
}

#[test]
fn named_tuple_optional_type_span_excludes_trailing_bracket() {
    assert_span(
        "type A = [label: T?];",
        syntax_kind_ext::OPTIONAL_TYPE,
        "T?",
    );
}

#[test]
fn mapped_type_member_span_excludes_trailing_brace() {
    assert_span(
        "interface I { [K in Keys]: Value; }",
        syntax_kind_ext::MAPPED_TYPE,
        "[K in Keys]: Value;",
    );
}

#[test]
fn infer_type_parameter_span_excludes_trailing_question() {
    // The inner TYPE_PARAMETER node for `infer U` must end before `?`, not include it
    assert_span(
        "type A = X extends infer U ? U : never;",
        syntax_kind_ext::TYPE_PARAMETER,
        "U",
    );
}

#[test]
fn infer_type_parameter_span_varies_names() {
    assert_span(
        "type A = X extends infer Result ? Result : never;",
        syntax_kind_ext::TYPE_PARAMETER,
        "Result",
    );
}

/// A bare (unparenthesized) `infer X` tuple element must not absorb a trailing
/// `?` optional marker. tsc parses the `infer` type directly and bypasses
/// postfix parsing, so the `?` is read as a missing `,` (TS1005) followed by a
/// missing element type (TS1110). Only `(infer X)?` is a valid optional element.
fn assert_bare_infer_postfix_rejected(source: &str, marker: char) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let marker_pos = source.find(marker).expect("expected postfix marker") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1005 && d.start == marker_pos && d.message == "',' expected."),
        "expected TS1005 ',' expected at the stray `{marker}` for `{source}`, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1110 && d.start == marker_pos + 1),
        "expected TS1110 'Type expected.' after the stray `{marker}` for `{source}`, got {diagnostics:?}"
    );
}

#[test]
fn bare_infer_optional_tuple_element_rejected() {
    assert_bare_infer_postfix_rejected("type A<T> = T extends [infer X?] ? X : never;", '?');
}

#[test]
fn bare_infer_optional_tuple_element_rejected_renamed_var() {
    // The rule is structural: it must fire regardless of the inferred name.
    assert_bare_infer_postfix_rejected(
        "type A<T> = T extends [infer Result?] ? Result : never;",
        '?',
    );
}

#[test]
fn bare_infer_optional_second_tuple_element_rejected() {
    assert_bare_infer_postfix_rejected(
        "type A<T> = T extends [infer A, infer B?] ? A : never;",
        '?',
    );
}

#[test]
fn bare_infer_nonnull_tuple_element_rejected() {
    // The `!` postfix marker is bypassed for bare `infer` exactly like `?`.
    assert_bare_infer_postfix_rejected("type A<T> = T extends [infer X!] ? X : never;", '!');
}

#[test]
fn bare_infer_optional_rest_tuple_element_rejected() {
    // `...infer R` is still a bare `infer` type, so `[...infer R?]` is the same
    // stray-marker error, not a rest-optional marker.
    assert_bare_infer_postfix_rejected("type A<T> = T extends [...infer R?] ? R : never;", '?');
}

#[test]
fn bare_infer_optional_named_tuple_member_rejected() {
    // A labeled member type `x: infer X` is still a bare `infer`, so `[x: infer X?]`
    // is the same stray-marker error.
    assert_bare_infer_postfix_rejected("type A<T> = T extends [x: infer X?] ? X : never;", '?');
}

#[test]
fn bare_infer_named_tuple_member_without_marker_accepted() {
    // Control: `[x: infer X]` (no stray marker) must keep parsing cleanly.
    let (parser, _root) = parse_source("type A<T> = T extends [x: infer X] ? X : never;");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn bare_infer_optional_marker_before_comma_reports_only_missing_comma() {
    // When another element follows the stray `?`, tsc reports only the missing
    // `,` (TS1005) and continues — no spurious `Type expected` (TS1110).
    let source = "type A<T> = T extends [infer X?, number] ? X : never;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let marker_pos = source.find('?').expect("expected `?`") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1005 && d.start == marker_pos && d.message == "',' expected."),
        "expected TS1005 at the stray `?`, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1110),
        "a following element must suppress TS1110, got {diagnostics:?}"
    );
}

#[test]
fn parenthesized_infer_optional_tuple_element_accepted() {
    // `(infer X)?` is the valid optional form and must parse without errors.
    let (parser, _root) = parse_source("type A<T> = T extends [(infer X)?] ? X : never;");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn non_infer_optional_tuple_element_still_accepted() {
    // Control: an ordinary optional element must keep working.
    let (parser, _root) = parse_source("type A<T> = T extends [string, number?] ? string : never;");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn bare_infer_in_conditional_still_parses() {
    // Control: the `?` of an enclosing conditional type must not be mistaken for
    // a postfix marker on the `infer` type.
    let (parser, _root) = parse_source("type A<T> = T extends infer X ? X : never;");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "expected no diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn infer_with_constraint_postfix_attaches_to_constraint() {
    // `[infer A extends string?]`: the `?` is the constraint's nullable marker
    // (TS17019), not a tuple-level optional — mirroring tsc, which parses the
    // constraint as a full type.
    let source = "type A<T> = T extends [infer A extends string?] ? A : never;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.code == 17019),
        "expected TS17019 on the constraint's postfix `?` for `{source}`, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1005 && d.code != 1110),
        "constraint postfix must not be reported as a missing comma/type, got {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Mapped type `as` clause with template literal types — bracket-scanning
// regression (issue #10936 / parser-5-20).
//
// When a mapped type entry uses a template literal in its `as` clause, e.g.
// `[K in T as K extends \`${P}:${N}\` ? N : never]: U`, the raw scanner that
// `look_ahead_is_computed_type_member_boundary` used internally would fail to
// re-enter template-continuation mode after the first `}` substitution close,
// misidentifying the `[` as an array/indexed-access suffix rather than a type
// member boundary.  The tests below pin every structural shape that was broken.
// ---------------------------------------------------------------------------

/// Single-member type literal whose only member is a mapped entry with a
/// template literal in the `as` clause — minimal reproducer.
#[test]
fn mapped_type_as_template_literal_single_member_no_errors() {
    assert_no_errors(
        "type T<O> = {\n  [K in keyof O as K extends `${infer P}:${infer N}` ? N : never]: O[K]\n};",
    );
}

/// A regular property followed by a mapped entry with a template-literal `as`
/// clause.  This is the exact shape that triggered the cascade error: the
/// bracket-scanning loop would misidentify the second line's `[` as an
/// array-access suffix of the property value type.
#[test]
fn mapped_type_as_template_literal_after_regular_member_no_errors() {
    assert_no_errors(
        "type T<O> = {\n  extra: string\n  [K in keyof O as K extends `${infer P}:${infer N}` ? N : never]: O[K]\n};",
    );
}

/// Same shape as above but with differently-spelled type-parameter names to
/// confirm the fix is structural and not hardcoded to specific identifier names.
#[test]
fn mapped_type_as_template_literal_renamed_params_no_errors() {
    assert_no_errors(
        "type T<O> = {\n  extra: string\n  [Key in keyof O as Key extends `${infer Prefix}:${infer Name}` ? Name : never]: O[Key]\n};",
    );
}

/// Multiple regular members surrounding a mapped entry with a template-literal
/// `as` clause.
#[test]
fn mapped_type_as_template_literal_surrounded_by_regular_members_no_errors() {
    assert_no_errors(
        "type T<O> = {\n  before: string\n  [K in keyof O as K extends `${infer A}:${infer B}` ? B : never]: O[K]\n  after: number\n};",
    );
}

/// Mapped type `as` clause with a template literal that has more than two
/// interpolation segments.
#[test]
fn mapped_type_as_template_literal_three_segments_no_errors() {
    assert_no_errors(
        "type T<O> = {\n  [K in keyof O as K extends `${infer A}:${infer B}:${infer C}` ? C : never]: O[K]\n};",
    );
}

/// A simple mapped type (no template literal) still parses correctly after the
/// early-return guard — verifies the guard does not break the non-template case.
#[test]
fn mapped_type_plain_as_clause_after_regular_member_no_errors() {
    assert_no_errors(
        "type T<O> = {\n  extra: string\n  [K in keyof O as K extends string ? K : never]: O[K]\n};",
    );
}

/// The same bracket-scanning fix applies inside an `interface` body.  Interfaces
/// cannot legally hold mapped type entries (the checker emits `TS7061`), but the
/// parser must still recognise `[K in …]` on a new line as a type member
/// boundary, not an array-access suffix of the preceding property type.
#[test]
fn mapped_type_as_template_literal_in_interface_body_no_parser_errors() {
    assert_no_errors(
        "interface I {\n  extra: string\n  [K in keyof I as K extends `${infer P}:${infer N}` ? N : never]: I[K]\n}",
    );
}

// ---------------------------------------------------------------------------
// Template-literal constrained extends branches — issue #10925
//
// The underlying rule: any syntactically valid TypeScript type expression that
// uses template literal types in conditional `extends` branches or as type
// parameter constraints must parse without cascading errors. These tests pin
// the structural shapes that appear frequently in ts-essentials and similar
// utility-type libraries.
// ---------------------------------------------------------------------------

/// Template literal as the extends arm of a conditional type (minimal).
#[test]
fn template_literal_extends_branch_minimal() {
    assert_no_errors("type T<U> = U extends `${infer A}` ? A : never;");
}

/// Template literal as a type parameter constraint.
#[test]
fn template_literal_type_parameter_constraint() {
    assert_no_errors("type T<U extends `${string}_${string}`> = U;");
}

/// Template literal as a constraint with infer in the extends branch (CamelCase-like).
#[test]
fn template_literal_extends_infer_recursive() {
    assert_no_errors(
        "type CamelCase<S extends string> = S extends `${infer P}_${infer Q}${infer R}` \
         ? `${S}${S}${CamelCase<S>}` : S;",
    );
}

/// Template literal with separator in conditional (Split-like pattern).
#[test]
fn template_literal_extends_separator_infer() {
    assert_no_errors(
        "type Split<S extends string, D extends string> = \
         S extends `${infer T}${D}${infer U}` ? [T, ...Split<U, D>] : [S];",
    );
}

/// `IsStringLiteral` pattern: template literal in the false branch of a chain.
#[test]
fn template_literal_extends_is_string_literal_pattern() {
    assert_no_errors(
        "type IsStringLiteral<T> = string extends T ? false : T extends `${infer _}` ? true : false;",
    );
}

/// Template literal in the false branch of a conditional type.
#[test]
fn template_literal_false_branch_construction() {
    assert_no_errors(
        "type T<S extends string> = S extends `prefix_${string}` ? S : `prefix_${S}`;",
    );
}

/// Complex chained conditional with multiple template literal extends arms.
#[test]
fn template_literal_extends_chained_dotted_path() {
    assert_no_errors(
        "type X<T extends string> = \
         T extends `${infer A}.${infer B}.${infer C}` ? [A, B, C] : \
         T extends `${infer A}.${infer B}` ? [A, B] : [T];",
    );
}

/// Template literal extends with differently-named infer variables (structural, not spelling).
#[test]
fn template_literal_extends_renamed_infer_vars() {
    assert_no_errors(
        "type RouteOf<T extends string> = T extends `${infer Method} ${infer Path}` ? Path : never;\n\
         type MethodOf<T extends string> = T extends `${infer M} ${infer P}` ? M : never;",
    );
}

/// Arrow function with template literal type parameter constraint.
#[test]
fn template_literal_constraint_arrow_function() {
    assert_no_errors("const f = <T extends `signal:${string}`>(x: T): T => x;");
}

/// Deeply nested template literal with multiple constraints.
#[test]
fn template_literal_deeply_nested_extends() {
    assert_no_errors(
        "type T<A extends string, B extends string> = \
         A extends `${infer X}_${B}` ? X extends `${infer Y}_${string}` ? Y : X : A;",
    );
}

/// Template literal key remapping combined with template literal value construction.
#[test]
fn template_literal_as_clause_and_value_template() {
    assert_no_errors(
        "type R<T> = { [K in keyof T as K extends `${infer P}:${infer N}` ? N : never]: \
         K extends `${infer P}:${infer N}` ? N : K };",
    );
}

/// Template literal in a function generic with readonly array constraint.
#[test]
fn template_literal_constraint_readonly_array_generic() {
    assert_no_errors(
        "declare function f<const T extends readonly `${string}:${string}`[]>(x: T): T;",
    );
}

/// Template literal in the extends check position combined with union.
#[test]
fn template_literal_extends_union_check() {
    assert_no_errors(
        "type T<S extends string> = (S extends `${string}_${string}` ? true : false) | boolean;",
    );
}

/// Template literal intersection constraint.
#[test]
fn template_literal_intersection_constraint() {
    assert_no_errors("type T<U extends `${string}` & `${number}`> = U;");
}

/// Multiple template literal types in the same declaration file (token-boundary isolation).
#[test]
fn template_literal_multiple_declarations_token_isolation() {
    assert_no_errors(
        "type A<T extends string> = T extends `${infer X}_${string}` ? X : T;\n\
         type B<T extends string> = T extends `${string}_${infer Y}` ? Y : T;\n\
         type C<T extends string> = A<T> extends B<T> ? true : false;",
    );
}

#[test]
fn test_repro_issue_10937_flatten_rows() {
    let source = r#"
type FlattenRows<T extends readonly unknown[]> =
  T extends readonly [infer Head, ...infer Tail]
    ? Head extends readonly unknown[]
      ? [...FlattenRows<Head>, ...FlattenRows<Tail>]
      : [Head, ...FlattenRows<Tail>]
    : [];

type RemapTuple<T extends readonly unknown[]> = {
  [K in keyof T]: T[K] extends readonly unknown[] ? FlattenRows<T[K]> : T[K]
};

type Row6 = RemapTuple<[["utility-types-project"], [1, 2], []]>;
"#;
    assert_no_errors(source);
}

#[test]
fn test_issue_10937_multiple_rest_spreads_no_diagnostics() {
    // Multiple rest spread elements in nested recursive conditional tuple types
    let cases = [
        // Multiple rest elements in a tuple
        "type T = [...A, ...B];",
        // Rest with generic type refs
        "type T<A, B> = [...A, ...B];",
        // Recursive with infer rest
        "type T<U> = U extends readonly [infer Head, ...infer Tail] ? Tail : never;",
        // Nested recursive flatten shape
        "type Flatten<T extends readonly unknown[]> = T extends readonly [infer H, ...infer Tail] ? [H, ...Flatten<Tail>] : [];",
        // Mapped tuple over keyof
        "type Remap<T extends readonly unknown[]> = { [K in keyof T]: T[K] };",
        // Combined mapped + conditional + recursive spread
        "type Remap<T extends readonly unknown[]> = { [K in keyof T]: T[K] extends readonly unknown[] ? T[K] : never };",
    ];
    for source in cases {
        assert_no_errors(source);
    }
}

#[test]
fn test_issue_10937_rest_span_stability_in_recursive_context() {
    // REST_TYPE nodes in nested positions must have stable spans
    assert_span("type T = [...A, ...B];", syntax_kind_ext::REST_TYPE, "...A");
    assert_span(
        "type T<U> = U extends readonly [infer Head, ...infer Tail] ? Tail : never;",
        syntax_kind_ext::REST_TYPE,
        "...infer Tail",
    );
}

#[test]
fn test_issue_10937_nested_generic_in_rest_tuple_element() {
    // The rest spread element contains a generic type - check spans are stable
    let source = "type T<A, B> = [...FlattenRows<A>, ...FlattenRows<B>];";
    let (parser, _root) = parse_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no diagnostics for nested generics in rest tuple: {:?}",
        parser.get_diagnostics()
    );
    // Reuse the already-parsed state — avoids a second parse inside assert_span.
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::REST_TYPE,
        "...FlattenRows<A>",
    );
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::REST_TYPE,
        "...FlattenRows<B>",
    );
}

#[test]
fn test_issue_10937_infer_constraint_in_rest_spread() {
    // These forms must parse without diagnostics.
    let no_error_cases = [
        "type T = [...string[], ...number[]];",
        "type Remap<T extends readonly unknown[]> = { [K in keyof T]: T[K] extends readonly unknown[] ? T[K] : never };",
        "type T<U> = U extends readonly [infer H, ...infer R] ? H : never;",
    ];
    for source in no_error_cases {
        assert_no_errors(source);
    }
    // infer with `extends` constraint in rest position (TS 4.7+ variadic infer constraint)
    assert_no_errors(
        "type T<U> = U extends readonly [infer H extends string, ...infer R extends number[]] ? H : never;",
    );
}

#[test]
fn test_issue_10937_span_stability_multiple_rest_in_nested_conditional() {
    // Verifies REST_TYPE spans are correct in a deeply nested recursive context
    let source = r#"
type FlattenRows<T extends readonly unknown[]> =
  T extends readonly [infer Head, ...infer Tail]
    ? Head extends readonly unknown[]
      ? [...FlattenRows<Head>, ...FlattenRows<Tail>]
      : [Head, ...FlattenRows<Tail>]
    : [];
"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "{diags:?}");

    // Every REST_TYPE node must start with `...` and not overshoot into trailing
    // whitespace — a two-sided bound on span correctness for all nodes at once.
    let arena = parser.get_arena();
    for node in &arena.nodes {
        if node.kind == syntax_kind_ext::REST_TYPE {
            let node_text = &source[node.pos as usize..node.end as usize];
            assert!(
                node_text.starts_with("..."),
                "REST_TYPE node at [{},{}] should start with '...', got {:?}",
                node.pos,
                node.end,
                node_text
            );
            assert!(
                !node_text.ends_with(|c: char| c.is_whitespace()),
                "REST_TYPE node at [{},{}] span overshoots into whitespace, got {:?}",
                node.pos,
                node.end,
                node_text
            );
        }
    }
}

#[test]
fn test_issue_10937_infer_extends_constraint_inside_tuple_in_extends_clause() {
    // This is the tricky case: infer T extends U inside a tuple that is itself
    // inside the extends clause of a conditional type. The DISALLOW_CONDITIONAL_TYPES
    // flag is active for the outer extends, cleared for the tuple element, and the
    // infer constraint handler must correctly back-track when it sees `?`.
    let cases = [
        // infer with constraint, spread form
        "type T<U> = U extends [infer H extends string] ? H : never;",
        "type T<U> = U extends [infer H extends string, infer T extends number] ? H : never;",
        "type T<U> = U extends [...infer H extends string[]] ? H : never;",
        // infer without constraint in extends position
        "type T<U> = U extends [infer H, ...infer R] ? H : never;",
        // nested infer constraint disambiguation: `?` after constraint belongs to outer conditional
        "type T<U> = U extends infer X extends string ? X : never;",
        // Verify these don't confuse the ? as infer constraint vs outer conditional
        "type T<U> = U extends readonly [infer Head, ...infer Tail] ? Tail : never;",
    ];
    for source in cases {
        assert_no_errors(source);
    }
}

#[test]
fn test_issue_10937_tuple_recovery_does_not_emit_false_comma_expected_in_spread_with_conditional() {
    // Spread elements that contain complex types (conditionals, mapped) inside tuples
    // must not trigger recovery (false ',' expected) when they're valid
    let valid_cases = [
        // Rest element with a complex conditional type
        "type T<U> = [...(U extends string ? string[] : number[])];",
        // Nested tuple spreads in a conditional
        "type T<U> = U extends any ? [...U[]] : never;",
        // Mapped type inside tuple element (edge case)
        "type T<U> = [{ [K in keyof U]: U[K] }];",
    ];
    for source in valid_cases {
        assert_no_errors(source);
    }
}

#[test]
fn rest_type_span_excludes_trailing_whitespace() {
    // REST_TYPE end uses `token_full_start()` of the next token, which is the
    // full-start position (before leading trivia). With a space before `,` the
    // span must still stop right after `T`, not include the space.
    assert_span("type A = [...T , U];", syntax_kind_ext::REST_TYPE, "...T");
}

#[test]
fn test_issue_10937_three_rest_elements_in_tuple() {
    // Three rest elements at parse level. TypeScript 4.2+ allows rest in
    // non-final positions; the parser must accept these without diagnostics
    // regardless of later semantic checks.
    let cases = [
        "type T = [...A, ...B, ...C];",
        "type T<A, B, C> = [...A, ...B, ...C];",
        "type T = [...string[], ...number[], ...boolean[]];",
        "type T = [...A, B, ...C];",
    ];
    for source in cases {
        assert_no_errors(source);
    }
    // Span check: first and last rest elements in a three-spread tuple.
    assert_span(
        "type T = [...A, ...B, ...C];",
        syntax_kind_ext::REST_TYPE,
        "...A",
    );
}

/// Regression: a conditional type used as a type argument (or inside an object-type
/// member / tuple element) must parse even when the enclosing type reference sits in
/// the `extends` position of an outer conditional type. The `extends`-position block on
/// top-level conditionals is one level deep in tsc (`noConditionalTypes`), so it must
/// not leak past a `<...>`, `{...}`, or `[...]` boundary. Previously tsz threaded this as
/// a persistent context flag that leaked into type arguments and type-literal members,
/// producing spurious `TS1005` diagnostics (e.g. `'>' expected.`).
#[test]
fn parse_conditional_type_in_delimited_context_within_extends_position() {
    let cases: &[&str] = &[
        "type R1<T> = T extends Foo<A extends B ? C : D> ? 1 : 2;",
        "type R2<Src> = Src extends Container<Lhs extends Rhs ? Yes : No> ? 1 : 2;",
        "type R3<T> = T extends Wrap<P extends Q ? X : Y, M extends N ? U : V> ? 1 : 2;",
        "type R4<T> = T extends Outer<Inner<P extends Q ? X : Y>> ? 1 : 2;",
        "type R5<T> = T extends Box<P extends Q ? R extends S ? X : Y : Z> ? 1 : 2;",
        "type R6<T> = T extends Box<P extends infer U ? U : never> ? 1 : 2;",
        "type R7<T> = T extends { value: P extends Q ? X : Y } ? 1 : 2;",
        "type R8<T> = T extends [P extends Q ? X : Y] ? 1 : 2;",
        "type R9<T> = T extends Box<{ [K in keyof Src]: Src[K] } & (Src extends Q ? X : Y)> ? 1 : 2;",
        "type R10<T> = T extends { [K in keyof T]: Box<T[K] extends object ? 1 : 0> } ? 1 : 2;",
    ];
    for src in cases {
        let (parser, _root) = parse_source(src);
        assert!(
            parser.get_diagnostics().is_empty(),
            "expected no parse diagnostics for {src:?}, got {:?}",
            parser.get_diagnostics()
        );
    }
}

/// Structural lock: the inner `A extends B ? C : D` must materialize as a real
/// `CONDITIONAL_TYPE` node nested inside the type argument, not be truncated to its
/// check type with the `extends` left dangling.
#[test]
fn conditional_type_argument_in_extends_position_builds_nested_conditional_node() {
    assert_span(
        "type R<T> = T extends Foo<A extends B ? C : D> ? 1 : 2;",
        syntax_kind_ext::CONDITIONAL_TYPE,
        "A extends B ? C : D",
    );
}

/// Regression guard: clearing the conditional block for delimited contexts must not
/// disturb the `infer U extends X ? ...` disambiguation. In an `extends` position the
/// `extends number` is the infer constraint and the `? 1 : 0` belongs to the outer
/// conditional, so the whole type still parses cleanly.
#[test]
fn infer_constraint_disambiguation_unaffected_by_delimited_context_fix() {
    assert_no_errors("type R<T> = T extends infer U extends number ? 1 : 0;");
    assert_no_errors("type R<T> = T extends infer U extends Foo<A extends B ? C : D> ? U : never;");
}

// --- Missing-comma recovery between type parameters / type arguments (#14835) ---
//
// When two type-list elements are separated by whitespace instead of a comma,
// tsc's `parseDelimitedList` reports a single TS1005 `','` expected (anchored at
// the offending element) and keeps parsing the list. tsz previously bailed with
// TS1005 `'>'` expected and cascaded into spurious downstream diagnostics. These
// guards cover every callable/declaration form that takes type parameters, plus
// type-argument lists in type position.

/// Offset tsc anchors the missing `','` at: the second element in `needle`
/// (`"<first> <second>"`), located inside `source`.
fn gap_offset(source: &str, needle: &str) -> u32 {
    let base = source
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in `{source}`"));
    let space = needle.find(' ').expect("needle must contain a space");
    (base + space + 1) as u32
}

/// Recovery must never fall back to `'>' expected` (the pre-fix behavior).
fn assert_no_close_gt(parser: &crate::parser::ParserState, source: &str) {
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .all(|d| d.message != "'>' expected."),
        "[{source}] recovery must not report a closing `'>'`, got {:?}",
        parser.get_diagnostics()
    );
}

/// A whitespace-separated type list recovers like tsc: one TS1005 `','` at the
/// gap, no `'>' expected`, and none of the historical cascade codes.
fn assert_missing_comma_recovery(file: &str, source: &str, gap: &str) {
    let (parser, _root) = parse_source_named(file, source);
    let diags = parser.get_diagnostics();
    let comma_at = gap_offset(source, gap);

    assert!(
        diags
            .iter()
            .any(|d| d.code == 1005 && d.start == comma_at && d.message == "',' expected."),
        "[{source}] expected TS1005 `','` at offset {comma_at}, got {diags:?}"
    );
    assert_no_close_gt(&parser, source);
    // Historical cascade codes: TS1068 (class), TS1131 (interface), TS1134 (arrow).
    for cascade in [1068u32, 1131, 1134] {
        assert!(
            diags.iter().all(|d| d.code != cascade),
            "[{source}] unexpected cascade TS{cascade}, got {diags:?}"
        );
    }
}

#[test]
fn missing_comma_between_type_parameters_recovers_across_all_forms() {
    // Every callable/declaration form that takes type parameters shares
    // `parse_type_parameters`, so all recover identically.
    for source in [
        "function f<A B>() {}",
        "class C<A B> {}",
        "interface I<A B> {}",
        "type Z<A B> = A;",
        "const f = <A B>() => 1;",
    ] {
        assert_missing_comma_recovery("g.ts", source, "A B");
    }
}

#[test]
fn missing_comma_between_type_parameters_does_not_drop_second_parameter() {
    // The recovery keeps parsing the list, so both type parameters survive.
    let (parser, _root) = parse_source("function f<A B>() {}");
    assert_eq!(
        count_nodes(&parser, syntax_kind_ext::TYPE_PARAMETER),
        2,
        "both type parameters should be parsed after comma recovery"
    );
}

#[test]
fn missing_comma_between_three_type_parameters_reports_a_comma_per_gap() {
    // Names are spaced far enough apart that the parser's
    // `ERROR_SUPPRESSION_DISTANCE` window does not collapse the two separate
    // missing-comma errors, so each gap is reported (matching tsc).
    let source = "function f<Aaaa Bbbb Cccc>() {}";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let b_at = gap_offset(source, "Aaaa Bbbb");
    let c_at = gap_offset(source, "Bbbb Cccc");
    assert!(
        diags
            .iter()
            .any(|d| d.code == 1005 && d.start == b_at && d.message == "',' expected."),
        "expected TS1005 `','` before `Bbbb`, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 1005 && d.start == c_at && d.message == "',' expected."),
        "expected TS1005 `','` before `Cccc`, got {diags:?}"
    );
    assert_no_close_gt(&parser, source);
    assert_eq!(
        count_nodes(&parser, syntax_kind_ext::TYPE_PARAMETER),
        3,
        "all three type parameters should be parsed"
    );
}

#[test]
fn missing_comma_between_three_short_type_parameters_still_parses_all() {
    // Even when the suppression window collapses the second comma error, the
    // recovery keeps parsing the list, so no element is dropped and no spurious
    // closing `'>'`/cascade appears.
    let source = "function f<A B C>() {}";
    let (parser, _root) = parse_source(source);
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .any(|d| d.code == 1005 && d.message == "',' expected."),
        "expected at least one TS1005 `','`, got {:?}",
        parser.get_diagnostics()
    );
    assert_no_close_gt(&parser, source);
    assert_eq!(
        count_nodes(&parser, syntax_kind_ext::TYPE_PARAMETER),
        3,
        "all three type parameters should be parsed"
    );
}

#[test]
fn missing_comma_with_constraint_then_next_type_parameter() {
    // `A extends X` is fully parsed, then the missing comma before `B` is recovered.
    let source = "function f<A extends X B>() {}";
    assert_missing_comma_recovery("g.ts", source, "X B");
    let (parser, _root) = parse_source(source);
    assert_eq!(count_nodes(&parser, syntax_kind_ext::TYPE_PARAMETER), 2);
}

#[test]
fn type_parameter_list_terminated_by_open_paren_reports_close_gt() {
    // tsc's `isListTerminator(TypeParameters)` includes `(`, `{`, `extends`,
    // and `implements`, so a type-parameter list ended by one of these closes
    // *without* a missing-comma error and `parse_expected_greater_than` reports
    // `'>' expected` at the offending token. Regression guard for
    // `assertInWrapSomeTypeParameter.ts`: `foo<U extends C<C<T>>(x: U)` — the
    // `(` ends the list, so tsc emits a single TS1005 `'>' expected` at the `(`
    // and never a `','` expected.
    let source = "class C<T extends C<T>> {\n    foo<U extends C<C<T>>(x: U) {\n        return null;\n    }\n}";
    // tsc emits exactly one diagnostic for this source: TS1005 `'>' expected.`
    // anchored at the `(` (byte offset 51 here / line 2 col 26). Pin both the
    // single-diagnostic count and the offset so the conformance fingerprint
    // matches exactly and no recovery cascade reappears.
    let open_paren_offset = source.find('(').expect("source contains `(`") as u32;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic for the type-parameter recovery, got {diags:?}"
    );
    let d = &diags[0];
    assert!(
        d.code == 1005 && d.message == "'>' expected." && d.start == open_paren_offset,
        "expected a single TS1005 `'>' expected.` at offset {open_paren_offset}, got {diags:?}"
    );
}

#[test]
fn missing_comma_between_type_arguments_terminates_and_reports_close_gt() {
    // Unlike type *parameters*, a type-*argument* list does NOT recover a
    // missing comma: tsc's `isListTerminator(TypeArguments)` is true for every
    // token except `,`, so the first non-comma token terminates the list and
    // `parse_expected_greater_than` reports `'>' expected` (never `','`). This
    // is the behavior the JSX recovery test `jsxUnclosedParserRecovery.ts`
    // depends on for forms like `<diddy<boolean> bananas="please">`.
    let (parser, _root) = parse_source("type R = Foo<A B>;");
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == 1005 && d.message == "'>' expected."),
        "expected TS1005 `'>' expected.`, got {diags:?}"
    );
    assert!(
        diags.iter().all(|d| d.message != "',' expected."),
        "type-argument list must not recover a missing comma, got {diags:?}"
    );
}

#[test]
fn type_parameter_list_followed_by_non_element_still_terminates() {
    // A non-element token after the missing comma (here `)`) must not loop and
    // must not be mis-recovered as another parameter.
    let (parser, _root) = parse_source("function f<A )>() {}");
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == 1005 && d.message == "',' expected."),
        "expected TS1005 `','`, got {diags:?}"
    );
}
