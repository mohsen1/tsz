//! TS1238 "not callable" elaboration chain for class decorators.
//!
//! Structural rule (oracle-verified, `typescript@7.0.2`): when a class
//! decorator's resolved type carries no call signature at all — a bare
//! primitive, a class (construct signatures only), or any other
//! non-callable value — tsc attaches a two-level elaboration chain beneath
//! TS1238: `This expression is not callable.` (TS2349) with `Type 'X' has
//! no call signatures.` (TS2757) nested one level beneath it. This chain is
//! identical under `--experimentalDecorators` and ES (TC39 stage-3) class
//! decorators.
//!
//! Before this fix:
//! - `--experimentalDecorators`: `check_class_decorator_call_signature`'s
//!   "no call signatures at all" branch emitted a bare TS1238 with no
//!   elaboration.
//! - ES mode: `check_es_class_decorator_arity` only ever inspected
//!   `function_shape`, so a decorator type with no function shape at all
//!   (a primitive, a class, an overloaded callable) silently passed
//!   validation with **no diagnostic whatsoever** — not even a bare TS1238.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

// Loaded with the default libs (matching `invocation_signature_detail_tests.rs`)
// so a bare primitive source resolves to its boxed wrapper interface
// (`Number`, `String`, ...) for display, the behavior a real compilation
// observes and what tsc's own oracle output shows.
fn check_legacy(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
        &load_default_lib_files(),
    )
}

fn check_es(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    )
}

fn ts1238_chain_text(diagnostics: &[Diagnostic]) -> String {
    let matching: Vec<_> = diagnostics.iter().filter(|d| d.code == 1238).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS1238, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

fn assert_not_callable_chain(text: &str, type_str: &str) {
    assert!(
        text.contains("This expression is not callable."),
        "missing 'not callable' chain link: {text:?}"
    );
    assert!(
        text.contains(&format!("Type '{type_str}' has no call signatures.")),
        "missing 'has no call signatures' chain link for {type_str:?}: {text:?}"
    );
}

// ───────────────────────── --experimentalDecorators ────────────────────────

#[test]
fn legacy_primitive_decorator_gets_not_callable_chain() {
    let text = ts1238_chain_text(&check_legacy("const d = 1; @d class C {}"));
    assert_not_callable_chain(&text, "Number");
}

#[test]
fn legacy_class_used_as_decorator_gets_not_callable_chain() {
    let text = ts1238_chain_text(&check_legacy("class D {} @D class C {}"));
    assert_not_callable_chain(&text, "typeof D");
}

#[test]
fn legacy_string_literal_decorator_gets_not_callable_chain_renamed_binder() {
    // Anti-hardcoding: a differently-named binder gets the same chain.
    let text = ts1238_chain_text(&check_legacy(
        "const notify = \"hi\"; @notify class Widget {}",
    ));
    assert_not_callable_chain(&text, "String");
}

// ─────────────────────────────── ES decorators ──────────────────────────────

#[test]
fn es_primitive_decorator_now_reports_ts1238_with_chain() {
    // Previously dropped entirely: no function_shape means the old
    // check_es_class_decorator_arity silently returned without a diagnostic.
    let diagnostics = check_es("const d = 1; @d class C {}");
    let text = ts1238_chain_text(&diagnostics);
    assert_not_callable_chain(&text, "Number");
}

#[test]
fn es_class_used_as_decorator_now_reports_ts1238_with_chain() {
    let diagnostics = check_es("class D {} @D class C {}");
    let text = ts1238_chain_text(&diagnostics);
    assert_not_callable_chain(&text, "typeof D");
}

#[test]
fn es_primitive_decorator_anchors_at_expression_not_at_sign() {
    let source = "const d = 1; @d class C {}";
    let diagnostics = check_es(source);
    let expr_start = source.rfind('d').expect("decorator identifier present");
    let at_sign = source.find('@').expect("@ present");
    let ts1238 = diagnostics
        .iter()
        .find(|d| d.code == 1238)
        .unwrap_or_else(|| panic!("expected TS1238, got: {diagnostics:?}"));
    assert_eq!(
        ts1238.start, expr_start as u32,
        "expected anchor at the expression ({expr_start}), got {}",
        ts1238.start
    );
    assert_ne!(ts1238.start, at_sign as u32, "must not anchor at `@`");
}

// ───────────────── controls: unaffected shapes stay unaffected ─────────────

#[test]
fn es_function_typed_decorator_is_not_flagged() {
    // tsc's `isUntypedFunctionCall` fallback: a decorator declared as the
    // global `Function` interface has no explicit call signatures but is
    // still treated as callable. Regression guard for the
    // `prepare_decorator_callee` gate added alongside the not-callable check.
    let codes: Vec<u32> = check_es("declare const d: Function; @d class C {}")
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(
        !codes.contains(&1238),
        "Function-typed decorator must not draw TS1238, got: {codes:?}"
    );
}

#[test]
fn es_too_many_params_decorator_keeps_bare_ts1238_no_not_callable_chain() {
    // A genuinely callable decorator that just declares too many required
    // parameters is a distinct failure kind (arity, not callability) and
    // must not gain the not-callable chain.
    let diagnostics = check_es(
        "function d(a: string, b: string, c: string) { return undefined as any; } @d class C {}",
    );
    let text = ts1238_chain_text(&diagnostics);
    assert!(
        !text.contains("has no call signatures"),
        "an arity-only failure must not draw the not-callable chain: {text:?}"
    );
}

#[test]
fn legacy_arity_failure_keeps_arity_elaboration_no_not_callable_chain() {
    let diagnostics = check_legacy("function d(a: string, b: string, c: string) {} @d class C {}");
    let text = ts1238_chain_text(&diagnostics);
    assert!(
        text.contains("the decorator expects"),
        "expected the pre-existing arity elaboration to survive: {text:?}"
    );
    assert!(
        !text.contains("has no call signatures"),
        "an arity-only failure must not draw the not-callable chain: {text:?}"
    );
}
