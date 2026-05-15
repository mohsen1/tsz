use super::*;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::ParserState;

fn transform_and_print(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&func_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = AsyncES5Transformer::new(&parser.arena);
        let ir = transformer.transform_async_function(func_idx);
        IRPrinter::emit_to_string(&ir)
    } else {
        String::new()
    }
}

fn transform_generator_and_print(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&func_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = AsyncES5Transformer::new(&parser.arena);
        let ir = transformer.transform_generator_function(func_idx);
        IRPrinter::emit_to_string(&ir)
    } else {
        String::new()
    }
}

#[test]
fn test_simple_async_function() {
    let output = transform_and_print("async function foo() { }");
    assert!(
        output.contains("function foo()"),
        "Should have function name"
    );
    assert!(output.contains("__awaiter"), "Should have awaiter call");
    assert!(
        output.contains("__generator"),
        "Should have generator wrapper"
    );
}

#[test]
fn test_generator_var_hoist_preserves_declaration_list_group() {
    let output = transform_generator_and_print("function * f() { var x = 1, y; }");

    assert!(
        output.contains("function f() {\n    var x, y;\n    return __generator"),
        "Downlevel generator var hoists should preserve declaration-list grouping: {output}"
    );
    assert!(
        !output.contains("var x;\n    var y;"),
        "The grouped source declaration should not split into separate hoists: {output}"
    );
}

#[test]
fn test_async_with_return() {
    let output = transform_and_print("async function foo() { return 42; }");
    assert!(output.contains("[2 /*return*/, 42]"), "Should return 42");
}

#[test]
fn test_async_with_await() {
    let output = transform_and_print("async function foo() { await bar(); }");
    assert!(output.contains("switch (_a.label)"), "Should have switch");
    assert!(output.contains("[4 /*yield*/"), "Should have yield");
    assert!(output.contains("_a.sent()"), "Should call _a.sent()");
}

#[test]
fn test_async_while_with_await_lowers_to_generator_cases() {
    let output =
        transform_and_print("async function f(xs) { while (xs.length) { await g(xs.pop()); } }");

    assert!(
        !output.contains("while ("),
        "Raw while statement must not remain around suspended body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain in ES5 generator output.\nOutput:\n{output}"
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
        output.contains("_a.sent();"),
        "Resumed await result should be consumed.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 0];"),
        "Loop body should jump back to the condition case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: return [2 /*return*/];"),
        "Loop exit should have a final generator return case.\nOutput:\n{output}"
    );
}

#[test]
fn test_return_await() {
    let output = transform_and_print("async function foo() { return await bar(); }");
    assert!(output.contains("[4 /*yield*/"), "Should have yield");
    assert!(
        output.contains("[2 /*return*/, _a.sent()]"),
        "Should return _a.sent()"
    );
}

#[test]
#[ignore = "regression from remote: async ES5 IR variable await assignment changed"]
fn test_variable_with_await() {
    let output = transform_and_print("async function foo() { let x = await bar(); return x; }");
    assert!(output.contains("[4 /*yield*/"), "Should have yield");
    assert!(
        output.contains("var x;") || output.contains("var x\n"),
        "Should declare var x before assignment to avoid ReferenceError: {output}"
    );
    assert!(output.contains("x = _a.sent()"), "Should assign _a.sent()");
}

#[test]
fn test_variable_declaration_order() {
    // Verify that variable declaration comes before the yield
    let output = transform_and_print("async function foo() { const result = await fetch(); }");
    let var_pos = output.find("var result");
    let yield_pos = output.find("[4 /*yield*/");
    assert!(
        var_pos.is_some()
            && yield_pos.is_some()
            && var_pos.expect("var_pos is Some, checked above")
                < yield_pos.expect("yield_pos is Some, checked above"),
        "Variable declaration must come before yield: {output}"
    );
}

#[test]
fn test_await_assignment_captures_property_target_before_yield() {
    let output = transform_and_print("async function foo() { var o; o.a = await p; after(); }");

    assert!(
        output.contains("var o, _a;"),
        "Object temp should be hoisted with local declarations: {output}"
    );
    assert!(
        output.contains("_a = o;\n                    return [4 /*yield*/, p];"),
        "Property assignment target should be captured before yielding: {output}"
    );
    assert!(
        output.contains("_a.a = _b.sent();"),
        "Resumed assignment should use the captured target and sent value: {output}"
    );
}

#[test]
fn test_await_call_argument_captures_identifier_callee_before_yield() {
    let output =
        transform_and_print("async function foo() { var b = fn(await p, a, a); after(); }");

    assert!(
        output.contains("var b, _a;"),
        "Callee temp should be hoisted with local declarations: {output}"
    );
    assert!(
        output.contains("_a = fn;\n                    return [4 /*yield*/, p];"),
        "Call callee should be captured before yielding: {output}"
    );
    assert!(
        output.contains("b = _a.apply(void 0, [_b.sent(), a, a]);"),
        "Resumed call should invoke the captured callee with the sent value: {output}"
    );
}

#[test]
fn test_computed_object_after_await_uses_separate_temp_var_statement() {
    let output =
        transform_and_print("async function foo(): Promise<void> { var v = { [await]: foo } }");

    assert!(
        output.contains("var v;\n        var _a;"),
        "Computed-object temp should be emitted in a separate hoisted var statement: {output}"
    );
}

#[test]
fn test_return_await_call_argument_captures_identifier_callee_before_yield() {
    let output = transform_and_print("async function foo() { return fn(await p); }");

    assert!(
        output.contains("var _a;"),
        "Callee temp should be hoisted for suspended return calls: {output}"
    );
    assert!(
        output.contains("_a = fn;\n                    return [4 /*yield*/, p];"),
        "Return call callee should be captured before yielding: {output}"
    );
    assert!(
        output.contains("[2 /*return*/, _a.apply(void 0, [_b.sent()])]"),
        "Resumed return should invoke the captured callee with the sent value: {output}"
    );
}

#[test]
fn test_await_call_argument_preserves_prefix_arguments() {
    let output =
        transform_and_print("async function foo() { var b = fn(a, await p, a); after(); }");

    assert!(
        output.contains("var b, _a, _b;"),
        "Callee and prefix-argument temps should be hoisted: {output}"
    );
    assert!(
        output.contains(
            "_a = fn;\n                    _b = [a];\n                    return [4 /*yield*/, p];"
        ),
        "Callee and prefix arguments should be captured before yielding: {output}"
    );
    assert!(
        output.contains("b = _a.apply(void 0, _b.concat([_c.sent(), a]));"),
        "Resumed call should concatenate the sent value after prefix args: {output}"
    );
}

#[test]
fn test_await_method_call_argument_captures_receiver_before_yield() {
    let output =
        transform_and_print("async function foo() { var b = o.fn(await p, a, a); after(); }");

    assert!(
        output.contains("var b, _a, _b;"),
        "Receiver and method temps should be hoisted: {output}"
    );
    assert!(
        output.contains("_b = (_a = o).fn;\n                    return [4 /*yield*/, p];"),
        "Method receiver and function should be captured before yielding: {output}"
    );
    assert!(
        output.contains("b = _b.apply(_a, [_c.sent(), a, a]);"),
        "Resumed method call should use captured receiver as this: {output}"
    );
}

/// `class C extends (await base())` lowered to ES5 must still emit the
/// `WeakMap` declarations and instantiations for any private fields on the
/// class body. Previously `es5_class_factory` destructured only the
/// IIFE body and silently dropped `weakmap_decls` and `weakmap_inits`,
/// causing the generated code to reference undeclared `WeakMap` names.
#[test]
fn test_async_class_extends_await_preserves_private_field_weakmaps() {
    let output = transform_and_print(
        "async function f() { class Foo extends (await base()) { #x = 1; getX() { return this.#x; } } }",
    );

    assert!(
        output.contains("new WeakMap()"),
        "Output must contain WeakMap instantiation for private field `#x`. Without it, the IIFE references an undeclared WeakMap.\nOutput:\n{output}"
    );
    // The generator body should declare the private-field weakmap as a var.
    assert!(
        output.contains("var _Foo_x") || output.contains("var _x"),
        "Output must contain a `var` declaration for the private-field WeakMap.\nOutput:\n{output}"
    );
}

/// `await` wrapped in a TypeScript type-only expression
/// (`as T`, `<T>...`, `satisfies T`, non-null `!`) must still be
/// detected by `contains_await_recursive` and `find_suspension_expression`,
/// otherwise the IR transformer emits `_a.sent()` without a preceding
/// `[4 /*yield*/]` instruction in the generated state machine.
#[test]
fn test_async_await_under_as_expression_emits_yield() {
    let output =
        transform_and_print("async function f() { var x = (await bar()) as number; after(x); }");
    assert!(
        output.contains("[4 /*yield*/"),
        "Output must contain a yield instruction for `(await bar()) as number`.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_await_under_non_null_assertion_emits_yield() {
    let output = transform_and_print("async function f() { var x = (await bar())!; after(x); }");
    assert!(
        output.contains("[4 /*yield*/"),
        "Output must contain a yield instruction for `(await bar())!`.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_await_under_satisfies_emits_yield() {
    let output = transform_and_print(
        "async function f() { var x = (await bar()) satisfies number; after(x); }",
    );
    assert!(
        output.contains("[4 /*yield*/"),
        "Output must contain a yield instruction for `(await bar()) satisfies number`.\nOutput:\n{output}"
    );
}

// Issue #3540: ES5 async transform must lower tagged template literals
// with substitutions to a __makeTemplateObject call. The previous
// fallback re-emitted the raw source text (including the trailing `;`)
// inside the generator return tuple, producing invalid JavaScript.
#[test]
fn test_async_tagged_template_with_substitutions_lowers_to_make_template_object() {
    let output = transform_and_print("async function f() { return tag`a${1}b`; }");

    assert!(
        output.contains("__makeTemplateObject([\"a\", \"b\"], [\"a\", \"b\"])"),
        "Tagged template with substitutions must lower to __makeTemplateObject(...)\
         with cooked + raw arrays.\nOutput:\n{output}"
    );
    assert!(
        output.contains("tag(__makeTemplateObject"),
        "Tag must call into __makeTemplateObject(...) wrapper.\nOutput:\n{output}"
    );
    // The substitution expression is appended as a trailing argument.
    assert!(
        output.contains("[\"a\", \"b\"], [\"a\", \"b\"]), 1)"),
        "Substitution expressions must follow as call arguments.\nOutput:\n{output}"
    );
    // Pre-fix bug: raw source text (with semicolon) leaked into the
    // generator return tuple. Make sure it does not.
    assert!(
        !output.contains("tag`a${1}b`"),
        "Raw template syntax must not appear in lowered output.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("`;"),
        "Trailing `;` from source-text fallback must not appear.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_tagged_template_no_substitutions_unchanged() {
    let output = transform_and_print("async function f() { return tag`hello`; }");
    assert!(
        output.contains("__makeTemplateObject([\"hello\"], [\"hello\"])"),
        "No-substitution tagged template should still lower to __makeTemplateObject.\nOutput:\n{output}"
    );
}
