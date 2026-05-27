#[test]
fn test_source_map_interface_function_type() {
    // Test interface describing function types
    let source = r#"interface SearchFunc {
    (source: string, subString: string): boolean;
}

interface Comparator<T> {
    (a: T, b: T): number;
}

interface AsyncCallback<T> {
    (error: Error | null, result: T | null): void;
}

const search: SearchFunc = function(source, subString) {
    return source.indexOf(subString) !== -1;
};

const numCompare: Comparator<number> = function(a, b) {
    return a - b;
};

const strCompare: Comparator<string> = function(a, b) {
    return a.localeCompare(b);
};

const callback: AsyncCallback<string> = function(error, result) {
    if (error) {
        console.log("Error:", error.message);
    } else {
        console.log("Result:", result);
    }
};

console.log(search("hello world", "world"));
console.log([3, 1, 2].sort(numCompare));
console.log(["c", "a", "b"].sort(strCompare));
callback(null, "success");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("search") && output.contains("numCompare"),
        "expected output to contain search and numCompare. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function type interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_class_implements() {
    // Test interface implemented by class
    let source = r#"interface Printable {
    print(): string;
}

interface Comparable<T> {
    compareTo(other: T): number;
}

interface Serializable {
    serialize(): string;
    deserialize(data: string): void;
}

class Document implements Printable, Serializable {
    constructor(private content: string) {}

    print(): string {
        return "Document: " + this.content;
    }

    serialize(): string {
        return JSON.stringify({ content: this.content });
    }

    deserialize(data: string): void {
        const obj = JSON.parse(data);
        this.content = obj.content;
    }
}

class Version implements Comparable<Version> {
    constructor(
        public major: number,
        public minor: number,
        public patch: number
    ) {}

    compareTo(other: Version): number {
        if (this.major !== other.major) return this.major - other.major;
        if (this.minor !== other.minor) return this.minor - other.minor;
        return this.patch - other.patch;
    }
}

const doc = new Document("Hello World");
console.log(doc.print());
console.log(doc.serialize());

const v1 = new Version(1, 2, 3);
const v2 = new Version(1, 3, 0);
console.log(v1.compareTo(v2));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Document") || output.contains("Version"),
        "expected output to contain Document or Version. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class implements interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_hybrid_types() {
    // Test interfaces with hybrid types (callable with properties)
    let source = r#"interface Counter {
    (start: number): string;
    interval: number;
    reset(): void;
}

interface Logger {
    (message: string): void;
    level: string;
    prefix: string;
    setLevel(level: string): void;
}

function createCounter(): Counter {
    const counter = function(start: number): string {
        return "Started at: " + start;
    } as Counter;
    counter.interval = 1000;
    counter.reset = function() {
        console.log("Counter reset");
    };
    return counter;
}

function createLogger(): Logger {
    const logger = function(message: string): void {
        console.log(logger.prefix + " [" + logger.level + "] " + message);
    } as Logger;
    logger.level = "INFO";
    logger.prefix = "App";
    logger.setLevel = function(level: string) {
        logger.level = level;
    };
    return logger;
}

const counter = createCounter();
console.log(counter(0));
console.log(counter.interval);
counter.reset();

const logger = createLogger();
logger("Hello");
logger.setLevel("DEBUG");
logger("World");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("createCounter") || output.contains("createLogger"),
        "expected output to contain createCounter or createLogger. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for hybrid types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_advanced_combined() {
    // Test combined advanced interface patterns
    let source = r#"interface EventEmitter<T extends string = string> {
    on(event: T, callback: (data: any) => void): this;
    off(event: T, callback: (data: any) => void): this;
    emit(event: T, data?: any): boolean;
    readonly listenerCount: number;
}

interface Repository<T, ID = number> {
    findById(id: ID): T | undefined;
    findAll(): T[];
    save(entity: T): T;
    delete(id: ID): boolean;
    [Symbol.iterator](): Iterator<T>;
}

interface ServiceConfig {
    readonly name: string;
    timeout?: number;
    retries?: number;
    onError?(error: Error): void;
}

interface Service<T> extends EventEmitter<"start" | "stop" | "error"> {
    readonly config: ServiceConfig;
    start(): Promise<void>;
    stop(): Promise<void>;
    getStatus(): "running" | "stopped" | "error";
}

const emitter: EventEmitter = {
    listenerCount: 0,
    on(event, callback) {
        console.log("Registered listener for", event);
        return this;
    },
    off(event, callback) {
        console.log("Removed listener for", event);
        return this;
    },
    emit(event, data) {
        console.log("Emitting", event, data);
        return true;
    }
};

const config: ServiceConfig = {
    name: "MyService",
    timeout: 5000,
    retries: 3,
    onError(error) {
        console.log("Service error:", error.message);
    }
};

emitter.on("message", (data) => console.log(data)).emit("message", "Hello");
console.log(config.name, config.timeout);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("emitter") || output.contains("config"),
        "expected output to contain emitter or config. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for advanced combined interface patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// More Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_nested_types() {
    // Test interface with deeply nested type structures
    let source = r#"interface Address {
    street: string;
    city: string;
    zip: string;
}

interface Contact {
    email: string;
    phone: string;
}

interface Company {
    name: string;
    address: Address;
    contacts: Contact[];
}

interface Employee {
    id: number;
    name: string;
    address: Address;
    contact: Contact;
    company: Company;
}

const employee: Employee = {
    id: 1,
    name: "John Doe",
    address: { street: "123 Main St", city: "NYC", zip: "10001" },
    contact: { email: "john@example.com", phone: "555-1234" },
    company: {
        name: "Acme Inc",
        address: { street: "456 Corp Ave", city: "NYC", zip: "10002" },
        contacts: [{ email: "info@acme.com", phone: "555-0000" }]
    }
};

console.log(employee.name);
console.log(employee.company.name);
console.log(employee.address.city);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("employee"),
        "expected output to contain employee. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_tuple_types() {
    // Test interface with tuple types
    let source = r#"interface Coordinate {
    position: [number, number];
    position3D: [number, number, number];
}

interface NamedTuple {
    range: [start: number, end: number];
    point: [x: number, y: number, z?: number];
}

interface MixedTuple {
    data: [string, number, boolean];
    rest: [string, ...number[]];
}

const coord: Coordinate = {
    position: [10, 20],
    position3D: [10, 20, 30]
};

const named: NamedTuple = {
    range: [0, 100],
    point: [5, 10]
};

const mixed: MixedTuple = {
    data: ["hello", 42, true],
    rest: ["prefix", 1, 2, 3, 4]
};

function processCoord(c: Coordinate): number {
    return c.position[0] + c.position[1];
}

console.log(coord.position);
console.log(named.range);
console.log(mixed.data);
console.log(processCoord(coord));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("coord") && output.contains("processCoord"),
        "expected output to contain coord and processCoord. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for tuple types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_literal_types() {
    // Test interface with literal types
    let source = r#"interface Status {
    code: 200 | 201 | 400 | 404 | 500;
    message: "success" | "created" | "error";
}

interface Config {
    mode: "development" | "production" | "test";
    debug: true | false;
    level: 1 | 2 | 3;
}

interface ButtonProps {
    variant: "primary" | "secondary" | "danger";
    size: "small" | "medium" | "large";
    disabled: boolean;
}

const status: Status = {
    code: 200,
    message: "success"
};

const config: Config = {
    mode: "production",
    debug: false,
    level: 2
};

const button: ButtonProps = {
    variant: "primary",
    size: "medium",
    disabled: false
};

function handleStatus(s: Status): void {
    if (s.code === 200) {
        console.log("OK:", s.message);
    }
}

console.log(status.code);
console.log(config.mode);
console.log(button.variant);
handleStatus(status);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("status") && output.contains("handleStatus"),
        "expected output to contain status and handleStatus. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for literal types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_never_unknown() {
    // Test interface with never and unknown types
    let source = r#"interface ErrorHandler {
    handle(error: unknown): never;
    log(message: string): void;
}

interface Parser {
    parse(input: unknown): string;
    validate(data: unknown): boolean;
}

interface Validator<T> {
    validate(value: unknown): value is T;
    assert(value: unknown): asserts value is T;
}

const handler: ErrorHandler = {
    handle(error: unknown): never {
        console.error("Fatal error:", error);
        throw new Error(String(error));
    },
    log(message: string): void {
        console.log(message);
    }
};

const parser: Parser = {
    parse(input: unknown): string {
        return String(input);
    },
    validate(data: unknown): boolean {
        return data !== null && data !== undefined;
    }
};

function processUnknown(value: unknown): string {
    if (typeof value === "string") {
        return value;
    }
    return String(value);
}

handler.log("Starting...");
console.log(parser.parse({ key: "value" }));
console.log(processUnknown(42));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("handler") || output.contains("processUnknown"),
        "expected output to contain handler or processUnknown. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for never/unknown types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_this_type() {
    // Test interface with this type for fluent APIs
    let source = r#"interface Chainable {
    setValue(value: string): this;
    setName(name: string): this;
    build(): string;
}

interface FluentBuilder<T> {
    add(item: T): this;
    remove(item: T): this;
    clear(): this;
    getItems(): T[];
}

const chainable: Chainable = {
    _value: "",
    _name: "",
    setValue(value: string) {
        (this as any)._value = value;
        return this;
    },
    setName(name: string) {
        (this as any)._name = name;
        return this;
    },
    build() {
        return (this as any)._name + ": " + (this as any)._value;
    }
} as any;

class ArrayBuilder<T> implements FluentBuilder<T> {
    private items: T[] = [];

    add(item: T): this {
        this.items.push(item);
        return this;
    }

    remove(item: T): this {
        const idx = this.items.indexOf(item);
        if (idx !== -1) this.items.splice(idx, 1);
        return this;
    }

    clear(): this {
        this.items = [];
        return this;
    }

    getItems(): T[] {
        return this.items.slice();
    }
}

const result = chainable.setValue("hello").setName("greeting").build();
console.log(result);

const builder = new ArrayBuilder<number>();
builder.add(1).add(2).add(3).remove(2);
console.log(builder.getItems());"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("chainable") || output.contains("ArrayBuilder"),
        "expected output to contain chainable or ArrayBuilder. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for this type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_overloaded_methods() {
    // Test interface with overloaded method signatures
    let source = r#"interface Processor {
    process(input: string): string;
    process(input: number): number;
    process(input: boolean): boolean;
    process(input: string | number | boolean): string | number | boolean;
}

interface Converter {
    convert(value: string, to: "number"): number;
    convert(value: string, to: "boolean"): boolean;
    convert(value: number, to: "string"): string;
}

interface EventTarget {
    addEventListener(type: "click", listener: (e: MouseEvent) => void): void;
    addEventListener(type: "keydown", listener: (e: KeyboardEvent) => void): void;
    addEventListener(type: string, listener: (e: Event) => void): void;
}

const processor: Processor = {
    process(input: any): any {
        if (typeof input === "string") return input.toUpperCase();
        if (typeof input === "number") return input * 2;
        return !input;
    }
};

const converter: Converter = {
    convert(value: any, to: string): any {
        if (to === "number") return Number(value);
        if (to === "boolean") return Boolean(value);
        return String(value);
    }
};

console.log(processor.process("hello"));
console.log(processor.process(21));
console.log(processor.process(false));
console.log(converter.convert("42", "number"));
console.log(converter.convert("true", "boolean"));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("processor") && output.contains("converter"),
        "expected output to contain processor and converter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for overloaded methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_async_methods() {
    // Test interface with async method signatures
    let source = r#"interface AsyncService {
    fetch(url: string): Promise<string>;
    fetchJson<T>(url: string): Promise<T>;
    post(url: string, data: any): Promise<void>;
}

interface DataLoader<T> {
    load(): Promise<T>;
    loadAll(): Promise<T[]>;
    refresh(): Promise<void>;
}

interface AsyncQueue<T> {
    enqueue(item: T): Promise<void>;
    dequeue(): Promise<T | undefined>;
    peek(): Promise<T | undefined>;
    isEmpty(): Promise<boolean>;
}

const service: AsyncService = {
    async fetch(url: string): Promise<string> {
        return "data from " + url;
    },
    async fetchJson<T>(url: string): Promise<T> {
        return { url } as any;
    },
    async post(url: string, data: any): Promise<void> {
        console.log("Posted to", url, data);
    }
};

async function useService(s: AsyncService): Promise<void> {
    const data = await s.fetch("/api/data");
    console.log(data);
    await s.post("/api/save", { value: 42 });
}

useService(service);
console.log("Service called");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("service") || output.contains("useService"),
        "expected output to contain service or useService. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_accessor_signatures() {
    // Test interface with getter/setter accessor signatures
    let source = r#"interface Readable {
    readonly value: string;
    readonly length: number;
}

interface Writable {
    value: string;
}

interface ReadWrite {
    get value(): string;
    set value(v: string);
    get computed(): number;
}

interface Observable<T> {
    get current(): T;
    set current(value: T);
    readonly previous: T | undefined;
}

const readable: Readable = {
    value: "hello",
    length: 5
};

const writable: Writable = {
    value: "initial"
};

class ObservableValue<T> implements Observable<T> {
    private _current: T;
    private _previous: T | undefined;

    constructor(initial: T) {
        this._current = initial;
    }

    get current(): T {
        return this._current;
    }

    set current(value: T) {
        this._previous = this._current;
        this._current = value;
    }

    get previous(): T | undefined {
        return this._previous;
    }
}

console.log(readable.value);
writable.value = "updated";
console.log(writable.value);

const obs = new ObservableValue<number>(0);
obs.current = 10;
obs.current = 20;
console.log(obs.current, obs.previous);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("readable") || output.contains("ObservableValue"),
        "expected output to contain readable or ObservableValue. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor signatures"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_symbol_properties() {
    // Test interface with symbol-keyed properties
    let source = r#"interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}

interface CustomIterable {
    [Symbol.iterator](): Iterator<number>;
    [Symbol.toStringTag]: string;
}

interface Disposable {
    [Symbol.dispose]?(): void;
}

class NumberRange implements CustomIterable {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Iterator<number> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }

    get [Symbol.toStringTag](): string {
        return "NumberRange";
    }
}

const range = new NumberRange(1, 5);
console.log(String(range));

for (const num of range) {
    console.log(num);
}"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("NumberRange") || output.contains("range"),
        "expected output to contain NumberRange or range. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for symbol properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_complex_combined() {
    // Test complex combined interface patterns
    let source = r#"interface BaseNode {
    readonly type: string;
    readonly id: number;
}

interface TextNode extends BaseNode {
    readonly type: "text";
    content: string;
}

interface ElementNode extends BaseNode {
    readonly type: "element";
    tagName: string;
    children: TreeNode[];
    attributes: { [key: string]: string };
}

type TreeNode = TextNode | ElementNode;

interface TreeVisitor<T> {
    visitText(node: TextNode): T;
    visitElement(node: ElementNode): T;
}

interface TreeTransformer extends TreeVisitor<TreeNode> {
    transform(root: TreeNode): TreeNode;
}

class NodeCounter implements TreeVisitor<number> {
    visitText(node: TextNode): number {
        return 1;
    }

    visitElement(node: ElementNode): number {
        let count = 1;
        for (const child of node.children) {
            if (child.type === "text") {
                count += this.visitText(child);
            } else {
                count += this.visitElement(child);
            }
        }
        return count;
    }
}

const textNode: TextNode = { type: "text", id: 1, content: "Hello" };
const elemNode: ElementNode = {
    type: "element",
    id: 2,
    tagName: "div",
    children: [textNode],
    attributes: { class: "container" }
};

const counter = new NodeCounter();
console.log(counter.visitText(textNode));
console.log(counter.visitElement(elemNode));
console.log(elemNode.tagName, elemNode.attributes);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("NodeCounter") || output.contains("textNode"),
        "expected output to contain NodeCounter or textNode. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for complex combined patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Extended Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_multiple_extends() {
    // Test interface extending multiple interfaces
    let source = r#"interface Named {
    name: string;
}

interface Aged {
    age: number;
}

interface Described {
    description: string;
}

interface Person extends Named, Aged {
    email: string;
}

interface DetailedPerson extends Named, Aged, Described {
    address: string;
}

const person: Person = {
    name: "John",
    age: 30,
    email: "john@example.com"
};

const detailed: DetailedPerson = {
    name: "Jane",
    age: 25,
    description: "Software Engineer",
    address: "123 Main St"
};

function greet(p: Named & Aged): string {
    return "Hello " + p.name + ", you are " + p.age;
}

console.log(person.name, person.age);
console.log(detailed.description);
console.log(greet(person));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("person") && output.contains("greet"),
        "expected output to contain person and greet. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_recursive_types() {
    // Test interface with recursive/self-referencing types
    let source = r#"interface TreeNode<T> {
    value: T;
    children: TreeNode<T>[];
    parent?: TreeNode<T>;
}

interface LinkedListNode<T> {
    value: T;
    next: LinkedListNode<T> | null;
    prev: LinkedListNode<T> | null;
}

interface JSONValue {
    [key: string]: JSONValue | string | number | boolean | null | JSONValue[];
}

const tree: TreeNode<string> = {
    value: "root",
    children: [
        { value: "child1", children: [] },
        { value: "child2", children: [
            { value: "grandchild", children: [] }
        ]}
    ]
};

const listNode: LinkedListNode<number> = {
    value: 1,
    next: { value: 2, next: null, prev: null },
    prev: null
};

function traverseTree<T>(node: TreeNode<T>, callback: (val: T) => void): void {
    callback(node.value);
    for (const child of node.children) {
        traverseTree(child, callback);
    }
}

traverseTree(tree, (v) => console.log(v));
console.log(listNode.value);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("tree") && output.contains("traverseTree"),
        "expected output to contain tree and traverseTree. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for recursive types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_discriminated_unions() {
    // Test interface with discriminated union patterns
    let source = r#"interface SuccessResult {
    kind: "success";
    data: string;
    timestamp: number;
}

interface ErrorResult {
    kind: "error";
    error: string;
    code: number;
}

interface LoadingResult {
    kind: "loading";
    progress: number;
}

type Result = SuccessResult | ErrorResult | LoadingResult;

interface Action {
    type: string;
}

interface AddAction extends Action {
    type: "add";
    payload: number;
}

interface RemoveAction extends Action {
    type: "remove";
    id: string;
}

type AppAction = AddAction | RemoveAction;

function handleResult(result: Result): string {
    switch (result.kind) {
        case "success":
            return "Data: " + result.data;
        case "error":
            return "Error " + result.code + ": " + result.error;
        case "loading":
            return "Loading: " + result.progress + "%";
    }
}

const success: SuccessResult = { kind: "success", data: "hello", timestamp: Date.now() };
const error: ErrorResult = { kind: "error", error: "Not found", code: 404 };

console.log(handleResult(success));
console.log(handleResult(error));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("handleResult") || output.contains("success"),
        "expected output to contain handleResult or success. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for discriminated unions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_type_guards() {
    // Test interface with type guard patterns
    let source = r#"interface Fish {
    swim(): void;
    name: string;
}

interface Bird {
    fly(): void;
    name: string;
}

interface Cat {
    meow(): void;
    name: string;
}

type Animal = Fish | Bird | Cat;

function isFish(animal: Animal): animal is Fish {
    return (animal as Fish).swim !== undefined;
}

function isBird(animal: Animal): animal is Bird {
    return (animal as Bird).fly !== undefined;
}

const fish: Fish = {
    name: "Nemo",
    swim() { console.log("Swimming..."); }
};

const bird: Bird = {
    name: "Tweety",
    fly() { console.log("Flying..."); }
};

function handleAnimal(animal: Animal): void {
    if (isFish(animal)) {
        animal.swim();
    } else if (isBird(animal)) {
        animal.fly();
    } else {
        animal.meow();
    }
}

handleAnimal(fish);
handleAnimal(bird);
console.log(fish.name, bird.name);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("isFish") || output.contains("handleAnimal"),
        "expected output to contain isFish or handleAnimal. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type guards"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_rest_elements() {
    // Test interface with rest elements in types
    let source = r#"interface FunctionWithRest {
    (...args: number[]): number;
}

interface ArrayWithRest {
    items: [string, ...number[]];
    mixed: [boolean, string, ...any[]];
}

interface SpreadParams {
    call(...args: string[]): void;
    apply(first: number, ...rest: string[]): string;
}

const sum: FunctionWithRest = function(...args: number[]): number {
    return args.reduce((a, b) => a + b, 0);
};

const arr: ArrayWithRest = {
    items: ["header", 1, 2, 3, 4],
    mixed: [true, "text", 1, "a", null]
};

const params: SpreadParams = {
    call(...args: string[]): void {
        console.log(args.join(", "));
    },
    apply(first: number, ...rest: string[]): string {
        return first + ": " + rest.join(" ");
    }
};

console.log(sum(1, 2, 3, 4, 5));
console.log(arr.items);
params.call("a", "b", "c");
console.log(params.apply(42, "hello", "world"));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("sum") || output.contains("params"),
        "expected output to contain sum or params. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

