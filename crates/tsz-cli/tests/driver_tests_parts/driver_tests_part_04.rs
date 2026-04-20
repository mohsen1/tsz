#[test]
fn compile_missing_project_directory_returns_error() {
    // Test that specifying a non-existent project directory returns an error diagnostic
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.project = Some(PathBuf::from("nonexistent_project"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    assert!(
        !result.diagnostics.is_empty(),
        "Should have error diagnostic for missing project directory"
    );
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY,
        "Should have correct error code"
    );
}
#[test]
fn compile_missing_tsconfig_in_project_dir_returns_error() {
    // Test that a project directory without tsconfig.json returns an error diagnostic
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create project directory but no tsconfig.json
    std::fs::create_dir_all(base.join("myproject")).expect("create dir");
    write_file(&base.join("myproject/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("myproject"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    // Should have error diagnostic since there's no tsconfig.json
    assert!(
        !result.diagnostics.is_empty(),
        "Should have error diagnostic when tsconfig.json is missing in project dir"
    );
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY,
        "Should have correct error code"
    );
}
#[test]
fn compile_missing_tsconfig_uses_defaults() {
    // Test that compilation works without tsconfig.json using defaults
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("src/index.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    // Output should be next to source when no outDir specified
    assert!(base.join("src/index.js").is_file());
}
#[test]
fn compile_ambient_external_module_without_internal_import_declaration_fixture() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs"
          },
          "files": [
            "ambientExternalModuleWithoutInternalImportDeclaration_0.ts",
            "ambientExternalModuleWithoutInternalImportDeclaration_1.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("ambientExternalModuleWithoutInternalImportDeclaration_0.ts"),
        r#"declare module 'M' {
    namespace C {
        export var f: number;
    }
    class C {
        foo(): void;
    }
    export = C;
}"#,
    );
    write_file(
        &base.join("ambientExternalModuleWithoutInternalImportDeclaration_1.ts"),
        r#"/// <reference path='ambientExternalModuleWithoutInternalImportDeclaration_0.ts'/>
import A = require('M');
var c = new A();"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}
#[test]
fn compile_alias_on_merged_module_interface_fixture_reports_ts2708() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs"
          },
          "files": [
            "aliasOnMergedModuleInterface_0.ts",
            "aliasOnMergedModuleInterface_1.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("aliasOnMergedModuleInterface_0.ts"),
        r#"declare module "foo" {
    namespace B {
        export interface A {}
    }
    interface B {
        bar(name: string): B.A;
    }
    export = B;
}"#,
    );
    write_file(
        &base.join("aliasOnMergedModuleInterface_1.ts"),
        r#"/// <reference path='aliasOnMergedModuleInterface_0.ts' />
import foo = require("foo");
declare var z: foo;
z.bar("hello");
var x: foo.A = foo.bar("hello");"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2708),
        "Expected TS2708, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// E2E: Generic Utility Library Compilation
// =============================================================================
#[test]
fn compile_generic_utility_library_array_utils() {
    // Test compilation of generic array utility functions
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "strict": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Generic array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function map<T, U>(arr: T[], fn: (item: T, index: number) => U): U[] {
    const result: U[] = [];
    for (let i = 0; i < arr.length; i++) {
        result.push(fn(arr[i], i));
    }
    return result;
}

export function filter<T>(arr: T[], predicate: (item: T) => boolean): T[] {
    const result: T[] = [];
    for (const item of arr) {
        if (predicate(item)) {
            result.push(item);
        }
    }
    return result;
}

export function find<T>(arr: T[], predicate: (item: T) => boolean): T | undefined {
    for (const item of arr) {
        if (predicate(item)) {
            return item;
        }
    }
    return undefined;
}

export function reduce<T, U>(arr: T[], fn: (acc: U, item: T) => U, initial: U): U {
    let acc = initial;
    for (const item of arr) {
        acc = fn(acc, item);
    }
    return acc;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/array.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/array.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output has type annotations stripped
    let js = std::fs::read_to_string(base.join("dist/src/array.js")).expect("read js");
    assert!(!js.contains(": T[]"), "Type annotations should be stripped");
    assert!(!js.contains(": U[]"), "Type annotations should be stripped");
    assert!(js.contains("function map"), "Function should be present");
    assert!(js.contains("function filter"), "Function should be present");
    assert!(js.contains("function find"), "Function should be present");
    assert!(js.contains("function reduce"), "Function should be present");

    // Verify declarations preserve types
    let dts = std::fs::read_to_string(base.join("dist/src/array.d.ts")).expect("read dts");
    assert!(
        dts.contains("map<T, U>") || dts.contains("map<T,U>"),
        "Generic should be in declaration"
    );
    assert!(
        dts.contains("filter<T>"),
        "Generic should be in declaration"
    );
}
#[test]
fn compile_generic_utility_library_type_utilities() {
    // Test compilation with type-level utilities (conditional types, mapped types)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Type utilities with runtime helpers
    write_file(
        &base.join("src/types.ts"),
        r#"
// Note: Object, Readonly, Partial are provided by lib.d.ts

// Type-level utilities (erased at runtime)
export type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? Readonly<T[P]> : T[P];
};

export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? Partial<T[P]> : T[P];
};

export type Nullable<T> = T | null;

// Mapped type that uses index access (T[P])
export type ValueTypes<T> = {
    [P in keyof T]: T[P];
};

// Runtime function using these types
export function deepFreeze<T extends object>(obj: T): DeepReadonly<T> {
    Object.freeze(obj);
    for (const key of Object.keys(obj)) {
        const value = (obj as Record<string, unknown>)[key];
        if (typeof value === "object" && value !== null) {
            deepFreeze(value as object);
        }
    }
    return obj as DeepReadonly<T>;
}

export function isNonNull<T>(value: T | null | undefined): value is T {
    return value !== null && value !== undefined;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Compilation should have no diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/types.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/types.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output - type aliases should be completely erased
    let js = std::fs::read_to_string(base.join("dist/src/types.js")).expect("read js");
    assert!(!js.contains("DeepReadonly"), "Type alias should be erased");
    assert!(!js.contains("DeepPartial"), "Type alias should be erased");
    assert!(
        js.contains("function deepFreeze"),
        "Runtime function should be present"
    );
    assert!(
        js.contains("function isNonNull"),
        "Runtime function should be present"
    );

    // Verify declarations preserve type utilities
    let dts = std::fs::read_to_string(base.join("dist/src/types.d.ts")).expect("read dts");
    assert!(
        dts.contains("DeepReadonly"),
        "Type alias should be in declaration"
    );
    assert!(
        dts.contains("DeepPartial"),
        "Type alias should be in declaration"
    );
}
#[test]
fn compile_generic_utility_library_multi_file() {
    // Test multi-file generic utility library with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function first<T>(arr: T[]): T | undefined {
    return arr[0];
}

export function last<T>(arr: T[]): T | undefined {
    return arr[arr.length - 1];
}
"#,
    );

    // String utilities
    write_file(
        &base.join("src/string.ts"),
        r#"
export function capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
}

export function repeat(str: string, count: number): string {
    let result = "";
    for (let i = 0; i < count; i++) {
        result += str;
    }
    return result;
}
"#,
    );

    // Function utilities
    write_file(
        &base.join("src/function.ts"),
        r#"
export function identity<T>(value: T): T {
    return value;
}

export function constant<T>(value: T): () => T {
    return () => value;
}

export function noop(): void {}
"#,
    );

    // Main index re-exporting everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export { first, last } from "./array";
export { capitalize, repeat } from "./string";
export { identity, constant, noop } from "./function";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    // All JS files should exist
    assert!(base.join("dist/src/array.js").is_file());
    assert!(base.join("dist/src/string.js").is_file());
    assert!(base.join("dist/src/function.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    // All declaration files should exist
    assert!(base.join("dist/src/array.d.ts").is_file());
    assert!(base.join("dist/src/string.d.ts").is_file());
    assert!(base.join("dist/src/function.d.ts").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // All source maps should exist
    assert!(base.join("dist/src/array.js.map").is_file());
    assert!(base.join("dist/src/index.js.map").is_file());

    // Verify index re-exports
    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read index");
    assert!(
        index_js.contains("require") || index_js.contains("export"),
        "Index should have exports"
    );

    // Verify index declaration
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("first") && index_dts.contains("last"),
        "Index declaration should re-export array utils"
    );
}
#[test]
fn compile_generic_utility_library_with_constraints() {
    // Test generic functions with complex constraints
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/constrained.ts"),
        r#"
// Generic with extends constraint
export function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

// Generic with multiple constraints
export function setProperty<T extends object, K extends keyof T>(
    obj: T,
    key: K,
    value: T[K]
): T {
    obj[key] = value;
    return obj;
}

// Generic with default type parameter
export function createArray<T = string>(length: number, fill: T): T[] {
    const result: T[] = [];
    for (let i = 0; i < length; i++) {
        result.push(fill);
    }
    return result;
}

// Function overloads with generics
export function wrap<T>(value: T): T[];
export function wrap<T>(value: T, count: number): T[];
export function wrap<T>(value: T, count: number = 1): T[] {
    const result: T[] = [];
    for (let i = 0; i < count; i++) {
        result.push(value);
    }
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/constrained.js")).expect("read js");
    assert!(
        !js.contains("extends keyof"),
        "Constraints should be stripped"
    );
    assert!(
        !js.contains("extends object"),
        "Constraints should be stripped"
    );
    assert!(
        js.contains("function getProperty"),
        "Function should be present"
    );
    assert!(js.contains("function wrap"), "Function should be present");

    let dts = std::fs::read_to_string(base.join("dist/src/constrained.d.ts")).expect("read dts");
    // Check that generic functions are present in declaration
    assert!(
        dts.contains("getProperty"),
        "getProperty should be in declaration"
    );
    assert!(
        dts.contains("setProperty"),
        "setProperty should be in declaration"
    );
    assert!(
        dts.contains("createArray"),
        "createArray should be in declaration"
    );
    assert!(dts.contains("wrap"), "wrap should be in declaration");
}
#[test]
#[ignore] // TODO: generic utility library classes should compile without errors
fn compile_generic_utility_library_classes() {
    // Test generic utility classes
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/collections.ts"),
        r#"
export class Stack<T> {
    private items: T[] = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | undefined {
        return this.items.pop();
    }

    peek(): T | undefined {
        return this.items[this.items.length - 1];
    }

    get size(): number {
        return this.items.length;
    }

    isEmpty(): boolean {
        return this.items.length === 0;
    }
}

export class Queue<T> {
    private items: T[] = [];

    enqueue(item: T): void {
        this.items.push(item);
    }

    dequeue(): T | undefined {
        return this.items.shift();
    }

    front(): T | undefined {
        return this.items[0];
    }

    get size(): number {
        return this.items.length;
    }
}

export class Result<T, E> {
    private constructor(
        private readonly value: T | undefined,
        private readonly error: E | undefined,
        private readonly isOk: boolean
    ) {}

    static ok<T, E>(value: T): Result<T, E> {
        return new Result<T, E>(value, undefined, true);
    }

    static err<T, E>(error: E): Result<T, E> {
        return new Result<T, E>(undefined, error, false);
    }

    isSuccess(): boolean {
        return this.isOk;
    }

    getValue(): T | undefined {
        return this.value;
    }

    getError(): E | undefined {
        return this.error;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/collections.js")).expect("read js");
    assert!(js.contains("class Stack"), "Class should be present");
    assert!(js.contains("class Queue"), "Class should be present");
    assert!(js.contains("class Result"), "Class should be present");
    assert!(!js.contains("<T>"), "Generic parameters should be stripped");
    assert!(!js.contains("T[]"), "Type annotations should be stripped");
    assert!(
        !js.contains(": void"),
        "Return type annotations should be stripped"
    );

    let dts = std::fs::read_to_string(base.join("dist/src/collections.d.ts")).expect("read dts");
    assert!(
        dts.contains("Stack<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Queue<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Result<T, E>") || dts.contains("Result<T,E>"),
        "Generic class should be in declaration"
    );
}

// =============================================================================
// E2E: Module Re-exports
// =============================================================================
#[test]
fn compile_module_named_reexports() {
    // Test named re-exports: export { foo, bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { add, multiply, PI } from "./utils";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );
    assert!(base.join("dist/src/utils.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify index re-exports
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("add"), "add should be re-exported");
    assert!(
        index_dts.contains("multiply"),
        "multiply should be re-exported"
    );
    assert!(index_dts.contains("PI"), "PI should be re-exported");
}
#[test]
fn compile_module_renamed_reexports() {
    // Test renamed re-exports: export { foo as bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/internal.ts"),
        r#"
export function internalHelper(): string {
    return "helper";
}

export const internalValue = 42;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { internalHelper as helper, internalValue as value } from "./internal";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("helper"), "helper should be re-exported");
    assert!(index_dts.contains("value"), "value should be re-exported");
}
#[test]
fn compile_module_star_reexports() {
    // Test star re-exports: export * from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/math.ts"),
        r#"
export function sum(arr: number[]): number {
    let total = 0;
    for (const n of arr) {
        total += n;
    }
    return total;
}

export function average(arr: number[]): number {
    return sum(arr) / arr.length;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export * from "./math";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("sum") || index_dts.contains("*"),
        "sum should be re-exported or star export present"
    );
}
#[test]
fn compile_module_chained_reexports() {
    // Test chained re-exports: A re-exports from B which re-exports from C
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Level 3: core module
    write_file(
        &base.join("src/core.ts"),
        r#"
export function coreFunction(): string {
    return "core";
}

export const CORE_VERSION = "1.0.0";
"#,
    );

    // Level 2: intermediate module
    write_file(
        &base.join("src/intermediate.ts"),
        r#"
export { coreFunction, CORE_VERSION } from "./core";

export function intermediateFunction(): string {
    return "intermediate";
}
"#,
    );

    // Level 1: public module
    write_file(
        &base.join("src/index.ts"),
        r#"
export { coreFunction, CORE_VERSION, intermediateFunction } from "./intermediate";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/core.js").is_file());
    assert!(base.join("dist/src/intermediate.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("coreFunction"),
        "coreFunction should be re-exported"
    );
    assert!(
        index_dts.contains("intermediateFunction"),
        "intermediateFunction should be re-exported"
    );
}
#[test]
fn compile_module_mixed_exports_and_reexports() {
    // Test mixing local exports with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/helpers.ts"),
        r#"
export function helperA(): string {
    return "A";
}

export function helperB(): string {
    return "B";
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
// Re-exports
export { helperA, helperB } from "./helpers";

// Local exports
export function localFunction(): number {
    return 42;
}

export const LOCAL_CONSTANT = "local";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read js");
    assert!(
        index_js.contains("localFunction"),
        "Local function should be in output"
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("helperA"),
        "helperA should be re-exported"
    );
    assert!(
        index_dts.contains("localFunction"),
        "localFunction should be exported"
    );
    assert!(
        index_dts.contains("LOCAL_CONSTANT"),
        "LOCAL_CONSTANT should be exported"
    );
}
#[test]
fn compile_module_type_only_reexports() {
    // Test type-only re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/types.ts"),
        r#"
export type UserId = number;

export type UserName = string;

export function createId(n: number): UserId {
    return n;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
// Type-only re-exports (should be erased from JS)
export type { UserId, UserName } from "./types";

// Value re-export
export { createId } from "./types";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read js");
    // Type-only exports should not appear in runtime output, but createId should
    assert!(
        index_js.contains("createId"),
        "createId should be in output"
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("UserId"),
        "UserId type should be in declaration"
    );
    assert!(
        index_dts.contains("createId"),
        "createId should be in declaration"
    );
}
#[test]
fn compile_module_default_reexport() {
    // Test default re-export
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/component.ts"),
        r#"
export default function Component(): string {
    return "Component";
}

export const version = "1.0";
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { default, version } from "./component";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("default") || index_dts.contains("Component"),
        "default export should be re-exported"
    );
    assert!(
        index_dts.contains("version"),
        "version should be re-exported"
    );
}
#[test]
fn compile_module_barrel_file() {
    // Test barrel file pattern (common in libraries)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Feature modules
    write_file(
        &base.join("src/features/auth.ts"),
        r#"
export function login(user: string): boolean {
    return user.length > 0;
}

export function logout(): void {}
"#,
    );

    write_file(
        &base.join("src/features/data.ts"),
        r#"
export function fetchData(): string[] {
    return [];
}

export function saveData(data: string[]): boolean {
    return data.length > 0;
}
"#,
    );

    // Barrel file
    write_file(
        &base.join("src/features/index.ts"),
        r#"
export { login, logout } from "./auth";
export { fetchData, saveData } from "./data";
"#,
    );

    // Main entry
    write_file(
        &base.join("src/index.ts"),
        r#"
export { login, logout, fetchData, saveData } from "./features";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/features/auth.js").is_file());
    assert!(base.join("dist/src/features/data.js").is_file());
    assert!(base.join("dist/src/features/index.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("login"), "login should be re-exported");
    assert!(
        index_dts.contains("fetchData"),
        "fetchData should be re-exported"
    );
}

// =============================================================================
// E2E: Classes with Generic Methods
// =============================================================================
#[test]
fn compile_class_with_generic_constructor() {
    // Test class with generic constructor pattern
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/builder.ts"),
        r#"
export class Builder<T> {
    private value: T;

    constructor(initial: T) {
        this.value = initial;
    }

    set(value: T): Builder<T> {
        this.value = value;
        return this;
    }

    transform<U>(fn: (value: T) => U): Builder<U> {
        return new Builder(fn(this.value));
    }

    build(): T {
        return this.value;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/builder.js")).expect("read js");
    assert!(js.contains("class Builder"), "Class should be present");
    assert!(js.contains("constructor("), "Constructor should be present");
    assert!(!js.contains("<T>"), "Generic should be stripped");

    let dts = std::fs::read_to_string(base.join("dist/src/builder.d.ts")).expect("read dts");
    assert!(
        dts.contains("Builder<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("transform<U>"),
        "Generic method should be in declaration"
    );
}

// =============================================================================
// E2E: Namespace Exports
// =============================================================================
#[test]
fn compile_basic_namespace_export() {
    // Test basic namespace compiles without errors and produces JS output
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export namespace Utils {
    export const VERSION = "1.0.0";
    export function greet(name: string): string {
        return "Hello, " + name;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/utils.js")).expect("read js");
    // Namespace should produce some output
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_nested_namespace_export() {
    // Test nested namespace compiles without errors
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/api.ts"),
        r#"
export namespace API {
    export namespace V1 {
        export function getUsers(): string[] {
            return ["user1", "user2"];
        }
    }

    export namespace V2 {
        export function getUsers(): string[] {
            return ["user1", "user2", "user3"];
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/api.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_namespace_with_class() {
    // Test namespace containing a class compiles without errors
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/models.ts"),
        r#"
export namespace Models {
    export class User {
        name: string;
        constructor(name: string) {
            this.name = name;
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/models.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Enum Compilation
// =============================================================================
#[test]
fn compile_numeric_enum() {
    // Test basic numeric enum compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/status.ts"),
        r#"
export enum Status {
    Pending,
    Active,
    Completed,
    Failed
}

export function getStatusName(status: Status): string {
    return Status[status];
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/status.js")).expect("read js");
    assert!(js.contains("Status"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_string_enum() {
    // Test string enum compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/direction.ts"),
        r#"
export enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
}

export function move(dir: Direction): Direction {
    return dir;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/direction.js")).expect("read js");
    assert!(js.contains("Direction"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_const_enum() {
    // Test const enum compilation (should be inlined)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/flags.ts"),
        r#"
export const enum Flags {
    None = 0,
    Read = 1,
    Write = 2,
    Execute = 4
}

export function hasFlag(flags: Flags, flag: Flags): boolean {
    return (flags & flag) !== 0;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/flags.js")).expect("read js");
    // Const enums may be inlined, so just verify compilation succeeded
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_enum_with_computed_values() {
    // Test enum with computed/expression values
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/sizes.ts"),
        r#"
export enum Size {
    Small = 1,
    Medium = Small * 2,
    Large = Medium * 2,
    ExtraLarge = Large * 2
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/sizes.js")).expect("read js");
    assert!(js.contains("Size"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Arrow Function Compilation
// =============================================================================
#[test]
fn compile_basic_arrow_function() {
    // Test basic arrow function compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export const add = (a: number, b: number): number => a + b;
export const multiply = (a: number, b: number): number => {
    return a * b;
};
export const identity = <T>(x: T): T => x;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/utils.js")).expect("read js");
    assert!(
        js.contains("=>") || js.contains("function"),
        "Arrow or function should be present"
    );
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_arrow_function_with_rest_params() {
    // Test arrow function with rest parameters
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/helpers.ts"),
        r#"
export const sum = (...numbers: number[]): number => {
    let total = 0;
    for (const n of numbers) {
        total += n;
    }
    return total;
};

export const first = <T>(...items: T[]): T => items[0];
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/helpers.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_arrow_function_with_default_params() {
    // Test arrow function with default parameters
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/greet.ts"),
        r#"
export const greet = (name: string, greeting: string = "Hello"): string => {
    return greeting + ", " + name;
};

export const repeat = (str: string, times: number = 1): string => {
    let result = "";
    for (let i = 0; i < times; i++) {
        result += str;
    }
    return result;
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/greet.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}
#[test]
fn compile_arrow_function_in_class() {
    // Test arrow functions as class properties (for lexical this)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/counter.ts"),
        r#"
export class Counter {
    count: number = 0;

    increment = (): void => {
        this.count++;
    };

    decrement = (): void => {
        this.count--;
    };

    reset = (): void => {
        this.count = 0;
    };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/counter.js")).expect("read js");
    assert!(js.contains("Counter"), "Class should be present");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Spread Operator Compilation
// =============================================================================
