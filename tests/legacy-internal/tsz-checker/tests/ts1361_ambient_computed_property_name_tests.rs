//! TS1361/TS1362 suppression for computed property names in ambient classes.
//!
//! Structural rule: a computed property name (`[expr]: T;`) inside any ambient
//! class never emits runtime code, so `tsc` never reports TS1361/TS1362 there
//! regardless of which of the four ambient spellings introduced the class:
//! `declare class`, a class inside `declare namespace`, a class inside
//! `declare module`, or any class in a `.d.ts` file. `tsz` implements this
//! through `is_in_ambient_computed_property_context`
//! (`crates/tsz-checker/src/symbols/scope_finder_contexts.rs`).
//!
//! Before the fix, the class-declaration arm tested
//! `has_declare_modifier(&class.modifiers)` — a syntactic test for the literal
//! `declare` keyword on the class node itself. That covers the explicit
//! `declare class` spelling, but a class inside `declare namespace`/`declare
//! module` carries no `declare` modifier of its own, so it fell through to
//! the `false` branch and produced a spurious TS1361 on any type-only-imported
//! identifier used as a computed property key. The fix delegates to
//! `is_ambient_class_declaration`, the same helper the caller of
//! `check_class_member_implementations` already uses to gate ambient classes
//! correctly across every spelling.
//!
//! A cross-file `import type { Sym } from './sym'` needs `check_multi_file`
//! (the single-file harness cannot resolve a real module specifier); see
//! `global_augmentation_computed_key_tests.rs` for the established pattern.
//!
//! Every expectation here was taken from the vendored `tsc` 7.0.2 oracle
//! (`--noEmit --strict --pretty false`), not from tsz's own output.

use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;

const TS1361: u32 = 1361;

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

const SYM_MODULE: &str = "export const Sym = \"sym\";\n";

fn codes(main: &str) -> Vec<u32> {
    check_multi_file(
        &[("main.ts", main), ("sym.ts", SYM_MODULE)],
        "main.ts",
        strict(),
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

fn assert_clean(main: &str, label: &str) {
    let got = codes(main);
    assert!(
        !got.contains(&TS1361),
        "{label}: expected no TS1361, got codes {got:?}"
    );
}

fn assert_reports_ts1361(main: &str, label: &str) {
    let got = codes(main);
    assert!(
        got.contains(&TS1361),
        "{label}: expected TS1361, got codes {got:?}"
    );
}

const IMPORT: &str = "import type { Sym } from './sym';\n";

// ---------------------------------------------------------------------------
// Positive cases: every ambient spelling must suppress TS1361.
// ---------------------------------------------------------------------------

#[test]
fn declare_class_computed_property_name_stays_clean() {
    let main = format!("{IMPORT}\ndeclare class C {{\n    [Sym]: string;\n}}\n");
    assert_clean(&main, "declare class");
}

#[test]
fn class_in_declare_namespace_computed_property_name_stays_clean() {
    let main = format!(
        "{IMPORT}\ndeclare namespace N {{\n    class C {{\n        [Sym]: string;\n    }}\n}}\n"
    );
    assert_clean(&main, "class inside declare namespace");
}

#[test]
fn class_in_declare_module_computed_property_name_stays_clean() {
    let main = format!(
        "{IMPORT}\ndeclare module './other' {{\n    class C {{\n        [Sym]: string;\n    }}\n}}\n"
    );
    let got = check_multi_file(
        &[
            ("main.ts", &main),
            ("sym.ts", SYM_MODULE),
            ("other.ts", "export const dummy = 1;\n"),
        ],
        "main.ts",
        strict(),
    )
    .iter()
    .map(|d| d.code)
    .collect::<Vec<u32>>();
    assert!(
        !got.contains(&TS1361),
        "class inside declare module: expected no TS1361, got codes {got:?}"
    );
}

#[test]
fn class_in_nested_declare_namespace_computed_property_name_stays_clean() {
    let main = format!(
        "{IMPORT}\ndeclare namespace Outer {{\n    namespace Inner {{\n        class C {{\n            [Sym]: string;\n        }}\n    }}\n}}\n"
    );
    assert_clean(&main, "class inside nested declare namespace");
}

// ---------------------------------------------------------------------------
// Boundary control: the already-correct forms must not move.
// ---------------------------------------------------------------------------

#[test]
fn interface_computed_property_name_stays_clean() {
    let main = format!("{IMPORT}\ninterface I {{\n    [Sym]: string;\n}}\n");
    assert_clean(&main, "interface member");
}

#[test]
fn declare_class_method_computed_property_name_stays_clean() {
    let main = format!("{IMPORT}\ndeclare class C {{\n    [Sym](): void;\n}}\n");
    assert_clean(&main, "declare class method");
}

// ---------------------------------------------------------------------------
// Negative controls: the non-ambient forms must keep reporting TS1361.
// ---------------------------------------------------------------------------

#[test]
fn plain_class_computed_property_name_still_reports_ts1361() {
    let main = format!("{IMPORT}\nclass C {{\n    [Sym]: string;\n}}\n");
    assert_reports_ts1361(&main, "plain class");
}

#[test]
fn class_expression_computed_property_name_still_reports_ts1361() {
    let main = format!("{IMPORT}\nconst C = class {{\n    [Sym]: string;\n}};\n");
    assert_reports_ts1361(&main, "class expression");
}
