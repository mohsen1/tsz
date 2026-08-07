//! Parity tests for `parse_parameter_modifiers` against tsc's
//! `checkGrammarModifiers` for a `Parameter` node.
//!
//! tsc selects a dedicated code for some misplaced parameter modifiers
//! (`abstract` -> TS1242, `accessor` -> TS1275/TS1243, `in`/`out` -> TS1274)
//! rather than the generic TS1090, and it reports at most one grammar error per
//! parameter (first grammar-invalid modifier wins, then it returns). Earlier,
//! tsz emitted TS1090 for every invalid modifier. See issue #16778.
//!
//! These assert the *parser* diagnostic list, which contains only the grammar
//! codes — the semantic TS2369 ("parameter property only allowed in a
//! constructor") is emitted by the checker and is out of scope here. Binder and
//! parameter names are varied across rows per the anti-hardcoding contract.

use crate::parser::test_fixture::parse_source;

fn grammar_diags(src: &str) -> Vec<(u32, String)> {
    let (parser, _root) = parse_source(src);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

fn codes(src: &str) -> Vec<u32> {
    grammar_diags(src).into_iter().map(|(c, _)| c).collect()
}

// --- dedicated code selection ------------------------------------------------

#[test]
fn abstract_on_function_parameter_is_ts1242() {
    assert_eq!(
        grammar_diags("function f(abstract x: number) {}"),
        vec![(
            1242,
            "'abstract' modifier can only appear on a class, method, or property declaration."
                .to_string()
        )]
    );
}

#[test]
fn accessor_on_function_parameter_is_ts1275() {
    assert_eq!(
        grammar_diags("function draw(accessor pt: number) {}"),
        vec![(
            1275,
            "'accessor' modifier can only appear on a property declaration.".to_string()
        )]
    );
}

#[test]
fn in_and_out_on_parameter_are_ts1274() {
    assert_eq!(
        grammar_diags("function reduce(in seed: number) {}"),
        vec![(
            1274,
            "'in' modifier can only appear on a type parameter of a class, interface or type alias"
                .to_string()
        )]
    );
    assert_eq!(
        grammar_diags("function emit(out sink: number) {}"),
        vec![(
            1274,
            "'out' modifier can only appear on a type parameter of a class, interface or type alias"
                .to_string()
        )]
    );
}

#[test]
fn abstract_on_constructor_parameter_is_ts1242() {
    assert_eq!(
        codes("class Shape { constructor(abstract radius: number) {} }"),
        vec![1242]
    );
}

#[test]
fn accessor_on_constructor_parameter_is_ts1275() {
    assert_eq!(
        codes("class Box { constructor(accessor width: number) {} }"),
        vec![1275]
    );
}

#[test]
fn abstract_on_method_setter_and_arrow_parameters_are_ts1242() {
    assert_eq!(
        codes("class C { render(abstract node: number) {} }"),
        vec![1242]
    );
    assert_eq!(
        codes("class C { set p(abstract value: number) {} }"),
        vec![1242]
    );
    assert_eq!(
        codes("const g = (abstract item: number) => item;"),
        vec![1242]
    );
    assert_eq!(
        codes("const o = { run(abstract task: number) {} };"),
        vec![1242]
    );
}

// --- combinations: accessor/readonly and ordering ----------------------------

#[test]
fn readonly_then_accessor_is_ts1243() {
    assert_eq!(
        grammar_diags("function f(readonly accessor entry: number) {}"),
        vec![(
            1243,
            "'accessor' modifier cannot be used with 'readonly' modifier.".to_string()
        )]
    );
    // constructor position too
    assert_eq!(
        codes("class Reg { constructor(readonly accessor slot: number) {} }"),
        vec![1243]
    );
}

#[test]
fn accessor_then_readonly_reports_only_accessor() {
    // accessor comes first: TS1275, then the run stops (readonly is valid anyway).
    assert_eq!(
        codes("function f(accessor readonly node: number) {}"),
        vec![1275]
    );
}

#[test]
fn static_after_readonly_is_ordering_ts1029() {
    assert_eq!(
        grammar_diags("function f(readonly static field: number) {}"),
        vec![(
            1029,
            "'static' modifier must precede 'readonly' modifier.".to_string()
        )]
    );
}

#[test]
fn accessibility_after_readonly_is_ordering_ts1029() {
    assert_eq!(
        grammar_diags("function f(readonly public value: number) {}"),
        vec![(
            1029,
            "'public' modifier must precede 'readonly' modifier.".to_string()
        )]
    );
}

// --- single-report: only the first grammar-invalid modifier fires ------------

#[test]
fn abstract_then_static_reports_only_abstract() {
    assert_eq!(
        codes("function f(abstract static x: number) {}"),
        vec![1242]
    );
}

#[test]
fn static_then_abstract_reports_only_static() {
    assert_eq!(
        codes("function f(static abstract x: number) {}"),
        vec![1090]
    );
}

#[test]
fn duplicate_abstract_reports_once() {
    assert_eq!(
        codes("function f(abstract abstract x: number) {}"),
        vec![1242]
    );
}

// --- regression: modifiers already correct stay correct ----------------------

#[test]
fn static_export_declare_async_on_parameter_stay_ts1090() {
    assert_eq!(codes("function f(static x: number) {}"), vec![1090]);
    assert_eq!(codes("function f(export x: number) {}"), vec![1090]);
    assert_eq!(codes("function f(declare x: number) {}"), vec![1090]);
    assert_eq!(codes("function f(async x: number) {}"), vec![1090]);
}

#[test]
fn duplicate_accessibility_and_readonly_unchanged() {
    assert_eq!(codes("function f(public public x: number) {}"), vec![1028]);
    assert_eq!(
        codes("function f(readonly readonly x: number) {}"),
        vec![1030]
    );
}

#[test]
fn valid_parameter_properties_have_no_parser_grammar_error() {
    // Grammar-valid parameter properties: any TS2369 is the checker's job.
    assert_eq!(
        codes(
            "class C { constructor(public a: number, private b: string, protected c: boolean, readonly d: number) {} }"
        ),
        Vec::<u32>::new()
    );
}
