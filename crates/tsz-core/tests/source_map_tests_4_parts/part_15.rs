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

