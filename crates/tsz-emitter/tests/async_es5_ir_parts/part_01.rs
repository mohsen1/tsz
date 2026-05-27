#[test]
fn async_generator_state_machine_lowers_nullish_coalescing_with_complex_lhs() {
    // The repro from issue #37686 in the conformance baseline: a
    // complex LHS (property access) on `??` must allocate a hoisted
    // temp `_a`, evaluate the LHS exactly once into that temp, and
    // reference the temp on the truthy branch. tsc emits this for
    // every target that predates ES2020; the state-machine path is
    // only engaged at ES5/ES2015 so we always lower here.
    let output = transform_async_generator_inner_and_print(
        "async function* f(a: { b?: number }) { let c = a.b ?? 10; while (c) { yield c--; } }",
    );

    assert!(
        !output.contains(" ?? "),
        "Raw `??` must not survive into the generator state-machine IR.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = a.b) !== null && _a !== void 0 ? _a : 10"),
        "Complex-LHS nullish coalescing must lower to (_a = lhs) !== null && _a !== void 0 ? _a : rhs.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a;"),
        "The hoisted nullish temp `_a` must be declared in the outer state-machine scope.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_state_machine_lowers_nullish_coalescing_with_simple_lhs() {
    // tsc's `isSimpleCopiableExpression` allows the LHS to be repeated
    // when it is an identifier/literal/keyword — no temp is allocated
    // in that case. The lowering must match.
    let output = transform_async_generator_inner_and_print(
        "async function* f(a: any) { let c = a ?? 10; await x; }",
    );

    assert!(
        !output.contains(" ?? "),
        "Raw `??` must not survive into the generator state-machine IR.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a !== null && a !== void 0 ? a : 10"),
        "Simple-LHS nullish coalescing must lower without a hoisted temp.\nOutput:\n{output}"
    );
    // Without a complex LHS we should not see `(_a = ...)` from the
    // nullish lowering, and no helper temp should be hoisted *because
    // of* this lowering (the generator may still allocate `_a` as its
    // state argument name; that lives inside `function (_a)` and is
    // not declared via `var`).
    assert!(
        !output.contains("(_a = a)"),
        "Simple-LHS path must not allocate a hoisted temp around the LHS.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_state_machine_lowers_nested_nullish_coalescing() {
    // `a ?? b ?? c` is left-associative in tsc: it lowers as
    // `(a ?? b) ?? c`, allocating one temp per non-simple subexpression.
    // The right-associative parsing would be `a ?? (b ?? c)`, which is
    // semantically different (different short-circuit grouping). Both
    // sides here are property accesses, so every step allocates a temp.
    let output = transform_async_generator_inner_and_print(
        "async function* f(o: any) { let c = o.x ?? o.y ?? 10; await q; }",
    );

    assert!(
        !output.contains(" ?? "),
        "Raw `??` must not survive into the generator state-machine IR.\nOutput:\n{output}"
    );
    // Both lowerings should each declare their own temp in the outer
    // scope, declared together in a single `var _a, _b;` line. Two
    // distinct `!== null && _ !== void 0` checks confirm two distinct
    // temps drove the lowering.
    let temp_checks = output.matches("!== null && _").count();
    assert!(
        temp_checks >= 2,
        "Nested nullish coalescing must lower with two distinct temporaries.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a") && output.contains("_b"),
        "Both temps must be declared in the outer state-machine scope.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_state_machine_rule_is_name_independent() {
    // §25/§26: the structural rule is "lower `??` in the state-machine
    // path", not "lower when the user picked a particular spelling".
    // Renaming the binding `c` to `x`, the parameter `a` to `paramX`,
    // and the property `b` to `propY` must produce the same lowered
    // shape — only the user-chosen spellings change.
    let baseline = transform_async_generator_inner_and_print(
        "async function* f(a: { b?: number }) { let c = a.b ?? 10; while (c) { yield c--; } }",
    );
    let renamed = transform_async_generator_inner_and_print(
        "async function* myAsync(paramX: { propY?: number }) { let x = paramX.propY ?? 10; while (x) { yield x--; } }",
    );

    for fragment in ["var _a;", ") !== null && _a !== void 0 ? _a : 10"] {
        assert!(
            baseline.contains(fragment),
            "Expected `{fragment}` in baseline output.\nOutput:\n{baseline}"
        );
        assert!(
            renamed.contains(fragment),
            "Expected `{fragment}` in renamed output (rule must be spelling-independent).\nOutput:\n{renamed}"
        );
    }
}

#[test]
fn async_generator_state_machine_emits_loop_entry_label_after_prefix() {
    // Correctness bug: when statements precede a suspending loop, the
    // loop entry must be its own case so `[3 /*break*/, <entry>]` from
    // the body re-enters at the condition check, not at the prefix.
    // Without an explicit `_b.label = <entry>` and a fresh case label,
    // the loop-back re-executes the initializer every iteration —
    // turning `let c = init; while (c) { yield c--; }` into an
    // infinite loop.
    let output = transform_async_generator_inner_and_print(
        "async function* f(a: any) { let c = a; while (c) { yield c--; } }",
    );

    // The loop entry must be on its own case, and the prior case must
    // end with `_b.label = <entry-label>` so fall-through reaches it.
    assert!(
        output.contains(".label = 1;"),
        "Prefix-then-loop must emit an explicit label assignment before the loop entry case.\nOutput:\n{output}"
    );
    // Loop-back inside the body must target the loop ENTRY label, not
    // the initial case (label 0) — otherwise the initializer re-runs.
    // We don't pin the exact opcode here; we just assert no
    // `[3 /*break*/, 0]` (the symptom of looping back to the prefix
    // case).
    assert!(
        !output.contains("[3 /*break*/, 0]"),
        "Loop-back must not target the initial case (where the prefix lives).\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_state_machine_keeps_simple_loop_label_intact() {
    // Negative: when there is no prefix, the loop entry stays on the
    // initial case (label 0). The label-flush helper must not insert a
    // spurious `_a.label = 1;` in that case — that would also be valid
    // semantics but tsc's output is the unprefixed shape, and gratuitous
    // label assignments produce baseline noise across the whole async
    // family. Verify the helper only fires when there is something to
    // flush.
    let output = transform_async_generator_inner_and_print(
        "async function* f() { while (true) { yield 1; } }",
    );

    assert!(
        !output.contains(".label = 1;"),
        "Loop without a prefix must not flush an empty preceding case.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_state_machine_lowers_nullish_coalescing() {
    // The same rule applies to `async function` (non-generator) at the
    // state-machine targets. Lower `??` inline; hoist the temp into the
    // awaiter wrapper's var scope so it lives alongside any user
    // hoisted vars and is shared across the `__generator` state.
    let output = emit_async_function_from_source(
        "async function f(o: { x?: number }) { let v = o.x ?? 0; await q(); }",
    );

    assert!(
        !output.contains(" ?? "),
        "Raw `??` must not survive into the async state-machine IR.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = o.x) !== null && _a !== void 0 ? _a : 0"),
        "Async-function path must lower nullish coalescing identically to async-generator path.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a;"),
        "The hoisted nullish temp must be declared in the awaiter wrapper scope.\nOutput:\n{output}"
    );
}

// Structural rule: when an async ES5 function contains
// `for (init; cond; incr) body` where any of init/cond/incr/body suspends on
// `await`, the generator state machine must lower it like `while` — the
// continue target is the incrementor case (so `continue` runs the incrementor
// and re-checks the condition), the backedge returns to the condition case,
// and `break` exits the loop. `var`s declared in the initializer are hoisted
// into the awaiter wrapper. None of this is keyed on identifier spelling.

#[test]
fn async_for_body_await_lowers_to_generator_cases() {
    let output = transform_and_print("async function f() { for (x; y; z) { await a; } }");

    assert!(
        !output.contains("for ("),
        "Raw for statement must not remain around a suspended body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain in ES5 generator output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!y) return [3 /*break*/, 4];"),
        "Loop condition should branch to the exit case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, a];"),
        "Await in the body should become a generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 1];"),
        "Body should jump back to the condition case.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_condition_await_yields_before_test() {
    let output = transform_and_print("async function f() { for (x; await y; z) { a; } }");

    assert!(
        output.contains("return [4 /*yield*/, y];"),
        "A top-level await in the condition should lower to a generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!_a.sent()) return [3 /*break*/, 4];"),
        "The yielded condition result should be tested via `_a.sent()`.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_incrementor_await_yields_before_backedge() {
    let output = transform_and_print("async function f() { for (x; y; await z) { a; } }");

    assert!(
        output.contains("return [4 /*yield*/, z];"),
        "A top-level await in the incrementor should lower to a generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 1];"),
        "After the incrementor yield, the loop should branch back to the condition case.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_initializer_await_yields_before_loop() {
    let output = transform_and_print("async function f() { for (await x; y; z) { a; } }");

    assert!(
        output.contains("case 0: return [4 /*yield*/, x];"),
        "A top-level await in the initializer should yield in the entry case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!y) return [3 /*break*/, 4];"),
        "The condition check should follow the initializer in its own case.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_lowering_is_not_keyed_on_identifier_spelling() {
    // Same structure as `async_for_body_await_lowers_to_generator_cases` with
    // every identifier renamed; the lowering must be identical.
    let output =
        transform_and_print("async function loop() { for (init; cond; step) { await work; } }");

    assert!(
        !output.contains("for (") && !output.contains("await "),
        "Renamed for-loop must lower the same way.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!cond) return [3 /*break*/, 4];"),
        "Renamed condition should still branch to the exit case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, work];"),
        "Renamed body await should still yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 1];"),
        "Renamed loop should still branch back to the condition case.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_continue_routes_to_incrementor_case() {
    let output = transform_and_print("async function f() { for (x; y; z) { await a; continue; } }");

    // continue must jump to the incrementor case (not the condition case),
    // so the incrementor runs before the condition is re-tested.
    assert!(
        output.contains("if (!y) return [3 /*break*/, 4];"),
        "Condition should branch to the exit case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 3];"),
        "`continue` should target the incrementor case (label 3).\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 1];"),
        "The incrementor case should branch back to the condition case (label 1).\nOutput:\n{output}"
    );
}

#[test]
fn async_for_var_initializer_is_hoisted_without_state_machine() {
    // No suspension anywhere: tsc keeps the `for` as-is but hoists the
    // initializer `var` into the awaiter wrapper and rewrites the init to a
    // bare assignment.
    let output = transform_and_print("async function f() { for (var c = x; y; z) { a; } }");

    assert!(
        output.contains("var c;"),
        "The for-initializer `var` must be hoisted into the awaiter wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (c = x; y; z)"),
        "The hoisted initializer must be rewritten to a bare assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("for (var c"),
        "The `var` keyword must not remain in the for-initializer.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_var_initializer_without_value_is_hoisted() {
    let output = transform_and_print("async function f() { for (var b; y; z) { a; } }");

    assert!(
        output.contains("var b;"),
        "An uninitialized for `var` must still be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (; y; z)"),
        "With no initializer value the for-init slot must be empty.\nOutput:\n{output}"
    );
}

#[test]
fn async_for_block_scoped_initializer_is_not_var_hoisted() {
    // `let`/`const` are block-scoped: the `var`-hoist shortcut must not apply,
    // otherwise the binding's scope/semantics would change. The block-scoped
    // initializer is left to the verbatim / ES5 block-scoping path. With the
    // bug present the emitter would produce a hoisted `var c;` plus
    // `for (c = x; y; z)`; the gate prevents both.
    for keyword in ["let", "const"] {
        let output = transform_and_print(&format!(
            "async function f() {{ for ({keyword} c = x; y; z) {{ a; }} }}"
        ));

        assert!(
            !output.contains("var c;"),
            "A block-scoped `{keyword}` for-initializer must not be hoisted to `var`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("for (c = x"),
            "A block-scoped `{keyword}` initializer must not be rewritten to a bare assignment.\nOutput:\n{output}"
        );
    }
}

// Negative/fallback case (no suspension and no hoistable `var` -> the for loop
// is emitted verbatim, not a state machine) is covered end-to-end by the
// `es5-asyncFunctionForStatements` emit baseline (`forStatement0`); the
// standalone IR-printer test harness cannot render verbatim AST references.

// Structural rule: when a `switch` statement's case block suspends (await in an
// async function, yield in a generator) — in a case-clause expression or a
// clause body — tsc lowers it into the `__generator` state machine: the
// discriminant is cached into a hoisted temp, dispatch `switch`es compare the
// temp against each clause expression and `return [3 /*break*/, L]` to the
// matched clause-body label, and each clause body lives at its own label with
// `break` rewritten to a jump to the switch-end label. This is independent of
// identifier spelling, which clause holds the suspension, and whether/where a
// `default` clause appears.

#[test]
fn switch_with_await_in_clause_expression_lowers_to_dispatch_state_machine() {
    let output = transform_and_print(
        "async function f() { switch (x) { case await y: a; break; default: b; break; } }",
    );

    assert!(
        output.contains("_a = x;"),
        "Discriminant must be cached into a hoisted temp before dispatch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, y];"),
        "A suspending case expression must yield before the dispatch switch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case _b.sent(): return [3 /*break*/, 2];"),
        "Dispatch must compare the cached discriminant against the resumed case value and jump to the clause body label, emitted inline.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 3];"),
        "With a default clause, the post-dispatch fallthrough must target the default body label.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2:")
            && output.contains("case 3:")
            && output.contains("case 4: return [2 /*return*/];"),
        "Clause bodies and the end label must be laid out as sequential generator cases.\nOutput:\n{output}"
    );
}

#[test]
fn switch_lowering_is_not_keyed_on_identifier_spelling() {
    // Same shape as above with every identifier renamed; the lowering must be
    // structural, not name-keyed.
    let output = transform_and_print(
        "async function f() { switch (disc) { case await chosen: hit; break; default: miss; break; } }",
    );

    assert!(
        output.contains("_a = disc;"),
        "Discriminant caching must work for any discriminant spelling.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, chosen];"),
        "Renamed suspending case expression must still yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case _b.sent(): return [3 /*break*/, 2];"),
        "Renamed shape must still produce the dispatch jump.\nOutput:\n{output}"
    );
}

#[test]
fn switch_with_await_in_clause_body_keeps_dispatch_synchronous() {
    let output = transform_and_print(
        "async function f() { switch (x) { case y: await a; break; default: b; break; } }",
    );

    assert!(
        output.contains("_a = x;") && output.contains("case y: return [3 /*break*/, 1];"),
        "When only a clause body suspends, the discriminant is still cached and the dispatch compares it synchronously.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, a];"),
        "The clause body await must yield inside the clause's body label.\nOutput:\n{output}"
    );
}

#[test]
fn switch_without_default_falls_through_to_end_label() {
    let output = transform_and_print(
        "async function f() { switch (x) { case y: a; break; case await z: b; break; } }",
    );

    assert!(
        output.contains("case y: return [3 /*break*/, 2];"),
        "Non-suspending leading clause must dispatch in its own group.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, z];"),
        "Suspending clause expression must start a new dispatch group after a yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case _b.sent(): return [3 /*break*/, 3];"),
        "Second dispatch group must compare the resumed value.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 4];")
            && output.contains("case 4: return [2 /*return*/];"),
        "With no default, the post-dispatch fallthrough must target the switch-end label.\nOutput:\n{output}"
    );
}

#[test]
fn switch_default_fallthrough_without_break_sets_label() {
    let output = transform_and_print(
        "async function f() { switch (x) { default: c; case y: a; break; case await z: b; break; } }",
    );

    assert!(
        output.contains("c;") && output.contains("_b.label = 3;"),
        "A non-terminating clause (default without break) must set `_b.label` so it falls through to the next case.\nOutput:\n{output}"
    );
}

#[test]
fn switch_discriminant_await_with_plain_body_keeps_switch_intact() {
    let output = transform_and_print(
        "async function f() { switch (await x) { case y: a; break; default: b; break; } }",
    );

    assert!(
        output.contains("return [4 /*yield*/, x];"),
        "A suspending discriminant must be yielded.\nOutput:\n{output}"
    );
    assert!(
        output.contains("switch (_a.sent()) {"),
        "When only the discriminant suspends, the switch stays intact over the sent value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return [3 /*break*/"),
        "A non-suspending case block must not be lowered into a dispatch state machine.\nOutput:\n{output}"
    );
}

#[test]
fn generator_switch_with_yield_lowers_like_async() {
    let output = transform_generator_and_print(
        "function* g() { switch (x) { case y: a; break; case yield z: b; break; } }",
    );

    assert!(
        output.contains("_a = x;"),
        "Generator-mode switch lowering must cache the discriminant just like async mode.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, z];"),
        "A `yield` in a generator switch clause expression must yield before the dispatch.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case _b.sent(): return [3 /*break*/, 3];"),
        "Generator-mode dispatch must compare the resumed value identically to async mode.\nOutput:\n{output}"
    );
}
