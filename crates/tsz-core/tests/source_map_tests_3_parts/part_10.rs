#[test]
fn test_source_map_interface_readonly_properties() {
    // Test interface with readonly properties
    let source = r#"interface Point {
    readonly x: number;
    readonly y: number;
}

interface Circle {
    readonly center: Point;
    readonly radius: number;
}

const point: Point = { x: 10, y: 20 };
const circle: Circle = {
    center: { x: 0, y: 0 },
    radius: 5
};

function distance(p1: Point, p2: Point): number {
    const dx = p1.x - p2.x;
    const dy = p1.y - p2.y;
    return Math.sqrt(dx * dx + dy * dy);
}

console.log(point.x, point.y);
console.log(circle.center.x, circle.radius);
console.log(distance(point, circle.center));"#;

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
        output.contains("point") && output.contains("distance"),
        "expected output to contain point and distance. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for readonly properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_index_signature() {
    // Test interface with index signatures
    let source = r#"interface StringDictionary {
    [key: string]: string;
}

interface NumberDictionary {
    [index: number]: string;
    length: number;
}

interface MixedDictionary {
    [key: string]: number | string;
    name: string;
    count: number;
}

const dict: StringDictionary = {
    foo: "bar",
    hello: "world"
};

const numDict: NumberDictionary = {
    0: "first",
    1: "second",
    length: 2
};

const mixed: MixedDictionary = {
    name: "test",
    count: 42,
    extra: "value"
};

function getValues(d: StringDictionary): string[] {
    return Object.values(d);
}

console.log(dict["foo"]);
console.log(numDict[0]);
console.log(mixed.name, mixed.count);
console.log(getValues(dict));"#;

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
        output.contains("dict") && output.contains("getValues"),
        "expected output to contain dict and getValues. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for index signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_call_signature() {
    // Test interface with call signatures
    let source = r#"interface StringProcessor {
    (input: string): string;
}

interface Calculator {
    (a: number, b: number): number;
    description: string;
}

interface Formatter {
    (value: any): string;
    (value: any, format: string): string;
}

const uppercase: StringProcessor = function(input) {
    return input.toUpperCase();
};

const add: Calculator = function(a, b) {
    return a + b;
};
add.description = "Adds two numbers";

const format: Formatter = function(value: any, fmt?: string) {
    if (fmt) {
        return fmt + ": " + String(value);
    }
    return String(value);
};

console.log(uppercase("hello"));
console.log(add(5, 3), add.description);
console.log(format(42));
console.log(format(42, "Number"));"#;

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
        output.contains("uppercase") && output.contains("format"),
        "expected output to contain uppercase and format. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_construct_signature() {
    // Test interface with construct signatures
    let source = r#"interface PointConstructor {
    new(x: number, y: number): { x: number; y: number };
}

interface ClockConstructor {
    new(hour: number, minute: number): ClockInterface;
}

interface ClockInterface {
    tick(): void;
    getTime(): string;
}

function createPoint(ctor: PointConstructor, x: number, y: number) {
    return new ctor(x, y);
}

const PointClass: PointConstructor = class {
    constructor(public x: number, public y: number) {}
};

const point = createPoint(PointClass, 10, 20);
console.log(point.x, point.y);"#;

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
        output.contains("createPoint") || output.contains("PointClass"),
        "expected output to contain createPoint or PointClass. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for construct signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_merging() {
    // Test interface merging (declaration merging)
    let source = r#"interface Box {
    height: number;
    width: number;
}

interface Box {
    depth: number;
    color: string;
}

interface Box {
    weight?: number;
}

const box: Box = {
    height: 10,
    width: 20,
    depth: 30,
    color: "red"
};

const heavyBox: Box = {
    height: 5,
    width: 5,
    depth: 5,
    color: "blue",
    weight: 100
};

function describeBox(b: Box): string {
    let desc = b.color + " box: " + b.width + "x" + b.height + "x" + b.depth;
    if (b.weight) {
        desc += " (" + b.weight + "kg)";
    }
    return desc;
}

console.log(describeBox(box));
console.log(describeBox(heavyBox));"#;

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
        output.contains("box") && output.contains("describeBox"),
        "expected output to contain box and describeBox. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface merging"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

