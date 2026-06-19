//! Diagnostic source-type display for a user-defined type-predicate guard
//! applied to an `unknown` / `any` operand.
//!
//! Structural rule: when a source identifier is explicitly declared `unknown`
//! or `any` and a user-defined `x is T` guard flow-narrows it to a more
//! specific checked type, an assignment / return TS2322 diagnostic must render
//! the **narrowed** checked type (tsc's `typeToString(sourceType)`), not the
//! stale declared top type. Value-level narrowing already worked (the
//! narrowed value is assignable to `T` and exposes `T`'s members); only the
//! assignment-source *display* repainted the source with the declared
//! `unknown` / `any`. The fix lives in the checker's assignment-source
//! diagnostic formatter, which now resolves the narrowed top-type source
//! before the declared-annotation / widening fallbacks run.
//!
//! Anti-hardcoding: the predicate functions and operands are renamed across
//! tests; the decision is keyed on the declared type being a top type and the
//! checked source being a different non-top type, never on identifier text.

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

fn code_messages(source: &str) -> Vec<(u32, String)> {
    crate::test_utils::check_source_with_libs_code_messages(source, "test.ts", opts(), &libs())
}

fn messages_for(source: &str, code: u32) -> Vec<String> {
    code_messages(source)
        .into_iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m)
        .collect()
}

// ---------------------------------------------------------------------------
// Reported repro (#14009): the `never`-assignment reveal of the narrowed type.
// ---------------------------------------------------------------------------

#[test]
fn unknown_predicate_guard_displays_narrowed_string_in_ts2322() {
    let source = r#"
declare function isStr(x: unknown): x is string;
function a(v: unknown) { if (isStr(v)) { const r: never = v; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'") && !msgs[0].contains("'unknown'"),
        "expected narrowed 'string' source, got {:?}",
        msgs[0]
    );
}

#[test]
fn unknown_predicate_guard_displays_object_intrinsic_in_ts2322() {
    let source = r#"
declare function isObj(x: unknown): x is object;
function b(v: unknown) { if (isObj(v)) { const r: never = v; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'object'") && !msgs[0].contains("'unknown'"),
        "expected narrowed 'object' source, got {:?}",
        msgs[0]
    );
}

#[test]
fn unknown_predicate_guard_displays_narrowed_generic_application_in_ts2322() {
    // The narrowed type is a generic application (`Record<string, unknown>`),
    // not an intrinsic — its display flows through the `TypeMismatch` render
    // arm and the declared-generic-alias source rewrite, both of which must
    // keep the narrowed application rather than repaint the declared top type.
    let source = r#"
declare function isRec(x: unknown): x is Record<string, unknown>;
function d(v: unknown) { if (isRec(v)) { const r: never = v; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("Record<string, unknown>") && !msgs[0].contains("'unknown'"),
        "expected narrowed 'Record<string, unknown>' source, got {:?}",
        msgs[0]
    );
}

#[test]
fn any_predicate_guard_displays_narrowed_string_in_ts2322() {
    let source = r#"
declare function isStrA(x: any): x is string;
function e(v: any) { if (isStrA(v)) { const r: never = v; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'") && !msgs[0].contains("'any'"),
        "expected narrowed 'string' source, got {:?}",
        msgs[0]
    );
}

// ---------------------------------------------------------------------------
// Adjacent: return position and a non-`never` failing target use the same
// assignment-source formatter, so both must show the narrowed source.
// ---------------------------------------------------------------------------

#[test]
fn unknown_predicate_guard_displays_narrowed_string_in_return_ts2322() {
    let source = r#"
declare function isStr(x: unknown): x is string;
function ret(v: unknown): number { if (isStr(v)) { return v; } return 0; }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'") && !msgs[0].contains("'unknown'"),
        "expected narrowed 'string' return source, got {:?}",
        msgs[0]
    );
}

#[test]
fn unknown_predicate_guard_displays_narrowed_string_against_number_target() {
    let source = r#"
declare function isStr(x: unknown): x is string;
function k(v: unknown) { if (isStr(v)) { const r: number = v; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'") && !msgs[0].contains("'unknown'"),
        "expected narrowed 'string' source against number target, got {:?}",
        msgs[0]
    );
}

// ---------------------------------------------------------------------------
// Anti-hardcoding: renamed binders behave identically.
// ---------------------------------------------------------------------------

#[test]
fn unknown_predicate_guard_display_is_not_name_keyed() {
    let source = r#"
declare function looksLikeText(payload: unknown): payload is string;
function consume(blob: unknown) { if (looksLikeText(blob)) { const sink: never = blob; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'") && !msgs[0].contains("'unknown'"),
        "expected narrowed 'string' source, got {:?}",
        msgs[0]
    );
}

// ---------------------------------------------------------------------------
// Value-level narrowing must keep working (the relation already used the
// narrowed type): a narrowed source assignable to its predicate type is clean,
// and a narrowed source exposes the predicate type's members.
// ---------------------------------------------------------------------------

#[test]
fn unknown_predicate_guard_narrowed_value_is_assignable_to_predicate_type() {
    let source = r#"
declare function isStr(x: unknown): x is string;
function ok(v: unknown) { if (isStr(v)) { const s: string = v; } }
"#;
    assert!(
        code_messages(source).is_empty(),
        "narrowed value should be assignable to its predicate type, got {:?}",
        code_messages(source)
    );
}

#[test]
fn unknown_predicate_guard_narrowed_value_exposes_predicate_members() {
    // Property access on the narrowed value resolves against `string` (a
    // genuinely-missing member reports TS2339 against `string`, not TS18046
    // against `unknown`).
    let source = r#"
declare function isStr(x: unknown): x is string;
function members(v: unknown) { if (isStr(v)) { v.nonExistent(); } }
"#;
    let msgs = messages_for(source, 2339);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'"),
        "expected receiver rendered as narrowed 'string', got {:?}",
        msgs[0]
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback: an un-narrowed `unknown` source still shows 'unknown'.
// The fix must not blanket-rewrite every `unknown` source display.
// ---------------------------------------------------------------------------

#[test]
fn unnarrowed_unknown_source_still_displays_unknown() {
    let source = r#"
function plain(v: unknown) { const r: string = v; }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'unknown'"),
        "un-narrowed unknown source must still render 'unknown', got {:?}",
        msgs[0]
    );
}

// ---------------------------------------------------------------------------
// Control: union operands already rendered the narrowed member; keep that.
// ---------------------------------------------------------------------------

#[test]
fn union_predicate_guard_still_displays_narrowed_member() {
    let source = r#"
declare function isS(x: string | number): x is string;
function c(v: string | number) { if (isS(v)) { const r: never = v; } }
"#;
    let msgs = messages_for(source, 2322);
    assert_eq!(msgs.len(), 1, "got {:?}", code_messages(source));
    assert!(
        msgs[0].contains("'string'"),
        "union operand should still render narrowed 'string', got {:?}",
        msgs[0]
    );
}
