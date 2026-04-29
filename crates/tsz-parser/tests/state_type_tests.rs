//! Tests for type expression parsing in the parser.
use crate::parser::test_fixture::{parse_source, parse_source_named};

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
    // `function(T1, T2): R` is tsc's JSDoc-legacy function-type form.  tsc
    // treats the bare types as positional parameters with synthetic `argN`
    // names (`(arg0: T1, arg1: T2) => R`) and emits only TS8020.  Our parser
    // must mirror that — emitting TS7051 or TS2300 would be a cascade.
    let source = "var g: function(number, number): number = (n, m) => n + m;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().any(|d| d.code == 8020),
        "Expected TS8020 for JSDoc legacy function type, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != 17019 && d.code != 17020),
        "Bare-type parameter list should not trigger postfix/prefix nullable diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn jsdoc_legacy_function_type_with_this_binding_preserves_it() {
    // `function(this: T, string)` — `this:` is a this-binding (index 0), and
    // the bare `string` should be parsed as the 1-based `arg1: string` so the
    // resulting call-site arity is 1, matching tsc.
    let source = "var f: function(this: number, string): string;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().any(|d| d.code == 8020),
        "Expected TS8020 for JSDoc legacy function type, got {diagnostics:?}"
    );
    // Only TS8020 should surface.  A cascading TS7051 for the bare `string`
    // parameter would indicate the parameter lost its type annotation.
    let unexpected: Vec<_> = diagnostics.iter().filter(|d| d.code != 8020).collect();
    assert!(
        unexpected.is_empty(),
        "JSDoc `function(this: T, X)` should only emit TS8020, got {unexpected:?}"
    );
}

#[test]
fn jsdoc_legacy_function_type_with_new_marker_is_parsed_as_constructor() {
    // `function(new: R, A)` denotes a constructor type whose return type is R.
    // Without the `new:` shortcut the parser would model it as a 2-arity
    // function `(new: R, A)`, which cascades into TS2554 at call sites such
    // as `new ctor('hi')`.  The parser should only emit TS8020.
    let source = "var c: function(new: number, string);";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().any(|d| d.code == 8020),
        "Expected TS8020 for JSDoc legacy constructor function type, got {diagnostics:?}"
    );
    let unexpected: Vec<_> = diagnostics.iter().filter(|d| d.code != 8020).collect();
    assert!(
        unexpected.is_empty(),
        "JSDoc `function(new: R, A)` should only emit TS8020, got {unexpected:?}"
    );
}
