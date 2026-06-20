//! ES5 down-level parity with tsc for an optional-chain **call** whose `this`
//! receiver is itself reached through an optional (`?.`) link.
//!
//! When the final access of a call's callee is non-optional (`obj.fn?.(...)`)
//! but its receiver `obj` is an optional chain (`a?.b`), the call's `this`
//! receiver must be captured **inside** the chain's nullish guard, mirroring
//! tsc's `flattenOptionalChain`:
//!
//! ```text
//! (_f = a === null || a === void 0 ? void 0 : (_t = a.b).fn)
//!     === null || _f === void 0 ? void 0 : _f.call(_t, args)
//! ```
//!
//! tsz previously wrapped the capture around the whole lowered receiver
//! (`(_t = a === null || a === void 0 ? void 0 : a.b).fn`), which dereferences
//! `void 0` — throwing `TypeError` — whenever the chain short-circuits. Found
//! by differential testing against tsc 6.0.2 (`--target es5`).
//!
//! Assertions compare against tsc's exact byte output and vary binder names so
//! the parity is proven structural, not keyed to any identifier spelling.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;
use test_support::parse_and_lower_print as lower_emit;

const DECLS: &str =
    "let obj:any,root:any,box:any,node:any,svc:any,p:any,m:any,n:any,x:any,req:any;";

fn es5_emit(expr: &str) -> String {
    let src = format!("{DECLS}\nlet out = {expr};\n");
    lower_emit(
        &src,
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    )
}

/// Assert the emitted `out = ...` statement is byte-identical to tsc 6.0.2.
fn assert_out(expr: &str, expected_stmt: &str) {
    let output = es5_emit(expr);
    let line = output
        .lines()
        .find(|l| l.trim_start().starts_with("var out ="))
        .unwrap_or_else(|| panic!("no `var out =` line in output:\n{output}"));
    assert_eq!(
        line.trim(),
        expected_stmt,
        "\nexpr: {expr}\nfull output:\n{output}"
    );
}

#[test]
fn property_receiver_through_optional_link() {
    // `this` (obj.a) captured inside the guard, not wrapping it.
    assert_out(
        "obj?.a.fn?.(1)",
        "var out = (_b = obj === null || obj === void 0 ? void 0 : (_a = obj.a).fn) === null || _b === void 0 ? void 0 : _b.call(_a, 1);",
    );
}

#[test]
fn property_receiver_is_name_agnostic() {
    // Same structural shape under different binder/property spellings.
    assert_out(
        "root?.left.run?.(x)",
        "var out = (_b = root === null || root === void 0 ? void 0 : (_a = root.left).run) === null || _b === void 0 ? void 0 : _b.call(_a, x);",
    );
}

#[test]
fn element_access_receiver_through_optional_link() {
    assert_out(
        "box?.[\"item\"].go?.(7)",
        "var out = (_b = box === null || box === void 0 ? void 0 : (_a = box[\"item\"]).go) === null || _b === void 0 ? void 0 : _b.call(_a, 7);",
    );
}

#[test]
fn double_optional_receiver_splices_at_inner_synchronous_tail() {
    // The receiver `node?.next?.value` itself has two optional links; the `this`
    // for `.emit` is the synchronous tail `_a.value`.
    assert_out(
        "node?.next?.value.emit?.(1)",
        "var out = (_c = (_a = node === null || node === void 0 ? void 0 : node.next) === null || _a === void 0 ? void 0 : (_b = _a.value).emit) === null || _c === void 0 ? void 0 : _c.call(_b, 1);",
    );
}

#[test]
fn call_in_the_middle_of_the_receiver_chain() {
    // `svc?.api()` is a call; its result is the `this` receiver of `.handler`.
    assert_out(
        "svc?.api().handler?.(req)",
        "var out = (_b = svc === null || svc === void 0 ? void 0 : (_a = svc.api()).handler) === null || _b === void 0 ? void 0 : _b.call(_a, req);",
    );
}

#[test]
fn nested_optional_call_allocates_inner_temps_first() {
    // `a?.b.c?.(1)?.(2)` — the inner call's temps must precede the outer one.
    assert_out(
        "p?.q.r?.(1)?.(2)",
        "var out = (_c = (_b = p === null || p === void 0 ? void 0 : (_a = p.q).r) === null || _b === void 0 ? void 0 : _b.call(_a, 1)) === null || _c === void 0 ? void 0 : _c(2);",
    );
}

#[test]
fn optional_call_after_non_optional_access_on_call_result() {
    // `obj?.a.b?.(1).c?.(2)` — `.c`'s receiver is the inner call result.
    assert_out(
        "obj?.a.b?.(1).c?.(2)",
        "var out = (_d = (_b = obj === null || obj === void 0 ? void 0 : (_a = obj.a).b) === null || _b === void 0 ? void 0 : (_c = _b.call(_a, 1)).c) === null || _d === void 0 ? void 0 : _d.call(_c, 2);",
    );
}

#[test]
fn long_synchronous_receiver_tail() {
    // Several non-optional links after the optional one collapse into one tail.
    assert_out(
        "obj?.a.b.c.run?.(x)",
        "var out = (_b = obj === null || obj === void 0 ? void 0 : (_a = obj.a.b.c).run) === null || _b === void 0 ? void 0 : _b.call(_a, x);",
    );
}

#[test]
fn parenthesized_receiver_terminates_the_chain() {
    // `(p?.q).r?.(1)` — parens end the optional chain, so the whole lowered
    // receiver is captured (tsc throws on short-circuit here, by design).
    assert_out(
        "(p?.q).r?.(1)",
        "var out = (_b = (_a = (p === null || p === void 0 ? void 0 : p.q)).r) === null || _b === void 0 ? void 0 : _b.call(_a, 1);",
    );
}

#[test]
fn non_optional_complex_receiver_is_unchanged() {
    // Control: a non-optional complex receiver keeps the legacy wrap form.
    assert_out(
        "(m + n).fn?.(1)",
        "var out = (_b = (_a = (m + n)).fn) === null || _b === void 0 ? void 0 : _b.call(_a, 1);",
    );
}

#[test]
fn capture_sits_inside_the_nullish_guard_not_around_it() {
    // Name-agnostic structural guard against the regression: the `this` capture
    // must open *after* a `? void 0 : ` (inside the else branch), never wrap the
    // ternary. In the buggy form the capture opens *before* the guard, so the
    // text immediately following `void 0 : ` is the bare receiver rather than a
    // fresh `(_temp = ` capture.
    for expr in [
        "obj?.a.fn?.(1)",
        "root?.left.run?.(x)",
        "box?.[\"item\"].go?.(7)",
        "obj?.a.b.c.run?.(x)",
    ] {
        let out = es5_emit(expr);
        assert!(
            out.contains("? void 0 : (_"),
            "expected `this` capture inside the guard for `{expr}`:\n{out}"
        );
        // The buggy form opens the capture immediately around the guard, e.g.
        // `(_a = obj === null || obj === void 0 ? void 0 : obj.a).fn`. The
        // synchronous tail must instead be a captured access ending in `.<name>)`
        // or `[...])`, so the over-wide `void 0 : <ident>).` shape never appears.
        assert!(
            !out.contains("void 0 : obj.a)"),
            "regressed: `this` capture wraps the whole guard for `{expr}`:\n{out}"
        );
    }
}
