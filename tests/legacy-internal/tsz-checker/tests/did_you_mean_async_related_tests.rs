//! Regression tests for the `TS1356` `Did you mean to mark this function as
//! 'async'?` pointer that tsc pairs with `TS1308` (`await` outside `async`)
//! and `TS1103` (`for await` outside `async`).
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! after building either diagnostic, tsc looks up `getContainingFunction` and
//! attaches the TS1356 related-information entry when that container exists,
//! is not a constructor, and does not carry `async`. The anchor is
//! `getErrorSpanForNode(container)` — the declared name where there is one,
//! the assigned name for an anonymous function expression, and the arrow
//! itself (trimmed to its header when its block body is multi-line) for an
//! arrow.
//!
//! The entry is a cross-location pointer, not a message-chain link: tsc
//! `--pretty` prints it with its own location and snippet while tsc
//! `--pretty false` prints nothing for it. `location_pointer_keeps_plain_mode_
//! output_unchanged` pins that tag, which is what keeps plain-mode (and so
//! conformance) output byte-identical.
//!
//! tsz builds the pointer in `error_reporter/async_suggestion.rs`, reached
//! from the two await-grammar sites in
//! `types/type_checking/core_statement_checks.rs`.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS1103: u32 = diagnostic_codes::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF;
const TS1308: u32 =
    diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS;
const TS1356: u32 = diagnostic_codes::DID_YOU_MEAN_TO_MARK_THIS_FUNCTION_AS_ASYNC;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

/// The text the TS1356 pointer covers, so each case asserts the anchor lands
/// exactly where tsc's does rather than merely existing.
fn pointer_text(source: &str, diagnostic: &Diagnostic) -> String {
    let pointers: Vec<_> = diagnostic
        .related_information
        .iter()
        .filter(|info| info.code == TS1356)
        .collect();
    assert_eq!(
        pointers.len(),
        1,
        "expected exactly one TS1356 pointer; got {:?}",
        diagnostic
            .related_information
            .iter()
            .map(|info| (info.code, info.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let pointer = pointers[0];
    assert_eq!(
        pointer.message_text,
        "Did you mean to mark this function as 'async'?"
    );
    source[pointer.start as usize..(pointer.start + pointer.length) as usize].to_string()
}

fn ts1308_pointer_text(source: &str) -> String {
    let diagnostic = only(&check_source_diagnostics(source), TS1308);
    pointer_text(source, &diagnostic)
}

fn ts1308_related_codes(source: &str) -> Vec<u32> {
    only(&check_source_diagnostics(source), TS1308)
        .related_information
        .iter()
        .map(|info| info.code)
        .collect()
}

#[test]
fn function_declaration_points_at_its_name() {
    let source = "function fetchIt() { const x = await 1; return x; }\n";
    assert_eq!(ts1308_pointer_text(source), "fetchIt");
}

/// Binder-name variation: the anchor is derived from the declaration, never
/// from any particular identifier text.
#[test]
fn renamed_function_declaration_points_at_its_own_name() {
    let source = "function zzz_other_name() { const x = await 1; return x; }\n";
    assert_eq!(ts1308_pointer_text(source), "zzz_other_name");
}

#[test]
fn generator_declaration_points_at_its_name() {
    let source = "function* gen() { const d = await 5; return d; }\n";
    assert_eq!(ts1308_pointer_text(source), "gen");
}

#[test]
fn named_function_expression_points_at_its_own_name() {
    let source = "const inner = function nested() { return await 1; };\n";
    assert_eq!(ts1308_pointer_text(source), "nested");
}

#[test]
fn anonymous_function_expression_points_at_the_variable_it_is_assigned_to() {
    let source = "const g = function () { return await 1; };\n";
    assert_eq!(ts1308_pointer_text(source), "g");
}

#[test]
fn anonymous_function_expression_points_at_the_property_it_is_assigned_to() {
    let source = "const po = { p: function () { return await 3; } };\n";
    assert_eq!(ts1308_pointer_text(source), "p");
}

#[test]
fn anonymous_function_expression_points_at_the_assignment_target() {
    let source = "let assigned;\nassigned = function () { return await 4; };\n";
    assert_eq!(ts1308_pointer_text(source), "assigned");
}

/// A class property initializer is **not** one of `getAssignedName`'s parent
/// shapes, so the anchor falls back to the `function` keyword rather than the
/// property name — oracle-confirmed, and the reason the assigned-name lookup
/// reads the parent kind instead of the nearest named declaration.
#[test]
fn unassigned_function_expression_points_at_the_function_keyword() {
    let source = "const r = (function () { return await 1; })();\n";
    assert_eq!(ts1308_pointer_text(source), "function");
}

#[test]
fn class_property_function_expression_points_at_the_function_keyword() {
    let source = "class D { p = function () { return await 2; }; }\n";
    assert_eq!(ts1308_pointer_text(source), "function");
}

#[test]
fn single_line_arrow_points_at_the_whole_arrow() {
    let source = "const one = () => { return await 2; };\n";
    assert_eq!(ts1308_pointer_text(source), "() => { return await 2; }");
}

#[test]
fn concise_body_arrow_points_at_the_whole_arrow() {
    let source = "const h = () => await 2;\n";
    assert_eq!(ts1308_pointer_text(source), "() => await 2");
}

/// tsc trims a multi-line arrow to its header so the pointer stays on one
/// line: everything up to and including the body's opening brace.
#[test]
fn multi_line_arrow_points_at_its_header_only() {
    let source = "const multi = () => {\n  return await 1;\n};\n";
    assert_eq!(ts1308_pointer_text(source), "() => {");
}

#[test]
fn method_declaration_points_at_its_name() {
    let source = "class C { m() { const b = await 2; return b; } }\n";
    assert_eq!(ts1308_pointer_text(source), "m");
}

#[test]
fn object_literal_method_points_at_its_name() {
    let source = "const o = { m() { return await 1; } };\n";
    assert_eq!(ts1308_pointer_text(source), "m");
}

#[test]
fn getter_points_at_its_name() {
    let source = "const obj = { get g() { return await 4; } };\n";
    assert_eq!(ts1308_pointer_text(source), "g");
}

#[test]
fn computed_method_name_points_at_the_whole_computed_name() {
    let source = "declare const k: string;\nconst cm = { [k]() { return await 5; } };\n";
    assert_eq!(ts1308_pointer_text(source), "[k]");
}

/// tsc excludes `SyntaxKind.Constructor` from the suggestion outright — a
/// constructor cannot be made `async`.
#[test]
fn constructor_gets_no_suggestion() {
    let source = "class C { constructor() { const a = await 1; } }\n";
    assert!(
        ts1308_related_codes(source).is_empty(),
        "a constructor must carry no TS1356 pointer"
    );
}

/// The nearest container wins, exactly as `getContainingFunction` does: a
/// non-async function nested inside an `async` one still gets the suggestion,
/// pointing at the inner function.
#[test]
fn nested_non_async_function_inside_async_points_at_the_inner_function() {
    let source = "async function outer() { const inner = function nested() { return await 1; }; return inner; }\n";
    assert_eq!(ts1308_pointer_text(source), "nested");
}

/// Top-level `await` in a script has no containing function, so there is
/// nothing to suggest marking.
#[test]
fn top_level_await_in_a_non_module_gets_no_suggestion() {
    let diagnostics = check_source_diagnostics("const x = await 1;\n");
    assert!(
        diagnostics
            .iter()
            .flat_map(|diagnostic| diagnostic.related_information.iter())
            .all(|info| info.code != TS1356),
        "top-level await must carry no TS1356 pointer: {diagnostics:?}"
    );
}

#[test]
fn for_await_in_a_plain_function_points_at_its_name() {
    let source =
        "declare const arr: number[];\nfunction f1() { for await (const x of arr) { x; } }\n";
    let diagnostic = only(&check_source_diagnostics(source), TS1103);
    assert_eq!(pointer_text(source, &diagnostic), "f1");
}

#[test]
fn for_await_in_a_constructor_gets_no_suggestion() {
    let source = "declare const arr: number[];\nclass C { constructor() { for await (const x of arr) { x; } } }\n";
    let diagnostic = only(&check_source_diagnostics(source), TS1103);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS1356),
        "a constructor must carry no TS1356 pointer: {diagnostic:?}"
    );
}

/// The pointer models tsc's `relatedInformation`, not a `messageText` chain
/// link, so it must carry that tag through to the reporter. Oracled on
/// `typescript@7.0.2`: `tsc --noEmit --strict --pretty false` prints the
/// TS1308 line alone, while `--pretty` prints the suggestion beneath it with
/// its own location and snippet. Only the tag distinguishes the two.
#[test]
fn location_pointer_keeps_plain_mode_output_unchanged() {
    let source = "function fetchIt() { const x = await 1; return x; }\n";
    let diagnostic = only(&check_source_diagnostics(source), TS1308);
    let pointer = diagnostic
        .related_information
        .iter()
        .find(|info| info.code == TS1356)
        .expect("TS1356 pointer");
    assert!(
        pointer.is_location_pointer(),
        "TS1356 must be a cross-location pointer: {pointer:?}"
    );
}
