//! Regressions for issue #3117 — `noUnusedLocals` must not blanket-suppress
//! every symbol named `React` in classic / preserve JSX mode.
//!
//! tsc only treats the JSX factory binding as referenced when the file
//! actually contains JSX *and* the binding is the import alias the JSX
//! emit/checking depends on. Unrelated locals named `React`, and unused
//! `React` imports in files without JSX, must still report TS6133.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;
use tsz_common::checker_options::{CheckerOptions, JsxMode};

fn react_options() -> CheckerOptions {
    CheckerOptions {
        no_unused_locals: true,
        jsx_mode: JsxMode::React,
        ..Default::default()
    }
}

fn preserve_options() -> CheckerOptions {
    CheckerOptions {
        no_unused_locals: true,
        jsx_mode: JsxMode::Preserve,
        ..Default::default()
    }
}

fn ts6133_react_count(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'React'"))
        .count()
}

// -- 1. Unused `import React` in a file without JSX must still report TS6133.

#[test]
fn unused_react_import_in_non_jsx_file_reports_ts6133_classic() {
    // Mirrors `imported.ts` from the issue reproduction: an unused default
    // import named `React` in a `.ts` file that contains no JSX. tsc reports
    // TS6133; tsz used to silently swallow it because of the name-only skip.
    let source = r#"
import React from "react";
export const value = 1;
"#;
    let diagnostics = check_source(source, "imported.ts", react_options());

    assert_eq!(
        ts6133_react_count(&diagnostics),
        1,
        "Expected one TS6133 for unused `React` import in non-JSX file under classic JSX mode, got: {diagnostics:?}"
    );
}

#[test]
fn unused_react_import_in_non_jsx_file_reports_ts6133_preserve() {
    let source = r#"
import React from "react";
export const value = 1;
"#;
    let diagnostics = check_source(source, "imported.ts", preserve_options());

    assert_eq!(
        ts6133_react_count(&diagnostics),
        1,
        "Expected one TS6133 for unused `React` import in non-JSX file under preserve JSX mode, got: {diagnostics:?}"
    );
}

// -- 2. Unrelated local `React` binding must always report TS6133.

#[test]
fn unused_local_named_react_reports_ts6133_in_classic_mode_without_jsx() {
    // Mirrors `index.ts` from the issue reproduction: a top-level
    // `const React = 1` with no JSX. The local is not the JSX factory
    // dependency — tsc reports TS6133.
    let source = r#"
export {};

const React = 1;
"#;
    let diagnostics = check_source(source, "index.ts", react_options());

    assert_eq!(
        ts6133_react_count(&diagnostics),
        1,
        "Expected TS6133 for unused local `const React = 1` (file without JSX, classic mode), got: {diagnostics:?}"
    );
}

#[test]
fn unused_local_named_react_reports_ts6133_even_when_file_has_jsx() {
    // The classic-mode skip must only apply to the JSX factory *import*,
    // not to a same-named local. Even when JSX is present, an unrelated
    // `const React = 1` is still unused and must report TS6133.
    //
    // Note: the local `const React = 1` shadows any imported `React` in
    // its scope; we put it in a nested block so JSX in the outer module
    // body still resolves. tsc still flags the inner local as unused.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { div: any; }
}
declare const ReactImport: { createElement(...args: any[]): any };
export const tree = (function () {
    const React = ReactImport;
    return React;
})();

function unrelated() {
    const React = 1;
}
unrelated();

export const view = <div />;
"#;
    let diagnostics = check_source(source, "view.tsx", react_options());

    // The inner `const React = 1` in `unrelated()` is unused.
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 6133 && d.message_text.contains("'React'")),
        "Expected TS6133 for unused inner local `const React = 1` even when file has JSX, got: {diagnostics:?}"
    );
}

// -- 3. Valid JSX factory-use control: `import React` IS suppressed when
//       the file contains JSX in classic / preserve mode.

#[test]
fn used_react_import_in_jsx_file_does_not_report_ts6133_classic() {
    // Control: when the file contains JSX and JSX mode is classic, the
    // `React` factory import is implicitly referenced by JSX emit. tsc
    // does not report TS6133 here, and neither should we.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { div: any; }
}
import React from "react";
export const view = <div />;
"#;
    let diagnostics = check_source(source, "view.tsx", react_options());

    assert_eq!(
        ts6133_react_count(&diagnostics),
        0,
        "Expected no TS6133 for `React` factory import in a JSX file (classic mode), got: {diagnostics:?}"
    );
}

#[test]
fn used_react_import_in_jsx_file_does_not_report_ts6133_preserve() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { div: any; }
}
import React from "react";
export const view = <div />;
"#;
    let diagnostics = check_source(source, "view.tsx", preserve_options());

    assert_eq!(
        ts6133_react_count(&diagnostics),
        0,
        "Expected no TS6133 for `React` factory import in a JSX file (preserve mode), got: {diagnostics:?}"
    );
}

// -- 4. react-jsx (automatic runtime) must always report unused React imports.

#[test]
fn unused_react_import_under_automatic_runtime_reports_ts6133_even_with_jsx() {
    // With the automatic runtime, the compiler emits `_jsx`/`_jsxs` from
    // `react/jsx-runtime`, so an explicit `import React from "react"` is
    // not implicitly referenced. tsc reports TS6133.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { div: any; }
}
import React from "react";
export const view = <div />;
"#;
    let diagnostics = check_source(
        source,
        "view.tsx",
        CheckerOptions {
            no_unused_locals: true,
            jsx_mode: JsxMode::ReactJsx,
            ..Default::default()
        },
    );

    assert_eq!(
        ts6133_react_count(&diagnostics),
        1,
        "Expected TS6133 for unused `React` import under react-jsx (automatic) runtime, got: {diagnostics:?}"
    );
}
