//! Tests for standard-library wiring in the LSP `Project`.
//!
//! These exercise the architectural fix for the LSP-vs-CLI divergence where the
//! project bound only the user's file and never merged the standard library, so
//! global values/types were unresolved (spurious `TS2304`, `error` hovers,
//! missing completions). A synthetic lib is used — the `tsz-lsp` crate has no
//! access to the embedded lib assets, which live in `tsz-core` — which is
//! sufficient to prove the wiring end-to-end: binding merges the lib globals,
//! the checker is seeded with the lib contexts, and the hover / completion
//! providers consume them.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;
use tsz_common::position::Position;

use super::Project;

/// A minimal synthetic standard library declaring global values. Binder names
/// are arbitrary on purpose — the wiring must not key off any specific
/// identifier or file name.
const SYNTHETIC_LIB: &str = "
interface Widget { readonly id: number; }
interface WidgetConstructor { new (): Widget; }
declare var Widget: WidgetConstructor;
declare var GLOBAL_FLAG: boolean;
declare function globalGreet(): string;
";

fn synthetic_lib() -> Arc<LibFile> {
    Arc::new(LibFile::from_source(
        "lib.synthetic.d.ts".to_string(),
        SYNTHETIC_LIB.to_string(),
    ))
}

fn project_with_lib() -> Project {
    let mut project = Project::new();
    project.set_lib_files(vec![synthetic_lib()]);
    project
}

#[test]
fn lib_global_resolves_no_ts2304_with_libs() {
    let mut project = project_with_lib();
    project.set_file("f.ts".to_string(), "const f = GLOBAL_FLAG;\n".to_string());

    let diagnostics = project.get_diagnostics("f.ts").expect("diagnostics");
    assert!(
        !diagnostics.iter().any(|d| d.code == Some(2304)),
        "a global declared by the installed lib must resolve, got {diagnostics:?}",
    );
}

#[test]
fn hover_resolves_lib_typed_value() {
    let mut project = project_with_lib();
    // Hover directly on the lib-declared global reference.
    project.set_file("f.ts".to_string(), "GLOBAL_FLAG;\n".to_string());

    let hover = project
        .get_hover("f.ts", Position::new(0, 1))
        .expect("hover");
    assert!(
        hover.display_string.contains("boolean"),
        "hover should reflect the lib-derived type, got {:?}",
        hover.display_string,
    );
    assert!(
        !hover.display_string.contains("error"),
        "hover must not render as `error` once libs resolve, got {:?}",
        hover.display_string,
    );
}

#[test]
fn hover_without_libs_does_not_resolve_lib_type() {
    // Contrast: with no libs the same global is unknown, so its type cannot be
    // `boolean` (this is the pre-fix `error`/`any` behavior).
    let mut project = Project::new();
    project.set_file("f.ts".to_string(), "GLOBAL_FLAG;\n".to_string());

    let hover = project.get_hover("f.ts", Position::new(0, 1));
    let display = hover.map(|h| h.display_string).unwrap_or_default();
    assert!(
        !display.contains("boolean"),
        "without libs the lib type must not resolve, got {display:?}",
    );
}

#[test]
fn completions_include_lib_globals_when_installed_after_open() {
    // Open the file first, then install libs: `set_lib_files` must re-bind the
    // already-loaded file and the completion provider must surface the lib's
    // global values. The source is a bare identifier so the cursor sits in an
    // expression position where global completions are offered.
    let mut project = Project::new();
    project.set_file("f.ts".to_string(), "zz".to_string());
    project.set_lib_files(vec![synthetic_lib()]);

    let completions = project
        .get_completions("f.ts", Position::new(0, 2))
        .expect("completions");
    assert!(
        completions.iter().any(|item| item.label == "GLOBAL_FLAG"),
        "lib-declared global value should appear in completions after libs install",
    );
    assert!(
        completions.iter().any(|item| item.label == "globalGreet"),
        "lib-declared global function should appear in completions",
    );
}

#[test]
fn completions_omit_lib_globals_without_libs() {
    // Without libs the synthetic globals are unknown; the hardcoded es5 fallback
    // list does not contain them either.
    let mut project = Project::new();
    project.set_file("f.ts".to_string(), "zz".to_string());

    let completions = project
        .get_completions("f.ts", Position::new(0, 2))
        .expect("completions");
    assert!(
        !completions.iter().any(|item| item.label == "GLOBAL_FLAG"),
        "unknown global must not appear without libs",
    );
}

#[test]
fn set_lib_files_is_idempotent_for_same_set() {
    // Installing the same `Arc` set twice should be a cheap no-op and must not
    // disturb already-correct resolution.
    let lib = synthetic_lib();
    let mut project = Project::new();
    project.set_lib_files(vec![Arc::clone(&lib)]);
    project.set_file("f.ts".to_string(), "const f = GLOBAL_FLAG;\n".to_string());
    project.set_lib_files(vec![Arc::clone(&lib)]);

    let diagnostics = project.get_diagnostics("f.ts").expect("diagnostics");
    assert!(
        !diagnostics.iter().any(|d| d.code == Some(2304)),
        "re-installing the same lib set must not regress resolution, got {diagnostics:?}",
    );
    assert!(project.has_lib_files());
}
