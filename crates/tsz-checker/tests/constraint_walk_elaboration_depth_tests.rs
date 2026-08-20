//! The constraint-walk elaboration (`push_deferred_constraint_walk_steps`,
//! wired up by #17786) must seed its per-step `depth` at the same level as
//! any other first-child elaboration line, not one level deeper.
//!
//! Structural rule: both call sites (`render_type_mismatch`'s plain
//! `TypeMismatch` fallthrough and the `IntrinsicTypeMismatch`/
//! `LiteralTypeMismatch` catch-all in `render_failure.rs`) build the
//! diagnostic's own top-level `message_text` themselves and only ask this
//! function to append `tsc`'s per-step walk lines beneath it — both gate the
//! call on `depth == 0`, i.e. the head is unindented, not a related-info
//! entry. The walk's first step is therefore the head's first child and
//! belongs at `depth == 0` (indent 2 in the CLI's `2 * (depth + 1)`
//! formula), exactly like every other first-child elaboration line; deeper
//! steps increment by one per line. `push_deferred_constraint_walk_steps`
//! computed `base_depth + 1 + i`, double-counting that first level and
//! shifting the whole chain by one level (+2 spaces) at every depth (#17797).
//! Fixed at the owning layer (`constraint_walk_display.rs`), no solver
//! change — the solver's `indexed_access_constraint_display_walk` steps are
//! unchanged; only the checker's depth-to-step assignment moved.
//!
//! Oracle-pinned against `typescript@7.0.2` (both `--pretty false`, so the
//! comparison is flag-symmetric with the CLI's plain-mode indent formula).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check_with(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn depths(diags: &[Diagnostic]) -> Vec<u8> {
    diags[0]
        .related_information
        .iter()
        .map(|r| r.depth)
        .collect()
}

/// Single-step walk (`w1.ts`): one elaboration line beneath the head, at
/// depth 0 (CLI indent 2) — oracle: `error TS2322: ... \n  Type 'number' ...`.
#[test]
fn single_step_walk_first_line_is_depth_zero() {
    let diags = check_with(
        r#"
interface Wares3 { p: number; q: number }
function pick3<K extends keyof Wares3>(x: Wares3, k: K) { const y: string = x[k]; }
"#,
    );
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got: {diags:#?}"
    );
    assert_eq!(diags[0].code, 2322);
    assert_eq!(
        diags[0].message_text,
        "Type 'Wares3[K]' is not assignable to type 'string'."
    );
    assert_eq!(depths(&diags), vec![0], "got: {diags:#?}");
    assert_eq!(
        diags[0].related_information[0].message_text,
        "Type 'number' is not assignable to type 'string'."
    );
}

/// Multi-step walk (`w2.ts`): three elaboration lines beneath the head, at
/// depths 0, 1, 2 (CLI indent 2, 4, 6) — oracle-pinned line-for-line.
#[test]
fn multi_step_walk_depths_increment_from_zero() {
    let diags = check_with(
        r#"
function pick<T, K extends keyof T>(x: T, k: K) { const y: string | undefined = x[k]; }
"#,
    );
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got: {diags:#?}"
    );
    assert_eq!(diags[0].code, 2322);
    assert_eq!(
        diags[0].message_text,
        "Type 'T[K]' is not assignable to type 'string | undefined'."
    );
    assert_eq!(depths(&diags), vec![0, 1, 2], "got: {diags:#?}");
    let lines: Vec<&str> = diags[0]
        .related_information
        .iter()
        .map(|r| r.message_text.as_str())
        .collect();
    assert_eq!(
        lines,
        vec![
            "Type 'T[keyof T]' is not assignable to type 'string | undefined'.",
            "Type 'T[string] | T[number] | T[symbol]' is not assignable to type 'string | undefined'.",
            "Type 'T[string]' is not assignable to type 'string | undefined'.",
        ]
    );
}

/// Negative control: an ordinary (non-deferred-constraint) nested-property
/// mismatch is untouched by this fix — its first elaboration line is
/// depth 0, its child depth 1, matching `tsc` exactly (this walk plays no
/// part in this chain at all, so it's a control against a universal depth
/// regression, not a same-code-path witness).
#[test]
fn ordinary_nested_property_chain_depths_unaffected() {
    let diags = check_with(
        r#"
interface A { x: { y: string } }
declare const v: { x: { y: number } };
const a: A = v;
"#,
    );
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got: {diags:#?}"
    );
    assert_eq!(diags[0].code, 2322);
    assert_eq!(depths(&diags), vec![0, 1], "got: {diags:#?}");
    assert_eq!(
        diags[0].related_information[0].message_text,
        "The types of 'x.y' are incompatible between these types."
    );
    assert_eq!(
        diags[0].related_information[1].message_text,
        "Type 'number' is not assignable to type 'string'."
    );
}
