/// Test async generator with await expressions and ES5 target
#[test]
fn test_source_map_async_generator_es5_await_expressions() {
    let source = r#"async function fetchData(id: number): Promise<string> {
    return `data-${id}`;
}

async function* fetchSequence(ids: number[]) {
    for (const id of ids) {
        const data = await fetchData(id);
        yield data;
    }
}

async function process() {
    const ids = [1, 2, 3];
    for await (const data of fetchSequence(ids)) {
        console.log(data);
    }
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
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        output.contains("fetchSequence"),
        "expected fetchSequence in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for await expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with for-await-of loops and ES5 target
#[test]
fn test_source_map_async_generator_es5_for_await_of() {
    let source = r#"async function* createStream(): AsyncGenerator<number> {
    yield 1;
    yield 2;
    yield 3;
}

async function processStream() {
    const stream = createStream();
    let total = 0;

    for await (const value of stream) {
        total += value;
    }

    return total;
}

async function nestedForAwait() {
    const streams = [createStream(), createStream()];
    for (const stream of streams) {
        for await (const value of stream) {
            console.log(value);
        }
    }
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
        output.contains("createStream"),
        "expected createStream in output. output: {output}"
    );
    assert!(
        output.contains("processStream"),
        "expected processStream in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-await-of"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with error handling and ES5 target
#[test]
fn test_source_map_async_generator_es5_error_handling() {
    let source = r#"async function* riskyGenerator() {
    try {
        yield 1;
        throw new Error("oops");
        yield 2;
    } catch (e) {
        yield "caught";
    } finally {
        yield "cleanup";
    }
}

async function handleErrors() {
    try {
        for await (const value of riskyGenerator()) {
            console.log(value);
        }
    } catch (e) {
        console.error(e);
    }
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
        output.contains("riskyGenerator"),
        "expected riskyGenerator in output. output: {output}"
    );
    assert!(
        output.contains("handleErrors"),
        "expected handleErrors in output. output: {output}"
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

/// Test async generator as class methods with ES5 target
#[test]
fn test_source_map_async_generator_es5_class_methods() {
    let source = r#"class DataSource {
    private items: string[] = ["a", "b", "c"];

    async *iterate() {
        for (const item of this.items) {
            yield item;
        }
    }

    async *transform(fn: (s: string) => string) {
        for (const item of this.items) {
            yield fn(item);
        }
    }
}

class Pipeline {
    async *chain(sources: DataSource[]) {
        for (const source of sources) {
            yield* source.iterate();
        }
    }
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
        output.contains("DataSource"),
        "expected DataSource in output. output: {output}"
    );
    assert!(
        output.contains("Pipeline"),
        "expected Pipeline in output. output: {output}"
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

/// Test async generator with multiple yields and awaits interleaved with ES5 target
#[test]
fn test_source_map_async_generator_es5_interleaved() {
    let source = r#"async function delay(ms: number): Promise<void> {
    return new Promise(r => setTimeout(r, ms));
}

async function* interleaved() {
    yield "starting";
    await delay(100);
    yield "step 1";
    await delay(100);
    yield "step 2";
    await delay(100);
    yield "step 3";
    await delay(100);
    yield "done";
}

async function run() {
    const results: string[] = [];
    for await (const step of interleaved()) {
        results.push(step);
    }
    return results;
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
        output.contains("delay"),
        "expected delay in output. output: {output}"
    );
    assert!(
        output.contains("interleaved"),
        "expected interleaved in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interleaved"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with return values and ES5 target
#[test]
fn test_source_map_async_generator_es5_return_values() {
    let source = r#"async function* withReturn(): AsyncGenerator<number, string, void> {
    yield 1;
    yield 2;
    return "completed";
}

async function* earlyReturn(condition: boolean) {
    yield "start";
    if (condition) {
        return "early exit";
    }
    yield "middle";
    yield "end";
    return "normal exit";
}

async function collectResults() {
    const gen = withReturn();
    let result: IteratorResult<number, string>;
    while (!(result = await gen.next()).done) {
        console.log(result.value);
    }
    console.log("Return value:", result.value);
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
        output.contains("withReturn"),
        "expected withReturn in output. output: {output}"
    );
    assert!(
        output.contains("earlyReturn"),
        "expected earlyReturn in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nested async generators with ES5 target
#[test]
fn test_source_map_async_generator_es5_nested() {
    let source = r#"async function* outerAsync() {
    async function* innerAsync() {
        yield "inner 1";
        yield "inner 2";
    }

    yield "outer start";
    yield* innerAsync();
    yield "outer end";
}

async function* recursiveGen(depth: number): AsyncGenerator<string> {
    yield `depth ${depth}`;
    if (depth > 0) {
        yield* recursiveGen(depth - 1);
    }
}

async function consume() {
    for await (const value of outerAsync()) {
        console.log(value);
    }
    for await (const value of recursiveGen(3)) {
        console.log(value);
    }
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
        output.contains("outerAsync"),
        "expected outerAsync in output. output: {output}"
    );
    assert!(
        output.contains("recursiveGen"),
        "expected recursiveGen in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested generators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test for async generator patterns with ES5 target
#[test]
fn test_source_map_async_generator_es5_comprehensive() {
    let source = r#"// Utility types and interfaces
interface AsyncIterable<T> {
    [Symbol.asyncIterator](): AsyncIterator<T>;
}

// Async event emitter
class AsyncEventEmitter {
    private events: Map<string, Function[]> = new Map();

    on(event: string, handler: Function) {
        if (!this.events.has(event)) {
            this.events.set(event, []);
        }
        this.events.get(event)!.push(handler);
    }

    async *subscribe(event: string): AsyncGenerator<any> {
        const queue: any[] = [];
        let resolve: ((value: any) => void) | null = null;

        this.on(event, (data: any) => {
            if (resolve) {
                resolve(data);
                resolve = null;
            } else {
                queue.push(data);
            }
        });

        while (true) {
            if (queue.length > 0) {
                yield queue.shift();
            } else {
                yield await new Promise(r => { resolve = r; });
            }
        }
    }
}

// Data processing pipeline
class DataPipeline<T> {
    constructor(private source: AsyncGenerator<T>) {}

    async *map<U>(fn: (item: T) => U | Promise<U>): AsyncGenerator<U> {
        for await (const item of this.source) {
            yield await fn(item);
        }
    }

    async *filter(predicate: (item: T) => boolean | Promise<boolean>): AsyncGenerator<T> {
        for await (const item of this.source) {
            if (await predicate(item)) {
                yield item;
            }
        }
    }

    async *take(count: number): AsyncGenerator<T> {
        let taken = 0;
        for await (const item of this.source) {
            if (taken >= count) break;
            yield item;
            taken++;
        }
    }

    async *batch(size: number): AsyncGenerator<T[]> {
        let batch: T[] = [];
        for await (const item of this.source) {
            batch.push(item);
            if (batch.length >= size) {
                yield batch;
                batch = [];
            }
        }
        if (batch.length > 0) {
            yield batch;
        }
    }
}

// Async iterator utilities
async function* merge<T>(...generators: AsyncGenerator<T>[]): AsyncGenerator<T> {
    const pending = generators.map(async (gen, i) => {
        const result = await gen.next();
        return { index: i, result };
    });

    while (pending.length > 0) {
        const { index, result } = await Promise.race(pending);
        if (!result.done) {
            yield result.value;
            pending[index] = (async () => {
                const res = await generators[index].next();
                return { index, result: res };
            })();
        }
    }
}

async function* range(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i < end; i++) {
        yield i;
    }
}

async function* fromPromises<T>(promises: Promise<T>[]): AsyncGenerator<T> {
    for (const promise of promises) {
        yield await promise;
    }
}

// Usage example
async function main() {
    const numbers = range(0, 100);
    const pipeline = new DataPipeline(numbers);

    const processed = pipeline
        .filter(n => n % 2 === 0)
        .map(n => n * 2)
        .take(10);

    for await (const num of processed) {
        console.log(num);
    }

    const emitter = new AsyncEventEmitter();
    const subscription = emitter.subscribe("data");

    setTimeout(() => {
        for (let i = 0; i < 5; i++) {
            emitter.on("data", () => i);
        }
    }, 100);
}

main().catch(console.error);"#;

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
        output.contains("AsyncEventEmitter"),
        "expected AsyncEventEmitter in output. output: {output}"
    );
    assert!(
        output.contains("DataPipeline"),
        "expected DataPipeline in output. output: {output}"
    );
    assert!(
        output.contains("merge"),
        "expected merge in output. output: {output}"
    );
    assert!(
        output.contains("range"),
        "expected range in output. output: {output}"
    );
    assert!(
        output.contains("fromPromises"),
        "expected fromPromises in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async generators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - OPTIONAL CHAINING PATTERNS
// =============================================================================
// Tests for optional chaining patterns with ES5 target to verify source maps
// work correctly with optional chaining transforms.

/// Test basic optional chaining property access with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_property_access() {
    let source = r#"interface User {
    name?: string;
    address?: {
        city?: string;
        zip?: string;
    };
}

function getUserCity(user: User | null) {
    return user?.address?.city;
}

const user: User = { name: "Alice" };
const city = user?.address?.city;
const name = user?.name;
console.log(city, name);"#;

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
        output.contains("getUserCity"),
        "expected getUserCity in output. output: {output}"
    );
    assert!(
        output.contains("user"),
        "expected user in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining method call with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_method_call() {
    let source = r#"interface Service {
    getData?(): string;
    process?(data: string): void;
}

function callService(service: Service | undefined) {
    const data = service?.getData?.();
    service?.process?.(data ?? "default");
    return data;
}

class Api {
    client?: {
        fetch?(url: string): Promise<any>;
    };

    async request(url: string) {
        const result = await this.client?.fetch?.(url);
        return result;
    }
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
        output.contains("callService"),
        "expected callService in output. output: {output}"
    );
    assert!(
        output.contains("Api"),
        "expected Api in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

