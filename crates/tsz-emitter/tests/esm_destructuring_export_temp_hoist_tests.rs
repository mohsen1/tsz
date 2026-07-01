//! ES module (`--module es2015|esnext`) ES5-downlevel parity with tsc for an
//! exported destructuring declaration whose source needs a temporary.
//!
//! `tsc` hoists the source temp as a plain, non-exported `var _a;` and folds its
//! assignment into the first binding via a comma expression, so the temp never
//! becomes a named export:
//!
//! ```js
//! var _a;
//! export var first = (_a = [1, 2, 3], _a[0]), rest = _a.slice(1);
//! ```
//!
//! tsz previously emitted `export var _a = [1, 2, 3], first = _a[0], ...`,
//! leaking `_a` as a spurious named export of the module. These cases pin the
//! hoisted-comma form and the boundaries where the existing (already-correct)
//! path must stay in control.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn esm_es5_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ES2015,
        ..Default::default()
    }
}

fn esnext_es5_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

fn es2015_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        ..Default::default()
    }
}

/// The temp must never appear in the exported declaration list.
fn assert_no_leaked_temp_export(output: &str) {
    assert!(
        !output.contains("export var _a"),
        "the synthesized source temp must not be emitted as a named export.\nOutput:\n{output}"
    );
}

#[test]
fn array_rest_export_hoists_temp_and_folds_assignment() {
    let source = "export const [first, ...rest] = [1, 2, 3];\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("var _a;"),
        "the source temp must be hoisted as a non-exported `var _a;`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export var first = (_a = [1, 2, 3], _a[0]), rest = _a.slice(1);"),
        "array-with-rest export must use tsc's hoisted-comma form.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

#[test]
fn array_no_rest_export_hoists_temp() {
    let source = "export const [a, b] = [4, 5];\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var a = (_a = [4, 5], _a[0]), b = _a[1];"),
        "array export must fold the source assignment into the first binding.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

#[test]
fn array_hole_preserves_element_index() {
    let source = "export const [, second, third] = [1, 2, 3];\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var second = (_a = [1, 2, 3], _a[1]), third = _a[2];"),
        "array holes must keep the element index for later bindings.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

#[test]
fn object_export_hoists_temp() {
    let source =
        "declare function getObj(): { x: number; y: number };\nexport const { x, y } = getObj();\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var x = (_a = getObj(), _a.x), y = _a.y;"),
        "object export must fold the source assignment into the first binding.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

#[test]
fn object_renamed_keys_export_hoists_temp() {
    let source = "declare function getObj(): { x: number; y: number };\nexport const { x: rx, y: ry } = getObj();\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var rx = (_a = getObj(), _a.x), ry = _a.y;"),
        "renamed object keys must read from the original property.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

/// Anti-hardcoding: entirely different binder names in the same structural
/// position must behave identically.
#[test]
fn renamed_binders_hoist_temp() {
    let source = "export const [alpha, ...omega] = [7, 8, 9];\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var alpha = (_a = [7, 8, 9], _a[0]), omega = _a.slice(1);"),
        "the fix must be driven by structure, not specific identifiers.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

/// `esnext` module output shares the ESM path and must hoist identically.
#[test]
fn esnext_module_hoists_temp() {
    let source = "export const [first, ...rest] = [1, 2, 3];\n";
    let output = parse_lower_emit(source, esnext_es5_opts());

    assert!(
        output.contains("export var first = (_a = [1, 2, 3], _a[0]), rest = _a.slice(1);"),
        "esnext module must use the same hoisted-comma form as es2015.\nOutput:\n{output}"
    );
    assert_no_leaked_temp_export(&output);
}

// --- Boundaries: the existing (already-correct) path must stay in control. ---

/// A single-element pattern reads the source once, so `tsc` inlines it with no
/// temp; the existing path already matches and must be left unchanged.
#[test]
fn single_element_pattern_stays_inlined_without_temp() {
    let source = "declare function f(): number[];\nexport const [only] = f();\nexport const [...all] = f();\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var only = f()[0];"),
        "a single non-rest element must inline the source (no temp).\nOutput:\n{output}"
    );
    assert!(
        output.contains("export var all = f().slice(0);"),
        "a single rest element must inline the source (no temp).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a"),
        "no temp should be minted for single-element patterns.\nOutput:\n{output}"
    );
}

/// A reusable identifier source is repeated inline at every access — no temp.
#[test]
fn reusable_identifier_source_stays_inlined() {
    let source = "declare const arr: number[];\nexport const [a, b] = arr;\n";
    let output = parse_lower_emit(source, esm_es5_opts());

    assert!(
        output.contains("export var a = arr[0], b = arr[1];"),
        "an identifier source must be reused inline with no temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a"),
        "no temp should be minted for a reusable identifier source.\nOutput:\n{output}"
    );
}

/// At ES2015+ destructuring is emitted natively — no lowering, no temp, no leak.
#[test]
fn native_destructuring_at_es2015_is_unchanged() {
    let source = "export const [first, ...rest] = [1, 2, 3];\n";
    let output = parse_lower_emit(source, es2015_opts());

    assert!(
        output.contains("export const [first, ...rest] = [1, 2, 3];"),
        "ES2015 output must keep native destructuring.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a"),
        "no temp should be minted at ES2015.\nOutput:\n{output}"
    );
}
