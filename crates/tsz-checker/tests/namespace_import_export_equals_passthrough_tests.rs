//! Regression tests for issue #14810: a namespace import (`import * as ns`) of
//! a module that uses `export = <value>` must bind `ns` to the `export =` value
//! directly, NOT to a synthetic `{ default: <value> }` wrapper.
//!
//! Structural rule: when `import * as ns from "M"` resolves to a non-node module
//! whose only export is `export = X`, tsc types `ns` as `X` itself
//! (`ns.a` / `ns(...)` / `new ns()` work; `ns.default` does not exist). The
//! synthetic `default` produced under `allowSyntheticDefaultImports` belongs only
//! to the *default-import* path, never to the namespace shape. Previously tsz
//! returned the `{ default: X }` wrapper here, producing spurious TS2339 (object/
//! class targets) and TS2349 (function targets).
//!
//! These cases all run with `--module commonjs --esModuleInterop` (the exact
//! configuration that triggered the false positive) and vary the binder/alias
//! identifiers so the fix is proven structural, not keyed on any name.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            target: ScriptTarget::ES2022,
            module: ModuleKind::CommonJS,
            strict: true,
            es_module_interop: true,
            no_lib: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn namespace_import_of_export_equals_object_exposes_properties() {
    // `export = <object>`: `ns.a` must resolve (no TS2339).
    let dep = r#"
declare const obj: { a: number; b: string };
export = obj;
"#;
    let main = r#"
import * as ns from "./dep";
const a: number = ns.a;
const b: string = ns.b;
"#;
    let codes = codes(
        &[("/proj/dep.d.ts", dep), ("/proj/main.ts", main)],
        "/proj/main.ts",
    );
    assert!(
        !codes.contains(&2339),
        "namespace import of `export = <object>` must expose properties directly (no TS2339); got {codes:?}"
    );
}

#[test]
fn namespace_import_of_export_equals_object_is_the_value_not_a_default_wrapper() {
    // Reveal probe (issue #14810): `ns` must BE the `export =` object, not a
    // `{ default: <object> }` wrapper. Assigning `ns` to the bare object shape
    // must succeed; if tsz still wrapped it as `{ default: { value: number } }`
    // this assignment would raise TS2322. Using assignability (instead of a
    // `.default` property probe) keeps the check independent of lib-type
    // resolution under `no_lib`.
    let dep = r#"
declare const payload: { value: number };
export = payload;
"#;
    let main = r#"
import * as bag from "./dep";
const direct: { value: number } = bag;
"#;
    let codes = codes(
        &[("/proj/dep.d.ts", dep), ("/proj/main.ts", main)],
        "/proj/main.ts",
    );
    assert!(
        !codes.contains(&2322),
        "namespace import of `export = <object>` must be the object itself, not a `{{ default }}` wrapper (no TS2322 on direct assignment); got {codes:?}"
    );
}

#[test]
fn namespace_import_of_export_equals_function_is_callable() {
    // `export = <function>`: `ns(...)` must be callable (no TS2349). Distinct
    // alias and parameter names to keep the rule structural.
    let dep = r#"
declare function transform(input: number): string;
export = transform;
"#;
    let main = r#"
import * as helper from "./dep";
const out: string = helper(5);
"#;
    let codes = codes(
        &[("/proj/dep.d.ts", dep), ("/proj/main.ts", main)],
        "/proj/main.ts",
    );
    assert!(
        !codes.contains(&2349),
        "namespace import of `export = <function>` must be callable (no TS2349); got {codes:?}"
    );
}

#[test]
fn namespace_import_of_export_equals_class_is_constructable() {
    // `export = <class>`: `new ns()` must construct (no TS2351/TS2349), and the
    // instance member must be visible.
    let dep = r#"
declare class Widget {
    constructor(size: number);
    size: number;
}
export = Widget;
"#;
    let main = r#"
import * as Boxed from "./dep";
const w = new Boxed(3);
const s: number = w.size;
"#;
    let codes = codes(
        &[("/proj/dep.d.ts", dep), ("/proj/main.ts", main)],
        "/proj/main.ts",
    );
    assert!(
        !codes.contains(&2351) && !codes.contains(&2349),
        "namespace import of `export = <class>` must be constructable (no TS2351/TS2349); got {codes:?}"
    );
}
