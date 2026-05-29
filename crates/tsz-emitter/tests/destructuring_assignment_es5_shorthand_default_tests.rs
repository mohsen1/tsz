//! ES5 lowering parity for destructuring-ASSIGNMENT targets.
//!
//! Two structural rules are covered:
//!
//! 1. A shorthand property carrying a cover-initialized default
//!    (`{ name = init }`) in a destructuring-assignment target lowers to the
//!    same extract-temp-and-`void 0` guard that the colon form
//!    (`{ key: name = init }`) emits:
//!    `tmp = source.key, name = tmp === void 0 ? init : tmp`.
//!    The fix keys on the structural presence of
//!    `ShorthandProperty.object_assignment_initializer`, never on the chosen
//!    identifier name, so renaming the binding must not change the lowering.
//!
//! 2. A for-of whose initializer is a bare expression assignment target
//!    (an array/object literal pattern, not a `VARIABLE_DECLARATION_LIST`) is a
//!    destructuring-assignment target. A spread element inside it
//!    (`for ([a, ...rest] of xs)`) must lower as destructuring rest
//!    (`rest = src.slice(1)`) and must NOT pull in the `__spreadArray` helper
//!    used for genuine array construction.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn es5_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

// -----------------------------------------------------------------------------
// Bug A: shorthand-default in a destructuring-assignment target
// -----------------------------------------------------------------------------

#[test]
fn assignment_shorthand_default_emits_void0_guard() {
    // `({ x = 5 } = src)` must extract `src.x` into a temp and guard the
    // default, exactly like the colon form would.
    let source = "let x: any; let src: any;\n({ x = 5 } = src);\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".x, x = ") && output.contains("=== void 0 ? 5 :"),
        "shorthand `{{ x = 5 }}` assignment should emit `_t = src.x, x = _t === void 0 ? 5 : _t`.\nOutput:\n{output}"
    );
    // Must NOT be the plain no-default lowering `x = src.x`.
    assert!(
        !output.contains("x = src.x;") && !output.contains("x = src.x)"),
        "shorthand default must not be dropped to a plain property read.\nOutput:\n{output}"
    );
}

#[test]
fn assignment_shorthand_default_is_name_independent() {
    // Renaming the bound variable must not change the structural lowering:
    // the void-0 guard is keyed on the AST shape, not the identifier `x`.
    let source = "let y: any; let src: any;\n({ y = 5 } = src);\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".y, y = ") && output.contains("=== void 0 ? 5 :"),
        "renamed shorthand `{{ y = 5 }}` must still emit the void-0 default guard.\nOutput:\n{output}"
    );
}

#[test]
fn assignment_colon_default_control_matches_shorthand_shape() {
    // Control: the colon form `{ x: x = 5 }` already lowered with the void-0
    // guard. The shorthand form must produce the same structural output, so the
    // two are intentionally compared for the same guard shape.
    let shorthand = parse_lower_emit(
        "let x: any; let src: any;\n({ x = 5 } = src);\n",
        es5_opts(),
    );
    let colon = parse_lower_emit(
        "let x: any; let src: any;\n({ x: x = 5 } = src);\n",
        es5_opts(),
    );

    let guard = "=== void 0 ? 5 :";
    assert!(
        shorthand.contains(guard) && colon.contains(guard),
        "both shorthand and colon defaults must emit the void-0 guard.\nShorthand:\n{shorthand}\nColon:\n{colon}"
    );
    assert!(
        shorthand.contains(".x, x = ") && colon.contains(".x, x = "),
        "both forms must extract `src.x` into a temp before the guarded assignment.\nShorthand:\n{shorthand}\nColon:\n{colon}"
    );
}

#[test]
fn assignment_shorthand_no_default_keeps_plain_read() {
    // Negative/fallback: a shorthand WITHOUT a default keeps the plain
    // property-read lowering (no spurious void-0 guard).
    let source = "let x: any; let src: any;\n({ x } = src);\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains("x = src.x"),
        "shorthand without a default should lower to `x = src.x`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("void 0"),
        "shorthand without a default must not emit a void-0 guard.\nOutput:\n{output}"
    );
}

// -----------------------------------------------------------------------------
// Bug B: for-of bare-expression assignment target with a spread
// -----------------------------------------------------------------------------

#[test]
fn for_of_bare_array_target_with_rest_lowers_as_slice_not_spread_array() {
    // `for ([a, ...rest] of xs)` is a destructuring-assignment target, so the
    // `[...rest]` is a rest pattern, not array construction. The rest must
    // lower to `.slice(...)` and the `__spreadArray` helper must NOT appear.
    let source =
        "let a: any, rest: any; let xs: any[] = [];\nfor ([a, ...rest] of xs) {\n    a;\n}\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".slice(1)"),
        "for-of array rest target should lower to `.slice(1)`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__spreadArray"),
        "for-of bare array assignment target must not pull in __spreadArray.\nOutput:\n{output}"
    );
}

#[test]
fn for_of_bare_array_rest_only_target_lowers_as_slice_not_spread_array() {
    // Rest-only bare target `for ([...rest] of xs)` lowers to `.slice(0)`.
    let source = "let rest: any; let xs: any[] = [];\nfor ([...rest] of xs) {\n    rest;\n}\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".slice(0)"),
        "rest-only for-of array target should lower to `.slice(0)`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__spreadArray"),
        "rest-only for-of bare target must not pull in __spreadArray.\nOutput:\n{output}"
    );
}

#[test]
fn for_of_bare_object_shorthand_default_target_emits_void0_guard() {
    // Combines both rules: a for-of bare object target with a shorthand default
    // (`for ({ name = "d" } of xs)`) must emit the void-0 default guard from
    // the property extracted out of each element.
    let source =
        "let name: any; let xs: any[] = [];\nfor ({ name = \"d\" } of xs) {\n    name;\n}\n";
    let output = parse_lower_emit(source, es5_opts());

    assert!(
        output.contains(".name, name = ") && output.contains("=== void 0 ? \"d\" :"),
        "for-of bare object shorthand default should emit the void-0 guard.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__spreadArray"),
        "object for-of target must not pull in __spreadArray.\nOutput:\n{output}"
    );
}
