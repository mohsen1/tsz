#[test]
fn test_source_map_decorator_composition_es5_method_params() {
    let source = r#"const paramMetadata = new Map<any, Map<string, any[]>>();

function param(name: string) {
    return function(target: any, methodKey: string, paramIndex: number) {
        let methodMeta = paramMetadata.get(target) || new Map();
        let params = methodMeta.get(methodKey) || [];
        params[paramIndex] = name;
        methodMeta.set(methodKey, params);
        paramMetadata.set(target, methodMeta);
    };
}

function inject(token: string) {
    return function(target: any, methodKey: string, paramIndex: number) {
        console.log("Inject", token, "at index", paramIndex);
    };
}

function validate(schema: any) {
    return function(target: any, methodKey: string, paramIndex: number) {
        console.log("Validate param", paramIndex, "with schema");
    };
}

class UserService {
    createUser(
        @param("name") @validate({ type: "string" }) name: string,
        @param("email") @validate({ type: "email" }) email: string,
        @param("age") @validate({ type: "number" }) age: number
    ) {
        return { name, email, age };
    }

    findUser(@inject("db") db: any, @param("id") id: number) {
        return db.find(id);
    }
}

const userService = new UserService();
console.log(userService.createUser("John", "john@example.com", 30));"#;

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
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("createUser"),
        "expected createUser in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method param decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_accessor() {
    let source = r#"function observable(target: any, key: string, descriptor: PropertyDescriptor) {
    const getter = descriptor.get;
    const setter = descriptor.set;

    if (setter) {
        descriptor.set = function(value: any) {
            console.log("Setting", key, "to", value);
            setter.call(this, value);
        };
    }

    if (getter) {
        descriptor.get = function() {
            const value = getter.call(this);
            console.log("Getting", key, "=", value);
            return value;
        };
    }

    return descriptor;
}

function lazy(target: any, key: string, descriptor: PropertyDescriptor) {
    const getter = descriptor.get;
    const cacheKey = Symbol(key);

    descriptor.get = function() {
        if (!(this as any)[cacheKey]) {
            (this as any)[cacheKey] = getter?.call(this);
        }
        return (this as any)[cacheKey];
    };

    return descriptor;
}

class Config {
    private _value = 0;

    @observable
    @lazy
    get computedValue() {
        return this._value * 2;
    }

    @observable
    set value(v: number) {
        this._value = v;
    }

    get value() {
        return this._value;
    }
}

const config = new Config();
config.value = 5;
console.log(config.computedValue);"#;

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
        output.contains("Config"),
        "expected Config in output. output: {output}"
    );
    assert!(
        output.contains("observable"),
        "expected observable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_multiple_targets() {
    let source = r#"function sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

function enumerable(value: boolean) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        descriptor.enumerable = value;
        return descriptor;
    };
}

function readonly(target: any, key: string) {
    Object.defineProperty(target, key, { writable: false });
}

@sealed
class Person {
    @readonly
    id: number = 1;

    name: string = "";

    @enumerable(false)
    get fullInfo() {
        return this.id + ": " + this.name;
    }

    @enumerable(true)
    greet() {
        return "Hello, " + this.name;
    }
}

const person = new Person();
person.name = "Alice";
console.log(person.greet());"#;

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
        output.contains("Person"),
        "expected Person in output. output: {output}"
    );
    assert!(
        output.contains("sealed"),
        "expected sealed in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple target decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_conditional() {
    let source = r#"const isProduction = false;
const enableLogging = true;

function conditionalLog(target: any, key: string, descriptor: PropertyDescriptor) {
    if (!enableLogging) return descriptor;

    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Calling:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function devOnly(target: any, key: string, descriptor: PropertyDescriptor) {
    if (isProduction) {
        descriptor.value = function() {
            throw new Error("Not available in production");
        };
    }
    return descriptor;
}

function deprecated(message: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.warn("Deprecated:", message);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

class FeatureService {
    @conditionalLog
    @devOnly
    debugInfo() {
        return { env: "development", debug: true };
    }

    @conditionalLog
    @deprecated("Use newMethod instead")
    oldMethod() {
        return "old";
    }

    @conditionalLog
    newMethod() {
        return "new";
    }
}

const service = new FeatureService();
console.log(service.debugInfo());
console.log(service.newMethod());"#;

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
        output.contains("FeatureService"),
        "expected FeatureService in output. output: {output}"
    );
    assert!(
        output.contains("deprecated"),
        "expected deprecated in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_generic() {
    let source = r#"function typed<T>(type: new () => T) {
    return function(target: any, key: string) {
        console.log("Property", key, "is typed as", type.name);
    };
}

function transform<T, U>(fn: (value: T) => U) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(input: T): U {
            const result = original.call(this, input);
            return fn(result);
        };
        return descriptor;
    };
}

function collect<T>() {
    const items: T[] = [];
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]): T {
            const result = original.apply(this, args);
            items.push(result);
            return result;
        };
        return descriptor;
    };
}

class DataProcessor {
    @typed(String)
    name: string = "";

    @transform((x: number) => x.toString())
    @collect<string>()
    process(value: number) {
        return value * 2;
    }
}

const processor = new DataProcessor();
console.log(processor.process(5));"#;

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
        output.contains("DataProcessor"),
        "expected DataProcessor in output. output: {output}"
    );
    assert!(
        output.contains("transform"),
        "expected transform in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_inheritance() {
    let source = r#"function logMethod(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Method:", key, "called on", this.constructor.name);
        return original.apply(this, args);
    };
    return descriptor;
}

function track(target: any, key: string) {
    console.log("Tracking property:", key);
}

abstract class BaseEntity {
    @track
    id: number = 0;

    @logMethod
    save() {
        console.log("Saving entity", this.id);
    }
}

class User extends BaseEntity {
    @track
    name: string = "";

    @logMethod
    save() {
        console.log("Saving user", this.name);
        super.save();
    }

    @logMethod
    validate() {
        return this.name.length > 0;
    }
}

class Admin extends User {
    @track
    role: string = "admin";

    @logMethod
    save() {
        console.log("Saving admin with role", this.role);
        super.save();
    }
}

const admin = new Admin();
admin.name = "John";
admin.save();"#;

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
        output.contains("BaseEntity"),
        "expected BaseEntity in output. output: {output}"
    );
    assert!(
        output.contains("User"),
        "expected User in output. output: {output}"
    );
    assert!(
        output.contains("Admin"),
        "expected Admin in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for inheritance decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_comprehensive() {
    let source = r#"// Comprehensive decorator composition patterns for ES5 transform testing

// Metadata storage
const metadata = new Map<any, Map<string, any>>();

// Basic decorators
function log(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Call:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

// Factory decorators
function prefix(p: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            return p + original.apply(this, args);
        };
        return descriptor;
    };
}

function retry(times: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = async function(...args: any[]) {
            for (let i = 0; i < times; i++) {
                try { return await original.apply(this, args); }
                catch (e) { if (i === times - 1) throw e; }
            }
        };
        return descriptor;
    };
}

// Metadata decorators
function setMeta(key: string, value: any) {
    return function(target: any) {
        let targetMeta = metadata.get(target) || new Map();
        targetMeta.set(key, value);
        metadata.set(target, targetMeta);
    };
}

// Property decorator
function track(target: any, key: string) {
    console.log("Tracking:", key);
}

// Parameter decorator
function param(name: string) {
    return function(target: any, methodKey: string, index: number) {
        console.log("Param", name, "at", index);
    };
}

// Accessor decorator
function observable(target: any, key: string, descriptor: PropertyDescriptor) {
    const getter = descriptor.get;
    descriptor.get = function() {
        console.log("Access:", key);
        return getter?.call(this);
    };
    return descriptor;
}

// Comprehensive class with all decorator types
@sealed
@setMeta("version", "1.0")
@setMeta("author", "Team")
class ComplexService {
    @track
    id: number = 0;

    @track
    name: string = "";

    private _status = "idle";

    @observable
    get status() {
        return this._status;
    }

    set status(v: string) {
        this._status = v;
    }

    @log
    @prefix("[INFO] ")
    getMessage() {
        return "Hello from " + this.name;
    }

    @log
    @retry(3)
    async fetchData(@param("url") url: string) {
        return { url, data: "result" };
    }

    @log
    process(
        @param("input") input: string,
        @param("options") options: any
    ) {
        return input.toUpperCase();
    }
}

// Inheritance with decorators
abstract class BaseService {
    @track
    baseId: number = 0;

    @log
    init() {
        console.log("Init base");
    }
}

class ChildService extends BaseService {
    @track
    childId: number = 0;

    @log
    init() {
        super.init();
        console.log("Init child");
    }
}

// Usage
const service = new ComplexService();
service.name = "Test";
console.log(service.getMessage());
console.log(service.status);

const child = new ChildService();
child.init();"#;

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
        output.contains("ComplexService"),
        "expected ComplexService in output. output: {output}"
    );
    assert!(
        output.contains("BaseService"),
        "expected BaseService in output. output: {output}"
    );
    assert!(
        output.contains("ChildService"),
        "expected ChildService in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log decorator in output. output: {output}"
    );
    assert!(
        output.contains("sealed"),
        "expected sealed decorator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive decorator composition"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Private Method Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_private_method_es5_instance_basic() {
    let source = r#"class Calculator {
    #add(a: number, b: number): number {
        return a + b;
    }

    #subtract(a: number, b: number): number {
        return a - b;
    }

    #multiply(a: number, b: number): number {
        return a * b;
    }

    calculate(op: string, a: number, b: number): number {
        switch (op) {
            case "+": return this.#add(a, b);
            case "-": return this.#subtract(a, b);
            case "*": return this.#multiply(a, b);
            default: return 0;
        }
    }
}

const calc = new Calculator();
console.log(calc.calculate("+", 5, 3));
console.log(calc.calculate("-", 10, 4));
console.log(calc.calculate("*", 6, 7));"#;

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
        output.contains("Calculator"),
        "expected Calculator in output. output: {output}"
    );
    assert!(
        output.contains("calculate"),
        "expected calculate in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_static() {
    let source = r#"class IdGenerator {
    static #counter = 0;

    static #generateId(): string {
        IdGenerator.#counter++;
        return "id_" + IdGenerator.#counter;
    }

    static #formatId(id: string): string {
        return "[" + id + "]";
    }

    static #validateId(id: string): boolean {
        return id.startsWith("id_");
    }

    static createId(): string {
        const id = IdGenerator.#generateId();
        if (IdGenerator.#validateId(id)) {
            return IdGenerator.#formatId(id);
        }
        return "";
    }

    static getCount(): number {
        return IdGenerator.#counter;
    }
}

console.log(IdGenerator.createId());
console.log(IdGenerator.createId());
console.log(IdGenerator.getCount());"#;

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
        output.contains("IdGenerator"),
        "expected IdGenerator in output. output: {output}"
    );
    assert!(
        output.contains("createId"),
        "expected createId in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_accessor() {
    let source = r#"class SecureData {
    #data: string = "";
    #accessCount = 0;

    get #internalData(): string {
        this.#accessCount++;
        return this.#data;
    }

    set #internalData(value: string) {
        this.#data = value.trim();
    }

    get value(): string {
        return this.#internalData;
    }

    set value(v: string) {
        this.#internalData = v;
    }

    get accessCount(): number {
        return this.#accessCount;
    }
}

class CachedValue {
    #cache: Map<string, any> = new Map();

    get #cacheSize(): number {
        return this.#cache.size;
    }

    #getCached(key: string): any {
        return this.#cache.get(key);
    }

    #setCached(key: string, value: any): void {
        this.#cache.set(key, value);
    }

    store(key: string, value: any): void {
        this.#setCached(key, value);
    }

    retrieve(key: string): any {
        return this.#getCached(key);
    }

    get size(): number {
        return this.#cacheSize;
    }
}

const data = new SecureData();
data.value = "  hello  ";
console.log(data.value);
console.log(data.accessCount);

const cache = new CachedValue();
cache.store("key1", "value1");
console.log(cache.retrieve("key1"));
console.log(cache.size);"#;

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
        output.contains("SecureData"),
        "expected SecureData in output. output: {output}"
    );
    assert!(
        output.contains("CachedValue"),
        "expected CachedValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private accessor methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_inheritance() {
    let source = r#"class BaseLogger {
    #prefix = "[BASE]";

    #formatMessage(msg: string): string {
        return this.#prefix + " " + msg;
    }

    log(msg: string): void {
        console.log(this.#formatMessage(msg));
    }
}

class ChildLogger extends BaseLogger {
    #childPrefix = "[CHILD]";

    #formatChildMessage(msg: string): string {
        return this.#childPrefix + " " + msg;
    }

    logChild(msg: string): void {
        console.log(this.#formatChildMessage(msg));
    }

    logBoth(msg: string): void {
        this.log(msg);
        this.logChild(msg);
    }
}

class GrandchildLogger extends ChildLogger {
    #level = "DEBUG";

    #addLevel(msg: string): string {
        return "[" + this.#level + "] " + msg;
    }

    debug(msg: string): void {
        const formatted = this.#addLevel(msg);
        this.logBoth(formatted);
    }
}

const logger = new GrandchildLogger();
logger.debug("Test message");"#;

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
        output.contains("BaseLogger"),
        "expected BaseLogger in output. output: {output}"
    );
    assert!(
        output.contains("ChildLogger"),
        "expected ChildLogger in output. output: {output}"
    );
    assert!(
        output.contains("GrandchildLogger"),
        "expected GrandchildLogger in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_async() {
    let source = r#"class AsyncService {
    #baseUrl = "https://api.example.com";

    async #fetch(endpoint: string): Promise<any> {
        const url = this.#baseUrl + endpoint;
        const response = await fetch(url);
        return response.json();
    }

    async #processData(data: any): Promise<any> {
        await new Promise(r => setTimeout(r, 100));
        return { processed: true, data };
    }

    async #validate(data: any): Promise<boolean> {
        return data != null;
    }

    async getData(endpoint: string): Promise<any> {
        const raw = await this.#fetch(endpoint);
        if (await this.#validate(raw)) {
            return this.#processData(raw);
        }
        return null;
    }
}

class AsyncQueue {
    #queue: Promise<void> = Promise.resolve();

    async #runTask(task: () => Promise<void>): Promise<void> {
        await task();
    }

    async enqueue(task: () => Promise<void>): Promise<void> {
        this.#queue = this.#queue.then(() => this.#runTask(task));
        await this.#queue;
    }
}

const service = new AsyncService();
service.getData("/users").then(console.log);

const queue = new AsyncQueue();
queue.enqueue(async () => console.log("Task 1"));
queue.enqueue(async () => console.log("Task 2"));"#;

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
        output.contains("AsyncService"),
        "expected AsyncService in output. output: {output}"
    );
    assert!(
        output.contains("AsyncQueue"),
        "expected AsyncQueue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_generator() {
    let source = r#"class NumberGenerator {
    #start: number;
    #end: number;

    constructor(start: number, end: number) {
        this.#start = start;
        this.#end = end;
    }

    *#range(): Generator<number> {
        for (let i = this.#start; i <= this.#end; i++) {
            yield i;
        }
    }

    *#evens(): Generator<number> {
        for (const n of this.#range()) {
            if (n % 2 === 0) yield n;
        }
    }

    *#odds(): Generator<number> {
        for (const n of this.#range()) {
            if (n % 2 !== 0) yield n;
        }
    }

    getEvens(): number[] {
        return [...this.#evens()];
    }

    getOdds(): number[] {
        return [...this.#odds()];
    }

    getAll(): number[] {
        return [...this.#range()];
    }
}

const gen = new NumberGenerator(1, 10);
console.log(gen.getAll());
console.log(gen.getEvens());
console.log(gen.getOdds());"#;

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
        output.contains("NumberGenerator"),
        "expected NumberGenerator in output. output: {output}"
    );
    assert!(
        output.contains("getEvens"),
        "expected getEvens in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_with_fields() {
    let source = r#"class BankAccount {
    #balance: number = 0;
    #transactions: string[] = [];
    #accountId: string;

    constructor(id: string, initial: number) {
        this.#accountId = id;
        this.#balance = initial;
        this.#logTransaction("INIT", initial);
    }

    #logTransaction(type: string, amount: number): void {
        this.#transactions.push(type + ": " + amount);
    }

    #validateAmount(amount: number): boolean {
        return amount > 0;
    }

    #canWithdraw(amount: number): boolean {
        return this.#balance >= amount;
    }

    deposit(amount: number): boolean {
        if (!this.#validateAmount(amount)) return false;
        this.#balance += amount;
        this.#logTransaction("DEP", amount);
        return true;
    }

    withdraw(amount: number): boolean {
        if (!this.#validateAmount(amount)) return false;
        if (!this.#canWithdraw(amount)) return false;
        this.#balance -= amount;
        this.#logTransaction("WTH", amount);
        return true;
    }

    getBalance(): number {
        return this.#balance;
    }

    getHistory(): string[] {
        return [...this.#transactions];
    }
}

const account = new BankAccount("ACC001", 100);
account.deposit(50);
account.withdraw(30);
console.log(account.getBalance());
console.log(account.getHistory());"#;

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
        output.contains("BankAccount"),
        "expected BankAccount in output. output: {output}"
    );
    assert!(
        output.contains("deposit"),
        "expected deposit in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_chained_calls() {
    let source = r#"class StringBuilder {
    #value = "";

    #append(str: string): this {
        this.#value += str;
        return this;
    }

    #prepend(str: string): this {
        this.#value = str + this.#value;
        return this;
    }

    #wrap(prefix: string, suffix: string): this {
        this.#value = prefix + this.#value + suffix;
        return this;
    }

    #transform(fn: (s: string) => string): this {
        this.#value = fn(this.#value);
        return this;
    }

    add(str: string): this {
        return this.#append(str);
    }

    addBefore(str: string): this {
        return this.#prepend(str);
    }

    surround(prefix: string, suffix: string): this {
        return this.#wrap(prefix, suffix);
    }

    apply(fn: (s: string) => string): this {
        return this.#transform(fn);
    }

    build(): string {
        return this.#value;
    }
}

const result = new StringBuilder()
    .add("Hello")
    .add(" ")
    .add("World")
    .surround("[", "]")
    .apply(s => s.toUpperCase())
    .build();

console.log(result);"#;

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
        output.contains("StringBuilder"),
        "expected StringBuilder in output. output: {output}"
    );
    assert!(
        output.contains("build"),
        "expected build in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained private method calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_parameters() {
    let source = r#"class ParameterHandler {
    #processDefaults(a: number = 0, b: string = "default"): string {
        return b + ": " + a;
    }

    #processRest(...items: number[]): number {
        return items.reduce((sum, n) => sum + n, 0);
    }

    #processDestructured({ x, y }: { x: number; y: number }): number {
        return x + y;
    }

    #processArrayDestructured([first, second]: [string, string]): string {
        return first + " and " + second;
    }

    #processGeneric<T>(value: T, transform: (v: T) => string): string {
        return transform(value);
    }

    withDefaults(a?: number, b?: string): string {
        return this.#processDefaults(a, b);
    }

    withRest(...nums: number[]): number {
        return this.#processRest(...nums);
    }

    withObject(obj: { x: number; y: number }): number {
        return this.#processDestructured(obj);
    }

    withArray(arr: [string, string]): string {
        return this.#processArrayDestructured(arr);
    }

    withGeneric<T>(val: T, fn: (v: T) => string): string {
        return this.#processGeneric(val, fn);
    }
}

const handler = new ParameterHandler();
console.log(handler.withDefaults());
console.log(handler.withDefaults(42, "custom"));
console.log(handler.withRest(1, 2, 3, 4, 5));
console.log(handler.withObject({ x: 10, y: 20 }));
console.log(handler.withArray(["hello", "world"]));
console.log(handler.withGeneric(123, n => n.toString()));"#;

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
        output.contains("ParameterHandler"),
        "expected ParameterHandler in output. output: {output}"
    );
    assert!(
        output.contains("withDefaults"),
        "expected withDefaults in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

