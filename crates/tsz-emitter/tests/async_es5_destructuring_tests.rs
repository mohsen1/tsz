//! ES5 down-level emit for destructuring binding patterns inside `async`
//! function bodies (issue #14080).
//!
//! The async→generator IR pipeline previously passed binding patterns through
//! verbatim, emitting syntactically-invalid JavaScript (`var ;` / ` = obj;`)
//! for destructuring `var`/`const` declarations and silently dropping
//! destructuring `catch` bindings. These tests pin the down-leveled output to
//! `tsc`'s `flattenBindingOrAssignmentElement` form: hoisted names plus a
//! comma-joined extraction expression. Binder names are varied per case so the
//! lowering keys on pattern shape, never on a specific identifier.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;
use tsz_emitter::output::printer::PrintOptions;

fn emit_es5(source: &str) -> String {
    parse_and_lower_print(source, PrintOptions::es5())
}

fn emit_es5_downlevel(source: &str) -> String {
    let mut opts = PrintOptions::es5();
    opts.downlevel_iteration = true;
    parse_and_lower_print(source, opts)
}

/// The pre-fix bug witness: a destructuring `const` after an `await` must not
/// emit an empty `var ;` or a nameless ` = obj;` assignment.
#[test]
fn object_pattern_after_await_extracts_each_binding() {
    let output = emit_es5("async function f() { await g(); const { a, b } = obj; sink(a, b); }");
    assert!(
        output.contains("a = obj.a, b = obj.b;"),
        "object pattern should extract each property; got:\n{output}"
    );
    assert!(
        !output.contains("var ;") && !output.contains(" = obj;\n"),
        "invalid empty var / nameless assignment must not appear; got:\n{output}"
    );
}

/// Leaf names and temporaries are hoisted into the `__awaiter` wrapper's `var`
/// list, leaving only the assignment inline.
#[test]
fn extracted_names_are_hoisted_not_declared_inline() {
    let output = emit_es5("async function f() { await g(); const { first, second } = pair; }");
    assert!(
        output.contains("var first, second;"),
        "binding names should be hoisted; got:\n{output}"
    );
}

/// Property rename and a defaulted binding take `tsc`'s temp form
/// `_t = src.p, name = _t === void 0 ? d : _t`.
#[test]
fn rename_and_default_use_tsc_temp_form() {
    let output =
        emit_es5("async function f() { await g(); const { message: note, code = 500 } = err; }");
    assert!(
        output.contains("note = err.message, _a = err.code, code = _a === void 0 ? 500 : _a;"),
        "rename + default should match tsc's temp form; got:\n{output}"
    );
}

/// A single-element nested pattern inlines the access path (no temp), matching
/// `tsc`'s `flattenObjectBindingOrAssignmentPattern` fast path.
#[test]
fn single_nested_pattern_inlines_access_path() {
    let output = emit_es5("async function f() { await g(); const { outer: { inner } } = box; }");
    assert!(
        output.contains("inner = box.outer.inner;"),
        "single nested pattern should inline the access path; got:\n{output}"
    );
}

/// Array binding patterns index into the source; holes are skipped.
#[test]
fn array_pattern_indexes_and_skips_holes() {
    let output = emit_es5("async function f() { await g(); const [head, , third] = list; }");
    assert!(
        output.contains("head = list[0], third = list[2];"),
        "array pattern should index and skip the hole; got:\n{output}"
    );
}

/// Object rest lowers to `__rest` with the excluded keys, and the helper is
/// emitted.
#[test]
fn object_rest_uses_rest_helper() {
    let output = emit_es5("async function f() { await g(); const { kept, ...others } = bag; }");
    assert!(
        output.contains("kept = bag.kept, others = __rest(bag, [\"kept\"]);"),
        "object rest should call __rest with excluded keys; got:\n{output}"
    );
    assert!(
        output.contains("var __rest ="),
        "the __rest helper definition must be emitted; got:\n{output}"
    );
}

/// Array rest slices the tail.
#[test]
fn array_rest_slices_tail() {
    let output = emit_es5("async function f() { await g(); const [lead, ...rest] = items; }");
    assert!(
        output.contains("lead = items[0], rest = items.slice(1);"),
        "array rest should slice the tail; got:\n{output}"
    );
}

/// A non-identifier initializer with multiple bindings is captured into a temp
/// once, matching `tsc`'s `ensureIdentifier`.
#[test]
fn complex_initializer_is_captured_once() {
    let output = emit_es5("async function f() { await g(); const { x, y } = make(); }");
    assert!(
        output.contains("_a = make(), x = _a.x, y = _a.y;"),
        "complex initializer should be captured into one temp; got:\n{output}"
    );
}

/// A destructuring `catch` binding extracts from the caught value instead of
/// being dropped.
#[test]
fn catch_pattern_extracts_from_caught_value() {
    let output =
        emit_es5("async function f() { try { await g(); } catch ({ reason }) { sink(reason); } }");
    assert!(
        output.contains("reason = _a.reason;"),
        "catch pattern should extract from the caught value; got:\n{output}"
    );
    assert!(
        output.contains(".sent();"),
        "the caught value should be bound via the generator sent value; got:\n{output}"
    );
}

/// `await` in the initializer captures the resolved value into a temp, then
/// destructures from it.
#[test]
fn awaited_initializer_destructures_resolved_value() {
    let output = emit_es5("async function f() { const { a, b } = await load(); }");
    assert!(
        output.contains("a = _a.a, b = _a.b;"),
        "an awaited initializer should be destructured after resolution; got:\n{output}"
    );
    assert!(
        output.contains(".sent();"),
        "the resolved value should come from the generator sent value; got:\n{output}"
    );
}

/// The plain-identifier `catch (e)` path is unaffected by the pattern handling.
#[test]
fn plain_identifier_catch_binding_unchanged() {
    let output = emit_es5("async function f() { try { await g(); } catch (e) { sink(e); } }");
    assert!(
        output.contains("e_1 = _a.sent();"),
        "plain identifier catch should still bind the sent value; got:\n{output}"
    );
    assert!(
        !output.contains("__rest"),
        "a plain catch should not pull in destructuring helpers; got:\n{output}"
    );
}

/// Under `downlevelIteration`, an array binding pattern reads the iterable via
/// `__read` before indexing.
#[test]
fn downlevel_iteration_array_pattern_uses_read_helper() {
    let output = emit_es5_downlevel("async function f() { await g(); const [one, two] = seq; }");
    assert!(
        output.contains("__read(seq, 2)"),
        "downlevelIteration array pattern should use __read; got:\n{output}"
    );
}

/// Binder names do not drive the lowering: a differently-named witness emits
/// the structurally identical shape.
#[test]
fn lowering_is_binder_name_agnostic() {
    let output = emit_es5("async function f() { await g(); const { alpha, beta } = src; }");
    assert!(
        output.contains("alpha = src.alpha, beta = src.beta;"),
        "renamed binders should produce the same structural shape; got:\n{output}"
    );
}
