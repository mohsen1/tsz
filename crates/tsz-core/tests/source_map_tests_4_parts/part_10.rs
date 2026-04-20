#[test]
fn test_source_map_class_static_block_es5_init_order() {
    let source = r#"const log: string[] = [];

class InitOrder {
    static a = (log.push("field a"), 1);

    static {
        log.push("block 1");
    }

    static b = (log.push("field b"), 2);

    static {
        log.push("block 2");
    }

    static c = (log.push("field c"), 3);

    static {
        log.push("block 3");
        console.log("Final order:", log.join(", "));
    }
}

console.log(InitOrder.a, InitOrder.b, InitOrder.c);"#;

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
        output.contains("InitOrder"),
        "expected InitOrder in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for init order static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_private_access() {
    let source = r#"class SecretHolder {
    static #secret = "initial";
    static #counter = 0;

    static {
        SecretHolder.#secret = "configured";
        SecretHolder.#counter = 100;
    }

    static getSecret() {
        return SecretHolder.#secret;
    }

    static getCounter() {
        return SecretHolder.#counter;
    }
}

console.log(SecretHolder.getSecret(), SecretHolder.getCounter());"#;

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
        output.contains("SecretHolder"),
        "expected SecretHolder in output. output: {output}"
    );
    assert!(
        output.contains("getSecret"),
        "expected getSecret in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private access static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_super_access() {
    let source = r#"class Base {
    static baseValue = 10;

    static getBaseValue() {
        return Base.baseValue;
    }
}

class Derived extends Base {
    static derivedValue: number;

    static {
        Derived.derivedValue = Derived.baseValue * 2;
        console.log("Derived initialized with:", Derived.derivedValue);
    }

    static getCombined() {
        return Derived.baseValue + Derived.derivedValue;
    }
}

console.log(Derived.getCombined());"#;

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
        output.contains("Base"),
        "expected Base in output. output: {output}"
    );
    assert!(
        output.contains("Derived"),
        "expected Derived in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for super access static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_static_field_init() {
    let source = r#"class Database {
    static connection: any;
    static config: { host: string; port: number };
    static ready = false;

    static {
        Database.config = {
            host: "localhost",
            port: 5432
        };
    }

    static {
        Database.connection = {
            config: Database.config,
            connected: false
        };
    }

    static {
        Database.ready = true;
        console.log("Database ready:", Database.config.host);
    }

    static connect() {
        if (Database.ready) {
            Database.connection.connected = true;
        }
    }
}

Database.connect();
console.log(Database.connection);"#;

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
        output.contains("Database"),
        "expected Database in output. output: {output}"
    );
    assert!(
        output.contains("connection"),
        "expected connection in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static field init blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_computed_props() {
    let source = r#"const KEY1 = "config1";
const KEY2 = "config2";

class ConfigMap {
    static [KEY1]: string;
    static [KEY2]: number;
    static data: Record<string, any> = {};

    static {
        ConfigMap[KEY1] = "value1";
        ConfigMap[KEY2] = 42;
        ConfigMap.data[KEY1] = ConfigMap[KEY1];
        ConfigMap.data[KEY2] = ConfigMap[KEY2];
    }

    static get(key: string) {
        return ConfigMap.data[key];
    }
}

console.log(ConfigMap.get(KEY1), ConfigMap.get(KEY2));"#;

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
        output.contains("ConfigMap"),
        "expected ConfigMap in output. output: {output}"
    );
    assert!(
        output.contains("KEY1"),
        "expected KEY1 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed props static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_async_patterns() {
    let source = r#"class AsyncLoader {
    static data: any[] = [];
    static initialized = false;

    static {
        const init = async () => {
            AsyncLoader.data = [1, 2, 3];
            AsyncLoader.initialized = true;
        };
        init();
    }

    static async load() {
        if (!AsyncLoader.initialized) {
            await new Promise(r => setTimeout(r, 100));
        }
        return AsyncLoader.data;
    }
}

class EventEmitter {
    static handlers: Map<string, Function[]> = new Map();

    static {
        EventEmitter.handlers.set("init", []);
        EventEmitter.handlers.set("load", []);
    }

    static on(event: string, handler: Function) {
        const handlers = EventEmitter.handlers.get(event) || [];
        handlers.push(handler);
        EventEmitter.handlers.set(event, handlers);
    }
}

console.log(AsyncLoader.data, EventEmitter.handlers);"#;

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
        output.contains("AsyncLoader"),
        "expected AsyncLoader in output. output: {output}"
    );
    assert!(
        output.contains("EventEmitter"),
        "expected EventEmitter in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async patterns static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_error_handling() {
    let source = r#"class SafeInit {
    static value: number;
    static error: Error | null = null;

    static {
        try {
            SafeInit.value = parseInt("42");
            if (isNaN(SafeInit.value)) {
                throw new Error("Invalid number");
            }
        } catch (e) {
            SafeInit.error = e as Error;
            SafeInit.value = 0;
        }
    }

    static getValue() {
        if (SafeInit.error) {
            console.error("Initialization failed:", SafeInit.error.message);
            return null;
        }
        return SafeInit.value;
    }
}

class Validator {
    static rules: Map<string, Function> = new Map();
    static errors: string[] = [];

    static {
        try {
            Validator.rules.set("required", (v: any) => v != null);
            Validator.rules.set("minLength", (v: string) => v.length >= 3);
        } catch (e) {
            Validator.errors.push("Failed to initialize rules");
        }
    }
}

console.log(SafeInit.getValue(), Validator.rules.size);"#;

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
        output.contains("SafeInit"),
        "expected SafeInit in output. output: {output}"
    );
    assert!(
        output.contains("Validator"),
        "expected Validator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for error handling static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_comprehensive() {
    let source = r#"// Comprehensive class static block patterns for ES5 transform testing

// Basic static block
class Counter {
    static count = 0;

    static {
        Counter.count = 100;
    }
}

// Multiple blocks with init order
const initLog: string[] = [];

class MultiBlock {
    static a = (initLog.push("a"), 1);

    static {
        initLog.push("block1");
    }

    static b = (initLog.push("b"), 2);

    static {
        initLog.push("block2");
    }
}

// Private field access
class PrivateAccess {
    static #privateValue = 0;

    static {
        PrivateAccess.#privateValue = 42;
    }

    static getValue() {
        return PrivateAccess.#privateValue;
    }
}

// Inheritance
class Parent {
    static parentValue = 10;
}

class Child extends Parent {
    static childValue: number;

    static {
        Child.childValue = Child.parentValue * 2;
    }
}

// Complex initialization
class ComplexInit {
    static config: { host: string; port: number };
    static connections: Map<string, any>;
    static ready = false;

    static {
        ComplexInit.config = {
            host: "localhost",
            port: 3000
        };
    }

    static {
        ComplexInit.connections = new Map();
        ComplexInit.connections.set("default", ComplexInit.config);
    }

    static {
        ComplexInit.ready = true;
        console.log("ComplexInit ready");
    }
}

// Error handling
class SafeLoader {
    static data: any[] = [];
    static error: Error | null = null;

    static {
        try {
            SafeLoader.data = [1, 2, 3];
        } catch (e) {
            SafeLoader.error = e as Error;
        }
    }
}

// Async initialization pattern
class AsyncInit {
    static promise: Promise<void>;

    static {
        AsyncInit.promise = (async () => {
            await Promise.resolve();
            console.log("Async init complete");
        })();
    }
}

// Computed property access
const KEY = "dynamicKey";

class DynamicClass {
    static [KEY]: string;

    static {
        DynamicClass[KEY] = "dynamic value";
    }
}

// Usage
console.log(Counter.count);
console.log(initLog);
console.log(PrivateAccess.getValue());
console.log(Child.childValue);
console.log(ComplexInit.ready);
console.log(SafeLoader.data);
console.log(DynamicClass[KEY]);"#;

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
        output.contains("Counter"),
        "expected Counter in output. output: {output}"
    );
    assert!(
        output.contains("MultiBlock"),
        "expected MultiBlock in output. output: {output}"
    );
    assert!(
        output.contains("PrivateAccess"),
        "expected PrivateAccess in output. output: {output}"
    );
    assert!(
        output.contains("Parent"),
        "expected Parent in output. output: {output}"
    );
    assert!(
        output.contains("Child"),
        "expected Child in output. output: {output}"
    );
    assert!(
        output.contains("ComplexInit"),
        "expected ComplexInit in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Decorator Composition Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_decorator_composition_es5_chained() {
    let source = r#"function log(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Calling:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function measure(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        const start = Date.now();
        const result = original.apply(this, args);
        console.log("Duration:", Date.now() - start);
        return result;
    };
    return descriptor;
}

function validate(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        if (args.some(a => a == null)) throw new Error("Invalid args");
        return original.apply(this, args);
    };
    return descriptor;
}

class Service {
    @log
    @measure
    @validate
    process(data: string) {
        return data.toUpperCase();
    }
}

const service = new Service();
console.log(service.process("test"));"#;

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
        output.contains("Service"),
        "expected Service in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_factory() {
    let source = r#"function log(prefix: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(prefix, key, args);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

function retry(times: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = async function(...args: any[]) {
            for (let i = 0; i < times; i++) {
                try {
                    return await original.apply(this, args);
                } catch (e) {
                    if (i === times - 1) throw e;
                }
            }
        };
        return descriptor;
    };
}

function cache(ttl: number) {
    const store = new Map<string, { value: any; expires: number }>();
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const cacheKey = JSON.stringify(args);
            const cached = store.get(cacheKey);
            if (cached && cached.expires > Date.now()) return cached.value;
            const result = original.apply(this, args);
            store.set(cacheKey, { value: result, expires: Date.now() + ttl });
            return result;
        };
        return descriptor;
    };
}

class Api {
    @log("[API]")
    @retry(3)
    @cache(5000)
    async fetchData(id: number) {
        return { id, data: "result" };
    }
}

const api = new Api();
api.fetchData(1).then(console.log);"#;

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
        output.contains("Api"),
        "expected Api in output. output: {output}"
    );
    assert!(
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for factory decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

