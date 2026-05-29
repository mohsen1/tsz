//! Regression tests for the namespace-IIFE-parameter false-rename bug.
//!
//! TypeScript emits a namespace as
//! `var N; (function (N) { ... })(N || (N = {}));` and only renames the IIFE
//! parameter (`N` -> `N_1`) when a *binding* declared in the IIFE's own
//! function scope (var/let/const/function/class/parameter/plain
//! `import = ...`) shadows the namespace name. Occurrences that are merely
//! *uses* of the name — qualified references (`N.foo`), callees
//! (`if (N.f())`), expression operands, enum-member names, and
//! export-qualified `export import N = ...` (which emits `N.N = ...` and
//! reuses the parameter) — must NOT trigger a rename.
//!
//! Previously the source-text binding scan treated a name preceded by `(` /
//! `,` as a binding and the `import` keyword as a binding, which misfired on
//! all of the above. The structural rule implemented in
//! `crates/tsz-emitter/src/emitter/declarations/namespace.rs` is:
//!
//! > When the namespace name appears in its body only as a qualified
//! > reference, callee, expression operand, enum-member name, or an
//! > export-qualified `import =`, it does not shadow the IIFE parameter and
//! > the parameter must not be renamed; only a genuine function-scope binding
//! > (var/let/const/function/class/parameter/plain `import =`) forces a
//! > rename.
//!
//! The tests deliberately vary the namespace name so a fix that hardcodes a
//! particular spelling (`TypeScript`, `Harness`, `M`, ...) would not pass.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_parser::ParserState;

fn emit(source: &str) -> String {
    let mut parser = ParserState::new("a.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::None,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

/// Asserts the IIFE parameter for `ns` is emitted bare (no `_1`/`_2` suffix).
fn assert_param_not_renamed(output: &str, ns: &str) {
    assert!(
        output.contains(&format!("(function ({ns}) {{")),
        "expected IIFE parameter `{ns}` to NOT be renamed.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(&format!("(function ({ns}_1)")),
        "expected IIFE parameter `{ns}` to NOT be renamed to `{ns}_1`.\nOutput:\n{output}"
    );
}

/// Asserts the IIFE parameter for `ns` IS renamed (`ns_1`).
fn assert_param_renamed(output: &str, ns: &str) {
    assert!(
        output.contains(&format!("(function ({ns}_1)")),
        "expected IIFE parameter `{ns}` to be renamed to `{ns}_1`.\nOutput:\n{output}"
    );
}

#[test]
fn qualified_reference_only_does_not_rename_param() {
    // The namespace name appears only as a qualified reference / callee in a
    // guarded expression. tsc keeps the IIFE parameter bare.
    for ns in ["TypeScript", "Harness", "Zebra"] {
        let source = format!(
            "namespace {ns} {{\n    export var flag = true;\n    if ({ns}.flag) {{ {ns}.flag = false; }}\n}}\n"
        );
        let output = emit(&source);
        assert_param_not_renamed(&output, ns);
    }
}

#[test]
fn parenthesized_qualified_reference_does_not_rename_param() {
    // `(<N.Foo>x)` / `(N.bar)`: the name is preceded by `(` but followed by
    // `.`, so it is a qualified reference, not a binding.
    for ns in ["TypeScript", "Outer"] {
        let source = format!(
            "namespace {ns} {{\n    export var x: any = 1;\n    export function read() {{ return ({ns}.x); }}\n}}\n"
        );
        let output = emit(&source);
        assert_param_not_renamed(&output, ns);
    }
}

#[test]
fn enum_member_name_equal_to_namespace_does_not_rename_param() {
    // An enum member sharing the namespace name is a property of the enum
    // object, not a function-scope binding. Varying both the namespace and
    // the unrelated enum name proves the rule is structural.
    for (ns, en) in [("TypeScript", "Reservation"), ("Color", "Palette")] {
        let source = format!(
            "namespace {ns} {{\n    export enum {en} {{\n        None = 0,\n        Other = 1,\n        {ns} = 4,\n    }}\n}}\n"
        );
        let output = emit(&source);
        assert_param_not_renamed(&output, ns);
    }
}

#[test]
fn export_import_equals_does_not_rename_param() {
    // `export import M = Z.M` emits as `M.M = Z.M` and reuses the IIFE
    // parameter, so no rename. Uses a dotted namespace `A.M` like the
    // conformance witness, plus a renamed variant.
    for (outer, alias, target_ns) in [("A", "M", "Z"), ("Outer", "Mid", "Src")] {
        let source = format!(
            "namespace {target_ns}.{alias} {{\n    export function bar() {{ return \"\"; }}\n}}\nnamespace {outer}.{alias} {{\n    export import {alias} = {target_ns}.{alias};\n    export function bar() {{}}\n    {alias}.bar();\n}}\n"
        );
        let output = emit(&source);
        // The innermost IIFE parameter for the `alias` sub-namespace must not
        // be renamed; it should emit `<alias>.<alias> = <target>.<alias>;`.
        assert!(
            output.contains(&format!("{alias}.{alias} = {target_ns}.{alias};")),
            "expected `export import {alias} = {target_ns}.{alias}` to emit \
             `{alias}.{alias} = {target_ns}.{alias};` and reuse the IIFE \
             parameter.\nOutput:\n{output}"
        );
        assert!(
            !output.contains(&format!("{alias}_1")),
            "export-qualified import-equals must not rename the IIFE \
             parameter to `{alias}_1`.\nOutput:\n{output}"
        );
    }
}

#[test]
fn inner_local_var_binding_does_rename_param() {
    // Negative/control: a genuine `var` binding inside an inner function shadows
    // the namespace name in the IIFE scope, so tsc DOES rename the parameter.
    for ns in ["M", "Box"] {
        let source = format!(
            "namespace {ns} {{\n    export var x = 1;\n    function inner() {{\n        var {ns};\n        var p = x;\n        return {ns};\n    }}\n}}\n"
        );
        let output = emit(&source);
        assert_param_renamed(&output, ns);
    }
}

#[test]
fn inner_function_parameter_does_rename_param() {
    // A nested function parameter sharing the namespace name is a genuine
    // binding; tsc renames the IIFE parameter.
    for ns in ["M", "Widget"] {
        let source = format!(
            "namespace {ns} {{\n    export var x = 3;\n    function fn({ns}, p) {{ return p; }}\n}}\n"
        );
        let output = emit(&source);
        assert_param_renamed(&output, ns);
    }
}

#[test]
fn inner_function_declaration_does_rename_param() {
    // A nested `function N()` declaration is a binding even though `N` is
    // followed by `(`; the function keyword makes it a declaration, not a
    // callee. tsc renames the IIFE parameter.
    for ns in ["M", "Gadget"] {
        let source = format!(
            "namespace {ns} {{\n    export var x = 1;\n    function outer() {{\n        function {ns}() {{ return x; }}\n        return {ns};\n    }}\n}}\n"
        );
        let output = emit(&source);
        assert_param_renamed(&output, ns);
    }
}
