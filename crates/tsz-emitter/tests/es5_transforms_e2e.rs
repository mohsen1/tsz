//! End-to-end ES5 transform tests using the full `lower_and_print` pipeline.
//!
//! These tests verify that the complete chain (parse -> lower -> print) produces
//! correct ES5 output for destructuring, class, and async transforms.

use crate::emitter::ModuleKind;
use crate::output::printer::{PrintOptions, lower_and_print};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions {
        target,
        ..PrintOptions::default()
    };
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

fn emit_es5(source: &str) -> String {
    emit_with_target(source, ScriptTarget::ES5)
}

fn emit_es5_with_module(source: &str, module: ModuleKind) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions {
        target: ScriptTarget::ES5,
        module,
        ..PrintOptions::default()
    };
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

fn emit_es5_with_comments(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, PrintOptions::es5()).code
}

#[test]
fn async_es5_for_loop_captured_let_with_await_uses_loop_generator() {
    let output = emit_es5(
        "async function f() {\n\
             var ar = [];\n\
             for (let i = 0; i < 1; i++) {\n\
                 await 1;\n\
                 ar.push(() => i);\n\
             }\n\
         }\n",
    );

    assert!(
        output.contains("var ar, _loop_1, i;"),
        "Captured loop helper and loop variable should be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1 = function (i)"),
        "Captured loop body should move into a loop helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [5 /*yield**/, _loop_1(i)];"),
        "Outer async loop should delegate to the loop helper generator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("ar.push(function () { return i; });"),
        "Captured arrow should close over the loop helper parameter.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_uses_ambient_value_for_custom_promise_constructor() {
    let output = emit_es5(
        "type MyPromise<T> = Promise<T>;\n\
         declare var MyPromise: typeof Promise;\n\
         async function f(): MyPromise<void> { }\n\
         var g = async (): MyPromise<void> => { };\n\
         class C { async m(): MyPromise<void> { } }\n",
    );

    assert!(
        output.matches("MyPromise, function").count() >= 3,
        "Async functions, arrows, and methods should pass the ambient value constructor.\nOutput:\n{output}"
    );
}

#[test]
fn class_method_for_of_delegates_es5_statement_lowering() {
    let output = emit_es5_with_comments(
        r#"
class Operation {
    validate(parameterValues: any) {
        let result: any = null;
        for (const parameterLocation of Object.keys(parameterValues)) {
            const parameter = (this as any).getParameter();
            // keep loop comment
            result = parameterLocation;
        }
        return result;
    }
}
"#,
    );

    assert!(
        output
            .contains("for (var _i = 0, _a = Object.keys(parameterValues); _i < _a.length; _i++)"),
        "Class method for-of should use the normal ES5 array-index lowering.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var parameterLocation = _a[_i];"),
        "Loop variable should be bound from the generated array temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var parameter = this.getParameter();"),
        "Type-erased `(this as any)` should print through the normal AST expression path.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(" of Object.keys(parameterValues)")
            && !output.contains("(this).getParameter()"),
        "Class method for-of must not leak raw for-of syntax or redundant type-erasure parens.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_labeled_block_break_after_await_lowers_to_generator_jump() {
    let output = emit_es5(
        "async function f() {\n\
             block: {\n\
                 await 1;\n\
                 break block;\n\
             }\n\
         }\n",
    );

    assert!(
        !output.contains("block:") && !output.contains("await 1"),
        "Raw labeled block and await syntax should not remain in ES5 output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 0: return [4 /*yield*/, 1];"),
        "Await should lower to the initial yield case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 2];"),
        "Labeled break should jump to the post-block case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: return [2 /*return*/];"),
        "Post-block case should contain the final generator return.\nOutput:\n{output}"
    );
}

// =============================================================================
// Array Destructuring
// =============================================================================

#[test]
fn test_array_destructuring_basic() {
    let output = emit_es5("const [a, b] = arr;\n");
    assert!(
        !output.contains("[a, b]"),
        "ES5 output should not contain array destructuring syntax.\nOutput:\n{output}"
    );
    // Should have temp variable
    assert!(
        output.contains("var") || output.contains("_a"),
        "Expected ES5 variable declaration.\nOutput:\n{output}"
    );
}

#[test]
fn test_object_destructuring_basic() {
    let output = emit_es5("const {x, y} = obj;\n");
    assert!(
        !output.contains("{x, y}"),
        "ES5 output should not contain object destructuring syntax.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_with_default_value() {
    let output = emit_es5("const {x = 5} = obj;\n");
    assert!(
        output.contains("void 0") || output.contains("undefined") || output.contains("5"),
        "Expected default value handling.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_rest_array() {
    let output = emit_es5("const [first, ...rest] = arr;\n");
    assert!(
        output.contains("slice"),
        "Expected Array.prototype.slice for rest elements.\nOutput:\n{output}"
    );
}

#[test]
fn test_assignment_object_rest_uses_es5_lowering() {
    // Regression: when targeting ES5 the new ES2018 object-rest assignment
    // handler must NOT intercept dispatch. The ES5 destructuring lowering
    // already emits fully ES5-compatible output (no `{ a } = src` syntax).
    let output = emit_es5("var a, rest;\n({a, ...rest} = obj);\n");
    // The buggy ES2018 path emits `{ a: a } = obj, rest = __rest(obj, ["a"])`
    // — the `{ a: a } = obj` portion is ES2015+ destructuring, invalid for ES5.
    // The correct ES5 lowering emits `a = obj.a, rest = __rest(obj, ["a"])`.
    assert!(
        !output.contains("} = obj"),
        "ES5 output must not contain object destructuring assignment syntax.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ a") && !output.contains("{a"),
        "ES5 output must not retain an object literal pattern on the LHS.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a = obj.a") || output.contains("a = obj[\"a\"]"),
        "Expected ES5-compatible property assignment for non-rest binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__rest"),
        "Expected __rest helper for object rest.\nOutput:\n{output}"
    );
}

#[test]
fn assignment_object_rest_strips_redundant_paren_around_object_rhs() {
    // Regression for `destructuringObjectBindingPatternAndAssignment5`: when the
    // RHS of an assignment-form object-rest destructuring is a parenthesized
    // type-erased object literal (`({ } as any)`), the lowering threads the
    // RHS into a temp inside a comma expression — never at statement-leading
    // position, so the outer paren is redundant. tsc emits `_a = {}` here, not
    // `_a = ({})`.
    let output = emit_with_target(
        r#"
function a() {
    let x: number;
    let y: any;
    ({ x, ...y } = ({ } as any));
}
"#,
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("_a = {}, "),
        "RHS object literal must be unwrapped from its redundant paren.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a = ({})"),
        "Redundant paren must not be preserved at non-statement-leading position.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_computed_exclusion_reuses_key_temp() {
    let output = emit_es5(
        r#"
declare function getKey(): string;
const source: any = {};
const { [getKey()]: value = 1, ...rest } = source;
console.log(value);
"#,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "Computed rest key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" = source[_"),
        "Computed property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "Computed property default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "__rest exclusion should use TypeScript's symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__rest(source, [getKey()])"),
        "__rest exclusion must not re-evaluate the computed key expression.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_computed_exclusion_reuses_key_temp() {
    let output = emit_es5(
        r#"
declare function getKey(): string;
let value: any;
let rest: any;
const source: any = {};
({ [getKey()]: value = 1, ...rest } = source);
console.log(value);
"#,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "Computed rest assignment key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("source[_"),
        "Computed assignment property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "Computed assignment default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "Assignment __rest exclusion should use symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("source[\"\"]") && !output.contains("__rest(source, [])"),
        "Assignment lowering must not drop the computed key.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_es2017_computed_default_reuses_key_temp() {
    let output = emit_with_target(
        r#"
declare function getKey(): string;
const source: any = {};
const { [getKey()]: value = 1, ...rest } = source;
console.log(value);
"#,
        ScriptTarget::ES2017,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "ES2017 computed rest key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[_"),
        "ES2017 computed property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "ES2017 computed property default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "ES2017 __rest exclusion should use symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_es2017_computed_key_with_nested_pattern_lowers_nested_binding() {
    let output = emit_with_target(
        r#"
declare function order(n: number): any;
let { [order(0)]: { [order(2)]: z } = order(1), ...w } = {} as any;
"#,
        ScriptTarget::ES2017,
    );

    assert!(
        !output.contains(",  ="),
        "Nested binding pattern under computed object-rest key must not emit an empty binding name.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches("order(0)").count(),
        1,
        "Outer computed key should be evaluated once.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches("order(2)").count(),
        1,
        "Nested computed key should be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("z = _"),
        "Nested computed binding should decompose from the defaulted value temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__rest("),
        "Outer object rest should still lower to __rest.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_es2017_computed_exclusion_reuses_key_temp() {
    let output = emit_with_target(
        r#"
declare function getKey(): string;
let value: any;
let rest: any;
const source: any = {};
({ [getKey()]: value = 1, ...rest } = source);
console.log(value);
"#,
        ScriptTarget::ES2017,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "ES2017 computed rest assignment key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("source[_"),
        "ES2017 computed assignment property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "ES2017 computed assignment default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "ES2017 assignment __rest exclusion should use symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__rest(source, [])") && !output.contains("__rest(source, [getKey()])"),
        "ES2017 assignment lowering must not drop or re-evaluate the computed key.\nOutput:\n{output}"
    );
}

#[test]
fn array_object_rest_es2017_defers_later_default_until_after_rest_binding() {
    let output = emit_with_target(
        "let [{ ...a }, b = a]: any[] = [{ x: 1 }];",
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("__rest("),
        "Nested object rest inside an array binding should lower to __rest.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ ...a }") && !output.contains("b = a] ="),
        "Lowering must remove object rest and defer the later default out of the array head.\nOutput:\n{output}"
    );
    let rest_pos = output
        .find("a = __rest")
        .expect("expected a deferred object-rest binding");
    let default_pos = output
        .find("b = _")
        .expect("expected a deferred default binding");
    assert!(
        rest_pos < default_pos,
        "Default initializer that references the rest binding must run after the rest binding.\nOutput:\n{output}"
    );
}

#[test]
fn nested_object_rest_lowers_when_outer_pattern_has_no_rest() {
    // Regression for `narrowingDestructuring`: when the outer object pattern has
    // no rest element but a nested element does (e.g. `{ f: { a, ...spread } }`),
    // the lowering pass must still introduce a nested temp and a `__rest` call.
    // The bug was a partial `needs_temp` predicate that only fired when the outer
    // pattern itself carried a rest element, leaving the source un-lowered.
    let output = emit_with_target(
        r#"
declare const value: { f: { a: number; b: string; c: number } };
const { f: { a, ...spread } } = value;
"#,
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("__rest("),
        "Nested object rest must be lowered to a __rest() call.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("...spread"),
        "Lowered output must not retain the rest spread syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value.f"),
        "Lowering should thread `value.f` into a temp before the __rest call.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_nested_object() {
    let output = emit_es5("const {a: {b}} = obj;\n");
    assert!(
        !output.contains("{a:"),
        "ES5 should not contain nested destructuring syntax.\nOutput:\n{output}"
    );
    // Should reference .a and .b through temp variables
    assert!(
        output.contains(".a") || output.contains("[\"a\"]"),
        "Expected property access for nested destructuring.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_rename() {
    let output = emit_es5("const {x: renamed} = obj;\n");
    assert!(
        output.contains("renamed"),
        "Expected renamed binding.\nOutput:\n{output}"
    );
}

// =============================================================================
// Class ES5 Transform
// =============================================================================

#[test]
fn test_class_to_iife() {
    let output = emit_es5_with_comments(
        "class Point {\n    constructor(x, y) {\n        this.x = x;\n        this.y = y;\n    }\n}\n",
    );
    assert!(
        output.contains("/** @class */"),
        "Expected @class annotation.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function Point("),
        "Expected constructor function.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return Point;"),
        "Expected return statement.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_extends_to_iife() {
    let output = emit_es5("class Dog extends Animal {\n    bark() { return 'woof'; }\n}\n");
    assert!(
        output.contains("__extends"),
        "Expected __extends helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_super"),
        "Expected _super parameter.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_static_method() {
    let output = emit_es5("class Counter {\n    static count() { return 0; }\n}\n");
    assert!(
        output.contains("Counter.count = function"),
        "Expected static method on class directly.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_prototype_method() {
    let output = emit_es5("class Greeter {\n    greet() { return 'hello'; }\n}\n");
    assert!(
        output.contains("Greeter.prototype.greet = function"),
        "Expected prototype method.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_private_field_weakmap() {
    let output = emit_es5("class Container {\n    #value = 42;\n}\n");
    assert!(
        output.contains("WeakMap"),
        "Expected WeakMap for private field.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_property_initializer() {
    let output = emit_es5("class Counter {\n    count = 0;\n}\n");
    assert!(
        output.contains("this.count ="),
        "Expected property initializer in constructor.\nOutput:\n{output}"
    );
}

#[test]
fn test_computed_string_field_preserves_source_quotes() {
    let output = emit_es5("class C {\n    ['this'] = '';\n}\n");
    assert!(
        output.contains("this['this'] = '';"),
        "Expected computed string field to preserve source quotes.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Expected computed string field not to be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_getter_setter_define_property() {
    let output = emit_es5("class Foo {\n    get bar() { return 1; }\n    set bar(v) {}\n}\n");
    assert!(
        output.contains("Object.defineProperty"),
        "Expected Object.defineProperty for accessors.\nOutput:\n{output}"
    );
}

// =============================================================================
// Arrow Function ES5 Transform
// =============================================================================

#[test]
fn test_arrow_function_to_function() {
    let output = emit_es5("const f = (x) => x * 2;\n");
    assert!(
        !output.contains("=>"),
        "ES5 should not contain arrow function syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function"),
        "Expected function keyword.\nOutput:\n{output}"
    );
}

#[test]
fn test_arrow_function_this_capture() {
    let output = emit_es5("class Foo {\n    bar() {\n        const f = () => this;\n    }\n}\n");
    assert!(
        output.contains("_this"),
        "Expected _this capture for arrow function using this.\nOutput:\n{output}"
    );
}

// =============================================================================
// Let/Const -> Var
// =============================================================================

#[test]
fn test_let_becomes_var() {
    let output = emit_es5("let x = 1;\n");
    assert!(
        output.contains("var x"),
        "Expected let to become var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let x"),
        "let should not appear in ES5.\nOutput:\n{output}"
    );
}

#[test]
fn test_const_becomes_var() {
    let output = emit_es5("const x = 1;\n");
    assert!(
        output.contains("var x"),
        "Expected const to become var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const x"),
        "const should not appear in ES5.\nOutput:\n{output}"
    );
}

// =============================================================================
// Async Function Transform
// =============================================================================

#[test]
fn test_async_function_awaiter() {
    let output = emit_es5("async function fetchData() {\n    await fetch('/api');\n}\n");
    assert!(
        output.contains("__awaiter"),
        "Expected __awaiter helper for async function.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("async "),
        "async keyword should not appear in ES5.\nOutput:\n{output}"
    );
}

#[test]
fn async_arrow_hoisted_locals_share_var_statement() {
    let output = emit_es5(
        "(async () => {\n\
             const response = await fetch('/api');\n\
             const blob = await response.blob();\n\
             const size = 300;\n\
             const image = new Image();\n\
         })();\n",
    );

    assert!(
        output.contains("var response, blob, size, image;"),
        "Async arrow hoisted locals should share one var statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var response;\n        var blob;"),
        "Async arrow hoisted locals should not split ordinary declarations.\nOutput:\n{output}"
    );
}

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
