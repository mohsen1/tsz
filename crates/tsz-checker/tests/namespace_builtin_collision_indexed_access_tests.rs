//! Regression tests: indexed access of a member of a user namespace whose name
//! equals a global builtin type.
//!
//! In a script, a top-level `namespace Iterator { export type Obj = ... }`
//! merges with the global lib `Iterator`. The qualified type `Iterator.Obj`
//! resolves, but the indexed access `Iterator.Obj["foo"]` used to stay
//! unevaluated (the leftmost `Iterator` resolved to the lib type symbol, whose
//! exports never contain `Obj`), producing a false `TS2322`. Resolving the
//! leftmost segment through lexical scope — preferring the local namespace and
//! falling back to the member's namespace parent linkage — lets `N.X[K]` reduce.
//!
//! The names are varied (`Iterator`, `Array`, `Promise`) so the fix exercises a
//! structural rule, not a single hardcoded builtin name.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    let libs: Vec<Arc<LibFile>> = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn indexed_access_of_builtin_named_namespace_member_reduces() {
    // tsc: clean. The indexed access must reduce to `number`, so the matching
    // literal is assignable and no TS2322 is emitted.
    let source = r#"
namespace Iterator { export type Obj = { foo: number }; }
const a: Iterator.Obj["foo"] = 5;
"#;
    let diags = check(source);
    assert!(
        !codes(&diags).contains(&2322),
        "expected no TS2322 for Iterator.Obj[\"foo\"] = 5, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn indexed_access_of_builtin_named_namespace_member_still_type_checks() {
    // The reduced type is `number`, so a string value must still be rejected.
    let source = r#"
namespace Iterator { export type Obj = { foo: number }; }
const b: Iterator.Obj["foo"] = "str";
"#;
    let diags = check(source);
    assert!(
        codes(&diags).contains(&2322),
        "expected TS2322 for string assigned to Iterator.Obj[\"foo\"], got: {:?}",
        codes(&diags)
    );
}

#[test]
fn array_named_namespace_member_indexed_access_reduces() {
    // Name varied to `Array` (also a global builtin) to keep the rule structural.
    let source = r#"
namespace Array { export type Box = { bar: string }; }
const c: Array.Box["bar"] = "hi";
const d: Array.Box["bar"] = 5;
"#;
    let diags = check(source);
    let only_2322: Vec<u32> = codes(&diags).into_iter().filter(|&c| c == 2322).collect();
    // The valid assignment must not error; the invalid one (number to string) must.
    assert_eq!(
        only_2322,
        vec![2322],
        "expected exactly one TS2322 (for `= 5`), got: {:?}",
        codes(&diags)
    );
}

#[test]
fn nested_builtin_named_namespace_member_indexed_access_reduces() {
    // Nested namespace under a builtin-named outer namespace.
    let source = r#"
namespace Promise { export namespace Inner { export type T = { n: number }; } }
const e: Promise.Inner.T["n"] = 3;
const f: Promise.Inner.T["n"] = "x";
"#;
    let diags = check(source);
    let only_2322: Vec<u32> = codes(&diags).into_iter().filter(|&c| c == 2322).collect();
    assert_eq!(
        only_2322,
        vec![2322],
        "expected exactly one TS2322 (for `= \"x\"`), got: {:?}",
        codes(&diags)
    );
}

#[test]
fn missing_member_of_builtin_named_namespace_still_reports_ts2694() {
    // Negative control: a genuinely missing member must still surface TS2694,
    // i.e. the parent-linkage fallback must not invent members.
    let source = r#"
namespace Iterator { export type Obj = { foo: number }; }
const g: Iterator.Missing = 1;
"#;
    let diags = check(source);
    assert!(
        codes(&diags).contains(&2694),
        "expected TS2694 for Iterator.Missing, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn member_does_not_leak_to_global_scope() {
    // The namespace member must not become resolvable as a bare global name.
    let source = r#"
namespace Iterator { export type Obj = { foo: number }; }
const h: Obj = { foo: 1 };
"#;
    let diags = check(source);
    assert!(
        codes(&diags).contains(&2304),
        "expected TS2304 for bare `Obj`, got: {:?}",
        codes(&diags)
    );
}
