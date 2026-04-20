#[test]
fn test_source_map_type_alias_basic() {
    // Test basic type alias with type erasure
    let source = r#"type StringOrNumber = string | number;
type Point = { x: number; y: number };

const value: StringOrNumber = "hello";
const point: Point = { x: 10, y: 20 };

console.log(value, point.x);"#;

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

    // Type alias should be erased
    assert!(
        !output.contains("type StringOrNumber") && !output.contains("type Point"),
        "type alias should be erased. output: {output}"
    );
    assert!(
        output.contains("value") && output.contains("point"),
        "expected output to contain value and point. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type alias usage"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_with_methods() {
    // Test interface with method signatures
    let source = r#"interface Calculator {
    add(a: number, b: number): number;
    subtract(a: number, b: number): number;
    multiply(a: number, b: number): number;
}

const calc: Calculator = {
    add(a, b) { return a + b; },
    subtract(a, b) { return a - b; },
    multiply(a, b) { return a * b; }
};

console.log(calc.add(5, 3));
console.log(calc.subtract(10, 4));
console.log(calc.multiply(2, 6));"#;

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
        output.contains("calc") && output.contains("add"),
        "expected output to contain calc and add. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface with methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_extends() {
    // Test interface extending another interface
    let source = r#"interface Animal {
    name: string;
    age: number;
}

interface Dog extends Animal {
    breed: string;
    bark(): void;
}

const dog: Dog = {
    name: "Rex",
    age: 5,
    breed: "German Shepherd",
    bark() {
        console.log("Woof!");
    }
};

dog.bark();
console.log(dog.name, dog.breed);"#;

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
        output.contains("dog") && output.contains("bark"),
        "expected output to contain dog and bark. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_union_intersection() {
    // Test union and intersection type aliases
    let source = r#"type ID = string | number;
type Name = { first: string; last: string };
type Age = { age: number };
type Person = Name & Age;

const id: ID = 123;
const person: Person = {
    first: "John",
    last: "Doe",
    age: 30
};

function printId(id: ID): void {
    console.log("ID:", id);
}

function printPerson(p: Person): void {
    console.log(p.first, p.last, p.age);
}

printId(id);
printPerson(person);"#;

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
        output.contains("printId") && output.contains("printPerson"),
        "expected output to contain printId and printPerson. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for union/intersection types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_generic() {
    // Test generic interface declarations
    let source = r#"interface Container<T> {
    value: T;
    getValue(): T;
    setValue(value: T): void;
}

interface Pair<K, V> {
    key: K;
    value: V;
}

const numContainer: Container<number> = {
    value: 42,
    getValue() { return this.value; },
    setValue(v) { this.value = v; }
};

const pair: Pair<string, number> = {
    key: "age",
    value: 30
};

console.log(numContainer.getValue());
console.log(pair.key, pair.value);"#;

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
        output.contains("numContainer") || output.contains("pair"),
        "expected output to contain numContainer or pair. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_generic() {
    // Test generic type alias declarations
    let source = r#"type Nullable<T> = T | null;
type Result<T, E> = { success: true; value: T } | { success: false; error: E };
type AsyncResult<T> = Promise<Result<T, Error>>;

const name: Nullable<string> = "John";
const nullName: Nullable<string> = null;

const success: Result<number, string> = { success: true, value: 42 };
const failure: Result<number, string> = { success: false, error: "Not found" };

function processResult<T>(result: Result<T, string>): T | null {
    if (result.success) {
        return result.value;
    }
    console.log("Error:", result.error);
    return null;
}

console.log(name, nullName);
console.log(processResult(success));
console.log(processResult(failure));"#;

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
        output.contains("processResult") || output.contains("success"),
        "expected output to contain processResult or success. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic type alias"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_mapped() {
    // Test mapped type aliases
    let source = r#"type Readonly<T> = { readonly [K in keyof T]: T[K] };
type Partial<T> = { [K in keyof T]?: T[K] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

interface User {
    id: number;
    name: string;
    email: string;
}

const readonlyUser: Readonly<User> = {
    id: 1,
    name: "John",
    email: "john@example.com"
};

const partialUser: Partial<User> = {
    name: "Jane"
};

const pickedUser: Pick<User, "id" | "name"> = {
    id: 2,
    name: "Bob"
};

console.log(readonlyUser.name);
console.log(partialUser.name);
console.log(pickedUser.id, pickedUser.name);"#;

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
        output.contains("readonlyUser") || output.contains("partialUser"),
        "expected output to contain readonlyUser or partialUser. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mapped types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_conditional() {
    // Test conditional type aliases
    let source = r#"type IsString<T> = T extends string ? true : false;
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type NonNullable<T> = T extends null | undefined ? never : T;

type StringCheck = IsString<string>;
type NumberCheck = IsString<number>;

const value1: NonNullable<string | null> = "hello";
const value2: UnwrapPromise<Promise<number>> = 42;

function checkType<T>(value: T): IsString<T> {
    return (typeof value === "string") as any;
}

console.log(value1, value2);
console.log(checkType("test"));
console.log(checkType(123));"#;

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
        output.contains("checkType") || output.contains("value1"),
        "expected output to contain checkType or value1. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_type_alias_combined() {
    // Test combined interface and type alias patterns
    let source = r#"interface BaseEntity {
    id: number;
    createdAt: Date;
    updatedAt: Date;
}

interface User extends BaseEntity {
    username: string;
    email: string;
}

interface Post extends BaseEntity {
    title: string;
    content: string;
    authorId: number;
}

type EntityType = "user" | "post";
type Entity<T extends EntityType> = T extends "user" ? User : Post;

type CreateInput<T extends BaseEntity> = Omit<T, "id" | "createdAt" | "updatedAt">;
type UpdateInput<T extends BaseEntity> = Partial<CreateInput<T>>;

const userInput: CreateInput<User> = {
    username: "johndoe",
    email: "john@example.com"
};

const postUpdate: UpdateInput<Post> = {
    title: "Updated Title"
};

function createEntity<T extends EntityType>(
    type: T,
    input: CreateInput<Entity<T>>
): Entity<T> {
    const now = new Date();
    return {
        ...input,
        id: Math.random(),
        createdAt: now,
        updatedAt: now
    } as Entity<T>;
}

function updateEntity<T extends BaseEntity>(
    entity: T,
    updates: UpdateInput<T>
): T {
    return {
        ...entity,
        ...updates,
        updatedAt: new Date()
    };
}

console.log(userInput);
console.log(postUpdate);
console.log(createEntity("user", userInput));"#;

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
        output.contains("createEntity") || output.contains("updateEntity"),
        "expected output to contain createEntity or updateEntity. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined interface/type patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Additional Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_optional_properties() {
    // Test interface with optional properties
    let source = r#"interface Config {
    host: string;
    port?: number;
    secure?: boolean;
    timeout?: number;
}

const config: Config = {
    host: "localhost"
};

const fullConfig: Config = {
    host: "example.com",
    port: 443,
    secure: true,
    timeout: 5000
};

function createConnection(cfg: Config): void {
    console.log("Connecting to", cfg.host);
    if (cfg.port) {
        console.log("Port:", cfg.port);
    }
}

createConnection(config);
createConnection(fullConfig);"#;

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
        output.contains("config") && output.contains("createConnection"),
        "expected output to contain config and createConnection. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

