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
            "rootDir": ".",
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
            "rootDir": ".",
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
            "rootDir": ".",
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
            "rootDir": ".",
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
            "rootDir": ".",
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
            "rootDir": ".",
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
            "rootDir": ".",
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

#[test]
fn compile_enum_with_nan_and_infinity_globals() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "module": "commonjs",
            "outDir": "out",
            "noEmitOnError": false,
            "pretty": false,
            "ignoreDeprecations": "6.0"
          },
          "files": ["a.ts"]
        }"#,
    );

    write_file(
        &base.join("a.ts"),
        r#"
enum E { A = Infinity, B }
enum N { A = NaN, B }
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "NaN and Infinity enum initializers should not report diagnostics: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("out/a.js")).expect("read js");
    for expected in [
        "E[E[\"A\"] = Infinity] = \"A\"",
        "E[E[\"B\"] = Infinity] = \"B\"",
        "N[N[\"A\"] = NaN] = \"A\"",
        "N[N[\"B\"] = NaN] = \"B\"",
    ] {
        assert!(
            js.contains(expected),
            "Expected emitted JS to contain {expected:?}, got:\n{js}"
        );
    }
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
            "rootDir": ".",
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

#[test]
fn compile_array_spread() {
    // Test array spread operator compilation
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
        &base.join("src/arrays.ts"),
        r#"
export function concat(a: number[], b: number[]): number[] {
    return [...a, ...b];
}

export function copy(a: number[]): number[] {
    return [...a];
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

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_es5_downlevel_iteration_single_call_spread_uses_read() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "module": "commonjs",
            "outDir": "dist",
            "downlevelIteration": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["src/calls.ts"]
        }"#,
    );

    write_file(
        &base.join("src/calls.ts"),
        r#"
function f(...args: any[]) {
    return args.join("|");
}
const s: any = "ab";
export const value = f(...s);
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/calls.js")).expect("read js");
    assert!(
        js.contains("f.apply(void 0, __spreadArray([], __read(s), false))"),
        "single call spread with downlevelIteration should read iterables before apply:\n{js}"
    );
    assert!(
        !js.contains("f.apply(void 0, s)"),
        "single call spread must not pass iterable directly to apply:\n{js}"
    );
}

#[test]
fn compile_es5_array_spread_packs_sparse_spread_segments() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "module": "commonjs",
            "outDir": "dist",
            "lib": ["es2015"],
            "ignoreDeprecations": "6.0"
          },
          "files": ["src/arrays.ts"]
        }"#,
    );

    write_file(
        &base.join("src/arrays.ts"),
        r#"
const xs = Array(2);
export const leading = [...xs, 4];
export const middle = [1, ...xs, 4];
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(
        js.contains("__spreadArray(__spreadArray([], xs, true), [4], false)"),
        "leading sparse spread should pack holes before appending literals:\n{js}"
    );
    assert!(
        js.contains("__spreadArray(__spreadArray([1], xs, true), [4], false)"),
        "middle sparse spread should pack holes between literal segments:\n{js}"
    );
}

#[test]
fn compile_object_spread() {
    // Test object spread operator compilation
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
        &base.join("src/objects.ts"),
        r#"
interface Person {
    name: string;
    age: number;
}

export function clone(obj: Person): Person {
    return { ...obj };
}

export function update(obj: Person, updates: Person): Person {
    return { ...obj, ...updates };
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

    let js = std::fs::read_to_string(base.join("dist/src/objects.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_function_call_spread() {
    // Test spread in function calls
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
        &base.join("src/calls.ts"),
        r#"
export function apply(fn: (...args: number[]) => number, args: number[]): number {
    return fn(...args);
}

export function log(...items: string[]): string[] {
    return items;
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

    let js = std::fs::read_to_string(base.join("dist/src/calls.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Template Literal Compilation
// =============================================================================

#[test]
fn compile_basic_template_literal() {
    // Test basic template literal compilation
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
export function greet(name: string): string {
    return `Hello, ${name}!`;
}

export function format(a: number, b: number): string {
    return `${a} + ${b} = ${a + b}`;
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

    let js = std::fs::read_to_string(base.join("dist/src/greet.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_multiline_template_literal() {
    // Test multiline template literal compilation
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
        &base.join("src/html.ts"),
        r#"
export function createDiv(content: string): string {
    const result = `<div><p>${content}</p></div>`;
    return result;
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

    let js = std::fs::read_to_string(base.join("dist/src/html.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nested_template_literal() {
    // Test nested template expressions
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
        &base.join("src/nested.ts"),
        r#"
export function wrap(inner: string, outer: string): string {
    return `${outer}: ${`[${inner}]`}`;
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

    let js = std::fs::read_to_string(base.join("dist/src/nested.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Destructuring Assignment Compilation
// =============================================================================

#[test]
fn compile_object_destructuring() {
    // Test object destructuring compilation
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
        &base.join("src/extract.ts"),
        r#"
interface Point {
    x: number;
    y: number;
}

export function getX(point: Point): number {
    const { x } = point;
    return x;
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

    let js = std::fs::read_to_string(base.join("dist/src/extract.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_array_destructuring() {
    // Test array destructuring compilation
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
        &base.join("src/arrays.ts"),
        r#"
export function getFirst(arr: number[]): number {
    const [first] = arr;
    return first;
}

export function getSecond(arr: number[]): number {
    const [, second] = arr;
    return second;
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

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_es5_downlevel_iteration_array_rest_reads_full_iterator() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "module": "commonjs",
            "lib": ["es2015", "dom"],
            "downlevelIteration": true,
            "ignoreDeprecations": "6.0",
            "outDir": "dist"
          },
          "files": ["src/rest.ts"]
        }"#,
    );

    write_file(
        &base.join("src/rest.ts"),
        r#"
const iter: any = new Set([1, 2]);
const [first, ...rest] = iter;
console.log(first, rest.join(","));
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/rest.js")).expect("read js");
    assert!(
        js.contains("__read(iter)"),
        "array rest binding should read the full iterator: {js}"
    );
    assert!(
        !js.contains("__read(iter, 1)"),
        "array rest binding must not truncate the iterator read: {js}"
    );
    assert!(
        js.contains("rest = _a.slice(1)") || js.contains("rest = _b.slice(1)"),
        "array rest binding should slice after the fixed element: {js}"
    );
}

#[test]
fn compile_destructuring_with_defaults() {
    // Test destructuring with default values
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
        &base.join("src/defaults.ts"),
        r#"
interface Config {
    host: string;
    port: number;
}

export function getPort(config: Config): number {
    const { port = 3000 } = config;
    return port;
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

    let js = std::fs::read_to_string(base.join("dist/src/defaults.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining() {
    // Test optional chaining (?.) compilation
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
        &base.join("src/optional.ts"),
        r#"
interface User {
    name: string;
    address?: {
        city: string;
    };
}

export function getCity(user: User): string | undefined {
    return user.address?.city;
}

export function getLength(arr?: string[]): number | undefined {
    return arr?.length;
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

    let js = std::fs::read_to_string(base.join("dist/src/optional.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nullish_coalescing() {
    // Test nullish coalescing (??) compilation
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
        &base.join("src/nullish.ts"),
        r#"
export function getValueOrDefault(value: string | null | undefined): string {
    return value ?? "default";
}

export function getNumberOrZero(num: number | null): number {
    return num ?? 0;
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

    let js = std::fs::read_to_string(base.join("dist/src/nullish.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining_with_call() {
    // Test optional chaining with method calls
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
        &base.join("src/optcall.ts"),
        r#"
interface Logger {
    log?: (msg: string) => void;
}

export function maybeLog(logger: Logger, msg: string): void {
    logger.log?.(msg);
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

    let js = std::fs::read_to_string(base.join("dist/src/optcall.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_inheritance() {
    // Test class inheritance compilation
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
        &base.join("src/classes.ts"),
        r#"
export class Animal {
    constructor(public name: string) {}
    speak(): string {
        return this.name;
    }
}

export class Dog extends Animal {
    constructor(name: string) {
        super(name);
    }
    speak(): string {
        return "Woof: " + super.speak();
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

    let js = std::fs::read_to_string(base.join("dist/src/classes.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_static_members() {
    // Test class static members compilation
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
        &base.join("src/staticclass.ts"),
        r#"
export class Counter {
    static count: number = 0;

    static increment(): number {
        Counter.count += 1;
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
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

    let js = std::fs::read_to_string(base.join("dist/src/staticclass.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_accessors() {
    // Test class getter/setter compilation
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
        &base.join("src/accessors.ts"),
        r#"
export class Rectangle {
    private _width: number = 0;
    private _height: number = 0;

    get width(): number {
        return this._width;
    }

    set width(value: number) {
        this._width = value;
    }

    get area(): number {
        return this._width * this._height;
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

    let js = std::fs::read_to_string(base.join("dist/src/accessors.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_computed_property_names() {
    // Test computed property names compilation
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
        &base.join("src/computed.ts"),
        r#"
const KEY = "dynamicKey";

export const obj = {
    [KEY]: "value",
    ["literal" + "Key"]: 42
};

export function getProp(key: string): { [k: string]: number } {
    return { [key]: 100 };
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

    let js = std::fs::read_to_string(base.join("dist/src/computed.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

