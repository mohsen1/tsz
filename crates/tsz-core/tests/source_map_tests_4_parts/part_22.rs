/// Test source map generation for `ReturnType`<T> utility type in ES5 output.
/// Validates that `ReturnType` extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_return_type_es5() {
    let source = r#"// Custom ReturnType implementation
type MyReturnType<T extends (...args: any[]) => any> = T extends (...args: any[]) => infer R ? R : never;

// Functions to extract return types from
function getString(): string {
    return "hello";
}

function getNumber(): number {
    return 42;
}

async function getAsyncData(): Promise<{ id: number; name: string }> {
    return { id: 1, name: "test" };
}

function getCallback(): (x: number) => boolean {
    return (x) => x > 0;
}

// Using ReturnType
type StringResult = ReturnType<typeof getString>;
type NumberResult = ReturnType<typeof getNumber>;
type AsyncResult = ReturnType<typeof getAsyncData>;
type CallbackResult = MyReturnType<typeof getCallback>;

// Functions that use extracted types
function processString(value: StringResult): void {
    console.log(value.toUpperCase());
}

function processNumber(value: NumberResult): void {
    console.log(value.toFixed(2));
}

// Generic wrapper using ReturnType
function wrapResult<T extends (...args: any[]) => any>(
    fn: T
): { result: ReturnType<T>; timestamp: Date } | null {
    try {
        return { result: fn(), timestamp: new Date() };
    } catch {
        return null;
    }
}

const wrapped = wrapResult(getString);
processString("test");
processNumber(123);"#;

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
        output.contains("getString"),
        "expected getString function in output. output: {output}"
    );
    assert!(
        output.contains("wrapResult"),
        "expected wrapResult function in output. output: {output}"
    );
    assert!(
        output.contains("processString"),
        "expected processString function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ReturnType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Parameters<T> utility type in ES5 output.
/// Validates that Parameters extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_parameters_es5() {
    let source = r#"// Custom Parameters implementation
type MyParameters<T extends (...args: any[]) => any> = T extends (...args: infer P) => any ? P : never;

// Functions with various parameter signatures
function simpleFunc(a: string, b: number): void {
    console.log(a, b);
}

function optionalFunc(required: string, optional?: number): boolean {
    return optional !== undefined;
}

function restFunc(first: string, ...rest: number[]): number {
    return rest.reduce((sum, n) => sum + n, 0);
}

function complexFunc(
    config: { host: string; port: number },
    callback: (err: Error | null, result: string) => void
): void {
    callback(null, `${config.host}:${config.port}`);
}

// Using Parameters
type SimpleParams = Parameters<typeof simpleFunc>;
type OptionalParams = Parameters<typeof optionalFunc>;
type RestParams = MyParameters<typeof restFunc>;
type ComplexParams = Parameters<typeof complexFunc>;

// Function that forwards parameters
function forward<T extends (...args: any[]) => any>(
    fn: T,
    ...args: Parameters<T>
): ReturnType<T> {
    return fn(...args);
}

// Partial application using Parameters
function partial<T extends (...args: any[]) => any>(
    fn: T,
    firstArg: Parameters<T>[0]
): (...rest: Parameters<T> extends [any, ...infer R] ? R : never[]) => ReturnType<T> {
    return (...rest) => fn(firstArg, ...rest);
}

const forwardedResult = forward(simpleFunc, "hello", 42);
const partialSimple = partial(simpleFunc, "fixed");
partialSimple(123);"#;

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
        output.contains("simpleFunc"),
        "expected simpleFunc function in output. output: {output}"
    );
    assert!(
        output.contains("forward"),
        "expected forward function in output. output: {output}"
    );
    assert!(
        output.contains("partial"),
        "expected partial function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Parameters utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for `ConstructorParameters`<T> utility type in ES5 output.
/// Validates that `ConstructorParameters` extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_constructor_params_es5() {
    let source = r#"// Custom ConstructorParameters implementation
type MyConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;

// Classes with various constructor signatures
class SimpleClass {
    constructor(public name: string, public age: number) {}
}

class ConfigurableClass {
    constructor(
        public config: { host: string; port: number },
        public options?: { timeout?: number; retries?: number }
    ) {}
}

class VariadicClass {
    public items: string[];
    constructor(first: string, ...rest: string[]) {
        this.items = [first, ...rest];
    }
}

// Using ConstructorParameters
type SimpleCtorParams = ConstructorParameters<typeof SimpleClass>;
type ConfigCtorParams = ConstructorParameters<typeof ConfigurableClass>;
type VariadicCtorParams = MyConstructorParameters<typeof VariadicClass>;

// Factory function using ConstructorParameters
function createInstance<T extends new (...args: any[]) => any>(
    ctor: T,
    ...args: ConstructorParameters<T>
): InstanceType<T> {
    return new ctor(...args);
}

// Builder pattern with ConstructorParameters
class Factory<T extends new (...args: any[]) => any> {
    private args: ConstructorParameters<T> | null = null;

    constructor(private ctor: T) {}

    withArgs(...args: ConstructorParameters<T>): this {
        this.args = args;
        return this;
    }

    build(): InstanceType<T> {
        if (!this.args) throw new Error("Args not set");
        return new this.ctor(...this.args);
    }
}

const simple = createInstance(SimpleClass, "Alice", 30);
const factory = new Factory(ConfigurableClass);
const configured = factory.withArgs({ host: "localhost", port: 8080 }).build();
console.log(simple, configured);"#;

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
        output.contains("SimpleClass"),
        "expected SimpleClass in output. output: {output}"
    );
    assert!(
        output.contains("createInstance"),
        "expected createInstance function in output. output: {output}"
    );
    assert!(
        output.contains("Factory"),
        "expected Factory class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ConstructorParameters utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for `InstanceType`<T> utility type in ES5 output.
/// Validates that `InstanceType` extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_instance_type_es5() {
    let source = r#"// Custom InstanceType implementation
type MyInstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : never;

// Various classes
class User {
    constructor(public id: number, public name: string) {}

    greet(): string {
        return `Hello, ${this.name}`;
    }
}

class Product {
    constructor(
        public sku: string,
        public price: number,
        public inStock: boolean
    ) {}

    getDisplayPrice(): string {
        return `$${this.price.toFixed(2)}`;
    }
}

abstract class Entity {
    abstract getId(): string;
}

class ConcreteEntity extends Entity {
    constructor(private id: string) {
        super();
    }

    getId(): string {
        return this.id;
    }
}

// Using InstanceType
type UserInstance = InstanceType<typeof User>;
type ProductInstance = InstanceType<typeof Product>;
type EntityInstance = MyInstanceType<typeof ConcreteEntity>;

// Registry using InstanceType
class Registry<T extends new (...args: any[]) => any> {
    private instances: Map<string, InstanceType<T>> = new Map();

    register(key: string, instance: InstanceType<T>): void {
        this.instances.set(key, instance);
    }

    get(key: string): InstanceType<T> | undefined {
        return this.instances.get(key);
    }

    getAll(): InstanceType<T>[] {
        return Array.from(this.instances.values());
    }
}

const userRegistry = new Registry<typeof User>();
userRegistry.register("user1", new User(1, "Alice"));
userRegistry.register("user2", new User(2, "Bob"));

const users = userRegistry.getAll();
console.log(users.map(u => u.greet()));"#;

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
        output.contains("User"),
        "expected User class in output. output: {output}"
    );
    assert!(
        output.contains("Product"),
        "expected Product class in output. output: {output}"
    );
    assert!(
        output.contains("Registry"),
        "expected Registry class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for InstanceType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for `ThisParameterType`<T> utility type in ES5 output.
/// Validates that `ThisParameterType` extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_this_parameter_es5() {
    let source = r#"// Custom ThisParameterType implementation
type MyThisParameterType<T> = T extends (this: infer U, ...args: any[]) => any ? U : unknown;

// Custom OmitThisParameter implementation
type MyOmitThisParameter<T> = unknown extends ThisParameterType<T>
    ? T
    : T extends (...args: infer A) => infer R
    ? (...args: A) => R
    : T;

// Functions with explicit this parameter
function greet(this: { name: string }): string {
    return `Hello, ${this.name}`;
}

function calculate(this: { multiplier: number }, value: number): number {
    return value * this.multiplier;
}

function processItems(
    this: { prefix: string },
    items: string[]
): string[] {
    return items.map(item => `${this.prefix}: ${item}`);
}

// Using ThisParameterType
type GreetThis = ThisParameterType<typeof greet>;
type CalculateThis = ThisParameterType<typeof calculate>;
type ProcessThis = MyThisParameterType<typeof processItems>;

// Binding functions with correct this
function bindThis<T, A extends any[], R>(
    fn: (this: T, ...args: A) => R,
    thisArg: T
): (...args: A) => R {
    return fn.bind(thisArg);
}

const context = { name: "Alice", multiplier: 2, prefix: "Item" };
const boundGreet = bindThis(greet, context);
const boundCalculate = bindThis(calculate, context);

// Method extraction with this handling
class Counter {
    count = 0;

    increment(this: Counter): void {
        this.count++;
    }

    getCount(this: Counter): number {
        return this.count;
    }
}

const counter = new Counter();
const incrementFn: MyOmitThisParameter<typeof counter.increment> = counter.increment.bind(counter);
incrementFn();

console.log(boundGreet(), boundCalculate(5), counter.getCount());"#;

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
        output.contains("greet"),
        "expected greet function in output. output: {output}"
    );
    assert!(
        output.contains("bindThis"),
        "expected bindThis function in output. output: {output}"
    );
    assert!(
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ThisParameterType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple utility type patterns.
/// Tests `ReturnType`, Parameters, `ConstructorParameters`, `InstanceType`, and `ThisParameterType` together.
#[test]
fn test_source_map_utility_type_es5_comprehensive() {
    let source = r#"// Comprehensive utility type patterns

// All utility types defined
type ReturnType<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : never;
type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;
type ConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : never;
type ThisParameterType<T> = T extends (this: infer U, ...args: any[]) => any ? U : unknown;
type OmitThisParameter<T> = unknown extends ThisParameterType<T>
    ? T
    : T extends (...args: infer A) => infer R ? (...args: A) => R : T;

// Service class using utility types
class ApiService {
    private baseUrl: string;

    constructor(baseUrl: string, private timeout: number = 5000) {
        this.baseUrl = baseUrl;
    }

    async fetch<T>(endpoint: string): Promise<T> {
        const response = await fetch(`${this.baseUrl}${endpoint}`);
        return response.json();
    }

    post<T, R>(endpoint: string, data: T): Promise<R> {
        return fetch(`${this.baseUrl}${endpoint}`, {
            method: "POST",
            body: JSON.stringify(data)
        }).then(r => r.json());
    }
}

// Dependency injection container using utility types
class Container {
    private factories: Map<string, (...args: any[]) => any> = new Map();
    private instances: Map<string, any> = new Map();

    register<T extends new (...args: any[]) => any>(
        key: string,
        ctor: T,
        ...args: ConstructorParameters<T>
    ): void {
        this.factories.set(key, () => new ctor(...args));
    }

    resolve<T extends new (...args: any[]) => any>(key: string): InstanceType<T> {
        if (!this.instances.has(key)) {
            const factory = this.factories.get(key);
            if (factory) {
                this.instances.set(key, factory());
            }
        }
        return this.instances.get(key);
    }
}

// Function composition using Parameters and ReturnType
function compose<
    F extends (...args: any[]) => any,
    G extends (arg: ReturnType<F>) => any
>(f: F, g: G): (...args: Parameters<F>) => ReturnType<G> {
    return (...args) => g(f(...args));
}

// Method decorator factory using utility types
function logMethod<T extends (...args: any[]) => any>(
    target: any,
    propertyKey: string,
    descriptor: TypedPropertyDescriptor<T>
): void {
    const original = descriptor.value!;
    descriptor.value = function(this: ThisParameterType<T>, ...args: Parameters<T>): ReturnType<T> {
        console.log(`Calling ${propertyKey} with`, args);
        return original.apply(this, args);
    } as T;
}

// Event handler with proper typing
interface EventMap {
    click: { x: number; y: number };
    keypress: { key: string; code: number };
    submit: { data: Record<string, string> };
}

class EventEmitter<T extends Record<string, any>> {
    private handlers: Map<keyof T, ((event: any) => void)[]> = new Map();

    on<K extends keyof T>(
        event: K,
        handler: (event: T[K]) => void
    ): void {
        if (!this.handlers.has(event)) {
            this.handlers.set(event, []);
        }
        this.handlers.get(event)!.push(handler);
    }

    emit<K extends keyof T>(event: K, data: T[K]): void {
        const handlers = this.handlers.get(event) || [];
        handlers.forEach(h => h(data));
    }
}

// Usage
const container = new Container();
container.register("api", ApiService, "https://api.example.com", 3000);
const api = container.resolve<typeof ApiService>("api");

const double = (x: number) => x * 2;
const stringify = (x: number) => x.toString();
const doubleAndStringify = compose(double, stringify);

const emitter = new EventEmitter<EventMap>();
emitter.on("click", (e) => console.log(e.x, e.y));
emitter.emit("click", { x: 10, y: 20 });

console.log(api, doubleAndStringify(21));"#;

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
        output.contains("ApiService"),
        "expected ApiService class in output. output: {output}"
    );
    assert!(
        output.contains("Container"),
        "expected Container class in output. output: {output}"
    );
    assert!(
        output.contains("compose"),
        "expected compose function in output. output: {output}"
    );
    assert!(
        output.contains("EventEmitter"),
        "expected EventEmitter class in output. output: {output}"
    );
    assert!(
        output.contains("register"),
        "expected register method in output. output: {output}"
    );
    assert!(
        output.contains("resolve"),
        "expected resolve method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive utility types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
