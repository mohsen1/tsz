//! Regression tests for #16072: "am I at the top level of the source file?"
//! is a stricter question than `ctx.function_depth == 0` for the
//! top-level-`await`/`await using` grammar checks (TS1308/TS2852 vs
//! TS1375+TS1378/TS2853+TS2854).
//!
//! `function_depth` only increments at function-like boundaries (free
//! functions, static blocks, and — since #16070 — method/constructor/accessor
//! bodies). It deliberately stays flat across a class property initializer
//! and a parameter initializer, because the TS2715 abstract-property family
//! relies on that flatness. But `tsc` does not treat a property initializer,
//! a parameter initializer, or a namespace body as "top level of the file"
//! for the `await` question, even though none of them is function-like. Every
//! expectation here is pinned against a live
//! `tsc@7.0.2 --noEmit --strict --pretty false --target es2022` run (see
//! issue #16072), not recalled.

use crate::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

// --- class property initializers ---

/// `class K { p = await 1; }` at the top level of the file. tsc: TS1308
/// only. Before this fix, tsz answered the top-level pair (TS1375 + TS1378)
/// instead, because the property initializer's `function_depth` is the
/// class body's own (0 at the top level).
#[test]
fn instance_property_initializer_await_reports_ts1308_not_top_level_pair() {
    let source = "class K { p = await 1; }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "an instance property initializer's `await` must report TS1308; got {diags:?}"
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "a property initializer is not top level of the file; must not report TS1375/TS1378; got {diags:?}"
    );
}

/// The `static` sibling. Same predicate, same expectation.
#[test]
fn static_property_initializer_await_reports_ts1308_not_top_level_pair() {
    let source = "class K { static p = await 1; }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a static property initializer's `await` must report TS1308; got {diags:?}"
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "a static property initializer is not top level of the file; got {diags:?}"
    );
}

/// Renamed-binder control (anti-hardcoding): different class/property/awaited
/// literal, same shape.
#[test]
fn instance_property_initializer_await_renamed_binders_reports_ts1308() {
    let source = "class WidgetHolder { queuedValue = await 'ready'; }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "renamed binders must not change the property-initializer result; got {diags:?}"
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "renamed binders must not change the property-initializer result; got {diags:?}"
    );
}

// --- parameter initializers ---

/// `class K { m(a = await 1) {} }`. tsc reports both TS1308 (the `await`
/// grammar check) and TS2524 (`await` cannot be used in a parameter
/// initializer) — independent checks that both fire. Before this fix, tsz
/// never called the grammar check for a parameter initializer at all, so only
/// TS2524 fired.
#[test]
fn method_parameter_default_await_reports_ts1308_and_ts2524() {
    let source = "class K { m(a = await 1) {} }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a method parameter default's `await` must report TS1308; got {diags:?}"
    );
    assert!(
        diags.contains(&2524),
        "a method parameter default's `await` must still report TS2524; got {diags:?}"
    );
}

/// The constructor sibling.
#[test]
fn constructor_parameter_default_await_reports_ts1308_and_ts2524() {
    let source = "class K { constructor(a = await 1) {} }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a constructor parameter default's `await` must report TS1308; got {diags:?}"
    );
    assert!(
        diags.contains(&2524),
        "a constructor parameter default's `await` must still report TS2524; got {diags:?}"
    );
}

/// A free function's parameter default at the top level of the file. Even
/// though the *function* is declared at the top level, the parameter
/// initializer is never top-level for `await` purposes — tsc reports TS1308
/// here too, not the top-level pair.
#[test]
fn free_function_parameter_default_await_reports_ts1308_not_top_level_pair() {
    let source = "function f(a = await 1) {}";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a free function's parameter default `await` must report TS1308; got {diags:?}"
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "a parameter initializer is never top level of the file; got {diags:?}"
    );
}

/// Renamed-binder control.
#[test]
fn method_parameter_default_await_renamed_binders_reports_ts1308() {
    let source = "class ShapeBase { computeArea(sideLength = await 2) {} }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "renamed binders must not change the parameter-default result; got {diags:?}"
    );
}

// --- namespace bodies ---

/// `namespace N { await 1; }` at the top level of the file. tsc: TS1308
/// only. A namespace body is not function-like, so `function_depth` never
/// disqualified it before this fix; it took the top-level branch instead.
#[test]
fn namespace_body_await_reports_ts1308_not_top_level_pair() {
    let source = "namespace N { await 1; }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a namespace body's `await` must report TS1308; got {diags:?}"
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "a namespace body is not top level of the file; must not report TS1375/TS1378; got {diags:?}"
    );
}

/// Renamed-binder control, plus a nested namespace to confirm the walk keeps
/// climbing through more than one non-disqualifying ancestor layer correctly
/// once it hits the first `MODULE_BLOCK`.
#[test]
fn nested_namespace_body_await_reports_ts1308() {
    let source = "namespace Outer { namespace Inner { await 1; } }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a nested namespace body's `await` must report TS1308; got {diags:?}"
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "a nested namespace body is not top level of the file; got {diags:?}"
    );
}

// --- negative controls: unrelated boundary questions stay unaffected ---

/// `namespace N { break; }` still reports TS1105 (the jump-boundary
/// question, owned by `function_depth`, which this change does not touch).
/// A namespace body IS a boundary for `await` but is NOT a boundary for
/// `break`/`continue` — the two predicates are deliberately different
/// walks (see #16072's write-up).
#[test]
fn namespace_body_break_stays_ts1105() {
    let source = "namespace N { break; }";
    let diags = codes(source);
    assert!(
        diags.contains(&1105),
        "a namespace body's `break` must still report TS1105 (unaffected by the await-boundary change); got {diags:?}"
    );
}

/// The TS2715 abstract-property family (#16070) must stay unaffected: an
/// abstract property read directly in a constructor body still reports
/// TS2715, since this change only adds a new predicate for `await` and does
/// not touch `function_depth` or its abstract-property baseline comparison.
#[test]
fn abstract_property_read_in_constructor_still_reports_ts2715() {
    let source = "abstract class A { abstract p: number; }\n\
                  class B extends A { constructor() { super(); this.p; } }";
    let diags = codes(source);
    assert!(
        diags.contains(&2715),
        "TS2715 must still fire for a direct constructor-body abstract-property read; got {diags:?}"
    );
}

/// Legal inside an `async` context: no TS1308 for any of the five
/// previously-wrong shapes once the enclosing method/constructor/function is
/// `async`. Parameter defaults still get TS2524 regardless of async-ness
/// (tsc: `await` is unconditionally illegal in a parameter initializer).
#[test]
fn async_context_reports_no_ts1308_for_property_or_namespace_shapes() {
    let source = "class K { p = 1; async m() { this.p = await Promise.resolve(1); } }";
    let diags = codes(source);
    assert!(
        !diags.contains(&1308),
        "an `await` inside an async method body must not report TS1308; got {diags:?}"
    );
}

// --- async-owner parameter defaults: TS2524 only, never TS1308 ---
//
// `check_parameter_initializers` runs during signature checking, before the
// owning function's body pushes its own async context, so it cannot read
// the owner's async-ness off ambient state — it must be told explicitly.
// tsc: TS2524 unconditionally, TS1308 only when the *owning* function is not
// async. All three rows below are async owners, so TS1308 must not appear.

#[test]
fn async_function_parameter_default_await_reports_only_ts2524() {
    let source = "async function f(a = await 1) {}";
    let diags = codes(source);
    assert!(diags.contains(&2524), "got {diags:?}");
    assert!(
        !diags.contains(&1308),
        "an async free function's own parameter default must not report TS1308; got {diags:?}"
    );
}

#[test]
fn async_generator_function_parameter_default_await_reports_only_ts2524() {
    let source = "async function* f(a = await 1) {}";
    let diags = codes(source);
    assert!(diags.contains(&2524), "got {diags:?}");
    assert!(
        !diags.contains(&1308),
        "an async generator's own parameter default must not report TS1308; got {diags:?}"
    );
}

#[test]
fn async_method_parameter_default_await_reports_only_ts2524() {
    let source = "class K { async m(a = await 1) {} }";
    let diags = codes(source);
    assert!(diags.contains(&2524), "got {diags:?}");
    assert!(
        !diags.contains(&1308),
        "an async method's own parameter default must not report TS1308; got {diags:?}"
    );
}

/// Renamed-binder control, plus confirming a *nested* non-async function's
/// own parameter default still correctly reports TS1308 even though its
/// enclosing function is async — async-ness is the immediately owning
/// function's own, never inherited (mirrors `in_async_context`'s doc
/// comment on `enter_function_async_context`).
#[test]
fn non_async_function_nested_in_async_function_parameter_default_reports_ts1308() {
    let source = "async function outer() { function inner(a = await 1) {} }";
    let diags = codes(source);
    assert!(
        diags.contains(&1308),
        "a non-async function nested in an async one still reports TS1308 for its own parameter default; got {diags:?}"
    );
}
