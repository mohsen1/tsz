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
        transformer.set_source_text(source);
        let ir = transformer.transform_async_function(func_idx);
        IRPrinter::emit_to_string(&ir)
    } else {
        String::new()
    }
}

// Structural rule: when an async ES5 function contains a try statement where
// any region (try, catch, finally) suspends on `await`, the generator state
// machine must emit a 4-tuple `_a.trys.push([start, catch, finally, end])`
// where each absent slot is sparse, allocate region boundary labels
// just-in-time so resume labels stay sequential within each region, bind the
// caught exception via `_a.sent()`, and route breaks from both try and catch
// to the region's *end* label. tsc's `__generator` driver routes the pending
// break through a finally region by pushing onto `_.ops`; breaking directly
// to the finally label would leave `_.ops` empty and wedge `endfinally`.

#[test]
fn try_catch_finally_with_await_in_each_block_lowers_to_sequential_state_machine() {
    let output = transform_and_print(
        "async function f() { try { await a(); } catch { await b(); } finally { await c(); } }",
    );

    assert!(
        output.contains("_a.trys.push([0, 2, 4, 6]);"),
        "trys.push should reserve the four region labels in start/catch/finally/end order so the runtime can dispatch throws and breaks correctly.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 0:") && output.contains("return [4 /*yield*/, a()];"),
        "Try body yield must live at the start label that trys.push references.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 1:") && output.contains("return [3 /*break*/, 6];"),
        "Try-body resume must break to the region end label so the runtime can route through finally via `_.ops`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: return [4 /*yield*/, b()];"),
        "Catch body must start at the catch label and emit its yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 3:") && output.contains("return [3 /*break*/, 6];"),
        "Catch-body resume must also break to the region end label; the driver pushes the break onto `_.ops` and dispatches finally for cleanup.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 4: return [4 /*yield*/, c()];"),
        "Finally body must start at the finally label and emit its yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 5:") && output.contains("return [7 /*endfinally*/];"),
        "Finally-body resume must end the region with endfinally.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 6: return [2 /*return*/];"),
        "End label must finalize the function.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a[1]"),
        "Catch should bind through _a.sent(), never raw element access on _a.\nOutput:\n{output}"
    );
}

#[test]
fn try_catch_finally_bound_exception_uses_sent_for_catch_binding() {
    let output = transform_and_print(
        "async function f() { try { await a(); } catch (e) { await b(e); } finally { await c(); } }",
    );

    assert!(
        output.contains("e = _a.sent();"),
        "Bound catch must capture the exception via `_a.sent()`.\nOutput:\n{output}"
    );
}

#[test]
fn try_catch_finally_uses_user_chosen_catch_binding_name() {
    // The catch binding name flows through unchanged — the fix must be
    // structural, not keyed on the spelling `e`.
    let output = transform_and_print(
        "async function f() { try { await a(); } catch (err) { await b(err); } finally { await c(); } }",
    );

    assert!(
        output.contains("err = _a.sent();"),
        "Catch binding spelled `err` must round-trip through the `_a.sent()` capture.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("e = _a.sent();"),
        "No phantom `e` binding should appear when the user named the variable `err`.\nOutput:\n{output}"
    );
}

#[test]
fn try_finally_without_catch_in_async_keeps_sparse_catch_slot() {
    let output =
        transform_and_print("async function f() { try { await a(); } finally { await c(); } }");

    assert!(
        output.contains("_a.trys.push([0, , 2, 4]);"),
        "Try-finally without a catch must emit the sparse catch slot.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 1:") && output.contains("return [3 /*break*/, 4];"),
        "Try-body break must target the end label even when a finally is present — the runtime routes the break through finally via `_.ops`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: return [4 /*yield*/, c()];"),
        "Finally body must start at the allocated finally label.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 4: return [2 /*return*/];"),
        "End label must come after the finally body.\nOutput:\n{output}"
    );
}

#[test]
fn try_catch_without_finally_in_async_keeps_sparse_finally_slot() {
    let output =
        transform_and_print("async function f() { try { await a(); } catch (e) { await b(e); } }");

    assert!(
        output.contains("_a.trys.push([0, 2, , 4]);"),
        "Try-catch without a finally must emit the sparse finally slot.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 1:") && output.contains("return [3 /*break*/, 4];"),
        "Try-body break must target the end label when no finally is present.\nOutput:\n{output}"
    );
    assert!(
        output.contains("e = _a.sent();"),
        "Bound catch must bind the exception via `_a.sent()`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 3:") && output.contains("return [3 /*break*/, 4];"),
        "Catch-body resume must break to the end label when no finally is present.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 4: return [2 /*return*/];"),
        "End label must come right after the catch body.\nOutput:\n{output}"
    );
}

#[test]
fn try_with_await_only_in_try_body_still_threads_through_finally() {
    let output = transform_and_print(
        "async function f() { try { await a(); } catch (e) { e; } finally { c(); } }",
    );

    assert!(
        output.contains("_a.trys.push([0, 2, 3, 4]);"),
        "When catch and finally are sync, their labels still occupy distinct slots after the try-body resume.\nOutput:\n{output}"
    );
    assert!(
        output.contains("e = _a.sent();") && output.contains("e;"),
        "Bound catch with no await still uses `_a.sent()`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [7 /*endfinally*/];"),
        "Finally must end with endfinally even if its body is sync.\nOutput:\n{output}"
    );
}

#[test]
fn nested_try_in_async_uses_distinct_placeholder_regions() {
    let output = transform_and_print(
        "async function f() { try { try { await a(); } catch { await b(); } } finally { await c(); } }",
    );

    assert!(
        output.contains("_a.trys.push([0, , 5, 7]);"),
        "Outer try-finally must reserve start=0, finally=5, end=7 after all inner labels.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.trys.push([0, 2, , 4]);"),
        "Inner try-catch must reserve start=0, catch=2, end=4 — independent of the outer region's placeholders.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 5: return [4 /*yield*/, c()];"),
        "Outer finally body must yield at its allocated label.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 7: return [2 /*return*/];"),
        "Outer end label must follow the finally body.\nOutput:\n{output}"
    );
}

#[test]
fn async_try_block_without_handler_emits_inline_body() {
    // A try with no catch and no finally is structurally equivalent to its
    // body; no trys.push entry should be emitted.
    let output = transform_and_print("async function f() { try { await a(); } }");

    assert!(
        !output.contains("_a.trys.push"),
        "A handler-less try should not produce a trys.push entry.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, a()];"),
        "The body of a handler-less try must still emit the yield.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_declaration_in_async_body_uses_structured_es5_assignment() {
    let output =
        transform_and_print("async function foo() { class C extends B { static { await; } } }");

    assert!(
        output.contains("var C;\n        return __generator"),
        "Class declarations inside async bodies should hoist the class binding to the awaiter scope.\nOutput:\n{output}"
    );
    assert!(
        output.contains("C = /** @class */ (function (_super)"),
        "Class declarations inside async bodies should lower to an assignment, not raw class syntax.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("class C extends B"),
        "Async generator cases must not fall back to raw class source.\nOutput:\n{output}"
    );
}

fn transform_generator_and_print(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&func_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = AsyncES5Transformer::new(&parser.arena);
        transformer.set_source_text(source);
        let ir = transformer.transform_generator_function(func_idx);
        IRPrinter::emit_to_string(&ir)
    } else {
        String::new()
    }
}

fn label_assignment_before(output: &str, needle: &str) -> String {
    let needle_pos = output
        .find(needle)
        .unwrap_or_else(|| panic!("Missing needle `{needle}`.\nOutput:\n{output}"));
    let prefix = &output[..needle_pos];
    let marker = "_a.label = ";
    let label_pos = prefix.rfind(marker).unwrap_or_else(|| {
        panic!("Missing label assignment before `{needle}`.\nOutput:\n{output}")
    });
    prefix[label_pos + marker.len()..]
        .split(';')
        .next()
        .expect("split always returns one segment")
        .trim()
        .to_string()
}

fn if_break_target_after(output: &str, needle: &str) -> String {
    let needle_pos = output
        .find(needle)
        .unwrap_or_else(|| panic!("Missing needle `{needle}`.\nOutput:\n{output}"));
    let suffix = &output[needle_pos..];
    let marker = "return [3 /*break*/, ";
    let target_pos = suffix
        .find(marker)
        .unwrap_or_else(|| panic!("Missing generator break after `{needle}`.\nOutput:\n{output}"));
    suffix[target_pos + marker.len()..]
        .split(']')
        .next()
        .expect("split always returns one segment")
        .trim()
        .to_string()
}

fn count_substring(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
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
fn test_async_if_then_await_else_uses_resume_before_else_label() {
    let output = transform_and_print(
        "async function test(skip: boolean) { if (!skip) { await 1 } else { throw Error('test') } }",
    );

    assert!(
        output.contains("if (!!skip) return [3 /*break*/, 2];"),
        "The initial branch should jump over the then-resume case into the else case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 1:")
            && output.contains("_a.sent();")
            && output.contains("return [3 /*break*/, 3];"),
        "The then branch should resume at case 1 before jumping to the final case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: throw Error('test');"),
        "The else branch should start after the then resume case.\nOutput:\n{output}"
    );
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
fn test_async_while_body_await_parenthesizes_binary_condition() {
    let output = transform_and_print(
        "async function f(i, limit) { while (i < limit) { await tick(i++); } }",
    );

    assert!(
        output.contains("if (!(i < limit)) return [3 /*break*/, 2];"),
        "Binary while condition should be negated with parentheses.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, tick(i++)];"),
        "Await in the while body should still lower to a generator yield.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_body_await_lowers_to_generator_cases() {
    let output = transform_and_print(
        "async function f(xs) { do { await g(xs.pop()); } while (xs.length); }",
    );

    assert!(
        !output.contains("do "),
        "Raw do-while statement must not remain around suspended body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain in ES5 generator output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, g(xs.pop())];"),
        "Await in the do-while body should become the first generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!xs.length) return [3 /*break*/, 2];"),
        "Do-while condition should be checked after the body resumes.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 0];"),
        "Truthy do-while condition should jump back to the body case.\nOutput:\n{output}"
    );
    let yield_pos = output.find("return [4 /*yield*/, g(xs.pop())];");
    let condition_pos = output.find("if (!xs.length) return [3 /*break*/, 2];");
    assert!(
        yield_pos.is_some()
            && condition_pos.is_some()
            && yield_pos.expect("yield_pos is Some, checked above")
                < condition_pos.expect("condition_pos is Some, checked above"),
        "Do-while lowering must preserve body-first semantics.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: return [2 /*return*/];"),
        "Do-while exit should have a final generator return case.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_single_statement_await_uses_post_body_binary_condition() {
    let output = transform_and_print(
        "async function f(i, limit) { do await tick(i++); while (i < limit); }",
    );

    assert!(
        !output.contains("do "),
        "Raw do-while statement must not remain around suspended single-statement body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain in ES5 generator output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, tick(i++)];"),
        "Single-statement do-while body await should become a generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!(i < limit)) return [3 /*break*/, 2];"),
        "Binary do-while condition should be negated with parentheses after the body resumes.\nOutput:\n{output}"
    );
    let yield_pos = output.find("return [4 /*yield*/, tick(i++)];");
    let condition_pos = output.find("if (!(i < limit)) return [3 /*break*/, 2];");
    assert!(
        yield_pos.is_some()
            && condition_pos.is_some()
            && yield_pos.expect("yield_pos is Some, checked above")
                < condition_pos.expect("condition_pos is Some, checked above"),
        "Do-while single-statement lowering must preserve body-first semantics.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_continue_jumps_to_post_body_condition() {
    let output = transform_and_print(
        "async function f(skip, keepGoing) { do { if (skip) continue; await tick(); } while (keepGoing()); }",
    );

    assert!(
        !output.contains("continue;"),
        "Loop-local continue must become a generator jump, not raw JS inside a switch case.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain in ES5 generator output.\nOutput:\n{output}"
    );
    let condition_label = label_assignment_before(&output, "if (!keepGoing())");
    let continue_jump = format!("return [3 /*break*/, {condition_label}];");
    let condition_pos = output.find("if (!keepGoing())");
    let continue_jump_pos = output.find(&continue_jump);
    assert!(
        continue_jump_pos.is_some()
            && condition_pos.is_some()
            && continue_jump_pos.expect("continue jump is present")
                < condition_pos.expect("condition check is present"),
        "Continue should jump forward to the post-body condition check before the loop can repeat.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_break_jumps_to_loop_exit() {
    let output = transform_and_print(
        "async function f(done, keepGoing) { do { if (done) break; await tick(); } while (keepGoing()); }",
    );

    assert!(
        !output.contains("break;"),
        "Loop-local break must become a generator jump, not raw JS inside a switch case.\nOutput:\n{output}"
    );
    assert!(
        {
            let exit_label = if_break_target_after(&output, "if (!keepGoing())");
            count_substring(&output, &format!("return [3 /*break*/, {exit_label}];")) >= 2
        },
        "Break and falsy condition should both target the generated loop-exit case.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_single_if_body_continue_jumps_to_condition() {
    let output = transform_and_print(
        "async function f(skip, keepGoing) { do if (skip) { continue; } else { await tick(); } while (keepGoing()); }",
    );

    assert!(
        !output.contains("continue;"),
        "Loop-local continue in a single-statement do body must become a generator jump.\nOutput:\n{output}"
    );
    let condition_label = label_assignment_before(&output, "if (!keepGoing())");
    let continue_jump = format!("return [3 /*break*/, {condition_label}];");
    let condition_pos = output.find("if (!keepGoing())");
    let continue_jump_pos = output.find(&continue_jump);
    assert!(
        continue_jump_pos.is_some()
            && condition_pos.is_some()
            && continue_jump_pos.expect("continue jump is present")
                < condition_pos.expect("condition check is present"),
        "Single-statement do body continue should target the post-body condition check.\nOutput:\n{output}"
    );
}

// Structural rule (added 2026-05-20, issue #8515):
// When the top-level expression of a `while` or `do-while` condition inside an
// async function is `await <expr>`, the condition must be lowered into a
// generator yield. `_a.sent()` then becomes the boolean tested by the IfBreak,
// so no raw `await` syntax remains in the generator body.
//
// Each test uses a different name choice for the awaited callee (`ok`,
// `keepGoing`, `tick`, `done`, `skip`, `body`) to prove the rule is structural
// rather than keyed on any particular identifier.

#[test]
fn test_async_while_condition_await_yields_then_branches_on_sent() {
    let output = transform_and_print("async function f() { while (await ok()) { work(); } }");
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain in ES5 generator output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 0: return [4 /*yield*/, ok()];"),
        "While condition await should lower into the loop-entry yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!_a.sent()) return [3 /*break*/, 2];"),
        "Post-resume IfBreak must read `_a.sent()` instead of the original condition.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 0];"),
        "Loop body must jump back to the yield case to re-evaluate the condition.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_while_condition_await_and_body_await_chain_yields() {
    let output = transform_and_print("async function f() { while (await ok()) { await tick(); } }");
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain when both condition and body suspend.\nOutput:\n{output}"
    );
    let cond_yield = output.find("return [4 /*yield*/, ok()];");
    let body_yield = output.find("return [4 /*yield*/, tick()];");
    assert!(
        cond_yield.is_some()
            && body_yield.is_some()
            && cond_yield.expect("condition yield present")
                < body_yield.expect("body yield present"),
        "Condition yield must precede body yield in the generated case order.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_condition_await_yields_after_body_in_same_case() {
    let output =
        transform_and_print("async function f() { do { work(); } while (await keepGoing()); }");
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain when only the do-while condition suspends.\nOutput:\n{output}"
    );
    // Without `continue` in the body, body and condition-yield share a single
    // case: the body runs, then the yield is the case's terminating return.
    assert!(
        output.contains("work();"),
        "Body statement should still be emitted in the first case.\nOutput:\n{output}"
    );
    let body_pos = output.find("work();");
    let yield_pos = output.find("return [4 /*yield*/, keepGoing()];");
    assert!(
        body_pos.is_some()
            && yield_pos.is_some()
            && body_pos.expect("body present") < yield_pos.expect("condition yield present"),
        "Body must precede the condition yield (do-while semantics).\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!_a.sent()) return [3 /*break*/, 2];"),
        "Resume case must test `_a.sent()` to decide loop exit.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 0];"),
        "Truthy condition must jump back to the loop-entry case.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_both_body_and_condition_await_lower_separately() {
    let output = transform_and_print(
        "async function f() { do { await tick(); } while (await keepGoing()); }",
    );
    assert!(
        !output.contains("await "),
        "Raw await syntax must not remain when both body and condition suspend.\nOutput:\n{output}"
    );
    let body_yield = output.find("return [4 /*yield*/, tick()];");
    let cond_yield = output.find("return [4 /*yield*/, keepGoing()];");
    assert!(
        body_yield.is_some()
            && cond_yield.is_some()
            && body_yield.expect("body yield present")
                < cond_yield.expect("condition yield present"),
        "Body yield must precede condition yield for do-while semantics.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_parenthesized_await_condition_still_lowers() {
    // Redundant parens around the await must be stripped before the
    // top-level-await pattern match so this case is treated identically.
    let output =
        transform_and_print("async function f() { do { work(); } while ((await keepGoing())); }");
    assert!(
        output.contains("return [4 /*yield*/, keepGoing()];"),
        "Parenthesised await condition must still lower to a yield.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "No raw await syntax should remain after parenthesised condition lowering.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_condition_await_with_continue_uses_own_case_for_yield() {
    // With `continue`, the yield case must be separate from the body case so
    // continue can jump to the condition check without re-running the body.
    let output = transform_and_print(
        "async function f(skip) { do { if (skip) continue; body(); } while (await keepGoing()); }",
    );
    assert!(
        !output.contains("continue;"),
        "Unlabeled continue inside an awaited do-while body must become a generator branch.\nOutput:\n{output}"
    );
    // The continue jumps to the yield case; that case label is whatever
    // `label_assignment_before` recorded right before the awaited condition.
    let condition_yield_label = label_assignment_before(&output, "return [4 /*yield*/, keepGoing");
    assert!(
        output.contains(&format!("return [3 /*break*/, {condition_yield_label}];")),
        "Continue must jump to the awaited-condition yield case (label {condition_yield_label}).\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_condition_await_with_break_jumps_to_exit() {
    let output = transform_and_print(
        "async function f(done) { do { if (done) break; body(); } while (await keepGoing()); }",
    );
    assert!(
        !output.contains("break;"),
        "Unlabeled break inside an awaited do-while body must become a generator branch.\nOutput:\n{output}"
    );
    let exit_label = if_break_target_after(&output, "if (!_a.sent())");
    assert!(
        count_substring(&output, &format!("return [3 /*break*/, {exit_label}];")) >= 2,
        "Both the body break and the false-condition branch should target the loop exit case.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_while_condition_await_with_continue_jumps_back_to_yield_case() {
    let output = transform_and_print(
        "async function f(skip) { while (await keepGoing()) { if (skip) continue; body(); } }",
    );
    assert!(
        !output.contains("continue;"),
        "Unlabeled continue inside an awaited while body must become a generator branch.\nOutput:\n{output}"
    );
    // For `while`, the yield case is also the loop-entry case (case 0). The
    // continue must jump back to it so the condition re-evaluates next cycle.
    assert!(
        output.contains("case 0: return [4 /*yield*/, keepGoing()];"),
        "Loop entry should be the condition yield case.\nOutput:\n{output}"
    );
    assert!(
        count_substring(&output, "return [3 /*break*/, 0];") >= 2,
        "Both the continue and the natural loop back-edge should jump to the loop-entry case.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_while_condition_await_with_break_jumps_to_exit_case() {
    let output = transform_and_print(
        "async function f(done) { while (await keepGoing()) { if (done) break; body(); } }",
    );
    assert!(
        !output.contains("break;"),
        "Unlabeled break inside an awaited while body must become a generator branch.\nOutput:\n{output}"
    );
    let exit_label = if_break_target_after(&output, "if (!_a.sent())");
    assert!(
        count_substring(&output, &format!("return [3 /*break*/, {exit_label}];")) >= 2,
        "Both the body break and the false-condition branch should target the loop exit case.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_do_while_condition_await_renamed_identifiers_not_hardcoded() {
    // Different identifier names must produce the same structural shape; the
    // rule is about the AST kind (AwaitExpression at top-level of the
    // condition), not the spelling of the callee.
    let output =
        transform_and_print("async function gizmo() { do { step(); } while (await poll()); }");
    assert!(
        output.contains("return [4 /*yield*/, poll()];"),
        "Renamed callee should still produce the same yield lowering.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!_a.sent()) return [3 /*break*/, 2];"),
        "Renamed callee should still drive the post-resume IfBreak on `_a.sent()`.\nOutput:\n{output}"
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
fn test_await_using_in_async_body_lowers_to_generator_disposable_region() {
    let output = transform_and_print(
        "async function foo() { await using d = { async [Symbol.asyncDispose]() {} }; await done(); }",
    );

    assert!(
        output.contains("var env_1, d, e_1, result_1;"),
        "Disposable region names should be hoisted before the generator body: {output}"
    );
    assert!(
        output.contains("_b.trys.push([1, 3, 4, 7]);"),
        "The async state machine should plan a try/finally region around `await using`: {output}"
    );
    assert!(
        output.contains("d = __addDisposableResource(env_1"),
        "`await using` declarations should register with __addDisposableResource: {output}"
    );
    assert!(
        output.contains("result_1 = __disposeResources(env_1);"),
        "The finally region should dispose the resource stack: {output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, result_1];"),
        "Async disposal must suspend on the dispose promise: {output}"
    );
    assert!(
        !output.contains("await using"),
        "Raw `await using` syntax must not leak into ES5 output: {output}"
    );
}

#[test]
fn test_async_for_in_parenthesized_await_object_lowers_without_raw_fallback() {
    let output = transform_and_print(
        "async function f() { for (var k in (await getObj())) { await h(k); } }",
    );

    assert!(
        output.contains("return [4 /*yield*/, getObj()];"),
        "Parenthesized direct await in for-in object should be lowered before key snapshotting.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await getObj") && !output.contains("for (var k in (await"),
        "Raw suspended for-in syntax must not remain in async ES5 output.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_for_in_awaited_element_target_lowers_object_and_index() {
    let output = transform_and_print(
        "async function f(obj) { for ((await getBox())[await getKey()] in obj) { await h(); } }",
    );

    assert!(
        output.contains("return [4 /*yield*/, getBox()];")
            && output.contains("return [4 /*yield*/, getKey()];"),
        "Awaited for-in element target should suspend for object and index in order.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await getBox")
            && !output.contains("await getKey")
            && !output.contains("for ((await"),
        "Raw awaited element target must not remain in async ES5 output.\nOutput:\n{output}"
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

/// `await` wrapped in a TypeScript type-only expression
/// (`as T`, `<T>...`, `satisfies T`, non-null `!`) must still be
/// detected by `contains_await_recursive` and `find_suspension_expression`,
/// otherwise the IR transformer emits `_a.sent()` without a preceding
/// `[4 /*yield*/]` instruction in the generated state machine.
/// Devin review: <https://github.com/mohsen1/tsz/pull/2278#discussion_r3176478496>
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

// Regression: ASTRef inside async-ES5 IR must go through AstPrinter (not the
// raw source-text fallback) so a `do { ... } while (...);` body is re-formatted
// to the canonical tsc shape at the surrounding indent. Pre-fix, the AstPrinter
// path was gated on `transforms non-empty || base_printer_options`, and async
// function bodies (which attach neither) fell through to a `text[pos..end]`
// slice. That slice inherits any imprecision in `node.end` — notably for
// statements whose terminating `;` is consumed via `parse_optional`, leaving
// the captured `token_end()` at the *next* token — and spills source from the
// enclosing block's closing `}` into the emitted output.
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

// ---------------------------------------------------------------------
// Discovery-phase boundary tests
//
// `body_contains_await` is the entry point of the read-only discovery
// pass that lowering decisions key on. These tests exercise the
// discovery module (`async_es5_ir_discovery.rs`) without going through
// the full IR-print path, so they fail fast when the predicate boundary
// drifts.

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

// ---------------------------------------------------------------------
// Async/generator state-machine ES2020+ operator lowering
//
// The state machine path (`AsyncES5Transformer`) is only engaged when
// the target predates native async/generators. At those targets `??` /
// `?.` / logical-assignment also do not exist, so the IR converter must
// lower them inline. The IR printer writes operators verbatim, so any
// ?? that survives into the IR ends up emitted as raw `??` — illegal
// ES5 syntax in tsc's baseline.

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
