/// Comprehensive test for logical assignment patterns with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_comprehensive() {
    let source = r#"// Comprehensive logical assignment scenarios
interface User {
    id: number;
    name: string | null;
    email: string | undefined;
    preferences: {
        theme: string | null;
        language: string | undefined;
        notifications: boolean;
    } | null;
}

class UserService {
    private users: Map<number, User> = new Map();

    createUser(id: number): User {
        const user: User = {
            id,
            name: null,
            email: undefined,
            preferences: null
        };
        this.users.set(id, user);
        return user;
    }

    ensureUserName(id: number): string {
        const user = this.users.get(id);
        if (user) {
            user.name ??= "Anonymous";
            return user.name;
        }
        return "Unknown";
    }

    ensureUserEmail(id: number, defaultEmail: string): string {
        const user = this.users.get(id);
        if (user) {
            user.email ||= defaultEmail;
            return user.email;
        }
        return defaultEmail;
    }

    ensurePreferences(id: number): void {
        const user = this.users.get(id);
        if (user) {
            user.preferences ??= {
                theme: null,
                language: undefined,
                notifications: true
            };
            user.preferences.theme ??= "light";
            user.preferences.language ||= "en";
            user.preferences.notifications &&= true;
        }
    }
}

class ConfigStore {
    private config: Record<string, any> = {};

    get<T>(key: string, defaultValue: T): T {
        let value = this.config[key] as T | undefined;
        value ??= defaultValue;
        return value;
    }

    set<T>(key: string, value: T): void {
        this.config[key] = value;
    }

    update<T>(key: string, updater: (val: T | undefined) => T): T {
        let current = this.config[key] as T | undefined;
        current ??= undefined as any;
        const updated = updater(current);
        this.config[key] = updated;
        return updated;
    }
}

// Utility functions
function ensureArray<T>(arr: T[] | null, defaultItems: T[]): T[] {
    let result = arr;
    result ??= defaultItems;
    return result;
}

function conditionalUpdate<T>(
    value: T | null,
    condition: boolean,
    newValue: T
): T | null {
    let result = value;
    if (condition) {
        result &&= newValue;
    } else {
        result ||= newValue;
    }
    return result;
}

// Usage
const userService = new UserService();
const user = userService.createUser(1);
console.log(userService.ensureUserName(1));
console.log(userService.ensureUserEmail(1, "default@example.com"));
userService.ensurePreferences(1);

const store = new ConfigStore();
console.log(store.get("theme", "dark"));
store.set("theme", "light");
console.log(store.get("theme", "dark"));

const items = ensureArray<string>(null, ["default"]);
console.log(items);

const updated = conditionalUpdate<string>("existing", true, "new");
console.log(updated);"#;

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
        output.contains("ConfigStore"),
        "expected ConfigStore in output. output: {output}"
    );
    assert!(
        output.contains("ensureArray"),
        "expected ensureArray in output. output: {output}"
    );
    assert!(
        output.contains("conditionalUpdate"),
        "expected conditionalUpdate in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive logical assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - CLASS STATIC BLOCK PATTERNS (INIT ORDER, PRIVATE ACCESS)
// =============================================================================
// Tests for class static block patterns with ES5 target to verify source maps
// work correctly with static block transforms, focusing on init order and private access.

/// Test basic class static block with ES5 target
#[test]
fn test_source_map_static_block_es5_basic() {
    let source = r#"class Counter {
    static count: number;

    static {
        Counter.count = 0;
        console.log("Counter initialized");
    }

    static increment(): number {
        return ++Counter.count;
    }
}

console.log(Counter.increment());
console.log(Counter.increment());
console.log(Counter.count);"#;

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
        output.contains("Counter"),
        "expected Counter in output. output: {output}"
    );
    assert!(
        output.contains("increment"),
        "expected increment in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic static block"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test class static block initialization order with ES5 target
#[test]
fn test_source_map_static_block_es5_init_order() {
    let source = r#"const log: string[] = [];

class InitOrder {
    static a = (log.push("field a"), "a");

    static {
        log.push("block 1");
    }

    static b = (log.push("field b"), "b");

    static {
        log.push("block 2");
    }

    static c = (log.push("field c"), "c");

    static {
        log.push("block 3");
        console.log("Init order:", log.join(", "));
    }
}

console.log(InitOrder.a, InitOrder.b, InitOrder.c);"#;

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
        output.contains("InitOrder"),
        "expected InitOrder in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for init order"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

/// Test async static method with this capture
#[test]
fn test_source_map_async_class_integration_es5_static_this_capture() {
    let source = r#"class ConfigManager {
    private static instance: ConfigManager | null = null;
    private static config: Map<string, any> = new Map();
    private static initialized: boolean = false;

    static async initialize(): Promise<void> {
        if (this.initialized) {
            return;
        }
        await this.loadConfig();
        this.initialized = true;
    }

    private static async loadConfig(): Promise<void> {
        // Simulate async config loading
        const data = await fetch("/config.json");
        const json = await data.json();
        for (const [key, value] of Object.entries(json)) {
            this.config.set(key, value);
        }
    }

    static async get(key: string): Promise<any> {
        if (!this.initialized) {
            await this.initialize();
        }
        return this.config.get(key);
    }

    static async set(key: string, value: any): Promise<void> {
        if (!this.initialized) {
            await this.initialize();
        }
        this.config.set(key, value);
        await this.persist();
    }

    private static async persist(): Promise<void> {
        const data = Object.fromEntries(this.config);
        console.log("Persisting config:", data);
    }

    static async getInstance(): Promise<ConfigManager> {
        if (!this.instance) {
            await this.initialize();
            this.instance = new ConfigManager();
        }
        return this.instance;
    }
}

ConfigManager.initialize().then(() => console.log("Config ready"));"#;

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
        output.contains("ConfigManager"),
        "expected ConfigManager in output. output: {output}"
    );
    assert!(
        output.contains("initialize"),
        "expected initialize in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async static with this capture"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator class method with ES5 target
#[test]
fn test_source_map_async_class_integration_es5_generator_method() {
    let source = r#"class DataStream {
    private items: string[] = [];
    private batchSize: number;

    constructor(items: string[], batchSize: number = 10) {
        this.items = items;
        this.batchSize = batchSize;
    }

    async *stream(): AsyncGenerator<string[], void, unknown> {
        for (let i = 0; i < this.items.length; i += this.batchSize) {
            const batch = this.items.slice(i, i + this.batchSize);
            await this.processBatch(batch);
            yield batch;
        }
    }

    async *filter(predicate: (item: string) => boolean): AsyncGenerator<string, void, unknown> {
        for await (const batch of this.stream()) {
            for (const item of batch) {
                if (predicate(item)) {
                    yield item;
                }
            }
        }
    }

    async *map<T>(transform: (item: string) => T): AsyncGenerator<T, void, unknown> {
        for await (const batch of this.stream()) {
            for (const item of batch) {
                yield transform(item);
            }
        }
    }

    private async processBatch(batch: string[]): Promise<void> {
        console.log(`Processing batch of ${batch.length} items`);
    }
}

const stream = new DataStream(["a", "b", "c", "d", "e"]);
(async () => {
    for await (const batch of stream.stream()) {
        console.log("Batch:", batch);
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
        output.contains("DataStream"),
        "expected DataStream in output. output: {output}"
    );
    assert!(
        output.contains("stream"),
        "expected stream method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async generator class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async constructor simulation pattern
#[test]
fn test_source_map_async_class_integration_es5_constructor_simulation() {
    let source = r#"class AsyncDatabase {
    private connection: any = null;
    private ready: boolean = false;

    private constructor() {
        // Private constructor - use create() instead
    }

    private async init(connectionString: string): Promise<void> {
        this.connection = await this.connect(connectionString);
        await this.runMigrations();
        this.ready = true;
    }

    private async connect(connectionString: string): Promise<any> {
        console.log("Connecting to:", connectionString);
        return { connected: true };
    }

    private async runMigrations(): Promise<void> {
        console.log("Running migrations...");
    }

    static async create(connectionString: string): Promise<AsyncDatabase> {
        const instance = new AsyncDatabase();
        await instance.init(connectionString);
        return instance;
    }

    async query(sql: string): Promise<any[]> {
        if (!this.ready) {
            throw new Error("Database not initialized");
        }
        console.log("Executing:", sql);
        return [];
    }

    async close(): Promise<void> {
        if (this.connection) {
            console.log("Closing connection");
            this.connection = null;
            this.ready = false;
        }
    }
}

// Factory pattern with async initialization
class AsyncService {
    private db: AsyncDatabase | null = null;

    private constructor() {}

    private async initialize(): Promise<void> {
        this.db = await AsyncDatabase.create("postgres://localhost/mydb");
    }

    static async create(): Promise<AsyncService> {
        const service = new AsyncService();
        await service.initialize();
        return service;
    }

    async getData(): Promise<any[]> {
        return this.db!.query("SELECT * FROM data");
    }
}

AsyncService.create().then(service => service.getData());"#;

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
        output.contains("AsyncDatabase"),
        "expected AsyncDatabase in output. output: {output}"
    );
    assert!(
        output.contains("AsyncService"),
        "expected AsyncService in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create factory in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async constructor simulation"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test combined async/class source map patterns
#[test]
fn test_source_map_async_class_integration_es5_comprehensive() {
    let source = r#"// Comprehensive async/class integration test
abstract class BaseRepository<T> {
    protected items: Map<string, T> = new Map();

    abstract validate(item: T): Promise<boolean>;

    async findById(id: string): Promise<T | undefined> {
        return this.items.get(id);
    }

    async save(id: string, item: T): Promise<void> {
        const isValid = await this.validate(item);
        if (!isValid) {
            throw new Error("Validation failed");
        }
        this.items.set(id, item);
    }
}

interface User {
    id: string;
    name: string;
    email: string;
}

class UserRepository extends BaseRepository<User> {
    private static instance: UserRepository;

    // Async arrow field
    validateEmail = async (email: string): Promise<boolean> => {
        return email.includes("@");
    };

    async validate(user: User): Promise<boolean> {
        const emailValid = await this.validateEmail(user.email);
        return emailValid && user.name.length > 0;
    }

    // Async generator for streaming users
    async *streamUsers(): AsyncGenerator<User, void, unknown> {
        for (const user of this.items.values()) {
            yield user;
        }
    }

    // Static async factory
    static async getInstance(): Promise<UserRepository> {
        if (!this.instance) {
            this.instance = new UserRepository();
            await this.instance.initialize();
        }
        return this.instance;
    }

    private async initialize(): Promise<void> {
        console.log("Initializing UserRepository");
    }

    // Async method with super call
    async save(id: string, user: User): Promise<void> {
        console.log(`Saving user: ${user.name}`);
        await super.save(id, user);
    }
}

class UserService {
    private repo: UserRepository | null = null;

    // Multiple async arrow fields
    getUser = async (id: string): Promise<User | undefined> => {
        const repo = await this.getRepo();
        return repo.findById(id);
    };

    createUser = async (user: User): Promise<void> => {
        const repo = await this.getRepo();
        await repo.save(user.id, user);
    };

    private async getRepo(): Promise<UserRepository> {
        if (!this.repo) {
            this.repo = await UserRepository.getInstance();
        }
        return this.repo;
    }
}

// Usage
const service = new UserService();
(async () => {
    await service.createUser({ id: "1", name: "John", email: "john@example.com" });
    const user = await service.getUser("1");
    console.log(user);

    const repo = await UserRepository.getInstance();
    for await (const u of repo.streamUsers()) {
        console.log("Streaming user:", u.name);
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
        output.contains("BaseRepository"),
        "expected BaseRepository in output. output: {output}"
    );
    assert!(
        output.contains("UserRepository"),
        "expected UserRepository in output. output: {output}"
    );
    assert!(
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("validateEmail"),
        "expected validateEmail in output. output: {output}"
    );
    assert!(
        output.contains("streamUsers"),
        "expected streamUsers in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async/class integration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - GENERATOR TRANSFORM PATTERNS
// =============================================================================
// Tests for generator transform patterns with ES5 target to verify source maps
// work correctly with generator state machine transforms.

/// Test generator function basic yield mapping with typed parameters
#[test]
fn test_source_map_generator_transform_es5_basic_yield_mapping() {
    let source = r#"function* numberSequence(start: number, end: number): Generator<number, void, unknown> {
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

function* alphabetGenerator(): Generator<string, void, unknown> {
    const letters = "abcdefghijklmnopqrstuvwxyz";
    for (const letter of letters) {
        yield letter;
    }
}

// Using the generators
const numbers = numberSequence(1, 5);
for (const n of numbers) {
    console.log("Number:", n);
}

const alphabet = alphabetGenerator();
console.log(alphabet.next().value);
console.log(alphabet.next().value);"#;

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
        output.contains("numberSequence"),
        "expected numberSequence in output. output: {output}"
    );
    assert!(
        output.contains("alphabetGenerator"),
        "expected alphabetGenerator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic yield mapping"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

