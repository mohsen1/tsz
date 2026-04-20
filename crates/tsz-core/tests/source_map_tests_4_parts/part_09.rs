#[test]
fn test_source_map_template_literal_es5_nested() {
    let source = r#"const a = 1;
const b = 2;
const c = 3;
const deep = `Level 1: ${`Level 2: ${`Level 3: ${a + b + c}`}`}`;
const complex = `A=${a}, B=${b}, Sum=${a + b}, Product=${a * b}`;
const conditional = `Value: ${a > 0 ? `positive ${a}` : `negative ${a}`}`;
console.log(deep, complex, conditional);"#;

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
        output.contains("deep"),
        "expected deep in output. output: {output}"
    );
    assert!(
        output.contains("complex"),
        "expected complex in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested templates"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_tagged() {
    let source = r#"function highlight(strings: TemplateStringsArray, ...values: any[]) {
    return strings.reduce((acc, str, i) => acc + str + (values[i] || ""), "");
}

function sql(strings: TemplateStringsArray, ...values: any[]) {
    return { query: strings.join("?"), params: values };
}

const name = "Bob";
const age = 25;

const highlighted = highlight`Name: ${name}, Age: ${age}`;
const query = sql`SELECT * FROM users WHERE name = ${name} AND age > ${age}`;

console.log(highlighted, query);"#;

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
        output.contains("highlight"),
        "expected highlight in output. output: {output}"
    );
    assert!(
        output.contains("sql"),
        "expected sql in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for tagged templates"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_multiline() {
    let source = r#"const html = `
<div>
    <h1>Title</h1>
    <p>Content here</p>
</div>
`;

const code = `function example() {
    return 42;
}`;

const json = `{
    "name": "test",
    "value": 123
}`;

console.log(html, code, json);"#;

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
        output.contains("html"),
        "expected html in output. output: {output}"
    );
    assert!(
        output.contains("code"),
        "expected code in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiline templates"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_function_call() {
    let source = r#"function getName(): string {
    return "Charlie";
}

function getAge(): number {
    return 28;
}

function format(s: string): string {
    return s.toUpperCase();
}

const msg1 = `Hello, ${getName()}!`;
const msg2 = `Age: ${getAge().toString()}`;
const msg3 = `Name: ${format(getName())}`;
const msg4 = `Combined: ${getName()} is ${getAge()}`;

console.log(msg1, msg2, msg3, msg4);"#;

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
        output.contains("getName"),
        "expected getName in output. output: {output}"
    );
    assert!(
        output.contains("getAge"),
        "expected getAge in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for templates with function calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_method_chain() {
    let source = r#"const str = "hello";
const arr = [1, 2, 3];

const msg1 = `Upper: ${str.toUpperCase()}`;
const msg2 = `Length: ${str.length}`;
const msg3 = `Replaced: ${str.replace("l", "L")}`;
const msg4 = `Joined: ${arr.join(", ")}`;
const msg5 = `Mapped: ${arr.map(x => x * 2).join("-")}`;
const msg6 = `Chained: ${str.toUpperCase().split("").reverse().join("")}`;

console.log(msg1, msg2, msg3, msg4, msg5, msg6);"#;

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
        output.contains("str"),
        "expected str in output. output: {output}"
    );
    assert!(
        output.contains("toUpperCase"),
        "expected toUpperCase in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for templates with method chains"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_conditional() {
    let source = r#"const x = 10;
const y = 5;
const active = true;
const name: string | null = "David";

const msg1 = `Status: ${active ? "active" : "inactive"}`;
const msg2 = `Max: ${x > y ? x : y}`;
const msg3 = `Name: ${name || "Unknown"}`;
const msg4 = `Sign: ${x > 0 ? "positive" : x < 0 ? "negative" : "zero"}`;
const msg5 = `Check: ${x && y ? `Both truthy: ${x}, ${y}` : "Not both truthy"}`;

console.log(msg1, msg2, msg3, msg4, msg5);"#;

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
        output.contains("active"),
        "expected active in output. output: {output}"
    );
    assert!(
        output.contains("msg1"),
        "expected msg1 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for templates with conditionals"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_complex_expressions() {
    let source = r#"const items = [1, 2, 3, 4, 5];
const obj = { x: 10, y: 20 };

const msg1 = `Sum: ${items.reduce((a, b) => a + b, 0)}`;
const msg2 = `Total: ${obj.x + obj.y}`;
const msg3 = `Array: [${items.map(i => i * 2)}]`;
const msg4 = `Object: {x: ${obj.x}, y: ${obj.y}}`;
const msg5 = `Complex: ${items.filter(i => i > 2).map(i => i * 3).join(", ")}`;
const msg6 = `Spread: ${[...items, 6, 7].length}`;
const msg7 = `Destructured: ${(({ x, y }) => x + y)(obj)}`;

console.log(msg1, msg2, msg3, msg4, msg5, msg6, msg7);"#;

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
        "expected items in output. output: {output}"
    );
    assert!(
        output.contains("reduce"),
        "expected reduce in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for templates with complex expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_comprehensive() {
    let source = r#"// Comprehensive template literal patterns for ES5 transform testing

// Basic literals
const greeting = `Hello, World!`;
const empty = ``;

// Expression interpolation
const name = "Eve";
const age = 35;
const intro = `My name is ${name} and I am ${age} years old`;

// Multiple expressions
const a = 10;
const b = 20;
const math = `${a} + ${b} = ${a + b}, ${a} * ${b} = ${a * b}`;

// Nested templates
const nested = `Outer: ${`Inner: ${name.toUpperCase()}`}`;
const deepNested = `L1: ${`L2: ${`L3: ${a + b + 30}`}`}`;

// Tagged templates
function css(strings: TemplateStringsArray, ...values: any[]) {
    return strings.reduce((result, str, i) => result + str + (values[i] || ""), "");
}

function html(strings: TemplateStringsArray, ...values: any[]) {
    return strings.map((s, i) => s + (values[i] !== undefined ? values[i] : "")).join("");
}

const styles = css`
    .container {
        width: ${100}%;
        padding: ${10}px;
    }
`;

const markup = html`<div class="${"wrapper"}"><span>${name}</span></div>`;

// Multiline
const multiline = `
    This is a
    multiline
    template
`;

// Function calls in templates
function getTitle(): string { return "Dr."; }
function formatName(n: string): string { return n.toUpperCase(); }

const formal = `${getTitle()} ${formatName(name)}`;

// Method chains
const str = "hello world";
const processed = `Original: ${str}, Upper: ${str.toUpperCase()}, Length: ${str.length}`;

// Arrays and objects
const items = [1, 2, 3];
const obj = { x: 5, y: 10 };

const arrTemplate = `Items: ${items.join(", ")}, Sum: ${items.reduce((s, i) => s + i, 0)}`;
const objTemplate = `Point: (${obj.x}, ${obj.y})`;

// Conditionals
const status = true;
const conditional = `Status: ${status ? "active" : "inactive"}`;

// Class with template methods
class Formatter {
    prefix = ">>>";

    format(value: string) {
        return `${this.prefix} ${value}`;
    }

    wrap(value: string) {
        return `[${value}]`;
    }
}

const formatter = new Formatter();

// IIFE in template
const iife = `Result: ${((x: number) => x * x)(7)}`;

// Usage
console.log(greeting, intro, math);
console.log(nested, deepNested);
console.log(styles, markup);
console.log(multiline);
console.log(formal, processed);
console.log(arrTemplate, objTemplate);
console.log(conditional);
console.log(formatter.format("test"));
console.log(iife);"#;

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
        output.contains("greeting"),
        "expected greeting in output. output: {output}"
    );
    assert!(
        output.contains("intro"),
        "expected intro in output. output: {output}"
    );
    assert!(
        output.contains("css"),
        "expected css tagged template in output. output: {output}"
    );
    assert!(
        output.contains("html"),
        "expected html tagged template in output. output: {output}"
    );
    assert!(
        output.contains("Formatter"),
        "expected Formatter class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive template literals"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Class Static Block Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_static_block_es5_basic() {
    let source = r#"class Counter {
    static count = 0;

    static {
        Counter.count = 10;
        console.log("Counter initialized");
    }

    increment() {
        Counter.count++;
    }
}

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
        "expected Counter in output. output: {output}"
    );
    assert!(
        output.contains("count"),
        "expected count in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic static block"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_multiple() {
    let source = r#"class Config {
    static host: string;
    static port: number;
    static protocol: string;

    static {
        Config.host = "localhost";
    }

    static {
        Config.port = 8080;
    }

    static {
        Config.protocol = "https";
    }

    static getUrl() {
        return Config.protocol + "://" + Config.host + ":" + Config.port;
    }
}

console.log(Config.getUrl());"#;

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
        "expected Config in output. output: {output}"
    );
    assert!(
        output.contains("host"),
        "expected host in output. output: {output}"
    );
    assert!(
        output.contains("port"),
        "expected port in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

