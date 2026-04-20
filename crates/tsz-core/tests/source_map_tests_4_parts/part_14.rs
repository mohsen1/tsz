/// Test optional chaining element access with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_element_access() {
    let source = r#"interface Data {
    items?: string[];
    matrix?: number[][];
    records?: Record<string, any>;
}

function getItem(data: Data | null, index: number) {
    return data?.items?.[index];
}

function getMatrixCell(data: Data, row: number, col: number) {
    return data?.matrix?.[row]?.[col];
}

function getRecord(data: Data, key: string) {
    return data?.records?.[key];
}

const data: Data = { items: ["a", "b", "c"] };
const first = data?.items?.[0];
const dynamic = data?.records?.["dynamic-key"];"#;

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
        output.contains("getItem"),
        "expected getItem in output. output: {output}"
    );
    assert!(
        output.contains("getMatrixCell"),
        "expected getMatrixCell in output. output: {output}"
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

/// Test nested optional chaining with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_nested() {
    let source = r#"interface DeepNested {
    level1?: {
        level2?: {
            level3?: {
                level4?: {
                    value?: string;
                };
            };
        };
    };
}

function getDeepValue(obj: DeepNested | null): string | undefined {
    return obj?.level1?.level2?.level3?.level4?.value;
}

const nested: DeepNested = {};
const deep = nested?.level1?.level2?.level3?.level4?.value;
const partial = nested?.level1?.level2;
console.log(deep, partial);"#;

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
        output.contains("getDeepValue"),
        "expected getDeepValue in output. output: {output}"
    );
    assert!(
        output.contains("nested"),
        "expected nested in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested chaining"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with nullish coalescing with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_with_nullish() {
    let source = r#"interface Config {
    settings?: {
        theme?: string;
        timeout?: number;
    };
}

function getTheme(config: Config | null): string {
    return config?.settings?.theme ?? "default";
}

function getTimeout(config: Config): number {
    return config?.settings?.timeout ?? 5000;
}

const config: Config = {};
const theme = config?.settings?.theme ?? "light";
const timeout = config?.settings?.timeout ?? 3000;
const nested = config?.settings?.theme ?? config?.settings?.timeout ?? "fallback";
console.log(theme, timeout, nested);"#;

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
        output.contains("getTheme"),
        "expected getTheme in output. output: {output}"
    );
    assert!(
        output.contains("getTimeout"),
        "expected getTimeout in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining in function context with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_function_context() {
    let source = r#"interface Handler {
    callback?: (data: string) => void;
    transform?: (input: number) => number;
}

function invokeHandler(handler: Handler | undefined, data: string) {
    handler?.callback?.(data);
}

function transformValue(handler: Handler, value: number): number | undefined {
    return handler?.transform?.(value);
}

const handlers: Handler[] = [];
const result = handlers[0]?.callback?.("test");
const mapped = handlers.map(h => h?.transform?.(42));

function chainedCalls(handler: Handler | null) {
    const fn = handler?.transform;
    return fn?.(100);
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
        output.contains("invokeHandler"),
        "expected invokeHandler in output. output: {output}"
    );
    assert!(
        output.contains("transformValue"),
        "expected transformValue in output. output: {output}"
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

/// Test optional chaining with chained methods with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_chained_methods() {
    let source = r#"interface Builder {
    setName?(name: string): Builder;
    setValue?(value: number): Builder;
    build?(): object;
}

function buildObject(builder: Builder | null) {
    return builder?.setName?.("test")?.setValue?.(42)?.build?.();
}

class FluentApi {
    private data: any;

    with?(key: string): FluentApi | undefined {
        return this;
    }

    get?(): any {
        return this.data;
    }
}

const api = new FluentApi();
const result = api?.with?.("key")?.get?.();
console.log(result);"#;

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
        output.contains("buildObject"),
        "expected buildObject in output. output: {output}"
    );
    assert!(
        output.contains("FluentApi"),
        "expected FluentApi in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with delete operator with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_delete() {
    let source = r#"interface Obj {
    prop?: {
        nested?: string;
    };
    items?: string[];
}

function deleteProp(obj: Obj | null) {
    delete obj?.prop?.nested;
}

function deleteElement(obj: Obj | undefined, index: number) {
    delete obj?.items?.[index];
}

const obj: Obj = { prop: { nested: "value" } };
delete obj?.prop?.nested;
delete obj?.items?.[0];
console.log(obj);"#;

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
        output.contains("deleteProp"),
        "expected deleteProp in output. output: {output}"
    );
    assert!(
        output.contains("deleteElement"),
        "expected deleteElement in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for delete operator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with call expression with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_call_expression() {
    let source = r#"type Callback = ((value: number) => void) | undefined;

function invokeCallback(cb: Callback, value: number) {
    cb?.(value);
}

const callbacks: Callback[] = [undefined, (v) => console.log(v)];
callbacks[0]?.(1);
callbacks[1]?.(2);

interface EventEmitter {
    on?: (event: string, handler: Function) => void;
    emit?: (event: string, ...args: any[]) => void;
}

function setupEmitter(emitter: EventEmitter | null) {
    emitter?.on?.("data", console.log);
    emitter?.emit?.("ready");
}

const maybeFunc: (() => number) | null = null;
const result = maybeFunc?.();
console.log(result);"#;

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
        output.contains("invokeCallback"),
        "expected invokeCallback in output. output: {output}"
    );
    assert!(
        output.contains("setupEmitter"),
        "expected setupEmitter in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

