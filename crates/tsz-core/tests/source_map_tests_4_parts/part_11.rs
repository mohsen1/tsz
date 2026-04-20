#[test]
fn test_source_map_decorator_composition_es5_metadata() {
    let source = r#"const metadata = new Map<any, Map<string, any>>();

function setMetadata(key: string, value: any) {
    return function(target: any, propertyKey?: string) {
        let targetMeta = metadata.get(target) || new Map();
        targetMeta.set(key, value);
        metadata.set(target, targetMeta);
    };
}

function getMetadata(key: string, target: any): any {
    const targetMeta = metadata.get(target);
    return targetMeta?.get(key);
}

@setMetadata("version", "1.0.0")
@setMetadata("author", "Team")
class Component {
    @setMetadata("required", true)
    name: string = "";

    @setMetadata("type", "handler")
    @setMetadata("async", true)
    async handleEvent(event: any) {
        console.log(event);
    }
}

const comp = new Component();
console.log(getMetadata("version", Component));
console.log(getMetadata("author", Component));"#;

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
        output.contains("Component"),
        "expected Component in output. output: {output}"
    );
    assert!(
        output.contains("metadata"),
        "expected metadata in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for metadata decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

