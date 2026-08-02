//! Regression tests for declaration merging across the bodies of a merged
//! namespace.
//!
//! `tsc` gives every namespace body its own `locals` table while the merged
//! namespace symbol owns a single shared `exports` table. A declaration written
//! without `export` therefore becomes a local of the body that wrote it and never
//! joins an exported declaration of the same name contributed by a *different*
//! body — `A.Point` sees only what was exported.
//!
//! tsz applied that rule to `var` only, so an interface / class / function /
//! enum / namespace / `let` / `const` written without `export` in a second body
//! merged into the first body's exported member and widened the namespace's
//! public type. The witness is
//! `conformance/internalModules/DeclarationMerging/TwoInternalModulesThatMergeEachWithExportedAndNonExportedInterfacesOfTheSameName.ts`,
//! where the merge added `fromCarth` to `A.Point` and produced a spurious TS2403.
//!
//! Within a *single* body the two declarations do land on one symbol — that is
//! what `tsc` reports TS2395 against — so the split stays scoped to the
//! cross-body case and those tests pin that it still does.
//!
//! Every expectation here was taken from `tsc` 7.0.2 run over the same source
//! with `--strict false --target es2015`.

use tsz_checker::test_utils::check_source_codes;

/// `var x: <expected>; var x: <actual>;` reads an inferred type back out as a
/// diagnostic: TS2403 fires exactly when the two are not identical. No TS2403
/// means the namespace member has the type the first declaration spells.
fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

#[test]
fn non_exported_interface_does_not_join_another_bodys_exported_interface() {
    let source = r"
namespace A {
    export interface Point {
        x: number;
        y: number;
        toCarth(): Point;
    }
}

namespace A {
    interface Point {
        fromCarth(): Point;
    }
}

var p: { x: number; y: number; toCarth(): A.Point; };
var p: A.Point;
";
    assert!(
        codes(source).is_empty(),
        "A.Point must stay the exported declaration only: {:?}",
        codes(source)
    );
}

/// Same rule, different binder names, to prove nothing keys on the identifiers
/// used by the conformance fixture.
#[test]
fn non_exported_interface_split_is_not_keyed_on_declaration_names() {
    let source = r"
namespace Widgets {
    export interface Descriptor { width: number; }
}

namespace Widgets {
    interface Descriptor { height: number; }
}

var d: { width: number };
var d: Widgets.Descriptor;
";
    assert!(
        codes(source).is_empty(),
        "renamed binders must behave identically: {:?}",
        codes(source)
    );
}

#[test]
fn exported_interfaces_in_separate_bodies_still_merge() {
    let source = r"
namespace A {
    export interface Shape { a: number; }
}

namespace A {
    export interface Shape { b: number; }
}

var c: { a: number; b: number };
var c: A.Shape;
";
    assert!(
        codes(source).is_empty(),
        "two exported declarations must still merge into one interface: {:?}",
        codes(source)
    );
}

#[test]
fn non_exported_declaration_first_then_exported_stays_separate() {
    let source = r"
namespace A {
    interface Shape { b: number; }
}

namespace A {
    export interface Shape { a: number; }
}

var c: { a: number };
var c: A.Shape;
";
    assert!(
        codes(source).is_empty(),
        "the exported declaration must not absorb an earlier body's local: {:?}",
        codes(source)
    );
}

#[test]
fn same_body_export_mismatch_still_merges_and_reports_ts2395() {
    let source = r"
namespace A {
    export interface Shape { a: number; }
    interface Shape { b: number; }
}
";
    let codes = codes(source);
    assert_eq!(
        codes,
        vec![2395, 2395],
        "within one body the declarations share a symbol, which is what TS2395 reports"
    );
}

#[test]
fn non_exported_nested_namespace_does_not_join_another_bodys_export() {
    let source = r"
namespace Outer {
    export namespace Inner { export interface Q { a: number; } }
}

namespace Outer {
    namespace Inner { export interface Q { b: number; } }
}

var q: { a: number };
var q: Outer.Inner.Q;
";
    assert!(
        codes(source).is_empty(),
        "a non-exported nested namespace is a local of its own body: {:?}",
        codes(source)
    );
}

#[test]
fn non_exported_function_does_not_join_another_bodys_exported_function() {
    let source = r"
namespace A {
    export function f(a: number) { return a; }
}

namespace A {
    function f(b: string) { return b; }
}

var g: (a: number) => number;
var g: typeof A.f;
";
    assert!(
        codes(source).is_empty(),
        "the two functions are separate symbols, so no duplicate-implementation error: {:?}",
        codes(source)
    );
}

#[test]
fn non_exported_class_does_not_join_another_bodys_exported_class() {
    let source = r"
namespace A {
    export class K { m(): number { return 1; } }
}

namespace A {
    class K { n(): string { return 'x'; } }
}

var k: { m(): number };
var k: A.K;
";
    assert!(
        codes(source).is_empty(),
        "A.K must stay the exported class: {:?}",
        codes(source)
    );
}

/// Enum object types are never identical to an object literal type, so the
/// `var x; var x` witness does not apply here (`tsc` reports TS2403 for it too).
/// Member access reads the split out directly instead: the second body's member
/// must not become visible on the exported enum.
#[test]
fn non_exported_enum_does_not_join_another_bodys_exported_enum() {
    let source = r"
namespace A {
    export enum E { First }
}

namespace A {
    enum E { Second }
}

var ok = A.E.First;
var bad = A.E.Second;
";
    assert_eq!(
        codes(source),
        vec![2339],
        "only the local body's member is missing from the exported enum"
    );
}

#[test]
fn non_exported_const_does_not_join_another_bodys_exported_const() {
    let source = r"
namespace A {
    export const v: number = 1;
}

namespace A {
    const v: string = 'x';
}

var w: number;
var w: typeof A.v;
";
    assert!(
        codes(source).is_empty(),
        "A.v must keep the exported const's type: {:?}",
        codes(source)
    );
}

/// The local declaration is what the *writing* body sees. Inside the second body
/// `Shape` is its own local interface, so `local` is typed by it, not by the
/// namespace's exported member.
#[test]
fn the_local_declaration_shadows_the_export_inside_its_own_body() {
    let source = r"
namespace A {
    export interface Shape { a: number; }
}

namespace A {
    interface Shape { b: number; }
    export var local: Shape;
}

var c: { b: number };
var c: typeof A.local;
";
    assert!(
        codes(source).is_empty(),
        "references inside the declaring body resolve to that body's local: {:?}",
        codes(source)
    );
}

/// The dotted / nested spelling of the same fixture shape, which reaches the
/// rule through a differently built namespace chain.
#[test]
fn dotted_namespace_bodies_split_a_non_exported_member_the_same_way() {
    let source = r"
namespace X.Y.Z {
    export interface Line { start: number; }
}

namespace X {
    export namespace Y.Z {
        interface Line { end: number; }
    }
}

var l: { start: number };
var l: X.Y.Z.Line;
";
    assert!(
        codes(source).is_empty(),
        "dotted namespace bodies follow the same locals/exports split: {:?}",
        codes(source)
    );
}

/// Negative control for the shadow itself: a non-exported declaration must not
/// start shadowing when there is no exported member of that name to shadow. The
/// two locals here belong to different bodies and are simply unrelated.
#[test]
fn two_non_exported_declarations_in_separate_bodies_do_not_merge() {
    let source = r"
namespace A {
    interface Shape { a: number; }
    export var first: Shape;
}

namespace A {
    interface Shape { b: number; }
    export var second: Shape;
}

var one: { a: number };
var one: typeof A.first;
var two: { b: number };
var two: typeof A.second;
";
    assert!(
        codes(source).is_empty(),
        "each body's local interface is its own type: {:?}",
        codes(source)
    );
}

/// An **ambient** namespace body is an export context: `tsc`'s
/// `setExportContextFlag` marks it so, and every declaration inside is implicitly
/// exported even without the keyword. The split must not fire there, or
/// `declare namespace JSX { interface IntrinsicElements {} }` stops merging with
/// the same shape declared in another ambient body — which is how JSX intrinsic
/// tags are contributed.
#[test]
fn ambient_namespace_bodies_are_export_contexts_and_still_merge() {
    let source = r"
declare namespace Amb {
    interface Registry { a: number; }
}

declare namespace Amb {
    interface Registry { b: number; }
}

var r: { a: number; b: number };
var r: Amb.Registry;
";
    assert!(
        codes(source).is_empty(),
        "unmarked declarations in an ambient body are implicitly exported: {:?}",
        codes(source)
    );
}

/// Same-body merging of a namespace onto an exported function is a legitimate
/// merge that this change must leave alone: both declarations are written in the
/// body that owns the symbol.
#[test]
fn same_body_namespace_still_merges_onto_an_exported_function() {
    let source = r"
namespace A {
    export function f() { }
    namespace f { export var tag: number; }
}
";
    assert!(
        !codes(source).contains(&2403),
        "same-body function/namespace merging must be untouched: {:?}",
        codes(source)
    );
}
