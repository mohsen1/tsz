//! Per-file `@jsx` / `@jsxFrag` pragma factories must mark the imports
//! they reference as used, so subsequent unused-import checks
//! (TS6133 / TS6192) don't false-positive on the import that brought
//! the factory into scope.
//!
//! Mirrors the `inlineJsxAndJsxFragPragma.tsx` conformance test
//! (subset that doesn't depend on cross-file resolver behavior).

use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::module_resolution::build_module_resolution_maps;

fn check_jsx_react(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let opts = CheckerOptions {
        jsx_mode: JsxMode::React,
        no_unused_locals: true,
        ..CheckerOptions::default()
    };
    let interner = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &interner,
        file_name.to_string(),
        opts,
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn check_jsx_react_files(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let opts = CheckerOptions {
        jsx_mode: JsxMode::React,
        no_unused_locals: true,
        no_lib: true,
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let interner = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &interner,
        file_names[entry_idx].clone(),
        opts,
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
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
