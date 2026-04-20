#[test]
fn test_source_map_destructuring_es5_nested_array() {
    let source = r#"const matrix = [[1, 2], [3, 4], [5, 6]];
const [[a, b], [c, d], [e, f]] = matrix;
console.log(a, b, c, d, e, f);

const deep = [[[1, 2]], [[3, 4]]];
const [[[x, y]], [[z, w]]] = deep;
console.log(x + y + z + w);

const mixed = [1, [2, [3, [4]]]];
const [one, [two, [three, [four]]]] = mixed;
console.log(one, two, three, four);"#;

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
        output.contains("matrix"),
        "expected matrix variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested array destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_nested_object() {
    let source = r#"const user = {
    profile: {
        name: "Bob",
        address: {
            city: "Boston",
            zip: "02101"
        }
    },
    settings: {
        theme: "dark",
        notifications: true
    }
};

const { profile: { name, address: { city, zip } } } = user;
console.log(name, city, zip);

const { settings: { theme, notifications } } = user;
console.log(theme, notifications);"#;

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
        output.contains("user"),
        "expected user variable in output. output: {output}"
    );
    assert!(
        output.contains("profile"),
        "expected profile property in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested object destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_mixed() {
    let source = r#"const data = {
    users: [
        { id: 1, name: "Alice" },
        { id: 2, name: "Bob" }
    ],
    metadata: {
        count: 2,
        tags: ["admin", "user"]
    }
};

const { users: [first, second], metadata: { count, tags: [tag1, tag2] } } = data;
console.log(first.name, second.name, count, tag1, tag2);

const response = { items: [[1, 2], [3, 4]], status: { code: 200 } };
const { items: [[a, b], [c, d]], status: { code } } = response;
console.log(a, b, c, d, code);"#;

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
        output.contains("metadata"),
        "expected metadata object in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixed destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_defaults() {
    let source = r#"const [a = 1, b = 2, c = 3] = [10];
console.log(a, b, c);

const { name = "Unknown", age = 0, active = true } = { name: "Alice" };
console.log(name, age, active);

const { config: { timeout = 5000, retries = 3 } = {} } = {};
console.log(timeout, retries);

function process({ value = 0, label = "default" } = {}) {
    return label + ": " + value;
}
console.log(process({ value: 42 }));"#;

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
        output.contains("process"),
        "expected process function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for destructuring with defaults"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_function_params() {
    let source = r#"function processUser({ name, age, email }: { name: string; age: number; email: string }) {
    console.log(name, age, email);
}

function processCoords([x, y, z]: number[]) {
    return x + y + z;
}

const greet = ({ firstName, lastName }: { firstName: string; lastName: string }) => {
    return "Hello, " + firstName + " " + lastName;
};

class Handler {
    handle({ type, payload }: { type: string; payload: any }) {
        console.log(type, payload);
    }
}

processUser({ name: "Alice", age: 30, email: "alice@example.com" });
console.log(processCoords([1, 2, 3]));
console.log(greet({ firstName: "John", lastName: "Doe" }));"#;

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
        output.contains("processUser"),
        "expected processUser function in output. output: {output}"
    );
    assert!(
        output.contains("processCoords"),
        "expected processCoords function in output. output: {output}"
    );
    assert!(
        output.contains("greet"),
        "expected greet function in output. output: {output}"
    );
    assert!(
        output.contains("Handler"),
        "expected Handler class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function parameter destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_rest_patterns() {
    let source = r#"const [first, second, ...rest] = [1, 2, 3, 4, 5];
console.log(first, second, rest);

const { name, ...others } = { name: "Alice", age: 30, city: "NYC" };
console.log(name, others);

function processItems([head, ...tail]: number[]) {
    console.log("Head:", head);
    console.log("Tail:", tail);
    return tail.reduce((a, b) => a + b, head);
}

const { a, b, ...remaining } = { a: 1, b: 2, c: 3, d: 4, e: 5 };
console.log(a, b, remaining);

console.log(processItems([10, 20, 30, 40]));"#;

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
        output.contains("rest"),
        "expected rest variable in output. output: {output}"
    );
    assert!(
        output.contains("processItems"),
        "expected processItems function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest pattern destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_loop_patterns() {
    let source = r#"const pairs = [[1, "one"], [2, "two"], [3, "three"]];
for (const [num, word] of pairs) {
    console.log(num + ": " + word);
}

const users = [
    { id: 1, name: "Alice" },
    { id: 2, name: "Bob" },
    { id: 3, name: "Charlie" }
];
for (const { id, name } of users) {
    console.log(id + " - " + name);
}

const entries = new Map([["a", 1], ["b", 2], ["c", 3]]);
for (const [key, value] of entries) {
    console.log(key + " => " + value);
}

const matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
for (const [a, b, c] of matrix) {
    console.log(a + b + c);
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
        output.contains("pairs"),
        "expected pairs array in output. output: {output}"
    );
    assert!(
        output.contains("users"),
        "expected users array in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for loop destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_comprehensive() {
    let source = r#"// Comprehensive destructuring patterns for ES5 transform testing

// Basic array and object destructuring
const [x, y, z] = [1, 2, 3];
const { name, age } = { name: "Alice", age: 30 };

// Nested destructuring
const {
    user: {
        profile: { firstName, lastName },
        settings: { theme }
    }
} = {
    user: {
        profile: { firstName: "John", lastName: "Doe" },
        settings: { theme: "dark" }
    }
};

// Destructuring with defaults
const [a = 1, b = 2] = [10];
const { timeout = 5000, retries = 3 } = {};

// Rest patterns
const [head, ...tail] = [1, 2, 3, 4, 5];
const { id, ...metadata } = { id: 1, type: "user", active: true };

// Function parameter destructuring
function processConfig({
    server: { host = "localhost", port = 8080 },
    database: { name: dbName, pool = 10 }
}: {
    server: { host?: string; port?: number };
    database: { name: string; pool?: number };
}) {
    return host + ":" + port + " -> " + dbName + " (pool: " + pool + ")";
}

// Arrow function with destructuring
const formatUser = ({ name, email }: { name: string; email: string }) =>
    name + " <" + email + ">";

// Class with destructuring in methods
class DataProcessor {
    private data: any[];

    constructor(data: any[]) {
        this.data = data;
    }

    process() {
        return this.data.map(({ value, label }) => label + ": " + value);
    }

    *iterate() {
        for (const { id, ...rest } of this.data) {
            yield { id, processed: true, ...rest };
        }
    }
}

// Destructuring in loops
const pairs: [string, number][] = [["a", 1], ["b", 2], ["c", 3]];
for (const [key, val] of pairs) {
    console.log(key + " = " + val);
}

// Complex nested array/object mix
const response = {
    data: {
        users: [
            { id: 1, info: { name: "Alice", scores: [90, 85, 92] } },
            { id: 2, info: { name: "Bob", scores: [88, 91, 87] } }
        ]
    },
    meta: { total: 2, page: 1 }
};

const {
    data: { users: [{ info: { name: user1Name, scores: [score1] } }] },
    meta: { total }
} = response;

// Swap using destructuring
let m = 1, n = 2;
[m, n] = [n, m];

// Computed property names with destructuring
const key = "dynamicKey";
const { [key]: dynamicValue } = { dynamicKey: "found!" };

// Usage
console.log(x, y, z, name, age);
console.log(firstName, lastName, theme);
console.log(a, b, timeout, retries);
console.log(head, tail, id, metadata);
console.log(processConfig({
    server: { host: "api.example.com" },
    database: { name: "mydb" }
}));
console.log(formatUser({ name: "Test", email: "test@example.com" }));

const processor = new DataProcessor([
    { id: 1, value: 10, label: "A" },
    { id: 2, value: 20, label: "B" }
]);
console.log(processor.process());

console.log(user1Name, score1, total);
console.log(m, n);
console.log(dynamicValue);"#;

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
        output.contains("processConfig"),
        "expected processConfig function in output. output: {output}"
    );
    assert!(
        output.contains("formatUser"),
        "expected formatUser function in output. output: {output}"
    );
    assert!(
        output.contains("DataProcessor"),
        "expected DataProcessor class in output. output: {output}"
    );
    assert!(
        output.contains("dynamicValue"),
        "expected dynamicValue variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Spread/Rest Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_spread_rest_es5_array_spread_basic() {
    let source = r#"const arr1 = [1, 2, 3];
const arr2 = [4, 5, 6];
const combined = [...arr1, ...arr2];
console.log(combined);

const numbers = [10, 20, 30];
const expanded = [...numbers];
console.log(expanded);

const nested = [[1, 2], [3, 4]];
const flat = [...nested[0], ...nested[1]];
console.log(flat);"#;

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
        output.contains("arr1"),
        "expected arr1 in output. output: {output}"
    );
    assert!(
        output.contains("combined"),
        "expected combined in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array spread"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_object_spread_basic() {
    let source = r#"const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3, d: 4 };
const merged = { ...obj1, ...obj2 };
console.log(merged);

const defaults = { theme: "light", lang: "en" };
const settings = { ...defaults, theme: "dark" };
console.log(settings);

const person = { name: "Alice", age: 30 };
const updated = { ...person, age: 31 };
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
        output.contains("obj1"),
        "expected obj1 in output. output: {output}"
    );
    assert!(
        output.contains("merged"),
        "expected merged in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object spread"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

