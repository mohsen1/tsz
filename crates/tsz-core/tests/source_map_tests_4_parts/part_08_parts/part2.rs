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
