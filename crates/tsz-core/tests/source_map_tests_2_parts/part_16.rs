#[test]
fn test_source_map_arrow_class_property() {
    // Test arrow function as class property
    let source = r#"class Counter {
    private count: number = 0;

    increment = () => {
        this.count++;
        return this.count;
    };

    decrement = () => this.count--;

    reset = () => {
        this.count = 0;
    };

    getCount = () => this.count;
}

const counter = new Counter();
counter.increment();"#;

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
        output.contains("Counter") || output.contains("function"),
        "expected output to contain class name or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow as class property"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_iife() {
    // Test arrow function in immediately invoked expression
    let source = r#"const result1 = (() => 42)();
const result2 = ((x: number) => x * 2)(21);
const result3 = ((a: number, b: number) => a + b)(10, 20);

const module = (() => {
    const privateValue = 100;
    return {
        getValue: () => privateValue,
        double: () => privateValue * 2
    };
})();

console.log(result1, result2, result3, module.getValue());"#;

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
        output.contains("result1") || output.contains("function"),
        "expected output to contain identifier or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow IIFE"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_object_return() {
    // Test arrow function returning object literal
    let source = r#"const createPoint = (x: number, y: number) => ({ x, y });
const createRect = (w: number, h: number) => ({ width: w, height: h, area: w * h });
const wrap = (value: any) => ({ value });
const createPerson = (name: string, age: number) => ({
    name,
    age,
    greet: () => "Hello, " + name
});

const point = createPoint(10, 20);
const rect = createRect(100, 50);
const person = createPerson("Alice", 30);"#;

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
        output.contains("createPoint") || output.contains("function"),
        "expected output to contain identifier or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow returning object"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_higher_order() {
    // Test higher-order arrow functions
    let source = r#"const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));
const pipe = <A, B, C>(f: (a: A) => B, g: (b: B) => C) => (x: A) => g(f(x));
const curry = (fn: (a: number, b: number) => number) => (a: number) => (b: number) => fn(a, b);
const partial = <T, U, R>(fn: (a: T, b: U) => R, a: T) => (b: U) => fn(a, b);

const add = (a: number, b: number) => a + b;
const curriedAdd = curry(add);
const add5 = curriedAdd(5);
const result = add5(10);"#;

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
        output.contains("compose") || output.contains("curry") || output.contains("function"),
        "expected output to contain identifier or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for higher-order arrows"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_combined() {
    // Test combined arrow function patterns
    let source = r#"class EventEmitter {
    private handlers: Map<string, ((data: any) => void)[]> = new Map();

    on = (event: string, handler: (data: any) => void) => {
        const handlers = this.handlers.get(event) || [];
        handlers.push(handler);
        this.handlers.set(event, handlers);
        return () => {
            const idx = handlers.indexOf(handler);
            if (idx > -1) handlers.splice(idx, 1);
        };
    };

    emit = (event: string, data: any) => {
        (this.handlers.get(event) || []).forEach(h => h(data));
    };
}

const emitter = new EventEmitter();
const unsubscribe = emitter.on("test", data => console.log(data));
emitter.emit("test", { message: "hello" });
unsubscribe();

const pipeline = [
    (x: number) => x + 1,
    (x: number) => x * 2,
    (x: number) => x - 3
].reduce((acc, fn) => (x: number) => fn(acc(x)), (x: number) => x);

console.log(pipeline(5));"#;

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
        output.contains("EventEmitter") || output.contains("emitter"),
        "expected output to contain class or variable name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined arrow patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Async Generator ES5 Source Map Tests
// ============================================================================

#[test]
fn test_source_map_async_generator_basic() {
    // Test basic async generator function
    let source = r#"async function* basicAsyncGen() {
    yield 1;
    yield 2;
    yield 3;
}

const gen = basicAsyncGen();
gen.next().then(console.log);"#;

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
        output.contains("basicAsyncGen") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic async generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_generator_with_await() {
    // Test async generator with await expressions
    let source = r#"async function* fetchSequence(urls: string[]) {
    for (const url of urls) {
        const response = await fetch(url);
        const data = await response.json();
        yield data;
    }
}

const urls = ["url1", "url2"];
const gen = fetchSequence(urls);"#;

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
        output.contains("fetchSequence") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async generator with await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_generator_yield_await() {
    // Test async generator with yield* and await
    let source = r#"async function* innerGen() {
    yield 1;
    yield 2;
}

async function* outerGen() {
    yield* innerGen();
    await Promise.resolve();
    yield 3;
}

const gen = outerGen();"#;

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
        output.contains("innerGen") || output.contains("outerGen") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async generator yield await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_generator_try_catch() {
    // Test async generator with try-catch
    let source = r#"async function* safeGenerator() {
    try {
        yield await Promise.resolve(1);
        yield await Promise.resolve(2);
        throw new Error("test error");
    } catch (e) {
        yield "error caught";
    } finally {
        yield "cleanup";
    }
}

const gen = safeGenerator();"#;

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
        output.contains("safeGenerator") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async generator try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_generator_class_method() {
    // Test async generator as class method
    let source = r#"class DataStream {
    private data: number[] = [1, 2, 3, 4, 5];

    async *iterate() {
        for (const item of this.data) {
            await new Promise(r => setTimeout(r, 100));
            yield item;
        }
    }

    async *filter(predicate: (n: number) => boolean) {
        for await (const item of this.iterate()) {
            if (predicate(item)) {
                yield item;
            }
        }
    }
}

const stream = new DataStream();
const gen = stream.iterate();"#;

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
        output.contains("DataStream") || output.contains("function"),
        "expected output to contain class name. output: {output}"
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

