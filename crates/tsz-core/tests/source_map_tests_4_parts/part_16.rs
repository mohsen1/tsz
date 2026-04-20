/// Test ??= nullish coalescing assignment with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_nullish_assign() {
    let source = r#"let value1: string | null = null;
let value2: string | undefined = undefined;
let value3: string | null = "existing";

value1 ??= "default1";
value2 ??= "default2";
value3 ??= "default3";

function ensureValue(input: number | undefined): number {
    let result = input;
    result ??= 0;
    return result;
}

const result1 = ensureValue(undefined);
const result2 = ensureValue(42);
console.log(value1, value2, value3, result1, result2);"#;

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
        output.contains("value1"),
        "expected value1 in output. output: {output}"
    );
    assert!(
        output.contains("ensureValue"),
        "expected ensureValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ??= assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with object properties with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_object_property() {
    let source = r#"interface Config {
    name: string | null;
    count: number | undefined;
    enabled: boolean;
}

const config: Config = {
    name: null,
    count: undefined,
    enabled: false
};

config.name ||= "default-name";
config.count ??= 0;
config.enabled &&= true;

function updateConfig(cfg: Config): void {
    cfg.name ??= "unnamed";
    cfg.count ||= 1;
}

updateConfig(config);
console.log(config);"#;

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
        output.contains("config"),
        "expected config in output. output: {output}"
    );
    assert!(
        output.contains("updateConfig"),
        "expected updateConfig in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object property"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with element access with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_element_access() {
    let source = r#"const arr: (string | null)[] = [null, "existing", null];
const obj: Record<string, number | undefined> = { a: undefined, b: 10 };

arr[0] ||= "default0";
arr[1] ||= "default1";
arr[2] ??= "default2";

obj["a"] ??= 0;
obj["b"] &&= 20;
obj["c"] ??= 30;

function updateArray(items: (string | null)[], index: number): void {
    items[index] ??= "fallback";
}

updateArray(arr, 0);
console.log(arr, obj);"#;

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
        output.contains("arr"),
        "expected arr in output. output: {output}"
    );
    assert!(
        output.contains("updateArray"),
        "expected updateArray in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for element access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test chained logical assignments with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_chained() {
    let source = r#"let a: string | null = null;
let b: string | null = null;
let c: string | null = null;

a ||= "a-default";
b ??= a;
c &&= b;

interface ChainedConfig {
    primary: string | null;
    secondary: string | null;
    tertiary: string | null;
}

function chainedAssignments(cfg: ChainedConfig): void {
    cfg.primary ??= "primary-default";
    cfg.secondary ||= cfg.primary;
    cfg.tertiary &&= cfg.secondary;
}

const cfg: ChainedConfig = { primary: null, secondary: null, tertiary: "exists" };
chainedAssignments(cfg);
console.log(a, b, c, cfg);"#;

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
        output.contains("chainedAssignments"),
        "expected chainedAssignments in output. output: {output}"
    );
    assert!(
        output.contains("cfg"),
        "expected cfg in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained assignments"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment in function context with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_function_context() {
    let source = r#"function processWithDefaults(
    name: string | null,
    count: number | undefined,
    enabled: boolean
): { name: string; count: number; enabled: boolean } {
    let n = name;
    let c = count;
    let e = enabled;

    n ??= "anonymous";
    c ||= 1;
    e &&= true;

    return { name: n, count: c, enabled: e };
}

const arrowWithAssign = (val: string | null): string => {
    let result = val;
    result ??= "arrow-default";
    return result;
};

function nestedFunction(): string {
    let outer: string | null = null;

    function inner(): void {
        outer ??= "from-inner";
    }

    inner();
    return outer ?? "never";
}

console.log(processWithDefaults(null, undefined, true));
console.log(arrowWithAssign(null));
console.log(nestedFunction());"#;

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
        output.contains("processWithDefaults"),
        "expected processWithDefaults in output. output: {output}"
    );
    assert!(
        output.contains("arrowWithAssign"),
        "expected arrowWithAssign in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function context"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment in class methods with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_class_methods() {
    let source = r#"class DataManager {
    private data: string | null = null;
    private count: number | undefined = undefined;
    private active: boolean = true;

    ensureData(): string {
        this.data ??= "default-data";
        return this.data;
    }

    ensureCount(): number {
        this.count ||= 0;
        return this.count;
    }

    updateActive(value: boolean): boolean {
        this.active &&= value;
        return this.active;
    }

    reset(): void {
        this.data = null;
        this.count = undefined;
        this.active = true;
    }
}

class CacheManager {
    private cache: Map<string, string | null> = new Map();

    getOrSet(key: string, defaultValue: string): string {
        let value = this.cache.get(key);
        value ??= defaultValue;
        this.cache.set(key, value);
        return value;
    }
}

const manager = new DataManager();
console.log(manager.ensureData());
console.log(manager.ensureCount());
console.log(manager.updateActive(false));

const cache = new CacheManager();
console.log(cache.getOrSet("key", "value"));"#;

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
        output.contains("DataManager"),
        "expected DataManager in output. output: {output}"
    );
    assert!(
        output.contains("CacheManager"),
        "expected CacheManager in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with side effects with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_side_effects() {
    let source = r#"let callCount = 0;

function getSideEffect(): string {
    callCount++;
    return "side-effect-value";
}

let value1: string | null = null;
let value2: string | null = "existing";

value1 ??= getSideEffect();
value2 ??= getSideEffect();

console.log("Call count:", callCount);

const obj = {
    _value: null as string | null,
    get value(): string | null {
        console.log("getter called");
        return this._value;
    },
    set value(v: string | null) {
        console.log("setter called");
        this._value = v;
    }
};

obj.value ??= "default";

function conditionalSideEffect(condition: boolean): string | null {
    if (condition) {
        return "truthy";
    }
    return null;
}

let sideEffectResult: string | null = null;
sideEffectResult ||= conditionalSideEffect(true);
console.log(value1, value2, sideEffectResult);"#;

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
        output.contains("getSideEffect"),
        "expected getSideEffect in output. output: {output}"
    );
    assert!(
        output.contains("callCount"),
        "expected callCount in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for side effects"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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
        "expected non-empty source mappings for init order"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

