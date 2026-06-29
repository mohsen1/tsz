//! Empty JSX fragments lowered through the automatic runtime must emit a tight
//! `{}` props object, matching tsc byte-for-byte. Issue #14781: the fragment
//! path previously opened the props object unconditionally with `, { ` and
//! closed with `}`, leaving a stray space (`{ }`) when there were no children.
//! The element automatic path always emitted `{}` for the empty case; the
//! fragment path now mirrors it across the ESM, dev, and CommonJS runtimes.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::emitter::JsxEmit;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print_named_with_opts, parse_and_print_named_with_opts};

fn emit_jsx(source: &str, jsx: JsxEmit, target: ScriptTarget) -> String {
    let opts = PrintOptions {
        jsx,
        target,
        ..Default::default()
    };
    parse_and_print_named_with_opts("test.tsx", source, opts)
}

fn emit_jsx_cjs(source: &str, jsx: JsxEmit) -> String {
    let opts = PrintOptions {
        jsx,
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2017,
        ..Default::default()
    };
    parse_and_lower_print_named_with_opts("test.tsx", source, opts)
}

#[test]
fn empty_fragment_esm_emits_tight_empty_props() {
    let output = emit_jsx(
        "const b = <></>;\n",
        JsxEmit::ReactJsx,
        ScriptTarget::ES2017,
    );
    assert!(
        output.contains("_jsx(_Fragment, {})"),
        "Empty fragment must emit a tight `{{}}` props object.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ }"),
        "Empty fragment must not leave a stray space inside the props object.\nOutput:\n{output}"
    );
}

#[test]
fn empty_fragment_dev_emits_tight_empty_props() {
    let output = emit_jsx(
        "const b = <></>;\n",
        JsxEmit::ReactJsxDev,
        ScriptTarget::ES2017,
    );
    assert!(
        output.contains("_jsxDEV(_Fragment, {},"),
        "Empty fragment under react-jsxdev must emit a tight `{{}}` props object.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ }"),
        "Empty fragment (dev) must not leave a stray space.\nOutput:\n{output}"
    );
}

#[test]
fn empty_fragment_cjs_emits_tight_empty_props() {
    let output = emit_jsx_cjs("const b = <></>;\n", JsxEmit::ReactJsx);
    assert!(
        output.contains(".Fragment, {})"),
        "Empty fragment under the CommonJS automatic runtime must emit `{{}}`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ }"),
        "Empty fragment (CJS) must not leave a stray space.\nOutput:\n{output}"
    );
}

#[test]
fn empty_element_and_self_closing_still_emit_tight_props() {
    // Regression guard: the element automatic path was already correct and must
    // stay byte-identical alongside the fragment fix.
    let output = emit_jsx(
        "const a = <div></div>;\nconst c = <span/>;\n",
        JsxEmit::ReactJsx,
        ScriptTarget::ES2017,
    );
    assert!(
        output.contains("_jsx(\"div\", {})"),
        "Empty element must keep emitting a tight `{{}}`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_jsx(\"span\", {})"),
        "Self-closing element must keep emitting a tight `{{}}`.\nOutput:\n{output}"
    );
}

#[test]
fn non_empty_fragment_still_emits_children_props() {
    // Fragments with content keep the spaced `{ children: ... }` shape that tsc
    // emits; the fix only tightened the empty case.
    let output = emit_jsx(
        "const b = <>hello</>;\n",
        JsxEmit::ReactJsx,
        ScriptTarget::ES2017,
    );
    assert!(
        output.contains("_jsx(_Fragment, { children: \"hello\" })"),
        "Non-empty fragment must keep the spaced `{{ children: ... }}` props object.\nOutput:\n{output}"
    );
}

#[test]
fn nested_empty_fragment_children_emit_tight_props() {
    // A fragment that only contains another empty fragment: the inner empty
    // fragment must still tighten while the outer carries it as a child.
    let output = emit_jsx(
        "const b = <><></></>;\n",
        JsxEmit::ReactJsx,
        ScriptTarget::ES2017,
    );
    assert!(
        output.contains("_jsx(_Fragment, { children: _jsx(_Fragment, {}) })"),
        "Nested empty fragment must tighten to `{{}}` inside the outer children.\nOutput:\n{output}"
    );
}
