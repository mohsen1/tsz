#[test]
fn test_source_map_async_es5_promise_race() {
    let source = r#"async function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
    const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Timeout')), ms);
    });
    return await Promise.race([promise, timeoutPromise]);
}

async function fetchWithTimeout(url: string): Promise<any> {
    const result = await timeout(fetch(url), 5000);
    return result.json();
}

fetchWithTimeout('/api/data');"#;

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
        output.contains("timeout") || output.contains("Promise"),
        "expected output to contain timeout function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise.race"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_error_handling() {
    let source = r#"class NetworkError extends Error {
    constructor(message: string, public statusCode: number) {
        super(message);
        this.name = 'NetworkError';
    }
}

async function fetchWithRetry(url: string, retries: number = 3): Promise<any> {
    for (let i = 0; i < retries; i++) {
        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new NetworkError('Request failed', response.status);
            }
            return await response.json();
        } catch (error) {
            if (i === retries - 1) {
                throw error;
            }
            await new Promise(r => setTimeout(r, 1000 * Math.pow(2, i)));
        }
    }
}

fetchWithRetry('/api/data').catch(console.error);"#;

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
        output.contains("fetchWithRetry") || output.contains("NetworkError"),
        "expected output to contain fetchWithRetry. output: {output}"
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

#[test]
fn test_source_map_async_es5_sequential_vs_parallel() {
    let source = r#"async function sequential(): Promise<number[]> {
    const a = await fetch('/a').then(r => r.json());
    const b = await fetch('/b').then(r => r.json());
    const c = await fetch('/c').then(r => r.json());
    return [a, b, c];
}

async function parallel(): Promise<number[]> {
    const [a, b, c] = await Promise.all([
        fetch('/a').then(r => r.json()),
        fetch('/b').then(r => r.json()),
        fetch('/c').then(r => r.json())
    ]);
    return [a, b, c];
}

async function mixed(): Promise<void> {
    const first = await fetch('/first').then(r => r.json());
    const [second, third] = await Promise.all([
        fetch('/second'),
        fetch('/third')
    ]);
    console.log(first, second, third);
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
        output.contains("sequential") || output.contains("parallel"),
        "expected output to contain sequential or parallel. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for sequential vs parallel"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_closure_capture() {
    let source = r#"function createAsyncCounter() {
    let count = 0;

    return {
        increment: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 100));
            return ++count;
        },
        decrement: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 100));
            return --count;
        },
        getCount: async (): Promise<number> => {
            await new Promise(r => setTimeout(r, 50));
            return count;
        }
    };
}

const counter = createAsyncCounter();
counter.increment().then(console.log);"#;

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
        output.contains("createAsyncCounter") || output.contains("increment"),
        "expected output to contain createAsyncCounter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for closure capture"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_inheritance() {
    let source = r#"abstract class AsyncResource {
    protected abstract load(): Promise<any>;

    async initialize(): Promise<void> {
        const data = await this.load();
        await this.process(data);
    }

    protected async process(data: any): Promise<void> {
        console.log('Processing:', data);
    }
}

class UserResource extends AsyncResource {
    protected async load(): Promise<any> {
        const response = await fetch('/api/users');
        return response.json();
    }

    protected async process(data: any): Promise<void> {
        await super.process(data);
        console.log('Users processed');
    }
}

const resource = new UserResource();
resource.initialize();"#;

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
        output.contains("AsyncResource") || output.contains("UserResource"),
        "expected output to contain AsyncResource or UserResource. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_factory_pattern() {
    let source = r#"interface Connection {
    query(sql: string): Promise<any[]>;
    close(): Promise<void>;
}

async function createConnection(config: any): Promise<Connection> {
    await new Promise(r => setTimeout(r, 100));

    return {
        query: async (sql: string): Promise<any[]> => {
            await new Promise(r => setTimeout(r, 50));
            return [{ id: 1, sql }];
        },
        close: async (): Promise<void> => {
            await new Promise(r => setTimeout(r, 50));
            console.log('Connection closed');
        }
    };
}

async function useConnection(): Promise<void> {
    const conn = await createConnection({ host: 'localhost' });
    const results = await conn.query('SELECT * FROM users');
    console.log(results);
    await conn.close();
}

useConnection();"#;

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
        output.contains("createConnection") || output.contains("useConnection"),
        "expected output to contain createConnection. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for factory pattern"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_queue_processing() {
    let source = r#"class AsyncQueue<T> {
    private queue: T[] = [];
    private processing = false;

    async add(item: T): Promise<void> {
        this.queue.push(item);
        if (!this.processing) {
            await this.process();
        }
    }

    private async process(): Promise<void> {
        this.processing = true;
        while (this.queue.length > 0) {
            const item = this.queue.shift()!;
            await this.handleItem(item);
        }
        this.processing = false;
    }

    private async handleItem(item: T): Promise<void> {
        await new Promise(r => setTimeout(r, 100));
        console.log('Processed:', item);
    }
}

const queue = new AsyncQueue<string>();
queue.add('item1');
queue.add('item2');"#;

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
        output.contains("AsyncQueue") || output.contains("process"),
        "expected output to contain AsyncQueue. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for queue processing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_event_emitter() {
    let source = r#"class AsyncEventEmitter {
    private listeners: Map<string, Array<(data: any) => Promise<void>>> = new Map();

    on(event: string, handler: (data: any) => Promise<void>): void {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event)!.push(handler);
    }

    async emit(event: string, data: any): Promise<void> {
        const handlers = this.listeners.get(event) || [];
        for (const handler of handlers) {
            await handler(data);
        }
    }

    async emitParallel(event: string, data: any): Promise<void> {
        const handlers = this.listeners.get(event) || [];
        await Promise.all(handlers.map(h => h(data)));
    }
}

const emitter = new AsyncEventEmitter();
emitter.on('data', async (d) => { console.log(d); });
emitter.emit('data', { value: 42 });"#;

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
        output.contains("AsyncEventEmitter") || output.contains("emit"),
        "expected output to contain AsyncEventEmitter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for event emitter"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_comprehensive() {
    let source = r#"// Async utility functions
const delay = (ms: number): Promise<void> =>
    new Promise(resolve => setTimeout(resolve, ms));

async function* asyncRange(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i < end; i++) {
        await delay(10);
        yield i;
    }
}

// Async class with all patterns
class DataProcessor {
    private cache = new Map<string, any>();

    constructor(private readonly baseUrl: string) {}

    async fetch(path: string): Promise<any> {
        const key = this.baseUrl + path;
        if (this.cache.has(key)) {
            return this.cache.get(key);
        }
        const response = await fetch(key);
        const data = await response.json();
        this.cache.set(key, data);
        return data;
    }

    async fetchMany(paths: string[]): Promise<any[]> {
        return Promise.all(paths.map(p => this.fetch(p)));
    }

    async *processStream(paths: string[]): AsyncGenerator<any> {
        for await (const i of asyncRange(0, paths.length)) {
            yield await this.fetch(paths[i]);
        }
    }

    async processWithRetry(path: string, retries = 3): Promise<any> {
        for (let i = 0; i < retries; i++) {
            try {
                return await this.fetch(path);
            } catch (e) {
                if (i === retries - 1) throw e;
                await delay(1000 * (i + 1));
            }
        }
    }
}

// Usage
const processor = new DataProcessor('https://api.example.com');

(async () => {
    const data = await processor.fetch('/users');
    const [users, posts] = await processor.fetchMany(['/users', '/posts']);

    for await (const item of processor.processStream(['/a', '/b', '/c'])) {
        console.log(item);
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
        output.contains("DataProcessor"),
        "expected output to contain DataProcessor. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Generator ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_generator_es5_control_flow() {
    let source = r#"function* controlFlowGenerator(n: number): Generator<number> {
    for (let i = 0; i < n; i++) {
        if (i % 2 === 0) {
            yield i * 2;
        } else {
            yield i * 3;
        }
    }

    let j = 0;
    while (j < 3) {
        yield j * 10;
        j++;
    }

    switch (n) {
        case 1: yield 100; break;
        case 2: yield 200; break;
        default: yield 999;
    }
}

const gen = controlFlowGenerator(5);
console.log([...gen]);"#;

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
        output.contains("controlFlowGenerator"),
        "expected output to contain controlFlowGenerator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for control flow generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

