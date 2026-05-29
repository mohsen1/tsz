#[test]
fn test_source_map_decorator_es5_class_with_metadata() {
    let source = r#"function Component(config: { selector: string; template: string }) {
    return function<T extends { new(...args: any[]): {} }>(constructor: T) {
        return class extends constructor {
            selector = config.selector;
            template = config.template;
        };
    };
}

@Component({
    selector: 'app-root',
    template: '<div>Hello</div>'
})
class AppComponent {
    title: string = 'My App';
}

const app = new AppComponent();"#;

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
        output.contains("AppComponent") || output.contains("Component"),
        "expected output to contain AppComponent. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class decorator with metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

#[test]
fn test_source_map_async_es5_promise_race() {
    let source = r#"async function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
    const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Timeout')), ms);
    });
    return await Promise.race([promise, timeoutPromise]);
}

async function fetchWithTimeout(url: string): Promise<any> {
    const result = await timeout(fetch(url), 5000);
    return result.json();
}

fetchWithTimeout('/api/data');"#;

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
        output.contains("timeout") || output.contains("Promise"),
        "expected output to contain timeout function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise.race"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_error_handling() {
    let source = r#"class NetworkError extends Error {
    constructor(message: string, public statusCode: number) {
        super(message);
        this.name = 'NetworkError';
    }
}

async function fetchWithRetry(url: string, retries: number = 3): Promise<any> {
    for (let i = 0; i < retries; i++) {
        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new NetworkError('Request failed', response.status);
            }
            return await response.json();
        } catch (error) {
            if (i === retries - 1) {
                throw error;
            }
            await new Promise(r => setTimeout(r, 1000 * Math.pow(2, i)));
        }
    }
}

fetchWithRetry('/api/data').catch(console.error);"#;

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
        output.contains("fetchWithRetry") || output.contains("NetworkError"),
        "expected output to contain fetchWithRetry. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for error handling"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_sequential_vs_parallel() {
    let source = r#"async function sequential(): Promise<number[]> {
    const a = await fetch('/a').then(r => r.json());
    const b = await fetch('/b').then(r => r.json());
    const c = await fetch('/c').then(r => r.json());
    return [a, b, c];
}

async function parallel(): Promise<number[]> {
    const [a, b, c] = await Promise.all([
        fetch('/a').then(r => r.json()),
        fetch('/b').then(r => r.json()),
        fetch('/c').then(r => r.json())
    ]);
    return [a, b, c];
}

async function mixed(): Promise<void> {
    const first = await fetch('/first').then(r => r.json());
    const [second, third] = await Promise.all([
        fetch('/second'),
        fetch('/third')
    ]);
    console.log(first, second, third);
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
        output.contains("sequential") || output.contains("parallel"),
        "expected output to contain sequential or parallel. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for sequential vs parallel"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_closure_capture() {
    let source = r#"function createAsyncCounter() {
    let count = 0;

    return {
        increment: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 100));
            return ++count;
        },
        decrement: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 100));
            return --count;
        },
        getCount: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 50));
            return count;
        }
    };
}

const counter = createAsyncCounter();
counter.increment().then(console.log);"#;

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
        output.contains("createAsyncCounter") || output.contains("increment"),
        "expected output to contain createAsyncCounter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for closure capture"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_inheritance() {
    let source = r#"abstract class AsyncResource {
    protected abstract load(): Promise<any>;

    async initialize(): Promise<void> {
        const data = await this.load();
        await this.process(data);
    }

    protected async process(data: any): Promise<void> {
        console.log('Processing:', data);
    }
}

class UserResource extends AsyncResource {
    protected async load(): Promise<any> {
        const response = await fetch('/api/users');
        return response.json();
    }

    protected async process(data: any): Promise<void> {
        await super.process(data);
        console.log('Users processed');
    }
}

const resource = new UserResource();
resource.initialize();"#;

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
        output.contains("AsyncResource") || output.contains("UserResource"),
        "expected output to contain AsyncResource or UserResource. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_factory_pattern() {
    let source = r#"interface Connection {
    query(sql: string): Promise<any[]>;
    close(): Promise<void>;
}

async function createConnection(config: any): Promise<Connection> {
    await new Promise(r => setTimeout(r, 100));

    return {
        query: async (sql: string): Promise<any[]> => {
            await new Promise(r => setTimeout(r, 50));
            return [{ id: 1, sql }];
        },
        close: async (): Promise<void> => {
            await new Promise(r => setTimeout(r, 50));
            console.log('Connection closed');
        }
    };
}

async function useConnection(): Promise<void> {
    const conn = await createConnection({ host: 'localhost' });
    const results = await conn.query('SELECT * FROM users');
    console.log(results);
    await conn.close();
}

useConnection();"#;

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
        output.contains("createConnection") || output.contains("useConnection"),
        "expected output to contain createConnection. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for factory pattern"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_queue_processing() {
    let source = r#"class AsyncQueue<T> {
    private queue: T[] = [];
    private processing = false;

    async add(item: T): Promise<void> {
        this.queue.push(item);
        if (!this.processing) {
            await this.process();
        }
    }

    private async process(): Promise<void> {
        this.processing = true;
        while (this.queue.length > 0) {
            const item = this.queue.shift()!;
            await this.handleItem(item);
        }
        this.processing = false;
    }

    private async handleItem(item: T): Promise<void> {
        await new Promise(r => setTimeout(r, 100));
        console.log('Processed:', item);
    }
}

const queue = new AsyncQueue<string>();
queue.add('item1');
queue.add('item2');"#;

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
        output.contains("AsyncQueue") || output.contains("process"),
        "expected output to contain AsyncQueue. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for queue processing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_event_emitter() {
    let source = r#"class AsyncEventEmitter {
    private listeners: Map<string, Array<(data: any) => Promise<void>>> = new Map();

    on(event: string, handler: (data: any) => Promise<void>): void {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event)!.push(handler);
    }

    async emit(event: string, data: any): Promise<void> {
        const handlers = this.listeners.get(event) || [];
        for (const handler of handlers) {
            await handler(data);
        }
    }

    async emitParallel(event: string, data: any): Promise<void> {
        const handlers = this.listeners.get(event) || [];
        await Promise.all(handlers.map(h => h(data)));
    }
}

const emitter = new AsyncEventEmitter();
emitter.on('data', async (d) => { console.log(d); });
emitter.emit('data', { value: 42 });"#;

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
        output.contains("AsyncEventEmitter") || output.contains("emit"),
        "expected output to contain AsyncEventEmitter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for event emitter"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_comprehensive() {
    let source = r#"// Async utility functions
const delay = (ms: number): Promise<void> =>
    new Promise(resolve => setTimeout(resolve, ms));

async function* asyncRange(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i < end; i++) {
        await delay(10);
        yield i;
    }
}

// Async class with all patterns
class DataProcessor {
    private cache = new Map<string, any>();

    constructor(private readonly baseUrl: string) {}

    async fetch(path: string): Promise<any> {
        const key = this.baseUrl + path;
        if (this.cache.has(key)) {
            return this.cache.get(key);
        }
        const response = await fetch(key);
        const data = await response.json();
        this.cache.set(key, data);
        return data;
    }

    async fetchMany(paths: string[]): Promise<any[]> {
        return Promise.all(paths.map(p => this.fetch(p)));
    }

    async *processStream(paths: string[]): AsyncGenerator<any> {
        for await (const i of asyncRange(0, paths.length)) {
            yield await this.fetch(paths[i]);
        }
    }

    async processWithRetry(path: string, retries = 3): Promise<any> {
        for (let i = 0; i < retries; i++) {
            try {
                return await this.fetch(path);
            } catch (e) {
                if (i === retries - 1) throw e;
                await delay(1000 * (i + 1));
            }
        }
    }
}

// Usage
const processor = new DataProcessor('https://api.example.com');

(async () => {
    const data = await processor.fetch('/users');
    const [users, posts] = await processor.fetchMany(['/users', '/posts']);

    for await (const item of processor.processStream(['/a', '/b', '/c'])) {
        console.log(item);
    }
})();"#;

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
        "expected output to contain DataProcessor. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Generator ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_generator_es5_control_flow() {
    let source = r#"function* controlFlowGenerator(n: number): Generator<number> {
    for (let i = 0; i < n; i++) {
        if (i % 2 === 0) {
            yield i * 2;
        } else {
            yield i * 3;
        }
    }

    let j = 0;
    while (j < 3) {
        yield j * 10;
        j++;
    }

    switch (n) {
        case 1: yield 100; break;
        case 2: yield 200; break;
        default: yield 999;
    }
}

const gen = controlFlowGenerator(5);
console.log([...gen]);"#;

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
        output.contains("controlFlowGenerator"),
        "expected output to contain controlFlowGenerator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for control flow generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_state_machine() {
    let source = r#"type State = 'idle' | 'loading' | 'success' | 'error';

function* stateMachine(): Generator<State, void, string> {
    let input: string;

    while (true) {
        yield 'idle';
        input = yield 'loading';

        if (input === 'success') {
            yield 'success';
        } else if (input === 'error') {
            yield 'error';
        }
    }
}

const machine = stateMachine();
console.log(machine.next());
console.log(machine.next());
console.log(machine.next('success'));"#;

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
        output.contains("stateMachine"),
        "expected output to contain stateMachine. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for state machine generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_finally() {
    let source = r#"function* generatorWithFinally(): Generator<number> {
    try {
        yield 1;
        yield 2;
        yield 3;
    } finally {
        console.log('Generator cleanup');
    }
}

function* nestedTryFinally(): Generator<string> {
    try {
        try {
            yield 'inner-1';
            yield 'inner-2';
        } finally {
            yield 'inner-finally';
        }
        yield 'outer-1';
    } finally {
        yield 'outer-finally';
    }
}

const gen1 = generatorWithFinally();
const gen2 = nestedTryFinally();
console.log([...gen1], [...gen2]);"#;

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
        output.contains("generatorWithFinally") || output.contains("nestedTryFinally"),
        "expected output to contain generator functions. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for finally generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_composition() {
    let source = r#"function* numbers(n: number): Generator<number> {
    for (let i = 1; i <= n; i++) {
        yield i;
    }
}

function* letters(s: string): Generator<string> {
    for (const c of s) {
        yield c;
    }
}

function* combined(): Generator<number | string> {
    yield* numbers(3);
    yield '---';
    yield* letters('abc');
    yield '---';
    yield* numbers(2);
}

function* flatten<T>(iterables: Iterable<T>[]): Generator<T> {
    for (const iterable of iterables) {
        yield* iterable;
    }
}

console.log([...combined()]);
console.log([...flatten([[1, 2], [3, 4], [5]])]);"#;

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
        output.contains("combined") || output.contains("flatten"),
        "expected output to contain composition generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for composition generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

