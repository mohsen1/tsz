use super::super::core::*;

#[test]
fn test_argument_count_mismatch_preserves_call_return_for_follow_on_property_access() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
const f = (hdr: string, val: number) => `${hdr}:\t${val}\r\n` as `${string}:\t${number}\r\n`;
f("x").foo;
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2554),
        "Expected TS2554 for missing call argument. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 to remain on the call result after TS2554 recovery. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_only_namespace_reexport_chain_does_not_emit_ts2305() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "lib.d.ts",
                "interface Array<T> {}\ninterface Boolean {}\ninterface CallableFunction {}\ninterface Function {}\ninterface IArguments {}\ninterface NewableFunction {}\ninterface Number {}\ninterface Object {}\ninterface RegExp {}\ninterface String {}\n",
            ),
            ("a.ts", "export class A {}\n"),
            ("b.ts", "export * as a from './a';\n"),
            ("c.ts", "import type { a } from './b';\nexport { a };\n"),
            ("d.ts", "import { a } from './c';\nnew a.A();\n"),
        ],
        "d.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2305),
        "Did not expect TS2305 for a type-only namespace re-export chain. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_extends_implicit_any_reports_ts2314_and_ts8026() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        r#"
/**
 * @template T
 */
class A {}

class B extends A {}

/** @augments A */
class C extends A {}

/** @augments A<number, number, number> */
class D extends A {}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2314: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2314)
        .collect();
    let ts8026: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 8026)
        .collect();

    assert_eq!(
        ts2314.len(),
        2,
        "Expected two TS2314 diagnostics for malformed @augments tags. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2314.iter().all(|(_, message)| message.contains("A<T>")),
        "Expected TS2314 messages to preserve the generic display name. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts8026.len(),
        1,
        "Expected one TS8026 diagnostic for the missing @extends tag. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts8026[0].1.contains("A<T>"),
        "Expected TS8026 to mention the generic base class display name. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_imported_generic_extends_without_augments_emits_ts8026_only() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            ("somelib.d.ts", "export declare class Foo<T> { prop: T; }\n"),
            (
                "index.js",
                r#"
import { Foo } from "./somelib";

class MyFoo extends Foo {
    constructor() {
        super();
        this.prop.alpha = 12;
    }
}
"#,
            ),
        ],
        "index.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8026: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 8026)
        .collect();

    assert_eq!(
        ts8026.len(),
        1,
        "Expected one TS8026 diagnostic for the missing @extends tag on an imported generic base. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts8026[0].1.contains("Foo<T>"),
        "Expected TS8026 to mention the imported generic base class display name. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2314),
        "Did not expect TS2314 for imported generic bases in JS. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_default_lib_collection_extends_does_not_emit_ts8026() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.js",
        r#"
class MySet extends Set {
    constructor() {
        super();
    }
}

class MyWeakSet extends WeakSet {
    constructor() {
        super();
    }
}

class MyMap extends Map {
    constructor() {
        super();
    }
}

class MyWeakMap extends WeakMap {
    constructor() {
        super();
    }
}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 8026),
        "Did not expect TS8026 for default-lib collection bases in JS. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unbounded_generic_constraint_mismatch_preserves_record_alias_display() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
function f3<T extends Record<string, any>>(o: T) {}

function user<T>(t: T) {
  f3(t);
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2345 && message.contains("Record<string, any>") }),
        "Expected TS2345 to preserve Record<string, any> in the parameter display. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_only_namespace_export_is_importable_from_reexporting_module() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            ("a.ts", "export class A {}\n"),
            ("b.ts", "export * as a from './a';\n"),
            ("c.ts", "import type { a } from './b';\nexport { a };\n"),
        ],
        "c.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2305),
        "Did not expect TS2305 when importing a namespace export through a re-exporting module. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_only_namespace_export_is_importable_from_reexporting_module_with_absolute_paths() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            ("/tmp/tsz-export-namespace/a.ts", "export class A {}\n"),
            (
                "/tmp/tsz-export-namespace/b.ts",
                "export * as a from './a';\n",
            ),
            (
                "/tmp/tsz-export-namespace/c.ts",
                "import type { a } from './b';\nexport { a };\n",
            ),
        ],
        "/tmp/tsz-export-namespace/c.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2305),
        "Did not expect TS2305 for an absolute-path namespace re-export import. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_keyword_as_identifier_in_different_scopes_no_false_ts2300() {
    // strictModeUseContextualKeyword.ts: `as` used as identifier in different scopes
    // should NOT produce TS2300 (Duplicate identifier).
    // A function declaration at the top level of another function's body is function-scoped,
    // not block-scoped, so it shouldn't conflict with outer-scope declarations.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
"use strict"
var as = 0;
function foo(as: string) { }
class C {
    public as() { }
}
function F() {
    function as() { }
}
function H() {
    let {as} = { as: 1 };
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2300_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2300)
        .collect();

    assert!(
        ts2300_diagnostics.is_empty(),
        "Should not emit TS2300 for contextual keyword 'as' used in different scopes. Got: {ts2300_diagnostics:#?}"
    );
}

#[test]
fn test_interface_does_not_depend_on_base_types_ts2339() {
    let source = r#"
// @target: es2015
var x: StringTree;
if (typeof x !== "string") {
    x.push("");
    x.push([""]);
}

type StringTree = string | StringTreeArray;
interface StringTreeArray extends Array<StringTree> { }
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 'Property push does not exist on type StringTree', got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2454),
        "Expected TS2454 'Variable x is used before being assigned', got: {diagnostics:?}"
    );
}

/// Full conformance test for classImplementsClass4.ts.
///
/// Oracle-corrected (`typescript@7.0.2`, re-verified for #17203): the original
/// assertions here (`[2720, 2741]`, no `2322`) do not match the pinned oracle,
/// which reports **two** top-level diagnostics — `error TS2720` at the
/// `implements` clause and a *separate* `error TS2322` at the second
/// assignment (`c2 = c`), each carrying its own `"Property 'x' is missing..."`
/// elaboration. `tsc` never emits a standalone top-level `TS2741` for this
/// shape; the elaboration is a message-chain link nested under the outer
/// diagnostic (`ts.DiagnosticMessageChain`, code `2741`, no source span of its
/// own) — tsz models that as `related_information` with `kind: ChainLink`,
/// not as its own entry in the flat diagnostics list.
///
/// tsz's `TS2322` chain-link elaboration already matches the oracle exactly
/// (verified byte-for-byte). Its `TS2720` now carries the matching elaboration
/// too (#17216): the whole-type relation names the offending member, so this
/// implementing class (no member of that name) gets the "is missing" form,
/// while the adjacent `case6` shape — a class with its own same-named private
/// member — gets `"Types have separate declarations of a private property
/// 'x'."`. The class-target private/protected branch used to early-exit with a
/// bare TS2720 and no member line; it now routes through the structural
/// relation exactly like the assignment path above.
#[test]
fn test_class_implements_class4_full_conformance() {
    let source = r#"
class A {
    private x = 1;
    foo(): number { return 1; }
}
class C implements A {
    foo() {
        return 1;
    }
}
class C2 extends A {}
declare var c: C;
declare var c2: C2;
c = c2;
c2 = c;
"#;
    let raw = compile_and_get_raw_diagnostics_named("test.ts", source, CheckerOptions::default());
    let codes: Vec<u32> = raw.iter().map(|d| d.code).collect();
    let implements_diag = raw.iter().find(|d| d.code == 2720).unwrap_or_else(|| {
        panic!("Expected TS2720 for 'class C implements A'. Got codes: {codes:?}")
    });
    // #17216: the class-target private branch now nests the missing-member line
    // under TS2720, matching the oracle's message chain (`C` has no member `x`).
    assert!(
        implements_diag
            .message_text
            .contains("Property 'x' is missing in type 'C' but required in type 'A'."),
        "TS2720 must nest the missing-member elaboration, got: {:?}",
        implements_diag.message_text
    );
    let assignment_diag = raw
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322 at 'c2 = c'. Got codes: {codes:?}"));
    assert_eq!(
        assignment_diag.message_text, "Type 'C' is not assignable to type 'C2'.",
        "unexpected TS2322 message"
    );
    let missing_link = assignment_diag
        .related_information
        .iter()
        .find(|info| info.code == 2741)
        .unwrap_or_else(|| {
            panic!(
                "Expected a TS2741 chain-link elaboration on the TS2322. Got: {:?}",
                assignment_diag.related_information
            )
        });
    assert_eq!(
        missing_link.message_text, "Property 'x' is missing in type 'C' but required in type 'A'.",
        "TS2741 chain link should name the declaring class 'A', not the target 'C2'"
    );
}

/// Test: class with only private members missing carries the missing-property
/// elaboration when assigning `C` (no private members) to `C2` (extends `A`,
/// which has `private x`).
///
/// Oracle-corrected (`typescript@7.0.2`): `tsc` reports one top-level
/// `error TS2322` at `c2 = c` — never a standalone `TS2741` — with
/// `"Property 'x' is missing in type 'C' but required in type 'A'."` nested as
/// a message-chain link. tsz's `related_information` chain link matches this
/// exactly; the original assertion (`has_error(.., 2741)`) checked for a
/// top-level diagnostic code `tsc` never produces for this shape.
#[test]
fn test_class_missing_private_member_simple() {
    let source = r#"
class A {
    private x = 1;
}
class C {}
class C2 extends A {}
declare var c: C;
declare var c2: C2;
c2 = c;
"#;
    let raw = compile_and_get_raw_diagnostics_named("test.ts", source, CheckerOptions::default());
    let assignment_diag = raw.iter().find(|d| d.code == 2322).unwrap_or_else(|| {
        panic!(
            "Expected TS2322 for 'c2 = c'. Got: {:?}",
            raw.iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        )
    });
    let missing_link = assignment_diag
        .related_information
        .iter()
        .find(|info| info.code == 2741)
        .unwrap_or_else(|| {
            panic!(
                "Expected a TS2741 chain-link elaboration on the TS2322. Got: {:?}",
                assignment_diag.related_information
            )
        });
    assert_eq!(
        missing_link.message_text,
        "Property 'x' is missing in type 'C' but required in type 'A'."
    );
}

/// Test: numeric enum mapped type assignment should not produce false TS2322.
/// Based on conformance test `numericEnumMappedType.ts`.
#[test]
fn test_numeric_enum_mapped_type_no_false_ts2322() {
    let source = r#"
enum E1 { ONE, TWO, THREE }
declare enum E2 { ONE, TWO, THREE }
type Bins1 = { [k in E1]?: string; }
type Bins2 = { [k in E2]?: string; }
const b1: Bins1 = {};
const b2: Bins2 = {};
const e1: E1 = E1.ONE;
const e2: E2 = E2.ONE;
b1[1] = "a";
b1[e1] = "b";
b2[1] = "a";
b2[e2] = "b";
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Should not emit false TS2322 for numeric enum mapped type access. Got: {diagnostics:?}"
    );
}

/// Test: spread of boolean respects freshness - no false TS2322.
/// Based on conformance test `spreadBooleanRespectsFreshness.ts`.
#[test]
fn test_spread_boolean_respects_freshness_no_false_ts2322() {
    let source = r#"
type Foo = FooBase | FooArray;
type FooBase = string | false;
type FooArray = FooBase[];
declare let foo1: Foo;
declare let foo2: Foo;
foo1 = [...Array.isArray(foo2) ? foo2 : [foo2]];
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Should not emit false TS2322 for spread boolean freshness. Got: {diagnostics:?}"
    );
}

/// Test: inline conditional has similar assignability - no false TS2322.
/// Based on conformance test `inlineConditionalHasSimilarAssignability.ts`.
#[test]
fn test_inline_conditional_assignability_no_false_ts2322() {
    let source = r#"
type MyExtract<T, U> = T extends U ? T : never

function foo<T>(a: T) {
  const b: Extract<any[], T> = 0 as any;
  a = b; // ok

  const c: (any[] extends T ? any[] : never) = 0 as any;
  a = c;

  const d: MyExtract<any[], T> = 0 as any;
  a = d; // ok

  type CustomType = any[] extends T ? any[] : never;
  const e: CustomType = 0 as any;
  a = e;
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Should not emit false TS2322 for inline conditional assignability. Got: {diagnostics:?}"
    );
}

/// Test: spread of object literal assignable to index signature - no false TS2322.
/// Based on conformance test `spreadOfObjectLiteralAssignableToIndexSignature.ts`.
#[test]
fn test_spread_object_literal_to_index_signature_no_false_ts2322() {
    let source = r#"
type Record<K extends keyof any, T> = { [P in K]: T };
const foo: Record<never, never> = {}
interface RecordOfRecords extends Record<keyof any, RecordOfRecords> {}
const recordOfRecords: RecordOfRecords = {}
recordOfRecords.propA = {...(foo !== undefined ? {foo} : {})}
recordOfRecords.propB = {...(foo && {foo})}
recordOfRecords.propC = {...(foo !== undefined && {foo})}
interface RecordOfRecordsOrEmpty extends Record<keyof any, RecordOfRecordsOrEmpty | {}> {}
const recordsOfRecordsOrEmpty: RecordOfRecordsOrEmpty = {}
recordsOfRecordsOrEmpty.propA = {...(foo !== undefined ? {foo} : {})}
recordsOfRecordsOrEmpty.propB = {...(foo && {foo})}
recordsOfRecordsOrEmpty.propC = {...(foo !== undefined && {foo})}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Should not emit false TS2322 for spread to index signature. Got: {diagnostics:?}"
    );
}

/// Same test but with the full source from deeplyNestedCheck.ts conformance test.
/// Includes both the object literal part (TS2741) and the array part (TS2322).
#[test]
fn test_deeply_nested_object_literal_missing_property_full_depth() {
    let source = r#"
interface DataSnapshot<X = {}> {
  child(path: string): DataSnapshot;
}

interface Snapshot<T> extends DataSnapshot {
  child<U extends Extract<keyof T, string>>(path: U): Snapshot<T[U]>;
}

interface A { b: B[] }
interface B { c: C }
interface C { d: D[] }
interface D { e: E[] }
interface E { f: F[] }
interface F { g: G }
interface G { h: H[] }
interface H { i: string }

const x: A = {
  b: [
    {
      c: {
        d: [
          {
            e: [
              {
                f: [
                  {
                    g: {
                      h: [
                        {
                        },
                      ],
                    },
                  },
                ],
              },
            ],
          },
        ],
      },
    },
  ],
};

const a1: string[][][][][] = [[[[[42]]]]];
const a2: string[][][][][][][][][][] = [[[[[[[[[[42]]]]]]]]]];
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2741),
        "Expected TS2741 for deeply nested missing property 'i'. Got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for deeply nested array type mismatch. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2403_promise_identity_with_constraints_and_lib() {
    // Same test but with lib files loaded (matches conformance binary behavior)
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
export interface IPromise<T, V> {
    then<U extends T, W extends V>(callback: (x: T) => IPromise<U, W>): IPromise<U, W>;
}
export interface Promise<T, V> {
    then<U extends T, W extends V>(callback: (x: T) => Promise<U, W>): Promise<U, W>;
}

// Error because constraint V doesn't match
var x: IPromise<string, number>;
var x: Promise<string, boolean>;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2403),
        "Expected TS2403 for redeclaration with different generic interface types (with lib).\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2403_promise_identity_with_constraints() {
    // promiseIdentityWithConstraints.ts: different constraints on type params
    // should cause TS2403 because the types are not identical
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
export interface IPromise<T, V> {
    then<U extends T, W extends V>(callback: (x: T) => IPromise<U, W>): IPromise<U, W>;
}
export interface Promise<T, V> {
    then<U extends T, W extends V>(callback: (x: T) => Promise<U, W>): Promise<U, W>;
}

// Error because constraint V doesn't match
var x: IPromise<string, number>;
var x: Promise<string, boolean>;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2403),
        "Expected TS2403 for redeclaration with different generic interface types.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2403_promise_identity_different_type_param_arity() {
    // promiseIdentityWithAny2.ts: different type parameter arity should cause TS2403
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
export interface IPromise<T, V> {
    then<U, W>(callback: (x: T) => IPromise<U, W>): IPromise<U, W>;
}
interface Promise<T, V> {
    then(callback: (x: T) => Promise<any, any>): Promise<any, any>;
}

// Error because type parameter arity doesn't match
var x: IPromise<string, number>;
var x: Promise<string, boolean>;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2403),
        "Expected TS2403 for redeclaration with different generic interface types (different arity).\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2403_promise_identity_structurally_identical_no_error() {
    // promiseIdentity.ts lines 8-9: IPromise<string> vs Promise<string>
    // with structurally identical interfaces should NOT produce TS2403
    // (tsc considers these identical via structural identity with coinductive cycles)
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
export interface IPromise<T> {
    then<U>(callback: (x: T) => IPromise<U>): IPromise<U>;
}
interface Promise2<T> {
    then<U>(callback: (x: T) => Promise2<U>): Promise2<U>;
}
var x: IPromise<string>;
var x: Promise2<string>;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2403),
        "Should NOT get TS2403 when interfaces are structurally identical.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2403_overload_order_is_not_redeclaration_identical() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface A {
    (x: { s: string }): string;
    (x: { n: number }): number;
}

interface C {
    (x: { n: number }): number;
    (x: { s: string }): string;
}

declare var w: A;
declare var w: C;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2403),
        "Expected TS2403 when overloaded signature order differs.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_failed_overload_call_returns_never_for_follow_on_property_access() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface A {
    (x: { s: string }): string;
    (x: { n: number }): number;
}

interface B {
    (x: { s: string }): string;
    (x: { n: number }): number;
}

declare var v: A;
declare var v: B;

v({ s: "", n: 0 }).toLowerCase();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2769_count = diagnostics.iter().filter(|(code, _)| *code == 2769).count();
    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();

    assert_eq!(
        ts2769_count, 1,
        "Expected exactly one TS2769 for the failed overload call.\nActual: {diagnostics:#?}"
    );
    assert_eq!(
        ts2339_count, 1,
        "Expected the failed overload result to behave like never and surface TS2339 on `.toLowerCase()`.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2339 && message.contains("type 'never'") }),
        "Expected TS2339 to report receiver type 'never', not an arbitrary failed overload return.\nActual: {diagnostics:#?}"
    );
}

// tsz must match tsc 6.0.2: a genuine no-overload-match reports TS2769 even when
// the callee's class carries its own structural error (TS2420). tsc treats the
// two as independent and emits TS2420 + TS2769 + the `never` follow-on TS2339
// together (verified against tsc 6.0.2). tsz previously suppressed TS2769 when a
// structural class/interface/namespace error sat on the callee's base symbol,
// matching neither tsc nor its own design goal; that suppression is removed.
#[test]
fn test_no_overload_match_reports_ts2769_even_with_structural_callee_error() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Required {
    required: string;
}

class Broken implements Required {
    static make(x: { n: number }): number;
    static make(x: { s: string }): string;
    static make(_x: unknown): string | number {
        return "";
    }
}

Broken.make({ s: "", n: 0 }).toLowerCase();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2420),
        "Expected TS2420 for the structurally invalid class.\nActual: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2769),
        "Expected TS2769 for the failed overload call; the callee's structural error must not suppress it (tsc emits both).\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("type 'never'")),
        "Expected the failed overload result to behave like `never` and surface TS2339 on `.toLowerCase()`, matching tsc.\nActual: {diagnostics:#?}"
    );
}

// Same rule with renamed binders, proving it is structural and not keyed on the
// `Broken`/`make` identifiers. tsc emits TS2420 + TS2769 + the `never` TS2339.
#[test]
fn test_no_overload_match_reports_ts2769_structural_callee_error_renamed() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Spec {
    value: string;
}

class Service implements Spec {
    static pick(x: { id: number }): number;
    static pick(x: { name: string }): string;
    static pick(_x: unknown): string | number {
        return "";
    }
}

Service.pick({ id: 0, name: "x" }).toUpperCase();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2420),
        "Expected TS2420 for the structurally invalid class.\nActual: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2769),
        "Expected TS2769 for the failed overload call regardless of binder names.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("type 'never'")),
        "Expected the failed overload result to surface a `never` TS2339, matching tsc.\nActual: {diagnostics:#?}"
    );
}

// Control: a structurally-broken callee class must not turn a *matching* overload
// call into a spurious no-overload-match. When one overload matches, tsc reports
// only the class's TS2420 and resolves the call to the matched return type, so the
// follow-on member access is valid (no TS2769, no `never` TS2339).
#[test]
fn test_matching_overload_on_structurally_broken_class_still_resolves() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Spec {
    value: string;
}

class Service implements Spec {
    static pick(x: { id: number }): number;
    static pick(x: { name: string }): string;
    static pick(_x: unknown): string | number {
        return "";
    }
}

Service.pick({ name: "x" }).toLowerCase();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2420),
        "Expected TS2420 for the structurally invalid class.\nActual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "A matching overload must not be reported as a no-overload-match.\nActual: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("type 'never'")),
        "A matching overload resolves to its real return type, so the follow-on access must be valid.\nActual: {diagnostics:#?}"
    );
}

/// TS2304: implements clause with unresolved name should emit TS2304.
/// From: bind1.ts
#[test]
fn test_ts2304_implements_unresolved_name() {
    let diagnostics = compile_and_get_diagnostics(
        r"
namespace M {
    export class C implements I {}
}
        ",
    );
    assert!(
        has_error(&diagnostics, 2304),
        "Should emit TS2304 for unresolved 'I' in implements clause (in namespace).\nActual errors: {diagnostics:#?}"
    );
}

/// TS2304: implements clause with unresolved name at top level should also emit TS2304.
#[test]
fn test_ts2304_implements_unresolved_name_top_level() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C implements I {}
        ",
    );
    assert!(
        has_error(&diagnostics, 2304),
        "Should emit TS2304 for unresolved 'I' in implements clause (top level).\nActual errors: {diagnostics:#?}"
    );
}

/// TS2304: extends clause with unresolved name should emit TS2304.
#[test]
fn test_ts2304_extends_unresolved_name() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C extends I {}
        ",
    );
    assert!(
        has_error(&diagnostics, 2304),
        "Should emit TS2304 for unresolved 'I' in extends clause.\nActual errors: {diagnostics:#?}"
    );
}
