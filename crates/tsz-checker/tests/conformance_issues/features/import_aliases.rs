use super::super::core::*;

#[test]
fn test_ts2536_with_lib_mismatched_keyof_source() {
    // Matches intersectionsOfLargeUnions.ts: T extends keyof ElementTagNameMap
    // (undefined), V extends HTMLElementTagNameMap[T][P].
    // HTMLElementTagNameMap is defined in lib.dom.d.ts.
    if !lib_files_available() {
        return;
    }
    let source = r#"
export function assertNodeProperty<
    T extends keyof ElementTagNameMap,
    P extends keyof ElementTagNameMap[T],
    V extends HTMLElementTagNameMap[T][P]>(value: V) {}
"#;
    let diagnostics =
        without_missing_global_type_errors(compile_and_get_diagnostics_with_lib(source));
    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 1,
        "Expected TS2536 for HTMLElementTagNameMap[T] where T extends keyof (undefined) ElementTagNameMap.\nGot: {diagnostics:#?}"
    );
}

/// Suppress cascading TS2339 when `typeof a` in a type parameter constraint
/// references a destructured parameter name.
///
/// When `<T extends typeof a>` is used with a destructured parameter
/// `({a}: {a:T})`, the `typeof a` fails to resolve (emitting TS2552/TS2304)
/// because parameter names are excluded from type parameter constraints.
/// The property access `a.b` should NOT additionally emit TS2339, as the
/// constraint failure already reports the root cause.
///
/// This tests the fix for: parameterNamesInTypeParameterList.ts conformance
#[test]
fn test_no_cascading_ts2339_for_typeof_constraint_with_destructured_param() {
    let source = r#"
class A {
    m1<T extends typeof a>({a}: {a:T}) {
        a.b
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // TS2552 should be emitted for the failed `typeof a` constraint
    assert!(
        has_error(&diagnostics, 2552),
        "Should emit TS2552 for unresolvable typeof constraint. Got: {diagnostics:?}"
    );
    // TS2339 should NOT be emitted — it's a cascading error from the constraint failure
    assert!(
        !has_error(&diagnostics, 2339),
        "Should suppress cascading TS2339 on destructured param with failed constraint. Got: {diagnostics:?}"
    );
}

/// Same as above but for standalone functions (not class methods).
#[test]
fn test_no_cascading_ts2339_for_typeof_constraint_with_destructured_param_function() {
    let source = r#"
class A {}
function f1<T extends typeof a>({a}: {a:T}) {
    a.b;
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2552),
        "Should emit TS2552 for unresolvable typeof constraint. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Should suppress cascading TS2339 on destructured param with failed constraint. Got: {diagnostics:?}"
    );
}

/// Truly unconstrained type parameters should still emit TS2339.
#[test]
fn test_ts2339_still_emitted_for_unconstrained_type_parameter() {
    let source = r#"
function g<T>(a: T) {
    a.b;
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2339),
        "Unconstrained type parameter should emit TS2339. Got: {diagnostics:?}"
    );
}

// =========================================================================
// Decrement/increment operator on enum with error-typed element access
// =========================================================================

#[test]
fn test_decrement_enum_element_access_with_undeclared_index_emits_ts2356_and_ts2542() {
    // When using `ENUM1[A]--` where `A` is an undeclared identifier,
    // tsc emits TS2304 (can't find name), TS2356 (arithmetic operand type),
    // and TS2542 (readonly index signature). The element access resolves
    // through the enum's number index signature (which returns `string`),
    // so the operand type is `string` (not arithmetic). The index signature
    // is also readonly, producing TS2542.
    let source = r#"
enum ENUM1 { A, B, C }
ENUM1[A]--;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2304),
        "Expected TS2304 for undeclared identifier 'A'. Got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2356),
        "Expected TS2356 (arithmetic operand type) for ENUM1[undeclared]--. Got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2542),
        "Expected TS2542 (readonly index signature) for ENUM1[undeclared]--. Got: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_decrement_enum_literal_access_emits_ts2540_not_ts2356() {
    // For `--ENUM1["A"]` where "A" is a string literal accessing a named
    // enum member, tsc emits TS2540 (readonly property) but NOT TS2356
    // (the operand type is the enum member type, which is arithmetic).
    let source = r#"
enum ENUM1 { A, B, C }
--ENUM1["A"];
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2540),
        "Expected TS2540 for readonly enum property. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2356),
        "Should NOT emit TS2356 for valid enum member type. Got: {diagnostics:?}"
    );
}

/// TS2344 false positive: when a type parameter `K extends object` is used
/// in a class member type annotation (not a `new` expression) referencing a
/// generic with an indexed-access-type constraint like `WeakKey`, the Lazy
/// refs inside the constraint (e.g., `WeakKeyTypes[keyof WeakKeyTypes]`) must
/// be resolved before the assignability check. Without resolving them,
/// `evaluate_type_for_assignability` returns the unevaluated `IndexAccess` and
/// the check incorrectly fails.
///
/// Regression test for `esNextWeakRefs_IterableWeakMap` conformance failure.
#[test]
fn test_ts2344_indexed_access_constraint_lazy_refs_resolved_in_class_member() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface MyWeakKeyTypes {
    object: object;
    symbol: symbol;
}
type MyWeakKey = MyWeakKeyTypes[keyof MyWeakKeyTypes];

declare class MyWeakRef<T extends MyWeakKey> { deref(): T | undefined; }
declare class MyWeakMap<K extends MyWeakKey, V> { }

class Foo<K extends object, V> {
    // Type references in class member positions (field type annotation,
    // method return type) go through validate_type_reference_type_arguments,
    // NOT validate_new_expression_type_arguments. This path must also
    // resolve Lazy refs in constraints.
    weakMap: MyWeakMap<K, V> = null as any;
    ref: MyWeakRef<K> = null as any;
    getRef(): MyWeakRef<K> { return null as any; }
}
        "#,
    );

    let ts2344: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "Should NOT get TS2344: K extends object satisfies MyWeakKey (= object | symbol).\n\
         The indexed access constraint must be fully evaluated before the check.\n\
         Got: {ts2344:#?}"
    );
}

/// When a type parameter T is narrowed by a type predicate like
/// `value is Extract<T, Function>`, the narrowed type should be
/// `Extract<T, Function>`, not `T & Extract<T, Function>`.
/// The intersection is redundant because `Extract<T, U>` is always
/// a subset of T.  Keeping the intersection prevents the solver from
/// recognising the result as callable after instantiation.
#[test]
fn test_type_predicate_extract_narrowing_no_redundant_intersection() {
    let source = r#"
function isFunction<T>(value: T): value is Extract<T, Function> {
    return typeof value === "function";
}

function getFunction<T>(item: T) {
    if (isFunction(item)) {
        return item;
    }
    throw new Error();
}

function test(x: string | (() => string) | undefined) {
    if (isFunction(x)) {
        x();  // x is narrowed to Extract<...>, which is () => string
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let call_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2348 || *c == 2349)
        .collect();
    assert!(
        call_errors.is_empty(),
        "Should NOT get TS2348/TS2349 for calling Extract<T, Function> narrowed value.\n\
         The narrowed type should be callable after instantiation.\n\
         Got: {call_errors:#?}"
    );
}

/// Uninstantiated namespace members (containing only type declarations like
/// interfaces) should NOT appear as value-level properties on `typeof Namespace`.
///
/// In TypeScript, `typeof Outer` only includes instantiated sub-namespaces
/// (those with value declarations like classes, functions, variables) as properties.
/// A sub-namespace that only exports interfaces is uninstantiated and should be
/// excluded from the value type.
///
/// Regression test for: typeofInternalModules.ts conformance failure.
/// Before this fix, we emitted TS2739 (multiple missing properties: instantiated,
/// uninstantiated) instead of TS2741 (single missing property: instantiated).
#[test]
fn test_typeof_namespace_excludes_uninstantiated_sub_namespaces() {
    let source = r#"
namespace Outer {
    export namespace instantiated {
        export class C { }
    }
    export namespace uninstantiated {
        export interface P { }
    }
}

import importInst = Outer.instantiated;
var x7: typeof Outer = Outer;
x7 = importInst;
    "#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    // We should NOT get TS2739 (multiple missing properties) because
    // `uninstantiated` is a type-only namespace and shouldn't be counted.
    let ts2739: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2739).collect();
    assert!(
        ts2739.is_empty(),
        "Should NOT emit TS2739 for uninstantiated namespace member. \
         `typeof Outer` should only include `instantiated` as a property, \
         not `uninstantiated` (which only contains interface P). \
         Got: {ts2739:#?}"
    );

    // We SHOULD get TS2741 (single missing property: 'instantiated')
    // because `typeof importInst` doesn't have an `instantiated` property
    // (it IS `typeof Outer.instantiated`).
    let ts2741: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2741).collect();
    assert!(
        !ts2741.is_empty(),
        "Expected TS2741 (single missing property) for assigning \
         `typeof instantiated` to `typeof Outer`. Got diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2344_conditional_true_branch_unsatisfied_extends() {
    // When a type parameter appears in a conditional type's true branch and
    // the extends type does NOT satisfy the required constraint, TS2344 should
    // still be emitted. Previously, Case 2 in
    // `type_argument_is_narrowed_by_conditional_true_branch` would suppress the
    // error even when the arg was the check type itself.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

interface ComponentType<P> { (props: P): any; }

type Wrapper = { __brand: any };
type AnyWrapper = Wrapper | { __brand2: any };

type Inner<C extends ComponentType<any>> = C;

type PropsWithRef<C extends ComponentType<any>> = { ref: C };

// In the true branch, C is narrowed to AnyWrapper & C.
// But AnyWrapper does NOT satisfy ComponentType<any>.
// TS2344 should be emitted for the C in Inner<C>.
type TestType<C extends string | ComponentType<any>> =
    C extends AnyWrapper ? PropsWithRef<Inner<C>> : PropsWithRef<C>;
        ",
    );
    assert!(
        has_error(&diagnostics, 2344),
        "Expected TS2344 for type arg in conditional true branch where extends type \
         doesn't satisfy the constraint. Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_ts1062_self_referencing_promise_in_await() {
    if !lib_files_available() {
        return;
    }
    let source = r#"
type T1 = 1 | Promise<T1> | T1[];

export async function myFunction(param: T1) {
    const awaited = await param;
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let ts1062_count = diagnostics.iter().filter(|(code, _)| *code == 1062).count();
    assert!(
        ts1062_count >= 1,
        "Expected TS1062 for self-referencing Promise<T1> in await expression.\nGot: {diagnostics:#?}"
    );
}

#[test]
fn test_ts1062_effect_result_self_referencing_promise() {
    if !lib_files_available() {
        return;
    }
    let source = r#"
type EffectResult =
    | (() => EffectResult)
    | Promise<EffectResult>;

export async function handleEffectResult(result: EffectResult) {
    if (result instanceof Function) {
        await handleEffectResult(result());
    } else if (result instanceof Promise) {
        await handleEffectResult(await result);
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let ts1062_count = diagnostics.iter().filter(|(code, _)| *code == 1062).count();
    assert!(
        ts1062_count >= 1,
        "Expected TS1062 for self-referencing Promise<EffectResult> in await.\nGot: {diagnostics:#?}"
    );
}

#[test]
fn test_no_ts1062_for_non_self_referencing_promise() {
    if !lib_files_available() {
        return;
    }
    let source = r#"
async function normalAwait() {
    const x = await Promise.resolve(42);
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts1062_count = diagnostics.iter().filter(|(code, _)| *code == 1062).count();
    assert_eq!(
        ts1062_count, 0,
        "Should NOT emit TS1062 for non-self-referencing Promise.\nGot: {diagnostics:#?}"
    );
}

/// Module elements inside function bodies should get specific grammar errors
/// (TS1231-TS1235, TS1258) from the checker, not just TS1184 from the parser.
/// Previously the parser emitted TS1184 for export/declare in block context,
/// which set `has_syntax_parse_errors` and suppressed the checker's specific codes.
#[test]
fn module_elements_in_wrong_context_emit_specific_codes() {
    let source = r#"
function blah() {
    namespace M { }
    export namespace N { export interface I { } }
    declare module "ambient" { }
    export = M;
    export * from "ambient";
    export default class C { }
    import I = M;
    import * as Foo from "ambient";
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();

    // TS1235: namespace in wrong context
    assert!(
        codes.contains(&1235),
        "Expected TS1235 (namespace not at top level). Got codes: {codes:?}"
    );
    // TS1234: ambient module in wrong context
    assert!(
        codes.contains(&1234),
        "Expected TS1234 (ambient module not at top level). Got codes: {codes:?}"
    );
    // TS1231: export assignment in wrong context
    assert!(
        codes.contains(&1231),
        "Expected TS1231 (export assignment not at top level). Got codes: {codes:?}"
    );
    // TS1233: export declaration in wrong context
    assert!(
        codes.contains(&1233),
        "Expected TS1233 (export declaration not at top level). Got codes: {codes:?}"
    );
    // TS1232: import declaration in wrong context
    assert!(
        codes.contains(&1232),
        "Expected TS1232 (import not at top level). Got codes: {codes:?}"
    );
    // TS1184: modifiers cannot appear here (for export default class)
    assert!(
        codes.contains(&1184),
        "Expected TS1184 (modifiers cannot appear here for export class). Got codes: {codes:?}"
    );
}

/// `declare const/class/enum` inside a function body should emit TS1184.
#[test]
fn declare_in_block_context_emits_ts1184() {
    let source = r#"
function f() {
    declare const x: number;
    declare class C {}
    declare enum E { A }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts1184_count = diagnostics.iter().filter(|(c, _)| *c == 1184).count();
    assert_eq!(
        ts1184_count, 3,
        "Expected 3 TS1184 errors (one for each declare). Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_esm_module_exports_string_literal_export_no_ts2351() {
    // When an ESM module uses `export { X as "module.exports" }`, CJS consumers
    // should resolve `require()` to the class value (constructable).
    // No TS2351 ("This expression is not constructable") should be emitted.
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"
export default class Foo {}
export { Foo as "module.exports" };
                "#,
            ),
            (
                "importer.cts",
                r#"
import Foo = require("./exporter.mts");
new Foo();
                "#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2351),
        "TS2351 should not be emitted when module has 'export {{ X as \"module.exports\" }}'. \
         The require() result should be the constructable class. Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_esm_module_exports_type_only_in_node20_cts_reports_ts1362_and_ts2614() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"
export default class Foo {}
export type { Foo as "module.exports" };
                "#,
            ),
            (
                "importer.cts",
                r#"
import Foo = require("./exporter.mjs");
new Foo();

import Foo2 from "./exporter.mjs";
new Foo2();

import * as Foo3 from "./exporter.mjs";
new Foo3();

import { Oops } from "./exporter.mjs";
                "#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts1362_count = diagnostics.iter().filter(|(code, _)| *code == 1362).count();
    let ts2614_count = diagnostics.iter().filter(|(code, _)| *code == 2614).count();

    assert_eq!(
        ts1362_count, 3,
        "Expected TS1362 for require/default/namespace value use of a type-only \
         \"module.exports\" binding. Got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts2614_count, 1,
        "Expected TS2614 for named import from a module with only a default-like \
         Node20 CommonJS interop binding. Got diagnostics: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2305),
        "TS2305 should not be emitted when Node20 require interop sees a \
         default-like \"module.exports\" binding. Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_esm_module_exports_type_only_in_node20_cjs_require_reports_ts1362() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"
export default class Foo {}
export type { Foo as "module.exports" };
                "#,
            ),
            (
                "importer.cjs",
                r#"
const Foo = require("./exporter.mjs");
new Foo();
                "#,
            ),
        ],
        "importer.cjs",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 1362),
        "Expected TS1362 when a CommonJS require() binding resolves to a type-only \
         \"module.exports\" export. Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_esm_module_exports_non_default_binding_default_import_is_namespace_object() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"
export default class Foo {}
const oops = "oops";
export { oops as "module.exports" };
                "#,
            ),
            (
                "importer.cts",
                r#"
import Foo2 from "./exporter.mjs";
new Foo2();
                "#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts2351_count = diagnostics.iter().filter(|(code, _)| *code == 2351).count();
    assert_eq!(
        ts2351_count, 1,
        "Default imports of a non-constructable \"module.exports\" binding should \
         be non-constructable. Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2339_on_generic_class_computed_property_self_reference() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const rC: RC<"a">;
rC.x
declare class RC<T extends "a" | "b"> {
    x: T;
    [rC.x]: "b";
}
        "#,
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "TS2339 should not be emitted for property access on a generic class \
         used in its own computed property name. The property 'x' exists on \
         the class instance type. Got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2339_interface_self_referencing_computed_property() {
    let source = r#"
declare const rI: RI<"a">;
rI.x;
interface RI<T extends "a" | "b"> {
    x: T;
    [rI.x]: "b";
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "TS2339 should not be emitted for self-referencing computed property in interface. Got: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts18046_nested_mapped_type_with_constrained_state() {
    // When State extends Record<string, unknown> and we have nested modules
    // through a mapped type, the nested module's State should be inferred
    // from its sibling state() function, not fall back to the constraint.
    //
    // The bug: binary operations (like +, ++, --) trigger TS18046 "is of type unknown"
    // because the State type parameter falls back to `unknown` (value type of
    // Record<string, unknown>) instead of being inferred from the state() return type.
    let source = r#"
type StateFunction<State> = (s: State) => any;

type Options<State, Modules> = {
  state?: () => State;
  mutations?: Record<string, StateFunction<State>>;
  modules?: {
    [k in keyof Modules]: Options<Modules[k], never>;
  };
};

declare function create<
  State extends Record<string, unknown>,
  Modules extends Record<string, Record<string, unknown>>
>(options: Options<State, Modules>): void;

create({
  modules: {
    foo: {
      state() { return { bar2: 1 }; },
      mutations: { inc: (state) => state.bar2++ },
    },
  },
});
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 18046),
        "TS18046 should not be emitted for nested module callbacks. \
         The state parameter should be inferred as {{ bar2: number }} from \
         the sibling state() method, not fall back to 'unknown' from the \
         Record<string, unknown> constraint. Got: {diagnostics:?}"
    );
}

#[test]
fn test_module_augmentation_class_prototype_assignable_to_augmented_interface() {
    let files = [
        (
            "/child1.ts",
            r#"
import { ParentThing } from './parent';

declare module './parent' {
    interface ParentThing {
        add: (a: number, b: number) => number;
    }
}

export function child1(prototype: ParentThing) {
    prototype.add = (a: number, b: number) => a + b;
}
"#,
        ),
        (
            "/parent.ts",
            r#"
import { child1 } from './child1';

export class ParentThing implements ParentThing {}

child1(ParentThing.prototype);
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/parent.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            emit_declarations: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "Module augmentation should merge interface members into class instance type. \
         ParentThing.prototype should include the augmented 'add' member and be \
         assignable to the augmented ParentThing parameter type. Got: {diagnostics:?}"
    );
}

#[test]
fn test_constructor_narrowing_derived_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C1 { property1!: number; }
class C2 extends C1 { property2!: number; }

declare let var1: C2 | string;
if (var1.constructor === C1) {
    var1; // should be never (C2.constructor !== C1)
    var1.property1; // TS2339: does not exist on never
}
if (var1.constructor === C2) {
    var1; // should be C2
    var1.property1; // OK
}
        ",
    );
    let ts2339_count = diagnostics.iter().filter(|d| d.0 == 2339).count();
    assert_eq!(
        ts2339_count, 1,
        "Constructor narrowing should produce exactly 1 TS2339 (on 'never' type). \
         C2 extends C1 but C2.constructor !== C1, so the C1 branch narrows to never. \
         Got: {diagnostics:?}"
    );
}

#[test]
fn test_constructor_narrowing_same_structure_different_classes() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C7 { property1!: number; }
class C8 { property1!: number; }

declare let x: C8 | string;
if (x.constructor === C7) {
    x; // should be never (C8.constructor !== C7, even with same structure)
}
if (x.constructor === C8) {
    x; // should be C8
    x.property1; // OK
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Constructor narrowing with same-structure classes: \
         x.constructor === C8 should narrow to C8 and allow property access. \
         Got: {diagnostics:?}"
    );
}

#[test]
fn test_constructor_identity_false_branch_keeps_original_union() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class A { a!: string; }
class B { b!: number; }

declare let x: A | B;
if (x.constructor !== A) {
    x; // A | B
    x.b; // TS2339: constructor inequality does not exclude A
}
        ",
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("A | B"))
        .collect();
    assert!(
        !ts2339.is_empty(),
        "Constructor identity inequality should keep the original union in \
         the false branch, matching tsc. Got: {diagnostics:?}"
    );
}

#[test]
fn test_user_declare_function_shadows_lib_function_no_false_ts2554() {
    // lib.dom.d.ts declares `function print(): void` (0 args).
    // A user `declare function print(s: string): void;` in a script file
    // should shadow the lib declaration, not merge as overloads.
    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
declare function print(s: string): void;
print('1');
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2554),
        "User declare function should shadow lib function; print('1') must not \
         produce TS2554 ('Expected 0 arguments'). Got: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2300_for_cross_module_default_import_alias() {
    // Reproduces allowImportClausesToMergeWithTypes.ts:
    // When file b.ts exports a value as default, and file a.ts imports it
    // with the same name as a local interface, no TS2300 should be emitted.
    // The import clause merges with the interface (type + value).
    let b_source = r#"
export const zzz = 123;
export default zzz;
"#;
    let a_source = r#"
export default interface zzz {
    x: string;
}
import zzz from "./a";
const x: zzz = { x: "" };
export { zzz as default };
"#;
    let diagnostics = compile_two_files_get_diagnostics_with_options(
        b_source,
        a_source,
        "./a",
        CheckerOptions {
            no_lib: true,
            ..Default::default()
        },
    );
    let ts2300_count = diagnostics.iter().filter(|(c, _)| *c == 2300).count();
    assert_eq!(
        ts2300_count, 0,
        "External module files should not emit false TS2300 for cross-file \
         default import aliases. Got: {diagnostics:?}"
    );
}
