/// Test generator with multiple yields and complex expressions
#[test]
fn test_source_map_generator_transform_es5_multiple_yields() {
    let source = r#"function* dataProcessor(items: string[]): Generator<{ index: number; value: string; processed: boolean }, number, unknown> {
    let processedCount = 0;

    for (let i = 0; i < items.length; i++) {
        const item = items[i];

        // Yield before processing
        yield { index: i, value: item, processed: false };

        // Simulate processing
        const processed = item.toUpperCase();
        processedCount++;

        // Yield after processing
        yield { index: i, value: processed, processed: true };
    }

    // Final yield with count
    yield { index: -1, value: `Total: ${processedCount}`, processed: true };

    return processedCount;
}

const processor = dataProcessor(["hello", "world", "test"]);
let result = processor.next();
while (!result.done) {
    console.log(result.value);
    result = processor.next();
}
console.log("Final count:", result.value);"#;

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
        output.contains("dataProcessor"),
        "expected dataProcessor in output. output: {output}"
    );
    assert!(
        output.contains("processedCount"),
        "expected processedCount in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple yields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test generator delegation with yield*
#[test]
fn test_source_map_generator_transform_es5_delegation() {
    let source = r#"function* innerGenerator(prefix: string): Generator<string, void, unknown> {
    yield `${prefix}-1`;
    yield `${prefix}-2`;
    yield `${prefix}-3`;
}

function* middleGenerator(): Generator<string, void, unknown> {
    yield "start";
    yield* innerGenerator("middle");
    yield "end";
}

function* outerGenerator(): Generator<string, void, unknown> {
    yield "outer-start";
    yield* middleGenerator();
    yield* innerGenerator("outer");
    yield "outer-end";
}

// Test chained delegation
function* chainedDelegation(): Generator<number, void, unknown> {
    const arrays = [[1, 2], [3, 4], [5, 6]];
    for (const arr of arrays) {
        yield* arr;
    }
}

const outer = outerGenerator();
for (const value of outer) {
    console.log(value);
}

const chained = chainedDelegation();
console.log([...chained]);"#;

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
        output.contains("innerGenerator"),
        "expected innerGenerator in output. output: {output}"
    );
    assert!(
        output.contains("outerGenerator"),
        "expected outerGenerator in output. output: {output}"
    );
    assert!(
        output.contains("chainedDelegation"),
        "expected chainedDelegation in output. output: {output}"
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

/// Test generator in class method with instance access
#[test]
fn test_source_map_generator_transform_es5_class_method() {
    let source = r#"class DataIterator {
    private data: number[];
    private name: string;

    constructor(name: string, data: number[]) {
        this.name = name;
        this.data = data;
    }

    *iterate(): Generator<number, void, unknown> {
        console.log(`Starting iteration for ${this.name}`);
        for (const item of this.data) {
            yield item;
        }
        console.log(`Finished iteration for ${this.name}`);
    }

    *iterateWithIndex(): Generator<[number, number], void, unknown> {
        for (let i = 0; i < this.data.length; i++) {
            yield [i, this.data[i]];
        }
    }

    *filter(predicate: (n: number) => boolean): Generator<number, void, unknown> {
        for (const item of this.data) {
            if (predicate(item)) {
                yield item;
            }
        }
    }

    static *range(start: number, end: number): Generator<number, void, unknown> {
        for (let i = start; i <= end; i++) {
            yield i;
        }
    }
}

const iterator = new DataIterator("test", [1, 2, 3, 4, 5]);
for (const num of iterator.iterate()) {
    console.log("Value:", num);
}

for (const [idx, val] of iterator.iterateWithIndex()) {
    console.log(`Index ${idx}: ${val}`);
}

for (const even of iterator.filter(n => n % 2 === 0)) {
    console.log("Even:", even);
}

for (const n of DataIterator.range(10, 15)) {
    console.log("Range:", n);
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
        output.contains("DataIterator"),
        "expected DataIterator in output. output: {output}"
    );
    assert!(
        output.contains("iterate"),
        "expected iterate method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test generator with try/finally for cleanup
#[test]
fn test_source_map_generator_transform_es5_try_finally() {
    let source = r#"function* resourceManager(): Generator<string, void, unknown> {
    console.log("Acquiring resource");
    try {
        yield "resource acquired";

        console.log("Using resource");
        yield "resource in use";

        console.log("Still using resource");
        yield "still in use";
    } finally {
        console.log("Releasing resource (cleanup)");
    }
}

function* nestedTryFinally(): Generator<number, void, unknown> {
    try {
        yield 1;
        try {
            yield 2;
            try {
                yield 3;
            } finally {
                console.log("Inner cleanup");
            }
            yield 4;
        } finally {
            console.log("Middle cleanup");
        }
        yield 5;
    } finally {
        console.log("Outer cleanup");
    }
}

function* tryCatchFinally(): Generator<string, void, unknown> {
    try {
        yield "before";
        throw new Error("test error");
    } catch (e) {
        yield `caught: ${(e as Error).message}`;
    } finally {
        yield "finally block";
    }
}

// Test resource management pattern
const rm = resourceManager();
rm.next();
rm.next();
rm.return(); // Early termination triggers finally

// Test nested cleanup
const nested = nestedTryFinally();
for (const n of nested) {
    console.log("Nested value:", n);
}

// Test full try/catch/finally
const tcf = tryCatchFinally();
for (const s of tcf) {
    console.log("TCF:", s);
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
        output.contains("resourceManager"),
        "expected resourceManager in output. output: {output}"
    );
    assert!(
        output.contains("nestedTryFinally"),
        "expected nestedTryFinally in output. output: {output}"
    );
    assert!(
        output.contains("tryCatchFinally"),
        "expected tryCatchFinally in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator try/finally"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test combined generator source map patterns
#[test]
fn test_source_map_generator_transform_es5_comprehensive() {
    let source = r#"// Comprehensive generator transform test
interface Task {
    id: number;
    name: string;
    status: "pending" | "running" | "completed";
}

class TaskQueue {
    private tasks: Task[] = [];
    private idCounter = 0;

    add(name: string): Task {
        const task: Task = {
            id: this.idCounter++,
            name,
            status: "pending"
        };
        this.tasks.push(task);
        return task;
    }

    *pending(): Generator<Task, void, unknown> {
        for (const task of this.tasks) {
            if (task.status === "pending") {
                yield task;
            }
        }
    }

    *all(): Generator<Task, void, unknown> {
        yield* this.tasks;
    }

    *process(): Generator<Task, number, unknown> {
        let processed = 0;
        for (const task of this.tasks) {
            if (task.status === "pending") {
                task.status = "running";
                yield task;
                task.status = "completed";
                processed++;
            }
        }
        return processed;
    }
}

function* pipeline<T, U>(
    source: Generator<T, void, unknown>,
    transform: (item: T) => U
): Generator<U, void, unknown> {
    for (const item of source) {
        yield transform(item);
    }
}

function* take<T>(source: Generator<T, void, unknown>, count: number): Generator<T, void, unknown> {
    let taken = 0;
    for (const item of source) {
        if (taken >= count) break;
        yield item;
        taken++;
    }
}

function* infiniteCounter(start: number = 0): Generator<number, never, unknown> {
    let count = start;
    while (true) {
        yield count++;
    }
}

// Usage
const queue = new TaskQueue();
queue.add("Task 1");
queue.add("Task 2");
queue.add("Task 3");

// Iterator over pending tasks
for (const task of queue.pending()) {
    console.log("Pending:", task.name);
}

// Pipeline with transform
const taskNames = pipeline(queue.all(), task => task.name.toUpperCase());
for (const name of taskNames) {
    console.log("Name:", name);
}

// Take from infinite sequence
const firstFive = take(infiniteCounter(100), 5);
console.log([...firstFive]);

// Process tasks
const processor = queue.process();
let result = processor.next();
while (!result.done) {
    console.log("Processing:", (result.value as Task).name);
    result = processor.next();
}
console.log("Total processed:", result.value);"#;

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
        output.contains("TaskQueue"),
        "expected TaskQueue in output. output: {output}"
    );
    assert!(
        output.contains("pipeline"),
        "expected pipeline in output. output: {output}"
    );
    assert!(
        output.contains("infiniteCounter"),
        "expected infiniteCounter in output. output: {output}"
    );
    assert!(
        output.contains("pending"),
        "expected pending method in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive generator transform"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: PRIVATE CLASS FEATURES TRANSFORM
// =============================================================================

/// Test source map generation for private field read access in ES5 output.
/// Validates that reading private fields (#field) generates proper source mappings.
#[test]
fn test_source_map_private_field_read_es5() {
    let source = r#"class Counter {
    #count = 0;

    getCount() {
        return this.#count;
    }
}

const counter = new Counter();
console.log(counter.getCount());"#;

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
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        output.contains("getCount"),
        "expected getCount method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field read"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private field write access in ES5 output.
/// Validates that writing to private fields (#field = value) generates proper source mappings.
#[test]
fn test_source_map_private_field_write_es5() {
    let source = r#"class Counter {
    #count = 0;

    increment() {
        this.#count = this.#count + 1;
    }

    reset() {
        this.#count = 0;
    }
}

const counter = new Counter();
counter.increment();
counter.reset();"#;

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
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        output.contains("increment"),
        "expected increment method in output. output: {output}"
    );
    assert!(
        output.contains("reset"),
        "expected reset method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field write"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private method calls in ES5 output.
/// Validates that calling private methods (#`method()`) generates proper source mappings.
#[test]
fn test_source_map_private_method_call_es5() {
    let source = r#"class Calculator {
    #validate(n: number): boolean {
        return n >= 0;
    }

    #compute(a: number, b: number): number {
        return a * b;
    }

    multiply(x: number, y: number): number {
        if (this.#validate(x) && this.#validate(y)) {
            return this.#compute(x, y);
        }
        return 0;
    }
}

const calc = new Calculator();
console.log(calc.multiply(5, 3));"#;

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
        output.contains("Calculator"),
        "expected Calculator class in output. output: {output}"
    );
    assert!(
        output.contains("multiply"),
        "expected multiply method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private method call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private accessors (get/set) in ES5 output.
/// Validates that private getters and setters generate proper source mappings.
#[test]
fn test_source_map_private_accessor_es5() {
    let source = r#"class Temperature {
    #celsius = 0;

    get #fahrenheit(): number {
        return this.#celsius * 9/5 + 32;
    }

    set #fahrenheit(value: number) {
        this.#celsius = (value - 32) * 5/9;
    }

    setFahrenheit(f: number) {
        this.#fahrenheit = f;
    }

    getFahrenheit(): number {
        return this.#fahrenheit;
    }
}

const temp = new Temperature();
temp.setFahrenheit(98.6);
console.log(temp.getFahrenheit());"#;

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
        output.contains("Temperature"),
        "expected Temperature class in output. output: {output}"
    );
    assert!(
        output.contains("setFahrenheit"),
        "expected setFahrenheit method in output. output: {output}"
    );
    assert!(
        output.contains("getFahrenheit"),
        "expected getFahrenheit method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private accessor"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private static members in ES5 output.
/// Validates that static private fields and methods generate proper source mappings.
#[test]
fn test_source_map_private_static_members_es5() {
    let source = r#"class IdGenerator {
    static #nextId = 1;

    static #generateId(): number {
        return IdGenerator.#nextId++;
    }

    static create(): number {
        return IdGenerator.#generateId();
    }

    static reset(): void {
        IdGenerator.#nextId = 1;
    }
}

console.log(IdGenerator.create());
console.log(IdGenerator.create());
IdGenerator.reset();"#;

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
        output.contains("IdGenerator"),
        "expected IdGenerator class in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create static method in output. output: {output}"
    );
    assert!(
        output.contains("reset"),
        "expected reset static method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private static members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple private class feature patterns.
/// Tests private fields, methods, accessors, and static members together.
#[test]
fn test_source_map_private_features_es5_comprehensive() {
    let source = r#"class BankAccount {
    static #accountCount = 0;
    #balance = 0;
    #transactions: string[] = [];

    #log(message: string): void {
        this.#transactions.push(message);
    }

    get #formattedBalance(): string {
        return `$${this.#balance.toFixed(2)}`;
    }

    static #validateAmount(amount: number): boolean {
        return amount > 0 && isFinite(amount);
    }

    constructor(initialBalance: number) {
        if (BankAccount.#validateAmount(initialBalance)) {
            this.#balance = initialBalance;
            this.#log(`Account opened with ${this.#formattedBalance}`);
        }
        BankAccount.#accountCount++;
    }

    deposit(amount: number): boolean {
        if (BankAccount.#validateAmount(amount)) {
            this.#balance += amount;
            this.#log(`Deposited: $${amount}`);
            return true;
        }
        return false;
    }

    withdraw(amount: number): boolean {
        if (BankAccount.#validateAmount(amount) && amount <= this.#balance) {
            this.#balance -= amount;
            this.#log(`Withdrew: $${amount}`);
            return true;
        }
        return false;
    }

    getBalance(): string {
        return this.#formattedBalance;
    }

    getHistory(): string[] {
        return [...this.#transactions];
    }

    static getAccountCount(): number {
        return BankAccount.#accountCount;
    }
}

const account = new BankAccount(100);
account.deposit(50);
account.withdraw(25);
console.log(account.getBalance());
console.log(account.getHistory());
console.log(BankAccount.getAccountCount());"#;

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
        output.contains("BankAccount"),
        "expected BankAccount class in output. output: {output}"
    );
    assert!(
        output.contains("deposit"),
        "expected deposit method in output. output: {output}"
    );
    assert!(
        output.contains("withdraw"),
        "expected withdraw method in output. output: {output}"
    );
    assert!(
        output.contains("getBalance"),
        "expected getBalance method in output. output: {output}"
    );
    assert!(
        output.contains("getHistory"),
        "expected getHistory method in output. output: {output}"
    );
    assert!(
        output.contains("getAccountCount"),
        "expected getAccountCount static method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive private features"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: TYPE PARAMETER CONSTRAINTS
// =============================================================================

/// Test source map generation for generic function with type parameter constraint in ES5 output.
/// Validates that generic functions with extends constraints generate proper source mappings.
#[test]
fn test_source_map_type_constraint_generic_function_es5() {
    let source = r#"interface HasLength {
    length: number;
}

function getLength<T extends HasLength>(item: T): number {
    return item.length;
}

function first<T extends any[]>(arr: T): T[0] {
    return arr[0];
}

const strLen = getLength("hello");
const arrLen = getLength([1, 2, 3]);
const firstItem = first([1, 2, 3]);"#;

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
        output.contains("getLength"),
        "expected getLength function in output. output: {output}"
    );
    assert!(
        output.contains("first"),
        "expected first function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic function with constraint"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for generic class with type parameter constraint in ES5 output.
/// Validates that generic classes with extends constraints generate proper source mappings.
#[test]
fn test_source_map_type_constraint_generic_class_es5() {
    let source = r#"interface Comparable<T> {
    compareTo(other: T): number;
}

class SortedList<T extends Comparable<T>> {
    private items: T[] = [];

    add(item: T): void {
        this.items.push(item);
        this.items.sort((a, b) => a.compareTo(b));
    }

    get(index: number): T {
        return this.items[index];
    }

    getAll(): T[] {
        return [...this.items];
    }
}

class NumberWrapper implements Comparable<NumberWrapper> {
    constructor(public value: number) {}

    compareTo(other: NumberWrapper): number {
        return this.value - other.value;
    }
}

const list = new SortedList<NumberWrapper>();
list.add(new NumberWrapper(5));
list.add(new NumberWrapper(2));"#;

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
        output.contains("SortedList"),
        "expected SortedList class in output. output: {output}"
    );
    assert!(
        output.contains("NumberWrapper"),
        "expected NumberWrapper class in output. output: {output}"
    );
    assert!(
        output.contains("compareTo"),
        "expected compareTo method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic class with constraint"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for generic interface with type parameter constraint in ES5 output.
/// Validates that generic interfaces with extends constraints generate proper source mappings.
#[test]
fn test_source_map_type_constraint_generic_interface_es5() {
    let source = r#"interface Entity {
    id: number;
    createdAt: Date;
}

interface Repository<T extends Entity> {
    findById(id: number): T | undefined;
    findAll(): T[];
    save(entity: T): T;
    delete(id: number): boolean;
}

interface User extends Entity {
    name: string;
    email: string;
}

class UserRepository implements Repository<User> {
    private users: User[] = [];

    findById(id: number): User | undefined {
        return this.users.find(u => u.id === id);
    }

    findAll(): User[] {
        return [...this.users];
    }

    save(user: User): User {
        this.users.push(user);
        return user;
    }

    delete(id: number): boolean {
        const index = this.users.findIndex(u => u.id === id);
        if (index >= 0) {
            this.users.splice(index, 1);
            return true;
        }
        return false;
    }
}

const repo = new UserRepository();"#;

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
        output.contains("UserRepository"),
        "expected UserRepository class in output. output: {output}"
    );
    assert!(
        output.contains("findById"),
        "expected findById method in output. output: {output}"
    );
    assert!(
        output.contains("findAll"),
        "expected findAll method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic interface with constraint"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for multiple type parameters with constraints in ES5 output.
/// Validates that functions with multiple constrained type parameters generate proper source mappings.
#[test]
fn test_source_map_type_constraint_multiple_params_es5() {
    let source = r#"interface Serializable {
    serialize(): string;
}

interface Deserializable<T> {
    deserialize(data: string): T;
}

function transform<
    TInput extends Serializable,
    TOutput,
    TTransformer extends { transform(input: TInput): TOutput }
>(input: TInput, transformer: TTransformer): TOutput {
    return transformer.transform(input);
}

function merge<T extends object, U extends object>(first: T, second: U): T & U {
    return { ...first, ...second };
}

function pick<T extends object, K extends keyof T>(obj: T, keys: K[]): Pick<T, K> {
    const result = {} as Pick<T, K>;
    for (const key of keys) {
        result[key] = obj[key];
    }
    return result;
}

const merged = merge({ a: 1 }, { b: 2 });
const picked = pick({ x: 1, y: 2, z: 3 }, ["x", "z"]);"#;

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
        output.contains("transform"),
        "expected transform function in output. output: {output}"
    );
    assert!(
        output.contains("merge"),
        "expected merge function in output. output: {output}"
    );
    assert!(
        output.contains("pick"),
        "expected pick function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple type parameter constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for constraint extends union type in ES5 output.
/// Validates that type parameters constrained by union types generate proper source mappings.
#[test]
fn test_source_map_type_constraint_union_es5() {
    let source = r#"type Primitive = string | number | boolean;

function formatPrimitive<T extends Primitive>(value: T): string {
    return String(value);
}

type JsonValue = string | number | boolean | null | JsonArray | JsonObject;
interface JsonArray extends Array<JsonValue> {}
interface JsonObject { [key: string]: JsonValue; }

function stringify<T extends JsonValue>(value: T): string {
    return JSON.stringify(value);
}

type EventType = "click" | "hover" | "focus" | "blur";

function addEventListener<T extends EventType>(
    type: T,
    handler: (event: T) => void
): void {
    console.log(`Adding listener for ${type}`);
}

const formatted = formatPrimitive(42);
const json = stringify({ key: "value" });
addEventListener("click", (e) => console.log(e));"#;

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
        output.contains("formatPrimitive"),
        "expected formatPrimitive function in output. output: {output}"
    );
    assert!(
        output.contains("stringify"),
        "expected stringify function in output. output: {output}"
    );
    assert!(
        output.contains("addEventListener"),
        "expected addEventListener function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for union type constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple type parameter constraint patterns.
/// Tests generic functions, classes, interfaces with various constraint types.
#[test]
fn test_source_map_type_constraint_es5_comprehensive() {
    let source = r#"// Base interfaces for constraints
interface Identifiable {
    id: string;
}

interface Timestamped {
    createdAt: Date;
    updatedAt: Date;
}

interface Validatable {
    validate(): boolean;
}

// Generic class with multiple constraints
class DataStore<T extends Identifiable & Timestamped> {
    private data: Map<string, T> = new Map();

    save(item: T): void {
        this.data.set(item.id, item);
    }

    find(id: string): T | undefined {
        return this.data.get(id);
    }

    findRecent(since: Date): T[] {
        return Array.from(this.data.values())
            .filter(item => item.updatedAt > since);
    }
}

// Generic function with constraint referencing another type parameter
function createValidator<
    T extends Validatable,
    TResult extends { valid: boolean; errors: string[] }
>(item: T, resultFactory: () => TResult): TResult {
    const result = resultFactory();
    result.valid = item.validate();
    return result;
}

// Class with constrained method type parameters
class Mapper<TSource extends object> {
    map<TTarget extends object>(
        source: TSource,
        mapper: (s: TSource) => TTarget
    ): TTarget {
        return mapper(source);
    }

    mapArray<TTarget extends object>(
        sources: TSource[],
        mapper: (s: TSource) => TTarget
    ): TTarget[] {
        return sources.map(mapper);
    }
}

// Conditional constraint pattern
type Constructor<T> = new (...args: any[]) => T;

function mixin<TBase extends Constructor<{}>>(Base: TBase) {
    return class extends Base {
        mixinProp = "mixed";
    };
}

// Usage
interface User extends Identifiable, Timestamped {
    name: string;
    email: string;
}

const store = new DataStore<User>();
const mapper = new Mapper<{ x: number }>();
const result = mapper.map({ x: 1 }, (s) => ({ y: s.x * 2 }));"#;

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
        output.contains("DataStore"),
        "expected DataStore class in output. output: {output}"
    );
    assert!(
        output.contains("createValidator"),
        "expected createValidator function in output. output: {output}"
    );
    assert!(
        output.contains("Mapper"),
        "expected Mapper class in output. output: {output}"
    );
    assert!(
        output.contains("mixin"),
        "expected mixin function in output. output: {output}"
    );
    assert!(
        output.contains("findRecent"),
        "expected findRecent method in output. output: {output}"
    );
    assert!(
        output.contains("mapArray"),
        "expected mapArray method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive type constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: CONDITIONAL TYPE EXPRESSIONS
// =============================================================================

/// Test source map generation for conditional types with infer keyword in ES5 output.
/// Validates that infer patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_infer_es5() {
    let source = r#"// Infer return type
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// Infer parameter types
type Parameters<T> = T extends (...args: infer P) => any ? P : never;

// Infer array element type
type ElementType<T> = T extends (infer E)[] ? E : never;

// Infer promise resolved type
type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T;

// Function using inferred types
function getReturnType<T extends (...args: any[]) => any>(
    fn: T
): ReturnType<T> | undefined {
    try {
        return fn() as ReturnType<T>;
    } catch {
        return undefined;
    }
}

function callWithArgs<T extends (...args: any[]) => any>(
    fn: T,
    ...args: Parameters<T>
): ReturnType<T> {
    return fn(...args);
}

const add = (a: number, b: number) => a + b;
const result = callWithArgs(add, 1, 2);"#;

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
        output.contains("getReturnType"),
        "expected getReturnType function in output. output: {output}"
    );
    assert!(
        output.contains("callWithArgs"),
        "expected callWithArgs function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional type with infer"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for distributive conditional types in ES5 output.
/// Validates that distributive conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_distributive_es5() {
    let source = r#"// Distributive conditional type
type ToArray<T> = T extends any ? T[] : never;

// Non-nullable extraction
type NonNullable<T> = T extends null | undefined ? never : T;

// Extract types from union
type Extract<T, U> = T extends U ? T : never;

// Exclude types from union
type Exclude<T, U> = T extends U ? never : T;

// Practical usage
type StringOrNumber = string | number | null | undefined;
type NonNullStringOrNumber = NonNullable<StringOrNumber>;
type OnlyStrings = Extract<StringOrNumber, string>;
type NoStrings = Exclude<StringOrNumber, string>;

function filterNonNull<T>(items: (T | null | undefined)[]): NonNullable<T>[] {
    return items.filter((item): item is NonNullable<T> => item != null);
}

function extractStrings(items: (string | number)[]): string[] {
    return items.filter((item): item is string => typeof item === "string");
}

const mixed = [1, "hello", null, 2, "world", undefined];
const nonNull = filterNonNull(mixed);
const strings = extractStrings([1, "a", 2, "b"]);"#;

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
        output.contains("filterNonNull"),
        "expected filterNonNull function in output. output: {output}"
    );
    assert!(
        output.contains("extractStrings"),
        "expected extractStrings function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for distributive conditional type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

