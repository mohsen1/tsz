/// Test source map generation for constraint extends union type in ES5 output.
/// Validates that type parameters constrained by union types generate proper source mappings.
#[test]
fn test_source_map_type_constraint_union_es5() {
    let source = r#"type Primitive = string | number | boolean;

function formatPrimitive<T extends Primitive>(value: T): string {
    return String(value);
}

type JsonValue = string | number | boolean | null | JsonArray | JsonObject;
interface JsonArray extends Array<JsonValue> {}
interface JsonObject { [key: string]: JsonValue; }

function stringify<T extends JsonValue>(value: T): string {
    return JSON.stringify(value);
}

type EventType = "click" | "hover" | "focus" | "blur";

function addEventListener<T extends EventType>(
    type: T,
    handler: (event: T) => void
): void {
    console.log(`Adding listener for ${type}`);
}

const formatted = formatPrimitive(42);
const json = stringify({ key: "value" });
addEventListener("click", (e) => console.log(e));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("formatPrimitive"),
        "expected formatPrimitive function in output. output: {output}"
    );
    assert!(
        output.contains("stringify"),
        "expected stringify function in output. output: {output}"
    );
    assert!(
        output.contains("addEventListener"),
        "expected addEventListener function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for union type constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple type parameter constraint patterns.
/// Tests generic functions, classes, interfaces with various constraint types.
#[test]
fn test_source_map_type_constraint_es5_comprehensive() {
    let source = r#"// Base interfaces for constraints
interface Identifiable {
    id: string;
}

interface Timestamped {
    createdAt: Date;
    updatedAt: Date;
}

interface Validatable {
    validate(): boolean;
}

// Generic class with multiple constraints
class DataStore<T extends Identifiable & Timestamped> {
    private data: Map<string, T> = new Map();

    save(item: T): void {
        this.data.set(item.id, item);
    }

    find(id: string): T | undefined {
        return this.data.get(id);
    }

    findRecent(since: Date): T[] {
        return Array.from(this.data.values())
            .filter(item => item.updatedAt > since);
    }
}

// Generic function with constraint referencing another type parameter
function createValidator<
    T extends Validatable,
    TResult extends { valid: boolean; errors: string[] }
>(item: T, resultFactory: () => TResult): TResult {
    const result = resultFactory();
    result.valid = item.validate();
    return result;
}

// Class with constrained method type parameters
class Mapper<TSource extends object> {
    map<TTarget extends object>(
        source: TSource,
        mapper: (s: TSource) => TTarget
    ): TTarget {
        return mapper(source);
    }

    mapArray<TTarget extends object>(
        sources: TSource[],
        mapper: (s: TSource) => TTarget
    ): TTarget[] {
        return sources.map(mapper);
    }
}

// Conditional constraint pattern
type Constructor<T> = new (...args: any[]) => T;

function mixin<TBase extends Constructor<{}>>(Base: TBase) {
    return class extends Base {
        mixinProp = "mixed";
    };
}

// Usage
interface User extends Identifiable, Timestamped {
    name: string;
    email: string;
}

const store = new DataStore<User>();
const mapper = new Mapper<{ x: number }>();
const result = mapper.map({ x: 1 }, (s) => ({ y: s.x * 2 }));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("DataStore"),
        "expected DataStore class in output. output: {output}"
    );
    assert!(
        output.contains("createValidator"),
        "expected createValidator function in output. output: {output}"
    );
    assert!(
        output.contains("Mapper"),
        "expected Mapper class in output. output: {output}"
    );
    assert!(
        output.contains("mixin"),
        "expected mixin function in output. output: {output}"
    );
    assert!(
        output.contains("findRecent"),
        "expected findRecent method in output. output: {output}"
    );
    assert!(
        output.contains("mapArray"),
        "expected mapArray method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive type constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: CONDITIONAL TYPE EXPRESSIONS
// =============================================================================

/// Test source map generation for conditional types with infer keyword in ES5 output.
/// Validates that infer patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_infer_es5() {
    let source = r#"// Infer return type
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// Infer parameter types
type Parameters<T> = T extends (...args: infer P) => any ? P : never;

// Infer array element type
type ElementType<T> = T extends (infer E)[] ? E : never;

// Infer promise resolved type
type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T;

// Function using inferred types
function getReturnType<T extends (...args: any[]) => any>(
    fn: T
): ReturnType<T> | undefined {
    try {
        return fn() as ReturnType<T>;
    } catch {
        return undefined;
    }
}

function callWithArgs<T extends (...args: any[]) => any>(
    fn: T,
    ...args: Parameters<T>
): ReturnType<T> {
    return fn(...args);
}

const add = (a: number, b: number) => a + b;
const result = callWithArgs(add, 1, 2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("getReturnType"),
        "expected getReturnType function in output. output: {output}"
    );
    assert!(
        output.contains("callWithArgs"),
        "expected callWithArgs function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional type with infer"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for distributive conditional types in ES5 output.
/// Validates that distributive conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_distributive_es5() {
    let source = r#"// Distributive conditional type
type ToArray<T> = T extends any ? T[] : never;

// Non-nullable extraction
type NonNullable<T> = T extends null | undefined ? never : T;

// Extract types from union
type Extract<T, U> = T extends U ? T : never;

// Exclude types from union
type Exclude<T, U> = T extends U ? never : T;

// Practical usage
type StringOrNumber = string | number | null | undefined;
type NonNullStringOrNumber = NonNullable<StringOrNumber>;
type OnlyStrings = Extract<StringOrNumber, string>;
type NoStrings = Exclude<StringOrNumber, string>;

function filterNonNull<T>(items: (T | null | undefined)[]): NonNullable<T>[] {
    return items.filter((item): item is NonNullable<T> => item != null);
}

function extractStrings(items: (string | number)[]): string[] {
    return items.filter((item): item is string => typeof item === "string");
}

const mixed = [1, "hello", null, 2, "world", undefined];
const nonNull = filterNonNull(mixed);
const strings = extractStrings([1, "a", 2, "b"]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("filterNonNull"),
        "expected filterNonNull function in output. output: {output}"
    );
    assert!(
        output.contains("extractStrings"),
        "expected extractStrings function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for distributive conditional type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for nested conditional types in ES5 output.
/// Validates that deeply nested conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_nested_es5() {
    let source = r#"// Nested conditional types
type DeepReadonly<T> = T extends (infer U)[]
    ? ReadonlyArray<DeepReadonly<U>>
    : T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;

// Type classification
type TypeName<T> = T extends string
    ? "string"
    : T extends number
    ? "number"
    : T extends boolean
    ? "boolean"
    : T extends undefined
    ? "undefined"
    : T extends Function
    ? "function"
    : "object";

// Flatten nested arrays
type Flatten<T> = T extends Array<infer U>
    ? U extends Array<any>
        ? Flatten<U>
        : U
    : T;

function getTypeName<T>(value: T): TypeName<T> {
    return typeof value as TypeName<T>;
}

function flatten<T>(arr: T[][]): Flatten<T[][]>[] {
    return arr.reduce((acc, val) => acc.concat(val), [] as Flatten<T[][]>[]);
}

const typeName = getTypeName("hello");
const flat = flatten([[1, 2], [3, 4]]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("getTypeName"),
        "expected getTypeName function in output. output: {output}"
    );
    assert!(
        output.contains("flatten"),
        "expected flatten function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for conditional types in function returns in ES5 output.
/// Validates that conditional return types generate proper source mappings.
#[test]
fn test_source_map_conditional_type_function_return_es5() {
    let source = r#"// Conditional return based on input type
type StringOrNumberResult<T> = T extends string ? string[] : number[];

function process<T extends string | number>(
    input: T
): StringOrNumberResult<T> {
    if (typeof input === "string") {
        return input.split("") as StringOrNumberResult<T>;
    }
    return [input] as StringOrNumberResult<T>;
}

// Conditional async return
type AsyncResult<T> = T extends Promise<infer U> ? U : Promise<T>;

async function ensureAsync<T>(value: T): Promise<AsyncResult<T>> {
    if (value instanceof Promise) {
        return value as unknown as AsyncResult<T>;
    }
    return value as AsyncResult<T>;
}

// Method overload simulation with conditional
type MethodResult<T, K extends keyof T> = T[K] extends (...args: any[]) => infer R
    ? R
    : T[K];

function invoke<T extends object, K extends keyof T>(
    obj: T,
    key: K
): MethodResult<T, K> {
    const prop = obj[key];
    if (typeof prop === "function") {
        return (prop as Function).call(obj) as MethodResult<T, K>;
    }
    return prop as MethodResult<T, K>;
}

const strResult = process("hello");
const numResult = process(42);
const asyncVal = ensureAsync(123);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("process"),
        "expected process function in output. output: {output}"
    );
    assert!(
        output.contains("ensureAsync"),
        "expected ensureAsync function in output. output: {output}"
    );
    assert!(
        output.contains("invoke"),
        "expected invoke function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional function return types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for conditional types with unions in ES5 output.
/// Validates that union conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_union_es5() {
    let source = r#"// Union in conditional check
type IsUnion<T, U = T> = T extends U
    ? [U] extends [T]
        ? false
        : true
    : never;

// Conditional with union result
type Result<T, E> = T extends Error ? { ok: false; error: E } : { ok: true; value: T };

// Union narrowing conditional
type UnwrapPromise<T> = T extends Promise<infer U>
    ? U
    : T extends PromiseLike<infer U>
    ? U
    : T;

// Handler type based on event
type EventHandler<T> = T extends "click"
    ? (e: MouseEvent) => void
    : T extends "keypress"
    ? (e: KeyboardEvent) => void
    : T extends "submit"
    ? (e: Event) => void
    : never;

function createHandler<T extends "click" | "keypress" | "submit">(
    eventType: T,
    handler: EventHandler<T>
): void {
    document.addEventListener(eventType, handler as EventListener);
}

function wrapResult<T>(value: T): Result<T, Error> {
    if (value instanceof Error) {
        return { ok: false, error: value } as Result<T, Error>;
    }
    return { ok: true, value } as Result<T, Error>;
}

const wrapped = wrapResult(42);
const errorWrapped = wrapResult(new Error("oops"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("createHandler"),
        "expected createHandler function in output. output: {output}"
    );
    assert!(
        output.contains("wrapResult"),
        "expected wrapResult function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional type with union"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple conditional type expression patterns.
/// Tests infer, distributive, nested, and union conditional types together.
#[test]
fn test_source_map_conditional_type_es5_comprehensive() {
    let source = r#"// Complex conditional type utility library

// Extract constructor parameters
type ConstructorParameters<T> = T extends new (...args: infer P) => any ? P : never;

// Instance type from constructor
type InstanceType<T> = T extends new (...args: any[]) => infer R ? R : never;

// Readonly deep with conditional
type DeepPartial<T> = T extends object
    ? { [P in keyof T]?: DeepPartial<T[P]> }
    : T;

// Property types extraction
type PropertyType<T, K> = K extends keyof T ? T[K] : never;

// Function property keys
type FunctionKeys<T> = {
    [K in keyof T]: T[K] extends Function ? K : never;
}[keyof T];

// Non-function property keys
type DataKeys<T> = {
    [K in keyof T]: T[K] extends Function ? never : K;
}[keyof T];

// Class using conditional types
class TypedRegistry<T extends object> {
    private items: Map<string, T> = new Map();

    register(id: string, item: T): void {
        this.items.set(id, item);
    }

    get<K extends keyof T>(id: string, key: K): PropertyType<T, K> | undefined {
        const item = this.items.get(id);
        if (item) {
            return item[key] as PropertyType<T, K>;
        }
        return undefined;
    }

    update(id: string, partial: DeepPartial<T>): boolean {
        const item = this.items.get(id);
        if (item) {
            Object.assign(item, partial);
            return true;
        }
        return false;
    }

    callMethod<K extends FunctionKeys<T>>(
        id: string,
        method: K,
        ...args: T[K] extends (...args: infer P) => any ? P : never[]
    ): T[K] extends (...args: any[]) => infer R ? R : undefined {
        const item = this.items.get(id);
        if (item && typeof item[method] === "function") {
            return (item[method] as Function).apply(item, args);
        }
        return undefined as any;
    }
}

// Factory with conditional return
function createInstance<T extends new (...args: any[]) => any>(
    ctor: T,
    ...args: ConstructorParameters<T>
): InstanceType<T> {
    return new ctor(...args);
}

interface User {
    name: string;
    age: number;
    greet(): string;
}

const registry = new TypedRegistry<User>();
registry.register("user1", { name: "Alice", age: 30, greet: () => "Hello" });
const userName = registry.get("user1", "name");
registry.update("user1", { age: 31 });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("TypedRegistry"),
        "expected TypedRegistry class in output. output: {output}"
    );
    assert!(
        output.contains("register"),
        "expected register method in output. output: {output}"
    );
    assert!(
        output.contains("update"),
        "expected update method in output. output: {output}"
    );
    assert!(
        output.contains("callMethod"),
        "expected callMethod method in output. output: {output}"
    );
    assert!(
        output.contains("createInstance"),
        "expected createInstance function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: IMPORT/EXPORT ALIASES
// =============================================================================

/// Test source map generation for named imports with aliases in ES5 output.
/// Validates that `import { foo as bar }` generates proper source mappings.
#[test]
fn test_source_map_import_named_alias_es5() {
    let source = r#"// Named import with alias
import { useState as useStateHook } from "react";
import { Component as ReactComponent, createElement as h } from "react";
import { map as arrayMap, filter as arrayFilter, reduce as arrayReduce } from "lodash";

// Using aliased imports
function MyComponent() {
    const [count, setCount] = useStateHook(0);
    return h("div", null, count);
}

const numbers = [1, 2, 3, 4, 5];
const doubled = arrayMap(numbers, (n: number) => n * 2);
const evens = arrayFilter(numbers, (n: number) => n % 2 === 0);
const sum = arrayReduce(numbers, (acc: number, n: number) => acc + n, 0);

console.log(doubled, evens, sum);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("MyComponent"),
        "expected MyComponent function in output. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected doubled variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named import aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for named exports with aliases in ES5 output.
/// Validates that `export { foo as bar }` generates proper source mappings.
#[test]
fn test_source_map_export_named_alias_es5() {
    let source = r#"// Internal implementations
function internalAdd(a: number, b: number): number {
    return a + b;
}

function internalSubtract(a: number, b: number): number {
    return a - b;
}

const internalPI = 3.14159;
const internalE = 2.71828;

class InternalCalculator {
    add(a: number, b: number): number {
        return internalAdd(a, b);
    }

    subtract(a: number, b: number): number {
        return internalSubtract(a, b);
    }
}

// Export with aliases
export { internalAdd as add };
export { internalSubtract as subtract };
export { internalPI as PI, internalE as E };
export { InternalCalculator as Calculator };

// Also export with different alias
export { internalAdd as sum, internalSubtract as difference };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("internalAdd"),
        "expected internalAdd function in output. output: {output}"
    );
    assert!(
        output.contains("InternalCalculator"),
        "expected InternalCalculator class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named export aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

