//! TS2769 "No overload matches this call" structured-elaboration tests.
//!
//! `tsc` renders the TS2769 body as a message chain: the head plus, for every
//! overload that matched arity but failed argument-type checks, an
//! `Overload {i} of {N}, '<sig>', gave the following error.` (TS2772) header
//! with the argument error nested beneath it. When four or more overloads fail
//! that way it collapses to `The last overload gave the following error.`
//! (TS2770) plus the last candidate's error. Overloads that fail on arity are
//! never part of the chain.
//!
//! These tests assert the ordered `(code, depth, message)` sequence of the
//! related-information chain so the header text, per-candidate nesting, overload
//! numbering, and candidate selection are all locked to `tsc`'s shape. Binder
//! names are varied across cases so nothing keys on a specific identifier.

use crate::test_utils::check_source_diagnostics;

/// Return the TS2769 diagnostic's related chain as `(code, depth, message)`
/// triples in render order.
fn ts2769_chain(source: &str) -> Vec<(u32, u8, String)> {
    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .unwrap_or_else(|| panic!("expected a TS2769 diagnostic, got: {diagnostics:?}"));
    diag.related_information
        .iter()
        .map(|info| (info.code, info.depth, info.message_text.clone()))
        .collect()
}

#[test]
fn two_call_overloads_emit_numbered_headers_in_declaration_order() {
    let chain = ts2769_chain(
        r#"
declare function pick(value: string): number;
declare function pick(value: number): string;
pick(true);
"#,
    );
    assert_eq!(
        chain,
        vec![
            (
                2772,
                0,
                "Overload 1 of 2, '(value: string): number', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 2 of 2, '(value: number): string', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
        ],
    );
}

#[test]
fn three_method_overloads_number_of_three_and_keep_order() {
    // Interface method overloads; the last declared overload is arity-2 and
    // fails on arity, so it is excluded from the chain — the "{N}" count still
    // reflects the total overload set (3).
    let chain = ts2769_chain(
        r#"
interface Widget {
    render(mode: string): void;
    render(mode: number): void;
    render(mode: boolean): void;
}
declare const widget: Widget;
widget.render(1n);
"#,
    );
    assert_eq!(
        chain,
        vec![
            (
                2772,
                0,
                "Overload 1 of 3, '(mode: string): void', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'bigint' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 2 of 3, '(mode: number): void', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'bigint' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 3 of 3, '(mode: boolean): void', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'bigint' is not assignable to parameter of type 'boolean'."
                    .to_string()
            ),
        ],
    );
}

#[test]
fn arity_failing_overload_is_excluded_but_counts_toward_total() {
    // The first overload matches arity (1 arg) and fails on type; the second
    // requires 2 args and fails on arity, so only the first appears in the
    // chain — as "Overload 1 of 2" (total overload count, not candidate count).
    let chain = ts2769_chain(
        r#"
declare function attach(target: string): void;
declare function attach(target: number, extra: number): void;
declare function attach(target: string): void;
attach(false);
"#,
    );
    // Two arity-1 overloads (`string`) fail on type; the arity-2 overload is
    // excluded. Both surviving candidates spell `string`.
    assert_eq!(
        chain,
        vec![
            (
                2772,
                0,
                "Overload 1 of 3, '(target: string): void', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 2 of 3, '(target: string): void', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
        ],
    );
}

#[test]
fn four_argument_error_candidates_collapse_to_last_overload() {
    // Four overloads match arity and fail on type: tsc shows only the last,
    // headed by "The last overload gave the following error." (TS2770).
    let chain = ts2769_chain(
        r#"
declare function coerce(input: string): void;
declare function coerce(input: number): void;
declare function coerce(input: boolean): void;
declare function coerce(input: symbol): void;
coerce(1n);
"#,
    );
    assert_eq!(
        chain,
        vec![
            (
                2770,
                0,
                "The last overload gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'bigint' is not assignable to parameter of type 'symbol'."
                    .to_string()
            ),
        ],
    );
}

#[test]
fn construct_overloads_render_call_form_headers_without_new() {
    // Constructor overloads use the call-form header ('(a: string): object'),
    // never a `new ` prefix — matching tsc's signatureToString for the chain.
    let chain = ts2769_chain(
        r#"
interface Factory {
    new (spec: string): object;
    new (spec: number): object;
}
declare const Factory: Factory;
new Factory(true);
"#,
    );
    assert_eq!(
        chain,
        vec![
            (
                2772,
                0,
                "Overload 1 of 2, '(spec: string): object', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 2 of 2, '(spec: number): object', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
        ],
    );
}

#[test]
fn overload_headers_reflect_renamed_binders_and_param_names() {
    // Anti-hardcoding: the header signature strings must reflect the actual
    // (renamed) callee and parameter names, proving nothing keys on a fixed
    // identifier. Only the parameter spelling changes vs the first test.
    let chain = ts2769_chain(
        r#"
declare function zorp(quux: string): number;
declare function zorp(quux: number): string;
zorp(true);
"#,
    );
    assert_eq!(
        chain,
        vec![
            (
                2772,
                0,
                "Overload 1 of 2, '(quux: string): number', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
            (
                2772,
                0,
                "Overload 2 of 2, '(quux: number): string', gave the following error.".to_string()
            ),
            (
                2345,
                1,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
        ],
    );
}
