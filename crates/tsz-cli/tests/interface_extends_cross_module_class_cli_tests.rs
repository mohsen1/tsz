//! Interface heritage that `extends` a **class imported from another module**
//! (#14161 A).
//!
//! Structural rule: when `interface I extends ImportedClass<Args>` and the
//! class declaration lives in another file's arena, tsc materializes the
//! class's *instance* members into `I`; tsz must route the cross-arena base
//! symbol through the symbol-based class-**instance** path before falling back
//! to `get_type_of_symbol`. That fallback returns the class *constructor*
//! (`Callable`) type, whose properties are the static side; the `Object` +
//! `Callable` merge then drops every instance member, so cross-file property
//! access on `I` reported every inherited member as missing (TS2339).
//!
//! The defect is cross-module only — same-file `interface extends class`
//! already worked because the class node is in the local arena. It needs the
//! real multi-module driver to reproduce (the in-crate checker harnesses
//! resolve the entry-only generic case regardless of the fix and cannot host
//! it). Cases vary generic vs non-generic bases, named vs default imports,
//! root-file order, and renamed binders, plus controls so the rule follows the
//! type shape rather than identifier names.

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

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn missing_property_messages(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Assert no TS2339 "property does not exist" in either root-file order
/// (consumer-first is the cross-file regression direction).
fn assert_no_missing_property_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = missing_property_messages(&compile_in_order(files, &names));
    assert!(
        forward.is_empty(),
        "expected no TS2339 in forward root order {names:?}, got: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = missing_property_messages(&compile_in_order(files, &reversed));
    assert!(
        backward.is_empty(),
        "expected no TS2339 in reversed root order {reversed:?}, got: {backward:?}"
    );
}

/// Core witness: interface extends a generic cross-module class (the issue's
/// repro A, `Base<bigint>`). All inherited instance members must resolve.
#[test]
fn interface_extends_cross_module_generic_class_inherits_instance_members() {
    assert_no_missing_property_both_orders(&[
        (
            "base.ts",
            r#"
export class Foundation<T = any> {
    marker!: string;
    emit(): string { return ""; }
    payload!: T;
}
"#,
        ),
        (
            "main.ts",
            r#"
import { Foundation } from './base';
interface Wing extends Foundation<bigint> {}
declare const w: Wing;
const a: string = w.emit();
const b: string = w.marker;
const c: bigint = w.payload;
"#,
        ),
    ]);
}

/// Non-generic cross-module class base with a renamed default import — the
/// materialization is structural, not arity- or name-scoped.
#[test]
fn interface_extends_cross_module_nongeneric_default_class_inherits_members() {
    assert_no_missing_property_both_orders(&[
        (
            "widget.ts",
            r#"
export default class Gadget {
    serial!: number;
    describe(): string { return ""; }
}
"#,
        ),
        (
            "app.ts",
            r#"
import Thing from './widget';
interface Panel extends Thing {}
declare const p: Panel;
const a: number = p.serial;
const b: string = p.describe();
"#,
        ),
    ]);
}

/// Multi-level: interface extends a class that extends another class, all
/// cross-module — every inherited member up the chain must resolve.
#[test]
fn interface_extends_cross_module_class_chain_inherits_all_members() {
    assert_no_missing_property_both_orders(&[
        (
            "animals.ts",
            r#"
export class Animal {
    name!: string;
}
export class Dog extends Animal {
    bark(): void {}
}
"#,
        ),
        (
            "pets.ts",
            r#"
import { Dog } from './animals';
interface Pet extends Dog {}
declare const pet: Pet;
const n: string = pet.name;
pet.bark();
"#,
        ),
    ]);
}

/// Control: interface extends a cross-module *interface* (not a class) must
/// keep resolving — the new class-instance branch returns `None` for non-class
/// bases so the interface/alias resolution still runs.
#[test]
fn interface_extends_cross_module_interface_still_resolves() {
    assert_no_missing_property_both_orders(&[
        (
            "shapes.ts",
            r#"
export interface Shape {
    area(): number;
}
"#,
        ),
        (
            "solids.ts",
            r#"
import type { Shape } from './shapes';
interface Solid extends Shape {}
declare const s: Solid;
const x: number = s.area();
"#,
        ),
    ]);
}

/// Negative control: a genuinely missing member on an interface extending a
/// cross-module class still reports exactly one TS2339 — instance-member
/// materialization must not silence real violations.
#[test]
fn interface_extends_cross_module_class_missing_member_still_errors() {
    let files = &[
        (
            "engine.ts",
            r#"
export class Engine {
    rpm!: number;
    start(): void {}
}
"#,
        ),
        (
            "turbine.ts",
            r#"
import { Engine } from './engine';
interface Turbine extends Engine {}
declare const t: Turbine;
t.rpm;
t.start();
t.absent;
"#,
        ),
    ];
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let missing = missing_property_messages(&compile_in_order(files, &names));
    assert_eq!(
        missing.len(),
        1,
        "expected exactly one TS2339 for the genuinely missing member, got: {missing:?}"
    );
    assert!(
        missing[0].contains("'absent'"),
        "the surviving TS2339 must be for the missing member, got: {:?}",
        missing[0]
    );
}
