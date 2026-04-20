#[test]
fn test_source_map_enum_es5_explicit_numeric() {
    let source = r#"enum HttpStatus {
    OK = 200,
    Created = 201,
    Accepted = 202,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    InternalServerError = 500
}

function handleResponse(status: HttpStatus): string {
    if (status >= 400) {
        return "Error";
    }
    return "Success";
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
        output.contains("HttpStatus") || output.contains("200"),
        "expected output to contain HttpStatus enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for explicit numeric enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_expression_initializers() {
    let source = r#"const BASE = 100;

enum Computed {
    First = BASE,
    Second = BASE + 1,
    Third = BASE * 2,
    Fourth = Math.floor(BASE / 3),
    Fifth = "prefix".length
}

const val = Computed.Third;"#;

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
        output.contains("Computed") || output.contains("BASE"),
        "expected output to contain Computed enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for expression initializer enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_ambient_declare() {
    let source = r#"declare enum ExternalStatus {
    Active,
    Inactive,
    Pending
}

enum LocalStatus {
    Active = 0,
    Inactive = 1,
    Pending = 2
}

const status: LocalStatus = LocalStatus.Active;"#;

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

    // Declare enums should be erased, only LocalStatus should remain
    assert!(
        output.contains("LocalStatus"),
        "expected output to contain LocalStatus enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ambient declare enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_member_as_type() {
    let source = r#"enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
}

type VerticalDirection = Direction.Up | Direction.Down;
type HorizontalDirection = Direction.Left | Direction.Right;

function move(dir: VerticalDirection): void {
    console.log(dir);
}

move(Direction.Up);"#;

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
        output.contains("Direction") || output.contains("UP"),
        "expected output to contain Direction enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for enum member as type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_keyof_typeof() {
    let source = r#"enum Color {
    Red = "red",
    Green = "green",
    Blue = "blue"
}

type ColorKey = keyof typeof Color;
type ColorValue = typeof Color[ColorKey];

function getColorName(key: ColorKey): string {
    return Color[key];
}

const result = getColorName("Red");"#;

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
        output.contains("Color") || output.contains("getColorName"),
        "expected output to contain Color enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for keyof typeof enum"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_nested_in_module() {
    let source = r#"module App {
    export enum Status {
        Loading,
        Ready,
        Error
    }

    export module Sub {
        export enum Priority {
            Low = 1,
            Medium = 2,
            High = 3
        }
    }
}

const status = App.Status.Ready;
const priority = App.Sub.Priority.High;"#;

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
        output.contains("App") || output.contains("Status"),
        "expected output to contain App module with enums. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested enum in module"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_with_interface() {
    let source = r#"enum TaskStatus {
    Todo = "TODO",
    InProgress = "IN_PROGRESS",
    Done = "DONE"
}

interface Task {
    id: number;
    title: string;
    status: TaskStatus;
}

function createTask(title: string): Task {
    return {
        id: Date.now(),
        title: title,
        status: TaskStatus.Todo
    };
}

const task = createTask("Test task");"#;

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
        output.contains("TaskStatus") || output.contains("createTask"),
        "expected output to contain TaskStatus enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for enum with interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_function_parameter() {
    let source = r#"enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3
}

function log(level: LogLevel, message: string): void {
    if (level >= LogLevel.Warn) {
        console.error(`[${LogLevel[level]}] ${message}`);
    } else {
        console.log(`[${LogLevel[level]}] ${message}`);
    }
}

log(LogLevel.Info, "Application started");
log(LogLevel.Error, "Something went wrong");"#;

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
        output.contains("LogLevel") || output.contains("log"),
        "expected output to contain LogLevel enum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for enum as function parameter"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_enum_es5_advanced_combined() {
    let source = r#"// Numeric enum with explicit values
enum Priority {
    Critical = 100,
    High = 75,
    Medium = 50,
    Low = 25
}

// String enum
enum Category {
    Bug = "BUG",
    Feature = "FEATURE",
    Task = "TASK"
}

// Const enum (should be inlined)
const enum Visibility {
    Public,
    Private,
    Internal
}

// Enum in class
class Issue {
    priority: Priority;
    category: Category;
    visibility: number;

    constructor(priority: Priority, category: Category) {
        this.priority = priority;
        this.category = category;
        this.visibility = Visibility.Public;
    }

    isPriority(level: Priority): boolean {
        return this.priority >= level;
    }
}

// Generic with enum constraint
function filterByCategory<T extends { category: Category }>(
    items: T[],
    category: Category
): T[] {
    return items.filter(item => item.category === category);
}

const issue = new Issue(Priority.High, Category.Bug);
console.log(issue.isPriority(Priority.Medium));"#;

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
        output.contains("Priority") || output.contains("Category"),
        "expected output to contain enum declarations. output: {output}"
    );
    assert!(
        output.contains("Issue"),
        "expected output to contain Issue class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for advanced enum patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Field ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_field_public_basic() {
    let source = r#"class Person {
    name: string;
    age: number;
    active: boolean;
}

const person = new Person();
person.name = "John";
person.age = 30;"#;

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
        output.contains("Person"),
        "expected output to contain Person class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for public fields"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

