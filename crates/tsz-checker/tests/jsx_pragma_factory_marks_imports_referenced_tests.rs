//! Per-file `@jsx` / `@jsxFrag` pragma factories must mark the imports
//! they reference as used, so subsequent unused-import checks
//! (TS6133 / TS6192) don't false-positive on the import that brought
//! the factory into scope.
//!
//! Mirrors the `inlineJsxAndJsxFragPragma.tsx` conformance test
//! (subset that doesn't depend on cross-file resolver behavior).

use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

fn check_jsx_react(source: &str, file_name: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        file_name,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            no_unused_locals: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_jsx_react_files(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            no_unused_locals: true,
            no_lib: true,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// `<><div></div></>` with `@jsx h, @jsxFrag Fragment` and both factories
/// imported. Both `h` and `Fragment` are used: `h` for the `<div>`,
/// `Fragment` for the `<>...</>`. Neither should be flagged TS6133 /
/// TS6192.
#[test]
fn pragma_factory_for_fragment_with_intrinsic_marks_both_factories_referenced() {
    let source = "\
/**
 * @jsx h
 * @jsxFrag Fragment
 */
declare function h(): void;
declare function Fragment(): void;
declare namespace JSX {
    interface IntrinsicElements { [e: string]: any; }
    interface Element {}
}
const _frag = <><div></div></>;
";
    let diags = check_jsx_react(source, "preacty.tsx");
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let unused_codes: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|c| matches!(c, 6133 | 6192))
        .collect();
    assert!(
        unused_codes.is_empty(),
        "Expected no TS6133/TS6192 — both `h` and `Fragment` are used by JSX, got: {diags:?}"
    );
}

/// `<></>` with `@jsx h, @jsxFrag null` — `null` is the user-driven
/// opt-out sentinel for the fragment factory, but the JSX factory `h`
/// is still conceptually used (fragments compile to `h(null, …)`).
#[test]
fn pragma_jsx_factory_with_null_jsxfrag_sentinel_still_marks_jsx_factory_referenced() {
    let source = "\
/* @jsx jsx */
/* @jsxfrag null */
declare function jsx(): void;
declare namespace JSX {
    interface IntrinsicElements { [e: string]: any; }
    interface Element {}
}
const _frag = <></>;
";
    let diags = check_jsx_react(source, "snabbdomy-only-fragment.tsx");
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let unused_codes: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|c| matches!(c, 6133 | 6192))
        .collect();
    assert!(
        unused_codes.is_empty(),
        "Expected no TS6133/TS6192 — `jsx` is used by the fragment, got: {diags:?}"
    );
}

#[test]
fn fragment_with_imported_fragment_but_unimported_jsx_factory_reports_ts2874() {
    let diags = check_jsx_react_files(
        &[
            (
                "/renderer.d.ts",
                r#"
export function h(): void;
export function Fragment(): void;
declare global {
    namespace JSX {
        interface IntrinsicElements { [e: string]: any; }
        interface Element {}
    }
}
"#,
            ),
            (
                "/entry.tsx",
                r#"/** @jsx h
 * @jsxFrag Fragment
 */
import { Fragment } from "./renderer";
const _frag = <></>;
"#,
            ),
        ],
        "/entry.tsx",
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2874 && message.contains("'h'")),
        "Expected TS2874 for missing JSX factory `h`, got: {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|(code, message)| *code == 2879 && message.contains("Fragment")),
        "Expected imported fragment factory to be in scope, got: {diags:?}"
    );
}

#[test]
fn fragment_with_null_jsxfrag_and_unimported_jsx_factory_reports_ts2874() {
    let diags = check_jsx_react_files(
        &[
            (
                "/renderer.d.ts",
                r#"
export function jsx(): void;
declare global {
    namespace JSX {
        interface IntrinsicElements { [e: string]: any; }
        interface Element {}
    }
}
"#,
            ),
            (
                "/entry.tsx",
                r#"/** @jsx jsx
 * @jsxfrag null
 */
import {} from "./renderer";
const _frag = <></>;
"#,
            ),
        ],
        "/entry.tsx",
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2874 && message.contains("'jsx'")),
        "Expected TS2874 for missing JSX factory `jsx`, got: {diags:?}"
    );
}
