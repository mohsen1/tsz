/// JSX factory imports must not be elided by the AMD/System helper-emission
/// usage check, even when the factory name doesn't textually appear in the
/// source (JSX elements reference it implicitly).
/// Devin review: <https://github.com/mohsen1/tsz/pull/2295#discussion_r3176647570>
#[test]
fn amd_jsx_factory_default_import_kept_in_helpers_check() {
    use crate::emitter::JsxEmit;
    let source = r#"import React from "react";
export const Foo = () => <div/>;
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        jsx: JsxEmit::React,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // The default-import factory ("React") has no textual value usage
    // (only JSX), but because it is a JSX factory we must keep the
    // __importDefault helper definition emitted in the AMD wrapper.
    assert!(
        output.contains("__importDefault"),
        "AMD wrapper should still emit __importDefault helper for JSX factory `React` even without textual value usage.\nOutput:\n{output}"
    );
}

#[test]
fn amd_jsx_factory_named_import_from_pragma_kept_in_helpers_check() {
    use crate::emitter::JsxEmit;
    let source = r#"/** @jsx h */
import { h } from "./renderer";
export const Foo = () => <div/>;
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        jsx: JsxEmit::React,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("\"./renderer\""),
        "AMD wrapper should keep a named import used only as an implicit @jsx factory.\nOutput:\n{output}"
    );
    assert!(
        output.contains("renderer_1.h"),
        "AMD JSX factory call should route through the wrapped import substitution.\nOutput:\n{output}"
    );
}

#[test]
fn system_react_jsx_runtime_dependency_is_wrapped() {
    use crate::emitter::JsxEmit;
    let source = r#"namespace JSX {}
class Component {
render() {
    return <div>{null/* preserved */}</div>;
}
}
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsx,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([\"react/jsx-runtime\"]"),
        "System automatic JSX emit should wrap the synthetic runtime dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var jsx_runtime_1, Component;"),
        "System wrapper should hoist the synthetic JSX runtime binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return _jsx(\"div\", { children: null"),
        "System automatic JSX emit should use the ESM-style JSX helper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import { jsx as _jsx }"),
        "System automatic JSX emit should not leave an ESM import outside the wrapper.\nOutput:\n{output}"
    );
}

#[test]
fn system_react_jsxdev_runtime_dependency_assigns_file_name() {
    use crate::emitter::JsxEmit;
    let source = r#"namespace JSX {}
class Component {
render() {
    return <div>{null/* preserved */}</div>;
}
}
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsxDev,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([\"react/jsx-dev-runtime\"]"),
        "System jsxdev emit should wrap the synthetic dev runtime dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var jsx_dev_runtime_1, _jsxFileName, Component;"),
        "System jsxdev emit should hoist the runtime and file-name bindings.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_jsxFileName = \"test.tsx\";"),
        "System jsxdev emit should assign the source file name inside execute().\nOutput:\n{output}"
    );
    assert!(output.contains("return _jsxDEV(\"div\""));
}

#[test]
fn system_react_jsxdev_runtime_dependency_overrides_stale_file_name_cache() {
    use crate::emitter::JsxEmit;
    let source = r#"namespace JSX {}
class Component {
render() {
    return <div>{null}</div>;
}
}
"#;
    let mut parser = ParserState::new("fresh.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsxDev,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.jsx_dev_file_name = Some("stale.tsx".to_string());
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("_jsxFileName = \"fresh.tsx\";"),
        "System jsxdev emit should always assign the current source file name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_jsxFileName = \"stale.tsx\";"),
        "System jsxdev emit should not reuse stale _jsxFileName values.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_emits_local_export_star_helper() {
    let output = emit_system_es2015(r#"export * from "a";"#);

    assert!(
        output.contains("System.register([\"a\"], function (exports_1, context_1) {"),
        "System export-star modules should register the re-export dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function exportStar_1(m) {"),
        "System export-star modules should emit the local export-star helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (n !== \"default\") exports[n] = m[n];"),
        "Pure export-star modules should only skip default without an exclusion map.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exportStar_1(a_1_1);"),
        "The dependency setter should forward namespace members through exportStar_1.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exportedNames_1"),
        "Pure export-star modules should not emit an exclusion map.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_excludes_local_named_exports() {
    let output = emit_system_es2015(
        r#"export * from "a";
export const x = 1;
"#,
    );

    assert!(
        output.contains("var x;"),
        "The local export should still be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var exportedNames_1 = {\n        \"x\": true\n    };"),
        "Local named exports should be listed in the export-star exclusion map.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "if (n !== \"default\" && !exportedNames_1.hasOwnProperty(n)) exports[n] = m[n];"
        ),
        "Export-star helper should consult the exclusion map when explicit names exist.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"x\", x = 1);"),
        "The local named export should still be published from execute().\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_default_function_uses_empty_exclusion_map() {
    let output = emit_system_es2015(
        r#"export * from "a";
export default function f() {}
"#,
    );

    assert!(
        output.contains("exports_1(\"default\", f);"),
        "The default function should still be hoisted and exported before the setter block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var exportedNames_1 = {};"),
        "Hoisted default function exports should use tsc's empty export-star exclusion map.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "if (n !== \"default\" && !exportedNames_1.hasOwnProperty(n)) exports[n] = m[n];"
        ),
        "The export-star helper should use the empty map shape for hoisted default functions.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_excludes_named_reexports_and_namespace_reexports() {
    let output = emit_system_es2015(
        r#"export * from "a";
export { y as renamed } from "b";
export * as ns from "c";
"#,
    );

    assert!(
        output.contains("System.register([\"a\", \"b\", \"c\"], function (exports_1, context_1) {"),
        "System should preserve re-export dependencies in source order.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "var exportedNames_1 = {\n        \"renamed\": true,\n        \"ns\": true\n    };"
        ),
        "Named and namespace re-exports should be excluded from export-star forwarding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exportStar_1(a_1_1);"),
        "The star re-export dependency should call exportStar_1.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"renamed\": b_2_1[\"y\"]"),
        "The named re-export should still be published from its setter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"ns\", c_3_1);"),
        "The namespace re-export should still be published from its setter.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_matches_mixed_import_reexport_fixture_shape() {
    let output = emit_system_es2015(
        r#"import * as x from "foo";
import * as y from "bar";
export * from "foo";
export * from "bar";
export {x};
export {y};
import {a1, b1, c1 as d1} from "foo";
export {a2, b2, c2 as d2} from "bar";

x,y,a1,b1,d1;
"#,
    );

    assert!(
        output.contains("var x, y, foo_1;"),
        "The mixed import/re-export fixture should hoist namespace imports and named-import module temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "var exportedNames_1 = {\n        \"x\": true,\n        \"y\": true,\n        \"a2\": true,\n        \"b2\": true,\n        \"d2\": true\n    };"
        ),
        "The exclusion map should include local exports and named re-exports, but not star exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("x = x_1;") && output.contains("exportStar_1(x_1);"),
        "The foo setter should assign the namespace import and forward star exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("y = y_1;") && output.contains("exportStar_1(y_1);"),
        "The bar setter should assign the namespace import and forward star exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "exports_1({\n                    \"a2\": y_1[\"a2\"],\n                    \"b2\": y_1[\"b2\"],\n                    \"d2\": y_1[\"c2\"]\n                });"
        ),
        "Named re-exports from bar should remain grouped in the setter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"x\", x);") && output.contains("exports_1(\"y\", y);"),
        "Local namespace re-exports should be published from execute().\nOutput:\n{output}"
    );
    assert!(
        output.contains("x, y, foo_1.a1, foo_1.b1, foo_1.c1;"),
        "Named import references should still substitute through the module temp.\nOutput:\n{output}"
    );
}

/// When a source file contains `/// <amd-module name='X'/>`, the
/// `System.register` call must include `"X"` as the first argument, matching tsc behavior for
/// `--module system` with the `amd-module` pragma.
#[test]
fn system_amd_module_name_directive_names_the_register_call() {
    let source = "/// <amd-module name='NamedModule'/>\nexport function foo() {}\n";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("System.register(\"NamedModule\","),
        "amd-module directive must name the System.register call.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function foo() { }"),
        "Exported function should appear inside the System wrapper.\nOutput:\n{output}"
    );
}

/// The `bundled_module_name` printer option also names the `System.register`
/// call (used for out-file bundled output). The `amd-module` directive takes
/// precedence when both are present.
#[test]
fn system_bundled_module_name_option_names_the_register_call() {
    let source = "export function bar() {}\n";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2015,
            bundled_module_name: Some("BundledModule".to_string()),
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("System.register(\"BundledModule\","),
        "bundled_module_name option must name the System.register call.\nOutput:\n{output}"
    );
}

/// When both `/// <amd-module name='X'/>` and `bundled_module_name` are present,
/// the directive takes precedence (matching tsc behavior for amd-module overriding
/// the bundled name).
#[test]
fn system_amd_module_directive_overrides_bundled_module_name() {
    let source = "/// <amd-module name='DirectiveName'/>\nexport function baz() {}\n";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2015,
            bundled_module_name: Some("BundledName".to_string()),
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("System.register(\"DirectiveName\","),
        "amd-module directive should take precedence over bundled_module_name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\"BundledName\""),
        "bundled_module_name should be suppressed when amd-module directive is present.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_export_destructuring_baseline_check() {
    // Reproduces tests/cases/compiler/systemModule13.ts
    let output = emit_system_es2015(
        r#"export let [x,y,z] = [1, 2, 3];
export const {a: z0, b: {c: z1}} = {a: true, b: {c: "123"}};
for ([x] of [[1]]) {}
"#,
    );
    println!("systemModule13 output:\n{output}");

    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var _a, x, y, z, _b, z0, z1;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            _a = [1, 2, 3], exports_1("x", x = _a[0]), exports_1("y", y = _a[1]), exports_1("z", z = _a[2]);
            _b = { a: true, b: { c: "123" } }, exports_1("z0", z0 = _b.a), exports_1("z1", z1 = _b.b.c);
            for ([x] of [[1]]) { }
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "System module destructuring exports should match tsc baseline.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_array_export_destructuring_uses_temp() {
    // Minimal test: single exported array binding pattern
    let output = emit_system_es2015("export let [x, y] = [1, 2];\n");
    println!("minimal array destructuring output:\n{output}");
    assert!(
        output.contains("var _a, x, y;"),
        "System module should hoist temp before bound names.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = [1, 2], exports_1(\"x\", x = _a[0]), exports_1(\"y\", y = _a[1])"),
        "System module should use temp to publish each element via exports_1.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_array_export_destructuring_reuses_identifier_source() {
    let output = emit_system_es2015("declare const arr: any;\nexport let [x, y] = arr;\n");
    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var x, y;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            exports_1("x", x = arr[0]), exports_1("y", y = arr[1]);
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "Reusable System module destructuring sources should not allocate an RHS temp.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_nested_object_export_destructuring_reuses_identifier_source() {
    let output =
        emit_system_es2015("declare const obj: any;\nexport const {a: {c}, b: d} = obj;\n");
    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var c, d;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            exports_1("c", c = obj.a.c), exports_1("d", d = obj.b);
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "Nested System module destructuring should publish direct reusable source paths.\nOutput:\n{output}"
    );
}
