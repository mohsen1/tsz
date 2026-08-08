//! TS2374 must not fabricate a "duplicate index signature" for a JSX
//! `IntrinsicElements` interface that declares a single index signature, even
//! when the `@types/*` package's `jsx-runtime` entrypoint re-imports the
//! package root (`import './';`).
//!
//! Structural rule: `tsc` reports TS2374 only when an interface actually
//! merges two or more same-kind index signatures across its declarations. A
//! JSX runtime entrypoint importing the package root does not duplicate the
//! `IntrinsicElements` interface — the corpus rows
//! `compiler/reactJsxReactResolvedNodeNext.tsx` and its ESM sibling both
//! produce zero diagnostics under `tsc@7.0.2` (no `.errors.txt` baseline), and
//! no TypeScript baseline anywhere reports "Duplicate index signature" on an
//! `IntrinsicElements` interface.
//!
//! These tests lock the parity floor: the single-signature JSX self-import
//! shape stays clean, while a genuinely duplicated index signature (two of the
//! same kind) still reports TS2374 — the detection must be driven by the real
//! merged-signature count, never by the JSX-runtime file shape.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file, check_source, diagnostic_count};
use tsz_common::checker_options::JsxMode;

fn react_jsx_options() -> CheckerOptions {
    CheckerOptions {
        jsx_mode: JsxMode::ReactJsx,
        ..CheckerOptions::default()
    }
}

/// The bug witness: a `@types/react`-shaped package whose `jsx-runtime` and
/// `jsx-dev-runtime` entrypoints re-import the package root. The single
/// `IntrinsicElements` index signature must NOT be reported as a duplicate.
#[test]
fn react_jsx_intrinsic_elements_single_string_index_with_self_import_runtime_is_clean() {
    let files = [
        (
            "/proj/node_modules/@types/react/index.d.ts",
            "declare namespace JSX {\n    interface IntrinsicElements { [x: string]: any; }\n}\n",
        ),
        (
            "/proj/node_modules/@types/react/jsx-runtime.d.ts",
            "import './';\n",
        ),
        (
            "/proj/node_modules/@types/react/jsx-dev-runtime.d.ts",
            "import './';\n",
        ),
    ];
    let diags = check_multi_file(
        &files,
        "/proj/node_modules/@types/react/index.d.ts",
        react_jsx_options(),
    );
    assert_eq!(
        diagnostic_count(&diags, 2374),
        0,
        "a single JSX.IntrinsicElements index signature must not be reported as a \
         duplicate just because the JSX runtime entrypoint re-imports the package \
         root; got: {diags:?}"
    );
}

/// Same self-import runtime shape, but the `IntrinsicElements` body genuinely
/// declares two same-kind index signatures. The real merged-count rule must
/// still fire TS2374 — removing the JSX-runtime heuristic must not disable
/// genuine duplicate detection inside a JSX namespace.
#[test]
fn react_jsx_intrinsic_elements_two_string_indexes_still_report_ts2374() {
    let files = [
        (
            "/proj/node_modules/@types/react/index.d.ts",
            "declare namespace JSX {\n    interface IntrinsicElements {\n        [x: string]: any;\n        [y: string]: any;\n    }\n}\n",
        ),
        (
            "/proj/node_modules/@types/react/jsx-runtime.d.ts",
            "import './';\n",
        ),
    ];
    let diags = check_multi_file(
        &files,
        "/proj/node_modules/@types/react/index.d.ts",
        react_jsx_options(),
    );
    assert!(
        diagnostic_count(&diags, 2374) >= 2,
        "two same-kind index signatures in IntrinsicElements must still report \
         TS2374 on each; got: {diags:?}"
    );
}

/// Structural, name-independent positive check: a plain interface with two
/// `string` index signatures reports TS2374 regardless of the binder names.
/// Guards that the fix left the ordinary duplicate-index path untouched.
#[test]
fn plain_interface_two_string_indexes_report_ts2374_name_independent() {
    for (iface, a, b) in [("Widget", "a", "b"), ("Zephyr", "keyA", "keyB")] {
        let source = format!(
            "interface {iface} {{\n    [{a}: string]: number;\n    [{b}: string]: number;\n}}\n"
        );
        let diags = check_source(&source, "dup_index.ts", CheckerOptions::default());
        assert!(
            diagnostic_count(&diags, 2374) >= 2,
            "interface {iface} with two string index signatures must report TS2374 \
             on each; got: {diags:?}"
        );
    }
}

/// A single index signature in a plain (non-JSX) interface is never a
/// duplicate — the ordinary baseline that the JSX heuristic wrongly departed
/// from.
#[test]
fn plain_interface_single_string_index_is_clean() {
    let diags = check_source(
        "interface Widget { [x: string]: any; }\n",
        "single_index.ts",
        CheckerOptions::default(),
    );
    assert_eq!(
        diagnostic_count(&diags, 2374),
        0,
        "a single string index signature must never report TS2374; got: {diags:?}"
    );
}
