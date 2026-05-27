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

#[test]
fn test_source_map_async_es5_promise_chain() {
    // Test Promise chain transforms
    let source = r#"function fetchUser(id: number): Promise<{ name: string }> {
    return Promise.resolve({ name: "User" + id });
}

function fetchPosts(userId: number): Promise<string[]> {
    return Promise.resolve(["Post 1", "Post 2"]);
}

async function getUserWithPosts(id: number) {
    const user = await fetchUser(id);
    const posts = await fetchPosts(id);
    return { user, posts };
}

// Promise chain equivalent
function getUserWithPostsChain(id: number) {
    return fetchUser(id)
        .then(function(user) {
            return fetchPosts(id).then(function(posts) {
                return { user: user, posts: posts };
            });
        })
        .catch(function(error) {
            console.error(error);
            return null;
        });
}

getUserWithPosts(1);
getUserWithPostsChain(2);"#;

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
        output.contains("fetchUser") && output.contains("fetchPosts"),
        "expected fetch functions in output. output: {output}"
    );
    assert!(
        output.contains("getUserWithPosts"),
        "expected getUserWithPosts function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise chain"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_arrow_function() {
    // Test async arrow functions
    let source = r#"const fetchData = async (url: string): Promise<any> => {
    const response = await fetch(url);
    return response.json();
};

const processItems = async (items: number[]): Promise<number[]> => {
    const results: number[] = [];
    for (const item of items) {
        const processed = await processItem(item);
        results.push(processed);
    }
    return results;
};

const processItem = async (item: number): Promise<number> => item * 2;

const shortAsync = async () => "done";

const asyncWithDefault = async (value: number = 10) => value * 2;

fetchData("https://api.example.com");
processItems([1, 2, 3]);
shortAsync();
asyncWithDefault();"#;

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
        output.contains("fetchData") && output.contains("processItems"),
        "expected async arrow functions in output. output: {output}"
    );
    assert!(
        output.contains("shortAsync"),
        "expected shortAsync in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async arrow functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_class_methods() {
    // Test async class methods
    let source = r#"class ApiClient {
    private baseUrl: string;

    constructor(baseUrl: string) {
        this.baseUrl = baseUrl;
    }

    async get(endpoint: string): Promise<any> {
        try {
            const response = await fetch(this.baseUrl + endpoint);
            return await response.json();
        } catch (error) {
            console.error("GET failed:", error);
            throw error;
        }
    }

    async post(endpoint: string, data: any): Promise<any> {
        try {
            const response = await fetch(this.baseUrl + endpoint, {
                method: "POST",
                body: JSON.stringify(data)
            });
            return await response.json();
        } catch (error) {
            console.error("POST failed:", error);
            throw error;
        }
    }

    static async create(url: string): Promise<ApiClient> {
        return new ApiClient(url);
    }
}

const client = new ApiClient("https://api.example.com");
client.get("/users");
client.post("/users", { name: "Alice" });
ApiClient.create("https://api.example.com");"#;

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
        output.contains("ApiClient"),
        "expected ApiClient class in output. output: {output}"
    );
    assert!(
        output.contains("get") && output.contains("post"),
        "expected get/post methods in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_iife() {
    // Test async IIFE (Immediately Invoked Function Expression)
    let source = r#"(async function() {
    console.log("Starting async IIFE");
    const result = await Promise.resolve("done");
    console.log("Result:", result);
})();

(async () => {
    const data = await fetchData();
    console.log("Data:", data);
})();

async function fetchData() {
    return { value: 42 };
}

const asyncResult = (async () => {
    return await Promise.all([
        Promise.resolve(1),
        Promise.resolve(2),
        Promise.resolve(3)
    ]);
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
        output.contains("fetchData"),
        "expected fetchData function in output. output: {output}"
    );
    assert!(
        output.contains("asyncResult"),
        "expected asyncResult variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async IIFE"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_nested_try_catch() {
    // Test nested try/catch in async functions
    let source = r#"async function complexOperation(): Promise<string> {
    try {
        const first = await step1();
        try {
            const second = await step2(first);
            try {
                const third = await step3(second);
                return third;
            } catch (innerError) {
                console.error("Step 3 failed:", innerError);
                return await fallback3();
            }
        } catch (middleError) {
            console.error("Step 2 failed:", middleError);
            return await fallback2();
        }
    } catch (outerError) {
        console.error("Step 1 failed:", outerError);
        return await fallback1();
    }
}

async function step1() { return "step1"; }
async function step2(input: string) { return input + "-step2"; }
async function step3(input: string) { return input + "-step3"; }
async function fallback1() { return "fallback1"; }
async function fallback2() { return "fallback2"; }
async function fallback3() { return "fallback3"; }

complexOperation();"#;

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
        output.contains("complexOperation"),
        "expected complexOperation function in output. output: {output}"
    );
    assert!(
        output.contains("step1") && output.contains("step2") && output.contains("step3"),
        "expected step functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested try/catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_parallel_await() {
    // Test parallel await patterns with Promise.all/race
    let source = r#"async function fetchAllData(): Promise<[any, any, any]> {
    const [users, posts, comments] = await Promise.all([
        fetchUsers(),
        fetchPosts(),
        fetchComments()
    ]);
    return [users, posts, comments];
}

async function fetchFirstResponse(): Promise<any> {
    try {
        const result = await Promise.race([
            fetchFast(),
            fetchSlow(),
            timeout(5000)
        ]);
        return result;
    } catch (error) {
        return null;
    }
}

async function fetchUsers() { return [{ id: 1 }]; }
async function fetchPosts() { return [{ id: 1 }]; }
async function fetchComments() { return [{ id: 1 }]; }
async function fetchFast() { return "fast"; }
async function fetchSlow() { return "slow"; }
function timeout(ms: number): Promise<never> {
    return new Promise(function(_, reject) {
        setTimeout(function() { reject(new Error("Timeout")); }, ms);
    });
}

fetchAllData();
fetchFirstResponse();"#;

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
        output.contains("fetchAllData"),
        "expected fetchAllData function in output. output: {output}"
    );
    assert!(
        output.contains("Promise"),
        "expected Promise in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parallel await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_error_rethrow() {
    // Test error rethrowing patterns in async
    let source = r#"class CustomError extends Error {
    constructor(message: string, public code: number) {
        super(message);
        this.name = "CustomError";
    }
}

async function processWithRetry(maxRetries: number): Promise<string> {
    let lastError: Error | null = null;

    for (let i = 0; i < maxRetries; i++) {
        try {
            return await attemptOperation();
        } catch (error) {
            lastError = error as Error;
            console.log("Attempt " + (i + 1) + " failed, retrying...");
        }
    }

    throw lastError || new Error("All retries failed");
}

async function attemptOperation(): Promise<string> {
    if (Math.random() < 0.5) {
        throw new CustomError("Random failure", 500);
    }
    return "success";
}

async function wrapError(): Promise<void> {
    try {
        await processWithRetry(3);
    } catch (error) {
        throw new CustomError("Wrapped: " + (error as Error).message, 400);
    }
}

processWithRetry(3);
wrapError();"#;

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
        output.contains("CustomError"),
        "expected CustomError class in output. output: {output}"
    );
    assert!(
        output.contains("processWithRetry"),
        "expected processWithRetry function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for error rethrow"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_transform_comprehensive() {
    // Comprehensive async/await transform test
    let source = r#"// Types
interface ApiResponse<T> {
    data: T;
    status: number;
}

// Async arrow with type parameters
const fetchTyped = async <T>(url: string): Promise<ApiResponse<T>> => {
    try {
        const response = await fetch(url);
        const data = await response.json();
        return { data: data as T, status: response.status };
    } catch (error) {
        throw new Error("Fetch failed: " + (error as Error).message);
    }
};

// Class with async methods and try/catch
class DataService {
    private cache: Map<string, any> = new Map();

    async fetchWithCache(key: string): Promise<any> {
        if (this.cache.has(key)) {
            return this.cache.get(key);
        }

        try {
            const data = await this.fetchRemote(key);
            this.cache.set(key, data);
            return data;
        } catch (error) {
            console.error("Cache miss and fetch failed:", error);
            throw error;
        }
    }

    private async fetchRemote(key: string): Promise<any> {
        return { key: key, value: "data" };
    }

    async batchFetch(keys: string[]): Promise<any[]> {
        const promises = keys.map(async (key) => {
            try {
                return await this.fetchWithCache(key);
            } catch {
                return null;
            }
        });
        return Promise.all(promises);
    }
}

// Async generator simulation with try/catch
async function* asyncGenerator(): AsyncGenerator<number> {
    for (let i = 0; i < 5; i++) {
        try {
            yield await Promise.resolve(i);
        } catch {
            yield -1;
        }
    }
}

// IIFE with complex async flow
const initApp = (async () => {
    try {
        console.log("Initializing...");
        const service = new DataService();
        const data = await service.batchFetch(["a", "b", "c"]);
        console.log("Data loaded:", data);
        return { success: true, data: data };
    } catch (error) {
        console.error("Init failed:", error);
        return { success: false, error: error };
    } finally {
        console.log("Init complete");
    }
})();

// Usage
fetchTyped<{ name: string }>("/api/user");
const service = new DataService();
service.fetchWithCache("test");
initApp.then(function(result) { console.log(result); });"#;

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
        output.contains("fetchTyped"),
        "expected fetchTyped function in output. output: {output}"
    );
    assert!(
        output.contains("DataService"),
        "expected DataService class in output. output: {output}"
    );
    assert!(
        output.contains("fetchWithCache"),
        "expected fetchWithCache method in output. output: {output}"
    );
    assert!(
        output.contains("batchFetch"),
        "expected batchFetch method in output. output: {output}"
    );
    assert!(
        output.contains("initApp"),
        "expected initApp variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async/await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Generator Transform ES5 Source Map Tests
// =============================================================================
// Tests for generator function compilation with ES5 target focusing on:
// yield expressions, generator delegation (yield*).

#[test]
fn test_source_map_generator_es5_basic_yield() {
    // Test basic yield expressions
    let source = r#"function* simpleGenerator() {
    yield 1;
    yield 2;
    yield 3;
}

const gen = simpleGenerator();
console.log(gen.next().value);
console.log(gen.next().value);
console.log(gen.next().value);"#;

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
        output.contains("simpleGenerator"),
        "expected simpleGenerator function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic yield"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_yield_with_values() {
    // Test yield with computed values
    let source = r#"function* valueGenerator(start: number) {
    let current = start;
    while (current < start + 5) {
        yield current * 2;
        current++;
    }
}

function* expressionYield() {
    const a = 10;
    const b = 20;
    yield a + b;
    yield a * b;
    yield Math.max(a, b);
}

const gen1 = valueGenerator(1);
const gen2 = expressionYield();

for (const val of gen1) {
    console.log(val);
}

console.log(gen2.next().value);"#;

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
        output.contains("valueGenerator") && output.contains("expressionYield"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for yield with values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_delegation() {
    // Test generator delegation (yield*)
    let source = r#"function* innerGenerator() {
    yield "a";
    yield "b";
    yield "c";
}

function* anotherInner() {
    yield 1;
    yield 2;
}

function* outerGenerator() {
    yield "start";
    yield* innerGenerator();
    yield "middle";
    yield* anotherInner();
    yield "end";
}

const gen = outerGenerator();
for (const value of gen) {
    console.log(value);
}"#;

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
        output.contains("innerGenerator") && output.contains("outerGenerator"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator delegation"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_return_value() {
    // Test generator with return values
    let source = r#"function* generatorWithReturn(): Generator<number, string, unknown> {
    yield 1;
    yield 2;
    yield 3;
    return "done";
}

function* conditionalReturn(shouldStop: boolean): Generator<number, string, unknown> {
    yield 1;
    if (shouldStop) {
        return "stopped early";
    }
    yield 2;
    yield 3;
    return "completed";
}

const gen1 = generatorWithReturn();
let result = gen1.next();
while (!result.done) {
    console.log("Value:", result.value);
    result = gen1.next();
}
console.log("Return:", result.value);

const gen2 = conditionalReturn(true);"#;

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
        output.contains("generatorWithReturn") && output.contains("conditionalReturn"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_try_catch() {
    // Test generator with try/catch
    let source = r#"function* safeGenerator(): Generator<number, void, unknown> {
    try {
        yield 1;
        yield 2;
        throw new Error("Generator error");
    } catch (error) {
        console.error("Caught:", error);
        yield -1;
    } finally {
        console.log("Generator cleanup");
    }
}

function* generatorWithFinally(): Generator<string, void, unknown> {
    try {
        yield "start";
        yield "middle";
    } finally {
        yield "cleanup";
    }
}

const gen1 = safeGenerator();
for (const val of gen1) {
    console.log(val);
}

const gen2 = generatorWithFinally();
console.log(gen2.next().value);"#;

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
        output.contains("safeGenerator") && output.contains("generatorWithFinally"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator try/catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_infinite() {
    // Test infinite generator patterns
    let source = r#"function* infiniteCounter(): Generator<number, never, unknown> {
    let count = 0;
    while (true) {
        yield count++;
    }
}

function* fibonacci(): Generator<number, never, unknown> {
    let a = 0;
    let b = 1;
    while (true) {
        yield a;
        const temp = a;
        a = b;
        b = temp + b;
    }
}

function* idGenerator(prefix: string): Generator<string, never, unknown> {
    let id = 0;
    while (true) {
        yield prefix + "-" + (id++);
    }
}

const counter = infiniteCounter();
console.log(counter.next().value);
console.log(counter.next().value);

const fib = fibonacci();
for (let i = 0; i < 10; i++) {
    console.log(fib.next().value);
}

const ids = idGenerator("user");
console.log(ids.next().value);"#;

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
        output.contains("infiniteCounter") && output.contains("fibonacci"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        output.contains("idGenerator"),
        "expected idGenerator function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for infinite generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_class_iterator() {
    // Test generator implementing iterator protocol
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Generator<number, void, unknown> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

class Tree<T> {
    constructor(
        public value: T,
        public left?: Tree<T>,
        public right?: Tree<T>
    ) {}

    *inOrder(): Generator<T, void, unknown> {
        if (this.left) {
            yield* this.left.inOrder();
        }
        yield this.value;
        if (this.right) {
            yield* this.right.inOrder();
        }
    }
}

const range = new Range(1, 5);
for (const num of range) {
    console.log(num);
}

const tree = new Tree(2, new Tree(1), new Tree(3));
for (const val of tree.inOrder()) {
    console.log(val);
}"#;

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
        output.contains("Range") && output.contains("Tree"),
        "expected Range and Tree classes in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for iterator protocol"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_class_methods() {
    // Test generator methods in classes
    let source = r#"class DataProcessor {
    private data: number[] = [];

    constructor(data: number[]) {
        this.data = data;
    }

    *processAll(): Generator<number, void, unknown> {
        for (const item of this.data) {
            yield this.process(item);
        }
    }

    *processFiltered(predicate: (n: number) => boolean): Generator<number, void, unknown> {
        for (const item of this.data) {
            if (predicate(item)) {
                yield this.process(item);
            }
        }
    }

    private process(item: number): number {
        return item * 2;
    }

    static *range(start: number, end: number): Generator<number, void, unknown> {
        for (let i = start; i <= end; i++) {
            yield i;
        }
    }
}

const processor = new DataProcessor([1, 2, 3, 4, 5]);
for (const result of processor.processAll()) {
    console.log(result);
}

for (const result of processor.processFiltered(function(n) { return n > 2; })) {
    console.log(result);
}

for (const n of DataProcessor.range(1, 10)) {
    console.log(n);
}"#;

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
        output.contains("DataProcessor"),
        "expected DataProcessor class in output. output: {output}"
    );
    assert!(
        output.contains("processAll") && output.contains("processFiltered"),
        "expected generator methods in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class generator methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

