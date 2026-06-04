#[test]
fn declaration_emit_aliased_transitive_import_reports_single_canonical_ts2883() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("r/node_modules/foo/node_modules/nested/index.d.ts"),
        "export interface NestedProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/index.d.ts"),
        r#"import { NestedProps as NP } from "nested";
export function foo(): [NP];
"#,
    );
    write_file(
        &base.join("r/entry.ts"),
        r#"import { foo } from "foo";
export const x = foo();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["r/entry.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected one TS2883 diagnostic for aliased nested tuple return reference, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages[0].contains("NestedProps"),
        "expected TS2883 to name canonical NestedProps, got: {}",
        ts2883_messages[0]
    );
    assert!(
        !ts2883_messages[0].contains("'NP'"),
        "did not expect TS2883 to name local alias NP, got: {}",
        ts2883_messages[0]
    );

    if let Ok(dts) = fs::read_to_string(base.join("r/entry.d.ts")) {
        assert!(
            !dts.contains("[NP]"),
            "did not expect declaration output to reference unbound alias NP: {dts}"
        );
    }
}

#[test]
fn declaration_emit_reports_non_serializable_foreign_unique_symbol_property() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("a.d.ts"),
        r#"export declare const timestampSymbol: unique symbol;

export declare const Timestamp: {
    [TKey in typeof timestampSymbol]: true;
};

export declare function now(): typeof Timestamp;
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import * as x from "./a";
export const timestamp = x.now();
"#,
    );
    write_file(
        &base.join("c.ts"),
        r#"import { now } from "./a";

export const timestamp = now();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "declaration": true
  },
  "files": ["b.ts", "c.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts4118_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 4118)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts4118_messages.len(),
        2,
        "expected two TS4118 diagnostics, got: {ts4118_messages:#?}"
    );
    assert!(
        ts4118_messages
            .iter()
            .all(|message| { message.contains("[timestampSymbol]") }),
        "expected TS4118 to mention [timestampSymbol], got: {ts4118_messages:#?}"
    );
}

#[test]
fn declaration_emit_reports_ts7056_for_private_import_type_alias() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("http-client.ts"),
        r#"type TPromise<ResolveType, RejectType = any> = Omit<Promise<ResolveType>, "then" | "catch"> & {
    then<TResult1 = ResolveType, TResult2 = never>(
        onfulfilled?: ((value: ResolveType) => TResult1 | PromiseLike<TResult1>) | undefined | null,
        onrejected?: ((reason: RejectType) => TResult2 | PromiseLike<TResult2>) | undefined | null,
    ): TPromise<TResult1 | TResult2, RejectType>;
    catch<TResult = never>(
        onrejected?: ((reason: RejectType) => TResult | PromiseLike<TResult>) | undefined | null,
    ): TPromise<ResolveType | TResult, RejectType>;
};

export interface HttpResponse<D extends unknown, E extends unknown = unknown> extends Response {
    data: D;
    error: E;
}

export class HttpClient<SecurityDataType = unknown> {
    public request = <T = any, E = any>(): TPromise<HttpResponse<T, E>> => {
        return '' as any;
    };
}
"#,
    );
    write_file(
        &base.join("Api.ts"),
        r#"import { HttpClient } from "./http-client";

export class Api<SecurityDataType = unknown> {
    constructor(private http: HttpClient<SecurityDataType>) { }

    abc1 = () => this.http.request();
    abc2 = () => this.http.request();
    abc3 = () => this.http.request();
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "commonjs",
    "skipLibCheck": true
  },
  "files": ["http-client.ts", "Api.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts7056: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 7056)
        .collect();

    assert_eq!(
        ts7056.len(),
        3,
        "expected TS7056 on all inferred Api property declarations, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_huge_declaration_output_truncation_skips_dts_but_keeps_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("test.ts"),
        r#"type props = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z";

type manyprops = `${props}${props}`;

export const c = [null as any as {[K in manyprops]: {[K2 in manyprops]: `${K}.${K2}`}}][0];
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["test.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts7056: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 7056)
        .collect();

    assert_eq!(
        ts7056.len(),
        1,
        "expected one TS7056, got: {:#?}",
        result.diagnostics
    );
    assert!(
        base.join("test.js").exists(),
        "expected JS emit to continue after TS7056"
    );
    assert!(
        !base.join("test.d.ts").exists(),
        "expected declaration emit to skip test.d.ts when TS7056 is reported"
    );
    assert!(
        result
            .emitted_files
            .iter()
            .all(|path| path.file_name().and_then(|name| name.to_str()) != Some("test.d.ts")),
        "test.d.ts should not be reported as emitted: {:#?}",
        result.emitted_files
    );
}

#[test]
fn declaration_emit_imported_function_alias_avoids_ts7056() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("a.ts"),
        r#"type O = {
    prop: string
    prop2: string
}

type I = {
    prop: string
}

export const fn = (v: O['prop'], p: Omit<O, 'prop'>, key: keyof O, p2: Omit<O, keyof I>) => {};
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import { fn } from "./a";

export const f = fn;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().all(|d| d.code != 7056),
        "expected no TS7056 for imported function alias emit, got: {:#?}",
        result.diagnostics
    );
    assert!(
        base.join("b.d.ts").exists(),
        "expected declaration emit to keep b.d.ts when no TS7056 is needed"
    );
}

#[test]
fn declaration_emit_duplicate_var_reuses_first_declaration_type() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"namespace M {
    export namespace C {
        export function f() {}
    }
    export namespace E {
        export function f() {}
    }
}

namespace M.P {
    export var x = M.C.f;
    export var x = M.E.f;
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    compile(&args, base).expect("compile should succeed");
    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    let first = dts
        .find("var x: typeof M.C.f;")
        .expect("first duplicate x should use M.C.f");
    let second = dts[first + 1..]
        .find("var x: typeof M.C.f;")
        .expect("second duplicate x should reuse first declaration type");
    assert!(
        second > 0 && !dts.contains("var x: typeof M.E.f;"),
        "expected duplicate var declarations to share first type: {dts}"
    );
}

#[test]
fn declaration_emit_typeof_value_reference_uses_relative_namespace_path() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"namespace X.Y.base {
    export function f() {}
    export class C {}
    export namespace M {
        export var v;
    }
    export enum E {}
}

namespace X.Y.base.Z {
    export var f = X.Y.base.f;
    export var C = X.Y.base.C;
    export var M = X.Y.base.M;
    export var E = X.Y.base.E;
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": false,
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    compile(&args, base).expect("compile should succeed");
    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains("var f: typeof base.f;")
            && dts.contains("var C: typeof base.C;")
            && dts.contains("var M: typeof base.M;")
            && dts.contains("var E: typeof base.E;"),
        "expected namespace-relative typeof references: {dts}"
    );
    assert!(
        !dts.contains("typeof X.Y.base."),
        "did not expect fully qualified references inside nested namespace: {dts}"
    );
}

#[test]
fn declaration_emit_type_reference_typeof_uses_referenced_global_value_without_import() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(&base.join("ref.d.ts"), "export interface $ { x: any }\n");
    write_file(
        &base.join("types/lib/index.d.ts"),
        "declare let $: { x: number }\n",
    );
    write_file(
        &base.join("app.ts"),
        r#"/// <reference types="lib"/>
import {$} from "./ref";

export interface A {
    x: typeof $;
    y: () => typeof $;
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "declaration": true,
    "emitDeclarationOnly": true,
    "typeRoots": ["./types"],
    "types": ["lib"]
  },
  "files": ["app.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    compile(&args, base).expect("compile should succeed");
    let dts = fs::read_to_string(base.join("app.d.ts")).expect("read app.d.ts");
    assert!(
        !dts.contains("import { $ } from \"./ref\";"),
        "interface-only import should not be preserved for typeof satisfied by referenced global: {dts}"
    );
    assert!(
        dts.contains("x: typeof $;") && dts.contains("y: () => typeof $;"),
        "expected typeof references to be emitted unchanged: {dts}"
    );
}

#[test]
fn declaration_emit_type_reference_typeof_keeps_imported_value() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(&base.join("ref.ts"), "export const $ = { x: 1 };\n");
    write_file(
        &base.join("app.ts"),
        r#"import {$} from "./ref";

export interface A {
    x: typeof $;
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "declaration": true,
    "emitDeclarationOnly": true
  },
  "files": ["app.ts", "ref.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    compile(&args, base).expect("compile should succeed");
    let dts = fs::read_to_string(base.join("app.d.ts")).expect("read app.d.ts");
    assert!(
        dts.contains("import { $ } from \"./ref\";"),
        "value import used by typeof should be preserved: {dts}"
    );
    assert!(
        dts.contains("x: typeof $;"),
        "expected typeof reference: {dts}"
    );
}

#[test]
fn declaration_emit_local_annotation_alias_preserves_class_instance_references() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"namespace m {
    class private1 {}
    namespace m2 {
        export class public1 {}
    }

    export var x: {
        x: private1;
        y: m2.public1;
        (): m2.public1[];
        method(): private1;
        [n: number]: private1;
        [s: string]: m2.public1;
    };
    export var x3 = x;

    export var y: (a: private1) => m2.public1;
    export var y2 = y;

    export var z: new (a: private1) => m2.public1;
    export var z2 = z;
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    compile(&args, base).expect("compile should succeed");
    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains("export var y2: (a: private1) => m2.public1;"),
        "expected inferred function alias to reuse source annotation: {dts}"
    );
    assert!(
        dts.contains("export var z2: new (a: private1) => m2.public1;"),
        "expected inferred constructor alias to reuse source annotation: {dts}"
    );
    assert!(
        dts.contains(
            "export var x3: {\n        (): m2.public1[];\n        [n: number]: private1;\n        [s: string]: m2.public1;\n        x: private1;"
        ),
        "expected inferred callable object alias to keep instance type refs and index order: {dts}"
    );
    assert!(
        !dts.contains("typeof private1") && !dts.contains("=> any"),
        "reused annotation should not degrade class instance refs: {dts}"
    );
}

#[test]
fn declaration_emit_imported_generic_call_preserves_function_type_argument() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("module.d.ts"),
        r#"declare module "module" {
    export interface Modifier<T> {}
    export function fn<T>(x: T): Modifier<T>;
}
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import { fn } from "module";

export const fail1 = fn(<T>(x: T): T => x);
export const works1 = fn((x: number) => x);
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "es2015"
  },
  "files": ["module.d.ts", "index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:#?}",
        result.diagnostics
    );

    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains("export declare const fail1: import(\"module\").Modifier<(<T>(x: T) => T)>;"),
        "expected inferred generic function type argument: {dts}"
    );
    assert!(
        dts.contains(
            "export declare const works1: import(\"module\").Modifier<(x: number) => number>;"
        ),
        "expected inferred arrow return type argument: {dts}"
    );
}

#[test]
fn declaration_emit_generic_call_preserves_class_expression_type_argument() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"declare const a: symbol;
export class A {
    [a]() { return 1; };
}
declare const e1: A[typeof a];

type Constructor = new (...args: any[]) => {};
declare function Mix<T extends Constructor>(classish: T): T & (new (...args: any[]) => {mixed: true});

export const Mixer = Mix(class {
    [a]() { return 1; };
});
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "es2015"
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:#?}",
        result.diagnostics
    );

    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains(
            "export declare const Mixer: {\n    new (): {\n        [a]: () => number;\n    };\n} & (new (...args: any[]) => {"
        ) && dts.contains("mixed: true;"),
        "expected inferred class-expression constructor type to survive generic substitution: {dts}"
    );
}

#[test]
fn declaration_emit_inferred_array_return_preserves_explicit_new_type_arguments() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"export class Box<T> {}
export namespace ns {
    export class Box<T> {}
}
export function local() {
    return [new Box<string>()];
}
export function qualified() {
    return [new ns.Box<number>()];
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "es2015"
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:#?}",
        result.diagnostics
    );

    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains("export declare function local(): Box<string>[];"),
        "expected local class type arguments in inferred array return: {dts}"
    );
    assert!(
        dts.contains("export declare function qualified(): ns.Box<number>[];"),
        "expected qualified class type arguments in inferred array return: {dts}"
    );
}

#[test]
fn declaration_emit_spread_stringly_keyed_enum_preserves_member_order() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"enum AgeGroups {
    "0-17",
    "18-22",
    "23-27",
    "28-34",
    "35-44",
    "45-59",
    "60-150",
}

export const SpotifyAgeGroupEnum = { ...AgeGroups };
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "es2015"
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:#?}",
        result.diagnostics
    );

    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    let expected = r#"export declare const SpotifyAgeGroupEnum: {
    [x: number]: string;
    "0-17": (typeof AgeGroups)["0-17"];
    "18-22": (typeof AgeGroups)["18-22"];
    "23-27": (typeof AgeGroups)["23-27"];
    "28-34": (typeof AgeGroups)["28-34"];
    "35-44": (typeof AgeGroups)["35-44"];
    "45-59": (typeof AgeGroups)["45-59"];
    "60-150": (typeof AgeGroups)["60-150"];
};"#;
    assert!(
        dts.contains(expected),
        "expected spread enum members to follow declaration order: {dts}"
    );
}

#[test]
fn declaration_emit_single_object_spread_projects_falsy_operand_types() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        r#"function f1<T>(a: T & undefined) {
    return { ...a };
}
function f2<T>(a: T | T & undefined) {
    return { ...a };
}
function f3<T extends undefined>(a: T) {
    return { ...a };
}
function f4<T extends undefined>(a: object | T) {
    return { ...a };
}
function f5<S, T extends undefined>(a: S | T) {
    return { ...a };
}
function f6<T extends object | undefined>(a: T) {
    return { ...a };
}
function g1<Q extends {}, A extends { z: (Q | undefined) & Q }>(a: A) {
    const { z } = a;
    return { ...z };
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let _ = compile(&args, base).expect("compile should succeed");
    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains("declare function f1<T>(a: T & undefined): any;"),
        "invalid definitely-falsy spread should emit any: {dts}"
    );
    assert!(
        dts.contains("declare function f2<T>(a: T | T & undefined): T | (T & undefined);"),
        "generic union with a definitely-falsy arm should preserve source spelling: {dts}"
    );
    assert!(
        dts.contains("declare function f3<T extends undefined>(a: T): any;"),
        "type parameter constrained to undefined should emit any: {dts}"
    );
    assert!(
        dts.contains("declare function f4<T extends undefined>(a: object | T): {};"),
        "object plus definitely-falsy arm should emit empty object: {dts}"
    );
    assert!(
        dts.contains("declare function f5<S, T extends undefined>(a: S | T): S | T;")
            && dts.contains("declare function f6<T extends object | undefined>(a: T): T;"),
        "generic spread operands should remain nameable: {dts}"
    );
    assert!(
        dts.contains("declare function g1<Q extends {}, A extends {\n    z: (Q | undefined) & Q;\n}>(a: A): Q;"),
        "destructured local spreads should keep solver-inferred generic return: {dts}"
    );
}

#[test]
fn declaration_emit_import_equals_alias_to_merged_namespace_value_allows_property_access() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("translation.ts"),
        r#"export interface Translation {
  translationKey: Translation.TranslationKeyEnum;
}

export namespace Translation {
  export type TranslationKeyEnum = 'translation1' | 'translation2';
  export const TranslationKeyEnum = {
    Translation1: 'translation1' as TranslationKeyEnum,
    Translation2: 'translation2' as TranslationKeyEnum,
  }
}
"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"import { Translation } from "./translation";
import TranslationKeyEnum = Translation.TranslationKeyEnum;

export class Test {
  TranslationKeyEnum = TranslationKeyEnum;
  print() {
    console.log(TranslationKeyEnum.Translation1);
  }
}
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import { Test } from "./test";
new Test().print();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "strict": true,
    "declaration": true
  },
  "files": ["translation.ts", "test.ts", "index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn declaration_emit_namespace_import_callable_member_avoids_ts7056() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("a.ts"),
        r#"export type SpecialString = string;
type PrivateSpecialString = string;

export namespace N {
    export type SpecialString = string;
}

export const o = (
    p1: SpecialString,
    p2: PrivateSpecialString,
    p3: N.SpecialString,
) => null! as { foo: SpecialString, bar: PrivateSpecialString, baz: N.SpecialString };
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import * as a from "./a";

export const g = a.o;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2020",
    "strict": true,
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().all(|d| d.code != 7056),
        "expected no TS7056 for namespace-import callable alias emit, got: {:#?}",
        result.diagnostics
    );

    let b_dts = fs::read_to_string(base.join("b.d.ts")).expect("b.d.ts should exist");
    assert!(
        b_dts.contains("typeof a.o"),
        "expected namespace import member reuse in b.d.ts, got:\n{b_dts}"
    );
}

#[test]
fn declaration_emit_reusable_local_property_names_avoid_ts7056() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("decl.ts"),
        r#"const u = "X";
type A = { a: { b : "value of b", notNecessary: typeof u }}
const a = { a: "value of a", notNecessary: u } as const

export const o1 = (o: A['a']['b']) => {}
export const o2 = (o: (typeof a)['a']) => {}
export const o3 = (o: typeof a['a']) => {}
export const o4 = (o: keyof (A['a'])) => {}
"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"import * as d from "./decl";

export const f = { ...d };
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "esnext",
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["decl.ts", "main.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().all(|d| d.code != 7056),
        "expected no TS7056 for reusable local property-name serialization, got: {:#?}",
        result.diagnostics
    );
    assert!(
        base.join("decl.d.ts").exists(),
        "expected declaration emit to keep decl.d.ts when no TS7056 is needed"
    );
}

#[test]
fn isolated_declaration_emit_does_not_cascade_ts2339_from_imported_generic_builder() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/@trpc/server/internals/config.d.ts"),
        r#"
export interface RootConfig<T> {
    prop: T;
}
"#,
    );
    write_file(
        &base.join("node_modules/@trpc/server/internals/utils.d.ts"),
        r#"
export interface ErrorFormatterShape<T={}> {
    prop: T;
}
export type PickFirstDefined<TType, TPick> = undefined extends TType
  ? undefined extends TPick
    ? never
    : TPick
  : TType;
export interface ErrorFormatter<T={},U={}> {
    prop: [T, U];
}
export interface DefaultErrorShape<T={}> {
    prop: T;
}
"#,
    );
    write_file(
        &base.join("node_modules/@trpc/server/middleware.d.ts"),
        r#"
export interface MiddlewareFunction<T={},U={}> {
    prop: [T, U];
}
export interface MiddlewareBuilder<T={},U={}> {
    prop: [T, U];
}
"#,
    );
    write_file(
        &base.join("node_modules/@trpc/server/index.d.ts"),
        r#"
import { RootConfig } from './internals/config';
import { ErrorFormatterShape, PickFirstDefined, ErrorFormatter, DefaultErrorShape } from './internals/utils';
declare class TRPCBuilder<TParams> {
    create<TOptions extends Record<string, any>>(): {
        procedure: {};
        middleware: <TNewParams extends Record<string, any>>(fn: import("./middleware").MiddlewareFunction<{
            _config: RootConfig<{
                errorShape: ErrorFormatterShape<PickFirstDefined<TOptions["errorFormatter"], ErrorFormatter<TParams["ctx"] extends object ? TParams["ctx"] : object, DefaultErrorShape>>>;
            }>;
        }, TNewParams>) => import("./middleware").MiddlewareBuilder<{
            _config: RootConfig<{
                errorShape: ErrorFormatterShape<PickFirstDefined<TOptions["errorFormatter"], ErrorFormatter<TParams["ctx"] extends object ? TParams["ctx"] : object, DefaultErrorShape>>>;
            }>;
        }, TNewParams>;
        router: {};
    };
}

export declare const initTRPC: TRPCBuilder<object>;
export {};
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
import { initTRPC } from "@trpc/server";

const trpc = initTRPC.create();

export const middleware = trpc.middleware;
export const router = trpc.router;
export const publicProcedure = trpc.procedure;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true,
    "isolatedDeclarations": true
  },
  "files": [
    "node_modules/@trpc/server/internals/config.d.ts",
    "node_modules/@trpc/server/internals/utils.d.ts",
    "node_modules/@trpc/server/middleware.d.ts",
    "node_modules/@trpc/server/index.d.ts",
    "index.ts"
  ]
}"#,
    );

    let project = base.to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("batch-style args");

    tsz_solver::construction::clear_thread_local_cache();
    tsz_solver::relations::subtype::reset_subtype_thread_local_state();
    tsz::checker::clear_all_thread_local_state();

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = compile(&args, &repo_root).expect("batch-style compile should succeed");
    let ts2339: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2339)
        .map(|diag| diag.message_text.clone())
        .collect();
    assert!(
        ts2339.is_empty(),
        "expected no cascading TS2339 from imported generic builder result, got: {ts2339:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diag| diag.code == 9010),
        "expected the isolated-declarations TS9010 diagnostic to remain, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn declaration_emit_transitive_react_styled_form_uses_public_import_surface() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    fs::create_dir_all(base.join("node_modules/react")).expect("react dir");
    fs::create_dir_all(base.join("node_modules/create-emotion-styled/types/react"))
        .expect("create-emotion-styled dir");
    fs::create_dir_all(base.join("node_modules/react-emotion")).expect("react-emotion dir");

    write_file(
        &base.join("node_modules/react/index.d.ts"),
        r#"declare namespace React {
    export interface DetailedHTMLProps<T, U> {}
    export interface HTMLAttributes<T> {}
}
export = React;
export as namespace React;
"#,
    );
    write_file(
        &base.join("node_modules/create-emotion-styled/types/react/index.d.ts"),
        r#"/// <reference types="react" />
declare module 'react' {
    interface HTMLAttributes<T> {
        css?: unknown;
    }
}
export interface StyledOtherComponentList {
    "div": React.DetailedHTMLProps<React.HTMLAttributes<HTMLDivElement>, HTMLDivElement>
}
export interface StyledOtherComponent<A, B, C> {}
"#,
    );
    write_file(
        &base.join("node_modules/create-emotion-styled/index.d.ts"),
        r#"export * from "./types/react";
"#,
    );
    write_file(
        &base.join("node_modules/react-emotion/index.d.ts"),
        r#"import {StyledOtherComponent, StyledOtherComponentList} from "create-emotion-styled";
export default function styled(tag: string): (o: object) => StyledOtherComponent<{}, StyledOtherComponentList["div"], any>;
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import styled from "react-emotion"

const Form = styled('div')({ color: "red" })

export default Form
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2300_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .map(|diagnostic| diagnostic.message_text.clone())
        .collect();

    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2883),
        "expected public import surface to avoid TS2883, got: {:#?}",
        result.diagnostics
    );
    assert!(
        ts2300_messages.is_empty(),
        "expected module augmentation interface merge to avoid TS2300, got: {ts2300_messages:#?}"
    );
    let dts = fs::read_to_string(base.join("index.d.ts")).expect("read index.d.ts");
    assert!(
        dts.contains(
            "declare const Form: import(\"create-emotion-styled\").StyledOtherComponent<{}, import(\"create-emotion-styled\").StyledOtherComponentList[\"div\"], any>;"
        ),
        "expected public styled import and indexed access argument: {dts}"
    );
}

#[test]
fn compile_project_namespace_import_qualified_type_sees_module_augmentation_exports() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/backbone/index.d.ts"),
        r#"declare namespace Backbone {
    interface Model<T extends object = any, TQuery = any, TOptions = any> {}
}
export = Backbone;
"#,
    );
    write_file(
        &base.join("node_modules/backbone-fetch-cache/index.d.ts"),
        r#"import * as Backbone from "backbone";
declare module "backbone" {
    interface ModelWithCache extends Backbone.Model<any, any, any> {
        cache: boolean;
    }
}
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import * as Backbone from "backbone";
import "backbone-fetch-cache";

let model: Backbone.ModelWithCache;
model.cache;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_NAMESPACE
                || d.code == diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER),
        "Expected namespace-import type position to see module augmentation members, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_project_export_equals_request_augmentation_avoids_ts2300() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/express/index.d.ts"),
        r#"declare namespace Express {
    export interface Request { }
    export interface Response { }
    export interface Application { }
}

declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request {
            get(name: string): string;
        }
        interface Response extends Express.Response {
            charset: string;
        }
        interface Application extends Express.Application {
            routes: any;
        }
        interface Express extends Application {
            createApplication(): Application;
        }
        interface RequestHandler {
            (req: Request, res: Response, next: Function): any;
        }
        export = e;
    }
}
"#,
    );
    write_file(
        &base.join("augmentation.ts"),
        r#"import * as e from "express";
declare module "express" {
    interface Request {
        id: number;
    }
}
"#,
    );
    write_file(
        &base.join("consumer.ts"),
        r#"import { Request } from "express";
import "./augmentation";

let x: Request;
const y = x.id;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true
  },
  "files": ["augmentation.ts", "consumer.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected export= module augmentation interface merge to avoid TS2300, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected augmented Request interface to expose id, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected TS2454 to remain the primary consumer error, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_project_ambient_import_equals_module_declaration_avoids_ts2300() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("server.d.ts"),
        r#"declare module "other" {
    export class C { }
}

declare module "server" {
    import events = require("other");

    namespace S {
        export var a: number;
    }

    export = S;
}
"#,
    );
    write_file(
        &base.join("client.ts"),
        r#"import { a } from "server";
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015"
  },
  "files": ["server.d.ts", "client.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected ambient external module import-equals declaration to avoid TS2300, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics for ambient import-equals external module declaration, got: {:?}",
        result.diagnostics
    );
}
