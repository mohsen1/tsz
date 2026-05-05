use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{JsxEmit, Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_emitter::output::printer::PrintOptions;
use tsz_parser::ParserState;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;

fn emit_jsx(source: &str, jsx: JsxEmit, target: ScriptTarget) -> String {
    let opts = PrintOptions {
        jsx,
        target,
        ..Default::default()
    };
    parse_and_print_named_with_opts("test.tsx", source, opts)
}

fn emit_jsx_with_printer_options(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

// =============================================================================
// Spread flattening: {...{...a, ...b}} → ...a, ...b
// =============================================================================

#[test]
fn classic_spread_flattening_object_assign() {
    // Classic mode, pre-ES2018: {...{...a, ...b}} should flatten into
    // Object.assign({}, a, b) instead of Object.assign({}, {...a, ...b})
    let source = r#"const el = <div {...{...a, ...b}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);
    assert!(
        output.contains("Object.assign({}, a, b)"),
        "Expected flattened Object.assign({{}}, a, b), got: {output}"
    );
}

#[test]
fn classic_spread_flattening_es2018() {
    // Classic mode, ES2018+: {...{...a, ...b}} should flatten into
    // { ...a, ...b } instead of { ...{...a, ...b} }
    let source = r#"const el = <div {...{...a, ...b}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("{ ...a, ...b }"),
        "Expected flattened inline spread, got: {output}"
    );
}

#[test]
fn automatic_spread_flattening() {
    // Automatic JSX mode: {...{...a, ...b}} should flatten spreads
    let source = r#"const el = <div {...{...a, ...b}} />;"#;
    let output = emit_jsx(source, JsxEmit::ReactJsx, ScriptTarget::ESNext);
    assert!(
        output.contains("...a, ...b"),
        "Expected flattened spread in automatic mode, got: {output}"
    );
    // Should NOT contain nested object literal
    assert!(
        !output.contains("...{"),
        "Should not have nested spread-of-object, got: {output}"
    );
}

#[test]
fn no_flatten_when_object_has_non_spread_props() {
    // Object literal with a mix of spread and non-spread props should NOT flatten
    let source = r#"const el = <div {...{...a, x: 1}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    // The object literal should be kept as-is since it has a non-spread property
    assert!(
        output.contains("{ ...{ ...a, x: 1 }"),
        "Mixed props object should not be flattened, got: {output}"
    );
}

#[test]
fn no_flatten_empty_object() {
    // Empty object literal: {...{}} should NOT flatten (nothing to flatten)
    let source = r#"const el = <div {...{}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    // Should keep the empty object spread as-is
    assert!(
        output.contains("{}"),
        "Empty object spread should be preserved, got: {output}"
    );
}

#[test]
fn flatten_single_inner_spread() {
    // Single inner spread: {...{...props}} should flatten to ...props
    let source = r#"const el = <div {...{...props}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("...props"),
        "Single inner spread should flatten, got: {output}"
    );
}

#[test]
fn flatten_preserves_named_attrs() {
    // Named attrs mixed with flattened spread
    let source = r#"const el = <div className="foo" {...{...a, ...b}} id="bar" />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("className"),
        "Named attrs should be preserved, got: {output}"
    );
    assert!(
        output.contains("...a"),
        "Inner spread a should be flattened, got: {output}"
    );
    assert!(
        output.contains("...b"),
        "Inner spread b should be flattened, got: {output}"
    );
    assert!(
        output.contains("id"),
        "Named attr id should be preserved, got: {output}"
    );
}

#[test]
fn classic_spread_no_flatten_variable() {
    // Regular spread (not an object literal): {...props} should NOT try to flatten
    let source = r#"const el = <div {...props} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("props"),
        "Regular spread should pass through, got: {output}"
    );
}

// =============================================================================
// Target-appropriate spread prop handling (committed in 8717f7d)
// =============================================================================

#[test]
fn classic_object_assign_pre_es2018() {
    // Classic mode with pre-ES2018 target should use Object.assign
    let source = r#"const el = <div className="test" {...props} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);
    assert!(
        output.contains("Object.assign("),
        "Pre-ES2018 classic mode should use Object.assign, got: {output}"
    );
}

#[test]
fn classic_inline_spread_es2018() {
    // Classic mode with ES2018+ target should use inline spread
    let source = r#"const el = <div className="test" {...props} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        !output.contains("Object.assign"),
        "ES2018+ classic mode should NOT use Object.assign, got: {output}"
    );
    assert!(
        output.contains("...props"),
        "ES2018+ classic mode should use inline spread, got: {output}"
    );
}

#[test]
fn automatic_object_assign_pre_es2018() {
    // Automatic JSX mode with pre-ES2018 target should use Object.assign for spreads
    let source = r#"const el = <div className="test" {...props}>child</div>;"#;
    let output = emit_jsx(source, JsxEmit::ReactJsx, ScriptTarget::ES2015);
    assert!(
        output.contains("Object.assign("),
        "Pre-ES2018 automatic mode with spreads should use Object.assign, got: {output}"
    );
}

#[test]
fn automatic_inline_spread_es2018() {
    // Automatic JSX mode with ES2018+ target should use inline spread
    let source = r#"const el = <div className="test" {...props}>child</div>;"#;
    let output = emit_jsx(source, JsxEmit::ReactJsx, ScriptTarget::ESNext);
    assert!(
        !output.contains("Object.assign"),
        "ES2018+ automatic mode should NOT use Object.assign, got: {output}"
    );
}

/// Regression: a JSX spread child whose argument is wrapped in parens for
/// an erased type cast (`(x as any)`) must emit as `...x`, not `...(x)`.
/// tsc strips the parens because they only existed to delimit the cast.
#[test]
fn classic_spread_child_unwraps_erased_type_cast_parens() {
    let source = "declare const Todo: any;\nconst el = <div>{...(<Todo /> as any)}</div>;";
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);
    assert!(
        output.contains("...React.createElement(Todo,"),
        "Spread JSX child must unwrap parens around erased type cast.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("...(React.createElement(Todo,"),
        "Spread JSX child must not keep the now-unnecessary outer parens.\nOutput:\n{output}"
    );
}

/// Counterpart: a spread of a plain parenthesized expression (without a
/// cast) also unwraps. `...(expr)` is equivalent to `...expr`, and tsc
/// emits the unparenthesized form.
#[test]
fn classic_spread_child_unwraps_plain_parentheses() {
    let source = "declare const arr: any[];\nconst el = <div>{...(arr)}</div>;";
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);
    assert!(
        output.contains("...arr"),
        "Spread JSX child should unwrap redundant outer parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("...(arr)"),
        "Redundant outer parens must not survive.\nOutput:\n{output}"
    );
}

// =============================================================================
// moduleDetection=legacy + JSX automatic runtime
// =============================================================================

/// Regression: `moduleDetection: "legacy"` keeps a non-module file as a
/// script even when it uses JSX. tsc emits the `_jsx(...)` calls bare —
/// no `import { jsx as _jsx } from "react/jsx-runtime"` is added,
/// because adding it would silently promote the file to an ES module.
#[test]
fn react_jsx_under_module_detection_legacy_skips_runtime_import() {
    let source = "namespace JSX {}\nclass Component {\n    render() { return <div />; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsx,
        module_detection_legacy: true,
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);

    assert!(
        !output.contains("react/jsx-runtime"),
        "Legacy detection must not auto-add the JSX runtime import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_jsx(\"div\""),
        "JSX call must still emit (referencing _jsx as undefined globals).\nOutput:\n{output}"
    );
}

#[test]
fn react_jsx_under_module_detection_legacy_commonjs_uses_bare_runtime_alias() {
    let source = "namespace JSX {}\nclass Component {\n    render() { return <div />; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        jsx: JsxEmit::ReactJsx,
        module_detection_legacy: true,
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);

    assert!(
        !output.contains("react/jsx-runtime"),
        "Legacy detection must not synthesize a JSX runtime require.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(0, _a.jsx)(\"div\""),
        "CommonJS legacy JSX calls should use the bare runtime alias.\nOutput:\n{output}"
    );
}

#[test]
fn react_jsxdev_under_module_detection_legacy_emits_file_name_without_import() {
    let source = "namespace JSX {}\nclass Component {\n    render() { return <div />; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsxDev,
        module_detection_legacy: true,
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);

    assert!(
        !output.contains("react/jsx-dev-runtime"),
        "Legacy detection must not synthesize a JSX dev runtime import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const _jsxFileName = \"test.tsx\";"),
        "JSX dev source locations still need the file-name constant.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_jsxDEV(\"div\", {"),
        "System legacy JSX dev calls should stay as bare _jsxDEV references.\nOutput:\n{output}"
    );
}

/// Counterpart: with `moduleDetection: "auto"` (default) and the same
/// non-module-syntax file, the JSX runtime import IS added (which then
/// makes the file an ES module).
#[test]
fn react_jsx_under_module_detection_auto_adds_runtime_import() {
    let source = "namespace JSX {}\nclass Component {\n    render() { return <div />; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        // module=None to skip the System.register wrapper for this assertion;
        // the import-emission decision is what we want to test.
        module: ModuleKind::None,
        jsx: JsxEmit::ReactJsx,
        module_detection_legacy: false,
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);

    assert!(
        output.contains("from \"react/jsx-runtime\""),
        "Auto detection should auto-add the JSX runtime import.\nOutput:\n{output}"
    );
}

/// Under `module=System` + legacy detection, tsc still emits a top-level
/// `"use strict";` even though the file is a non-module script. This is
/// because System modules imply strict mode.
#[test]
fn react_jsx_under_module_detection_legacy_system_emits_use_strict() {
    let source = "namespace JSX {}\nclass Component {\n    render() { return <div />; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsx,
        module_detection_legacy: true,
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);

    assert!(
        output.starts_with("\"use strict\";"),
        "module=System + legacy detection on a non-module file must still emit `use strict`.\nOutput:\n{output}"
    );
}
