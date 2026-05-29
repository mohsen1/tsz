/// Comprehensive test for optional chaining patterns with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_comprehensive() {
    let source = r#"// Complex optional chaining scenarios
interface User {
    id: number;
    name?: string;
    profile?: {
        avatar?: string;
        settings?: {
            theme?: string;
            notifications?: boolean;
        };
    };
    friends?: User[];
    getFriendById?(id: number): User | undefined;
}

interface AppState {
    currentUser?: User;
    users?: Map<number, User>;
    cache?: {
        get?(key: string): any;
        set?(key: string, value: any): void;
    };
}

class UserService {
    private state: AppState;

    constructor(state: AppState) {
        this.state = state;
    }

    getCurrentUserName(): string {
        return this.state?.currentUser?.name ?? "Anonymous";
    }

    getUserAvatar(): string | undefined {
        return this.state?.currentUser?.profile?.avatar;
    }

    getUserTheme(): string {
        return this.state?.currentUser?.profile?.settings?.theme ?? "light";
    }

    getNotificationsEnabled(): boolean {
        return this.state?.currentUser?.profile?.settings?.notifications ?? true;
    }

    getFriendName(index: number): string | undefined {
        return this.state?.currentUser?.friends?.[index]?.name;
    }

    findFriend(userId: number, friendId: number): User | undefined {
        const user = this.state?.users?.get(userId);
        return user?.getFriendById?.(friendId);
    }

    getCachedValue(key: string): any {
        return this.state?.cache?.get?.(key);
    }

    setCachedValue(key: string, value: any): void {
        this.state?.cache?.set?.(key, value);
    }
}

// Utility functions with optional chaining
function safeAccess<T, K extends keyof T>(obj: T | null | undefined, key: K): T[K] | undefined {
    return obj?.[key];
}

function safeCall<T, R>(fn: ((arg: T) => R) | undefined, arg: T): R | undefined {
    return fn?.(arg);
}

function deepGet(obj: any, ...keys: string[]): any {
    let current = obj;
    for (const key of keys) {
        current = current?.[key];
        if (current === undefined) break;
    }
    return current;
}

// Usage examples
const appState: AppState = {
    currentUser: {
        id: 1,
        name: "Alice",
        profile: {
            avatar: "avatar.png",
            settings: {
                theme: "dark"
            }
        },
        friends: [
            { id: 2, name: "Bob" },
            { id: 3, name: "Charlie" }
        ]
    }
};

const service = new UserService(appState);
console.log(service.getCurrentUserName());
console.log(service.getUserAvatar());
console.log(service.getUserTheme());
console.log(service.getNotificationsEnabled());
console.log(service.getFriendName(0));

// Edge cases
const nullUser: User | null = null;
const undefinedFriends = nullUser?.friends?.[0]?.name;
const chainedMethods = nullUser?.getFriendById?.(1)?.getFriendById?.(2);
const mixedAccess = appState?.currentUser?.friends?.[0]?.profile?.settings?.theme ?? "default";"#;

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
        output.contains("safeAccess"),
        "expected safeAccess in output. output: {output}"
    );
    assert!(
        output.contains("safeCall"),
        "expected safeCall in output. output: {output}"
    );
    assert!(
        output.contains("deepGet"),
        "expected deepGet in output. output: {output}"
    );
    assert!(
        output.contains("appState"),
        "expected appState in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive optional chaining"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - NULLISH COALESCING PATTERNS
// =============================================================================
// Tests for nullish coalescing patterns with ES5 target to verify source maps
// work correctly with nullish coalescing transforms.

/// Test basic nullish coalescing with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_basic() {
    let source = r#"const value1: string | null = null;
const value2: string | undefined = undefined;
const value3: string | null | undefined = "hello";

const result1 = value1 ?? "default1";
const result2 = value2 ?? "default2";
const result3 = value3 ?? "default3";

console.log(result1, result2, result3);"#;

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
        output.contains("result1"),
        "expected result1 in output. output: {output}"
    );
    assert!(
        output.contains("result2"),
        "expected result2 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with null values with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_with_null() {
    let source = r#"function getValueOrDefault(input: string | null): string {
    return input ?? "fallback";
}

const nullValue: string | null = null;
const nonNullValue: string | null = "actual";

const result1 = nullValue ?? "was null";
const result2 = nonNullValue ?? "was null";
const result3 = getValueOrDefault(null);
const result4 = getValueOrDefault("test");

console.log(result1, result2, result3, result4);"#;

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
        output.contains("getValueOrDefault"),
        "expected getValueOrDefault in output. output: {output}"
    );
    assert!(
        output.contains("nullValue"),
        "expected nullValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for null values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with undefined values with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_with_undefined() {
    let source = r#"function getOrUndefined(): string | undefined {
    return undefined;
}

function getOrValue(): string | undefined {
    return "value";
}

const undefinedValue: string | undefined = undefined;
const definedValue: string | undefined = "defined";

const result1 = undefinedValue ?? "was undefined";
const result2 = definedValue ?? "was undefined";
const result3 = getOrUndefined() ?? "fallback";
const result4 = getOrValue() ?? "fallback";

console.log(result1, result2, result3, result4);"#;

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
        output.contains("getOrUndefined"),
        "expected getOrUndefined in output. output: {output}"
    );
    assert!(
        output.contains("getOrValue"),
        "expected getOrValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for undefined values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test chained nullish coalescing with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_chained() {
    let source = r#"const first: string | null = null;
const second: string | undefined = undefined;
const third: string | null = null;
const fourth: string = "found";

const result1 = first ?? second ?? third ?? fourth;
const result2 = first ?? second ?? "early default";
const result3 = first ?? "immediate default" ?? second;

function chainedDefaults(
    a: string | null,
    b: string | undefined,
    c: string | null
): string {
    return a ?? b ?? c ?? "final fallback";
}

const chainResult = chainedDefaults(null, undefined, "c-value");
console.log(result1, result2, result3, chainResult);"#;

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
        output.contains("first"),
        "expected first in output. output: {output}"
    );
    assert!(
        output.contains("chainedDefaults"),
        "expected chainedDefaults in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with function calls with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_function_call() {
    let source = r#"function maybeGetValue(): string | null {
    return Math.random() > 0.5 ? "value" : null;
}

function getDefault(): string {
    return "default from function";
}

const result1 = maybeGetValue() ?? "inline default";
const result2 = maybeGetValue() ?? getDefault();

class DataProvider {
    getValue(): string | undefined {
        return undefined;
    }

    getDefault(): string {
        return "class default";
    }

    getResult(): string {
        return this.getValue() ?? this.getDefault();
    }
}

const provider = new DataProvider();
const classResult = provider.getResult();
console.log(result1, result2, classResult);"#;

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
        output.contains("maybeGetValue"),
        "expected maybeGetValue in output. output: {output}"
    );
    assert!(
        output.contains("DataProvider"),
        "expected DataProvider in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing in assignments with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_assignment() {
    let source = r#"let value: string | null = null;
let result: string;

result = value ?? "assigned default";

function assignWithDefault(input: number | undefined): number {
    let output: number;
    output = input ?? 0;
    return output;
}

class Container {
    private data: string | null = null;

    setData(value: string | null): void {
        this.data = value ?? "container default";
    }

    getData(): string {
        return this.data ?? "no data";
    }
}

const container = new Container();
container.setData(null);
const containerData = container.getData();
console.log(result, containerData);"#;

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
        output.contains("assignWithDefault"),
        "expected assignWithDefault in output. output: {output}"
    );
    assert!(
        output.contains("Container"),
        "expected Container in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing in conditionals with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_conditional() {
    let source = r#"function processValue(input: string | null): string {
    if ((input ?? "default") === "default") {
        return "was nullish";
    }
    return input ?? "unreachable";
}

const value: number | undefined = undefined;
const condition = (value ?? 0) > 10;

const ternaryResult = (value ?? 0) > 5 ? "big" : "small";

function conditionalChain(a: boolean | null, b: boolean | undefined): boolean {
    return (a ?? false) && (b ?? true);
}

const chainResult = conditionalChain(null, undefined);
console.log(condition, ternaryResult, chainResult);"#;

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
        output.contains("processValue"),
        "expected processValue in output. output: {output}"
    );
    assert!(
        output.contains("conditionalChain"),
        "expected conditionalChain in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with objects with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_objects() {
    let source = r#"interface Config {
    name?: string;
    count?: number;
    enabled?: boolean;
}

const config: Config | null = null;
const defaultConfig: Config = { name: "default", count: 0, enabled: true };

const finalConfig = config ?? defaultConfig;

function mergeConfigs(base: Config | undefined, override: Config | null): Config {
    return override ?? base ?? { name: "fallback", count: -1, enabled: false };
}

const merged = mergeConfigs(undefined, null);

const partialConfig: Config = {
    name: (config ?? defaultConfig).name ?? "unnamed",
    count: (config ?? defaultConfig).count ?? 0,
    enabled: (config ?? defaultConfig).enabled ?? false
};

console.log(finalConfig, merged, partialConfig);"#;

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
        output.contains("finalConfig"),
        "expected finalConfig in output. output: {output}"
    );
    assert!(
        output.contains("mergeConfigs"),
        "expected mergeConfigs in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for objects"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with optional chaining with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_with_optional_chaining() {
    let source = r#"interface User {
    name?: string;
    profile?: {
        email?: string;
        settings?: {
            theme?: string;
            language?: string;
        };
    };
}

function getUserTheme(user: User | null): string {
    return user?.profile?.settings?.theme ?? "light";
}

function getUserLanguage(user: User | undefined): string {
    return user?.profile?.settings?.language ?? "en";
}

const user: User | null = null;
const theme = user?.profile?.settings?.theme ?? "default-theme";
const language = user?.profile?.settings?.language ?? "default-lang";
const email = user?.profile?.email ?? "no-email@example.com";

class UserService {
    private user: User | null = null;

    getTheme(): string {
        return this.user?.profile?.settings?.theme ?? "system";
    }

    getDisplayName(): string {
        return this.user?.name ?? "Guest";
    }
}

const service = new UserService();
console.log(theme, language, email, service.getTheme());"#;

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
        output.contains("getUserTheme"),
        "expected getUserTheme in output. output: {output}"
    );
    assert!(
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test for nullish coalescing patterns with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_comprehensive() {
    let source = r#"// Comprehensive nullish coalescing scenarios
interface AppConfig {
    apiUrl?: string;
    timeout?: number;
    retries?: number;
    headers?: Record<string, string>;
    features?: {
        darkMode?: boolean;
        notifications?: boolean;
        analytics?: boolean;
    };
}

interface User {
    id: number;
    name?: string;
    email?: string;
    preferences?: AppConfig;
}

class ConfigManager {
    private defaultConfig: AppConfig = {
        apiUrl: "https://api.example.com",
        timeout: 5000,
        retries: 3,
        features: {
            darkMode: false,
            notifications: true,
            analytics: true
        }
    };

    private userConfig: AppConfig | null = null;

    setUserConfig(config: AppConfig | null): void {
        this.userConfig = config;
    }

    getApiUrl(): string {
        return this.userConfig?.apiUrl ?? this.defaultConfig.apiUrl ?? "https://fallback.com";
    }

    getTimeout(): number {
        return this.userConfig?.timeout ?? this.defaultConfig.timeout ?? 1000;
    }

    getRetries(): number {
        return this.userConfig?.retries ?? this.defaultConfig.retries ?? 0;
    }

    isDarkModeEnabled(): boolean {
        return this.userConfig?.features?.darkMode ?? this.defaultConfig.features?.darkMode ?? false;
    }

    areNotificationsEnabled(): boolean {
        return this.userConfig?.features?.notifications ?? this.defaultConfig.features?.notifications ?? true;
    }
}

class UserManager {
    private users: Map<number, User> = new Map();

    addUser(user: User): void {
        this.users.set(user.id, user);
    }

    getUserName(id: number): string {
        return this.users.get(id)?.name ?? "Unknown User";
    }

    getUserEmail(id: number): string {
        return this.users.get(id)?.email ?? "no-reply@example.com";
    }

    getUserApiUrl(id: number): string {
        const user = this.users.get(id);
        return user?.preferences?.apiUrl ?? "https://default-api.com";
    }
}

// Utility functions
function coalesce<T>(value: T | null | undefined, defaultValue: T): T {
    return value ?? defaultValue;
}

function coalesceMany<T>(...values: (T | null | undefined)[]): T | undefined {
    for (const value of values) {
        if (value !== null && value !== undefined) {
            return value;
        }
    }
    return undefined;
}

function getNestedValue<T>(
    obj: any,
    path: string[],
    defaultValue: T
): T {
    let current = obj;
    for (const key of path) {
        current = current?.[key];
        if (current === null || current === undefined) {
            return defaultValue;
        }
    }
    return current ?? defaultValue;
}

// Usage
const configManager = new ConfigManager();
const userManager = new UserManager();

userManager.addUser({ id: 1, name: "Alice" });
userManager.addUser({ id: 2, email: "bob@example.com" });

console.log(configManager.getApiUrl());
console.log(configManager.getTimeout());
console.log(configManager.isDarkModeEnabled());
console.log(userManager.getUserName(1));
console.log(userManager.getUserEmail(2));

const maybeValue: string | null = null;
const result = coalesce(maybeValue, "fallback");
const multiResult = coalesceMany<string>(null, undefined, "found", "ignored");

console.log(result, multiResult);"#;

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
        output.contains("UserManager"),
        "expected UserManager in output. output: {output}"
    );
    assert!(
        output.contains("coalesce"),
        "expected coalesce in output. output: {output}"
    );
    assert!(
        output.contains("coalesceMany"),
        "expected coalesceMany in output. output: {output}"
    );
    assert!(
        output.contains("getNestedValue"),
        "expected getNestedValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - LOGICAL ASSIGNMENT PATTERNS
// =============================================================================
// Tests for logical assignment patterns (&&=, ||=, ??=) with ES5 target to verify
// source maps work correctly with logical assignment transforms.

/// Test &&= logical AND assignment with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_and_assign() {
    let source = r#"let value1: string | null = "hello";
let value2: string | null = null;

value1 &&= "updated";
value2 &&= "updated";

function updateIfTruthy(input: string | null): string | null {
    let result = input;
    result &&= "modified";
    return result;
}

const result1 = updateIfTruthy("test");
const result2 = updateIfTruthy(null);
console.log(value1, value2, result1, result2);"#;

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
        output.contains("value1"),
        "expected value1 in output. output: {output}"
    );
    assert!(
        output.contains("updateIfTruthy"),
        "expected updateIfTruthy in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for &&= assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test ||= logical OR assignment with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_or_assign() {
    let source = r#"let value1: string | null = null;
let value2: string | null = "existing";

value1 ||= "default";
value2 ||= "default";

function setDefault(input: string | null): string {
    let result: string | null = input;
    result ||= "fallback";
    return result;
}

const result1 = setDefault(null);
const result2 = setDefault("provided");
console.log(value1, value2, result1, result2);"#;

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
        output.contains("value1"),
        "expected value1 in output. output: {output}"
    );
    assert!(
        output.contains("setDefault"),
        "expected setDefault in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ||= assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

