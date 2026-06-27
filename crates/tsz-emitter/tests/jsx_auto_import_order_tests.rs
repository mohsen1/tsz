//! Automatic JSX runtime (`--jsx react-jsx` / `react-jsxdev`) emits the
//! `react/jsx-runtime` (or `jsx-dev-runtime`) named-import specifiers in the
//! order each helper is FIRST needed during the source walk — matching `tsc`,
//! which inserts an implicit runtime import the first time an element needing it
//! is lowered. Regression coverage for issue #14779 (previously tsz used a fixed
//! `jsx, jsxs, Fragment` order).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::emitter::JsxEmit;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;

/// Emit `source` as an ESM `.tsx` file under the requested automatic runtime and
/// return the single generated runtime import line (or the whole output if none
/// was found, so failures are legible).
fn auto_import_line(source: &str, jsx: JsxEmit) -> String {
    let opts = PrintOptions {
        jsx,
        target: ScriptTarget::ES2017,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_and_print_named_with_opts("test.tsx", source, opts);
    output
        .lines()
        .find(|l| l.contains("jsx-runtime") || l.contains("jsx-dev-runtime"))
        .map(str::to_string)
        .unwrap_or(output)
}

// ---------------------------------------------------------------------------
// react-jsx (production automatic runtime)
// ---------------------------------------------------------------------------

#[test]
fn react_jsx_specifiers_follow_first_reference_order_jsxs_fragment_jsx() {
    // ord1 from the issue: jsxs (multi-child) → Fragment → jsx.
    let source = "const a = <div>{x}{y}</div>;\nconst b = <></>;\nconst c = <p>single</p>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsx);
    assert_eq!(
        line,
        "import { jsxs as _jsxs, Fragment as _Fragment, jsx as _jsx } from \"react/jsx-runtime\";",
        "specifiers must follow first-reference order (jsxs, Fragment, jsx)"
    );
}

#[test]
fn react_jsx_specifiers_follow_first_reference_order_jsx_jsxs_fragment() {
    // jsx (single child) → jsxs (multi-child) → Fragment.
    let source = "const c = <p>single</p>;\nconst a = <div>{x}{y}</div>;\nconst b = <></>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsx);
    assert_eq!(
        line,
        "import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from \"react/jsx-runtime\";",
    );
}

#[test]
fn react_jsx_specifiers_follow_first_reference_order_fragment_jsx_jsxs() {
    // An empty fragment references Fragment first, then its `_jsx` callee; the
    // multi-child element that follows then introduces jsxs.
    let source = "const b = <></>;\nconst a = <div>{x}{y}</div>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsx);
    assert_eq!(
        line,
        "import { Fragment as _Fragment, jsx as _jsx, jsxs as _jsxs } from \"react/jsx-runtime\";",
    );
}

#[test]
fn react_jsx_single_helper_is_unambiguous() {
    let source = "const c = <p>single</p>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsx);
    assert_eq!(line, "import { jsx as _jsx } from \"react/jsx-runtime\";");
}

#[test]
fn react_jsx_nested_elements_register_children_before_parent() {
    // Post-order: the inner multi-child `<span>` (jsxs) is lowered before the
    // single-child `<section>` (jsx) that contains it.
    let source = "const a = <section><span>{x}{y}</span></section>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsx);
    assert_eq!(
        line, "import { jsxs as _jsxs, jsx as _jsx } from \"react/jsx-runtime\";",
        "children must register their helpers before the enclosing parent"
    );
}

// ---------------------------------------------------------------------------
// react-jsxdev (development automatic runtime): jsx/jsxs collapse to jsxDEV
// ---------------------------------------------------------------------------

#[test]
fn react_jsxdev_specifiers_follow_first_reference_order_fragment_first() {
    // devord2 from the issue: Fragment (from `<></>`) precedes jsxDEV.
    let source = "const b = <></>;\nconst a = <p>x</p>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsxDev);
    assert_eq!(
        line,
        "import { Fragment as _Fragment, jsxDEV as _jsxDEV } from \"react/jsx-dev-runtime\";",
    );
}

#[test]
fn react_jsxdev_specifiers_follow_first_reference_order_jsxdev_first() {
    // devord control from the issue: a plain element first → jsxDEV, then Fragment.
    let source = "const a = <p>x</p>;\nconst b = <></>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsxDev);
    assert_eq!(
        line,
        "import { jsxDEV as _jsxDEV, Fragment as _Fragment } from \"react/jsx-dev-runtime\";",
    );
}

// ---------------------------------------------------------------------------
// Custom @jsxImportSource pragma: same ordering on the custom module path.
// ---------------------------------------------------------------------------

#[test]
fn custom_jsx_import_source_keeps_first_reference_order() {
    let source = "/** @jsxImportSource preact */\nconst a = <div>{x}{y}</div>;\nconst b = <></>;\nconst c = <p>single</p>;\n";
    let line = auto_import_line(source, JsxEmit::ReactJsx);
    assert_eq!(
        line,
        "import { jsxs as _jsxs, Fragment as _Fragment, jsx as _jsx } from \"preact/jsx-runtime\";",
    );
}
