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

#[test]
fn test_source_map_generator_es5_async_generator() {
    // Test async generator patterns
    let source = r#"async function* asyncDataFetcher(urls: string[]): AsyncGenerator<any, void, unknown> {
    for (const url of urls) {
        const data = await fetch(url).then(function(r) { return r.json(); });
        yield data;
    }
}

async function* timedGenerator(interval: number): AsyncGenerator<number, void, unknown> {
    let count = 0;
    while (count < 5) {
        await new Promise(function(resolve) { setTimeout(resolve, interval); });
        yield count++;
    }
}

async function* paginatedFetch(baseUrl: string): AsyncGenerator<any[], void, unknown> {
    let page = 1;
    let hasMore = true;

    while (hasMore) {
        const response = await fetch(baseUrl + "?page=" + page);
        const data = await response.json();
        yield data.items;
        hasMore = data.hasMore;
        page++;
    }
}

async function processAsync() {
    const fetcher = asyncDataFetcher(["url1", "url2"]);
    for await (const data of fetcher) {
        console.log(data);
    }
}

processAsync();"#;

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
        output.contains("asyncDataFetcher") && output.contains("timedGenerator"),
        "expected async generator functions in output. output: {output}"
    );
    assert!(
        output.contains("paginatedFetch"),
        "expected paginatedFetch function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async generators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_transform_comprehensive() {
    // Comprehensive generator transform test
    let source = r#"// Utility generators
function* take<T>(iterable: Iterable<T>, count: number): Generator<T, void, unknown> {
    let i = 0;
    for (const item of iterable) {
        if (i >= count) return;
        yield item;
        i++;
    }
}

function* map<T, U>(iterable: Iterable<T>, fn: (item: T) => U): Generator<U, void, unknown> {
    for (const item of iterable) {
        yield fn(item);
    }
}

function* filter<T>(iterable: Iterable<T>, predicate: (item: T) => boolean): Generator<T, void, unknown> {
    for (const item of iterable) {
        if (predicate(item)) {
            yield item;
        }
    }
}

// Complex generator with delegation
function* pipeline<T>(
    source: Iterable<T>,
    ...transforms: ((items: Iterable<T>) => Iterable<T>)[]
): Generator<T, void, unknown> {
    let current: Iterable<T> = source;
    for (const transform of transforms) {
        current = transform(current);
    }
    yield* current as Generator<T>;
}

// Class with generator methods
class LazyCollection<T> {
    constructor(private items: T[]) {}

    *[Symbol.iterator](): Generator<T, void, unknown> {
        yield* this.items;
    }

    *map<U>(fn: (item: T) => U): Generator<U, void, unknown> {
        for (const item of this.items) {
            yield fn(item);
        }
    }

    *filter(predicate: (item: T) => boolean): Generator<T, void, unknown> {
        for (const item of this.items) {
            if (predicate(item)) {
                yield item;
            }
        }
    }

    *flatMap<U>(fn: (item: T) => Iterable<U>): Generator<U, void, unknown> {
        for (const item of this.items) {
            yield* fn(item);
        }
    }
}

// Usage
const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

// Using take
for (const n of take(numbers, 3)) {
    console.log("Take:", n);
}

// Using map
for (const n of map(numbers, function(x) { return x * 2; })) {
    console.log("Map:", n);
}

// Using filter
for (const n of filter(numbers, function(x) { return x % 2 === 0; })) {
    console.log("Filter:", n);
}

// Using LazyCollection
const collection = new LazyCollection(numbers);
for (const n of collection.filter(function(x) { return x > 5; })) {
    console.log("Collection:", n);
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
        output.contains("take"),
        "expected take function in output. output: {output}"
    );
    assert!(
        output.contains("map"),
        "expected map function in output. output: {output}"
    );
    assert!(
        output.contains("filter"),
        "expected filter function in output. output: {output}"
    );
    assert!(
        output.contains("LazyCollection"),
        "expected LazyCollection class in output. output: {output}"
    );
    assert!(
        output.contains("pipeline"),
        "expected pipeline function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Destructuring Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_destructuring_es5_array_basic() {
    let source = r#"const numbers = [1, 2, 3, 4, 5];
const [first, second, third] = numbers;
console.log(first, second, third);

const [a, b, c, d, e] = [10, 20, 30, 40, 50];
console.log(a + b + c + d + e);

let [x, y] = [100, 200];
[x, y] = [y, x];
console.log(x, y);"#;

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
        "expected first variable in output. output: {output}"
    );
    assert!(
        output.contains("second"),
        "expected second variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_object_basic() {
    let source = r#"const person = { name: "Alice", age: 30, city: "NYC" };
const { name, age, city } = person;
console.log(name, age, city);

const { name: personName, age: personAge } = person;
console.log(personName, personAge);

const config = { host: "localhost", port: 8080 };
const { host, port } = config;
console.log(host + ":" + port);"#;

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
        output.contains("name"),
        "expected name property in output. output: {output}"
    );
    assert!(
        output.contains("personName"),
        "expected personName alias in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

