/// Test async static method with this capture
#[test]
fn test_source_map_async_class_integration_es5_static_this_capture() {
    let source = r#"class ConfigManager {
    private static instance: ConfigManager | null = null;
    private static config: Map<string, any> = new Map();
    private static initialized: boolean = false;

    static async initialize(): Promise<void> {
        if (this.initialized) {
            return;
        }
        await this.loadConfig();
        this.initialized = true;
    }

    private static async loadConfig(): Promise<void> {
        // Simulate async config loading
        const data = await fetch("/config.json");
        const json = await data.json();
        for (const [key, value] of Object.entries(json)) {
            this.config.set(key, value);
        }
    }

    static async get(key: string): Promise<any> {
        if (!this.initialized) {
            await this.initialize();
        }
        return this.config.get(key);
    }

    static async set(key: string, value: any): Promise<void> {
        if (!this.initialized) {
            await this.initialize();
        }
        this.config.set(key, value);
        await this.persist();
    }

    private static async persist(): Promise<void> {
        const data = Object.fromEntries(this.config);
        console.log("Persisting config:", data);
    }

    static async getInstance(): Promise<ConfigManager> {
        if (!this.instance) {
            await this.initialize();
            this.instance = new ConfigManager();
        }
        return this.instance;
    }
}

ConfigManager.initialize().then(() => console.log("Config ready"));"#;

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
        output.contains("ConfigManager"),
        "expected ConfigManager in output. output: {output}"
    );
    assert!(
        output.contains("initialize"),
        "expected initialize in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async static with this capture"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator class method with ES5 target
#[test]
fn test_source_map_async_class_integration_es5_generator_method() {
    let source = r#"class DataStream {
    private items: string[] = [];
    private batchSize: number;

    constructor(items: string[], batchSize: number = 10) {
        this.items = items;
        this.batchSize = batchSize;
    }

    async *stream(): AsyncGenerator<string[], void, unknown> {
        for (let i = 0; i < this.items.length; i += this.batchSize) {
            const batch = this.items.slice(i, i + this.batchSize);
            await this.processBatch(batch);
            yield batch;
        }
    }

    async *filter(predicate: (item: string) => boolean): AsyncGenerator<string, void, unknown> {
        for await (const batch of this.stream()) {
            for (const item of batch) {
                if (predicate(item)) {
                    yield item;
                }
            }
        }
    }

    async *map<T>(transform: (item: string) => T): AsyncGenerator<T, void, unknown> {
        for await (const batch of this.stream()) {
            for (const item of batch) {
                yield transform(item);
            }
        }
    }

    private async processBatch(batch: string[]): Promise<void> {
        console.log(`Processing batch of ${batch.length} items`);
    }
}

const stream = new DataStream(["a", "b", "c", "d", "e"]);
(async () => {
    for await (const batch of stream.stream()) {
        console.log("Batch:", batch);
    }
})();"#;

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
        output.contains("DataStream"),
        "expected DataStream in output. output: {output}"
    );
    assert!(
        output.contains("stream"),
        "expected stream method in output. output: {output}"
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

/// Test async constructor simulation pattern
#[test]
fn test_source_map_async_class_integration_es5_constructor_simulation() {
    let source = r#"class AsyncDatabase {
    private connection: any = null;
    private ready: boolean = false;

    private constructor() {
        // Private constructor - use create() instead
    }

    private async init(connectionString: string): Promise<void> {
        this.connection = await this.connect(connectionString);
        await this.runMigrations();
        this.ready = true;
    }

    private async connect(connectionString: string): Promise<any> {
        console.log("Connecting to:", connectionString);
        return { connected: true };
    }

    private async runMigrations(): Promise<void> {
        console.log("Running migrations...");
    }

    static async create(connectionString: string): Promise<AsyncDatabase> {
        const instance = new AsyncDatabase();
        await instance.init(connectionString);
        return instance;
    }

    async query(sql: string): Promise<any[]> {
        if (!this.ready) {
            throw new Error("Database not initialized");
        }
        console.log("Executing:", sql);
        return [];
    }

    async close(): Promise<void> {
        if (this.connection) {
            console.log("Closing connection");
            this.connection = null;
            this.ready = false;
        }
    }
}

// Factory pattern with async initialization
class AsyncService {
    private db: AsyncDatabase | null = null;

    private constructor() {}

    private async initialize(): Promise<void> {
        this.db = await AsyncDatabase.create("postgres://localhost/mydb");
    }

    static async create(): Promise<AsyncService> {
        const service = new AsyncService();
        await service.initialize();
        return service;
    }

    async getData(): Promise<any[]> {
        return this.db!.query("SELECT * FROM data");
    }
}

AsyncService.create().then(service => service.getData());"#;

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
        output.contains("AsyncDatabase"),
        "expected AsyncDatabase in output. output: {output}"
    );
    assert!(
        output.contains("AsyncService"),
        "expected AsyncService in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create factory in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async constructor simulation"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test combined async/class source map patterns
#[test]
fn test_source_map_async_class_integration_es5_comprehensive() {
    let source = r#"// Comprehensive async/class integration test
abstract class BaseRepository<T> {
    protected items: Map<string, T> = new Map();

    abstract validate(item: T): Promise<boolean>;

    async findById(id: string): Promise<T | undefined> {
        return this.items.get(id);
    }

    async save(id: string, item: T): Promise<void> {
        const isValid = await this.validate(item);
        if (!isValid) {
            throw new Error("Validation failed");
        }
        this.items.set(id, item);
    }
}

interface User {
    id: string;
    name: string;
    email: string;
}

class UserRepository extends BaseRepository<User> {
    private static instance: UserRepository;

    // Async arrow field
    validateEmail = async (email: string): Promise<boolean> => {
        return email.includes("@");
    };

    async validate(user: User): Promise<boolean> {
        const emailValid = await this.validateEmail(user.email);
        return emailValid && user.name.length > 0;
    }

    // Async generator for streaming users
    async *streamUsers(): AsyncGenerator<User, void, unknown> {
        for (const user of this.items.values()) {
            yield user;
        }
    }

    // Static async factory
    static async getInstance(): Promise<UserRepository> {
        if (!this.instance) {
            this.instance = new UserRepository();
            await this.instance.initialize();
        }
        return this.instance;
    }

    private async initialize(): Promise<void> {
        console.log("Initializing UserRepository");
    }

    // Async method with super call
    async save(id: string, user: User): Promise<void> {
        console.log(`Saving user: ${user.name}`);
        await super.save(id, user);
    }
}

class UserService {
    private repo: UserRepository | null = null;

    // Multiple async arrow fields
    getUser = async (id: string): Promise<User | undefined> => {
        const repo = await this.getRepo();
        return repo.findById(id);
    };

    createUser = async (user: User): Promise<void> => {
        const repo = await this.getRepo();
        await repo.save(user.id, user);
    };

    private async getRepo(): Promise<UserRepository> {
        if (!this.repo) {
            this.repo = await UserRepository.getInstance();
        }
        return this.repo;
    }
}

// Usage
const service = new UserService();
(async () => {
    await service.createUser({ id: "1", name: "John", email: "john@example.com" });
    const user = await service.getUser("1");
    console.log(user);

    const repo = await UserRepository.getInstance();
    for await (const u of repo.streamUsers()) {
        console.log("Streaming user:", u.name);
    }
})();"#;

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
        output.contains("BaseRepository"),
        "expected BaseRepository in output. output: {output}"
    );
    assert!(
        output.contains("UserRepository"),
        "expected UserRepository in output. output: {output}"
    );
    assert!(
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("validateEmail"),
        "expected validateEmail in output. output: {output}"
    );
    assert!(
        output.contains("streamUsers"),
        "expected streamUsers in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async/class integration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - GENERATOR TRANSFORM PATTERNS
// =============================================================================
// Tests for generator transform patterns with ES5 target to verify source maps
// work correctly with generator state machine transforms.

/// Test generator function basic yield mapping with typed parameters
#[test]
fn test_source_map_generator_transform_es5_basic_yield_mapping() {
    let source = r#"function* numberSequence(start: number, end: number): Generator<number, void, unknown> {
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

function* alphabetGenerator(): Generator<string, void, unknown> {
    const letters = "abcdefghijklmnopqrstuvwxyz";
    for (const letter of letters) {
        yield letter;
    }
}

// Using the generators
const numbers = numberSequence(1, 5);
for (const n of numbers) {
    console.log("Number:", n);
}

const alphabet = alphabetGenerator();
console.log(alphabet.next().value);
console.log(alphabet.next().value);"#;

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
        output.contains("numberSequence"),
        "expected numberSequence in output. output: {output}"
    );
    assert!(
        output.contains("alphabetGenerator"),
        "expected alphabetGenerator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic yield mapping"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

