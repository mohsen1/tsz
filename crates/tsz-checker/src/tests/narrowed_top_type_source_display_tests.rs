//! `TS2322`/`TS2741` source display for a top-type (`any`/`unknown`) operand
//! that flow analysis narrowed to a concrete type.
//!
//! Structural rule: when an identifier declared as a top type (`any` or
//! `unknown`) is narrowed by control flow — a user-defined type-predicate guard,
//! `typeof`, `instanceof`, a discriminant, etc. — to a concrete type, an
//! assignability diagnostic on that operand renders the **narrowed** type, the
//! way `tsc` does (`getFlowTypeOfReference`). `tsz` previously repainted the
//! source with the operand's stale *declaration* annotation (`unknown`/`any`)
//! while the relation itself correctly used the narrowed type, so the diagnostic
//! text diverged from `tsc`. The narrowing and the relation were always correct;
//! only the diagnostic source-type display was wrong.
//!
//! Owner layer: checker diagnostic display
//! (`error_reporter::core::diagnostic_source` /
//! `error_reporter::assignability_alias_display`). The fix keys only on the
//! structural top-type identity of the source's declared symbol type, never on
//! any identifier, annotation, or printer-output text.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_common::options::checker::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn libs() -> Vec<Arc<LibFile>> {
    crate::test_utils::load_lib_files(&["es5.d.ts"])
}

fn messages(source: &str) -> Vec<(u32, String)> {
    crate::test_utils::check_source_with_libs(source, "test.ts", opts(), &libs())
        .into_iter()
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

/// The single `TS2322` (or `TS2741`) message emitted for `source`.
fn sole_assignment_message(source: &str) -> String {
    let msgs: Vec<String> = messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322 || *code == 2741)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one assignment diagnostic, got {msgs:?}"
    );
    msgs.into_iter().next().unwrap()
}

// ---------------------------------------------------------------------------
// Positive: a top-type operand narrowed by a user-defined predicate renders the
// narrowed type, not the declared `unknown`/`any`.
// ---------------------------------------------------------------------------

#[test]
fn unknown_narrowed_by_predicate_renders_narrowed_type() {
    let source = r#"
declare function isStr(x: unknown): x is string;
function f(v: unknown) {
    if (isStr(v)) {
        const r: never = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "got: {msg}"
    );
    assert!(
        !msg.contains("'unknown'"),
        "stale declared type leaked: {msg}"
    );
}

#[test]
fn any_narrowed_by_predicate_renders_narrowed_type() {
    let source = r#"
declare function isStr(x: any): x is string;
function f(v: any) {
    if (isStr(v)) {
        const r: never = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "got: {msg}"
    );
    assert!(!msg.contains("'any'"), "stale declared type leaked: {msg}");
}

#[test]
fn unknown_narrowed_to_object_intrinsic_renders_object() {
    let source = r#"
declare function isObj(x: unknown): x is object;
function f(v: unknown) {
    if (isObj(v)) {
        const r: never = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'object' is not assignable to type 'never'"),
        "got: {msg}"
    );
}

#[test]
fn unknown_narrowed_to_record_renders_record_in_missing_property() {
    // The narrowed `Record<string, unknown>` source must surface in the TS2741
    // missing-property message, not the declared `unknown` (object-with-index
    // signature source display path).
    let source = r#"
declare function isRec(x: unknown): x is Record<string, unknown>;
function f(v: unknown) {
    if (isRec(v)) {
        const r: { a: number } = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(msg.contains("type 'Record<string, unknown>'"), "got: {msg}");
    assert!(
        !msg.contains("'unknown'"),
        "stale declared type leaked: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Broad: the rule is narrowing-kind agnostic (typeof / instanceof), not specific
// to user-defined predicates.
// ---------------------------------------------------------------------------

#[test]
fn unknown_narrowed_by_typeof_renders_narrowed_type() {
    let source = r#"
function f(v: unknown) {
    if (typeof v === "string") {
        const r: never = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "got: {msg}"
    );
}

#[test]
fn any_narrowed_by_typeof_renders_narrowed_type() {
    let source = r#"
function f(v: any) {
    if (typeof v === "number") {
        const r: never = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'number' is not assignable to type 'never'"),
        "got: {msg}"
    );
    assert!(!msg.contains("'any'"), "stale declared type leaked: {msg}");
}

#[test]
fn unknown_narrowed_by_instanceof_renders_class_name() {
    let source = r#"
class Box {}
function f(v: unknown) {
    if (v instanceof Box) {
        const r: never = v;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'Box' is not assignable to type 'never'"),
        "got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Anti-hardcoding: the result is independent of binder names.
// ---------------------------------------------------------------------------

#[test]
fn renamed_binders_render_identically() {
    let source = r#"
declare function looksLikeText(candidate: unknown): candidate is string;
function process(payload: unknown) {
    if (looksLikeText(payload)) {
        const sink: never = payload;
    }
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'string' is not assignable to type 'never'"),
        "got: {msg}"
    );
    assert!(
        !msg.contains("'unknown'"),
        "stale declared type leaked: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback: an *un-narrowed* top-type operand still renders its
// declared top type, exactly as `tsc` does.
// ---------------------------------------------------------------------------

#[test]
fn unnarrowed_unknown_still_renders_unknown() {
    let source = r#"
function f(v: unknown) {
    const r: string = v;
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'unknown' is not assignable to type 'string'"),
        "got: {msg}"
    );
}

#[test]
fn unnarrowed_any_to_never_still_renders_any() {
    // `any` is assignable to everything except `never`; tsc reports TS2322 for
    // `any` -> `never` and shows the source as `any` when un-narrowed.
    let source = r#"
function f(v: any) {
    const r: never = v;
}
"#;
    let msg = sole_assignment_message(source);
    assert!(
        msg.contains("Type 'any' is not assignable to type 'never'"),
        "got: {msg}"
    );
}
