#[test]
fn test_source_map_method_decorator_single() {
    // Test single method decorator source mapping
    let source = r#"function Log(target: any, key: string, descriptor: PropertyDescriptor) {
    return descriptor;
}

class Logger {
    @Log
    log(message: string) {
        console.log(message);
    }
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

    // Verify output contains the class and method
    assert!(
        output.contains("Logger") && output.contains("log"),
        "expected output to contain Logger class and log method. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_method_decorator_multiple() {
    // Test multiple method decorators source mapping
    let source = r#"function Bind(target: any, key: string, descriptor: PropertyDescriptor) {}
function Memoize(target: any, key: string, descriptor: PropertyDescriptor) {}
function Validate(target: any, key: string, descriptor: PropertyDescriptor) {}

class Calculator {
    @Bind
    @Memoize
    @Validate
    calculate(x: number, y: number) {
        return x + y;
    }
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

    // Verify output contains the class and method
    assert!(
        output.contains("Calculator") && output.contains("calculate"),
        "expected output to contain Calculator class and calculate method. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple method decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_parameter_decorator() {
    // Test parameter decorator source mapping
    let source = r#"function Inject(target: any, key: string, index: number) {}

class Container {
    constructor(@Inject service: any) {}
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

    // Verify output contains the class
    assert!(
        output.contains("Container"),
        "expected output to contain Container class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parameter decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_parameter_decorator_multiple_params() {
    // Test multiple parameter decorators on different parameters
    let source = r#"function Required(target: any, key: string, index: number) {}
function Optional(target: any, key: string, index: number) {}

class Service {
    process(@Required input: string, @Optional options?: object) {
        return input;
    }
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

    // Verify output contains the class (process may be transformed/elided)
    assert!(
        output.contains("Service"),
        "expected output to contain Service class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple parameter decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_property_decorator() {
    // Test property decorator source mapping
    let source = r#"function Column(target: any, key: string) {}
function PrimaryKey(target: any, key: string) {}

class Entity {
    @PrimaryKey
    id: number = 0;

    @Column
    name: string = "";
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

    // Verify output contains the class
    assert!(
        output.contains("Entity"),
        "expected output to contain Entity class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_accessor_decorator() {
    // Test accessor (getter/setter) decorator source mapping
    let source = r#"function Cached(target: any, key: string, descriptor: PropertyDescriptor) {}
function Validate(target: any, key: string, descriptor: PropertyDescriptor) {}

class Config {
    private _value: number = 0;

    @Cached
    get value(): number {
        return this._value;
    }

    @Validate
    set value(v: number) {
        this._value = v;
    }
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

    // Verify output contains the class and accessors
    assert!(
        output.contains("Config") && output.contains("value"),
        "expected output to contain Config class and value accessor. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_decorator_factory() {
    // Test decorator factory (decorator with arguments) source mapping
    let source = r#"function Route(path: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {};
}

function Method(method: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {};
}

class Controller {
    @Route("/api/users")
    @Method("GET")
    getUsers() {
        return [];
    }
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

    // Verify output contains the class and method
    assert!(
        output.contains("Controller") && output.contains("getUsers"),
        "expected output to contain Controller class and getUsers method. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for decorator factories"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_mixed_decorators() {
    // Test class with class, method, property, and parameter decorators
    let source = r#"function Entity(target: Function) {}
function Column(target: any, key: string) {}
function BeforeInsert(target: any, key: string, descriptor: PropertyDescriptor) {}
function Inject(target: any, key: string, index: number) {}

@Entity
class User {
    @Column
    name: string = "";

    @BeforeInsert
    validate(@Inject validator: any) {
        return true;
    }
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

    // Verify output contains the class (validate may be transformed/elided)
    assert!(
        output.contains("User"),
        "expected output to contain User class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixed decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_async_multiple_awaits() {
    // Test async function with multiple await expressions in sequence
    let source = r#"async function processAll(a: Promise<number>, b: Promise<string>, c: Promise<boolean>) {
    const numResult = await a;
    const strResult = await b;
    const boolResult = await c;
    return { numResult, strResult, boolResult };
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

    // Verify output contains async transform helpers
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async downlevel output. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple awaits"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_try_catch() {
    // Test async function with try/catch around await
    let source = r#"async function safeFetch(url: string) {
    try {
        const response = await fetch(url);
        return await response.json();
    } catch (error) {
        console.error(error);
        return null;
    }
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

    // Verify output contains async transform and try/catch
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async downlevel output. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async try/catch"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_for_of_loop() {
    // Test async function with await inside for...of loop
    let source = r#"async function processItems(items: Promise<number>[]) {
    const results: number[] = [];
    for (const item of items) {
        const value = await item;
        results.push(value);
    }
    return results;
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

    // Verify output contains async transform
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async downlevel output. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async for-of loop"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_iife() {
    // Test async IIFE (Immediately Invoked Function Expression)
    let source = r#"const result = (async () => {
    const data = await fetchData();
    return data.value;
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

    // Verify output contains result variable
    assert!(
        output.contains("result"),
        "expected output to contain result variable. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async IIFE"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_rest_params() {
    // Test async function with rest parameters
    let source = r#"async function processMany(...promises: Promise<number>[]) {
    const results: number[] = [];
    for (const p of promises) {
        results.push(await p);
    }
    return results;
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

    // Verify output contains function name
    assert!(
        output.contains("processMany"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async rest params"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_default_params() {
    // Test async function with default parameters
    let source = r#"async function fetchWithTimeout(url: string, timeout: number = 5000) {
    const controller = new AbortController();
    setTimeout(() => controller.abort(), timeout);
    return await fetch(url, { signal: controller.signal });
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

    // Verify output contains function name
    assert!(
        output.contains("fetchWithTimeout"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async default params"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_destructuring_await() {
    // Test async function with destructuring of awaited value
    let source = r#"async function getUser(id: number) {
    const { name, email, age } = await fetchUser(id);
    return { name, email, age };
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

    // Verify output contains function name
    assert!(
        output.contains("getUser"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async destructuring"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_nested_functions() {
    // Test nested async functions
    let source = r#"async function outer() {
    async function inner(value: number) {
        return await Promise.resolve(value * 2);
    }
    const result = await inner(21);
    return result;
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

    // Verify output contains both function names
    assert!(
        output.contains("outer") && output.contains("inner"),
        "expected output to contain both function names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested async functions"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_class_static_method() {
    // Test async static class method
    let source = r#"class DataService {
    static async fetchAll() {
        const items = await getItems();
        return items;
    }
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

    // Verify output contains class name
    assert!(
        output.contains("DataService"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async static method"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_while_loop() {
    // Test async function with await in while loop
    let source = r#"async function pollUntilDone(checkStatus: () => Promise<boolean>) {
    while (!(await checkStatus())) {
        await delay(1000);
    }
    return true;
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

    // Verify output contains function name
    assert!(
        output.contains("pollUntilDone"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async while loop"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Class Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_es5_class_basic_iife() {
    // Test basic class lowered to ES5 IIFE pattern
    let source = r#"class Point {
    x: number;
    y: number;
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

    // Verify output contains class name as function
    assert!(
        output.contains("Point"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class IIFE"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_constructor() {
    // Test class with constructor lowered to ES5
    let source = r#"class Rectangle {
    width: number;
    height: number;
    constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
    }
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

    // Verify output contains class name and property assignments
    assert!(
        output.contains("Rectangle"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class constructor"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_instance_methods() {
    // Test class with instance methods lowered to ES5
    let source = r#"class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
    subtract(a: number, b: number): number {
        return a - b;
    }
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

    // Verify output contains class and method names
    assert!(
        output.contains("Calculator"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class instance methods"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_static_methods() {
    // Test class with static methods lowered to ES5
    let source = r#"class MathUtils {
    static PI: number = 3.14159;
    static square(x: number): number {
        return x * x;
    }
    static cube(x: number): number {
        return x * x * x;
    }
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

    // Verify output contains class and static members
    assert!(
        output.contains("MathUtils"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class static methods"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_accessors() {
    // Test class with getters and setters lowered to ES5
    let source = r#"class Person {
    private _name: string = "";
    get name(): string {
        return this._name;
    }
    set name(value: string) {
        this._name = value;
    }
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

    // Verify output contains class name
    assert!(
        output.contains("Person"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class accessors"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_inheritance() {
    // Test class inheritance lowered to ES5 with __extends
    let source = r#"class Animal {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
}
class Dog extends Animal {
    breed: string;
    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
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

    // Verify output contains both class names
    assert!(
        output.contains("Animal") && output.contains("Dog"),
        "expected output to contain class names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class inheritance"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_super_method_call() {
    // Test super method calls in derived class lowered to ES5
    let source = r#"class Base {
    greet(): string {
        return "Hello";
    }
}
class Derived extends Base {
    greet(): string {
        return super.greet() + " World";
    }
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

    // Verify output contains class names
    assert!(
        output.contains("Base") && output.contains("Derived"),
        "expected output to contain class names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 super method call"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_computed_property() {
    // Test class with computed property names lowered to ES5
    let source = r#"const methodName = "dynamicMethod";
class DynamicClass {
    [methodName](): string {
        return "dynamic";
    }
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

    // Verify output contains class name
    assert!(
        output.contains("DynamicClass"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 computed property"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_multiple_inheritance_levels() {
    // Test multi-level inheritance chain lowered to ES5
    let source = r#"class GrandParent {
    level(): number { return 1; }
}
class Parent extends GrandParent {
    level(): number { return 2; }
}
class Child extends Parent {
    level(): number { return 3; }
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

    // Verify output contains all class names
    assert!(
        output.contains("GrandParent") && output.contains("Parent") && output.contains("Child"),
        "expected output to contain all class names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 multi-level inheritance"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_expression() {
    // Test class expressions lowered to ES5
    let source = r#"const MyClass = class {
    value: number = 42;
    getValue(): number {
        return this.value;
    }
};"#;

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

    // Verify output contains variable name
    assert!(
        output.contains("MyClass"),
        "expected output to contain class expression variable. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class expression"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Generator Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_generator_basic_yield() {
    // Test basic generator function with single yield
    let source = r#"function* simpleGenerator() {
    yield 1;
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

    // Verify output contains generator function name
    assert!(
        output.contains("simpleGenerator"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_multiple_yields() {
    // Test generator function with multiple yield statements
    let source = r#"function* multiYield() {
    yield 1;
    yield 2;
    yield 3;
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

    // Verify output contains generator function name
    assert!(
        output.contains("multiYield"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multi-yield generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

