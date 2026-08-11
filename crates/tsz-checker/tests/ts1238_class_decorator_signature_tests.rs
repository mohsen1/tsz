//! TS1238 class-decorator signature resolution across both decorator modes
//! (issue #17108).
//!
//! Structural rule: a class decorator is resolved as the call
//! `decorator(value[, context])` — `decorator(classConstructor)` under
//! `--experimentalDecorators`, and `decorator(typeof C,
//! ClassDecoratorContext<typeof C>)` (truncated to the decorator's declared
//! arity) in TC39 stage-3 ES mode. When that resolution fails, tsc reports
//! TS1238 ("Unable to resolve signature of class decorator when called as an
//! expression.") with an elaboration keyed on the failure kind:
//!
//! - non-callable callee -> "This expression is not callable." /
//!   "Type 'X' has no call signatures.";
//! - argument type mismatch -> the TS2345 "Argument of type X is not
//!   assignable to parameter of type Y." line;
//! - too few / too many arguments -> the TS1278/TS1279 arity line.
//!
//! A bare-reference zero-argument factory used uncalled (`@d`, `@ns.d`) draws
//! TS1329 instead; a parenthesized/inline factory falls through to the arity
//! failure.
//!
//! tsz previously (a) dropped the diagnostic *entirely* in ES mode for
//! non-arity failures, and (b) emitted only the bare TS1238 (no elaboration)
//! under `--experimentalDecorators`. Oracle-verified against pinned
//! `typescript@7.0.2` in both modes.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn es(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn legacy(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
}

fn find(diags: &[Diagnostic], code: u32) -> Option<&Diagnostic> {
    diags.iter().find(|d| d.code == code)
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

// ───────────────────────── ES mode: non-arity failures ──────────────────────
//
// These were the headline #17108 symptom: nothing at all was emitted.

#[test]
fn es_non_callable_value_emits_ts1238_with_chain() {
    // A non-function value used as an ES class decorator.
    let diags = es("const deco = 1;\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 in ES mode");
    assert_eq!(primary.related_information.len(), 2);
    assert_eq!(primary.related_information[0].code, 2349);
    assert_eq!(
        primary.related_information[0].message_text,
        "This expression is not callable."
    );
    assert_eq!(primary.related_information[1].code, 2757);
    assert_eq!(primary.related_information[1].depth, 1);
}

#[test]
fn es_argument_type_mismatch_emits_ts1238_with_argument_line() {
    // A rest parameter must not mask the argument type mismatch: `value`
    // (`typeof C`) is not assignable to the `string` first parameter.
    let diags = es("function deco(a: string, b: string, ...r: string[]) {}\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 in ES mode");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 2345);
    assert_eq!(
        primary.related_information[0].message_text,
        "Argument of type 'typeof C' is not assignable to parameter of type 'string'."
    );
}

#[test]
fn es_second_argument_is_checked_against_context_parameter() {
    // The ES decorator receives a *second* `context` argument, so a `number`
    // second parameter — which the first argument (`typeof C`) satisfies —
    // still fails on the context argument. (The full lib renders the context
    // as `ClassDecoratorContext<typeof C>`, oracle-verified; the reduced
    // unit-test lib falls back to `{}`, so this asserts the stable target
    // parameter type rather than the context type's rendered name.)
    let diags = es("function deco(a: typeof C, b: number) {}\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 in ES mode");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 2345);
    assert!(
        primary.related_information[0]
            .message_text
            .ends_with("is not assignable to parameter of type 'number'."),
        "expected the context argument to fail against the number parameter, got: {}",
        primary.related_information[0].message_text
    );
}

#[test]
fn es_class_used_as_decorator_is_not_callable() {
    // A class has construct signatures but no call signatures.
    let diags = es("class Deco {}\n@Deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 in ES mode");
    assert_eq!(primary.related_information.len(), 2);
    assert_eq!(primary.related_information[0].code, 2349);
    assert_eq!(primary.related_information[1].code, 2757);
}

// ───────────────────────── ES mode: arity vs TS1329 ─────────────────────────

#[test]
fn es_bare_reference_zero_arg_factory_is_ts1329_not_ts1238() {
    // A bare-reference zero-parameter factory draws the "did you mean to call
    // it first" hint, not the arity TS1238.
    let diags = es("function deco() {}\n@deco\nclass C {}\n");
    assert!(codes(&diags).contains(&1329), "got: {:?}", codes(&diags));
    assert!(!codes(&diags).contains(&1238), "got: {:?}", codes(&diags));
}

#[test]
fn es_property_access_zero_arg_factory_is_ts1329() {
    // The reference form covers property-access chains too (`@ns.deco`).
    let diags = es("const ns = { deco: () => {} };\n@ns.deco\nclass C {}\n");
    assert!(codes(&diags).contains(&1329), "got: {:?}", codes(&diags));
    assert!(!codes(&diags).contains(&1238), "got: {:?}", codes(&diags));
}

#[test]
fn es_parenthesized_zero_arg_factory_is_ts1238_not_ts1329() {
    // A parenthesized/inline factory is NOT a bare reference, so tsc reports
    // the arity TS1238 rather than the TS1329 call-it-first hint.
    let diags = es("@(() => {})\nclass C {}\n");
    assert!(codes(&diags).contains(&1238), "got: {:?}", codes(&diags));
    assert!(!codes(&diags).contains(&1329), "got: {:?}", codes(&diags));
}

#[test]
fn es_too_many_required_params_emits_ts1238_arity() {
    // Three required parameters cannot be satisfied by the two-argument ES
    // decorator ABI: the runtime supplies 2, the decorator wants 3, so this is
    // a too-few-arguments failure from the call's perspective and also gains
    // the TS6210 missing-argument pointer at the unsupplied third parameter.
    let diags = es("function deco(a: any, b: any, c: any) {}\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 in ES mode");
    assert_eq!(primary.related_information.len(), 2);
    assert_eq!(primary.related_information[0].code, 1278);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects 3."
    );
    assert_eq!(primary.related_information[1].code, 6210);
}

#[test]
fn es_one_or_two_param_decorator_is_accepted() {
    // 1- and 2-parameter `any` decorators satisfy the ES ABI with no error.
    for source in [
        "function deco(a: any) {}\n@deco\nclass C {}\n",
        "function deco(a: any, b: any) {}\n@deco\nclass C {}\n",
    ] {
        let diags = es(source);
        assert!(
            !codes(&diags).contains(&1238) && !codes(&diags).contains(&1329),
            "expected no decorator-signature diagnostic for {source:?}, got: {:?}",
            codes(&diags)
        );
    }
}

// ───────────────────────── legacy mode: non-arity failures ──────────────────

#[test]
fn legacy_non_callable_value_emits_ts1238_with_chain() {
    let diags = legacy("const deco = 1;\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 (legacy)");
    assert_eq!(primary.related_information.len(), 2);
    assert_eq!(primary.related_information[0].code, 2349);
    assert_eq!(primary.related_information[1].code, 2757);
    assert_eq!(primary.related_information[1].depth, 1);
}

#[test]
fn legacy_argument_type_mismatch_emits_ts1238_with_argument_line() {
    let diags = legacy("function deco(target: string) {}\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 (legacy)");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 2345);
    assert_eq!(
        primary.related_information[0].message_text,
        "Argument of type 'typeof C' is not assignable to parameter of type 'string'."
    );
}

#[test]
fn legacy_bare_reference_zero_arg_factory_is_ts1329() {
    // A zero-parameter legacy class decorator factory used uncalled.
    let diags = legacy("function deco() {}\n@deco\nclass C {}\n");
    assert!(codes(&diags).contains(&1329), "got: {:?}", codes(&diags));
    assert!(!codes(&diags).contains(&1238), "got: {:?}", codes(&diags));
}

#[test]
fn legacy_too_few_args_keeps_arity_elaboration() {
    // Regression guard: the legacy too-few arity elaboration still fires.
    let diags = legacy("function deco(target: Function, extra: string) {}\n@deco\nclass C {}\n");
    let primary = find(&diags, 1238).expect("expected TS1238 (legacy)");
    assert_eq!(primary.related_information[0].code, 1278);
}

#[test]
fn legacy_well_typed_decorator_has_no_signature_diagnostic() {
    // Positive control: a `(target: Function) => void` decorator is valid.
    let diags = legacy("function deco(target: Function) {}\n@deco\nclass C {}\n");
    assert!(
        !codes(&diags).contains(&1238) && !codes(&diags).contains(&1270),
        "got: {:?}",
        codes(&diags)
    );
}

// ───────────────────────── renamed-binder invariance ────────────────────────

#[test]
fn behavior_is_independent_of_decorator_and_class_names() {
    // Anti-hardcoding: the same non-callable failure must fire regardless of
    // the identifiers chosen for the decorator and the class.
    for (deco, class) in [("deco", "C"), ("wrap", "Widget"), ("qqq", "ZZZ")] {
        let source = format!("const {deco} = 42;\n@{deco}\nclass {class} {{}}\n");
        let es_diags = es(&source);
        let legacy_diags = legacy(&source);
        assert!(
            codes(&es_diags).contains(&1238),
            "ES: expected TS1238 for {source:?}, got: {:?}",
            codes(&es_diags)
        );
        assert!(
            codes(&legacy_diags).contains(&1238),
            "legacy: expected TS1238 for {source:?}, got: {:?}",
            codes(&legacy_diags)
        );
    }
}
