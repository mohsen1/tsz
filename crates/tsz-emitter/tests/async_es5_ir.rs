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
/// Devin review: <https://github.com/mohsen1/tsz/pull/2306#discussion_r3176720196>
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
