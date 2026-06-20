//! An interface inheriting members from a base that is not a plain same-file
//! interface.
//!
//! (A) `interface D extends Base {}` where `Base` is a `class` imported from
//!     another module. tsc materializes the base class's *instance* members
//!     into the interface; tsz dropped them because the local heritage merge
//!     only resolved class declarations in the current file's arena and then
//!     fell back to the class symbol's *constructor* type. The fix resolves a
//!     class base — including one reached through an import alias / default
//!     import — through the class-instance resolver, symmetric with the
//!     class-extends-class direction. This is the runtypes/arktype canary
//!     defect (#14161): both reduce to this one invariant.
//!
//! (B) `interface I<T extends {...}> extends T {}` — extending one of its own
//!     type parameters. tsc's `isValidBaseType` resolves the parameter to its
//!     base constraint, so an object-constrained parameter is a valid base and
//!     no TS2312 is reported. tsz emitted a spurious TS2312; the fix gates the
//!     diagnostic on the solver's `is_valid_interface_base_type`, which now
//!     resolves type-parameter constraints.
//!
//! Needs the real multi-module driver: the in-crate `check_multi_file_with_libs`
//! harness does not set up the cross-arena class-instance resolution path, so
//! these reproduce only end-to-end (mirrors `interface_extends_generic_alias_cli_tests`).
//! Cases vary binder names and import forms so the rule follows the type shape,
//! not identifier text.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile `files` (written into one temp dir) with the given root-file order.
fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022,dom,dom.iterable",
    ];
    argv.extend_from_slice(root_order);

    let cli_args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&cli_args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn codes_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Assert no TS2339 "property does not exist" in either root-file order
/// (consumer-first is the cross-file regression direction).
fn assert_no_missing_property_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = codes_with_code(&compile_in_order(files, &names), 2339);
    assert!(
        forward.is_empty(),
        "expected no TS2339 in forward root order {names:?}, got: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = codes_with_code(&compile_in_order(files, &reversed), 2339);
    assert!(
        backward.is_empty(),
        "expected no TS2339 in reversed root order {reversed:?}, got: {backward:?}"
    );
}

// ── (A) interface extends cross-module class ──────────────────────────────

#[test]
fn imported_class_base_inherits_instance_members() {
    assert_no_missing_property_both_orders(&[
        (
            "main.ts",
            r#"
import { Base } from "./base";
interface Derived extends Base {}
declare const d: Derived;
const s: string = d.greet();
const t: string = d.tag;
"#,
        ),
        (
            "base.ts",
            r#"
export class Base {
    tag!: string;
    greet(): string { return ""; }
}
"#,
        ),
    ]);
}

#[test]
fn renamed_default_import_class_base_inherits_members() {
    // Default export + locally renamed import: the rule is structural, not
    // keyed on the imported identifier.
    assert_no_missing_property_both_orders(&[
        (
            "main.ts",
            r#"
import Renamed from "./widget";
interface Panel extends Renamed {}
declare const p: Panel;
const n: number = p.id;
p.render();
"#,
        ),
        (
            "widget.ts",
            r#"
export default class Widget {
    id!: number;
    render(): void {}
}
"#,
        ),
    ]);
}

#[test]
fn imported_generic_class_base_inherits_instantiated_members() {
    assert_no_missing_property_both_orders(&[
        (
            "main.ts",
            r#"
import { Container } from "./container";
interface StrBox extends Container<string> {}
declare const b: StrBox;
const v: string = b.value;
const w: string = b.wrap("x");
"#,
        ),
        (
            "container.ts",
            r#"
export class Container<T> {
    value!: T;
    wrap(v: T): T { return v; }
}
"#,
        ),
    ]);
}

#[test]
fn imported_generic_class_base_member_is_instantiated_not_widened() {
    // The inherited member is instantiated to `string`, so a `number`
    // annotation must still be rejected (no widening to `any`).
    let files = &[
        (
            "main.ts",
            r#"
import { Container } from "./container";
interface StrBox extends Container<string> {}
declare const b: StrBox;
export const bad: number = b.value;
"#,
        ),
        (
            "container.ts",
            r#"
export class Container<T> {
    value!: T;
}
"#,
        ),
    ];
    let diags = compile_in_order(files, &["main.ts", "container.ts"]);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "the inherited member must be instantiated to `string` (TS2322 on a \
         `number` annotation), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn imported_class_base_genuinely_missing_member_still_errors() {
    // Negative control: inherited members resolve, but a truly absent member
    // still reports TS2339 (the fix must not blanket-suppress).
    let files = &[
        (
            "main.ts",
            r#"
import { Base } from "./base";
interface Derived extends Base {}
declare const d: Derived;
export const x = d.nope;
"#,
        ),
        (
            "base.ts",
            r#"
export class Base { tag!: string; }
"#,
        ),
    ];
    let missing = codes_with_code(&compile_in_order(files, &["main.ts", "base.ts"]), 2339);
    assert!(
        missing.iter().any(|m| m.contains("nope")),
        "a member on neither the interface nor its base class must still report \
         TS2339, got: {missing:?}"
    );
}

#[test]
fn imported_interface_base_still_clean() {
    // Boundary that already worked — guard against regression.
    assert_no_missing_property_both_orders(&[
        (
            "main.ts",
            r#"
import { BaseI } from "./base";
interface DerivedI extends BaseI {}
declare const d: DerivedI;
const s: string = d.greet();
const t: string = d.tag;
"#,
        ),
        (
            "base.ts",
            r#"
export interface BaseI { tag: string; greet(): string; }
"#,
        ),
    ]);
}

// ── (B) interface extends a constrained type parameter: diagnostic parity ─

#[test]
fn object_constrained_type_param_base_no_ts2312() {
    let files = &[(
        "a.ts",
        r#"
interface Box<attachments extends { kind: string }> extends attachments {}
declare const b: Box<{ kind: string }>;
"#,
    )];
    let ts2312 = codes_with_code(&compile_in_order(files, &["a.ts"]), 2312);
    assert!(
        ts2312.is_empty(),
        "interface extending a type parameter whose constraint is an object \
         base must not report TS2312, got: {ts2312:?}"
    );
}

#[test]
fn unconstrained_type_param_base_still_reports_ts2312() {
    let files = &[(
        "a.ts",
        r#"
interface Bag<contents> extends contents {}
"#,
    )];
    let ts2312 = codes_with_code(&compile_in_order(files, &["a.ts"]), 2312);
    assert!(
        !ts2312.is_empty(),
        "interface extending an unconstrained type parameter must still report \
         TS2312"
    );
}

#[test]
fn primitive_constrained_type_param_base_still_reports_ts2312() {
    let files = &[(
        "a.ts",
        r#"
interface Wrap<v extends number> extends v {}
"#,
    )];
    let ts2312 = codes_with_code(&compile_in_order(files, &["a.ts"]), 2312);
    assert!(
        !ts2312.is_empty(),
        "interface extending a type parameter constrained to a primitive must \
         still report TS2312"
    );
}
