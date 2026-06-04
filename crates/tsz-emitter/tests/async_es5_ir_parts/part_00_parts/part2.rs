/// Drive the full async-ES5 emit pipeline for the first top-level function
/// in `source`. The surrounding indent is set to 3 levels so embedded `ASTRef`
/// statements appear inside a typical `__awaiter`/`__generator` wrapper depth,
/// mirroring the indent at which the original regression surfaced.
fn emit_async_function_from_source(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let Some(root_node) = parser.arena.get(root) else {
        return String::new();
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        return String::new();
    };
    let Some(&func_idx) = source_file.statements.nodes.first() else {
        return String::new();
    };
    let mut emitter = crate::transforms::async_es5::AsyncES5Emitter::new(&parser.arena);
    emitter.set_source_map_context(source, 0);
    emitter.set_indent_level(3);
    emitter.emit_async_function(func_idx)
}

#[test]
fn test_async_do_while_no_await_uses_formatted_emission() {
    let output = emit_async_function_from_source("async function f() { do { x; } while (y); }");
    assert!(
        output.contains("do {\n                        x;\n                    } while (y);"),
        "Do-while inside async-ES5 body should be re-formatted to tsc's multi-line shape at the surrounding indent.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("do { x; } while (y);"),
        "Do-while must not be emitted as a single-line raw source slice.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("} while (y);\n}"),
        "ASTRef fallback must not spill an extra `}}` from the enclosing block.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_labeled_do_while_no_await_uses_formatted_emission() {
    let output =
        emit_async_function_from_source("async function f() { L: do { break L; } while (y); }");
    assert!(
        output.contains(
            "L: do {\n                        break L;\n                    } while (y);"
        ),
        "Labeled do-while inside async-ES5 body should re-format at the surrounding indent.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_inline_if_no_await_uses_formatted_emission() {
    let output =
        emit_async_function_from_source("async function f() { if (x) { y; } else { z; } }");
    // Branch bodies should sit at the surrounding indent.
    assert!(
        output.contains("if (x) {\n                        y;\n                    }\n                    else {\n                        z;\n                    }"),
        "Inline if/else inside async-ES5 body should re-format at the surrounding indent.\nOutput:\n{output}"
    );
}

fn first_function_body(parser: &mut ParserState) -> NodeIndex {
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("root");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("source file");
    let func_idx = *source_file.statements.nodes.first().expect("function");
    let func_node = parser.arena.get(func_idx).expect("function node");
    let func = parser.arena.get_function(func_node).expect("function decl");
    func.body
}

fn body_suspends(source: &str, generator_mode: bool) -> bool {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let body_idx = first_function_body(&mut parser);
    let mut transformer = AsyncES5Transformer::new(&parser.arena);
    transformer.generator_mode = generator_mode;
    transformer.body_contains_await(body_idx)
}

fn body_contains_await(source: &str) -> bool {
    body_suspends(source, false)
}

fn body_contains_yield_in_generator(source: &str) -> bool {
    body_suspends(source, true)
}

#[test]
fn discovery_body_contains_await_returns_true_for_direct_await() {
    assert!(body_contains_await("async function f() { await bar(); }"));
}

#[test]
fn discovery_body_contains_await_ignores_nested_async_function() {
    // Discovery must not climb into a nested function body; the inner
    // `await` belongs to the nested async function's own state machine.
    assert!(!body_contains_await(
        "async function f() { async function g() { await bar(); } }"
    ));
}

#[test]
fn discovery_body_contains_await_ignores_nested_arrow_function() {
    assert!(!body_contains_await(
        "async function f() { const g = async () => { await bar(); }; }"
    ));
}

#[test]
fn discovery_body_contains_await_sees_through_type_assertion() {
    // `(await foo()) as T` is stripped by `expression_to_ir`, so the
    // discovery pass must look through the type wrapper.
    assert!(body_contains_await(
        "async function f() { var x = (await foo()) as T; }"
    ));
}

#[test]
fn discovery_body_contains_await_sees_through_non_null_assertion() {
    assert!(body_contains_await(
        "async function f() { var x = (await foo())!; }"
    ));
}

#[test]
fn discovery_body_contains_await_sees_class_heritage() {
    // Class bodies are function-like, but heritage clauses run in the
    // surrounding async scope.
    assert!(body_contains_await(
        "async function f() { class C extends (await base()) {} }"
    ));
}

#[test]
fn discovery_body_contains_await_skips_class_member_bodies() {
    assert!(!body_contains_await(
        "async function f() { class C { m() { await bar(); } } }"
    ));
}

#[test]
fn discovery_body_contains_await_sees_using_declarations() {
    // `using` and `await using` introduce disposable regions that the
    // generator state machine must own, so the predicate must flag them
    // even when there is no syntactic `await` in the body.
    assert!(body_contains_await(
        "async function f() { using d = acquire(); }"
    ));
}

#[test]
fn discovery_body_contains_await_sees_for_await_of() {
    assert!(body_contains_await(
        "async function f() { for await (const x of stream()) {} }"
    ));
}

#[test]
fn discovery_body_contains_await_sees_computed_property_await() {
    assert!(body_contains_await(
        "async function f() { var o = { [await key()]: 1 }; }"
    ));
}

#[test]
fn discovery_generator_mode_classifies_yield_as_suspension() {
    // In generator mode, `yield` is the suspension point, not `await`.
    assert!(body_contains_yield_in_generator(
        "function* f() { yield 1; }"
    ));
}

#[test]
fn discovery_generator_mode_ignores_body_without_yield() {
    assert!(!body_contains_yield_in_generator("function* f() { x(); }"));
}

#[test]
fn discovery_body_contains_await_returns_false_for_pure_body() {
    assert!(!body_contains_await("async function f() { var x = 1; }"));
}

#[test]
fn for_await_of_in_async_function_uses_async_iterator_state_machine() {
    let output = transform_and_print("async function f() { let y; for await (const x of y) {} }");

    assert!(
        output.contains("__asyncValues(y)") && output.contains(".next()"),
        "Plain `for await...of` must lower through the async iterator helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".trys.push([0, 5, 6, 11]);")
            && output.contains("if (!(!_a && !_b && (_c = y_1.return)))"),
        "The lowered loop must protect iterator close with tsc's outer try/finally shape.\nOutput:\n{output}"
    );
    assert!(
        output.contains("x = _d;") && !output.contains("for await"),
        "The iteration value should assign after the awaited `next()` result resumes.\nOutput:\n{output}"
    );
}

#[test]
fn labeled_for_await_continue_targets_iteration_case() {
    let output = transform_and_print(
        "async function f() { let y; outer: for await (const x of y) { continue outer; } }",
    );

    assert!(
        output.contains("x = _d;\n                    return [3 /*break*/, 3];"),
        "A labeled continue targeting the for-await loop should jump to the loop iteration case.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("continue outer;"),
        "The source-level labeled continue must not survive in ES5 async output.\nOutput:\n{output}"
    );
}

#[test]
fn for_await_in_async_generator_wraps_protocol_awaits() {
    let output = transform_async_generator_inner_and_print(
        "async function* f() { for await (const x of y) {} }",
    );

    assert!(
        output.contains("return [4 /*yield*/, __await(y_1.next())];"),
        "Async generators must yield `__await(iterator.next())` for for-await protocol awaits.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, __await(_c.call(y_1))];"),
        "Async generators must also wrap iterator `return()` awaits during cleanup.\nOutput:\n{output}"
    );
}

fn transform_async_generator_inner_and_print(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let Some(root_node) = parser.arena.get(root) else {
        return String::new();
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        return String::new();
    };
    let Some(&func_idx) = source_file.statements.nodes.first() else {
        return String::new();
    };
    let Some(func_node) = parser.arena.get(func_idx) else {
        return String::new();
    };
    let Some(func) = parser.arena.get_function(func_node) else {
        return String::new();
    };

    let mut transformer = AsyncES5Transformer::new(&parser.arena);
    transformer.set_source_text(source);
    let ir = transformer.transform_async_generator_inner_function(
        Some("f_1".to_string()),
        &func.parameters.nodes,
        func.body,
        false,
    );

    let mut printer = IRPrinter::with_arena(&parser.arena);
    printer.set_target_es5(true);
    printer.set_source_text(source);
    printer.emit(&ir);
    printer.take_output()
}
