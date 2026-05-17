use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{JsxEmit, Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_print_named_with_opts, parse_source_named};

fn emit_jsx(source: &str, jsx: JsxEmit, target: ScriptTarget) -> String {
    let opts = PrintOptions {
        jsx,
        target,
        ..Default::default()
    };
    parse_and_print_named_with_opts("test.tsx", source, opts)
}

fn emit_jsx_with_printer_options(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = parse_source_named("test.tsx", source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn emit_classic_cjs_jsx(source: &str) -> String {
    emit_jsx_with_printer_options(
        source,
        PrinterOptions {
            jsx: JsxEmit::React,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

#[test]
fn classic_named_import_factory_from_pragma_is_runtime_dependency() {
    let source = r#"/** @jsx dom */
import { dom } from "./renderer";
export const element = <h />;
"#;
    let output = emit_classic_cjs_jsx(source);

    assert!(
        output.contains("const renderer_1 = require(\"./renderer\");"),
        "Named JSX factory import must be preserved as a runtime dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(0, renderer_1.dom)(\"h\", null);"),
        "Classic JSX should call the CommonJS named-import substitution for @jsx dom.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\ndom(\"h\", null);"),
        "Classic JSX factory should not bypass CommonJS import substitution.\nOutput:\n{output}"
    );
}

#[test]
fn classic_renamed_import_factory_uses_imported_property_name() {
    let source = r#"/** @jsx h */
import { dom as h } from "./renderer";
export const element = <h />;
"#;
    let output = emit_classic_cjs_jsx(source);

    assert!(
        output.contains("const renderer_1 = require(\"./renderer\");"),
        "Renamed factory import must be preserved as a runtime dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(0, renderer_1.dom)(\"h\", null);"),
        "CommonJS substitution should use the imported property name, not the local alias.\nOutput:\n{output}"
    );
}

#[test]
fn classic_fragment_factory_reference_uses_substitution_without_indirect_call() {
    let source = r#"/** @jsx h */
/** @jsxFrag Frag */
import { h, Fragment as Frag } from "./renderer";
export const element = <></>;
"#;
    let output = emit_classic_cjs_jsx(source);

    assert!(
        output.contains("(0, renderer_1.h)(renderer_1.Fragment, null);"),
        "Fragment factory is a value argument, so only the call target gets `(0, ...)`.\nOutput:\n{output}"
    );
}

#[test]
fn classic_fragment_factory_pragma_tag_is_case_insensitive() {
    let source = r#"/* @jsx jsx */
/* @jsxfrag null */
import { jsx } from "./renderer";
export const element = <><span /></>;
"#;
    let output = emit_classic_cjs_jsx(source);

    assert!(
        output.contains("(0, renderer_1.jsx)(null, null,"),
        "Lower-case @jsxfrag should be recognized as the classic fragment factory pragma.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("React.Fragment"),
        "Recognized @jsxfrag null must suppress the default React.Fragment factory.\nOutput:\n{output}"
    );
}

#[test]
fn classic_jsx_component_tag_uses_cjs_identifier_substitution() {
    let source = r#"/** @jsx h */
declare const h: any;
export const MySFC = () => <p />;
export class MyClass {}
export const tree = <MySFC><MyClass /></MySFC>;
"#;
    let output = emit_classic_cjs_jsx(source);

    assert!(
        output.contains("exports.tree = h(exports.MySFC, null,"),
        "Inline-exported variable component tags should use the normal CJS exported-var substitution.\nOutput:\n{output}"
    );
    assert!(
        output.contains("h(MyClass, null)"),
        "Exported class component tags remain lexical class references.\nOutput:\n{output}"
    );
    assert!(
        output.contains("h(\"p\", null)"),
        "Intrinsic JSX tags must still emit as string literals.\nOutput:\n{output}"
    );
}

#[test]
fn classic_jsx_namespace_factory_schedules_import_star_helper() {
    let source = r#"/** @jsx React.createElement */
import * as React from "./renderer";
<h></h>;
"#;
    let output = emit_jsx_with_printer_options(
        source,
        PrinterOptions {
            jsx: JsxEmit::React,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            es_module_interop: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var __importStar ="),
        "Namespace JSX factory imports need the importStar helper before module body emission.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var __createBinding ="),
        "The importStar helper depends on createBinding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const React = __importStar(require(\"./renderer\"));"),
        "The namespace import used only by the JSX pragma should be preserved as an interop require.\nOutput:\n{output}"
    );
    assert!(
        output.contains("React.createElement(\"h\", null);"),
        "The classic JSX transform should use the pragma factory.\nOutput:\n{output}"
    );
}

#[test]
fn jsx_runtime_classic_pragma_overrides_automatic_helper_planning() {
    let source = r#"/* @jsxRuntime classic */
import * as React from "react";
export const el = <h />;
"#;
    let output = emit_jsx_with_printer_options(
        source,
        PrinterOptions {
            jsx: JsxEmit::ReactJsx,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            es_module_interop: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var __importStar ="),
        "Classic runtime pragma should schedule namespace-import helpers even under global automatic JSX.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const React = __importStar(require(\"react\"));"),
        "Classic runtime pragma should preserve the React namespace import as the factory value.\nOutput:\n{output}"
    );
    assert!(
        output.contains("React.createElement(\"h\", null);"),
        "Classic runtime pragma should emit createElement calls under global automatic JSX.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("react/jsx-runtime"),
        "Classic runtime pragma must suppress the synthesized automatic runtime import.\nOutput:\n{output}"
    );
}

#[test]
fn jsx_runtime_automatic_pragma_overrides_classic_emit() {
    let source = r#"/* @jsxRuntime automatic */
export const el = <h />;
"#;
    let output = emit_jsx_with_printer_options(
        source,
        PrinterOptions {
            jsx: JsxEmit::React,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("const jsx_runtime_1 = require(\"react/jsx-runtime\");"),
        "Automatic runtime pragma should synthesize the JSX runtime import under global classic JSX.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.el = (0, jsx_runtime_1.jsx)(\"h\", {});"),
        "Automatic runtime pragma should emit jsx runtime calls under global classic JSX.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("React.createElement"),
        "Automatic runtime pragma must suppress classic createElement output.\nOutput:\n{output}"
    );
}

#[test]
fn jsx_runtime_automatic_pragma_tag_is_case_insensitive() {
    let source = r#"/* @jsxruntime automatic */
export const el = <h />;
"#;
    let output = emit_jsx_with_printer_options(
        source,
        PrinterOptions {
            jsx: JsxEmit::React,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("const jsx_runtime_1 = require(\"react/jsx-runtime\");"),
        "Lower-case @jsxruntime should synthesize the automatic JSX runtime import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.el = (0, jsx_runtime_1.jsx)(\"h\", {});"),
        "Lower-case @jsxruntime should select automatic JSX emit.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("React.createElement"),
        "Lower-case @jsxruntime must suppress classic createElement output.\nOutput:\n{output}"
    );
}

#[test]
fn jsx_import_source_pragma_tag_is_case_insensitive() {
    let source = r#"/* @jsximportsource preact */
export const el = <h />;
"#;
    let output = emit_jsx_with_printer_options(
        source,
        PrinterOptions {
            jsx: JsxEmit::ReactJsx,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("const jsx_runtime_1 = require(\"preact/jsx-runtime\");"),
        "Lower-case @jsximportsource should drive the automatic runtime package.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("require(\"react/jsx-runtime\")"),
        "Recognized @jsximportsource preact must suppress the default React runtime import.\nOutput:\n{output}"
    );
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
fn classic_jsx_drops_trailing_line_comment_after_attribute_expression() {
    let source = r#"function f() {
    return (
        <Component
            value={'s'}
            onChange={val => console.log(val)} // attribute note
        />
    );
}"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "React.createElement(Component, { value: 's', onChange: val => console.log(val) })"
        ),
        "Classic JSX transform should emit the attribute object without the trailing line comment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("attribute note"),
        "Trailing line comment after a JSX attribute expression should not leak into transformed JS.\nOutput:\n{output}"
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

#[test]
fn malformed_attribute_spread_value_preserves_empty_initializer() {
    let source = r#"
declare const React: any
declare namespace JSX {
    interface IntrinsicElements {
        [k: string]: any
    }
}

const X: any
const a: any
<X a={...a} />
"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);

    assert!(
        output.contains("React.createElement(X, { a: , a: true });"),
        "Malformed spread attribute value should keep the empty property initializer.\nOutput:\n{output}"
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

#[test]
fn preserve_jsx_reopened_namespace_qualifies_exported_var_tags() {
    let source = r#"namespace M {
    export var X: any;
}
namespace M {
    var y = <X></X>;
}
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::AMD,
        jsx: JsxEmit::Preserve,
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);

    assert!(
        output.contains("var y = <M.X></M.X>;"),
        "Reopened namespace JSX tags must qualify exported values.\nOutput:\n{output}"
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

#[test]
fn react_jsxdev_column_number_uses_utf16_units() {
    // tsc reports `columnNumber` in UTF-16 code units (the same units a JS
    // runtime sees when indexing strings). Source text containing non-ASCII
    // characters before the JSX element must not shift the column past tsc.
    //
    // Layout (1-based UTF-16 columns):
    //   c o n s t   x   =   " 😀 "  ,     y     =     <
    //   1 2 3 4 5 6 7 8 9 10 11 12-13 14 15 16 17 18 19 20 21
    // The astral `😀` occupies UTF-16 columns 12 and 13 (surrogate pair),
    // so the `<` lands at column 21.
    let source = "const x = \"\u{1F600}\", y = <div />;\n";
    let opts = PrintOptions {
        jsx: JsxEmit::ReactJsxDev,
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_and_print_named_with_opts("test.tsx", source, opts);

    assert!(
        output.contains("columnNumber: 21"),
        "Expected UTF-16 columnNumber: 21 for `<` after an emoji, got:\n{output}"
    );
    assert!(
        !output.contains("columnNumber: 23"),
        "columnNumber must not count UTF-8 bytes (would render 23), got:\n{output}"
    );
}

#[test]
fn react_jsxdev_column_number_with_bmp_non_ascii() {
    // BMP non-ASCII characters (here `é`) are one UTF-16 code unit each, so
    // the column count should match the character index even though `é` is
    // two UTF-8 bytes. `<` lands at UTF-16 column 14 here.
    //   c o n s t _ c a f é _  =  _ <
    //   1 2 3 4 5 6 7 8 9 10 11 12 13 14
    let source = "const caf\u{00E9} = <div />;\n";
    let opts = PrintOptions {
        jsx: JsxEmit::ReactJsxDev,
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_and_print_named_with_opts("test.tsx", source, opts);

    assert!(
        output.contains("columnNumber: 14"),
        "Expected columnNumber: 14 for `<` after a BMP non-ASCII identifier, got:\n{output}"
    );
    // UTF-8 byte counting would have produced 15 (`é` is two bytes).
    assert!(
        !output.contains("columnNumber: 15"),
        "columnNumber must not count UTF-8 bytes (would render 15), got:\n{output}"
    );
}

#[test]
fn react_jsxdev_preserves_virtual_src_file_name() {
    let source = "const el = <div />;\n";
    let opts = PrintOptions {
        jsx: JsxEmit::ReactJsxDev,
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_and_print_named_with_opts("/tmp/tsz-emit/.src/preact.tsx", source, opts);

    assert!(
        output.contains("const _jsxFileName = \"/.src/preact.tsx\";"),
        "JSX dev virtual source locations should keep the TypeScript harness path.\nOutput:\n{output}"
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

// Issue #4010: Classic JSX (jsx=react) should honor a per-file `@jsx` pragma
// and use that factory instead of falling back to React.createElement.
#[test]
fn classic_jsx_pragma_overrides_react_create_element() {
    let source = r#"/** @jsx h */
const el = <div id="a" />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2018);
    assert!(
        output.contains("h(\"div\""),
        "Expected the @jsx pragma factory `h` to drive the call, got: {output}"
    );
    assert!(
        !output.contains("React.createElement"),
        "Default React.createElement must not appear when @jsx pragma is set, got: {output}"
    );
}

// Issue #4010: An expression-style pragma value (dot-separated identifier
// chain) is allowed and must be emitted verbatim.
#[test]
fn classic_jsx_pragma_dotted_factory() {
    let source = r#"/** @jsx Preact.h */
const el = <div />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2018);
    assert!(
        output.contains("Preact.h(\"div\""),
        "Expected dotted pragma factory `Preact.h`, got: {output}"
    );
}

// Issue #4010: The pragma must override the `compilerOptions.jsxFactory`
// option per-file, mirroring tsc.
#[test]
fn classic_jsx_pragma_beats_jsx_factory_option() {
    let source = r#"/** @jsx h */
const el = <div />;"#;
    let opts = PrinterOptions {
        jsx: JsxEmit::React,
        target: ScriptTarget::ES2018,
        jsx_factory: Some("React.createElement".to_string()),
        ..Default::default()
    };
    let output = emit_jsx_with_printer_options(source, opts);
    assert!(
        output.contains("h(\"div\""),
        "Expected per-file pragma to override the option, got: {output}"
    );
    assert!(
        !output.contains("React.createElement"),
        "Option-set factory must lose to the pragma, got: {output}"
    );
}

// Issue #4010: Non-leading `@jsx` (e.g. inside the body) must NOT be treated
// as a pragma — only leading block comments before any code are scanned.
#[test]
fn classic_jsx_pragma_ignored_when_not_leading() {
    let source = r#"const dummy = 1;
/** @jsx h */
const el = <div />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2018);
    assert!(
        output.contains("React.createElement(\"div\""),
        "Pragma after code should be ignored, got: {output}"
    );
}
