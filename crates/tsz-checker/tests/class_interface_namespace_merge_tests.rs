//! Tests for declaration-merging in type position when a single name binds a
//! `class`, an `interface`, and one or more `namespace` blocks (in any order).
//!
//! The structural rule under test:
//!
//! > When a symbol is merged from a class declaration AND an interface
//! > declaration (regardless of any additional namespace blocks), tsc resolves
//! > the symbol in type position to the class instance type, which already
//! > incorporates both the class's own members and the interface's members.
//! > tsz must do the same — independent of identifier spelling, declaration
//! > order, and the presence of namespace blocks.
//!
//! Previously, `class+interface+namespace` merging routed type-position
//! resolution through `compute_interface_type_from_declarations`, which filters
//! to interface declarations and silently drops class instance members. The
//! resulting type was missing the class's fields/methods, producing spurious
//! TS2353 ("does not exist in type") on assignments to object literals.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file, check_source_with_libs, load_lib_files};

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318) // Filter missing global type errors
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn no_ts2353(source: &str) {
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for class+interface[+namespace] merge in type position, got: {diags:?}",
    );
}

fn assert_no_codes(diags: &[(u32, String)], forbidden: &[u32]) {
    let hits: Vec<_> = diags
        .iter()
        .filter(|(code, _)| forbidden.contains(code))
        .collect();
    assert!(hits.is_empty(), "unexpected diagnostics: {hits:?}");
}

#[test]
fn class_interface_namespace_object_literal_sees_class_member() {
    // Original repro from #10931 (rule-witness: literal `Bar`).
    let source = r#"
class Bar { field: number = 1; }
interface Bar { extra: string; }
namespace Bar { export const helper = "x"; }
const c: Bar = { field: 2, extra: "y" };
"#;
    no_ts2353(source);
}

#[test]
fn class_interface_namespace_renamed_identifier_sees_class_member() {
    // Rule witness with a different identifier spelling — proves the fix is
    // not keyed on the literal name `Bar`.
    let source = r#"
class Quux { field: number = 1; }
interface Quux { extra: string; }
namespace Quux { export const helper = "h"; }
const q: Quux = { field: 9, extra: "z" };
"#;
    no_ts2353(source);
}

#[test]
fn cross_file_namespace_class_instance_members_survive_delegation() {
    let diags = check_multi_file(
        &[
            (
                "part1.ts",
                r#"
namespace A {
    export interface Point { x: number; y: number; }
}
"#,
            ),
            (
                "part2.ts",
                r#"
namespace A {
    export namespace Utils {
        export class Plane {
            constructor(public tl: Point, public br: Point) {}
        }
    }
}
"#,
            ),
            (
                "part3.ts",
                r#"
var p: { tl: A.Point; br: A.Point };
var p: A.Utils.Plane;
const q: A.Utils.Plane = { tl: { x: 0, y: 0 }, br: { x: 1, y: 1 } };
"#,
            ),
        ],
        "part3.ts",
        CheckerOptions::default(),
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();

    assert_no_codes(&diags, &[2322, 2403]);
}

#[test]
fn default_export_property_class_import_type_position_uses_instance_type() {
    let diags = check_multi_file(
        &[
            (
                "a.ts",
                r#"
namespace A {
    export class B { constructor(b: number) {} }
    export namespace B { export const b: number = 0; }
}
export default A.B;
"#,
            ),
            (
                "index.ts",
                r#"
import B from "./a";
const b: B = new B(B.b);
"#,
            ),
        ],
        "index.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();

    assert_no_codes(&diags, &[2322, 2739]);
}

#[test]
fn default_export_identifier_class_namespace_interface_heritage_uses_instance_type() {
    let diags = check_multi_file(
        &[
            (
                "vessel.ts",
                r#"
class Vessel<Payload = unknown> {
    value!: Payload;
    static #hidden = 0;
    static make<Shape extends Vessel>(): Shape {
        return null as any;
    }
}
namespace Vessel {
    export type Core<Payload = unknown> = Vessel<Payload>;
    export const label = "vessel";
}
export default Vessel;
"#,
            ),
            (
                "cargo.ts",
                r#"
import Vessel from "./vessel";

interface Cargo extends Vessel<number> {
    kind: "cargo";
}

const cargo: Cargo = Vessel.make<Cargo>();
const value: number = cargo.value;
const label: string = Vessel.label;
"#,
            ),
        ],
        "cargo.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();

    assert!(
        diags.is_empty(),
        "default-exported class+namespace heritage should be tsc-clean: {diags:?}"
    );
}

#[test]
fn default_export_identifier_class_without_namespace_interface_heritage_uses_instance_type() {
    let diags = check_multi_file(
        &[
            (
                "crate.ts",
                r#"
class Crate<Item = unknown> {
    item!: Item;
    static #token = 0;
    static create<Shape extends Crate>(): Shape {
        return null as any;
    }
}
export default Crate;
"#,
            ),
            (
                "shipment.ts",
                r#"
import Crate from "./crate";

interface Shipment extends Crate<string> {
    ready: true;
}

const shipment: Shipment = Crate.create<Shipment>();
const item: string = shipment.item;
"#,
            ),
        ],
        "shipment.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();

    assert!(
        diags.is_empty(),
        "default-exported class heritage should be tsc-clean without a namespace: {diags:?}"
    );
}

#[test]
fn named_export_class_value_side_stays_available() {
    let diags = check_multi_file(
        &[
            (
                "repository.ts",
                r#"
export class Repository<Entry = unknown> {
    entry!: Entry;
    static open<Shape extends Repository>(): Shape {
        return null as any;
    }
}
"#,
            ),
            (
                "record.ts",
                r#"
import { Repository } from "./repository";

const record: Repository<boolean> = Repository.open<Repository<boolean>>();
const entry: boolean = record.entry;
"#,
            ),
        ],
        "record.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();

    assert!(
        diags.is_empty(),
        "named-import class type/value control should remain tsc-clean: {diags:?}"
    );
}

#[test]
fn interface_class_namespace_declaration_order_sees_class_member() {
    // Declaration order swapped: interface first, then class, then namespace.
    // (tsc reports TS2434 for namespace-before-class — that's expected and
    // not what we're testing — but the merged Bar type must still include
    // class members for the object-literal assignment.)
    let source = r#"
interface Bar { extra: string; }
class Bar { field: number = 1; }
namespace Bar { export const helper = "h"; }
const c: Bar = { field: 1, extra: "x" };
"#;
    no_ts2353(source);
}

#[test]
fn class_interface_multiple_namespace_blocks_sees_class_member() {
    // Multiple namespace blocks merging into the same symbol.
    let source = r#"
class Bar { field: number = 1; }
interface Bar { extra: string; }
namespace Bar { export const a = 1; }
namespace Bar { export const b = 2; }
const v: Bar = { field: 0, extra: "y" };
"#;
    no_ts2353(source);
}

#[test]
fn class_interface_namespace_class_methods_and_properties_visible() {
    // Both methods and properties from the class declaration must be visible
    // in the merged type's structural shape.
    let source = r#"
class Bar {
  field: number = 1;
  method(): void {}
}
interface Bar { extra: string; }
namespace Bar { export const helper = "x"; }
const c: Bar = { field: 2, extra: "y", method() {} };
"#;
    no_ts2353(source);
}

#[test]
fn class_interface_namespace_property_access_preserves_class_members() {
    // Property-access path: class members and interface members must both be
    // resolvable through the merged symbol.
    let source = r#"
class Bar {
  field: number = 1;
  method(): string { return "m"; }
}
interface Bar { extra?: boolean; }
namespace Bar { export const helper = 1; }

declare const b: Bar;
const a: number = b.field;
const s: string = b.method();
const e: boolean | undefined = b.extra;
"#;
    let diags = get_diagnostics(source);
    let blockers: Vec<_> = diags
        .iter()
        .filter(|d| d.0 == 2339 || d.0 == 2353 || d.0 == 2322)
        .collect();
    assert!(
        blockers.is_empty(),
        "Property access on class+interface+namespace merge should not report TS2339/TS2353/TS2322; got: {diags:?}",
    );
}

#[test]
fn class_interface_namespace_generic_class_preserves_type_params() {
    // Generic class merged with interface and namespace — type-position
    // resolution must still produce the class instance type with type
    // parameters preserved.
    let source = r#"
class Box<T> {
  value: T;
  constructor(v: T) { this.value = v; }
}
interface Box<T> { id: string; }
namespace Box { export const empty = 0; }

declare const b: Box<number>;
const v: number = b.value;
const id: string = b.id;
"#;
    let diags = get_diagnostics(source);
    let blockers: Vec<_> = diags
        .iter()
        .filter(|d| d.0 == 2339 || d.0 == 2353 || d.0 == 2322)
        .collect();
    assert!(
        blockers.is_empty(),
        "Generic class+interface+namespace property access should not report TS2339/TS2353/TS2322; got: {diags:?}",
    );
}

#[test]
fn class_only_namespace_excess_property_still_flagged() {
    // Negative case: class+namespace (no interface) must still reject an
    // object literal property that exists on neither the class nor the
    // namespace. This proves the fix does not silently widen the merged
    // type to accept any property.
    let source = r#"
class Bar { field: number = 1; }
namespace Bar { export const helper = "x"; }
const c: Bar = { field: 1, bogus: 1 };
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for unknown property 'bogus', got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'bogus'"),
        "Expected TS2353 to mention excess key bogus, got: {ts2353:?}",
    );
}

#[test]
fn class_interface_namespace_excess_property_still_flagged() {
    // Negative case: class+interface+namespace must still reject excess
    // properties that exist on neither the class nor the interface.
    let source = r#"
class Bar { field: number = 1; }
interface Bar { extra: string; }
namespace Bar { export const helper = "x"; }
const c: Bar = { field: 1, extra: "x", bogus: 1 };
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for unknown property 'bogus', got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'bogus'"),
        "Expected TS2353 to mention excess key bogus, got: {ts2353:?}",
    );
}
