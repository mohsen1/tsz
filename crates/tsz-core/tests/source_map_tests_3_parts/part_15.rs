#[test]
fn test_source_map_decorator_es5_method_with_descriptor() {
    let source = r#"function Log(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log(`Calling ${propertyKey} with`, args);
        return original.apply(this, args);
    };
    return descriptor;
}

class Calculator {
    @Log
    add(a: number, b: number): number {
        return a + b;
    }

    @Log
    multiply(a: number, b: number): number {
        return a * b;
    }
}

const calc = new Calculator();
calc.add(2, 3);"#;

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
        output.contains("Calculator") || output.contains("add"),
        "expected output to contain Calculator class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method decorator with descriptor"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_property_validation() {
    let source = r#"function MinLength(min: number) {
    return function(target: any, propertyKey: string) {
        let value: string;
        Object.defineProperty(target, propertyKey, {
            get: () => value,
            set: (newValue: string) => {
                if (newValue.length < min) {
                    throw new Error(`${propertyKey} must be at least ${min} chars`);
                }
                value = newValue;
            }
        });
    };
}

class User {
    @MinLength(3)
    username: string;

    @MinLength(8)
    password: string;

    constructor(username: string, password: string) {
        this.username = username;
        this.password = password;
    }
}

const user = new User("john", "password123");"#;

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
        output.contains("User") || output.contains("MinLength"),
        "expected output to contain User class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property validation decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_parameter_injection() {
    let source = r#"const INJECT_KEY = Symbol('inject');

function Inject(token: string) {
    return function(target: any, propertyKey: string | symbol, parameterIndex: number) {
        const existing = Reflect.getMetadata(INJECT_KEY, target, propertyKey) || [];
        existing.push({ index: parameterIndex, token });
        Reflect.defineMetadata(INJECT_KEY, existing, target, propertyKey);
    };
}

class Database {
    query(sql: string): any[] { return []; }
}

class Logger {
    log(msg: string): void { console.log(msg); }
}

class UserService {
    constructor(
        @Inject('Database') private db: Database,
        @Inject('Logger') private logger: Logger
    ) {}

    getUsers(): any[] {
        this.logger.log('Fetching users');
        return this.db.query('SELECT * FROM users');
    }
}"#;

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
        output.contains("UserService") || output.contains("Inject"),
        "expected output to contain UserService class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parameter injection decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_factory_chain() {
    let source = r#"function Memoize() {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const cache = new Map();
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const cacheKey = JSON.stringify(args);
            if (!cache.has(cacheKey)) {
                cache.set(cacheKey, original.apply(this, args));
            }
            return cache.get(cacheKey);
        };
    };
}

function Throttle(ms: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        let lastCall = 0;
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const now = Date.now();
            if (now - lastCall >= ms) {
                lastCall = now;
                return original.apply(this, args);
            }
        };
    };
}

function Bind() {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        return {
            get() {
                return descriptor.value.bind(this);
            }
        };
    };
}

class ApiClient {
    @Memoize()
    @Throttle(1000)
    @Bind()
    fetchData(url: string): Promise<any> {
        return fetch(url).then(r => r.json());
    }
}"#;

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
        output.contains("ApiClient") || output.contains("fetchData"),
        "expected output to contain ApiClient class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for decorator factory chain"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_accessor_readonly() {
    let source = r#"function Readonly(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    descriptor.writable = false;
    return descriptor;
}

function Enumerable(value: boolean) {
    return function(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
        descriptor.enumerable = value;
        return descriptor;
    };
}

class Config {
    private _apiKey: string = '';

    @Readonly
    @Enumerable(false)
    get apiKey(): string {
        return this._apiKey;
    }

    set apiKey(value: string) {
        this._apiKey = value;
    }
}

const config = new Config();
config.apiKey = 'secret-key';"#;

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
        output.contains("Config") || output.contains("apiKey"),
        "expected output to contain Config class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor readonly decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_abstract_class() {
    let source = r#"function Sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

function Abstract(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    descriptor.value = function() {
        throw new Error('Abstract method must be implemented');
    };
    return descriptor;
}

@Sealed
abstract class Animal {
    abstract name: string;

    @Abstract
    abstract makeSound(): void;

    move(distance: number): void {
        console.log(`Moving ${distance} meters`);
    }
}

class Dog extends Animal {
    name = 'Dog';

    makeSound(): void {
        console.log('Bark!');
    }
}

const dog = new Dog();
dog.makeSound();"#;

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
        output.contains("Animal") || output.contains("Dog"),
        "expected output to contain Animal or Dog class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for abstract class decorator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_static_members() {
    let source = r#"function Singleton<T extends { new(...args: any[]): {} }>(constructor: T) {
    let instance: T;
    return class extends constructor {
        constructor(...args: any[]) {
            if (instance) {
                return instance;
            }
            super(...args);
            instance = this as any;
        }
    };
}

function StaticInit(target: any, propertyKey: string) {
    const init = target[propertyKey];
    target[propertyKey] = null;
    setTimeout(() => {
        target[propertyKey] = init;
    }, 0);
}

@Singleton
class Database {
    @StaticInit
    static connectionPool: any[] = [];

    static maxConnections: number = 10;

    connect(): void {
        Database.connectionPool.push({});
    }
}

const db1 = new Database();
const db2 = new Database();
console.log(db1 === db2);"#;

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
        output.contains("Database") || output.contains("Singleton"),
        "expected output to contain Database class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static member decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_es5_conditional() {
    let source = r#"const DEBUG = true;

function DebugOnly(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    if (!DEBUG) {
        descriptor.value = function() {};
    }
    return descriptor;
}

function ConditionalDecorator(condition: boolean) {
    return function(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
        if (!condition) {
            return descriptor;
        }
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(`[${propertyKey}] called`);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

class Service {
    @DebugOnly
    debugInfo(): void {
        console.log('Debug info');
    }

    @ConditionalDecorator(DEBUG)
    process(data: any): any {
        return data;
    }
}

const service = new Service();
service.debugInfo();
service.process({});"#;

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
        output.contains("Service") || output.contains("DEBUG"),
        "expected output to contain Service class. output: {output}"
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
fn test_source_map_decorator_es5_comprehensive() {
    let source = r#"// Class decorator factory
function Entity(tableName: string) {
    return function<T extends { new(...args: any[]): {} }>(constructor: T) {
        return class extends constructor {
            __tableName = tableName;
        };
    };
}

// Property decorator
function Column(type: string) {
    return function(target: any, propertyKey: string) {
        Reflect.defineMetadata('column:type', type, target, propertyKey);
    };
}

// Method decorator
function Transaction(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = async function(...args: any[]) {
        console.log('BEGIN TRANSACTION');
        try {
            const result = await original.apply(this, args);
            console.log('COMMIT');
            return result;
        } catch (e) {
            console.log('ROLLBACK');
            throw e;
        }
    };
    return descriptor;
}

// Parameter decorator
function Required(target: any, propertyKey: string, parameterIndex: number) {
    const required = Reflect.getMetadata('required', target, propertyKey) || [];
    required.push(parameterIndex);
    Reflect.defineMetadata('required', required, target, propertyKey);
}

@Entity('users')
class UserRepository {
    @Column('varchar')
    name: string;

    @Column('int')
    age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }

    @Transaction
    async save(@Required entity: any): Promise<void> {
        console.log('Saving entity');
    }

    @Transaction
    async delete(@Required id: number): Promise<void> {
        console.log('Deleting entity', id);
    }
}

const repo = new UserRepository('John', 30);
repo.save({});"#;

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
        output.contains("UserRepository"),
        "expected output to contain UserRepository class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Async/Await ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_async_es5_promise_all() {
    let source = r#"async function fetchAll(urls: string[]): Promise<any[]> {
    const promises = urls.map(url => fetch(url).then(r => r.json()));
    const results = await Promise.all(promises);
    return results;
}

async function parallelFetch(): Promise<void> {
    const [users, posts, comments] = await Promise.all([
        fetch('/api/users'),
        fetch('/api/posts'),
        fetch('/api/comments')
    ]);
    console.log(users, posts, comments);
}

fetchAll(['url1', 'url2', 'url3']);"#;

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
        output.contains("fetchAll") || output.contains("Promise"),
        "expected output to contain fetchAll. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise.all"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

