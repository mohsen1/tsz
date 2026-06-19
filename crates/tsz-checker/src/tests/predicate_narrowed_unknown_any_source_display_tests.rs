//! Regression tests for the diagnostic *display* of a source operand that was
//! declared `unknown`/`any` and then flow-narrowed by a user-defined
//! type-predicate guard (`x is T`).
//!
//! tsz narrows such an operand correctly (the value's type *is* the predicate
//! type — property access and assignability behave accordingly), but the
//! TS2322 assignment-source display repainted the operand with its *declared*
//! `unknown`/`any` annotation instead of the narrowed type. tsc renders the
//! narrowed type:
//!
//! ```ts
//! declare function isStr(x: unknown): x is string;
//! function f(v: unknown) { if (isStr(v)) { const r: never = v; } }
//! //                                              ^ tsc: Type 'string' ...
//! //                                                tsz (before): Type 'unknown' ...
//! ```
//!
//! The annotation-recovery display heuristics exist to recover lost
//! alias/parameter *names*; for a narrowed `unknown`/`any` operand they widened
//! the display back to the declared supertype. The fix suppresses that repaint
//! when the source identifier's declared type is `unknown`/`any` but its
//! narrowed type is concrete. Tests vary every binder name so the behavior is
//! not keyed on identifier text, and include negative controls (un-narrowed
//! `unknown`/`any`, concrete declared types) that must be unaffected.

use crate::test_utils::check_source_diagnostics;

fn first_2322_msg(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322 = diags.iter().find(|d| d.code == 2322).unwrap_or_else(|| {
        panic!(
            "Expected TS2322, got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        )
    });
    ts2322.message_text.clone()
}

#[test]
fn predicate_narrowed_unknown_operand_displays_string() {
    let msg = first_2322_msg(
        r#"
declare function isStr(x: unknown): x is string;
function f(v: unknown) { if (isStr(v)) { const r: never = v; } }
"#,
    );
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "narrowed unknown should display the narrowed 'string'. Got: {msg}"
    );
    assert!(
        !msg.contains("'unknown'"),
        "narrowed unknown must not display 'unknown'. Got: {msg}"
    );
}

#[test]
fn predicate_narrowed_unknown_operand_displays_object() {
    let msg = first_2322_msg(
        r#"
declare function isObj(value: unknown): value is object;
function g(payload: unknown) { if (isObj(payload)) { const sink: never = payload; } }
"#,
    );
    assert!(
        msg.contains("Type 'object' is not assignable to type 'never'"),
        "narrowed unknown should display the narrowed 'object'. Got: {msg}"
    );
}

#[test]
fn predicate_narrowed_unknown_operand_displays_named_object_predicate() {
    // A named-interface predicate type carries an index signature, which routes
    // the source display through the `should_prefer_declared_source_annotation`
    // path (distinct from the bare-primitive path above). Use a locally declared
    // interface so the test is independent of the standard library.
    let msg = first_2322_msg(
        r#"
interface Bag { [key: string]: unknown }
declare function isBag(candidate: unknown): candidate is Bag;
function h(slot: unknown) { if (isBag(slot)) { const dead: never = slot; } }
"#,
    );
    assert!(
        msg.contains("Type 'Bag' is not assignable to type 'never'"),
        "narrowed unknown should display the predicate type 'Bag'. Got: {msg}"
    );
    assert!(
        !msg.contains("'unknown' is not assignable"),
        "narrowed unknown must not display the declared 'unknown'. Got: {msg}"
    );
}

#[test]
fn predicate_narrowed_any_operand_displays_string() {
    let msg = first_2322_msg(
        r#"
declare function looksLikeText(input: any): input is string;
function process(raw: any) { if (looksLikeText(raw)) { const out: never = raw; } }
"#,
    );
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "narrowed any should display the narrowed 'string'. Got: {msg}"
    );
    assert!(
        !msg.contains("'any'"),
        "narrowed any must not display 'any'. Got: {msg}"
    );
}

#[test]
fn predicate_narrowed_unknown_in_contextual_return_displays_string() {
    let msg = first_2322_msg(
        r#"
declare function isText(token: unknown): token is string;
function emit(token: unknown): number { if (isText(token)) { return token; } return 0; }
"#,
    );
    assert!(
        msg.contains("Type 'string' is not assignable to type 'number'"),
        "narrowed unknown in a contextual return should display 'string'. Got: {msg}"
    );
}

#[test]
fn predicate_narrowed_unknown_missing_property_displays_narrowed_shape() {
    // The missing-property (TS2741) path renders the source through a different
    // entry (`format_top_level_assignability_message_types_at`) than the bare
    // TS2322 path; it must also render the narrowed shape, not `unknown`.
    let diags = check_source_diagnostics(
        r#"
interface Target { a: string }
declare function isShape(probe: unknown): probe is { b: number };
function f(slot: unknown) { if (isShape(slot)) { const t: Target = slot; } }
"#,
    );
    let ts2741 = diags.iter().find(|d| d.code == 2741).unwrap_or_else(|| {
        panic!(
            "Expected TS2741, got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        )
    });
    assert!(
        ts2741.message_text.contains("{ b: number; }"),
        "missing-property source should display the narrowed shape. Got: {}",
        ts2741.message_text
    );
    assert!(
        !ts2741.message_text.contains("'unknown'"),
        "missing-property source must not display the declared 'unknown'. Got: {}",
        ts2741.message_text
    );
}

// --- Negative controls: behavior must be unchanged when there is no narrowing.

#[test]
fn unnarrowed_unknown_operand_still_displays_unknown() {
    let msg = first_2322_msg(
        r#"
function f(v: unknown) { const r: never = v; }
"#,
    );
    assert!(
        msg.contains("Type 'unknown' is not assignable to type 'never'"),
        "an un-narrowed unknown operand must still display 'unknown'. Got: {msg}"
    );
}

#[test]
fn unnarrowed_any_operand_still_displays_any() {
    let msg = first_2322_msg(
        r#"
function f(v: any) { const r: never = v; }
"#,
    );
    assert!(
        msg.contains("Type 'any' is not assignable to type 'never'"),
        "an un-narrowed any operand must still display 'any'. Got: {msg}"
    );
}

#[test]
fn predicate_narrowed_union_operand_display_unchanged() {
    // A concrete (union) declared type was never repainted; the narrowed
    // member must still display, exactly as before the fix.
    let msg = first_2322_msg(
        r#"
declare function isStr(x: string | number): x is string;
function f(v: string | number) { if (isStr(v)) { const r: never = v; } }
"#,
    );
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "narrowed union member should display 'string'. Got: {msg}"
    );
}
