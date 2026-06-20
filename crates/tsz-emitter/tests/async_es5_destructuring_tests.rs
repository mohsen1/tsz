//! ES5 emit parity for destructuring inside `async` functions.
//!
//! The async->generator IR pipeline previously passed binding patterns through
//! verbatim, emitting syntactically invalid JavaScript (`var ;`, ` = obj;`) for
//! every destructuring `var`/`const` and dropping destructuring `catch`
//! bindings entirely. Each assertion below is the exact body `tsc` 6.x emits at
//! `--target es5` (verified by differential testing). Binder names are varied
//! per case so the lowering cannot key on any identifier text.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    emit_es5_opts(source, false)
}

fn emit_es5_downlevel(source: &str) -> String {
    emit_es5_opts(source, true)
}

fn emit_es5_opts(source: &str, downlevel_iteration: bool) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        remove_comments: true,
        downlevel_iteration,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

const PRELUDE: &str = "declare function g(): Promise<any>; declare const obj: any; declare const arr: any[]; \
     declare function h(): any; declare const k: any; declare function use(...a: any[]): void;";

fn assert_contains(output: &str, needle: &str, why: &str) {
    assert!(
        output.contains(needle),
        "{why}\nexpected to find:\n  {needle}\nin output:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Object binding patterns
// ---------------------------------------------------------------------------

#[test]
fn object_pattern_identifier_source_reused_without_temp() {
    // `tsc`: identifier source is reused per element (no temp).
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ a, b }} = obj; use(a, b); }}"
    ));
    assert_contains(&output, "var a, b;", "bound names hoisted");
    assert_contains(&output, "a = obj.a, b = obj.b;", "comma-joined extraction");
    assert!(
        !output.contains("var ;") && !output.contains(" = obj;\n"),
        "must not emit the invalid pre-fix shape.\nOutput:\n{output}"
    );
}

#[test]
fn object_pattern_call_source_captured_into_temp() {
    // Non-identifier source is read once via a temp.
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ first, second }} = h(); use(first, second); }}"
    ));
    assert_contains(&output, "var _a, first, second;", "temp + names hoisted");
    assert_contains(
        &output,
        "_a = h(), first = _a.first, second = _a.second;",
        "source captured once, then extracted",
    );
}

#[test]
fn object_pattern_single_element_inlines_source() {
    // A single non-computed element reads the source exactly once: no temp.
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ only }} = h(); use(only); }}"
    ));
    assert_contains(&output, "only = h().only;", "single element inlines call");
    assert!(
        !output.contains("_a = h()"),
        "single element must not allocate a source temp.\nOutput:\n{output}"
    );
}

#[test]
fn object_pattern_renamed_and_default() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ value = 5, label: renamed }} = obj; use(value, renamed); }}"
    ));
    assert_contains(&output, "var _a, value, renamed;", "default temp + names");
    assert_contains(
        &output,
        "_a = obj.value, value = _a === void 0 ? 5 : _a, renamed = obj.label;",
        "default uses a temp + void-0 ternary; rename reads source property",
    );
}

#[test]
fn object_pattern_rest_uses_rest_helper() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ head, ...others }} = obj; use(head, others); }}"
    ));
    assert_contains(
        &output,
        "head = obj.head, others = __rest(obj, [\"head\"]);",
        "object rest excludes preceding keys via __rest",
    );
    assert_contains(&output, "var __rest", "the __rest helper is emitted");
}

#[test]
fn object_pattern_nested_single_inlines_access_chain() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ outer: {{ inner }} }} = obj; use(inner); }}"
    ));
    assert_contains(&output, "inner = obj.outer.inner;", "nested single inlines");
}

#[test]
fn object_pattern_nested_with_default_uses_two_temps() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ a, b: {{ c }} = obj }} = obj; use(a, c); }}"
    ));
    assert_contains(
        &output,
        "a = obj.a, _a = obj.b, _b = _a === void 0 ? obj : _a, c = _b.c;",
        "default on a nested pattern uses an access temp and a defaulted temp",
    );
}

#[test]
fn object_pattern_computed_key_captures_source_and_key() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const {{ [\"x\" + k]: picked }} = obj; use(picked); }}"
    ));
    assert_contains(
        &output,
        "_a = obj, _b = \"x\" + k, picked = _a[_b];",
        "computed key forces a source temp and a key temp",
    );
}

// ---------------------------------------------------------------------------
// Array binding patterns
// ---------------------------------------------------------------------------

#[test]
fn array_pattern_index_access() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const [one, two] = arr; use(one, two); }}"
    ));
    assert_contains(
        &output,
        "one = arr[0], two = arr[1];",
        "index access per slot",
    );
}

#[test]
fn array_pattern_hole_skips_index() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const [, , third] = arr; use(third); }}"
    ));
    assert_contains(&output, "third = arr[2];", "elisions advance the index");
}

#[test]
fn array_pattern_rest_uses_slice() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const [lead, ...tail] = h(); use(lead, tail); }}"
    ));
    assert_contains(
        &output,
        "_a = h(), lead = _a[0], tail = _a.slice(1);",
        "array rest slices from its index",
    );
}

// ---------------------------------------------------------------------------
// Mixed / multi declarations in one statement
// ---------------------------------------------------------------------------

#[test]
fn mixed_identifier_and_pattern_join_one_statement() {
    // `tsc` joins every declaration in the statement into one comma chain.
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const plain = 1, {{ b, c }} = obj; use(plain, b, c); }}"
    ));
    assert_contains(
        &output,
        "plain = 1, b = obj.b, c = obj.c;",
        "plain decl joins the destructuring comma chain",
    );
}

// ---------------------------------------------------------------------------
// downlevelIteration array forms
// ---------------------------------------------------------------------------

#[test]
fn array_pattern_downlevel_iteration_uses_read_with_count() {
    let output = emit_es5_downlevel(&format!(
        "{PRELUDE} async function f() {{ await g(); const [p, q] = arr; use(p, q); }}"
    ));
    assert_contains(
        &output,
        "_a = __read(arr, 2), p = _a[0], q = _a[1];",
        "downlevelIteration reads with an explicit element count",
    );
}

#[test]
fn array_pattern_downlevel_iteration_rest_reads_without_count() {
    let output = emit_es5_downlevel(&format!(
        "{PRELUDE} async function f() {{ await g(); const [p, ...rest] = h(); use(p, rest); }}"
    ));
    assert_contains(
        &output,
        "_a = __read(h()), p = _a[0], rest = _a.slice(1);",
        "a rest element drops the read count",
    );
}

// ---------------------------------------------------------------------------
// Destructuring a directly-awaited initializer (`const { a } = await g()`)
// ---------------------------------------------------------------------------

#[test]
fn await_initializer_object_pattern_reads_sent_value() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ const {{ a, b }} = await g(); use(a, b); }}"
    ));
    assert_contains(&output, "var _a, a, b;", "source temp + names hoisted");
    assert_contains(
        &output,
        "_a = _b.sent(), a = _a.a, b = _a.b;",
        "the resumed value is captured then destructured",
    );
}

#[test]
fn await_initializer_array_pattern_reads_sent_value() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ const [x, y] = await g(); use(x, y); }}"
    ));
    assert_contains(
        &output,
        "_a = _b.sent(), x = _a[0], y = _a[1];",
        "array indices off the temp",
    );
}

#[test]
fn await_initializer_single_element_inlines_parenthesized_sent() {
    // A single element reads the resumed value inline; the call result is
    // parenthesized (`(_a.sent()).a`) so the member access stays well-formed.
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ const {{ only }} = await g(); use(only); }}"
    ));
    assert_contains(
        &output,
        "only = (_a.sent()).only;",
        "single element inlines the sent value",
    );
}

#[test]
fn await_initializer_object_rest_reads_sent_value() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ const {{ head, ...others }} = await g(); use(head, others); }}"
    ));
    assert_contains(
        &output,
        "_a = _b.sent(), head = _a.head, others = __rest(_a, [\"head\"]);",
        "rest excludes the named key from the captured resumed value",
    );
}

// ---------------------------------------------------------------------------
// catch-clause destructuring bindings
// ---------------------------------------------------------------------------

#[test]
fn catch_object_pattern_binds_via_sent_temp() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ try {{ await g(); }} catch ({{ message }}) {{ use(message); }} }}"
    ));
    assert_contains(
        &output,
        "var _a, message;",
        "catch temp + bound name hoisted",
    );
    assert_contains(&output, "_a = _b.sent();", "caught value bound to a temp");
    assert_contains(
        &output,
        "message = _a.message;",
        "pattern extracted from the temp",
    );
    assert!(
        !output.contains("catch ({") && !output.contains("catch (["),
        "the binding pattern must not survive as native ES5 catch syntax.\nOutput:\n{output}"
    );
}

#[test]
fn catch_object_pattern_multi_binding() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ try {{ await g(); }} catch ({{ code, detail }}) {{ use(code, detail); }} }}"
    ));
    assert_contains(
        &output,
        "code = _a.code, detail = _a.detail;",
        "multiple catch-pattern bindings are comma-joined",
    );
}

#[test]
fn catch_array_pattern_binding() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ try {{ await g(); }} catch ([reason, extra]) {{ use(reason, extra); }} }}"
    ));
    assert_contains(
        &output,
        "reason = _a[0], extra = _a[1];",
        "array catch pattern reads by index from the temp",
    );
}

// ---------------------------------------------------------------------------
// Controls: plain identifier bindings stay on the existing path.
// ---------------------------------------------------------------------------

#[test]
fn plain_identifier_catch_binding_unchanged() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ try {{ await g(); }} catch (err) {{ use(err); }} }}"
    ));
    assert_contains(
        &output,
        "err_1 = _a.sent();",
        "plain catch identifier keeps its renamed temp",
    );
}

#[test]
fn plain_identifier_var_binding_unchanged() {
    let output = emit_es5(&format!(
        "{PRELUDE} async function f() {{ await g(); const plain = h(); use(plain); }}"
    ));
    assert_contains(
        &output,
        "plain = h();",
        "plain identifier var keeps simple assignment",
    );
}
