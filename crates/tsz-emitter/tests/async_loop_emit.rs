use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn async_while_branch_continue_stays_in_current_case() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            while (x) {
                if (1) continue;
                await y;
            }
        }
        async function g() {
            D: while (x) {
                if (1) continue D;
                await y;
            }
        }",
    );

    assert!(
        output.contains("if (1)\n                        return [3 /*break*/, 0];\n                    return [4 /*yield*/, y];"),
        "Branch-local async while continues should emit one same-case generator backedge before the following await.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return [3 /*break*/, 0];\n                    return [3 /*break*/, 0];"),
        "The async while loop must not append a duplicate backedge after a terminating continue branch.\nOutput:\n{output}"
    );
}

#[test]
fn async_while_branch_break_targets_loop_exit() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            while (x) {
                if (1) break;
                await y;
            }
        }
        async function g() {
            H: while (x) {
                if (1) break H;
                await y;
            }
        }",
    );

    assert!(
        output.contains("if (1)\n                        return [3 /*break*/, 2];\n                    return [4 /*yield*/, y];"),
        "Branch-local async while breaks should target the loop exit in the current case, including labeled breaks.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("4294967295"),
        "Loop-exit placeholders inside branch-local labeled breaks must be patched before printing.\nOutput:\n{output}"
    );
}

#[test]
fn async_do_while_uses_condition_case_after_body() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            do {
                await x;
            } while (y);
        }
        async function g() {
            do {
                if (1) continue;
                await x;
            } while (y);
        }",
    );

    assert!(
        output.contains("_a.sent();\n                    _a.label = 2;\n                case 2:\n                    if (y) return [3 /*break*/, 0];"),
        "Async do-while fallthrough should enter a dedicated condition case with tsc's positive backedge test.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (1)\n                        return [3 /*break*/, 2];\n                    return [4 /*yield*/, x];"),
        "Do-while continues should jump to the post-body condition case, not directly to the loop body.\nOutput:\n{output}"
    );
}

#[test]
fn async_do_while_awaited_condition_uses_positive_sent_backedge() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            do {
                x;
            } while (await y);
        }
        async function g() {
            H: do {
                break H;
            } while (await y);
        }",
    );

    assert!(
        output.contains("case 1: return [4 /*yield*/, y];\n                case 2:\n                    if (_a.sent()) return [3 /*break*/, 0];"),
        "Awaited do-while conditions should resume through `_a.sent()` and use a positive loop backedge.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "case 0: return [3 /*break*/, 3];\n                case 1: return [4 /*yield*/, y];"
        ),
        "A labeled break in a do-while body should jump to the loop exit while preserving the condition case shape.\nOutput:\n{output}"
    );
}

#[test]
fn async_do_while_body_await_without_continue_materializes_condition_case() {
    // A do-while whose body awaits but contains no `continue` must still get a
    // dedicated condition case (the post-body fallthrough). tsc emits
    // `_a.sent(); _a.label = 2; case 2: if (cond) return [3 /*break*/, 0]; _a.label = 3;`
    // rather than collapsing the condition into the body case with a negated
    // test. Use non-default identifiers to prove the rule keys on the do-while
    // structure, not on a particular spelling.
    let output = emit_es5(
        "declare var first, second: any;
        async function run() {
            do {
                await first;
            } while (second);
        }",
    );

    assert!(
        output.contains("_a.sent();\n                    _a.label = 2;\n                case 2:\n                    if (second) return [3 /*break*/, 0];\n                    _a.label = 3;\n                case 3: return [2 /*return*/];"),
        "A no-continue do-while with an awaiting body should fall through to a dedicated condition case using tsc's positive backedge test.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("if (!second)"),
        "The do-while condition must use the positive backedge test, never a negated `if (!cond)` collapse.\nOutput:\n{output}"
    );
}

#[test]
fn async_do_while_unlabeled_break_targets_exit_with_condition_case() {
    // An unlabeled `break` inside an awaiting do-while body jumps directly to
    // the loop exit case, while the post-body condition case is still
    // materialized. Renamed identifiers keep the assertion structural.
    let output = emit_es5(
        "declare var alpha, beta: any;
        async function run() {
            do {
                await alpha;
                break;
            } while (beta);
        }",
    );

    // body: yield alpha (case 0), then `_a.sent(); break -> exit` (case 1).
    assert!(
        output.contains("_a.sent();\n                    return [3 /*break*/, 3];"),
        "An unlabeled break should jump straight to the do-while exit case.\nOutput:\n{output}"
    );
    // condition case is still present with the positive backedge test.
    assert!(
        output.contains("case 2:\n                    if (beta) return [3 /*break*/, 0];\n                    _a.label = 3;\n                case 3: return [2 /*return*/];"),
        "The do-while condition case must remain materialized even when the body breaks.\nOutput:\n{output}"
    );
}

#[test]
fn async_conditional_true_await_reserves_false_label_after_resume() {
    let output = emit_es5(
        "declare var x, y, z, a: any;
        async function f() {
            a = x ? await y : z;
        }",
    );

    assert!(
        output.contains("if (!x) return [3 /*break*/, 2];\n                    return [4 /*yield*/, y];\n                case 1:\n                    _a = _b.sent();\n                    return [3 /*break*/, 3];\n                case 2:\n                    _a = z;\n                    _b.label = 3;\n                case 3:\n                    a = _a;"),
        "A true-branch await in a conditional assignment should branch around the yield resume case before assigning the final target.\nOutput:\n{output}"
    );
}

#[test]
fn async_conditional_false_await_reserves_end_label_after_resume() {
    let output = emit_es5(
        "declare var x, y, z, a: any;
        async function f() {
            a = x ? y : await z;
        }",
    );

    assert!(
        output.contains("if (!x) return [3 /*break*/, 1];\n                    _a = y;\n                    return [3 /*break*/, 3];\n                case 1: return [4 /*yield*/, z];\n                case 2:\n                    _a = _b.sent();\n                    _b.label = 3;\n                case 3:\n                    a = _a;"),
        "A false-branch await in a conditional assignment should keep the non-await branch's end jump after the yield resume case.\nOutput:\n{output}"
    );
}

#[test]
fn async_with_statement_captures_awaited_expression_before_body_case() {
    let output = emit_es5(
        "declare var x, y: any;
        async function f() {
            with (await x) {
                y;
            }
        }",
    );

    assert!(
        output.contains("case 0: return [4 /*yield*/, x];\n                case 1:\n                    _a = _b.sent();\n                    _b.label = 2;\n                case 2:\n                    with (_a) {\n                        y;\n                    }\n                    _b.label = 3;\n                case 3: return [2 /*return*/];"),
        "Awaited with expressions should be captured before entering a dedicated with-body case.\nOutput:\n{output}"
    );
}

#[test]
fn async_with_statement_wraps_suspended_body_segments() {
    let output = emit_es5(
        "declare var x, y, z, a, b: any;
        async function f() {
            with (x) {
                with (z) {
                    a;
                    await y;
                    b;
                }
            }
        }",
    );

    assert!(
        output.contains("case 1:\n                    with (_a) {\n                        _b = z;\n                    }\n                    _c.label = 2;\n                case 2:\n                    with (_a) {\n                        with (_b) {\n                            a;\n                            return [4 /*yield*/, y];\n                        }\n                    }\n                case 3:\n                    with (_a) {\n                        with (_b) {\n                            _c.sent();\n                            b;\n                        }\n                    }\n                    _c.label = 4;"),
        "Suspended with bodies should wrap both the yield segment and resume segment in the captured with scopes.\nOutput:\n{output}"
    );
}

#[test]
fn async_body_hoists_function_and_nested_var_declarations() {
    let output = emit_es5(
        "declare var y: any;
        async function f() {
            var a0, a1 = 1;
            function z() {
                var b0, b1 = 1;
            }
            await 0;
            if (true) {
                var c0, c1 = 1;
            }
            for (var a = 0; y;) {
            }
        }",
    );

    assert!(
        output.contains(
            "function z() {\n            var b0, b1 = 1;\n        }\n        var a0, a1, c0, c1, a;"
        ),
        "Async body function declarations should hoist before async-body var declarations, while nested function vars stay local.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (true) {\n                        c1 = 1;\n                    }"),
        "Nested async-body var declarations should hoist and leave initializer assignments in place.\nOutput:\n{output}"
    );
}

#[test]
fn for_await_captured_iteration_binding_uses_plain_loop_helper() {
    let output = emit_es5(
        "async function* stream() { yield 1; }
        function wait() { return Promise.resolve(); }
        const log = console.log;
        (async () => {
            for await (const entry of stream()) {
                log(`loop ${entry}`);
                (async () => {
                    const inner = entry;
                    await wait();
                    log(`inner ${inner} ${entry}`);
                })();
            }
        })();",
    );

    assert!(
        output.contains("_loop_1 = function ()"),
        "Captured for-await iteration variables should get a per-iteration helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var entry =") && output.contains("log(\"loop \".concat(entry));"),
        "The captured binding should be local to the helper and template literals should lower with tsc's concat shape.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return __awaiter(void 0, void 0, void 0, function ()")
            && output.contains("log(\"inner \".concat(inner, \" \").concat(entry));"),
        "Nested async arrows inside the helper should still lower through __awaiter and close over the helper-local binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1();"),
        "The async iterator state machine should call the helper for each awaited result.\nOutput:\n{output}"
    );
}
