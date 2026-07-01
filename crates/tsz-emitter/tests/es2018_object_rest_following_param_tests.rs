//! Regression tests for the ES2018 "preceding object rest/spread" parameter
//! transform at module targets es2015–es2017 (binding patterns are native, but
//! object rest/spread is not).
//!
//! When a function parameter binding contains an object rest (`{ a, ...r }`)
//! below ES2018, tsc lowers that parameter to a generated temp AND every
//! parameter that follows it: a following binding pattern is rewritten to a temp
//! whose destructuring is fully flattened into the body
//! (`collectParametersWithPrecedingObjectRestOrSpread` +
//! `flattenDestructuringBinding`), so evaluation order matches the hoisted rest.
//! tsz previously lowered only the object-rest parameter itself and left the
//! following binding pattern in the parameter list — a byte divergence and a
//! real evaluation-order bug.
//!
//! Note the asymmetry (matches tsc 6.0.2): the *leading* object-rest parameter
//! keeps a native binding for its non-rest elements (`{ a } = _a`), while
//! *following* parameters are fully flattened (`rb = _b.b`).
//!
//! Source: `crates/tsz-emitter/src/emitter/functions.rs`
//! (`emit_function_parameters_js`) and the `ParamPrologueEntry` machinery in
//! `crates/tsz-emitter/src/emitter/statements/core.rs`.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as emit;

fn es2017_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2017,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

fn es2018_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2018,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

/// A following array binding pattern (with an elision hole) is flattened by
/// index from its temp — the array pattern must not survive in the parameter
/// list.
#[test]
fn following_array_pattern_is_flattened_from_its_temp() {
    let out = emit(
        "function widget({ head, ...tail }, [first, , third]) { return third; }\n",
        es2017_opts(),
    );

    assert!(
        out.contains("function widget(_a, _b)"),
        "the following array param must become a generated temp `_b`.\n{out}"
    );
    assert!(
        out.contains("var { head } = _a, tail = __rest(_a, [\"head\"]);"),
        "the leading object-rest param keeps a native binding + __rest.\n{out}"
    );
    assert!(
        out.contains("var first = _b[0], third = _b[2];"),
        "the following array pattern is flattened by index (hole skipped).\n{out}"
    );
    assert!(
        !out.contains("[first, , third]"),
        "the array binding pattern must not survive in the parameter list.\n{out}"
    );
}

/// A following object binding pattern is flattened by member access (not left as
/// a native `{ b, c }` binding).
#[test]
fn following_object_pattern_is_flattened_by_member() {
    let out = emit(
        "function box({ lead, ...others }, { alpha, beta }) { return alpha; }\n",
        es2017_opts(),
    );

    assert!(
        out.contains("function box(_a, _b)"),
        "the following object param must become a temp `_b`.\n{out}"
    );
    assert!(
        out.contains("var alpha = _b.alpha, beta = _b.beta;"),
        "the following object pattern is flattened by member access.\n{out}"
    );
    assert!(
        !out.contains("{ alpha, beta }"),
        "the object binding pattern must not survive in the parameter list.\n{out}"
    );
}

/// A following parameter that has its own object rest is also fully flattened
/// (`rb = _b.b, r2 = __rest(_b, [...])`), unlike the leading one which keeps a
/// native binding.
#[test]
fn following_object_rest_pattern_is_flattened() {
    let out = emit(
        "function pipe({ start, ...more }, { mid: renamed, ...leftover }) { return renamed; }\n",
        es2017_opts(),
    );

    assert!(
        out.contains("function pipe(_a, _b)"),
        "both params become temps.\n{out}"
    );
    assert!(
        out.contains("var { start } = _a, more = __rest(_a, [\"start\"]);"),
        "the leading object-rest param keeps its native binding.\n{out}"
    );
    assert!(
        out.contains("var renamed = _b.mid, leftover = __rest(_b, [\"mid\"]);"),
        "the following object-rest param is flattened (member access, not a native binding).\n{out}"
    );
}

/// A following nested object pattern introduces an inline temp for the nested
/// level (`_c = _b.b, x = _c.x`).
#[test]
fn following_nested_object_pattern_uses_inline_temp() {
    let out = emit(
        "function nest({ top, ...rest }, { inner: { left, right } }) { return left; }\n",
        es2017_opts(),
    );

    assert!(
        out.contains("var _c = _b.inner, left = _c.left, right = _c.right;"),
        "a following nested object pattern flattens through an inline temp `_c`.\n{out}"
    );
}

/// A following binding pattern with a default introduces a `=== void 0` ternary
/// source temp, then flattens from it.
#[test]
fn following_binding_default_uses_void0_ternary_temp() {
    let out = emit(
        "function withdef({ key, ...spread }, [lo, hi] = [1, 2]) { return lo; }\n",
        es2017_opts(),
    );

    assert!(
        out.contains("var _c = _b === void 0 ? [1, 2] : _b, lo = _c[0], hi = _c[1];"),
        "a following binding default resolves `param === void 0 ? init : param` \
         into a temp before flattening.\n{out}"
    );
}

/// A binding parameter that PRECEDES the first object-rest parameter is
/// preserved natively — only following parameters are rewritten.
#[test]
fn parameter_before_object_rest_is_preserved() {
    let out = emit(
        "function order([first, second], { tag, ...bag }) { return tag; }\n",
        es2017_opts(),
    );

    assert!(
        out.contains("function order([first, second], _a)"),
        "a binding param before the object-rest param stays native in the \
         parameter list.\n{out}"
    );
    assert!(
        out.contains("var { tag } = _a, bag = __rest(_a, [\"tag\"]);"),
        "only the object-rest param (and anything after it) is lowered.\n{out}"
    );
}

/// The body prologue statements are emitted in parameter order, interleaving a
/// following plain-identifier default (`if (x === void 0)`) with a following
/// binding pattern (`var c = _b[0]`).
#[test]
fn prologue_statements_follow_parameter_order() {
    let out = emit(
        "function seq({ a, ...r }, flag = 5, [c, d]) { return flag; }\n",
        es2017_opts(),
    );

    let default_at = out.find("if (flag === void 0)");
    let binding_at = out.find("var c = _b[0]");
    assert!(
        default_at.is_some() && binding_at.is_some(),
        "both the plain-id default and the flattened following binding must be \
         present.\n{out}"
    );
    assert!(
        default_at < binding_at,
        "prologue entries must be emitted in parameter order (default for param \
         2 before the flattened binding for param 3).\n{out}"
    );
}

/// At ES2018 the syntax is native — object rest in parameters is preserved and
/// no following parameter is rewritten.
#[test]
fn es2018_preserves_native_object_rest_parameters() {
    let out = emit(
        "function keep({ a, ...r }, [c, d]) { return c; }\n",
        es2018_opts(),
    );

    assert!(
        out.contains("{ a, ...r }") && out.contains("[c, d]"),
        "an ES2018 target preserves both native binding patterns.\n{out}"
    );
    assert!(
        !out.contains("__rest"),
        "no `__rest` downleveling for a native ES2018 target.\n{out}"
    );
}
