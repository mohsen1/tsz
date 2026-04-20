/// Test multiple class static blocks with ES5 target
#[test]
fn test_source_map_static_block_es5_multiple() {
    let source = r#"class MultiBlock {
    static config: Record<string, any> = {};

    static {
        MultiBlock.config.name = "app";
    }

    static {
        MultiBlock.config.version = "1.0.0";
    }

    static {
        MultiBlock.config.debug = false;
    }

    static {
        MultiBlock.config.features = ["a", "b", "c"];
        console.log("Config complete:", MultiBlock.config);
    }

    static getConfig(): Record<string, any> {
        return MultiBlock.config;
    }
}

console.log(MultiBlock.getConfig());"#;

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
        output.contains("MultiBlock"),
        "expected MultiBlock in output. output: {output}"
    );
    assert!(
        output.contains("getConfig"),
        "expected getConfig in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with private field access with ES5 target
#[test]
fn test_source_map_static_block_es5_private_field_access() {
    let source = r#"class PrivateFields {
    static #secret: string;
    static #counter: number;

    static {
        PrivateFields.#secret = "hidden-value";
        PrivateFields.#counter = 0;
    }

    static getSecret(): string {
        return PrivateFields.#secret;
    }

    static incrementCounter(): number {
        return ++PrivateFields.#counter;
    }

    static {
        console.log("Private fields initialized");
        console.log("Secret length:", PrivateFields.#secret.length);
    }
}

console.log(PrivateFields.getSecret());
console.log(PrivateFields.incrementCounter());"#;

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
        output.contains("PrivateFields"),
        "expected PrivateFields in output. output: {output}"
    );
    assert!(
        output.contains("getSecret"),
        "expected getSecret in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with private method access with ES5 target
#[test]
fn test_source_map_static_block_es5_private_method_access() {
    let source = r#"class PrivateMethods {
    static #initialize(): void {
        console.log("Initializing...");
    }

    static #validate(value: string): boolean {
        return value.length > 0;
    }

    static #format(value: string): string {
        return value.toUpperCase();
    }

    static {
        PrivateMethods.#initialize();
        const valid = PrivateMethods.#validate("test");
        console.log("Validation:", valid);
    }

    static process(input: string): string {
        if (PrivateMethods.#validate(input)) {
            return PrivateMethods.#format(input);
        }
        return "";
    }
}

console.log(PrivateMethods.process("hello"));"#;

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
        output.contains("PrivateMethods"),
        "expected PrivateMethods in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private method access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with static field initialization with ES5 target
#[test]
fn test_source_map_static_block_es5_static_field_init() {
    let source = r#"class StaticInit {
    static readonly API_URL: string;
    static readonly TIMEOUT: number;
    static readonly HEADERS: Record<string, string>;

    static {
        const env = { api: "https://api.example.com", timeout: 5000 };
        StaticInit.API_URL = env.api;
        StaticInit.TIMEOUT = env.timeout;
        StaticInit.HEADERS = {
            "Content-Type": "application/json",
            "Accept": "application/json"
        };
    }

    static fetch(endpoint: string): Promise<any> {
        console.log(`Fetching ${StaticInit.API_URL}/${endpoint}`);
        return Promise.resolve({});
    }
}

console.log(StaticInit.API_URL);
console.log(StaticInit.TIMEOUT);
StaticInit.fetch("users");"#;

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
        output.contains("StaticInit"),
        "expected StaticInit in output. output: {output}"
    );
    assert!(
        output.contains("fetch"),
        "expected fetch in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static field init"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with computed property names with ES5 target
#[test]
fn test_source_map_static_block_es5_computed_props() {
    let source = r#"const KEY1 = "dynamicKey1";
const KEY2 = "dynamicKey2";

class ComputedProps {
    static [KEY1]: string;
    static [KEY2]: number;
    static computed: Record<string, any> = {};

    static {
        ComputedProps[KEY1] = "dynamic-value-1";
        ComputedProps[KEY2] = 42;
        ComputedProps.computed[KEY1] = "nested-dynamic";
    }

    static get(key: string): any {
        return ComputedProps.computed[key];
    }
}

console.log(ComputedProps[KEY1]);
console.log(ComputedProps[KEY2]);
console.log(ComputedProps.get(KEY1));"#;

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
        output.contains("ComputedProps"),
        "expected ComputedProps in output. output: {output}"
    );
    assert!(
        output.contains("KEY1"),
        "expected KEY1 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed props"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with async patterns with ES5 target
#[test]
fn test_source_map_static_block_es5_async_patterns() {
    let source = r#"class AsyncInit {
    static data: any;
    static ready: Promise<void>;

    static {
        AsyncInit.ready = (async () => {
            await new Promise(r => setTimeout(r, 100));
            AsyncInit.data = { loaded: true };
            console.log("Async init complete");
        })();
    }

    static async getData(): Promise<any> {
        await AsyncInit.ready;
        return AsyncInit.data;
    }
}

class LazyLoader {
    static #cache: Map<string, any> = new Map();

    static {
        console.log("LazyLoader initialized");
    }

    static async load(key: string): Promise<any> {
        if (!LazyLoader.#cache.has(key)) {
            const data = await fetch(key);
            LazyLoader.#cache.set(key, data);
        }
        return LazyLoader.#cache.get(key);
    }
}

AsyncInit.getData().then(console.log);"#;

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
        output.contains("AsyncInit"),
        "expected AsyncInit in output. output: {output}"
    );
    assert!(
        output.contains("LazyLoader"),
        "expected LazyLoader in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with error handling with ES5 target
#[test]
fn test_source_map_static_block_es5_error_handling() {
    let source = r#"class ErrorHandling {
    static config: any;
    static error: Error | null = null;

    static {
        try {
            const raw = '{"valid": true}';
            ErrorHandling.config = JSON.parse(raw);
            console.log("Config loaded successfully");
        } catch (e) {
            ErrorHandling.error = e as Error;
            ErrorHandling.config = { fallback: true };
            console.error("Failed to load config:", e);
        } finally {
            console.log("Init complete");
        }
    }

    static isValid(): boolean {
        return ErrorHandling.error === null;
    }
}

class SafeInit {
    static #initialized = false;

    static {
        try {
            SafeInit.#initialized = true;
        } catch {
            SafeInit.#initialized = false;
        }
    }

    static isReady(): boolean {
        return SafeInit.#initialized;
    }
}

console.log(ErrorHandling.isValid());
console.log(SafeInit.isReady());"#;

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
        output.contains("ErrorHandling"),
        "expected ErrorHandling in output. output: {output}"
    );
    assert!(
        output.contains("SafeInit"),
        "expected SafeInit in output. output: {output}"
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

/// Comprehensive test for class static block patterns with ES5 target
#[test]
fn test_source_map_static_block_es5_comprehensive() {
    let source = r#"// Comprehensive static block scenarios
class Registry {
    static #instances: Map<string, any> = new Map();
    static #initialized = false;

    static {
        Registry.#initialized = true;
        console.log("Registry initialized");
    }

    static register<T>(key: string, instance: T): void {
        Registry.#instances.set(key, instance);
    }

    static get<T>(key: string): T | undefined {
        return Registry.#instances.get(key) as T | undefined;
    }

    static isInitialized(): boolean {
        return Registry.#initialized;
    }
}

class ConfigManager {
    static readonly defaults: Record<string, any>;
    static #config: Record<string, any>;

    static {
        ConfigManager.defaults = {
            theme: "light",
            language: "en",
            timeout: 5000
        };
        ConfigManager.#config = { ...ConfigManager.defaults };
    }

    static get<T>(key: string): T {
        return ConfigManager.#config[key] as T;
    }

    static set<T>(key: string, value: T): void {
        ConfigManager.#config[key] = value;
    }

    static reset(): void {
        ConfigManager.#config = { ...ConfigManager.defaults };
    }
}

class EventBus {
    static #handlers: Map<string, Function[]> = new Map();
    static #eventCount = 0;

    static {
        EventBus.#handlers = new Map();
        console.log("EventBus ready");
    }

    static on(event: string, handler: Function): void {
        if (!EventBus.#handlers.has(event)) {
            EventBus.#handlers.set(event, []);
        }
        EventBus.#handlers.get(event)!.push(handler);
    }

    static emit(event: string, ...args: any[]): void {
        EventBus.#eventCount++;
        const handlers = EventBus.#handlers.get(event) || [];
        handlers.forEach(h => h(...args));
    }

    static getEventCount(): number {
        return EventBus.#eventCount;
    }
}

class DependencyInjector {
    static #container: Map<string, () => any> = new Map();

    static {
        DependencyInjector.#container.set("logger", () => console);
        DependencyInjector.#container.set("config", () => ConfigManager);
    }

    static {
        DependencyInjector.#container.set("events", () => EventBus);
        console.log("DI container configured");
    }

    static resolve<T>(key: string): T {
        const factory = DependencyInjector.#container.get(key);
        if (!factory) throw new Error(`No provider for ${key}`);
        return factory() as T;
    }

    static register<T>(key: string, factory: () => T): void {
        DependencyInjector.#container.set(key, factory);
    }
}

// Usage
Registry.register("app", { name: "MyApp" });
console.log(Registry.get("app"));
console.log(Registry.isInitialized());

ConfigManager.set("theme", "dark");
console.log(ConfigManager.get("theme"));
ConfigManager.reset();

EventBus.on("test", (msg: string) => console.log(msg));
EventBus.emit("test", "Hello!");
console.log(EventBus.getEventCount());

const logger = DependencyInjector.resolve<Console>("logger");
logger.log("DI working!");"#;

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
        output.contains("Registry"),
        "expected Registry in output. output: {output}"
    );
    assert!(
        output.contains("ConfigManager"),
        "expected ConfigManager in output. output: {output}"
    );
    assert!(
        output.contains("EventBus"),
        "expected EventBus in output. output: {output}"
    );
    assert!(
        output.contains("DependencyInjector"),
        "expected DependencyInjector in output. output: {output}"
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
// ES5 SOURCE MAP TESTS - ASYNC/CLASS INTEGRATION PATTERNS
// =============================================================================
// Tests for async/class integration patterns with ES5 target to verify source maps
// work correctly with combined async and class transforms.

/// Test async method in derived class with super call
#[test]
fn test_source_map_async_class_integration_es5_derived_super_call() {
    let source = r#"class BaseService {
    protected baseUrl: string = "https://api.example.com";

    async fetchData(endpoint: string): Promise<any> {
        const response = await fetch(this.baseUrl + endpoint);
        return response.json();
    }

    async validate(data: any): Promise<boolean> {
        return data !== null && data !== undefined;
    }
}

class UserService extends BaseService {
    private userId: string;

    constructor(userId: string) {
        super();
        this.userId = userId;
    }

    async getUser(): Promise<any> {
        const data = await super.fetchData(`/users/${this.userId}`);
        const isValid = await super.validate(data);
        if (!isValid) {
            throw new Error("Invalid user data");
        }
        return data;
    }

    async updateUser(updates: any): Promise<any> {
        const currentData = await super.fetchData(`/users/${this.userId}`);
        const merged = { ...currentData, ...updates };
        return merged;
    }
}

const service = new UserService("123");
service.getUser().then(console.log);"#;

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
        output.contains("BaseService"),
        "expected BaseService in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async derived class with super"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async arrow field initializer with ES5 target
#[test]
fn test_source_map_async_class_integration_es5_arrow_field_initializer() {
    let source = r#"class EventHandler {
    private name: string;
    private count: number = 0;

    // Async arrow as class field - captures 'this' lexically
    handleClick = async (event: any): Promise<void> => {
        this.count++;
        console.log(`${this.name} clicked ${this.count} times`);
        await this.processEvent(event);
    };

    handleHover = async (): Promise<string> => {
        return `Hovering over ${this.name}`;
    };

    handleSubmit = async (data: any): Promise<boolean> => {
        const result = await this.validate(data);
        if (result) {
            await this.save(data);
        }
        return result;
    };

    constructor(name: string) {
        this.name = name;
    }

    private async processEvent(event: any): Promise<void> {
        console.log("Processing:", event);
    }

    private async validate(data: any): Promise<boolean> {
        return data !== null;
    }

    private async save(data: any): Promise<void> {
        console.log("Saving:", data);
    }
}

const handler = new EventHandler("Button");
handler.handleClick({ type: "click" });"#;

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
        output.contains("EventHandler"),
        "expected EventHandler in output. output: {output}"
    );
    assert!(
        output.contains("handleClick"),
        "expected handleClick in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async arrow field initializer"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

