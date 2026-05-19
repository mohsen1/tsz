//! Tests for the `@jsx` factory pragma overriding the global `--jsx` compiler option.
//!
//! Rule: when a file has a `/** @jsx h */` pragma (with no `@jsxRuntime`), tsc
//! forces classic emit regardless of the `--jsx` compiler option.  This applies
//! to both the JSX transformation itself *and* to import-elision decisions (the
//! factory root must be treated as a value reference so it is not elided).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::emitter::{JsxEmit, PrinterOptions};

#[path = "test_support.rs"]
mod test_support;

use test_support::emit_named_with_printer_options;

fn esm_react_jsx(source: &str) -> String {
    emit_named_with_printer_options(
        "test.tsx",
        source,
        PrinterOptions {
            jsx: JsxEmit::ReactJsx,
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2022,
            ..Default::default()
        },
    )
}

fn cjs_react_jsx(source: &str) -> String {
    emit_named_with_printer_options(
        "test.tsx",
        source,
        PrinterOptions {
            jsx: JsxEmit::ReactJsx,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2022,
            ..Default::default()
        },
    )
}

// =============================================================================
// @jsx pragma forces classic mode regardless of --jsx option
// =============================================================================

#[test]
fn jsx_factory_pragma_overrides_react_jsx_to_classic_esm() {
    // @jsx h pragma with --jsx react-jsx should emit h() calls, not _jsx() calls
    let source = r#"/** @jsx h */
import { h } from "preact";
export const el = <div id="x">hello</div>;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("h(\"div\", { id: \"x\" }, \"hello\")"),
        "@jsx h pragma must emit classic h() call; got:\n{output}"
    );
    assert!(
        !output.contains("_jsx"),
        "@jsx h pragma must suppress automatic _jsx emit; got:\n{output}"
    );
    assert!(
        !output.contains("react/jsx-runtime"),
        "@jsx h pragma must suppress automatic runtime import; got:\n{output}"
    );
}

#[test]
fn jsx_factory_pragma_overrides_react_jsx_to_classic_cjs() {
    // Same rule applies for CommonJS output.
    let source = r#"/** @jsx h */
import { h } from "preact";
export const el = <div />;
"#;
    let output = cjs_react_jsx(source);

    assert!(
        !output.contains("jsx_runtime"),
        "@jsx pragma must suppress the CJS automatic runtime require; got:\n{output}"
    );
    // The actual call may use CJS substitution, e.g. preact_1.h(...)
    assert!(
        output.contains("\"div\""),
        "Classic JSX must emit the tag name as a string; got:\n{output}"
    );
    assert!(
        !output.contains("_jsx"),
        "@jsx pragma must suppress automatic _jsx; got:\n{output}"
    );
}

#[test]
fn jsx_factory_pragma_self_closing_uses_classic() {
    let source = r#"/** @jsx h */
import { h } from "preact";
export const a = <img src="x.png" />;
export const b = <hr />;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("h(\"img\","),
        "Self-closing elements must use classic h() call; got:\n{output}"
    );
    assert!(
        output.contains("h(\"hr\","),
        "Self-closing HR must use classic h() call; got:\n{output}"
    );
}

#[test]
fn jsx_factory_pragma_with_fragment_uses_classic() {
    // @jsx h + @jsxFrag F → both factory and fragment factory come from pragmas.
    let source = r#"/** @jsx h */
/** @jsxFrag F */
import { h, F } from "preact";
export const el = <><div>hello</div></>;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("h(F,"),
        "@jsxFrag F pragma must use F as the fragment factory; got:\n{output}"
    );
    assert!(
        output.contains("h(\"div\","),
        "Children inside fragment must still use classic h(); got:\n{output}"
    );
    assert!(
        !output.contains("jsx-runtime"),
        "@jsx pragma must suppress runtime import; got:\n{output}"
    );
}

#[test]
fn jsx_factory_pragma_alternate_name_k_uses_classic() {
    // The rule must be name-agnostic: 'K' is as valid as 'h'.
    let source = r#"/** @jsx K */
import { K } from "./custom";
export const el = <span className="a" />;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("K(\"span\","),
        "@jsx K pragma must use K as the factory; got:\n{output}"
    );
    assert!(
        !output.contains("_jsx"),
        "@jsx K must suppress automatic _jsx; got:\n{output}"
    );
}

// =============================================================================
// Import elision: @jsx pragma factory root must be treated as a value reference
// =============================================================================

#[test]
fn jsx_factory_pragma_import_not_elided_in_esm() {
    // The factory import must survive import elision when it is the JSX factory.
    let source = r#"/** @jsx h */
import { h } from "preact";
export const el = <div />;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("import { h }"),
        "Factory import must be preserved; got:\n{output}"
    );
}

#[test]
fn jsx_factory_pragma_import_not_elided_in_cjs() {
    let source = r#"/** @jsx h */
import { h } from "preact";
export const el = <div />;
"#;
    let output = cjs_react_jsx(source);

    assert!(
        output.contains("require(\"preact\")"),
        "Factory CJS require must be preserved; got:\n{output}"
    );
}

#[test]
fn jsx_factory_pragma_dotted_namespace_preserves_root_import() {
    // @jsx React.createElement + --jsx react-jsx should preserve the React import.
    let source = r#"/** @jsx React.createElement */
import React from "react";
export const el = <div />;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("React.createElement"),
        "@jsx React.createElement must use classic createElement; got:\n{output}"
    );
    assert!(
        output.contains("import React"),
        "React import must be preserved when used as factory root; got:\n{output}"
    );
}

// =============================================================================
// @jsxRuntime absence with @jsx pragma ≠ @jsxRuntime classic with @jsx pragma
// =============================================================================

#[test]
fn jsx_runtime_pragma_absent_but_factory_present_is_classic() {
    // No @jsxRuntime + @jsx h → classic mode (same as @jsxRuntime classic + @jsx h).
    let source = r#"/** @jsx h */
import { h } from "custom";
export const el = <h1>title</h1>;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("h(\"h1\","),
        "Absent @jsxRuntime + @jsx h must still select classic mode; got:\n{output}"
    );
}

#[test]
fn jsx_runtime_classic_plus_factory_pragma_is_classic() {
    // @jsxRuntime classic + @jsx h → classic mode (consistent with the absent case).
    let source = r#"/** @jsxRuntime classic */
/** @jsx h */
import { h } from "custom";
export const el = <h1>title</h1>;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("h(\"h1\","),
        "@jsxRuntime classic + @jsx h must select classic mode; got:\n{output}"
    );
}

#[test]
fn no_factory_pragma_keeps_automatic_emit() {
    // Without @jsx pragma, --jsx react-jsx must still use automatic mode.
    let source = r#"export const el = <div id="y" />;
"#;
    let output = esm_react_jsx(source);

    assert!(
        output.contains("_jsx("),
        "No @jsx pragma must keep automatic _jsx emit; got:\n{output}"
    );
    assert!(
        output.contains("react/jsx-runtime"),
        "No @jsx pragma must inject automatic runtime import; got:\n{output}"
    );
}
