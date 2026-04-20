#[test]
fn test_source_map_private_method_es5_accessor() {
    let source = r#"class SecureData {
    #data: string = "";
    #accessCount = 0;

    get #internalData(): string {
        this.#accessCount++;
        return this.#data;
    }

    set #internalData(value: string) {
        this.#data = value.trim();
    }

    get value(): string {
        return this.#internalData;
    }

    set value(v: string) {
        this.#internalData = v;
    }

    get accessCount(): number {
        return this.#accessCount;
    }
}

class CachedValue {
    #cache: Map<string, any> = new Map();

    get #cacheSize(): number {
        return this.#cache.size;
    }

    #getCached(key: string): any {
        return this.#cache.get(key);
    }

    #setCached(key: string, value: any): void {
        this.#cache.set(key, value);
    }

    store(key: string, value: any): void {
        this.#setCached(key, value);
    }

    retrieve(key: string): any {
        return this.#getCached(key);
    }

    get size(): number {
        return this.#cacheSize;
    }
}

const data = new SecureData();
data.value = "  hello  ";
console.log(data.value);
console.log(data.accessCount);

const cache = new CachedValue();
cache.store("key1", "value1");
console.log(cache.retrieve("key1"));
console.log(cache.size);"#;

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
        output.contains("SecureData"),
        "expected SecureData in output. output: {output}"
    );
    assert!(
        output.contains("CachedValue"),
        "expected CachedValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private accessor methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_inheritance() {
    let source = r#"class BaseLogger {
    #prefix = "[BASE]";

    #formatMessage(msg: string): string {
        return this.#prefix + " " + msg;
    }

    log(msg: string): void {
        console.log(this.#formatMessage(msg));
    }
}

class ChildLogger extends BaseLogger {
    #childPrefix = "[CHILD]";

    #formatChildMessage(msg: string): string {
        return this.#childPrefix + " " + msg;
    }

    logChild(msg: string): void {
        console.log(this.#formatChildMessage(msg));
    }

    logBoth(msg: string): void {
        this.log(msg);
        this.logChild(msg);
    }
}

class GrandchildLogger extends ChildLogger {
    #level = "DEBUG";

    #addLevel(msg: string): string {
        return "[" + this.#level + "] " + msg;
    }

    debug(msg: string): void {
        const formatted = this.#addLevel(msg);
        this.logBoth(formatted);
    }
}

const logger = new GrandchildLogger();
logger.debug("Test message");"#;

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
        output.contains("BaseLogger"),
        "expected BaseLogger in output. output: {output}"
    );
    assert!(
        output.contains("ChildLogger"),
        "expected ChildLogger in output. output: {output}"
    );
    assert!(
        output.contains("GrandchildLogger"),
        "expected GrandchildLogger in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_async() {
    let source = r#"class AsyncService {
    #baseUrl = "https://api.example.com";

    async #fetch(endpoint: string): Promise<any> {
        const url = this.#baseUrl + endpoint;
        const response = await fetch(url);
        return response.json();
    }

    async #processData(data: any): Promise<any> {
        await new Promise(r => setTimeout(r, 100));
        return { processed: true, data };
    }

    async #validate(data: any): Promise<boolean> {
        return data != null;
    }

    async getData(endpoint: string): Promise<any> {
        const raw = await this.#fetch(endpoint);
        if (await this.#validate(raw)) {
            return this.#processData(raw);
        }
        return null;
    }
}

class AsyncQueue {
    #queue: Promise<void> = Promise.resolve();

    async #runTask(task: () => Promise<void>): Promise<void> {
        await task();
    }

    async enqueue(task: () => Promise<void>): Promise<void> {
        this.#queue = this.#queue.then(() => this.#runTask(task));
        await this.#queue;
    }
}

const service = new AsyncService();
service.getData("/users").then(console.log);

const queue = new AsyncQueue();
queue.enqueue(async () => console.log("Task 1"));
queue.enqueue(async () => console.log("Task 2"));"#;

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
        output.contains("AsyncService"),
        "expected AsyncService in output. output: {output}"
    );
    assert!(
        output.contains("AsyncQueue"),
        "expected AsyncQueue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_generator() {
    let source = r#"class NumberGenerator {
    #start: number;
    #end: number;

    constructor(start: number, end: number) {
        this.#start = start;
        this.#end = end;
    }

    *#range(): Generator<number> {
        for (let i = this.#start; i <= this.#end; i++) {
            yield i;
        }
    }

    *#evens(): Generator<number> {
        for (const n of this.#range()) {
            if (n % 2 === 0) yield n;
        }
    }

    *#odds(): Generator<number> {
        for (const n of this.#range()) {
            if (n % 2 !== 0) yield n;
        }
    }

    getEvens(): number[] {
        return [...this.#evens()];
    }

    getOdds(): number[] {
        return [...this.#odds()];
    }

    getAll(): number[] {
        return [...this.#range()];
    }
}

const gen = new NumberGenerator(1, 10);
console.log(gen.getAll());
console.log(gen.getEvens());
console.log(gen.getOdds());"#;

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
        output.contains("NumberGenerator"),
        "expected NumberGenerator in output. output: {output}"
    );
    assert!(
        output.contains("getEvens"),
        "expected getEvens in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_with_fields() {
    let source = r#"class BankAccount {
    #balance: number = 0;
    #transactions: string[] = [];
    #accountId: string;

    constructor(id: string, initial: number) {
        this.#accountId = id;
        this.#balance = initial;
        this.#logTransaction("INIT", initial);
    }

    #logTransaction(type: string, amount: number): void {
        this.#transactions.push(type + ": " + amount);
    }

    #validateAmount(amount: number): boolean {
        return amount > 0;
    }

    #canWithdraw(amount: number): boolean {
        return this.#balance >= amount;
    }

    deposit(amount: number): boolean {
        if (!this.#validateAmount(amount)) return false;
        this.#balance += amount;
        this.#logTransaction("DEP", amount);
        return true;
    }

    withdraw(amount: number): boolean {
        if (!this.#validateAmount(amount)) return false;
        if (!this.#canWithdraw(amount)) return false;
        this.#balance -= amount;
        this.#logTransaction("WTH", amount);
        return true;
    }

    getBalance(): number {
        return this.#balance;
    }

    getHistory(): string[] {
        return [...this.#transactions];
    }
}

const account = new BankAccount("ACC001", 100);
account.deposit(50);
account.withdraw(30);
console.log(account.getBalance());
console.log(account.getHistory());"#;

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
        output.contains("BankAccount"),
        "expected BankAccount in output. output: {output}"
    );
    assert!(
        output.contains("deposit"),
        "expected deposit in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_chained_calls() {
    let source = r#"class StringBuilder {
    #value = "";

    #append(str: string): this {
        this.#value += str;
        return this;
    }

    #prepend(str: string): this {
        this.#value = str + this.#value;
        return this;
    }

    #wrap(prefix: string, suffix: string): this {
        this.#value = prefix + this.#value + suffix;
        return this;
    }

    #transform(fn: (s: string) => string): this {
        this.#value = fn(this.#value);
        return this;
    }

    add(str: string): this {
        return this.#append(str);
    }

    addBefore(str: string): this {
        return this.#prepend(str);
    }

    surround(prefix: string, suffix: string): this {
        return this.#wrap(prefix, suffix);
    }

    apply(fn: (s: string) => string): this {
        return this.#transform(fn);
    }

    build(): string {
        return this.#value;
    }
}

const result = new StringBuilder()
    .add("Hello")
    .add(" ")
    .add("World")
    .surround("[", "]")
    .apply(s => s.toUpperCase())
    .build();

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
        output.contains("StringBuilder"),
        "expected StringBuilder in output. output: {output}"
    );
    assert!(
        output.contains("build"),
        "expected build in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained private method calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_parameters() {
    let source = r#"class ParameterHandler {
    #processDefaults(a: number = 0, b: string = "default"): string {
        return b + ": " + a;
    }

    #processRest(...items: number[]): number {
        return items.reduce((sum, n) => sum + n, 0);
    }

    #processDestructured({ x, y }: { x: number; y: number }): number {
        return x + y;
    }

    #processArrayDestructured([first, second]: [string, string]): string {
        return first + " and " + second;
    }

    #processGeneric<T>(value: T, transform: (v: T) => string): string {
        return transform(value);
    }

    withDefaults(a?: number, b?: string): string {
        return this.#processDefaults(a, b);
    }

    withRest(...nums: number[]): number {
        return this.#processRest(...nums);
    }

    withObject(obj: { x: number; y: number }): number {
        return this.#processDestructured(obj);
    }

    withArray(arr: [string, string]): string {
        return this.#processArrayDestructured(arr);
    }

    withGeneric<T>(val: T, fn: (v: T) => string): string {
        return this.#processGeneric(val, fn);
    }
}

const handler = new ParameterHandler();
console.log(handler.withDefaults());
console.log(handler.withDefaults(42, "custom"));
console.log(handler.withRest(1, 2, 3, 4, 5));
console.log(handler.withObject({ x: 10, y: 20 }));
console.log(handler.withArray(["hello", "world"]));
console.log(handler.withGeneric(123, n => n.toString()));"#;

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
        output.contains("ParameterHandler"),
        "expected ParameterHandler in output. output: {output}"
    );
    assert!(
        output.contains("withDefaults"),
        "expected withDefaults in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_comprehensive() {
    let source = r#"// Comprehensive private method patterns for ES5 transform testing

// Basic instance private methods
class Counter {
    #count = 0;

    #increment(): void {
        this.#count++;
    }

    #decrement(): void {
        this.#count--;
    }

    inc(): void { this.#increment(); }
    dec(): void { this.#decrement(); }
    get value(): number { return this.#count; }
}

// Static private methods
class Utils {
    static #formatNumber(n: number): string {
        return n.toFixed(2);
    }

    static #validateInput(input: any): boolean {
        return input != null;
    }

    static format(n: number): string {
        return Utils.#formatNumber(n);
    }

    static isValid(input: any): boolean {
        return Utils.#validateInput(input);
    }
}

// Private accessors
class Config {
    #settings: Map<string, any> = new Map();

    get #size(): number {
        return this.#settings.size;
    }

    #get(key: string): any {
        return this.#settings.get(key);
    }

    #set(key: string, value: any): void {
        this.#settings.set(key, value);
    }

    set(key: string, value: any): void {
        this.#set(key, value);
    }

    get(key: string): any {
        return this.#get(key);
    }

    get count(): number {
        return this.#size;
    }
}

// Async private methods
class DataLoader {
    #cache: Map<string, any> = new Map();

    async #fetchFromApi(url: string): Promise<any> {
        return { url, data: "mock" };
    }

    async #processResponse(response: any): Promise<any> {
        return { processed: true, ...response };
    }

    async load(url: string): Promise<any> {
        if (this.#cache.has(url)) {
            return this.#cache.get(url);
        }
        const response = await this.#fetchFromApi(url);
        const processed = await this.#processResponse(response);
        this.#cache.set(url, processed);
        return processed;
    }
}

// Private methods with inheritance
class Animal {
    #name: string;

    constructor(name: string) {
        this.#name = name;
    }

    #formatName(): string {
        return "[" + this.#name + "]";
    }

    describe(): string {
        return "Animal: " + this.#formatName();
    }
}

class Dog extends Animal {
    #breed: string;

    constructor(name: string, breed: string) {
        super(name);
        this.#breed = breed;
    }

    #formatBreed(): string {
        return "(" + this.#breed + ")";
    }

    describe(): string {
        return super.describe() + " " + this.#formatBreed();
    }
}

// Usage
const counter = new Counter();
counter.inc();
counter.inc();
console.log(counter.value);

console.log(Utils.format(3.14159));
console.log(Utils.isValid("test"));

const config = new Config();
config.set("key", "value");
console.log(config.get("key"));
console.log(config.count);

const loader = new DataLoader();
loader.load("/api/data").then(console.log);

const dog = new Dog("Rex", "German Shepherd");
console.log(dog.describe());"#;

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
        output.contains("Counter"),
        "expected Counter in output. output: {output}"
    );
    assert!(
        output.contains("Utils"),
        "expected Utils in output. output: {output}"
    );
    assert!(
        output.contains("Config"),
        "expected Config in output. output: {output}"
    );
    assert!(
        output.contains("DataLoader"),
        "expected DataLoader in output. output: {output}"
    );
    assert!(
        output.contains("Animal"),
        "expected Animal in output. output: {output}"
    );
    assert!(
        output.contains("Dog"),
        "expected Dog in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - ASYNC GENERATOR PATTERNS
// =============================================================================
// Tests for async generator patterns with ES5 target to verify source maps
// work correctly with async generator transforms.

/// Test basic async generator function with ES5 target
#[test]
fn test_source_map_async_generator_es5_basic() {
    let source = r#"async function* generateNumbers() {
    yield 1;
    yield 2;
    yield 3;
}

async function consume() {
    for await (const num of generateNumbers()) {
        console.log(num);
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
        output.contains("generateNumbers"),
        "expected generateNumbers in output. output: {output}"
    );
    assert!(
        output.contains("consume"),
        "expected consume in output. output: {output}"
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

/// Test async generator with yield* delegation and ES5 target
#[test]
fn test_source_map_async_generator_es5_yield_delegation() {
    let source = r#"async function* innerGenerator() {
    yield "a";
    yield "b";
}

async function* outerGenerator() {
    yield "start";
    yield* innerGenerator();
    yield "end";
}

async function main() {
    for await (const value of outerGenerator()) {
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
        output.contains("innerGenerator"),
        "expected innerGenerator in output. output: {output}"
    );
    assert!(
        output.contains("outerGenerator"),
        "expected outerGenerator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for yield delegation"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

