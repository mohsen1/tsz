/// Class methods with overloads that reference class type parameters in their
/// return types should not produce false TS2394 errors. The manual signature
/// lowering used for overload compatibility doesn't have class type params in
/// scope, producing Error types in the lowered signature. The checker must
/// detect these buried Error types and fall back to `get_type_of_node`.
#[test]
fn test_class_method_overload_with_class_type_param_in_return_no_false_ts2394() {
    // Simple case: class type param T directly in return type
    let d = compile_and_get_diagnostics(
        r#"
interface Vector<T> { _brand: T }
class Foo<T> {
    test(): Vector<T>;
    test(): Vector<any> {
        return undefined as any;
    }
}
"#,
    );
    let ts2394_count = d.iter().filter(|(c, _)| *c == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Vector<T> should be compatible with Vector<any> in overload check, got: {d:?}"
    );

    // Complex case: class type param in conditional type (Exclude) within tuple return
    let d = compile_and_get_diagnostics(
        r#"
type Exclude2<T, U> = T extends U ? never : T;
interface Seq<T> { tail(): Opt<Seq<T>>; }
class Opt<T> { toVector(): Vector<T> { return undefined as any; } }
class Vector<T> implements Seq<T> {
    tail(): Opt<Vector<T>> { return undefined as any; }
    partition2<U extends T>(predicate:(v:T)=>v is U): [Vector<U>,Vector<Exclude2<T, U>>];
    partition2(predicate:(x:T)=>boolean): [Vector<T>,Vector<T>];
    partition2<U extends T>(predicate:(v:T)=>boolean): [Vector<U>,Vector<any>] {
        return undefined as any;
    }
}
"#,
    );
    let ts2394_count = d.iter().filter(|(c, _)| *c == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Overload with Exclude<T,U> return should be compatible with Vector<any>, got: {d:?}"
    );

    // Case with deferred conditional type in return
    let d = compile_and_get_diagnostics(
        r#"
type MyCond<T> = T extends string ? number : boolean;
interface Vector<T> { _brand: T }
class Foo<T> {
    test(): Vector<MyCond<T>>;
    test(): Vector<any> {
        return undefined as any;
    }
}
"#,
    );
    let ts2394_count = d.iter().filter(|(c, _)| *c == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Vector<MyCond<T>> should be compatible with Vector<any> in overload check, got: {d:?}"
    );
}

#[test]
fn test_generic_identity_callback_arg_emits_ts2345() {
    // When a generic function like `<T>(value: T) => T` is passed as an argument
    // to a parameter expecting a non-generic callback, the display target must use
    // the TARGET's return type (not the source's instantiated return) so the
    // re-check in check_argument_assignable_or_report doesn't suppress the error.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function identity<T>(value: T): T;
declare function take(cb: (value: string | number) => boolean): void;
take(identity);
"#,
    );
    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for generic identity not assignable to callback, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_identity_callback_valid_case_no_error() {
    // When the generic callback IS compatible, no error should be emitted.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function identity<T>(value: T): T;
declare function take(cb: (value: string) => string): void;
take(identity);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should NOT emit TS2345 when generic callback is compatible, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_rest_arguments_iife_emits_ts8029() {
    let source = r#"
self.importScripts = (function (importScripts) {
    /**
     * @param {...unknown} rest
     */
    return function () {
        return importScripts.apply(this, arguments);
    };
})(importScripts);
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "index.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8029 = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 8029)
        .expect("expected TS8029");
    assert!(
        ts8029
            .message_text
            .contains("It would match 'arguments' if it had an array type."),
        "Expected TS8029 to mention implicit arguments-array matching. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic.code != 8024),
        "Expected the implicit-arguments case to upgrade to TS8029 instead of TS8024. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_intersection_with_primitive_preserves_property_conflict() {
    // Regression test for commonTypeIntersection conformance failure.
    // { __typename?: 'TypeTwo' } & string should NOT be assignable to
    // { __typename?: 'TypeOne' } & string because the __typename properties conflict.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare let x1: { __typename?: 'TypeTwo' } & { a: boolean };
let y1: { __typename?: 'TypeOne' } & { a: boolean } = x1;

declare let x2: { __typename?: 'TypeTwo' } & string;
let y2: { __typename?: 'TypeOne' } & string = x2;
"#,
    );
    let ts2322_count = diagnostics.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 errors (one for each incompatible intersection assignment), got {ts2322_count}.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2395_still_fires_for_non_ambient_mixed_export_declarations() {
    // TS2395 should still fire when non-ambient declarations mix exported and non-exported
    let diagnostics = compile_and_get_diagnostics(
        r#"
export namespace Foo {
    export function bar(): void;
}
namespace Foo {
    export function bar(): void;
}
"#,
    );
    let ts2395 = diagnostics.iter().filter(|(c, _)| *c == 2395).count();
    assert!(
        ts2395 > 0,
        "Expected TS2395 for mixed export declarations in non-ambient context. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2395_suppressed_in_declare_namespace_mixed_export_merge() {
    // complexRecursiveCollections.ts pattern: inside `declare namespace`, a merged symbol
    // may have both exported and non-exported declarations (e.g., namespace + function + interface).
    // tsc does NOT emit TS2395 for ambient declarations.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare namespace Immutable {
    export namespace List {
        function isList(maybeList: any): boolean;
        function of<T>(...values: Array<T>): List<T>;
    }
    export function List(): List<any>;
    export function List<T>(): List<T>;
    export interface List<T> {
        set(index: number, value: T): List<T>;
    }
}
"#,
    );
    let ts2395 = diagnostics.iter().filter(|(c, _)| *c == 2395).count();
    assert_eq!(
        ts2395, 0,
        "Should not emit TS2395 in declare namespace context. Got: {diagnostics:#?}"
    );
}

/// Object literal union normalization must include empty objects.
///
/// When computing the best common type of `[{ a: 1, b: 2 }, { a: "abc" }, {}]`,
/// tsc normalizes the `{}` member to `{ a?: undefined; b?: undefined }` so that
/// property access on the resulting union works without TS2339. Previously, tsz
/// skipped empty objects during normalization, causing false TS2339 errors after
/// freshness was stripped for variable declarations.
#[test]
fn test_object_literal_normalization_empty_object_no_ts2339() {
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
let a2 = [{ a: 1, b: 2 }, { a: "abc" }, {}][0];
a2.a;
a2.b;
"#,
        options,
    );
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Should not emit TS2339 for property access on normalized object literal union with empty object member. Got: {ts2339:#?}"
    );
}

#[test]
fn test_implement_array_interface_ts2416_not_ts2420() {
    // tsc emits TS2416 for the 'every' property because the class declares
    // a single-overload 'every' that is incompatible with Array's overloaded
    // 'every' (which includes a type-predicate overload).
    // We should NOT emit TS2420 (class incorrectly implements interface) at
    // the class level — only TS2416 at the specific property.
    let source = r#"
declare class MyArray<T> implements Array<T> {
    toString(): string;
    toLocaleString(): string;
    concat<U extends T[]>(...items: U[]): T[];
    concat(...items: T[]): T[];
    join(separator?: string): string;
    pop(): T;
    push(...items: T[]): number;
    reverse(): T[];
    shift(): T;
    slice(start?: number, end?: number): T[];
    sort(compareFn?: (a: T, b: T) => number): this;
    splice(start: number): T[];
    splice(start: number, deleteCount: number, ...items: T[]): T[];
    unshift(...items: T[]): number;

    indexOf(searchElement: T, fromIndex?: number): number;
    lastIndexOf(searchElement: T, fromIndex?: number): number;
    every(callbackfn: (value: T, index: number, array: T[]) => boolean, thisArg?: any): boolean;
    some(callbackfn: (value: T, index: number, array: T[]) => boolean, thisArg?: any): boolean;
    forEach(callbackfn: (value: T, index: number, array: T[]) => void, thisArg?: any): void;
    map<U>(callbackfn: (value: T, index: number, array: T[]) => U, thisArg?: any): U[];
    filter(callbackfn: (value: T, index: number, array: T[]) => boolean, thisArg?: any): T[];
    reduce(callbackfn: (previousValue: T, currentValue: T, currentIndex: number, array: T[]) => T, initialValue?: T): T;
    reduce<U>(callbackfn: (previousValue: U, currentValue: T, currentIndex: number, array: T[]) => U, initialValue: U): U;
    reduceRight(callbackfn: (previousValue: T, currentValue: T, currentIndex: number, array: T[]) => T, initialValue?: T): T;
    reduceRight<U>(callbackfn: (previousValue: U, currentValue: T, currentIndex: number, array: T[]) => U, initialValue: U): U;

    length: number;

    [n: number]: T;
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);

    let ts2416 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2416)
        .collect::<Vec<_>>();
    let ts2420 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2420)
        .collect::<Vec<_>>();

    assert!(
        !ts2416.is_empty(),
        "Expected TS2416 for 'every' property incompatibility. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2416.iter().any(|(_, message)| {
            message.contains("Property 'every'") && message.contains("base type 'T[]'")
        }),
        "Expected TS2416 to display the global Array<T> target as 'T[]'. Actual diagnostics: {ts2416:#?}"
    );
    assert!(
        ts2420.is_empty(),
        "Should NOT emit TS2420 for this case. Only TS2416 expected. Got TS2420: {ts2420:#?}"
    );
}

#[test]
fn test_class_implements_public_dynamic_name_class_shape_no_ts2720() {
    let source = r#"
const c0 = "a";
const c1 = 1;
const s0 = Symbol();

declare class T1 {
    [c0]: number;
    [c1]: string;
    [s0]: boolean;
}
declare class T2 extends T1 {
}

const c4 = "a";
const c5 = 1;
const s2: typeof s0 = s0;

declare class T13 implements T2 {
    a: number;
    1: string;
    [s2]: boolean;
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2720 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2720)
        .collect::<Vec<_>>();
    assert!(
        ts2720.is_empty(),
        "Expected no TS2720 for public dynamic-name class shape. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_implements_interface_member_mismatch_prefers_ts2416() {
    let source = r#"
interface FileSystem {
  read: number;
}

class WorkerFS implements FileSystem {
  read: string;
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2416 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2416)
        .collect::<Vec<_>>();
    let ts2420 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2420)
        .collect::<Vec<_>>();
    assert!(
        !ts2416.is_empty(),
        "Expected TS2416 for implemented member type mismatch. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2420.is_empty(),
        "Expected member-level TS2416 to suppress broad TS2420. Actual TS2420 diagnostics: {ts2420:#?}"
    );
}

#[test]
fn test_generic_array_extension_global_array_display_uses_shorthand() {
    let source = r#"
export declare class ObservableArray<T> implements Array<T> {
    concat<U extends T[]>(...items: U[]): T[];
    concat(...items: T[]): T[];
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    let ts2420 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2420)
        .collect::<Vec<_>>();

    assert!(
        ts2420
            .iter()
            .any(|(_, message)| message.contains("interface 'T[]'")),
        "Expected global Array<T> implements diagnostic to display interface 'T[]'. Actual TS2420 diagnostics: {ts2420:#?}"
    );
}

#[test]
fn test_module_local_array_interface_in_implements_shadows_global_array() {
    let source = r#"
export {};

interface Array<T> {
    custom: T;
}

class Box implements Array<number> {
    custom = 1;
}

new Box().custom.toFixed();
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);

    assert!(
        !has_error(&diagnostics, 2420),
        "Expected module-local Array<number> to be checked instead of global number[]. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Expected Box.custom to remain visible after the implements check. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_module_local_array_interface_missing_member_uses_local_display() {
    let source = r#"
export {};

interface Array<T> {
    custom: T;
}

class Box implements Array<number> {}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    let ts2420 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2420)
        .expect("Expected TS2420 for missing local Array.custom member");

    assert!(
        ts2420.1.contains("interface 'Array<number>'"),
        "Expected local interface display name in TS2420. Actual diagnostic: {ts2420:#?}"
    );
    assert!(
        ts2420.1.contains("custom"),
        "Expected TS2420 to mention the local Array.custom member. Actual diagnostic: {ts2420:#?}"
    );
    assert!(
        !ts2420.1.contains("number[]"),
        "Did not expect global array display name for module-local Array. Actual diagnostic: {ts2420:#?}"
    );
}

#[test]
fn test_module_local_array_type_alias_shadows_global_array_reference() {
    let source = r#"
export {};

type Array<T> = {
    custom: T;
};

declare const value: Array<number>;

value.custom.toFixed();
value.length;
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert!(
        ts2339
            .iter()
            .any(|(_, message)| message.contains("length")
                && message.contains("type 'Array<number>'")),
        "Expected local Array<number> to reject only value.length. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .all(|(_, message)| !message.contains("custom")),
        "Expected value.custom to resolve through the local Array alias. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_module_local_array_alias_shadows_type_literal_member_reference() {
    let source = r#"
export {};

type Array<T> = {
    custom: T;
};

type ReadonlyArray<T> = {
    readonlyCustom: T;
};

type Box = {
    value: Array<number>;
    readonlyValue: ReadonlyArray<number>;
};

declare const box: Box;

box.value.custom.toFixed();
box.value.length;
box.readonlyValue.readonlyCustom.toFixed();
box.readonlyValue.length;
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert!(
        ts2339
            .iter()
            .any(|(_, message)| message.contains("length")
                && message.contains("type 'Array<number>'")),
        "Expected local Array<number> in type literal member to reject box.value.length. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339.len() == 2 && ts2339.iter().all(|(_, message)| message.contains("length")),
        "Expected local ReadonlyArray<number> in type literal member to reject box.readonlyValue.length. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .all(|(_, message)| !message.contains("custom")),
        "Expected local Array and ReadonlyArray members to remain visible in type literals. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_module_local_array_alias_shadows_function_type_reference() {
    let source = r#"
export {};

type Array<T> = {
    custom: T;
};

type ReadonlyArray<T> = {
    readonlyCustom: T;
};

type Fn = (value: Array<string>) => Array<string>;
type ReadonlyFn = (value: ReadonlyArray<string>) => ReadonlyArray<string>;
declare const fn: Fn;
declare const readonlyFn: ReadonlyFn;

const result = fn({ custom: "ok" });
const readonlyResult = readonlyFn({ readonlyCustom: "ok" });
result.custom.toUpperCase();
result.length;
readonlyResult.readonlyCustom.toUpperCase();
readonlyResult.length;
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert!(
        !has_error(&diagnostics, 2345),
        "Expected function parameters to use local Array aliases, not builtin arrays. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|(_, message)| message.contains("length")
                && message.contains("type 'Array<string>'")),
        "Expected local Array<string> function return type to reject result.length. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .all(|(_, message)| !message.contains("custom")),
        "Expected result.custom and readonlyResult.readonlyCustom to resolve through local aliases. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Regression test: when an object literal property uses `this` as the value
/// and the `this` type is not assignable to the expected property type, the
/// elaboration should emit TS2322 ("Type X is not assignable to type Y"),
/// not TS2741 ("Property X is missing..."). This matches tsc behavior in
/// `elaborateElementwise` for `this` keyword expressions.
///
/// Based on conformance test `fuzzy.ts`.
#[test]
fn test_elaboration_emits_ts2322_not_ts2741_for_this_keyword_property() {
    let source = r#"
interface I {
    a(): void;
    b(): void;
}
interface R {
    oneI: I;
}
class C implements I {
    a(): void {}
    doesntWork(): R {
        return { oneI: this };
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        has_error(&diagnostics, 2420),
        "Expected TS2420 for class incorrectly implementing interface. Got: {diagnostics:#?}"
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for property type mismatch in object literal elaboration. Got: {diagnostics:#?}"
    );

    let ts2741 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2741)
        .collect::<Vec<_>>();
    assert!(
        ts2741.is_empty(),
        "Should NOT emit TS2741 for `this` keyword in elaboration — tsc uses TS2322. Got: {ts2741:#?}"
    );
}

/// Regression test: var redeclaration with generic call whose argument is incompatible
/// should emit TS2403 when the call infers a different return type than the prior declaration.
///
/// tsc does NOT propagate prior var declaration types as contextual type for call expressions.
/// Without this fix, the contextual type `Function[]` from the first declaration would cause
/// `stringMapToArray(numberMap)` to infer `T=Function`, returning `Function[]` (matching the
/// prior declaration) and suppressing the TS2403 error. With the fix, `T` is correctly
/// inferred as `unknown` from the failed argument matching, returning `unknown[]`.
#[test]
fn test_ts2403_var_redecl_generic_call_no_contextual_from_prior_decl() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface NumberMap<T> {
    [index: number]: T;
}
interface StringMap<T> {
    [index: string]: T;
}
declare function stringMapToArray<T>(object: StringMap<T>): T[];
declare var numberMap: NumberMap<string>;
var v1: string[];
var v1 = stringMapToArray(numberMap);
"#,
    );
    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for NumberMap<string> not assignable to StringMap<unknown>. Got: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2403),
        "Expected TS2403 for var redeclaration type mismatch (string[] vs unknown[]). Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2403_optional_param_var_redeclaration_in_constructor() {
    let source = r#"
class C {
    constructor(options?: number) {
        var options = (options || 0);
    }
}
"#;
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2403),
        "Expected TS2403 for var re-declaring optional parameter with different type.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2460_no_false_positive_when_renamed_and_star_reexport_both_present() {
    // When a module does both `export { X as Y }` and `export * from "..."` (which
    // re-exports X), both Y and X are valid imports. tsc accepts this; tsz must not
    // emit TS2460 for X just because it was also exported as Y.
    let files = [
        (
            "/source.ts",
            r#"
export interface User {
  id: number;
}
"#,
        ),
        (
            "/middle.ts",
            r#"
import { User } from "./source";
export { User as IUser };
export * from "./source";
"#,
        ),
        (
            "/test.ts",
            r#"
import { IUser, User } from "./middle";
const u1: User = { id: 1 };
const u2: IUser = u1;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/test.ts",
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2460),
        "Expected no TS2460 when symbol is also available via star re-export.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2460_no_false_positive_when_direct_export_also_renamed() {
    // When the declaration is exported under its own name and also re-exported
    // under an alias, both names are valid import targets. TS2460 is only for
    // declarations that are local-only except for the renamed export.
    let files = [
        (
            "/main.ts",
            r#"
export const namedConst = 42;
export interface User {
  id: number;
}
export { namedConst as renamedConst, User as Person };
"#,
        ),
        (
            "/consumer.ts",
            r#"
import { namedConst, renamedConst, User, Person } from "./main";

const n: number = namedConst;
const r: number = renamedConst;
const u: User = { id: 1 };
const p: Person = u;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/consumer.ts",
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2460),
        "Expected no TS2460 when a declaration is directly exported and also renamed.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2460_still_fires_when_only_renamed_export_no_star_reexport() {
    // Sanity-check: when a module ONLY renames the export (no star re-export that
    // brings the original name back), importing the original name must still be TS2460.
    let files = [
        (
            "/source.ts",
            r#"
export interface Widget {
  id: number;
}
"#,
        ),
        (
            "/middle.ts",
            r#"
import { Widget } from "./source";
export { Widget as IWidget };
"#,
        ),
        (
            "/test.ts",
            r#"
import { Widget } from "./middle";
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/test.ts",
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2460),
        "Expected TS2460 when symbol is only exported under a renamed alias.\nActual: {diagnostics:#?}"
    );
}
