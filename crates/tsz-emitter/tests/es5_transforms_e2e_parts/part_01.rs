#[test]
fn async_arrow_import_meta_hoisted_locals_share_var_statement() {
    let output = emit_es5_with_module(
        "(async () => {\n\
             const response = await fetch(new URL(\"../hamsters.jpg\", import.meta.url).toString());\n\
             const blob = await response.blob();\n\
             \n\
             const size = import.meta.scriptElement.dataset.size || 300;\n\
             \n\
             const image = new Image();\n\
             image.src = URL.createObjectURL(blob);\n\
             image.width = image.height = size;\n\
             \n\
             document.body.appendChild(image);\n\
         })();\n",
        ModuleKind::CommonJS,
    );

    assert!(
        output.contains("var response, blob, size, image;"),
        "Async arrow import.meta hoisted locals should share one var statement.\nOutput:\n{output}"
    );
}

#[test]
fn system_import_meta_file_is_wrapped_as_module() {
    let output = emit_es5_with_module(
        "(async () => {\n\
             const response = await fetch(new URL(\"../hamsters.jpg\", import.meta.url).toString());\n\
             const blob = await response.blob();\n\
             \n\
             const size = import.meta.scriptElement.dataset.size || 300;\n\
             \n\
             const image = new Image();\n\
             image.src = URL.createObjectURL(blob);\n\
             image.width = image.height = size;\n\
             \n\
             document.body.appendChild(image);\n\
         })();\n",
        ModuleKind::System,
    );

    assert!(
        output.starts_with("System.register([], function (exports_1, context_1) {"),
        "System import.meta files should be module-wrapped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("context_1.meta.url"),
        "System import.meta should lower to context_1.meta.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"use strict\";\n    var __awaiter"),
        "System async helpers should be emitted inside the wrapper after the strict prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var response, blob, size, image;"),
        "System async arrow hoisted locals should share one var statement.\nOutput:\n{output}"
    );
}

#[test]
fn system_import_meta_preserves_import_property_lookalikes() {
    let output = emit_es5_with_module(
        "export let x = import.meta;\n\
         export let y = import.metal;\n\
         export let z = import.import.import.malkovich;\n",
        ModuleKind::System,
    );

    assert!(
        output.contains("exports_1(\"x\", x = context_1.meta);"),
        "System should lower only real import.meta.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"y\", y = import.metal);"),
        "System should preserve import.metal lookalikes.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"z\", z = import.import.import.malkovich);"),
        "System should preserve nested import property lookalikes.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("context_1.metal") && !output.contains("context_1.import"),
        "System import.meta lowering must not rewrite non-meta import properties.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_while_with_await_lowers_loop_body() {
    let output = emit_es5(
        "async function f(xs) {\n    while (xs.length) {\n        await g(xs.pop());\n    }\n}\n",
    );

    assert!(
        !output.contains("while (xs.length)"),
        "Source while loop should be lowered into generator cases.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "await keyword should not appear in ES5.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!xs.length) return [3 /*break*/, 2];"),
        "Loop condition should branch to the exit case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, g(xs.pop())];"),
        "Await in the loop body should become a generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 0];"),
        "Loop body should jump back to the condition case.\nOutput:\n{output}"
    );
}

// =============================================================================
// Template Literals
// =============================================================================

#[test]
fn test_template_literal_to_concatenation() {
    let output = emit_es5("const msg = `Hello ${name}!`;\n");
    // ES5 should convert template literals to string concatenation
    assert!(
        !output.contains('`'),
        "ES5 should not contain template literal syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("+") || output.contains("concat"),
        "Expected string concatenation.\nOutput:\n{output}"
    );
}

// =============================================================================
// Spread Transform
// =============================================================================

#[test]
fn test_spread_in_call() {
    let output = emit_es5("foo(...args);\n");
    assert!(
        !output.contains("...args"),
        "ES5 should not contain spread syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("apply") || output.contains("__spreadArray"),
        "Expected apply or __spreadArray for spread.\nOutput:\n{output}"
    );
}

// =============================================================================
// Exponentiation Transform (ES2016)
// =============================================================================

#[test]
fn test_exponentiation_to_math_pow() {
    let output = emit_es5("const x = 2 ** 3;\n");
    assert!(
        output.contains("Math.pow"),
        "Expected Math.pow for exponentiation.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("**"),
        "ES5 should not contain ** operator.\nOutput:\n{output}"
    );
}

// =============================================================================
// Enum ES5 Transform
// =============================================================================

#[test]
fn test_enum_to_iife() {
    let output = emit_es5_with_comments("enum Color {\n    Red,\n    Green,\n    Blue\n}\n");
    // Enums become IIFEs in ES5
    assert!(
        output.contains("Color[Color[") || output.contains("Color[\"Red\"]"),
        "Expected enum IIFE pattern.\nOutput:\n{output}"
    );
}

// =============================================================================
// Type Stripping
// =============================================================================

#[test]
fn test_type_annotations_stripped() {
    let output = emit_es5("const x: number = 42;\n");
    assert!(
        !output.contains(": number"),
        "Type annotations should be stripped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("42"),
        "Value should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn test_interface_stripped() {
    let output = emit_es5("interface Point { x: number; y: number; }\n");
    assert!(
        !output.contains("interface"),
        "Interface should be stripped from JS output.\nOutput:\n{output}"
    );
}

#[test]
fn test_type_alias_stripped() {
    let output = emit_es5("type ID = string | number;\n");
    assert!(
        !output.contains("type ID"),
        "Type alias should be stripped from JS output.\nOutput:\n{output}"
    );
}

// Structural rule: when an async function targeting ES5 contains a dynamic
// import call and the module system is CommonJS, the IR transformer must lower
// `import("mod")` to `Promise.resolve().then(function () { return
// __importStar(require("mod")); })` — the same form the regular printer emits.
// The name of the specifier and the presence/absence of other awaits are
// irrelevant to the lowering rule; any string-literal specifier must produce
// the same pattern.

#[test]
fn async_es5_cjs_dynamic_import_lowered_to_promise_resolve_require() {
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./mod\"); }",
        ModuleKind::CommonJS,
    );
    assert!(
        output.contains("Promise.resolve().then("),
        "CJS async ES5: import() must become Promise.resolve().then(...).\nOutput:\n{output}"
    );
    assert!(
        output.contains("require(\"./mod\")"),
        "CJS async ES5: require() with the original specifier must appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "CJS async ES5: raw import() call must not remain in output.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_cjs_dynamic_import_different_specifier_also_lowered() {
    // Prove the rule operates on the specifier value, not just "mod".
    let output = emit_es5_with_module(
        "async function load() { return await import(\"@scope/package\"); }",
        ModuleKind::CommonJS,
    );
    assert!(
        output.contains("require(\"@scope/package\")"),
        "CJS async ES5: specifier must be preserved verbatim in require().\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "CJS async ES5: raw import() call must not remain.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_amd_dynamic_import_lowered_to_new_promise_require() {
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./amd-mod\"); }",
        ModuleKind::AMD,
    );
    assert!(
        output.contains("new Promise("),
        "AMD async ES5: import() must be wrapped in new Promise(...).\nOutput:\n{output}"
    );
    assert!(
        output.contains("require([\"./amd-mod\"]"),
        "AMD async ES5: AMD-style require([specifier]) must appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "AMD async ES5: raw import() must not remain in output.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_umd_dynamic_import_lowered_same_as_amd() {
    // UMD and AMD share the same promise-based require wrapper.
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./umd-lib\"); }",
        ModuleKind::UMD,
    );
    assert!(
        output.contains("new Promise("),
        "UMD async ES5: import() must be wrapped in new Promise(...).\nOutput:\n{output}"
    );
    assert!(
        output.contains("require([\"./umd-lib\"]"),
        "UMD async ES5: AMD-style require() must appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "UMD async ES5: raw import() must not remain.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_no_dynamic_import_lowering_for_esnext() {
    // ESNext does not lower dynamic imports — import() must pass through.
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./esm\"); }",
        ModuleKind::ESNext,
    );
    assert!(
        output.contains("import("),
        "ESNext async ES5: import() must pass through unchanged.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_system_dynamic_import_lowered_to_context_import() {
    // System module: import() → context_1.import(specifier)
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./sys-mod\"); }",
        ModuleKind::System,
    );
    assert!(
        output.contains("context_1.import(\"./sys-mod\")"),
        "System async ES5: import() must become context_1.import(...).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("require("),
        "System async ES5: require() must not appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Promise.resolve()"),
        "System async ES5: Promise.resolve() CJS form must not appear.\nOutput:\n{output}"
    );
}

#[test]
fn empty_array_and_object_binding_patterns_evaluate_rhs_only() {
    // When `var {} = expr` or `var [] = expr`, ES5 should just evaluate
    // the RHS into a temp binding — no destructuring needed.
    let output = emit_es5(
        "(function () {\n\
             var a: any;\n\
             var {} = a;\n\
             var [] = a;\n\
         })();\n",
    );

    assert!(
        !output.contains("void 0"),
        "Empty binding should not produce `void 0` for a non-missing RHS.\nOutput:\n{output}"
    );
    assert!(
        output.contains("= a;"),
        "Each empty binding must evaluate the RHS at least once.\nOutput:\n{output}"
    );
}

#[test]
fn nested_empty_binding_patterns_evaluate_intermediate_properties() {
    // `var { p1: {}, p2: [] } = a` — evaluate `a.p1` and `a.p2` (side-effects)
    // but extract nothing from either nested empty pattern.
    let output = emit_es5(
        "(function () {\n\
             var a: any;\n\
             var { p1: {}, p2: [] } = a;\n\
         })();\n",
    );

    assert!(
        output.contains("a.p1"),
        "Nested empty binding should still access the intermediate property.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a.p2"),
        "Nested empty binding should access both intermediate properties.\nOutput:\n{output}"
    );
}

#[test]
fn class_property_initializer_uses_outer_renamed_let() {
    // When a `let` in the outer block scope is renamed during ES5 lowering
    // (because it shadows an outer `let`), a class property initializer that
    // references that variable must use the renamed form.
    let output = emit_es5(
        "let x = 1;\n\
         {\n\
             let x = 2;\n\
             class C {\n\
                 p = x;\n\
             }\n\
         }\n",
    );
    assert!(
        output.contains("this.p = x_1"),
        "Class property initializer must reference the renamed let (x_1), not the original (x).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this.p = x;"),
        "Class property initializer must not reference the pre-rename name (x).\nOutput:\n{output}"
    );
}

#[test]
fn class_property_initializer_uses_outer_renamed_let_different_names() {
    // Same structural rule as above but with different variable names to
    // prove the fix is not keyed on the spelling `x`.
    let output = emit_es5(
        "let value = 1;\n\
         {\n\
             let value = 2;\n\
             class Widget {\n\
                 field = value;\n\
             }\n\
         }\n",
    );
    assert!(
        output.contains("this.field = value_1"),
        "Class property initializer must reference the renamed let (value_1).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this.field = value;"),
        "Class property initializer must not reference the pre-rename name (value).\nOutput:\n{output}"
    );
}

#[test]
fn class_property_initializer_no_rename_when_no_shadow() {
    // When no shadowing occurs, the variable name must pass through unchanged.
    let output = emit_es5(
        "let count = 42;\n\
         class Counter {\n\
             n = count;\n\
         }\n",
    );
    assert!(
        output.contains("this.n = count"),
        "Unshadowed let reference must remain unchanged.\nOutput:\n{output}"
    );
}
