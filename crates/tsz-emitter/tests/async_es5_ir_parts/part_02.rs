// Destructuring lowering inside async/generator ES5 bodies (issue #14080).
// Binder names are varied per case so the assertions cannot be satisfied by a
// fixture-name fast path; every expected string is byte-checked against `tsc`
// 6.0.2 output for the matching snippet.

#[test]
fn async_es5_object_destructuring_simple_identifier_source() {
    let output =
        transform_and_print("async function f() { await g(); const { alpha, beta } = obj; }");
    assert!(
        output.contains("var alpha, beta;"),
        "destructured names must be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("alpha = obj.alpha, beta = obj.beta;"),
        "object destructuring of an identifier source must inline the source.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var ;") && !output.contains(" = obj;"),
        "no invalid empty binding may be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_object_destructuring_non_simple_source_uses_temp() {
    let output =
        transform_and_print("async function f() { await g(); const { one, two } = make(); }");
    assert!(
        output.contains("var _a, one, two;"),
        "a non-inlineable source must be captured in a hoisted temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = make(), one = _a.one, two = _a.two;"),
        "the temp must hold the source and each name reads from it.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_array_destructuring_simple_identifier_source() {
    let output =
        transform_and_print("async function f() { await g(); const [head, tail] = items; }");
    assert!(
        output.contains("var head, tail;"),
        "array binding names must be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("head = items[0], tail = items[1];"),
        "array destructuring uses indexed access.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_nested_single_property_chains_access() {
    let output =
        transform_and_print("async function f() { await g(); const { outer: { inner } } = box; }");
    assert!(
        output.contains("var inner;"),
        "only the leaf binding name is hoisted for a single-property nest.\nOutput:\n{output}"
    );
    assert!(
        output.contains("inner = box.outer.inner;"),
        "a single-element nested pattern must chain property access (no temp).\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_default_value_uses_void_zero_temp() {
    let output =
        transform_and_print("async function f() { await g(); const { count = 500 } = config; }");
    assert!(
        output.contains("var _a, count;"),
        "default lowering hoists the access temp and the binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = config.count, count = _a === void 0 ? 500 : _a;"),
        "defaults capture the access in a temp and compare === void 0.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_object_rest_uses_rest_helper() {
    let output =
        transform_and_print("async function f() { await g(); const { taken, ...others } = source; }");
    assert!(
        output.contains("taken = source.taken, others = __rest(source, [\"taken\"]);"),
        "object rest must exclude the taken keys via __rest.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_array_rest_uses_slice() {
    let output =
        transform_and_print("async function f() { await g(); const [first, ...remaining] = list; }");
    assert!(
        output.contains("first = list[0], remaining = list.slice(1);"),
        "array rest must slice from the rest index.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_destructuring_before_first_await_in_case_zero() {
    let output =
        transform_and_print("async function f() { const { lhs, rhs } = pair; await g(); }");
    assert!(
        output.contains("var lhs, rhs;") && output.contains("lhs = pair.lhs, rhs = pair.rhs;"),
        "a destructuring decl before the first await lowers in case 0.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_destructuring_of_awaited_value() {
    let output =
        transform_and_print("async function f() { const { left, right } = await make(); }");
    assert!(
        output.contains("var _a, left, right;"),
        "awaited destructuring hoists the value temp and the names.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = _b.sent(), left = _a.left, right = _a.right;"),
        "the awaited value flows through a temp into each name.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_catch_clause_object_pattern() {
    let output = transform_and_print(
        "async function f() { try { await g(); } catch ({ message }) { sink(message); } }",
    );
    assert!(
        output.contains("var _a, message;"),
        "the catch temp and the destructured name are hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = _b.sent();") && output.contains("message = _a.message;"),
        "a destructuring catch binds the caught value then extracts the pattern.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var ;"),
        "no invalid empty binding may be emitted for a catch pattern.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_catch_clause_two_element_pattern() {
    let output = transform_and_print(
        "async function f() { try { await g(); } catch ({ code, detail }) { sink(code, detail); } }",
    );
    assert!(
        output.contains("_a = _b.sent();")
            && output.contains("code = _a.code, detail = _a.detail;"),
        "multi-binding catch patterns extract from the caught-value temp.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_literal_computed_key_is_static_access() {
    let output =
        transform_and_print("async function f() { await g(); const { [\"name\"]: id, age } = rec; }");
    assert!(
        output.contains("id = rec[\"name\"], age = rec.age;"),
        "a string-literal computed key lowers to static element access (no temp).\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_dynamic_computed_key_captures_key_temp() {
    let output =
        transform_and_print("async function f() { await g(); const { [sel]: picked, rest } = src; }");
    assert!(
        output.contains("_a = src, _b = sel, picked = _a[_b], rest = _a.rest;"),
        "a dynamic computed key forces a source temp and captures the key.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_dynamic_computed_key_with_rest_coerces_exclusion() {
    let output =
        transform_and_print("async function f() { await g(); const { [sel]: picked, ...others } = src; }");
    assert!(
        output.contains(
            "others = __rest(_a, [typeof _b === \"symbol\" ? _b : _b + \"\"]);"
        ),
        "a dynamic key excluded by an object rest is coerced to its __rest key form.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_catch_clause_nested_pattern() {
    let output = transform_and_print(
        "async function f() { try { await g(); } catch ({ info: { reason } }) { sink(reason); } }",
    );
    assert!(
        output.contains("_a = _b.sent();") && output.contains("reason = _a.info.reason;"),
        "a nested catch pattern chains access through the caught-value temp.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_yield_await_re_marks_awaited_value() {
    // Issue #14765: `yield await p` in an async generator lowered to the ES5
    // `__generator` state machine must re-mark the awaited result as an await
    // (`__await.apply(void 0, [_a.sent()])`) before re-yielding it. A bare
    // `_a.sent()` at the middle label corrupts the async-iterator protocol.
    let output = transform_async_generator_inner_and_print(
        "async function* ag(p: Promise<number>) { yield await p; }",
    );

    assert!(
        output.contains("return [4 /*yield*/, __await(p)];"),
        "The inner await must still yield `__await(p)`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, __await.apply(void 0, [_a.sent()])];"),
        "`yield await p` must re-mark the awaited value with `__await.apply(void 0, [_a.sent()])`, not bare `_a.sent()`.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_return_value_is_awaited() {
    // An async generator awaits its return value: `return expr` lowers to
    // `yield __await(expr)` then `return _a.sent()`. tsz previously dropped the
    // wrapper and emitted a bare `return [2, expr]`.
    let output = transform_async_generator_inner_and_print(
        "async function* ag(x: number) { return x; }",
    );
    assert!(
        output.contains("return [4 /*yield*/, __await(x)];"),
        "An async generator must await its return value via `yield __await(x)`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [2 /*return*/, _a.sent()];"),
        "The awaited return value must resolve to the resumed `_a.sent()`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return [2 /*return*/, x];"),
        "The bare un-awaited return must not survive.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_bare_return_awaits_void() {
    // Even a bare `return;` awaits the implicit `undefined`.
    let output = transform_async_generator_inner_and_print(
        "async function* ag() { foo(); return; }",
    );
    assert!(
        output.contains("return [4 /*yield*/, __await(void 0)];")
            && output.contains("return [2 /*return*/, _a.sent()];"),
        "A bare `return;` in an async generator awaits `void 0`.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_implicit_completion_stays_linear() {
    // No explicit return and no await/yield: tsc emits the body linearly with
    // a plain `return [2 /*return*/]`, no state machine and no await wrap.
    let output = transform_async_generator_inner_and_print(
        "async function* ag() { foo(); }",
    );
    assert!(
        !output.contains("switch (_a.label)") && output.contains("return [2 /*return*/];"),
        "An async generator with no explicit return stays linear.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_yield_await_then_return_keeps_protocol_labels() {
    // The adjacent case from #14765: `const r = yield await p; return r;` shows
    // the same dropped wrapper, and the shifted labels collapse the
    // return-await. Both must be present once the wrapper is restored.
    let output = transform_async_generator_inner_and_print(
        "async function* ag(p: Promise<number>) { const r = yield await p; return r; }",
    );

    assert!(
        output.contains("return [4 /*yield*/, __await.apply(void 0, [_a.sent()])];"),
        "The `yield await` result must be re-awaited before re-yielding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return [4 /*yield*/, _a.sent()];\n            case 1: return [4 /*yield*/, _a.sent()];"),
        "The middle label must not collapse to a bare sent value.\nOutput:\n{output}"
    );
}
