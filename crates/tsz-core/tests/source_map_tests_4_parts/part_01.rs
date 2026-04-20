#[test]
fn test_source_map_accessor_es5_computed_names() {
    // Test computed accessor names transformation
    let source = r#"const propName = "dynamicProp";
const symbolKey = Symbol("mySymbol");

class Dynamic {
    private _values: { [key: string]: any } = {};

    get [propName](): any {
        return this._values[propName];
    }

    set [propName](value: any) {
        this._values[propName] = value;
    }
}

const obj = new Dynamic();
obj[propName] = "test";"#;

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
        output.contains("Dynamic"),
        "expected Dynamic class in output. output: {output}"
    );
    assert!(
        output.contains("propName"),
        "expected propName reference in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed accessor names"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_decorator() {
    // Test accessor with decorators transformation
    let source = r#"function readonly(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    descriptor.writable = false;
    return descriptor;
}

function log(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.get;
    descriptor.get = function() {
        console.log("Getting " + propertyKey);
        return original?.call(this);
    };
    return descriptor;
}

class Config {
    private _value: string = "default";

    @log
    @readonly
    get value(): string {
        return this._value;
    }
}

const config = new Config();
console.log(config.value);"#;

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
        output.contains("Config"),
        "expected Config class in output. output: {output}"
    );
    assert!(
        output.contains("readonly") || output.contains("log"),
        "expected decorator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_getter_only() {
    // Test getter-only accessor transformation
    let source = r#"class Rectangle {
    constructor(private width: number, private height: number) {}

    get area(): number {
        return this.width * this.height;
    }

    get perimeter(): number {
        return 2 * (this.width + this.height);
    }

    get diagonal(): number {
        return Math.sqrt(this.width * this.width + this.height * this.height);
    }
}

const rect = new Rectangle(3, 4);
console.log(rect.area, rect.perimeter, rect.diagonal);"#;

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
        output.contains("Rectangle"),
        "expected Rectangle class in output. output: {output}"
    );
    assert!(
        output.contains("area") && output.contains("perimeter"),
        "expected getter names in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for getter-only accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_setter_only() {
    // Test setter-only accessor transformation
    let source = r#"class Logger {
    private logs: string[] = [];

    set message(value: string) {
        this.logs.push("[INFO] " + value);
    }

    set warning(value: string) {
        this.logs.push("[WARN] " + value);
    }

    set error(value: string) {
        this.logs.push("[ERROR] " + value);
    }

    getLogs(): string[] {
        return this.logs;
    }
}

const logger = new Logger();
logger.message = "Started";
logger.warning = "Low memory";
logger.error = "Failed";"#;

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
        output.contains("Logger"),
        "expected Logger class in output. output: {output}"
    );
    assert!(
        output.contains("message") && output.contains("warning"),
        "expected setter names in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for setter-only accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_inherited() {
    // Test inherited accessor transformation
    let source = r#"class Base {
    protected _value: number = 0;

    get value(): number {
        return this._value;
    }

    set value(v: number) {
        this._value = v;
    }
}

class Derived extends Base {
    get value(): number {
        return super.value * 2;
    }

    set value(v: number) {
        super.value = v / 2;
    }

    get doubleValue(): number {
        return this._value * 2;
    }
}

const derived = new Derived();
derived.value = 10;
console.log(derived.value, derived.doubleValue);"#;

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
        output.contains("Base") && output.contains("Derived"),
        "expected Base and Derived classes in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for inherited accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_validation() {
    // Test accessor with validation logic
    let source = r#"class ValidatedInput {
    private _email: string = "";
    private _age: number = 0;
    private _name: string = "";

    get email(): string {
        return this._email;
    }

    set email(value: string) {
        if (!value.includes("@")) {
            throw new Error("Invalid email");
        }
        this._email = value;
    }

    get age(): number {
        return this._age;
    }

    set age(value: number) {
        if (value < 0 || value > 150) {
            throw new Error("Invalid age");
        }
        this._age = value;
    }

    get name(): string {
        return this._name;
    }

    set name(value: string) {
        if (value.length < 2) {
            throw new Error("Name too short");
        }
        this._name = value.trim();
    }
}

const input = new ValidatedInput();
input.email = "test@example.com";
input.age = 25;
input.name = "Alice";"#;

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
        output.contains("ValidatedInput"),
        "expected ValidatedInput class in output. output: {output}"
    );
    assert!(
        output.contains("email") && output.contains("age") && output.contains("name"),
        "expected accessor names in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for validation accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_lazy_initialization() {
    // Test lazy initialization accessors
    let source = r#"class LazyLoader {
    private _data: string[] | null = null;
    private _config: object | null = null;

    get data(): string[] {
        if (this._data === null) {
            this._data = this.loadData();
        }
        return this._data;
    }

    get config(): object {
        if (this._config === null) {
            this._config = this.loadConfig();
        }
        return this._config;
    }

    private loadData(): string[] {
        return ["item1", "item2", "item3"];
    }

    private loadConfig(): object {
        return { setting: true };
    }
}

const loader = new LazyLoader();
console.log(loader.data);
console.log(loader.config);"#;

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
        output.contains("LazyLoader"),
        "expected LazyLoader class in output. output: {output}"
    );
    assert!(
        output.contains("data") && output.contains("config"),
        "expected accessor names in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for lazy initialization accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_comprehensive() {
    // Comprehensive accessor patterns test
    let source = r#"// Decorator for logging
function track(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.get;
    descriptor.get = function() {
        console.log("Accessed: " + key);
        return original?.call(this);
    };
    return descriptor;
}

// Base class with accessors
class Observable {
    private _listeners: Function[] = [];

    protected notify(prop: string, value: any): void {
        this._listeners.forEach(function(fn) { fn(prop, value); });
    }

    subscribe(fn: Function): void {
        this._listeners.push(fn);
    }
}

// Derived class with multiple accessor patterns
class Person extends Observable {
    private _firstName: string = "";
    private _lastName: string = "";
    private _age: number = 0;
    private static _count: number = 0;

    constructor() {
        super();
        Person._count++;
    }

    // Basic getter/setter
    get firstName(): string {
        return this._firstName;
    }

    set firstName(value: string) {
        this._firstName = value;
        this.notify("firstName", value);
    }

    // Getter/setter with validation
    get age(): number {
        return this._age;
    }

    set age(value: number) {
        if (value < 0) throw new Error("Age cannot be negative");
        this._age = value;
        this.notify("age", value);
    }

    // Computed getter
    @track
    get fullName(): string {
        return this._firstName + " " + this._lastName;
    }

    set fullName(value: string) {
        var parts = value.split(" ");
        this._firstName = parts[0] || "";
        this._lastName = parts[1] || "";
    }

    // Static accessor
    static get count(): number {
        return Person._count;
    }

    static set count(value: number) {
        Person._count = value;
    }
}

// Usage
const person = new Person();
person.firstName = "John";
person.age = 30;
console.log(person.fullName);
console.log(Person.count);"#;

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
        output.contains("Observable"),
        "expected Observable class in output. output: {output}"
    );
    assert!(
        output.contains("Person"),
        "expected Person class in output. output: {output}"
    );
    assert!(
        output.contains("firstName"),
        "expected firstName accessor in output. output: {output}"
    );
    assert!(
        output.contains("fullName"),
        "expected fullName accessor in output. output: {output}"
    );
    assert!(
        output.contains("track"),
        "expected track decorator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive accessor patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// For-Of/For-In Loop ES5 Transform Source Map Tests
// =============================================================================
// Tests for for-of and for-in loop compilation with ES5 target.
// for-of loops should transform to use iterators while preserving source maps.

#[test]
fn test_source_map_loop_es5_basic_for_of() {
    // Test basic for-of loop transformation
    let source = r#"const items = [1, 2, 3, 4, 5];
let sum = 0;

for (const item of items) {
    sum += item;
}

console.log(sum);"#;

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
        output.contains("items"),
        "expected items array in output. output: {output}"
    );
    assert!(
        output.contains("sum"),
        "expected sum variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic for-of"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_basic_for_in() {
    // Test basic for-in loop transformation
    let source = r#"const obj = { a: 1, b: 2, c: 3 };
const keys: string[] = [];

for (const key in obj) {
    keys.push(key);
}

console.log(keys);"#;

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
        output.contains("obj"),
        "expected obj object in output. output: {output}"
    );
    assert!(
        output.contains("key"),
        "expected key variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic for-in"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

