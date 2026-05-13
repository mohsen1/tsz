//! Ordering parity with tsc for ES5 array destructuring that contains a
//! nested binding pattern.
//!
//! When an array pattern like `[{ ...a }, b = a]` is downlevelled to ES5, the
//! object-rest element is first captured in a temp, then decomposed after later
//! non-simple elements have captured their own temps. Without this, a later
//! element's default expression can observe a name that has not been bound yet
//! by an earlier element's object-rest decomposition. Source:
//! <https://github.com/microsoft/TypeScript/issues/39181>
//!
//! Covers the conformance fixture
//! `tests/cases/conformance/es6/destructuring/destructuringEvaluationOrder.ts`
//! at `target=es5`.

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

#[test]
fn array_destructuring_with_object_rest_emits_reads_before_decompositions() {
    let source = "let arr: any = [{ x: 1 }];\nlet [{ ...a }, b = a]: any[] = arr;\n";
    let output = parse_lower_emit(source, es5_opts());

    // The two array reads must appear before the rest-decomposition of `a`,
    // and `a = __rest(...)` must appear before `b = _* === void 0 ? a : _*`
    // (b's default references `a`, so `a` must be in scope first).
    let read_q = output
        .find("_a = arr[0]")
        .unwrap_or_else(|| panic!("expected `_a = arr[0]` read in output:\n{output}"));
    let read_r = output
        .find("_b = arr[1]")
        .unwrap_or_else(|| panic!("expected `_b = arr[1]` read in output:\n{output}"));
    let rest_decomp = output.find("a = __rest(_a").unwrap_or_else(|| {
        panic!("expected `a = __rest(_a, ...)` rest decomposition in output:\n{output}")
    });
    let b_assign = output.find("b = _b").unwrap_or_else(|| {
        panic!("expected `b = _b === void 0 ? a : _b` assignment in output:\n{output}")
    });

    assert!(
        read_q < read_r,
        "Read of element 0 must precede read of element 1.\nOutput:\n{output}"
    );
    assert!(
        read_r < rest_decomp,
        "All array reads must come before any decomposition.\nOutput:\n{output}"
    );
    assert!(
        rest_decomp < b_assign,
        "Element 0's decomposition (`a`) must precede element 1's defaulted assignment that uses `a`.\nOutput:\n{output}"
    );
}

#[test]
fn array_destructuring_plain_nested_patterns_keep_single_pass_order() {
    // Plain nested patterns do not contain object rest, so member-access
    // decomposition remains interleaved with array reads.
    let source = "let [{ x }, { y }]: any[] = [{ x: 1 }, { y: 2 }];\n";
    let output = parse_lower_emit(source, es5_opts());

    let read_0 = output
        .find("_a[0]")
        .unwrap_or_else(|| panic!("missing `_a[0]` read:\n{output}"));
    let dot_x = output
        .find(".x")
        .unwrap_or_else(|| panic!("missing `.x` decomposition:\n{output}"));
    let read_1 = output
        .find("_a[1]")
        .unwrap_or_else(|| panic!("missing `_a[1]` read:\n{output}"));
    let dot_y = output
        .find(".y")
        .unwrap_or_else(|| panic!("missing `.y` decomposition:\n{output}"));

    assert!(
        read_0 < dot_x,
        "Element 0's read must precede its property decomposition.\nOutput:\n{output}"
    );
    assert!(
        dot_x < read_1,
        "Plain nested element 0 should decompose before element 1 is read.\nOutput:\n{output}"
    );
    assert!(
        read_1 < dot_y,
        "Element 1's read must precede its property decomposition.\nOutput:\n{output}"
    );
}

#[test]
fn array_destructuring_default_before_object_rest_keeps_single_pass_order() {
    let source =
        "declare function f(): any;\nlet arr: any = [];\nlet [a = f(), { ...b }]: any[] = arr;\n";
    let output = parse_lower_emit(source, es5_opts());

    let first_read = output
        .find("_a = arr[0]")
        .unwrap_or_else(|| panic!("missing first element read:\n{output}"));
    let default_assign = output
        .find("a = _a === void 0 ? f() : _a")
        .unwrap_or_else(|| panic!("missing defaulted `a` assignment:\n{output}"));
    let rest_read = output
        .find("_b = arr[1]")
        .unwrap_or_else(|| panic!("missing object-rest element read:\n{output}"));
    let rest_decomp = output
        .find("b = __rest(_b")
        .unwrap_or_else(|| panic!("missing object-rest decomposition:\n{output}"));

    assert!(
        first_read < default_assign,
        "Element 0's read must precede its default assignment.\nOutput:\n{output}"
    );
    assert!(
        default_assign < rest_read,
        "Default initializer before object rest must not be deferred past later reads.\nOutput:\n{output}"
    );
    assert!(
        rest_read < rest_decomp,
        "Object-rest decomposition runs after its temp capture.\nOutput:\n{output}"
    );
}

#[test]
fn array_destructuring_without_nested_keeps_single_pass_layout() {
    // Sanity: shapes that have no nested pattern stay on the single-pass
    // path and keep the existing per-element interleaving (no behavioural
    // change for the common case).
    let source = "let arr: any = [1, 2];\nlet [a = 10, b = 20]: any[] = arr;\n";
    let output = parse_lower_emit(source, es5_opts());

    // Single-pass interleaves: read for `a`, then `a` decl, then read for
    // `b`, then `b` decl.  The substring `arr[1]` should appear after the
    // first defaulted assignment that uses `arr[0]`.
    let first_a_decl = output
        .find("a = ")
        .unwrap_or_else(|| panic!("missing `a = ...` assignment:\n{output}"));
    let read_1 = output
        .find("arr[1]")
        .unwrap_or_else(|| panic!("missing `arr[1]` read:\n{output}"));
    assert!(
        first_a_decl < read_1,
        "Without nested patterns, element 1's read should still come after element 0's decl.\nOutput:\n{output}"
    );
}
