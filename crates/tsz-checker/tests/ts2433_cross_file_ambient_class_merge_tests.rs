//! Regression tests for TS2433 ("A namespace declaration cannot be in a
//! different file from a class or function with which it is merged").
//!
//! Structural rule: when a namespace merges with a class/function declared in
//! a *different* file, tsc does not report TS2433 if every same-named
//! class/function declaration is ambient (`declare class` / `declare
//! function`) — exactly the same carve-out tsc already grants when the merge
//! is within one file. tsz's cross-file candidate search checked only the
//! `CLASS`/`FUNCTION` symbol flags and unconditionally reported TS2433,
//! without checking whether the found declaration was ambient, because it
//! never consulted the *other* file's arena to find out.
//!
//! Oracle: `typescript@7.0.2`, matches `TypeScript/tests/cases/compiler/ambientClassDeclarationWithExtends.ts`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;

// `check_multi_file` (without the global index) gives every file's binder its
// own raw `SymbolId`s starting at 0, so two files whose sole top-level
// declaration happens to be their first binding collide on `SymbolId(0)` and
// get misread as the *same* symbol. `check_multi_file_with_global_index` gives
// each file a disjoint id range, matching the production driver's bind-result
// reducer — the invariant `check_namespace_merges_with_class_or_function`'s
// cross-file candidate search relies on.
fn codes(files: &[(&str, &str)], entry_file: &str) -> Vec<u32> {
    check_multi_file_with_global_index(files, entry_file, CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn cross_file_namespace_merged_with_ambient_class_reports_no_ts2433() {
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
declare class E {
    public bar: number;
}
namespace F { var y: number; }
"#,
            ),
            (
                "use.ts",
                r#"
declare class F extends E { }
var f: E = new F();
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        !diags.contains(&2433),
        "declare class merged cross-file with a namespace must not report TS2433, got: {diags:?}"
    );
}

#[test]
fn cross_file_namespace_merged_with_non_ambient_class_still_reports_ts2433() {
    // Negative control: the class in the other file is NOT ambient, so tsc
    // still reports TS2433. Proves the fix does not blanket-suppress the
    // cross-file check.
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
class E {
    bar: number = 0;
}
namespace F { var y: number; }
"#,
            ),
            (
                "use.ts",
                r#"
class F extends E { }
var f: E = new F();
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        diags.contains(&2433),
        "non-ambient class merged cross-file with a namespace must still report TS2433, got: {diags:?}"
    );
}

#[test]
fn cross_file_namespace_merged_with_ambient_function_reports_no_ts2433() {
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
namespace Zeta { var y: number; }
"#,
            ),
            (
                "use.ts",
                r#"
declare function Zeta(): void;
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        !diags.contains(&2433),
        "declare function merged cross-file with a namespace must not report TS2433, got: {diags:?}"
    );
}

#[test]
fn cross_file_namespace_merged_with_non_ambient_function_still_reports_ts2433() {
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
namespace Zeta { var y: number; }
"#,
            ),
            (
                "use.ts",
                r#"
function Zeta(): void {}
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        diags.contains(&2433),
        "non-ambient function merged cross-file with a namespace must still report TS2433, got: {diags:?}"
    );
}

#[test]
fn cross_file_nested_namespace_merged_with_ambient_class_reports_no_ts2433() {
    // The namespace is nested inside an outer namespace, so the cross-file
    // partner must be resolved by walking the enclosing namespace's exports in
    // the other file — not just its top-level `file_locals`.
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
namespace Outer { export namespace Inner { var y: number; } }
"#,
            ),
            (
                "use.ts",
                r#"
namespace Outer { export declare class Inner { } }
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        !diags.contains(&2433),
        "nested namespace merged cross-file with an ambient class must not report TS2433, got: {diags:?}"
    );
}

#[test]
fn cross_file_nested_namespace_merged_with_non_ambient_class_still_reports_ts2433() {
    // Negative control for the nested case: a non-ambient class in the other
    // file's enclosing namespace must still report TS2433.
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
namespace Outer { export namespace Inner { var y: number; } }
"#,
            ),
            (
                "use.ts",
                r#"
namespace Outer { export class Inner { } }
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        diags.contains(&2433),
        "nested namespace merged cross-file with a non-ambient class must still report TS2433, got: {diags:?}"
    );
}

#[test]
fn cross_file_namespace_merged_with_ambient_class_renamed_binder_reports_no_ts2433() {
    // Same as the first test with every identifier renamed, to prove the fix
    // is not keyed on the literal name `F`/`E` from the conformance fixture.
    let diags = codes(
        &[
            (
                "decl.ts",
                r#"
declare class Widget {
    public label: string;
}
namespace Gadget { var count: number; }
"#,
            ),
            (
                "use.ts",
                r#"
declare class Gadget extends Widget { }
var g: Widget = new Gadget();
"#,
            ),
        ],
        "decl.ts",
    );
    assert!(
        !diags.contains(&2433),
        "declare class merged cross-file with a namespace must not report TS2433 under renamed binders, got: {diags:?}"
    );
}
