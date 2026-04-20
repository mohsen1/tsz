#[test]
fn test_source_map_loop_es5_for_of_destructuring() {
    // Test for-of with destructuring transformation
    let source = r#"const pairs = [[1, "one"], [2, "two"], [3, "three"]];

for (const [num, name] of pairs) {
    console.log(num, name);
}

const entries = [{ id: 1, value: "a" }, { id: 2, value: "b" }];

for (const { id, value } of entries) {
    console.log(id, value);
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
        output.contains("pairs") && output.contains("entries"),
        "expected pairs and entries arrays in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_in_destructuring() {
    // Test for-in with object property access
    let source = r#"const config = {
    host: "localhost",
    port: 8080,
    debug: true
};

const settings: { [key: string]: any } = {};

for (const prop in config) {
    if (config.hasOwnProperty(prop)) {
        settings[prop] = config[prop as keyof typeof config];
    }
}

console.log(settings);"#;

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
        output.contains("config") && output.contains("settings"),
        "expected config and settings in output. output: {output}"
    );
    assert!(
        output.contains("hasOwnProperty"),
        "expected hasOwnProperty call in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-in destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_string() {
    // Test for-of with string iteration
    let source = r#"const message = "Hello";
const chars: string[] = [];

for (const char of message) {
    chars.push(char.toUpperCase());
}

console.log(chars.join("-"));"#;

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
        output.contains("message") && output.contains("chars"),
        "expected message and chars in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of string"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_map_set() {
    // Test for-of with Map and Set iteration
    let source = r#"const map = new Map<string, number>();
map.set("a", 1);
map.set("b", 2);

for (const [key, value] of map) {
    console.log(key, value);
}

const set = new Set<number>([1, 2, 3]);

for (const item of set) {
    console.log(item);
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
        output.contains("Map") && output.contains("Set"),
        "expected Map and Set in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of Map/Set"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_nested_for_of() {
    // Test nested for-of loops
    let source = r#"const matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
let total = 0;

for (const row of matrix) {
    for (const cell of row) {
        total += cell;
    }
}

const nested = [["a", "b"], ["c", "d"]];

for (const outer of nested) {
    for (const inner of outer) {
        console.log(inner);
    }
}

console.log(total);"#;

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
        output.contains("matrix") && output.contains("nested"),
        "expected matrix and nested arrays in output. output: {output}"
    );
    assert!(
        output.contains("total"),
        "expected total variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested for-of"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_break_continue() {
    // Test for-of with break and continue
    let source = r#"const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let result: number[] = [];

for (const num of numbers) {
    if (num === 3) {
        continue;
    }
    if (num > 7) {
        break;
    }
    result.push(num);
}

outer: for (const x of [1, 2, 3]) {
    for (const y of [4, 5, 6]) {
        if (x === 2 && y === 5) {
            break outer;
        }
        console.log(x, y);
    }
}

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
        output.contains("numbers") && output.contains("result"),
        "expected numbers and result in output. output: {output}"
    );
    assert!(
        output.contains("break") || output.contains("continue"),
        "expected break/continue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of break/continue"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_iterator() {
    // Test for-of with custom iterator
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator]() {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

const range = new Range(1, 5);

for (const num of range) {
    console.log(num);
}

function* generateNumbers() {
    yield 1;
    yield 2;
    yield 3;
}

for (const n of generateNumbers()) {
    console.log(n);
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
        output.contains("Range"),
        "expected Range class in output. output: {output}"
    );
    assert!(
        output.contains("generateNumbers"),
        "expected generateNumbers function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of iterator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_comprehensive() {
    // Comprehensive for-of/for-in test
    let source = r#"// Data structures
interface User {
    id: number;
    name: string;
    roles: string[];
}

const users: User[] = [
    { id: 1, name: "Alice", roles: ["admin", "user"] },
    { id: 2, name: "Bob", roles: ["user"] }
];

// for-of with object destructuring
for (const { id, name, roles } of users) {
    console.log("User:", id, name);

    // Nested for-of with array
    for (const role of roles) {
        console.log("  Role:", role);
    }
}

// for-in with property enumeration
const config: { [key: string]: any } = {
    host: "localhost",
    port: 8080,
    debug: true,
    timeout: 5000
};

const configCopy: { [key: string]: any } = {};

for (const key in config) {
    if (Object.prototype.hasOwnProperty.call(config, key)) {
        configCopy[key] = config[key];
    }
}

// for-of with index tracking
const items = ["a", "b", "c"];
let index = 0;

for (const item of items) {
    console.log(index, item);
    index++;
}

// for-of with Map entries
const userMap = new Map<number, string>();
userMap.set(1, "Alice");
userMap.set(2, "Bob");

for (const [userId, userName] of userMap.entries()) {
    console.log("Map entry:", userId, userName);
}

// Labeled for-of with break
search: for (const user of users) {
    for (const role of user.roles) {
        if (role === "admin") {
            console.log("Found admin:", user.name);
            break search;
        }
    }
}

console.log("Config copy:", configCopy);"#;

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
        output.contains("users"),
        "expected users array in output. output: {output}"
    );
    assert!(
        output.contains("config"),
        "expected config object in output. output: {output}"
    );
    assert!(
        output.contains("userMap"),
        "expected userMap in output. output: {output}"
    );
    assert!(
        output.contains("hasOwnProperty"),
        "expected hasOwnProperty check in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive for-of/for-in"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Async/Await Transform ES5 Source Map Tests
// =============================================================================
// Tests for async/await compilation with ES5 target focusing on:
// try/catch in async, Promise chain transforms, async arrow functions.

#[test]
fn test_source_map_async_es5_try_catch_basic() {
    // Test basic try/catch in async function
    let source = r#"async function fetchData(url: string): Promise<string> {
    try {
        const response = await fetch(url);
        const data = await response.text();
        return data;
    } catch (error) {
        console.error("Fetch failed:", error);
        return "";
    }
}

fetchData("https://api.example.com/data");"#;

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
        output.contains("fetchData"),
        "expected fetchData function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async try/catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_try_catch_finally() {
    // Test try/catch/finally in async function
    let source = r#"async function processWithCleanup(): Promise<void> {
    let resource: any = null;
    try {
        resource = await acquireResource();
        await processResource(resource);
    } catch (error) {
        console.error("Processing failed:", error);
        throw error;
    } finally {
        if (resource) {
            await releaseResource(resource);
        }
        console.log("Cleanup complete");
    }
}

async function acquireResource() {
    return { id: 1 };
}

async function processResource(r: any) {
    console.log("Processing", r);
}

async function releaseResource(r: any) {
    console.log("Releasing", r);
}

processWithCleanup();"#;

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
        output.contains("processWithCleanup"),
        "expected processWithCleanup function in output. output: {output}"
    );
    assert!(
        output.contains("acquireResource") && output.contains("releaseResource"),
        "expected resource functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async try/catch/finally"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

