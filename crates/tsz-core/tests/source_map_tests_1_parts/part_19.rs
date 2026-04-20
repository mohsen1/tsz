#[test]
fn test_source_map_class_decorator_multiple() {
    // Test multiple class decorators source mapping
    let source = r#"function Component(target: Function) {}
function Injectable(target: Function) {}
function Sealed(target: Function) {}

@Component
@Injectable
@Sealed
class Service {
    constructor() {}
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

    // Verify output contains the decorated class
    assert!(
        output.contains("Service"),
        "expected output to contain Service class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple class decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

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

