#[test]
fn test_source_map_decorator_metadata_es5_property_descriptors() {
    let source = r#"function Observable(): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        let value: any;
        const getter = function(this: any) {
            console.log(`Getting ${String(propertyKey)}`);
            return value;
        };
        const setter = function(this: any, newVal: any) {
            console.log(`Setting ${String(propertyKey)} to ${newVal}`);
            value = newVal;
        };
        Object.defineProperty(target, propertyKey, {
            get: getter,
            set: setter,
            enumerable: true,
            configurable: true
        });
    };
}

function DefaultValue(defaultVal: any): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        let value = defaultVal;
        Object.defineProperty(target, propertyKey, {
            get() { return value; },
            set(newVal) { value = newVal; },
            enumerable: true,
            configurable: true
        });
    };
}

function Readonly(): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        Object.defineProperty(target, propertyKey, {
            writable: false,
            configurable: false
        });
    };
}

class Config {
    @Observable()
    @DefaultValue("development")
    environment: string;

    @Observable()
    @DefaultValue(3000)
    port: number;

    @Readonly()
    version: string = "1.0.0";
}

const config = new Config();
console.log(config.environment);
config.port = 8080;
console.log(config.port);"#;

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
        "expected output to contain Config class. output: {output}"
    );
    assert!(
        output.contains("Observable"),
        "expected output to contain Observable decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_method_descriptors() {
    let source = r#"function Log(prefix: string): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(`${prefix} Calling ${String(propertyKey)} with`, args);
            const result = original.apply(this, args);
            console.log(`${prefix} Result:`, result);
            return result;
        };
        return descriptor;
    };
}

function Memoize(): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        const cache = new Map<string, any>();
        descriptor.value = function(...args: any[]) {
            const key = JSON.stringify(args);
            if (cache.has(key)) {
                return cache.get(key);
            }
            const result = original.apply(this, args);
            cache.set(key, result);
            return result;
        };
        return descriptor;
    };
}

function Throttle(ms: number): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        let lastCall = 0;
        descriptor.value = function(...args: any[]) {
            const now = Date.now();
            if (now - lastCall >= ms) {
                lastCall = now;
                return original.apply(this, args);
            }
        };
        return descriptor;
    };
}

class Calculator {
    @Log("[CALC]")
    @Memoize()
    fibonacci(n: number): number {
        if (n <= 1) return n;
        return this.fibonacci(n - 1) + this.fibonacci(n - 2);
    }

    @Log("[CALC]")
    @Throttle(1000)
    expensiveOperation(x: number): number {
        return x * x;
    }
}

const calc = new Calculator();
console.log(calc.fibonacci(10));
console.log(calc.expensiveOperation(5));"#;

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
        "expected output to contain Calculator class. output: {output}"
    );
    assert!(
        output.contains("Memoize"),
        "expected output to contain Memoize decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_accessor_descriptors() {
    let source = r#"function Enumerable(value: boolean): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        descriptor.enumerable = value;
        return descriptor;
    };
}

function Configurable(value: boolean): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        descriptor.configurable = value;
        return descriptor;
    };
}

function ValidateSet(validator: (val: any) => boolean): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        const originalSet = descriptor.set;
        if (originalSet) {
            descriptor.set = function(value: any) {
                if (!validator(value)) {
                    throw new Error(`Invalid value for ${String(propertyKey)}`);
                }
                originalSet.call(this, value);
            };
        }
        return descriptor;
    };
}

class Person {
    private _name: string = "";
    private _age: number = 0;

    @Enumerable(true)
    @Configurable(false)
    get name(): string {
        return this._name;
    }

    @ValidateSet(v => typeof v === "string" && v.length > 0)
    set name(value: string) {
        this._name = value;
    }

    @Enumerable(true)
    get age(): number {
        return this._age;
    }

    @ValidateSet(v => typeof v === "number" && v >= 0 && v <= 150)
    set age(value: number) {
        this._age = value;
    }
}

const person = new Person();
person.name = "John";
person.age = 30;
console.log(person.name, person.age);"#;

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
        "expected output to contain Person class. output: {output}"
    );
    assert!(
        output.contains("ValidateSet"),
        "expected output to contain ValidateSet decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_class_constructor() {
    let source = r#"interface ClassConstructor<T = any> {
    new (...args: any[]): T;
}

function Injectable(): ClassDecorator {
    return function<T extends ClassConstructor>(target: T) {
        // Mark class as injectable
        (target as any).__injectable__ = true;
        return target;
    };
}

function Singleton(): ClassDecorator {
    return function<T extends ClassConstructor>(target: T) {
        let instance: any = null;
        const original = target;
        const newConstructor: any = function(...args: any[]) {
            if (instance === null) {
                instance = new original(...args);
            }
            return instance;
        };
        newConstructor.prototype = original.prototype;
        Object.setPrototypeOf(newConstructor, original);
        return newConstructor;
    };
}

function Registry(name: string): ClassDecorator {
    return function<T extends ClassConstructor>(target: T) {
        const registry = (globalThis as any).__registry__ || new Map();
        registry.set(name, target);
        (globalThis as any).__registry__ = registry;
        return target;
    };
}

@Injectable()
@Singleton()
@Registry("DatabaseService")
class DatabaseService {
    private connectionString: string;

    constructor(connectionString: string = "default") {
        this.connectionString = connectionString;
        console.log("DatabaseService created");
    }

    query(sql: string): any[] {
        return [];
    }
}

@Injectable()
@Registry("UserRepository")
class UserRepository {
    constructor(private db: DatabaseService) {}

    findAll(): any[] {
        return this.db.query("SELECT * FROM users");
    }
}

const db1 = new DatabaseService("conn1");
const db2 = new DatabaseService("conn2");
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
        output.contains("DatabaseService"),
        "expected output to contain DatabaseService class. output: {output}"
    );
    assert!(
        output.contains("Singleton"),
        "expected output to contain Singleton decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class constructor metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_design_type() {
    let source = r#"// Simulating design:type, design:paramtypes, design:returntype metadata
const typeMetadata = new WeakMap<Object, Map<string, any>>();

function Type(type: any): PropertyDecorator & ParameterDecorator {
    return function(target: Object, propertyKey?: string | symbol, parameterIndex?: number) {
        if (propertyKey !== undefined) {
            if (!typeMetadata.has(target)) {
                typeMetadata.set(target, new Map());
            }
            typeMetadata.get(target)!.set(String(propertyKey), { type });
        }
    };
}

function ReturnType(type: any): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!typeMetadata.has(target)) {
            typeMetadata.set(target, new Map());
        }
        const existing = typeMetadata.get(target)!.get(String(propertyKey)) || {};
        typeMetadata.get(target)!.set(String(propertyKey), { ...existing, returnType: type });
        return descriptor;
    };
}

function ParamTypes(...types: any[]): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!typeMetadata.has(target)) {
            typeMetadata.set(target, new Map());
        }
        const existing = typeMetadata.get(target)!.get(String(propertyKey)) || {};
        typeMetadata.get(target)!.set(String(propertyKey), { ...existing, paramTypes: types });
        return descriptor;
    };
}

class Entity {
    @Type(String)
    id: string = "";

    @Type(String)
    name: string = "";

    @Type(Number)
    age: number = 0;

    @Type(Boolean)
    active: boolean = true;

    @ReturnType(String)
    @ParamTypes(String, Number)
    format(template: string, precision: number): string {
        return `${this.name} (${this.age})`;
    }
}

const entity = new Entity();
console.log(typeMetadata.get(entity));"#;

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
        output.contains("Entity"),
        "expected output to contain Entity class. output: {output}"
    );
    assert!(
        output.contains("ReturnType"),
        "expected output to contain ReturnType decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for design type metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_comprehensive() {
    let source = r#"// Comprehensive decorator metadata test combining all patterns

// Type metadata storage
const classMetadata = new WeakMap<Function, Map<string, any>>();
const propertyMetadata = new WeakMap<Object, Map<string | symbol, any>>();
const methodMetadata = new WeakMap<Object, Map<string | symbol, any>>();
const parameterMetadata = new WeakMap<Object, Map<string | symbol, Map<number, any>>>();

// Class decorators
function Controller(path: string): ClassDecorator {
    return function(target: Function) {
        if (!classMetadata.has(target)) {
            classMetadata.set(target, new Map());
        }
        classMetadata.get(target)!.set("path", path);
        classMetadata.get(target)!.set("type", "controller");
    };
}

function Service(): ClassDecorator {
    return function(target: Function) {
        if (!classMetadata.has(target)) {
            classMetadata.set(target, new Map());
        }
        classMetadata.get(target)!.set("type", "service");
        classMetadata.get(target)!.set("injectable", true);
    };
}

// Property decorators
function Column(options?: { type?: string; nullable?: boolean }): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        if (!propertyMetadata.has(target)) {
            propertyMetadata.set(target, new Map());
        }
        propertyMetadata.get(target)!.set(propertyKey, { column: true, ...options });
    };
}

// Method decorators
function Get(path: string): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!methodMetadata.has(target)) {
            methodMetadata.set(target, new Map());
        }
        methodMetadata.get(target)!.set(propertyKey, { method: "GET", path });
        return descriptor;
    };
}

function Post(path: string): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (!methodMetadata.has(target)) {
            methodMetadata.set(target, new Map());
        }
        methodMetadata.get(target)!.set(propertyKey, { method: "POST", path });
        return descriptor;
    };
}

// Parameter decorators
function Body(): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey || "constructor";
        if (!parameterMetadata.has(target)) {
            parameterMetadata.set(target, new Map());
        }
        if (!parameterMetadata.get(target)!.has(key)) {
            parameterMetadata.get(target)!.set(key, new Map());
        }
        parameterMetadata.get(target)!.get(key)!.set(parameterIndex, { source: "body" });
    };
}

function Query(name: string): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey || "constructor";
        if (!parameterMetadata.has(target)) {
            parameterMetadata.set(target, new Map());
        }
        if (!parameterMetadata.get(target)!.has(key)) {
            parameterMetadata.get(target)!.set(key, new Map());
        }
        parameterMetadata.get(target)!.get(key)!.set(parameterIndex, { source: "query", name });
    };
}

// Accessor decorator
function Cached(): MethodDecorator {
    return function(target: Object, propertyKey: string | symbol, descriptor: PropertyDescriptor) {
        if (descriptor.get) {
            const originalGet = descriptor.get;
            let cached: any;
            let hasCached = false;
            descriptor.get = function() {
                if (!hasCached) {
                    cached = originalGet.call(this);
                    hasCached = true;
                }
                return cached;
            };
        }
        return descriptor;
    };
}

// Entity class
@Service()
class UserEntity {
    @Column({ type: "uuid" })
    id: string = "";

    @Column({ type: "varchar", nullable: false })
    name: string = "";

    @Column({ type: "varchar", nullable: true })
    email: string = "";

    @Cached()
    get displayName(): string {
        return `${this.name} <${this.email}>`;
    }
}

// Controller class
@Controller("/users")
class UserController {
    constructor(private userService: UserEntity) {}

    @Get("/")
    async getAll(@Query("limit") limit: number): Promise<UserEntity[]> {
        return [];
    }

    @Get("/:id")
    async getOne(@Query("id") id: string): Promise<UserEntity | null> {
        return null;
    }

    @Post("/")
    async create(@Body() data: Partial<UserEntity>): Promise<UserEntity> {
        return new UserEntity();
    }
}

// Usage
const controller = new UserController(new UserEntity());
console.log(classMetadata.get(UserController));
console.log(classMetadata.get(UserEntity));
console.log(methodMetadata.get(UserController.prototype));"#;

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
        output.contains("UserController"),
        "expected output to contain UserController class. output: {output}"
    );
    assert!(
        output.contains("UserEntity"),
        "expected output to contain UserEntity class. output: {output}"
    );
    assert!(
        output.contains("Controller"),
        "expected output to contain Controller decorator. output: {output}"
    );
    assert!(
        output.contains("Service"),
        "expected output to contain Service decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive decorator metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Module Bundling ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_module_es5_commonjs_require() {
    let source = r#"// CommonJS require patterns
import { readFile, writeFile } from 'fs';
import * as path from 'path';
import http from 'http';

const express = require('express');
const { Router } = require('express');
const lodash = require('lodash');

export function loadConfig(configPath: string): any {
    const fullPath = path.resolve(configPath);
    const content = readFile(fullPath, 'utf-8');
    return JSON.parse(content as any);
}

export function saveConfig(configPath: string, config: any): void {
    const fullPath = path.resolve(configPath);
    writeFile(fullPath, JSON.stringify(config, null, 2));
}

export class Server {
    private app: any;
    private router: any;

    constructor() {
        this.app = express();
        this.router = Router();
    }

    start(port: number): void {
        this.app.listen(port, () => {
            console.log(`Server running on port ${port}`);
        });
    }
}

const server = new Server();
server.start(3000);"#;

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
        output.contains("Server"),
        "expected output to contain Server class. output: {output}"
    );
    assert!(
        output.contains("loadConfig"),
        "expected output to contain loadConfig function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for CommonJS require"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_dynamic_import() {
    let source = r#"// Dynamic import patterns
async function loadModule(moduleName: string): Promise<any> {
    const module = await import(moduleName);
    return module.default || module;
}

async function loadMultipleModules(names: string[]): Promise<any[]> {
    const modules = await Promise.all(
        names.map(name => import(name))
    );
    return modules.map(m => m.default || m);
}

class PluginLoader {
    private plugins: Map<string, any> = new Map();

    async load(pluginPath: string): Promise<void> {
        const plugin = await import(pluginPath);
        const name = plugin.name || pluginPath;
        this.plugins.set(name, plugin.default || plugin);
    }

    async loadAll(pluginPaths: string[]): Promise<void> {
        await Promise.all(pluginPaths.map(p => this.load(p)));
    }

    get(name: string): any {
        return this.plugins.get(name);
    }
}

// Lazy loading with fallback
async function lazyLoad<T>(
    loader: () => Promise<{ default: T }>,
    fallback: T
): Promise<T> {
    try {
        const module = await loader();
        return module.default;
    } catch {
        return fallback;
    }
}

const loader = new PluginLoader();
loader.loadAll(['./plugin1', './plugin2']).then(() => {
    console.log('Plugins loaded');
});"#;

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
        output.contains("PluginLoader"),
        "expected output to contain PluginLoader class. output: {output}"
    );
    assert!(
        output.contains("loadModule"),
        "expected output to contain loadModule function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for dynamic import"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_reexports() {
    let source = r#"// Re-export patterns
export { foo, bar } from './moduleA';
export { default as baz } from './moduleB';
export * from './moduleC';
export * as utils from './utils';
export type { SomeType } from './types';

// Named re-exports with renaming
export { original as renamed } from './moduleD';
export { ClassA as ExportedClass, functionB as exportedFunction } from './moduleE';

// Re-export with local use
import { helper } from './helpers';
export { helper };

function useHelper(): string {
    return helper('test');
}

// Mixed exports
export const localConst = 'local';
export function localFunction(): void {}
export class LocalClass {}

export { useHelper };"#;

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
        output.contains("LocalClass"),
        "expected output to contain LocalClass. output: {output}"
    );
    assert!(
        output.contains("useHelper"),
        "expected output to contain useHelper function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for re-exports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_barrel_exports() {
    let source = r#"// Barrel export pattern (index.ts style)

// Components
export { Button } from './components/Button';
export { Input } from './components/Input';
export { Select } from './components/Select';
export { Modal } from './components/Modal';

// Hooks
export { useState } from './hooks/useState';
export { useEffect } from './hooks/useEffect';
export { useCallback } from './hooks/useCallback';

// Utils
export * from './utils/string';
export * from './utils/number';
export * from './utils/date';

// Types
export type { ButtonProps } from './components/Button';
export type { InputProps } from './components/Input';
export type { Config, Options } from './types';

// Default export aggregation
import DefaultComponent from './DefaultComponent';
export default DefaultComponent;

// Re-export with namespace
export * as components from './components';
export * as hooks from './hooks';
export * as utils from './utils';

// Constants
export const VERSION = '1.0.0';
export const API_URL = 'https://api.example.com';"#;

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
        output.contains("VERSION"),
        "expected output to contain VERSION constant. output: {output}"
    );
    assert!(
        output.contains("API_URL"),
        "expected output to contain API_URL constant. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for barrel exports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_circular_imports() {
    let source = r#"// Circular import pattern handling
// This simulates moduleA that imports from moduleB which imports from moduleA

import type { BType } from './moduleB';

export interface AType {
    name: string;
    ref?: BType;
}

export class ClassA {
    private data: AType;
    private linked?: import('./moduleB').ClassB;

    constructor(name: string) {
        this.data = { name };
    }

    async link(): Promise<void> {
        const moduleB = await import('./moduleB');
        this.linked = new moduleB.ClassB(this);
    }

    getData(): AType {
        return this.data;
    }

    setRef(ref: BType): void {
        this.data.ref = ref;
    }
}

export function createA(name: string): ClassA {
    return new ClassA(name);
}

// Lazy circular reference resolution
let _classB: typeof import('./moduleB').ClassB | null = null;

export async function getClassB(): Promise<typeof import('./moduleB').ClassB> {
    if (!_classB) {
        const mod = await import('./moduleB');
        _classB = mod.ClassB;
    }
    return _classB;
}

const instance = new ClassA('test');
instance.link().then(() => console.log('Linked'));"#;

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
        output.contains("ClassA"),
        "expected output to contain ClassA class. output: {output}"
    );
    assert!(
        output.contains("createA"),
        "expected output to contain createA function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for circular imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_conditional_imports() {
    let source = r#"// Conditional import patterns
declare const process: { env: { NODE_ENV: string; PLATFORM: string } };

// Environment-based conditional import
const getLogger = async () => {
    if (process.env.NODE_ENV === 'production') {
        return import('./prodLogger');
    } else {
        return import('./devLogger');
    }
};

// Platform-based conditional import
const getPlatformModule = async () => {
    switch (process.env.PLATFORM) {
        case 'web':
            return import('./platform/web');
        case 'node':
            return import('./platform/node');
        case 'electron':
            return import('./platform/electron');
        default:
            return import('./platform/default');
    }
};

// Feature flag conditional import
interface FeatureFlags {
    newUI: boolean;
    betaFeatures: boolean;
}

async function loadFeatures(flags: FeatureFlags): Promise<any[]> {
    const features: Promise<any>[] = [];

    if (flags.newUI) {
        features.push(import('./features/newUI'));
    }

    if (flags.betaFeatures) {
        features.push(import('./features/beta'));
    }

    return Promise.all(features);
}

// Polyfill conditional import
async function loadPolyfills(): Promise<void> {
    if (typeof globalThis.fetch === 'undefined') {
        await import('whatwg-fetch');
    }

    if (typeof globalThis.Promise === 'undefined') {
        await import('es6-promise');
    }

    if (!Array.prototype.includes) {
        await import('array-includes');
    }
}

// Usage
getLogger().then(logger => logger.default.info('App started'));
loadPolyfills().then(() => console.log('Polyfills loaded'));"#;

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
        output.contains("getLogger"),
        "expected output to contain getLogger function. output: {output}"
    );
    assert!(
        output.contains("loadPolyfills"),
        "expected output to contain loadPolyfills function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_namespace_imports() {
    let source = r#"// Namespace import patterns
import * as React from 'react';
import * as ReactDOM from 'react-dom';
import * as _ from 'lodash';
import * as utils from './utils';

// Using namespace imports
const element = React.createElement('div', { className: 'container' },
    React.createElement('h1', null, 'Hello'),
    React.createElement('p', null, 'World')
);

// Lodash namespace usage
const data = [1, 2, 3, 4, 5];
const doubled = _.map(data, (n: number) => n * 2);
const sum = _.reduce(doubled, (acc: number, n: number) => acc + n, 0);
const unique = _.uniq([1, 1, 2, 2, 3]);

// Custom utils namespace
const formatted = utils.formatDate(new Date());
const validated = utils.validateEmail('test@example.com');
const parsed = utils.parseJSON('{"key": "value"}');

// Re-export namespace
export { React, ReactDOM, _ as lodash, utils };

// Namespace with type usage
type ReactElement = React.ReactElement;
type LodashArray = _.LoDashStatic;

export function render(el: ReactElement, container: Element): void {
    ReactDOM.render(el, container);
}

console.log(sum, unique, formatted);"#;

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
        output.contains("render"),
        "expected output to contain render function. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected output to contain doubled variable. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_comprehensive() {
    let source = r#"// Comprehensive module bundling test combining all patterns

// Static imports
import { EventEmitter } from 'events';
import * as fs from 'fs';
import path from 'path';

// CommonJS require
const express = require('express');
const bodyParser = require('body-parser');

// Type-only imports
import type { ServerOptions, RequestHandler } from 'express';

// Re-exports
export { EventEmitter } from 'events';
export * as fsUtils from 'fs';
export type { ServerOptions };

// Dynamic import loader
class ModuleRegistry {
    private modules: Map<string, any> = new Map();
    private loading: Map<string, Promise<any>> = new Map();

    async load(name: string, path: string): Promise<any> {
        if (this.modules.has(name)) {
            return this.modules.get(name);
        }

        if (!this.loading.has(name)) {
            this.loading.set(name, import(path).then(mod => {
                const module = mod.default || mod;
                this.modules.set(name, module);
                this.loading.delete(name);
                return module;
            }));
        }

        return this.loading.get(name);
    }

    get(name: string): any {
        return this.modules.get(name);
    }

    has(name: string): boolean {
        return this.modules.has(name);
    }
}

// Conditional module loading
declare const process: { env: Record<string, string> };

async function loadEnvironmentModules(): Promise<void> {
    const env = process.env.NODE_ENV || 'development';

    // Environment-specific config
    const configModule = await import(`./config/${env}`);
    const config = configModule.default;

    // Conditional feature modules
    if (config.features?.analytics) {
        await import('./modules/analytics');
    }

    if (config.features?.monitoring) {
        await import('./modules/monitoring');
    }

    // Platform-specific modules
    const platform = process.env.PLATFORM || 'node';
    await import(`./platform/${platform}`);
}

// Barrel export simulation
export { Button, Input, Form } from './components';
export { useForm, useValidation } from './hooks';
export * from './utils';

// Main application
export class Application extends EventEmitter {
    private registry: ModuleRegistry;
    private server: any;

    constructor() {
        super();
        this.registry = new ModuleRegistry();
        this.server = express();
        this.server.use(bodyParser.json());
    }

    async initialize(): Promise<void> {
        await loadEnvironmentModules();

        // Load plugins dynamically
        const pluginPaths = ['./plugins/auth', './plugins/api', './plugins/static'];
        await Promise.all(pluginPaths.map(p => this.registry.load(path.basename(p), p)));

        this.emit('initialized');
    }

    async loadPlugin(name: string, pluginPath: string): Promise<void> {
        const plugin = await this.registry.load(name, pluginPath);
        if (plugin.setup) {
            await plugin.setup(this.server);
        }
        this.emit('plugin:loaded', name);
    }

    start(port: number): void {
        this.server.listen(port, () => {
            this.emit('started', port);
            console.log(`Server running on port ${port}`);
        });
    }
}

// Factory export
export function createApp(): Application {
    return new Application();
}

// Default export
export default Application;

// Usage
const app = createApp();
app.initialize().then(() => app.start(3000));"#;

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
        output.contains("Application"),
        "expected output to contain Application class. output: {output}"
    );
    assert!(
        output.contains("ModuleRegistry"),
        "expected output to contain ModuleRegistry class. output: {output}"
    );
    assert!(
        output.contains("createApp"),
        "expected output to contain createApp function. output: {output}"
    );
    assert!(
        output.contains("loadEnvironmentModules"),
        "expected output to contain loadEnvironmentModules function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive module bundling"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// JSX ES5 Transform Source Map Tests
// =============================================================================
// Tests for JSX compilation with ES5 target - JSX elements should transform
// to React.createElement calls while preserving source map accuracy.

#[test]
fn test_source_map_jsx_es5_basic_element() {
    // Test basic JSX element transformation to React.createElement
    let source = r#"const element = <div className="container">Hello World</div>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // JSX should be in output (either preserved or transformed)
    assert!(
        output.contains("div") || output.contains("createElement"),
        "expected JSX element in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX element"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_fragment() {
    // Test JSX fragment transformation
    let source = r#"const fragment = <>
    <span>First</span>
    <span>Second</span>
</>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Fragment should be in output
    assert!(
        output.contains("span") || output.contains("Fragment"),
        "expected JSX fragment content in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX fragment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
