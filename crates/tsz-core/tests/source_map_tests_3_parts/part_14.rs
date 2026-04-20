#[test]
fn test_source_map_class_field_public_initializers() {
    let source = r#"class Config {
    host: string = "localhost";
    port: number = 8080;
    debug: boolean = false;
    tags: string[] = [];
    metadata: object = {};
}

const config = new Config();
console.log(config.host, config.port);"#;

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
        output.contains("Config") || output.contains("localhost"),
        "expected output to contain Config class or initializers. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for field initializers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_static_basic() {
    let source = r#"class Counter {
    static count: number = 0;
    static name: string = "Counter";

    static increment(): void {
        Counter.count++;
    }

    static reset(): void {
        Counter.count = 0;
    }
}

Counter.increment();
console.log(Counter.count);"#;

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
        "expected output to contain Counter class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_static_initializers() {
    let source = r#"class App {
    static version: string = "1.0.0";
    static buildDate: Date = new Date();
    static features: string[] = ["auth", "logging"];
    static config = {
        debug: true,
        timeout: 5000
    };
}

console.log(App.version, App.features);"#;

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
        output.contains("App") || output.contains("version"),
        "expected output to contain App class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static field initializers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_computed() {
    let source = r#"const nameKey = "name";
const ageKey = "age";

class Person {
    [nameKey]: string = "Unknown";
    [ageKey]: number = 0;
    ["status"]: string = "active";
    [Symbol.toStringTag]: string = "Person";
}

const p = new Person();
console.log(p[nameKey]);"#;

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
        output.contains("Person") || output.contains("nameKey"),
        "expected output to contain Person class or computed keys. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_private_es5() {
    let source = r#"class BankAccount {
    #balance: number = 0;
    #owner: string;

    constructor(owner: string, initialBalance: number) {
        this.#owner = owner;
        this.#balance = initialBalance;
    }

    deposit(amount: number): void {
        this.#balance += amount;
    }

    getBalance(): number {
        return this.#balance;
    }
}

const account = new BankAccount("John", 100);
account.deposit(50);"#;

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
        output.contains("BankAccount") || output.contains("deposit"),
        "expected output to contain BankAccount class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private fields ES5"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_static_private() {
    let source = r#"class Logger {
    static #instance: Logger | null = null;
    static #logLevel: number = 1;

    private constructor() {}

    static getInstance(): Logger {
        if (!Logger.#instance) {
            Logger.#instance = new Logger();
        }
        return Logger.#instance;
    }

    static setLogLevel(level: number): void {
        Logger.#logLevel = level;
    }
}

const logger = Logger.getInstance();"#;

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
        output.contains("Logger") || output.contains("getInstance"),
        "expected output to contain Logger class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static private fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_readonly() {
    let source = r#"class Constants {
    readonly PI: number = 3.14159;
    readonly E: number = 2.71828;
    static readonly MAX_SIZE: number = 1000;
    static readonly APP_NAME: string = "MyApp";
}

const c = new Constants();
console.log(c.PI, Constants.MAX_SIZE);"#;

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
        output.contains("Constants") || output.contains("3.14159"),
        "expected output to contain Constants class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for readonly fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_with_accessors() {
    let source = r#"class Rectangle {
    #width: number = 0;
    #height: number = 0;

    get width(): number {
        return this.#width;
    }

    set width(value: number) {
        this.#width = Math.max(0, value);
    }

    get height(): number {
        return this.#height;
    }

    set height(value: number) {
        this.#height = Math.max(0, value);
    }

    get area(): number {
        return this.#width * this.#height;
    }
}

const rect = new Rectangle();
rect.width = 10;
rect.height = 5;"#;

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
        output.contains("Rectangle") || output.contains("width"),
        "expected output to contain Rectangle class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for fields with accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_field_combined() {
    let source = r#"const dynamicKey = "dynamicProp";

class CompleteEntity {
    // Public fields
    name: string = "";
    count: number = 0;

    // Static fields
    static instances: number = 0;
    static readonly VERSION: string = "1.0";

    // Private fields
    #id: number;
    #secret: string = "hidden";

    // Static private
    static #totalCreated: number = 0;

    // Computed field
    [dynamicKey]: boolean = true;

    // Readonly
    readonly createdAt: Date = new Date();

    constructor(name: string) {
        this.name = name;
        this.#id = ++CompleteEntity.#totalCreated;
        CompleteEntity.instances++;
    }

    get id(): number {
        return this.#id;
    }

    static getTotal(): number {
        return CompleteEntity.#totalCreated;
    }
}

const entity = new CompleteEntity("Test");
console.log(entity.name, entity.id, CompleteEntity.instances);"#;

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
        output.contains("CompleteEntity"),
        "expected output to contain CompleteEntity class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined class fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Decorator ES5 Source Map Tests - Extended Patterns
// =============================================================================

#[test]
fn test_source_map_decorator_es5_class_with_metadata() {
    let source = r#"function Component(config: { selector: string; template: string }) {
    return function<T extends { new(...args: any[]): {} }>(constructor: T) {
        return class extends constructor {
            selector = config.selector;
            template = config.template;
        };
    };
}

@Component({
    selector: 'app-root',
    template: '<div>Hello</div>'
})
class AppComponent {
    title: string = 'My App';
}

const app = new AppComponent();"#;

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
        output.contains("AppComponent") || output.contains("Component"),
        "expected output to contain AppComponent. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class decorator with metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

